//! Snapshot round trip, capacity, and replay from any snapshot (RSK-230, RSK-231, RSK-272).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)] // allow-list: test code, not a hot path

mod common;
use common::*;

use std::collections::BTreeMap;

use hft_risk::mode::{GateEvent, ModeInput, RecoveryFlags, StorageKind};
use hft_risk::snapshot::{
    SECTION_GATES, SECTION_MARGIN, SECTION_MODE_TABLE, SECTION_RISK_COUNTERS, SECTION_RISK_STATE, row_kind,
};
use hft_risk::{
    Capacities, EquitySource, EquityView, IntentView, LegSummary, MarginSample, ReconcileGroup, Risk,
    RiskTimer,
};
use hft_strategy_sdk::SnapshotReader;
use hft_types::{AccountId, CapSource, GroupId, Notional, Rate, Side, Tif, VenueId};
use proptest::prelude::*;

thread_local! {
    static BASE: W = r0();
}
fn base() -> W {
    BASE.with(|b| b.clone())
}

fn sections(bytes: &[u8]) -> Vec<(u16, u32, usize)> {
    let mut r = SnapshotReader::new(bytes);
    let mut out = Vec::new();
    while let Some(s) = r.next_section().unwrap() {
        out.push((s.section, s.count, s.body.len()));
    }
    out
}

/// Observable state compared across a restore.
fn observe(w: &W) -> String {
    let now = w.nowv();
    let mut s = String::new();
    s += &format!("kill {:?}\n", w.risk.mode.kill_bits());
    for (c, _, e) in w.risk.mode.cells() {
        s += &format!("eff {c:?} {e:?}\n");
    }
    for a in 1..=3u8 {
        s += &format!("rel {a} {}\n", w.risk.mode.released(acc(a)));
        s += &format!("view {a} {:?}\n", w.risk.view(acc(a), &now));
        for v in [A, B, C] {
            s += &format!(
                "gates {a} {v:?} {} {} {} {}\n",
                w.risk.mode.hedge_gate_ok(acc(a), v),
                w.risk.mode.venue_gate_ok(acc(a), v),
                w.risk.auto_external_ok(acc(a), v, 0),
                w.risk.lev_bad(acc(a), v)
            );
        }
    }
    s
}

#[test]
fn rsk_230_snapshot_roundtrip() {
    let mut w = r0();
    w.ok(kill_flatten(1));
    w.ok(daily(1));
    w.ok(broken(1, G1));
    let h = hedge(1, G1, A1, Side::Buy, "0.0001", "50001.00", "0.0001");
    w.commits(&h, 30);
    w.hit(A, 1);
    // (1, B): user stream up at seq 40, then a group with view_seq 150 holding a Warn item
    // (the difference streak) that stays undisposed.
    let s = w.seq.max(40);
    w.apply_seq(
        ModeInput::Gate(GateEvent::OrderConns {
            account: acc(1),
            venue: B,
            primary_up: false,
            standby_up: true,
        }),
        s - 1,
    )
    .unwrap();
    w.apply_seq(
        ModeInput::Gate(GateEvent::OrderConns {
            account: acc(1),
            venue: B,
            primary_up: true,
            standby_up: true,
        }),
        40,
    )
    .unwrap();
    w.seq = s + 1;
    let g = vec![
        summary(1, B, 77, 150, 0, 0),
        pos_item(1, B, 77, B1, "0.003", "150"),
    ];
    w.step(1, &Step::Recon(B, g)).unwrap();
    w.margin(1, B, &msample("0.15", "0.10", T0), T0);
    w.ok(auto(CapSource::RECOVERY, sc_acct(2)));
    w.ok(ModeInput::RecoveryFlags {
        account: acc(2),
        flags: RecoveryFlags(0),
    });
    let before = observe(&w);
    let b1 = w.snapshot();
    let secs = sections(&b1);
    let order: Vec<u16> = secs.iter().map(|x| x.0).collect();
    assert_eq!(
        order,
        vec![
            SECTION_RISK_STATE,
            SECTION_MODE_TABLE,
            SECTION_RISK_COUNTERS,
            SECTION_GATES,
            SECTION_MARGIN
        ]
    );
    let (_, n4, len4) = secs[0];
    assert_eq!(n4 as usize, w.risk.mode.caps().len());
    assert!(
        len4 >= 48 * n4 as usize && len4 < 48 * n4 as usize + 8,
        "section 4 rows are 48 B"
    );
    let kinds: std::collections::BTreeSet<u8> = section17(&b1).iter().map(|r| r.h.kind).collect();
    for k in [
        row_kind::ACCT,
        row_kind::BUCKET,
        row_kind::RECON,
        row_kind::RECON_GATE,
        row_kind::LEV,
        row_kind::USER_STREAM,
    ] {
        assert!(kinds.contains(&k), "kind {k} missing");
    }
    let us = section17(&b1)
        .into_iter()
        .find(|r| r.h.kind == row_kind::USER_STREAM && r.h.account == 1 && r.h.venue == B.0)
        .unwrap();
    let us = <hft_risk::snapshot::RiskUsRow as zerocopy::FromBytes>::read_from_bytes(&us.body).unwrap();
    assert_eq!((us.us_up_seq, us.prev_view_seq, us.uflags), (40, 150, 0));
    w.restore_from(&b1);
    let b2 = w.snapshot();
    assert_eq!(b1, b2, "second snapshot differs");
    assert_eq!(observe(&w), before);
}

