//! Executor `dst` cases run as fixed-step sequences on the real engine (crates/hft-exec/TESTS.md
//! EXE-501, EXE-502, EXE-504), like `scenarios.rs` (B0 rig; g7/g8 of the cases ≙ s2's groups on
//! account 2). Randomised fills come from a seeded LCG and every run repeats over a seed range.

use hft_event::{ControlCmd, OrderAction, OrderDecision, OrderEventKind};
use hft_strategy_sdk::{ExecSpec, LinkRole, SimulOnFail, SimulParams, TargetState};
use hft_testkit::dst::Scenario;
use hft_types::{GroupId, Qty, RejectBits, RunMode, Side};

use crate::common::*;
use crate::scenarios::*;
use crate::scenarios_hedge::{case, exec_decs};

/// Deterministic pseudo-random stream (64-bit LCG).
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
    fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next() % n }
    }
}

/// Accepted new orders of account 2 since `m`.
fn new_orders(h: &H, m: usize) -> Vec<OrderDecision> {
    decs(h, m)
        .into_iter()
        .filter(|d| d.account == AC2 && d.action == OrderAction::New && d.reject == 0)
        .collect()
}

/// b2 lot (0.01 ETH).
const LOT_B2: i64 = 1_000_000;

/// s2's cell state.
fn state(h: &H, g: GroupId) -> Option<TargetState> {
    h.s.engine().exec.cell(AC2, g).map(|c| c.view.state)
}

// ------------------------------------------------------------------------------------- EXE-501