#[test]
fn rsk_231_restore_capacity() {
    let mut w = r0();
    w.ok(kill_acct(1, false));
    w.ok(daily(1));
    w.ok(halted(1, 1, G1));
    assert_eq!(w.risk.mode.caps().len(), 3);
    let snap = w.snapshot();
    let caps = Capacities { max_caps: 2, ..CAPS };
    let mut r = SnapshotReader::new(&snap);
    let res = Risk::restore(&mut r, &w.p, w.rd, &gws_for(w.rd), &caps, &exec_bounds(3), MC);
    assert!(res.is_err());
}

// ------------------------------------------------------------------------------------ RSK-272

#[derive(Clone, Debug)]
enum Op {
    Eval(u8),
    Hit(u8, u8),
    Apply(u8),
    Margin(u8, u8, u8, u16),
    Timer(u8, u8),
    Pnl(u8, i32),
    DayRoll,
    Legs(u8, u8, bool, u16),
    Cfg(u8),
    Recon(u8, bool),
    AutoExt(u8),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => (0u8..6).prop_map(Op::Eval),
        1 => (0u8..3, 1u8..=2).prop_map(|(v, a)| Op::Hit(v, a)),
        3 => (0u8..20).prop_map(Op::Apply),
        2 => (1u8..=2, 0u8..2, 0u8..6, 0u16..20_000).prop_map(|(a, v, l, age)| Op::Margin(a, v, l, age)),
        1 => (0u8..3, 0u8..2).prop_map(|(k, v)| Op::Timer(k, v)),
        2 => (1u8..=2, -8_000i32..2_000).prop_map(|(a, p)| Op::Pnl(a, p)),
        1 => Just(Op::DayRoll),
        2 => (1u8..=2, 0u8..2, any::<bool>(), 0u16..2_000).prop_map(|(a, g, qi, f)| Op::Legs(a, g, qi, f)),
        1 => (0u8..4).prop_map(Op::Cfg),
        2 => (0u8..3, any::<bool>()).prop_map(|(v, warn)| Op::Recon(v, warn)),
        1 => (0u8..2).prop_map(Op::AutoExt),
    ]
}

const VS: [VenueId; 3] = [A, B, C];

fn intents() -> Vec<IntentView> {
    vec![
        open(1, G1, A1, Side::Buy, "0.0001", "50000.00", Tif::Gtc),
        open(2, G2, B1, Side::Sell, "0.001", "49995.0", Tif::Ioc),
        hedge(1, G1, B1, Side::Sell, "0.001", "49995.0", "0.001"),
        link_open(
            1,
            G1,
            A1,
            Side::Buy,
            "0.001",
            "50000.00",
            (1, B, B1, Side::Sell, "0.001", "50"),
        ),
        hedge_fix(1, G3, B2, Side::Buy, "0.01", "2501.0", "0.01"),
        open(1, G3, A2, Side::Buy, "0.001", "2500.00", Tif::Gtc),
    ]
}

fn mode_inputs() -> Vec<ModeInput<'static>> {
    vec![
        kill_acct(1, false),
        resume(CapSource::KILL, sc_acct(1)),
        kill_global(false),
        resume(CapSource::KILL, sc_any()),
        daily(2),
        resume(CapSource::DAILY_LOSS, sc_acct(2)),
        set_mode(1, 1, G1, RunMode::CloseOnly),
        set_mode(1, 1, G1, RunMode::Live),
        broken(2, G2),
        ModeInput::AutoClear {
            source: CapSource::BROKEN,
            scope: sc_ag(2, G2),
        },
        ModeInput::Clock { ok: false },
        ModeInput::Clock { ok: true },
        ModeInput::Storage {
            kind: StorageKind::PersistLag,
            level: 1,
        },
        ModeInput::Storage {
            kind: StorageKind::Both,
            level: 0,
        },
        auto(CapSource::RECON, sc_av(1, B)),
        resume(CapSource::RECON, sc_av(1, B)),
        halted(1, 1, G3),
        resume(CapSource::HALTED, sc_cell(1, 1, G3)),
        kill_flatten(2),
        resume(CapSource::KILL, sc_acct(2)),
    ]
}
use hft_types::RunMode;

struct Eqs;
impl EquitySource for Eqs {
    fn equity(&self, a: AccountId) -> EquityView {
        EquityView {
            total: Notional(200_000_000_000 + i64::from(a.0)),
            valid: true,
        }
    }
}

/// State carried by the test driver (not part of the Risk).
#[derive(Clone, Default)]
struct Drv {
    now: i64,
    day: u32,
    id: u64,
    legs: BTreeMap<(u8, u32), LegSummary>,
    pnl: BTreeMap<u8, i64>,
}

/// `Risk::apply` the way the sequencer drives it (unlatches resumed daily losses and resets their
/// trigger with the current day P&L).
fn rapply(w: &mut W, d: &Drv, inp: ModeInput<'_>) -> Result<(), hft_risk::mode::ModeErr> {
    w.mm.sync(&w.risk);
    let s = w.seq;
    w.seq += 1;
    let r = w.risk.apply(inp, s, &w.mm, &mut w.out);
    let resumed: Vec<AccountId> = w.out.daily_loss_resumed.iter().copied().collect();
    for a in resumed {
        let p = d.pnl.get(&a.0).copied().unwrap_or(0);
        w.risk.on_daily_loss_resumed(a, Notional(p));
    }
    r
}

/// Applies the mode inputs a `MarginOut` carries; returns their outcomes.
fn apply_mo(w: &mut W, d: &Drv) -> String {
    let inputs: Vec<ModeInput<'static>> = w.mo.mode.iter().copied().collect();
    let mut s = format!("{:?}", w.mo);
    for i in inputs {
        let r = rapply(w, d, i);
        s += &format!(" {r:?} {:?}", w.out);
    }
    s
}