/// The target of a one-leg group (b2, a perp) +0.5 → −0.5 under random IOC outcomes (full, partial, none, Unknown then a
/// late fill by query): no order ever moves the exchange position across zero by itself, and the
/// target is reached.
#[test]
fn exe_501_flip_no_cross() {
    case(Scenario::new(
        "EXE-501",
        "a long-to-short flip never crosses zero in one order",
    ));
    for seed in 1..=12u64 {
        let mut rng = Lcg(seed);
        reset_script();
        let mut b = b0();
        b.params[1].groups.push(group(G2B, ETH, &[leg(B2, B)]));
        // A one-leg group is all "naked": keep the exposure gate (#9) out of this case.
        let rd = b.rd.load();
        let mut rp = b0_risk(&rd);
        let mut accts = rp.accounts.as_slice().to_vec();
        for a in accts.iter_mut().filter(|a| a.account == AC2) {
            a.default_group.naked_gate_min = n("100000000");
        }
        rp.accounts = hft_risk::PreVec::from_vec(accts);
        b.risk = Some(rp);
        let mut h = b.boot();
        h.steady();
        let slice = {
            let mut s = slice0();
            s.max_chunk_notional = n("800");
            s.cooldown_ms = 0;
            s
        };
        let mut pos = 0i64; // exchange truth of b1 for account 2
        let mut answered: Vec<(u32, u32)> = Vec::new();
        for (goal, label) in [(q("0.5"), "long"), (q("-0.5"), "short")] {
            let m0 = h.mark();
            h.set_target_s2(target(AC2, G2B, &[(0, goal)], ExecSpec::TakerSlice(slice)));
            let mut rounds = 0;
            while pos != goal.0 && rounds < 400 {
                rounds += 1;
                // Fresh margin every round (the venue pushes it; stale margin is MarginStress).
                h.margin_report(AC2, "100000", "0", "100000");
                h.margin_report(AC1, "100000", "0", "100000");
                h.run();
                h.mark_md(B2, "3000.3");
                let open: Vec<_> = new_orders(&h, m0)
                    .into_iter()
                    .filter(|d| d.inst == B2 && !answered.contains(&(d.slot.0, d.r#gen)))
                    .collect();
                if open.is_empty() {
                    let t = h.now() + 100 * MS;
                    keep_alive_to(&mut h, t);
                    h.quote(B2, "3000.1", "3000.6");
                    h.run();
                    continue;
                }
                for d in open {
                    answered.push((d.slot.0, d.r#gen));
                    // No order may cross zero: a sell from a long sells at most the long.
                    let signed = if d.side == Side::Buy { d.qty.0 } else { -d.qty.0 };
                    assert!(
                        !(pos > 0 && pos + signed < 0) && !(pos < 0 && pos + signed > 0),
                        "seed {seed} {label}: order {d:?} could cross zero from {pos}"
                    );
                    let lots = d.qty.0 / LOT_B2;
                    let filled = match rng.below(4) {
                        0 => lots,
                        1 => rng.below(lots as u64 + 1) as i64,
                        2 => 0,
                        _ => -1, // Unknown, then a late fill found by query
                    };
                    h.ack(&d);
                    if filled < 0 {
                        // The order connection drops; the order fills; the query finds it.
                        let f = (1 + rng.below(lots as u64)) as i64 * LOT_B2;
                        let t = h.now();
                        h.push(
                            hft_sequencer::driver::live::RingId::Rep(0),
                            &conn(
                                B,
                                0,
                                hft_event::ConnStateKind::Down,
                                hft_event::ConnRole::Order,
                                hft_types::InstId(0),
                                AC2,
                                t,
                            ),
                        );
                        h.push(
                            hft_sequencer::driver::live::RingId::Rep(0),
                            &conn(
                                B,
                                1,
                                hft_event::ConnStateKind::Down,
                                hft_event::ConnRole::Order,
                                hft_types::InstId(0),
                                AC2,
                                t,
                            ),
                        );
                        h.run();
                        h.adv(300 * MS);
                        let t = h.now();
                        h.push(
                            hft_sequencer::driver::live::RingId::Rep(0),
                            &conn(
                                B,
                                0,
                                hft_event::ConnStateKind::Up,
                                hft_event::ConnRole::Order,
                                hft_types::InstId(0),
                                AC2,
                                t,
                            ),
                        );
                        h.run();
                        let mut ev = order_event(&d, OrderEventKind::QueryResult, h.epoch());
                        ev.detail = if f >= d.qty.0 { 3 } else { 4 };
                        ev.cum_filled = Qty(f);
                        ev.avg_px = d.px;
                        h.push(hft_sequencer::driver::live::RingId::Rep(0), &ev);
                        h.run();
                        pos += if d.side == Side::Buy { f } else { -f };
                    } else if filled == 0 {
                        expire(&mut h, &d);
                    } else {
                        let f = Qty(filled * LOT_B2);
                        h.fill(&d, f, f, d.px);
                        if f.0 < d.qty.0 {
                            let mut ev = order_event(&d, OrderEventKind::Expired, h.epoch());
                            ev.cum_filled = f;
                            h.push(hft_sequencer::driver::live::RingId::Rep(0), &ev);
                            h.run();
                        }
                        pos += if d.side == Side::Buy { f.0 } else { -f.0 };
                    }
                }
            }
            let xs: Vec<_> = exec_decs(&h, m0)
                .iter()
                .rev()
                .take(6)
                .map(|x| (x.action, x.state_from, x.state_to, x.detail, x.gate))
                .collect();
            assert_eq!(
                pos,
                goal.0,
                "seed {seed}: {label} target reached; state {:?}; last exec {xs:?}; rejects {:?}; caps {:?}",
                state(&h, G2B),
                decs(&h, m0)
                    .iter()
                    .filter(|d| d.reject != 0)
                    .map(|d| (d.inst, d.side, d.qty, d.reject))
                    .collect::<Vec<_>>(),
                h.s.engine()
                    .risk
                    .mode
                    .caps()
                    .iter()
                    .map(|c| (c.source, c.scope.account, c.scope.group))
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                h.s.engine().ledger.pos(AC2, B2).qty.0,
                pos,
                "seed {seed}: books = exchange"
            );
        }
    }
}

// ------------------------------------------------------------------------------------- EXE-502

fn simul(on_fail: SimulOnFail) -> ExecSpec {
    ExecSpec::Simultaneous(SimulParams {
        slice: slice0(),
        on_fail,
        _pad0: [0; 7],
        fix_tolerance: r("0.02"),
        residual: hft_strategy_sdk::ResidualPolicy::Hold,
        _pad1: [0; 7],
    })
}

/// s2's Simultaneous open of g2 (a1 +0.01, b1 −0.01): both legs out; the first fills, the venue
/// rejects the second. Returns the first and the second leg.
fn simul_open(h: &mut H, on_fail: SimulOnFail) -> (OrderDecision, OrderDecision) {
    let m = h.mark();
    h.set_target_s2(target(
        AC2,
        G2,
        &[(0, q("0.01")), (1, q("-0.01"))],
        simul(on_fail),
    ));
    let os = new_orders(h, m);
    let first = os.iter().find(|d| d.inst == A1).copied().expect("first leg");
    let second = os
        .iter()
        .find(|d| d.inst == B1)
        .copied()
        .expect("simultaneous second leg");
    h.ack(&first);
    h.fill(&first, first.qty, first.qty, first.px);
    (first, second)
}

fn reverses(h: &H, m: usize) -> Vec<OrderDecision> {
    decs(h, m)
        .into_iter()
        .filter(|d| d.account == AC2 && d.link_role == LinkRole::Reverse as u8)
        .collect()
}

#[test]
fn exe_502_simultaneous_on_fail() {
    super::simultaneous::check_simultaneous_fault_seeds_and_snapshot_replay();
    case(Scenario::new(
        "EXE-502",
        "Simultaneous: second-leg failure by on_fail; reverse failure Broken",
    ));
    // A: Reverse — the filled, unhedged first leg is reversed with an IOC; no Broken.
    let mut h = steady();
    let m0 = h.mark();
    let (_, second) = simul_open(&mut h, SimulOnFail::Reverse);
    let m = h.mark();
    h.reject(&second);
    let rv = reverses(&h, m);
    assert_eq!(rv.len(), 1, "one reverse: {rv:?}");
    assert_eq!(
        (rv[0].inst, rv[0].side, rv[0].qty, rv[0].tif),
        (A1, Side::Sell, q("0.01"), hft_types::Tif::Ioc)
    );
    h.ack(&rv[0]);
    h.fill(&rv[0], rv[0].qty, rv[0].qty, rv[0].px);
    let t = h.now();
    keep_alive_to(&mut h, t + 2 * SEC);
    assert!(
        !matches!(state(&h, G2), Some(TargetState::Broken(_))),
        "A: no Broken"
    );
    assert!(
        cap_sets(&h, m0)
            .iter()
            .all(|c| c.source != hft_types::CapSource::BROKEN)
    );

    // B: Hold — no reverse; LegTracker keeps the naked a1; the repair then follows the threshold.
    let mut h = steady();
    let (_, second) = simul_open(&mut h, SimulOnFail::Hold);
    let m = h.mark();
    h.reject(&second);
    assert!(reverses(&h, m).is_empty(), "B: no reverse");
    let lv = h.s.engine().oms.legs(AC2, G2).view;
    assert_eq!(
        (lv.leg[0].net, lv.leg[1].net),
        (q("0.01"), Qty(0)),
        "B: naked recorded"
    );
    let t = h.now();
    keep_alive_to(&mut h, t + 3 * SEC);
    // The naked first leg is then handled by the threshold rules: the target's second leg is
    // held by the exposure gate (#9) while the group is naked, and the HedgeFix (risk-reducing,
    // not gated) trims the excess a1.
    let fix = decs(&h, m)
        .into_iter()
        .find(|d| d.account == AC2 && d.reject == 0 && d.link_role == LinkRole::HedgeFix as u8);
    assert!(
        fix.is_some_and(|d| d.inst == A1 && d.side == Side::Sell && d.qty == q("0.01")),
        "B: HedgeFix of the naked a1; state {:?}; decs {:?}",
        state(&h, G2),
        decs(&h, m)
            .iter()
            .map(|d| (d.inst, d.side, d.qty, d.link_role, d.reject))
            .collect::<Vec<_>>()
    );

    // C: the reverse is refused too until its retries run out: Broken{ReverseFailed}, P0 ExecBroken.
    let mut h = steady();
    let m0 = h.mark();
    let (_, second) = simul_open(&mut h, SimulOnFail::Reverse);
    h.reject(&second);
    let mut handled = Vec::new();
    for _ in 0..12 {
        for d in reverses(&h, m0) {
            if d.reject == 0 && !handled.contains(&(d.slot.0, d.r#gen)) {
                handled.push((d.slot.0, d.r#gen));
                h.reject(&d);
            }
        }
        if matches!(state(&h, G2), Some(TargetState::Broken(_))) {
            break;
        }
        let t = h.now();
        keep_alive_to(&mut h, t + 300 * MS);
    }
    assert_eq!(
        state(&h, G2),
        Some(TargetState::Broken(hft_strategy_sdk::BrokenReason::ReverseFailed)),
        "C: Broken{{ReverseFailed}}"
    );
    assert!(
        h.notes_since(m0)
            .iter()
            .any(|n| n.source == hft_event::NoteSource::Exec
                && n.code & 0xFFFF == hft_exec::decision::note::EXEC_BROKEN
                && n.level == hft_event::NoteLevel::P0),
        "C: P0 ExecBroken"
    );
}

// ------------------------------------------------------------------------------------- EXE-504

/// s2's second group (EXE-501: b2 alone; EXE-504: a2/b2).
const G2B: GroupId = GroupId(0x0200_0002);

#[test]
fn exe_504_close_only_midway() {
    case(Scenario::new(
        "EXE-504",
        "CloseOnly midway: reduce continues, increase Stuck, resumes on Live",
    ));
    reset_script();
    let mut b = b0();
    b.params[1].groups.push(gspec_eth(G2B));
    let mut h = b.boot();
    h.steady();
    // g2b's position: a2 +0.9 / b2 −0.9 (a completed pair target).
    let m = h.mark();
    h.set_target_s2(target(
        AC2,
        G2B,
        &[(0, q("0.9")), (1, q("-0.9"))],
        ExecSpec::PairOnFill(pair0()),
    ));
    for _ in 0..40 {
        let open: Vec<_> = new_orders(&h, m)
            .into_iter()
            .filter(|d| slot_state(&h, d).is_some_and(|s| s.open()))
            .collect();
        if open.is_empty() && state(&h, G2B) == Some(TargetState::Idle) {
            break;
        }
        for d in open {
            h.ack(&d);
            h.fill(&d, d.qty, d.qty, d.px);
        }
        let t = h.now();
        keep_alive_to(&mut h, t + 400 * MS);
        h.quote(A2, "3000", "3000.5");
        h.run();
    }
    assert_eq!(state(&h, G2B), Some(TargetState::Idle), "g2b opened");
    // g2 opening target and g2b reducing target in progress: slices only partly filled.
    let small = {
        let mut p = pair0();
        p.slice.max_chunk_notional = n("100");
        p.slice.min_chunk_notional = n("10");
        p.slice.cooldown_ms = 400;
        p
    };
    let m1 = h.mark();
    h.set_target_s2(target(
        AC2,
        G2,
        &[(0, q("0.02")), (1, q("-0.02"))],
        ExecSpec::PairOnFill(small),
    ));
    h.set_target_s2(target(
        AC2,
        G2B,
        &[(0, Qty(0)), (1, Qty(0))],
        ExecSpec::PairOnFill(small),
    ));
    let pump = |h: &mut H, until: u64, m: usize| {
        while h.now() < until {
            let open: Vec<_> = new_orders(h, m)
                .into_iter()
                .filter(|d| slot_state(h, d).is_some_and(|s| s.open()))
                .collect();
            for d in open {
                h.ack(&d);
                h.fill(&d, d.qty, d.qty, d.px);
            }
            let t = h.now() + 100 * MS;
            keep_alive_to(h, t);
            h.quote(A1, "50000", "50001");
            h.quote(A2, "3000", "3000.5");
            h.margin_report(AC2, "100000", "0", "100000");
            h.run();
            h.mark_md(B1, "50010.5");
            h.mark_md(B2, "3000.3");
        }
    };
    let t0 = h.now();
    pump(&mut h, t0 + SEC, m1);
    // t 1 s: account 2 CloseOnly.
    let id = h.cmd_confirmed(
        AC2,
        &ControlCmd::SetMode {
            scope: scope(S2, AC2, GroupId(0)),
            mode: RunMode::CloseOnly,
        },
    );
    h.run();
    assert_eq!(h.res(id).0, 0, "{:?}", h.res(id));
    let m2 = h.mark();
    pump(&mut h, t0 + 20 * SEC, m1);
    let during = decs(&h, m2);
    // No increasing order of g2 goes out (second legs of fills just before, risk-reducing, may).
    let increasing: Vec<_> = during
        .iter()
        .filter(|d| {
            d.account == AC2
                && d.group == G2
                && d.action == OrderAction::New
                && d.reject == 0
                && d.oflags & hft_event::OrderFlags::RISK_REDUCING.0 == 0
        })
        .collect();
    assert!(
        increasing.is_empty(),
        "CloseOnly: g2 sends no increasing order: {increasing:?}"
    );
    let gated: Vec<u64> = h
        .since(m2)
        .iter()
        .filter_map(|r| {
            r.read::<OrderDecision>()
                .filter(|d| d.account == AC2 && d.group == G2 && d.reject == RejectBits::CLOSE_ONLY.0)
                .map(|_| r.tsc)
        })
        .collect();
    for w in gated.windows(2) {
        assert!(w[1] - w[0] >= 500 * MS, "one gated intent per gated_backoff_ms");
    }
    assert!(
        matches!(state(&h, G2), Some(TargetState::Stuck(hft_strategy_sdk::StuckReason::Gated(b))) if b.0 & RejectBits::CLOSE_ONLY.0 != 0),
        "g2 Stuck(Gated(CLOSE_ONLY)): {:?}",
        state(&h, G2)
    );
    assert!(
        during
            .iter()
            .any(|d| d.account == AC2 && d.group == G2B && d.reject == 0),
        "g2b keeps reducing"
    );
    // t 20 s: Live again; g2 continues.
    let id = h.cmd_confirmed(
        AC2,
        &ControlCmd::SetMode {
            scope: scope(S2, AC2, GroupId(0)),
            mode: RunMode::Live,
        },
    );
    h.run();
    assert_eq!(h.res(id).0, 0, "{:?}", h.res(id));
    let m3 = h.mark();
    pump(&mut h, t0 + 23 * SEC, m1);
    assert!(
        decs(&h, m3)
            .iter()
            .any(|d| d.account == AC2 && d.group == G2 && d.reject == 0),
        "g2 resumes after Live"
    );
    let _ = exec_decs(&h, m3);
}