fn exec(w: &mut W, d: &mut Drv, o: &Op, its: &[IntentView], mins: &[ModeInput<'static>]) -> String {
    d.now += 37 * MS;
    w.now = d.now;
    w.mv.tsc = d.now as u64;
    let now = now_at(d.now);
    match *o {
        Op::Eval(k) => {
            let r = w.risk.evaluate(&its[usize::from(k)], &w.mv, &now);
            if let Ok(a) = &r {
                w.risk.commit(a, &now);
            }
            format!("{r:?}")
        }
        Op::Hit(v, a) => format!("{:?}", w.risk.on_rate_limit_hit(VS[usize::from(v)], acc(a), &now)),
        Op::Apply(k) => {
            let r = rapply(w, d, mins[usize::from(k)]);
            format!("{r:?} {:?}", w.out)
        }
        Op::Margin(a, v, l, age) => {
            let liq = [
                50_000_000i64,
                120_000_000,
                140_000_000,
                200_000_000,
                300_000_000,
                i64::MAX,
            ][usize::from(l)];
            let s = MarginSample {
                rep_liq_dist: Rate(liq),
                est_liq_dist: Rate(liq),
                reported_ratio: Rate(100_000_000),
                est_ratio: Rate(100_000_000),
                report_ens: d.now - i64::from(age) * MS,
                mark_ens: d.now - i64::from(age) * MS,
                has_report: true,
                truncated: false,
            };
            w.risk
                .on_margin(acc(a), [B, C][usize::from(v)], &s, &now, &mut w.mo);
            apply_mo(w, d)
        }
        Op::Timer(k, v) => {
            let (a, vv) = (acc(1), [B, C][usize::from(v)]);
            let t = match k {
                0 => RiskTimer::MarginStale { a, v: vv },
                1 => RiskTimer::MarginDeadline { a, v: vv },
                _ => RiskTimer::MarginForce { a, v: vv },
            };
            w.risk.on_timer(t, &now, &mut w.mo);
            apply_mo(w, d)
        }
        Op::Pnl(a, p) => {
            let e = Eqs.equity(acc(a));
            let pn = i64::from(p) * 1_000_000;
            d.pnl.insert(a, pn);
            let r = w.risk.on_pnl(acc(a), Notional(pn), e);
            let mut s = format!("{r:?}");
            if let Some(i) = r {
                let x = rapply(w, d, i);
                s += &format!(" {x:?} {:?}", w.out);
            }
            s
        }
        Op::DayRoll => {
            d.day += 1;
            w.risk.on_day_roll(d.day, &Eqs);
            String::from("roll")
        }
        Op::Legs(a, g, q, f) => {
            let grp = if a == 1 { [G1, G3][usize::from(g)] } else { G2 };
            let s = LegSummary {
                naked_ratio: Rate(i64::from(f) * 500_000),
                naked_notional: Notional(i64::from(f) * 1_000_000),
                naked_filled_notional: Notional(i64::from(f) * 10_000),
                quiescent: q,
            };
            d.legs.insert((a, grp.0), s);
            let r = w.risk.on_legs(acc(a), grp, &s);
            let mut out = format!("{r:?}");
            if let Some(i) = r {
                let x = rapply(w, d, i);
                out += &format!(" {x:?} {:?}", w.out);
            }
            out
        }
        Op::Cfg(k) => {
            let x = ["3.00", "3.01", "0", "2.50"][usize::from(k)];
            let h = hdr(1, B, 0, KEYS);
            w.risk.on_account_cfg(&h, &[lev(B1, x), lev(B2, "3.00")]);
            let r = rapply(
                w,
                d,
                ModeInput::Gate(GateEvent::AccountCfg {
                    hdr: &h,
                    keys_required: KEYS,
                }),
            );
            format!("{r:?} {:?}", w.out)
        }
        Op::Recon(v, warn) => {
            d.id += 1;
            let vv = VS[usize::from(v)];
            let mut g = vec![summary(1, vv, d.id, d.id, 0, 0)];
            if warn {
                g.push(pos_item(
                    1,
                    vv,
                    d.id,
                    if vv == B { B1 } else { C1 },
                    "0.003",
                    "150",
                ));
            }
            let r = w.risk.on_reconcile(&ReconcileGroup { items: &g });
            let mut s = format!("{r:?}");
            if let Some(gi) = r.gate {
                let x = rapply(w, d, ModeInput::Gate(GateEvent::Recon(&gi)));
                s += &format!(" {x:?} {:?}", w.out);
            }
            if let Some(inp) = r.recon {
                let x = rapply(w, d, inp);
                s += &format!(" {x:?} {:?}", w.out);
            }
            s
        }
        Op::AutoExt(v) => format!("{}", w.risk.auto_external_ok(acc(1), [B, C][usize::from(v)], 0)),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]
    #[test]
    fn rsk_272_replay_from_any_snapshot(ops in prop::collection::vec(op(), 400), k in 1usize..400) {
        let its = intents();
        let mins = mode_inputs();
        let start = Drv { now: T0, day: 100, id: 900_000, legs: BTreeMap::new(), pnl: BTreeMap::new() };
        // Continuous run.
        let mut a = base();
        let mut da = start.clone();
        let oa: Vec<String> = ops.iter().map(|o| exec(&mut a, &mut da, o, &its, &mins)).collect();
        // Interrupted at k.
        let mut b = base();
        let mut db = start;
        for o in &ops[..k] {
            exec(&mut b, &mut db, o, &its, &mins);
        }
        let snap = b.snapshot();
        b.restore_from(&snap);
        b.risk.set_run_start(0);
        for ((acct, g), s) in &db.legs {
            b.risk.on_legs(acc(*acct), GroupId(*g), s);
        }
        let ob: Vec<String> = ops[k..].iter().map(|o| exec(&mut b, &mut db, o, &its, &mins)).collect();
        for (i, (x, y)) in oa[k..].iter().zip(ob.iter()).enumerate() {
            prop_assert_eq!(x, y, "first difference at step {} (k = {}): {:?}", k + i, k, ops[k + i]);
        }
        prop_assert_eq!(a.snapshot(), b.snapshot());
    }
}
