//! Hedge-wiring scenarios on the real engine (docs/tests/scenarios.md: B-4, B-5, C1, C4, QF-1,
//! LT-1, FP-5, FP-6, FP-7), run as deterministic step sequences on the B0 rig like
//! `scenarios.rs` (same W1 mapping: `VS` ≙ A, `VP` ≙ B, `S1` ≙ a1, `P1` ≙ b1, account `a` ≙ 1).
//! Prices and thresholds are B0's (a1 50 000 / 50 001, b1 50 010 / 50 011, `rr_max_cross` 0.01,
//! `conn_liveness_ms` 2 000, `hedge_retry_min_ms` 100, `hedge_retry_max` 3, `hedge_deadline_ms`
//! 3 000, `fix_cooldown_ms` 1 000, `fix_deadline_ms` 10 000, `dust_notional` 5 USDT); where a
//! case quotes a W1 number the assertion uses the value the engine computes from B0's.

use hft_event::{ConnRole, ConnStateKind, EventKind, NoteLevel, OrderDecision, OrderEventKind, OrderFlags};
use hft_oms::{LinkBroken, LinkState, notes};
use hft_sequencer::driver::live::RingId;
use hft_strategy_sdk::{ExecSpec, FirstLeg, LinkRole, OnFail, OrderIntent, PairParams};
use hft_testkit::dst::Scenario;
use hft_types::{AccountId, CapSource, InstId, Px, Qty, RejectBits, Side};

use crate::common::*;
use crate::scenarios::*;

/// Registers a `dst` case this file runs as a fixed-step sequence on the real engine
/// (test-trace locates the case at the function containing the registration); the seeded W1
/// run comes with a test strategy that opens the `S1` buy first leg (scenarios.rs header).
pub(crate) fn case(s: Scenario) {
    let _ = s;
}

/// `OrderEvent{FeeOnly}` of `d` (fee in `asset`).
pub(crate) fn fee_only(h: &mut H, d: &OrderDecision, fee: Qty, asset: hft_types::AssetId) {
    let mut ev = order_event(d, OrderEventKind::FeeOnly, h.epoch());
    ev.fee = fee;
    ev.fee_asset = asset;
    ev.cum_filled = d.qty;
    let r = h.rep_ring(d.account);
    h.push(r, &ev);
    h.run();
}

/// `ExecDecision`s since `m`.
pub(crate) fn exec_decs(h: &H, m: usize) -> Vec<hft_event::ExecDecision> {
    h.since(m)
        .iter()
        .filter_map(|r| r.read::<hft_event::ExecDecision>())
        .collect()
}

/// The journal record with sequence number `seq`.
fn rec_at(h: &H, seq: u64) -> Option<hft_sequencer::MemRecord> {
    h.recs().into_iter().find(|r| r.seq == seq)
}

/// Decision records since `m` with their record tsc.
fn decs_t(h: &H, m: usize) -> Vec<(u64, OrderDecision)> {
    h.since(m)
        .iter()
        .filter_map(|r| r.read::<OrderDecision>().map(|d| (r.tsc, d)))
        .collect()
}

fn retries(h: &H, m: usize) -> Vec<(u64, OrderDecision)> {
    decs_t(h, m)
        .into_iter()
        .filter(|(_, d)| d.link_role == LinkRole::SecondRetry as u8)
        .collect()
}

/// The link-open of s1 with `f` applied to its hedge plan.
fn link_with(hx: &Hx<'_>, qty: &str, f: fn(&mut hft_strategy_sdk::HedgePlan)) -> OrderIntent {
    let mut it = pair_first(hx, q(qty));
    if let Some(l0) = it.link.get() {
        let mut l = *l0;
        f(&mut l.hedge);
        it.link = hft_strategy_sdk::LegLinkOpt::some(l);
    }
    it
}

fn open_with(h: &mut H, f: fn(&Hx<'_>) -> OrderIntent) -> OrderDecision {
    let m = place_on_a1(h, &[S1], f);
    let d = decs(h, m).into_iter().find(|d| d.inst == A1).expect("first leg");
    assert_eq!(d.reject, 0, "{d:?}");
    h.ack(&d);
    d
}

fn broken_notes(h: &H, m: usize) -> Vec<hft_event::Note> {
    notes_of(h, m, notes::HEDGE_BROKEN)
}

fn cap_set_has(h: &H, m: usize, src: CapSource, a: AccountId, g: Option<hft_types::GroupId>) -> bool {
    cap_sets(h, m)
        .iter()
        .any(|c| c.source == src && c.scope.account == a && g.is_none_or(|g| c.scope.group == g))
}

// ------------------------------------------------------------------------------------- B-5

/// Every second leg and retry is rejected by the venue while both books stay unchanged and their
/// connections answer every 1 000 ms: the retries come from `Timer{Oms, HedgeRetry}` Inputs.
#[test]
fn b_5_hedge_retry_timer_drives_retries() {
    case(Scenario::new(
        "B-5",
        "HedgeRetry timer drives the retries of a quiet book",
    ));
    for expire_instead in [false, true] {
        let mut h = steady();
        let first = open_first(&mut h);
        let m0 = h.mark();
        h.fill(&first, first.qty, first.qty, first.px);
        let mut broke_at = None;
        for _ in 0..6 {
            let open = decs(&h, m0)
                .into_iter()
                .rev()
                .find(|d| d.inst == B1 && d.reject == 0)
                .expect("hedge leg");
            let m = h.mark();
            if expire_instead {
                // B-5.2: zero-fill expiry instead of a rejection.
                h.ack(&open);
                expire(&mut h, &open);
            } else {
                h.reject(&open);
            }
            if last_link(&h, AC1).is_some_and(|l| l.state == LinkState::Broken) {
                broke_at = Some(m);
                break;
            }
            let due = hedge_retry_timers(&h).first().map(|t| t.0).expect("retry timer");
            // Connections alive (pongs), quotes unchanged.
            let t = h.now();
            for v in [A, B] {
                h.push(
                    RingId::Rep(0),
                    &conn(
                        v,
                        0,
                        ConnStateKind::Alive,
                        ConnRole::Md,
                        InstId(0),
                        AccountId(0),
                        t,
                    ),
                );
            }
            h.run();
            run_to(&mut h, due);
        }
        let m = broke_at.expect("Broken after the retries");
        let rs = retries(&h, m0);
        assert_eq!(rs.len(), 3, "exactly hedge_retry_max SecondRetry");
        for (t, d) in &rs {
            let trig = rec_at(&h, d.trigger_seq).expect("trigger record");
            let tm = trig.read::<hft_event::Timer>().expect("a Timer Input");
            assert_eq!((tm.owner, tm.timer_id & 0xFF), (3, 4), "Timer{{Oms, HedgeRetry}}");
            let _ = t;
        }
        for w in rs.windows(2) {
            assert!(w[1].0 - w[0].0 >= 100 * MS, "spacing ≥ hedge_retry_min_ms");
        }
        // No Bbo between the first fill and the last retry.
        assert!(h.since(m0).iter().all(|r| r.kind != EventKind::BBO));
        let l = last_link(&h, AC1).expect("link");
        assert_eq!(l.state, LinkState::Broken);
        let hb = broken_notes(&h, m);
        assert_eq!(hb.len(), 1);
        assert_eq!(hb[0].level, NoteLevel::P0);
        assert_eq!(hb[0].v[1], LinkBroken::RetryExhausted as u64);
        assert!(cap_set_has(&h, m, CapSource::BROKEN, AC1, Some(G1)));
        assert!(cap_set_has(&h, m, CapSource::HEDGE_FAILURE, AC1, None));
    }
}

/// B-5.1: a HARD Kill lands between the first leg and its fill: the second leg and every retry
/// are refused by risk #1 (`HARD`), one decision per `HedgeRetry`, not counted; `Broken{Deadline}`
/// after `hedge_deadline_ms`.
#[test]
fn b_5_1_hard_kill_gates_retries_until_deadline() {
    let mut h = steady();
    let first = open_first(&mut h);
    let id = h.cmd_confirmed(
        AC1,
        &hft_event::ControlCmd::Kill {
            scope: scope(hft_types::StrategyId(0), AC1, hft_types::GroupId(0)),
            level: hft_types::KillLevel::Account,
            flatten: false,
            cancel_resting: false,
            hard: true,
        },
    );
    h.run();
    assert_eq!(h.res(id).0, 0);
    let n = h.io_ref().out.len();
    let m0 = h.mark();
    let t_fill = h.now();
    h.fill(&first, first.qty, first.qty, first.px);
    keep_alive_to(&mut h, t_fill + 2_900 * MS);
    assert_eq!(last_link(&h, AC1).map(|l| l.state), Some(LinkState::Hedging));
    let hedges: Vec<_> = decs(&h, m0).into_iter().filter(|d| d.inst == B1).collect();
    assert!(!hedges.is_empty());
    assert!(
        hedges.iter().all(|d| d.reject == RejectBits::HARD.0),
        "{hedges:?}"
    );
    assert!(frames(&h, n).iter().all(|f| f.1.inst != B1), "nothing out");
    assert_eq!(last_link(&h, AC1).map(|l| l.retries), Some(0));
    // One decision per HedgeRetry Input.
    for d in hedges
        .iter()
        .filter(|d| d.link_role == LinkRole::SecondRetry as u8)
    {
        let tm = rec_at(&h, d.trigger_seq)
            .and_then(|r| r.read::<hft_event::Timer>())
            .expect("HedgeRetry Input");
        assert_eq!((tm.owner, tm.timer_id & 0xFF), (3, 4));
        let same: Vec<_> = hedges.iter().filter(|x| x.trigger_seq == d.trigger_seq).collect();
        assert_eq!(same.len(), 1);
    }
    let m = h.mark();
    keep_alive_to(&mut h, t_fill + 3_200 * MS);
    assert_eq!(last_link(&h, AC1).map(|l| l.state), Some(LinkState::Broken));
    let hb = broken_notes(&h, m);
    assert_eq!(hb.len(), 1);
    assert_eq!(
        (hb[0].level, hb[0].v[1]),
        (NoteLevel::P0, LinkBroken::Deadline as u64)
    );
}

// ------------------------------------------------------------------------------------- FP-5

#[test]
fn fp_5_second_leg_rejected_retries_then_broken() {
    case(Scenario::new(
        "FP-5",
        "second leg rejected: SecondRetry, then Broken",
    ));
    let mut h = steady();
    clear_log();
    let first = open_first(&mut h);
    let m0 = h.mark();
    h.fill(&first, first.qty, first.qty, first.px);
    let mut slots = Vec::new();
    let mut broke_at = None;
    for k in 1..=4u8 {
        let open = decs(&h, m0)
            .into_iter()
            .rev()
            .find(|d| d.inst == B1 && d.reject == 0)
            .expect("hedge leg");
        slots.push((open.slot, open.r#gen));
        let m = h.mark();
        h.reject(&open);
        let l = last_link(&h, AC1).expect("link");
        if k <= 3 {
            assert_eq!(l.retries, k, "retries after rejection {k}");
            assert_eq!(hedge_retry_timers(&h).len(), 1, "HedgeRetry after rejection {k}");
            let due = hedge_retry_timers(&h)[0].0;
            run_to(&mut h, due);
        } else {
            assert_eq!(l.retries, 4);
            assert_eq!(l.state, LinkState::Broken);
            broke_at = Some(m);
        }
    }
    let m = broke_at.expect("Broken on the 4th rejection");
    let hs: Vec<_> = decs_t(&h, m0).into_iter().filter(|(_, d)| d.inst == B1).collect();
    assert_eq!(
        hs.iter()
            .filter(|(_, d)| d.link_role == LinkRole::Second as u8)
            .count(),
        1
    );
    assert_eq!(
        hs.iter()
            .filter(|(_, d)| d.link_role == LinkRole::SecondRetry as u8)
            .count(),
        3
    );
    for (_, d) in &hs {
        assert_eq!((d.qty, d.side, d.reject), (q("0.01"), Side::Sell, 0));
        assert!(d.oflags & OrderFlags::RISK_REDUCING.0 != 0);
        assert!(d.oflags & OrderFlags::REDUCE_ONLY.0 == 0);
    }
    for w in hs.windows(2) {
        assert!(w[1].0 - w[0].0 >= 100 * MS);
    }
    slots.sort();
    slots.dedup();
    assert_eq!(slots.len(), 4, "a new slot for every hedge order");
    let hb = broken_notes(&h, m);
    assert_eq!((hb.len(), hb[0].level), (1, NoteLevel::P0));
    assert_eq!(hb[0].v[1], LinkBroken::RetryExhausted as u64);
    assert!(cap_set_has(&h, m, CapSource::BROKEN, AC1, Some(G1)));
    assert!(cap_set_has(&h, m, CapSource::HEDGE_FAILURE, AC1, None));
    h.run();
    assert!(
        log()
            .iter()
            .any(|g| g.s == S1 && g.kind == "Risk" && g.text.contains("HedgeBroken")),
        "RiskEvent::HedgeBroken to s1"
    );
    assert!(matches!(
        h.s.engine().exec.cell(AC1, G1).map(|c| c.view.state),
        Some(hft_strategy_sdk::TargetState::Broken(_))
    ));
    let lv = h.s.engine().oms.legs(AC1, G1).view;
    assert_eq!((lv.leg[0].net, lv.leg[1].net), (q("0.01"), Qty(0)));
    let m = place_on_a1(&mut h, &[S1], |hx| open_buy(hx, A1));
    assert_eq!(decs(&h, m)[0].reject, RejectBits::CLOSE_ONLY.0);
}

/// FP-5.1: `on_fail = Reverse`: the first rejection sends the reverse a1 sell (no reduceOnly on
/// spot), owed 0; its fill closes the link without retries or Broken.
#[test]
fn fp_5_1_reverse_on_reject() {
    let mut h = steady();
    let first = open_with(&mut h, |hx| {
        link_with(hx, "0.01", |p| p.on_fail = OnFail::Reverse)
    });
    let m0 = h.mark();
    h.fill(&first, first.qty, first.qty, first.px);
    let second = decs(&h, m0)
        .into_iter()
        .find(|d| d.inst == B1)
        .expect("second leg");
    let m = h.mark();
    h.reject(&second);
    let rev = decs(&h, m)
        .into_iter()
        .find(|d| d.link_role == LinkRole::Reverse as u8)
        .expect("reverse in the rejection's Input");
    assert_eq!(
        (rev.inst, rev.side, rev.qty, rev.tif, rev.reject),
        (A1, Side::Sell, q("0.01"), hft_types::Tif::Ioc, 0)
    );
    assert!(rev.oflags & OrderFlags::RISK_REDUCING.0 != 0);
    assert!(rev.oflags & OrderFlags::REDUCE_ONLY.0 == 0);
    let l = last_link(&h, AC1).expect("link");
    assert_eq!(l.owed(), Qty(0));
    h.ack(&rev);
    h.fill(&rev, rev.qty, rev.qty, rev.px);
    let l = last_link(&h, AC1).expect("link");
    assert_eq!((l.state, l.first_net), (LinkState::Done, Qty(0)));
    assert_eq!(h.s.engine().oms.legs(AC1, G1).view.leg[0].net, Qty(0));
    assert!(retries(&h, m0).is_empty());
    assert!(broken_notes(&h, m0).is_empty());
}

/// FP-5.3: `on_fail = Hold`: no retry and no reverse; the link is Done with the a1 leg naked.
#[test]
fn fp_5_3_hold_on_reject() {
    let mut h = steady();
    let first = open_with(&mut h, |hx| link_with(hx, "0.01", |p| p.on_fail = OnFail::Hold));
    let m0 = h.mark();
    h.fill(&first, first.qty, first.qty, first.px);
    let second = decs(&h, m0)
        .into_iter()
        .find(|d| d.inst == B1)
        .expect("second leg");
    h.reject(&second);
    assert_eq!(last_link(&h, AC1).map(|l| l.state), Some(LinkState::Done));
    assert!(hedge_retry_timers(&h).is_empty());
    assert!(
        decs(&h, m0)
            .iter()
            .all(|d| d.link_role != LinkRole::Reverse as u8)
    );
    assert!(retries(&h, m0).is_empty());
    assert_eq!(h.s.engine().oms.legs(AC1, G1).view.leg[0].net, q("0.01"));
}

// ------------------------------------------------------------------------------------- FP-6

/// The cross price of a b1 sell second leg: `cap_cross_px(Sell, bid, mid, px_param, lim, tick)`.
fn b1_sell_px(h: &H, bid: &str, ask: &str, px_param: hft_types::Rate) -> Px {
    let e = h.s.engine();
    let lim = e.risk.hedge_px_limits(AC1, B, B1);
    let mid = Px::mid_floor(px(bid), px(ask));
    hft_types::cap_cross_px(Side::Sell, px(bid), mid, px_param, &lim, h.rd.inst(B1).tick)
}

#[test]
fn fp_6_quiet_second_leg_book_still_priced() {
    case(Scenario::new("FP-6", "quiet second-leg book still priced"));
    // b1 fixed from −10 s, its connection answering every 1 000 ms.
    let mut h = steady();
    h.quote(B1, "50010", "50010.1");
    h.run();
    for _ in 0..10 {
        h.adv(SEC);
        let t = h.now();
        h.push(
            RingId::Rep(0),
            &conn(
                B,
                0,
                ConnStateKind::Alive,
                ConnRole::Md,
                InstId(0),
                AccountId(0),
                t,
            ),
        );
        h.push(
            RingId::Rep(0),
            &conn(
                A,
                0,
                ConnStateKind::Alive,
                ConnRole::Md,
                InstId(0),
                AccountId(0),
                t,
            ),
        );
        h.run();
    }
    let first = open_first(&mut h);
    h.adv(200 * MS);
    let n = h.io_ref().out.len();
    let m = h.mark();
    h.fill(&first, first.qty, first.qty, first.px);
    let second = decs(&h, m)
        .into_iter()
        .find(|d| d.inst == B1)
        .expect("second leg in the fill's Input");
    let want = b1_sell_px(&h, "50010", "50010.1", r("0.003"));
    assert_eq!(
        (
            second.side,
            second.qty,
            second.px,
            second.link_role,
            second.reject
        ),
        (Side::Sell, q("0.01"), want, LinkRole::Second as u8, 0)
    );
    assert!(frames(&h, n).iter().any(|f| f.1.inst == B1));
    assert!(
        notes_of(&h, m, notes::HEDGE_SLIP_CAPPED).is_empty(),
        "not capped, not slipped"
    );
    assert!(notes_of(&h, m, notes::HEDGE_PX_STALE).is_empty());
    assert!(hedge_retry_timers(&h).is_empty());
    assert_eq!(last_link(&h, AC1).map(|l| l.retries), Some(0));
    assert!(broken_notes(&h, m).is_empty());
}

/// FP-6.1: b1's connection Down at +0.1 s, Up with a Bbo at +1.0 s: the fill at +0.2 s is gated
/// `StaleQuote` (one `OmsHedgePxStale`, HedgeRetry at +0.3 s, retries 0); the first retry after
/// +1.0 s goes out at the same price. FP-6.2: the connection never answers again and the fill is
/// late: gated, retries 0, `Broken{Deadline}` once `hedge_deadline_ms` passed.
#[test]
fn fp_6_1_conn_down_gates_until_up() {
    let mut h = steady();
    h.quote(B1, "50010", "50010.1");
    h.run();
    let first = open_first(&mut h);
    let t0 = h.now();
    h.adv(100 * MS);
    let t = h.now();
    h.push(
        RingId::Rep(0),
        &conn(
            B,
            0,
            ConnStateKind::Down,
            ConnRole::Md,
            InstId(0),
            AccountId(0),
            t,
        ),
    );
    h.run();
    h.at(t0 + 200 * MS);
    let m = h.mark();
    let t_fill = h.now();
    h.fill(&first, first.qty, first.qty, first.px);
    assert!(
        decs(&h, m).iter().all(|d| d.inst != B1),
        "gated: no second leg decision"
    );
    assert_eq!(notes_of(&h, m, notes::HEDGE_PX_STALE).len(), 1);
    let timers = hedge_retry_timers(&h);
    assert_eq!(timers.len(), 1);
    assert_eq!(timers[0].0, t_fill + 100 * MS);
    assert_eq!(last_link(&h, AC1).map(|l| l.retries), Some(0));
    run_to(&mut h, t0 + 1_000 * MS);
    let t = h.now();
    h.push(
        RingId::Rep(0),
        &conn(B, 0, ConnStateKind::Up, ConnRole::Md, InstId(0), AccountId(0), t),
    );
    h.run();
    h.quote(B1, "50010", "50010.1");
    h.run();
    let m2 = h.mark();
    let due = hedge_retry_timers(&h)
        .first()
        .map(|x| x.0)
        .expect("HedgeRetry pending");
    run_to(&mut h, due);
    let rs = retries(&h, m2);
    assert_eq!(rs.len(), 1, "{rs:?}");
    let want = b1_sell_px(&h, "50010", "50010.1", r("0.003"));
    assert_eq!((rs[0].1.qty, rs[0].1.px, rs[0].1.reject), (q("0.01"), want, 0));
    assert!(broken_notes(&h, m).is_empty());

    // FP-6.2: silent from +0.001 s, fill at +2.5 s.
    let mut h = steady();
    h.quote(B1, "50010", "50010.1");
    h.run();
    let first = open_first(&mut h);
    let t0 = h.now();
    h.at(t0 + 2_500 * MS);
    // a1's own connection keeps answering.
    let t = h.now();
    h.push(
        RingId::Rep(0),
        &conn(
            A,
            0,
            ConnStateKind::Alive,
            ConnRole::Md,
            InstId(0),
            AccountId(0),
            t,
        ),
    );
    h.run();
    let m = h.mark();
    let t_fill = h.now();
    h.fill(&first, first.qty, first.qty, first.px);
    assert!(notes_of(&h, m, notes::HEDGE_PX_STALE).len() >= 1);
    run_to(&mut h, t_fill + 2_950 * MS);
    assert_eq!(
        last_link(&h, AC1).map(|l| (l.state, l.retries)),
        Some((LinkState::Hedging, 0))
    );
    run_to(&mut h, t_fill + 3_200 * MS);
    assert_eq!(last_link(&h, AC1).map(|l| l.state), Some(LinkState::Broken));
    let hb = broken_notes(&h, m);
    assert_eq!(
        (hb.len(), hb[0].level, hb[0].v[1]),
        (1, NoteLevel::P0, LinkBroken::Deadline as u64)
    );
}

// ------------------------------------------------------------------------------------- FP-7

fn slip_link(hx: &Hx<'_>) -> OrderIntent {
    link_with(hx, "0.01", |p| {
        p.px_param = r("0.0005");
        p.max_slip = r("0.0005");
    })
}

#[test]
fn fp_7_slippage_priced_at_cap_not_failed() {
    case(Scenario::new(
        "FP-7",
        "slippage priced at the cross cap, not failed",
    ));
    let mut h = steady();
    h.quote(B1, "50010", "50010.1");
    h.run();
    let first = open_with(&mut h, slip_link);
    let l = last_link(&h, AC1).expect("link");
    assert_eq!(l.ref_px, px("50010"));
    // b1 moves 0.4 % (> max_slip, < rr_max_cross of the mid).
    h.quote(B1, "49810", "49810.1");
    h.run();
    let n = h.io_ref().out.len();
    let m = h.mark();
    h.fill(&first, first.qty, first.qty, first.px);
    let second = decs(&h, m)
        .into_iter()
        .find(|d| d.inst == B1)
        .expect("second leg");
    let cap = sell_cap(&h, "49810", "49810.1");
    assert_eq!(
        (second.qty, second.tif, second.px, second.reject),
        (q("0.01"), hft_types::Tif::Ioc, cap, 0)
    );
    assert!(frames(&h, n).iter().any(|f| f.1.inst == B1 && f.1.px == cap));
    let sc = notes_of(&h, m, notes::HEDGE_SLIP_CAPPED);
    assert_eq!(sc.len(), 1);
    assert_eq!(
        sc[0].v[..4],
        [
            u64::from(l.link_id),
            px("50010").0 as u64,
            px("49810").0 as u64,
            cap.0 as u64
        ]
    );
    assert_eq!(last_link(&h, AC1).map(|l| l.retries), Some(0));
    h.ack(&second);
    h.fill(&second, second.qty, second.qty, second.px);
    assert_eq!(last_link(&h, AC1).map(|l| l.state), Some(LinkState::Done));

    // FP-7.2: the IOC at the cap expires unfilled → retries 1, HedgeRetry at +100 ms.
    let mut h = steady();
    h.quote(B1, "50010", "50010.1");
    h.run();
    let first = open_with(&mut h, slip_link);
    h.quote(B1, "49810", "49810.1");
    h.run();
    let m = h.mark();
    h.fill(&first, first.qty, first.qty, first.px);
    let second = decs(&h, m)
        .into_iter()
        .find(|d| d.inst == B1)
        .expect("second leg");
    h.ack(&second);
    let t = h.now();
    expire(&mut h, &second);
    assert_eq!(last_link(&h, AC1).map(|l| l.retries), Some(1));
    assert_eq!(hedge_retry_timers(&h).first().map(|x| x.0), Some(t + 100 * MS));
}

/// FP-7.1: the bid beyond the cap at the fill: gated `BeyondCross` (one `OmsHedgePxGated`,
/// retries 0, nothing out); after the book is back the first retry goes out at the cap with an
/// `OmsHedgeSlipCapped`; no Broken.
#[test]
fn fp_7_1_beyond_cross_gated_until_back() {
    let mut h = steady();
    h.quote(B1, "50010", "50010.1");
    h.run();
    let first = open_with(&mut h, slip_link);
    h.quote(B1, "48000", "52000");
    h.run();
    let n = h.io_ref().out.len();
    let m = h.mark();
    let t_fill = h.now();
    h.fill(&first, first.qty, first.qty, first.px);
    assert!(decs(&h, m).iter().all(|d| d.inst != B1));
    assert!(frames(&h, n).iter().all(|f| f.1.inst != B1));
    assert_eq!(notes_of(&h, m, notes::HEDGE_PX_GATED).len(), 1);
    assert_eq!(last_link(&h, AC1).map(|l| l.retries), Some(0));
    run_to(&mut h, t_fill + 800 * MS);
    h.quote(B1, "49810", "49810.1");
    h.run();
    let m2 = h.mark();
    run_to(&mut h, t_fill + 1_500 * MS);
    let rs = retries(&h, m2);
    assert_eq!(rs.len(), 1, "{rs:?}");
    assert_eq!(
        (rs[0].1.px, rs[0].1.reject),
        (sell_cap(&h, "49810", "49810.1"), 0)
    );
    assert_eq!(notes_of(&h, m2, notes::HEDGE_SLIP_CAPPED).len(), 1);
    assert!(broken_notes(&h, m).is_empty());
}

// ------------------------------------------------------------------------------------- QF-1

/// b1 unchanged from −10 s, its connection answering every 1 000 ms; returns the harness at 0.
fn quiet_b1() -> H {
    let mut h = steady();
    h.quote(B1, "50010", "50010.1");
    h.run();
    for _ in 0..10 {
        h.adv(SEC);
        let t = h.now();
        for v in [A, B] {
            h.push(
                RingId::Rep(0),
                &conn(
                    v,
                    0,
                    ConnStateKind::Alive,
                    ConnRole::Md,
                    InstId(0),
                    AccountId(0),
                    t,
                ),
            );
        }
        h.run();
    }
    h
}

fn big_link(hx: &Hx<'_>) -> OrderIntent {
    link_with(hx, "0.1", |p| {
        p.px_param = r("0.002");
        p.max_slip = r("0.001");
    })
}

#[test]
fn qf_1_quiet_book_still_hedges() {
    case(Scenario::new("QF-1", "quiet second-leg book still hedged"));
    let mut h = quiet_b1();
    let m0 = h.mark();
    let first = open_with(&mut h, big_link);
    let t0 = h.now();
    h.adv(200 * MS);
    let m = h.mark();
    h.fill(&first, first.qty, first.qty, first.px);
    let second = decs(&h, m)
        .into_iter()
        .find(|d| d.inst == B1)
        .expect("second leg in the fill's Input");
    let want = b1_sell_px(&h, "50010", "50010.1", r("0.002"));
    assert_eq!(
        (
            second.link_role,
            second.side,
            second.qty,
            second.tif,
            second.px,
            second.reject
        ),
        (
            LinkRole::Second as u8,
            Side::Sell,
            q("0.1"),
            hft_types::Tif::Ioc,
            want,
            0
        )
    );
    h.ack(&second);
    h.fill(&second, second.qty, second.qty, second.px);
    // QF-1.1: another linked open at +5 s is not LINK_STALE.
    keep_alive_to(&mut h, t0 + 5 * SEC);
    let m1 = place_on_a1(&mut h, &[S1], |hx| link_with(hx, "0.05", |_| {}));
    let d = decs(&h, m1)
        .into_iter()
        .find(|d| d.inst == A1)
        .expect("second open");
    assert_eq!(d.reject, 0);
    keep_alive_to(&mut h, t0 + 10 * SEC);
    assert!(notes_of(&h, m0, notes::HEDGE_PX_STALE).is_empty());
    assert!(
        decs(&h, m0)
            .iter()
            .all(|d| d.reject & (RejectBits::STALE_MID.0 | RejectBits::LINK_STALE.0) == 0)
    );
    assert!(broken_notes(&h, m0).is_empty());
    assert!(cap_sets(&h, m0).iter().all(|c| c.source != CapSource::BROKEN));
}

/// QF-1.4 / QF-1.6: b1's last frame 1.9 s before the open, so the open passes and the fill at
/// +0.2 s is gated `StaleQuote`; .4 the connection answers again at +1.0 s and the next retry goes
/// out; .6 it never does: no Broken before +3.2 s, `Broken{Deadline}` at the first retry after.
#[test]
fn qf_1_4_stale_at_fill_gated_then_retried() {
    for recover in [true, false] {
        let mut h = steady();
        h.quote(B1, "50010", "50010.1");
        h.run();
        let t_last = h.now();
        h.at(t_last + 1_900 * MS);
        let t = h.now();
        h.push(
            RingId::Rep(0),
            &conn(
                A,
                0,
                ConnStateKind::Alive,
                ConnRole::Md,
                InstId(0),
                AccountId(0),
                t,
            ),
        );
        h.run();
        let first = open_with(&mut h, big_link);
        let t0 = h.now();
        h.at(t0 + 200 * MS);
        let m = h.mark();
        h.fill(&first, first.qty, first.qty, first.px);
        assert!(decs(&h, m).iter().all(|d| d.inst != B1));
        assert_eq!(notes_of(&h, m, notes::HEDGE_PX_STALE).len(), 1);
        assert_eq!(hedge_retry_timers(&h).len(), 1);
        assert_eq!(last_link(&h, AC1).map(|l| l.retries), Some(0));
        if recover {
            run_to(&mut h, t0 + 1_000 * MS);
            let t = h.now();
            h.push(
                RingId::Rep(0),
                &conn(
                    B,
                    0,
                    ConnStateKind::Alive,
                    ConnRole::Md,
                    InstId(0),
                    AccountId(0),
                    t,
                ),
            );
            h.run();
            let m2 = h.mark();
            run_to(&mut h, t0 + 1_100 * MS);
            let rs = retries(&h, m2);
            assert_eq!(rs.len(), 1, "{rs:?}");
            assert_eq!((rs[0].1.qty, rs[0].1.reject), (q("0.1"), 0));
            assert!(broken_notes(&h, m).is_empty());
        } else {
            run_to(&mut h, t0 + 3_150 * MS);
            assert!(broken_notes(&h, m).is_empty(), "no Broken before fill + 3 000 ms");
            run_to(&mut h, t0 + 3_500 * MS);
            let hb = broken_notes(&h, m);
            assert_eq!(
                (hb.len(), hb[0].level, hb[0].v[1]),
                (1, NoteLevel::P0, LinkBroken::Deadline as u64)
            );
            assert!(cap_set_has(&h, m, CapSource::BROKEN, AC1, Some(G1)));
        }
    }
}

// ------------------------------------------------------------------------------------- B-4

/// s2's PairOnFill target on g2 (a1 first, ratio 1/1, `fix_tolerance` 0 → the default).
fn pair_fixed() -> PairParams {
    let mut p = pair0();
    p.first_leg = FirstLeg::Fixed;
    p.first_leg_ix = 0;
    p.fix_tolerance = r("0");
    p
}

/// s2 opens g2 a1 +0.01 / b1 −0.01; returns the first leg (filled without a fee) and the second
/// leg (out, unanswered).
fn g2_pair_open(h: &mut H) -> (OrderDecision, OrderDecision) {
    let m = h.mark();
    h.set_target_s2(target(
        AC2,
        G2,
        &[(0, q("0.01")), (1, q("-0.01"))],
        ExecSpec::PairOnFill(pair_fixed()),
    ));
    let first = decs(h, m)
        .into_iter()
        .find(|d| d.inst == A1 && d.account == AC2)
        .expect("first leg");
    assert_eq!((first.qty, first.reject), (q("0.01"), 0));
    h.ack(&first);
    let m = h.mark();
    h.fill(&first, first.qty, first.qty, first.px);
    let second = decs(h, m)
        .into_iter()
        .find(|d| d.inst == B1 && d.account == AC2)
        .expect("second leg");
    assert_eq!(second.reject, 0);
    (first, second)
}

fn fix_state(h: &H) -> hft_exec::FixState {
    h.s.engine().exec.cell(AC2, G2).expect("cell").fix
}

fn no_fix(h: &H, m: usize) -> bool {
    exec_decs(h, m).iter().all(|x| {
        x.action != hft_exec::decision::action::HEDGE_FIX
            && x.action != hft_exec::decision::action::RESIDUAL_TRIM
            && x.action != hft_exec::decision::action::EXTERNAL_DEFAULT
    }) && decs(h, m).iter().all(|d| d.link_role != LinkRole::HedgeFix as u8)
}

#[test]
fn b_4_inflight_second_leg_suppresses_hedge_fix() {
    case(Scenario::new("B-4", "in-flight second leg suppresses HedgeFix"));
    let mut h = steady();
    let m0 = h.mark();
    let (first, second) = g2_pair_open(&mut h);
    let t0 = h.now();
    h.at(t0 + 100 * MS);
    fee_only(&mut h, &first, q("0.00001"), BTC);
    let lv = h.s.engine().oms.legs(AC2, G2).view;
    assert_eq!(lv.naked_qty, q("0.00001"), "in-flight second leg counted");
    assert_eq!(lv.open_links, 1);
    assert!(no_fix(&h, m0));
    assert_eq!((fix_state(&h).first_tsc.0, fix_state(&h).count), (0, 0));
    // The second leg's reports are 800 ms late.
    keep_alive_to(&mut h, t0 + 800 * MS);
    h.ack(&second);
    h.fill(&second, second.qty, second.qty, second.px);
    assert_eq!(last_link(&h, AC2).map(|l| l.state), Some(LinkState::Done));
    keep_alive_to(&mut h, t0 + 5 * SEC);
    assert!(no_fix(&h, m0), "filled naked 0.00001 is dust");

    // B-4.1: the second leg expires unfilled; its SecondRetry is out until it fills: Hedging,
    // no HedgeFix and no fix window although the filled naked is 0.01.
    let mut h = steady();
    let m0 = h.mark();
    let (_, second) = g2_pair_open(&mut h);
    h.ack(&second);
    expire(&mut h, &second);
    let due = hedge_retry_timers(&h)[0].0;
    run_to(&mut h, due);
    let retry = retries(&h, m0).first().map(|x| x.1).expect("SecondRetry");
    keep_alive_to(&mut h, due + 800 * MS);
    assert_eq!(last_link(&h, AC2).map(|l| l.state), Some(LinkState::Hedging));
    assert!(no_fix(&h, m0));
    assert_eq!(fix_state(&h).first_tsc.0, 0);
    h.ack(&retry);
    h.fill(&retry, retry.qty, retry.qty, retry.px);
    assert_eq!(last_link(&h, AC2).map(|l| l.state), Some(LinkState::Done));

    // B-4.4: after the link is Done, a manual b1 buy 0.01 closes the hedge leg: manual_hold
    // (`ExecManualHold`), then no HedgeFix / ResidualTrim / ExternalDefault, FixState zero.
    let mut h = steady();
    let (_, second) = g2_pair_open(&mut h);
    h.ack(&second);
    h.fill(&second, second.qty, second.qty, second.px);
    let m = h.mark();
    let mut x = crate::process::ext(
        AC2,
        B1,
        Side::Buy,
        "0.01",
        hft_event::ExternalCause::Manual,
        4_404,
    );
    x.px = px("50011");
    h.push(RingId::Rep(0), &x);
    h.run();
    assert!(
        h.notes_since(m)
            .iter()
            .any(|n| n.source == hft_event::NoteSource::Exec
                && n.code & 0xFFFF == hft_exec::decision::note::EXEC_MANUAL_HOLD),
        "ExecManualHold"
    );
    let t = h.now();
    keep_alive_to(&mut h, t + 15 * SEC);
    assert!(no_fix(&h, m));
    assert_eq!(fix_state(&h), hft_exec::FixState::ZERO);
}

/// B-4.3: the second leg is refused until `hedge_deadline_ms` → g2 Broken; while Broken the
/// executor sends no HedgeFix (only the bounded auto-fix after `broken_auto_fix_ms`).
#[test]
fn b_4_3_broken_group_no_hedge_fix() {
    let mut h = steady();
    let m0 = h.mark();
    let (_, second) = g2_pair_open(&mut h);
    h.reject(&second);
    for _ in 0..8 {
        if has_cap(&h, CapSource::BROKEN, AC2, Some(G2)) {
            break;
        }
        let Some(due) = hedge_retry_timers(&h).first().map(|t| t.0) else {
            break;
        };
        keep_alive_to(&mut h, due);
        if let Some(open) = decs(&h, m0).into_iter().rev().find(|d| {
            d.inst == B1 && d.reject == 0 && d.account == AC2 && slot_state(&h, d).is_some_and(|s| s.open())
        }) {
            h.reject(&open);
        }
    }
    assert!(has_cap(&h, CapSource::BROKEN, AC2, Some(G2)));
    let m = h.mark();
    let t = h.now();
    keep_alive_to(&mut h, t + 30 * SEC);
    assert!(no_fix(&h, m), "no HedgeFix while Broken");
}

// ------------------------------------------------------------------------------------- C1

/// An `UnknownResolved` entry of the reconciler for `d` (`order_status` NotFound).
fn unknown_resolved(
    h: &H,
    d: &OrderDecision,
    history_checked: u8,
    exch_qty: Qty,
    id: u64,
) -> hft_event::ReconcileReport {
    let mut x = recon_summary(
        d.account,
        d.venue,
        id,
        h.j().records().last().map_or(0, |r| r.seq),
    );
    x.item = 4;
    x.count = 1;
    x.inst = d.inst;
    x.slot = d.slot;
    x.r#gen = d.r#gen;
    x.epoch = h.epoch();
    x.order_status = 7;
    x.history_checked = history_checked;
    x.exch_qty = exch_qty;
    x
}

fn query_result(h: &mut H, d: &OrderDecision, status: u8) {
    let mut ev = order_event(d, OrderEventKind::QueryResult, h.epoch());
    ev.detail = status.into();
    let r = h.rep_ring(d.account);
    h.push(r, &ev);
    h.run();
}

fn queries(h: &H, m: usize, d: &OrderDecision) -> Vec<(u64, OrderDecision)> {
    decisions(&h.since(m))
        .into_iter()
        .filter(|x| x.1.action == hft_event::OrderAction::Query && x.1.slot == d.slot)
        .collect()
}

fn slot_of(h: &H, d: &OrderDecision) -> Option<hft_oms::OrderSlot> {
    let r = hft_strategy_sdk::OrderRef::new(d.venue, d.account, d.slot.0, d.r#gen);
    h.s.engine().oms.slot(r).copied()
}

/// Keeps both venues' market connections alive up to `t`, answering every Query of `d` with
/// NotFound (and its `SendReport{Query, Written}`).
fn not_found_to(h: &mut H, d: &OrderDecision, t: u64) {
    while h.now() < t {
        let step = (t - h.now()).min(100 * MS);
        let m = h.mark();
        h.adv(step);
        let now = h.now();
        for v in [A, B] {
            h.push(
                RingId::Rep(0),
                &conn(
                    v,
                    0,
                    ConnStateKind::Alive,
                    ConnRole::Md,
                    InstId(0),
                    AccountId(0),
                    now,
                ),
            );
        }
        h.run();
        for (seq, _) in queries(h, m, d) {
            let now = h.now();
            h.push(RingId::Rep(0), &send_report(d, hft_event::GwOp::Query, seq, now));
            h.run();
            query_result(h, d, 7);
        }
    }
}

#[test]
fn c1_hedge_leg_notfound_waits_history() {
    case(Scenario::new(
        "C1",
        "hedge leg NotFound stays Unknown until trade history",
    ));
    reset_script();
    let mut b = b0();
    b.setup_edit.push(|s| s.oms.hedge_deadline_ms = 60_000);
    let mut h = b.boot();
    h.steady();
    let first = open_first(&mut h);
    let m0 = h.mark();
    h.fill(&first, first.qty, first.qty, first.px);
    let (seq, s2) = decisions(&h.since(m0))
        .into_iter()
        .find(|d| d.1.inst == B1)
        .expect("second leg");
    let t_sent = h.now();
    h.push(
        RingId::Rep(0),
        &send_report(&s2, hft_event::GwOp::Place, seq, t_sent),
    );
    h.run();
    h.adv(10 * MS);
    order_conns(&mut h, B, AC1, ConnStateKind::Down, &[0, 1]);
    assert_eq!(slot_state(&h, &s2), Some(hft_oms::OrderState::Unknown));
    // Down until +0.95 s, then Up: one Query in the Up Input.
    keep_alive_to(&mut h, t_sent + 950 * MS);
    let m = h.mark();
    order_conns(&mut h, B, AC1, ConnStateKind::Up, &[0]);
    let q0 = queries(&h, m, &s2);
    assert_eq!(q0.len(), 1, "one Query in the Up Input");
    let t = h.now();
    h.push(
        RingId::Rep(0),
        &send_report(&s2, hft_event::GwOp::Query, q0[0].0, t),
    );
    h.run();
    // NotFound within the grace: no HIST_WAIT.
    let m = h.mark();
    query_result(&mut h, &s2, 7);
    assert!(notes_of(&h, m, notes::NEEDS_REST_QUERY).is_empty());
    let grace_end = t_sent + 2_000 * MS;
    not_found_to(&mut h, &s2, grace_end + 1_100 * MS);
    let sl = slot_of(&h, &s2).expect("slot");
    assert_eq!(sl.state, hft_oms::OrderState::Unknown);
    assert!(
        sl.sflags.has(hft_oms::SlotFlags::HIST_WAIT),
        "HIST_WAIT after the grace"
    );
    let nr: Vec<_> = notes_of(&h, m, notes::NEEDS_REST_QUERY);
    assert!(
        !nr.is_empty() && nr.iter().all(|n| n.v[0] >> 16 & 1 == 1),
        "trades = 1"
    );
    // HIST_WAIT: the timer rewrites the Note instead of querying; query_count stays.
    let qc = sl.query_count;
    let m = h.mark();
    let t4 = t_sent + 4_950 * MS;
    not_found_to(&mut h, &s2, t4);
    assert!(queries(&h, m, &s2).is_empty());
    assert!(!notes_of(&h, m, notes::NEEDS_REST_QUERY).is_empty());
    h.push(RingId::Rep(0), &unknown_resolved(&h, &s2, 0, Qty(0), 77_001));
    h.run();
    let sl = slot_of(&h, &s2).expect("slot");
    assert_eq!((sl.state, sl.query_count), (hft_oms::OrderState::Unknown, qc));
    not_found_to(&mut h, &s2, t_sent + 6_950 * MS);
    assert!(retries(&h, m0).is_empty(), "no SecondRetry while Unknown");
    // History confirmed: Rejected (Reconcile), HedgeRetry, then SecondRetry 0.01.
    let m = h.mark();
    h.push(RingId::Rep(0), &unknown_resolved(&h, &s2, 1, Qty(0), 77_002));
    h.run();
    let sl = slot_of(&h, &s2).expect("slot");
    assert_eq!((sl.state, sl.reject_code), (hft_oms::OrderState::Rejected, 0));
    let tr: Vec<_> = notes_of(&h, m, notes::TRANSITION);
    assert!(
        tr.iter().any(|n| n.v[1] == u64::from(s2.slot.0)
            && (n.v[0] >> 32) & 0xFF == hft_oms::TransitionCause::Reconcile as u64),
        "{tr:?}"
    );
    let due = hedge_retry_timers(&h)
        .first()
        .map(|x| x.0)
        .expect("HedgeRetry registered");
    keep_alive_to(&mut h, due);
    let rs = retries(&h, m);
    assert_eq!(rs.len(), 1);
    assert_eq!((rs[0].1.qty, rs[0].1.reject), (q("0.01"), 0));
}

/// C1.3: `hedge_deadline_ms` 3 000: the first deadline check after it (a sweep or the slot's
/// `UnknownQuery`) sets `Broken{Deadline}` with the second leg still Unknown.
#[test]
fn c1_3_deadline_while_unknown() {
    let mut h = steady();
    let first = open_first(&mut h);
    let m0 = h.mark();
    let t_fill = h.now();
    h.fill(&first, first.qty, first.qty, first.px);
    let (seq, s2) = decisions(&h.since(m0))
        .into_iter()
        .find(|d| d.1.inst == B1)
        .expect("second leg");
    h.push(
        RingId::Rep(0),
        &send_report(&s2, hft_event::GwOp::Place, seq, t_fill),
    );
    h.run();
    h.adv(10 * MS);
    order_conns(&mut h, B, AC1, ConnStateKind::Down, &[0, 1]);
    keep_alive_to(&mut h, t_fill + 2_950 * MS);
    assert_ne!(last_link(&h, AC1).map(|l| l.state), Some(LinkState::Broken));
    let m = h.mark();
    keep_alive_to(&mut h, t_fill + 5 * SEC);
    assert_eq!(last_link(&h, AC1).map(|l| l.state), Some(LinkState::Broken));
    let hb = broken_notes(&h, m);
    assert_eq!(
        (hb.len(), hb[0].level, hb[0].v[1]),
        (1, NoteLevel::P0, LinkBroken::Deadline as u64)
    );
    assert_eq!(slot_state(&h, &s2), Some(hft_oms::OrderState::Unknown));
}

// ------------------------------------------------------------------------------------- C4

fn hold_link(hx: &Hx<'_>) -> OrderIntent {
    link_with(hx, "0.02", |p| p.on_fail = OnFail::Hold)
}

/// g1 left with a1 +0.01 extra: a Hold link whose second leg filled 0.01 of 0.02 and expired.
/// With `backoff`, (A, a) enters a 20 s backoff from another order's rate-limit rejection at the
/// moment the link is done.
fn naked_a1(h: &mut H, backoff: bool) {
    let first = open_with(h, hold_link);
    if backoff {
        rate_limit_hit(h, A1);
    }
    let m = h.mark();
    h.fill(&first, first.qty, first.qty, first.px);
    let second = decs(h, m).into_iter().find(|d| d.inst == B1).expect("second leg");
    h.ack(&second);
    h.fill(&second, q("0.01"), q("0.01"), second.px);
    expire(h, &second);
    assert_eq!(last_link(h, AC1).map(|l| l.state), Some(LinkState::Done));
}

fn hedge_fix_decs(h: &H, m: usize) -> Vec<(u64, OrderDecision)> {
    decs_t(h, m)
        .into_iter()
        .filter(|(_, d)| d.link_role == LinkRole::HedgeFix as u8)
        .collect()
}

fn fix_exhausted(h: &H, m: usize) -> bool {
    exec_decs(h, m).iter().any(|x| {
        x.action == hft_exec::decision::action::STATE
            && x.state_to == 4
            && x.detail
                == hft_exec::pack(
                    hft_exec::decision::table::BROKEN,
                    hft_strategy_sdk::BrokenReason::FixExhausted.code(),
                    0,
                )
    })
}

#[test]
fn c4_auto_hedge_fix_gated_bounded() {
    case(Scenario::new(
        "C4",
        "auto HedgeFix gated: not counted, bounded by fix_deadline_ms",
    ));
    reset_script();
    let mut b = b0();
    let rd = b.rd.load();
    let mut rp = b0_risk(&rd);
    let mut v = rp.venues.as_slice().to_vec();
    for x in v.iter_mut() {
        if x.venue == A && x.account == AC1 {
            x.backoff_base_ms = 20_000;
            x.backoff_max_ms = 20_000;
        }
    }
    rp.venues = hft_risk::PreVec::from_vec(v);
    b.risk = Some(rp);
    let mut h = b.boot();
    h.steady();
    let n = h.io_ref().out.len();
    let m0 = h.mark();
    let t1 = h.now();
    naked_a1(&mut h, true);
    keep_alive_to(&mut h, t1 + 13 * SEC);
    let fx = hedge_fix_decs(&h, m0);
    assert!(!fx.is_empty());
    for (_, d) in &fx {
        assert_eq!(
            (d.inst, d.side, d.qty, d.reject),
            (A1, Side::Sell, q("0.01"), RejectBits::RATE_BACKOFF.0)
        );
    }
    for w in fx.windows(2) {
        assert!(w[1].0 - w[0].0 >= 500 * MS, "gated_backoff_ms");
    }
    let first_fix = fx[0].0;
    assert!(fx.iter().filter(|(t, _)| *t <= first_fix + 10 * SEC).count() <= 21);
    // Broken{FixExhausted} at the first advance after first trigger + fix_deadline_ms.
    let broken_rec = h
        .since(m0)
        .into_iter()
        .find(|r| {
            r.read::<hft_event::ExecDecision>()
                .is_some_and(|x| x.action == hft_exec::decision::action::STATE && x.state_to == 4)
        })
        .expect("Broken");
    assert!(fix_exhausted(&h, m0));
    let first_tsc = first_fix;
    assert!(
        broken_rec.tsc > first_tsc + 10 * SEC - 500 * MS,
        "no Broken before the deadline"
    );
    assert!(broken_rec.tsc <= first_tsc + 10 * SEC + 600 * MS);
    assert!(
        h.notes_since(m0)
            .iter()
            .any(|x| x.source == hft_event::NoteSource::Exec
                && x.code & 0xFFFF == hft_exec::decision::note::EXEC_BROKEN
                && x.level == NoteLevel::P0)
    );
    assert!(cap_set_has(&h, m0, CapSource::BROKEN, AC1, Some(G1)));
    assert!(
        frames(&h, n)
            .iter()
            .all(|f| f.1.inst != A1 || f.1.side != Side::Sell),
        "no HedgeFix frame"
    );
}

/// C4.1: no rate limit; the venue rejects every HedgeFix: exactly `fix_max` (3) orders at least
/// `fix_cooldown_ms` apart, the third rejection sets `Broken{FixExhausted}` (P0 `ExecBroken`).
#[test]
fn c4_1_rejected_fixes_exhaust() {
    let mut h = steady();
    let m0 = h.mark();
    naked_a1(&mut h, false);
    let t = h.now();
    let mut seen = 0;
    let mut until = t;
    while until < t + 15 * SEC && !fix_exhausted(&h, m0) {
        until += 100 * MS;
        keep_alive_to(&mut h, until);
        let fx = hedge_fix_decs(&h, m0);
        if fx.len() > seen {
            seen = fx.len();
            let d = fx[seen - 1].1;
            assert_eq!(d.reject, 0);
            h.reject(&d);
        }
    }
    let fx = hedge_fix_decs(&h, m0);
    assert_eq!(fx.len(), 3, "{fx:?}");
    for w in fx.windows(2) {
        assert!(w[1].0 - w[0].0 >= 1_000 * MS, "fix_cooldown_ms");
    }
    assert!(fix_exhausted(&h, m0));
    assert!(
        h.notes_since(m0)
            .iter()
            .any(|x| x.source == hft_event::NoteSource::Exec
                && x.code & 0xFFFF == hft_exec::decision::note::EXEC_BROKEN
                && x.level == NoteLevel::P0)
    );
}

// ------------------------------------------------------------------------------------- LT-1

#[test]
fn lt_1_lot_alignment_and_mismatch() {
    case(Scenario::new("LT-1", "two-leg lot alignment and LotMismatch"));
    reset_script();
    let mut b = b0();
    for s in b.rd.insts.iter_mut() {
        if s.id == B1 {
            s.lot = 1_000_000;
            s.min_qty = Qty(1_000_000);
            s.lot_str = "0.01".into();
            s.min_qty_str = "0.01".into();
        }
    }
    b.setup_edit
        .push(|s| s.exec.default.force_slice.max_chunk_notional = n("300"));
    let mut h = b.boot();
    h.steady();
    let mut pp = pair0();
    pp.first_leg = FirstLeg::Fixed;
    pp.first_leg_ix = 0;
    pp.slice.max_chunk_notional = n("3000");
    pp.slice.depth_frac = r("1.0");
    pp.slice.depth_levels = 1;
    let m0 = h.mark();
    h.set_target_s2(target(
        AC2,
        G2,
        &[(0, q("0.13")), (1, q("-0.13"))],
        ExecSpec::PairOnFill(pp),
    ));
    assert_eq!(
        h.s.engine().exec.cell(AC2, G2).map(|c| c.pair_lot),
        Some(q("0.01"))
    );
    let mut firsts = Vec::new();
    let t = h.now();
    let mut until = t;
    let mut answered = 0usize;
    while until < t + 20 * SEC {
        until += 100 * MS;
        keep_alive_to(&mut h, until);
        let ds: Vec<_> = decs(&h, m0)
            .into_iter()
            .filter(|d| d.account == AC2 && d.reject == 0 && d.action == hft_event::OrderAction::New)
            .collect();
        for d in ds.iter().skip(answered) {
            if d.inst == A1 {
                firsts.push(d.qty);
            }
            h.ack(d);
            h.fill(d, d.qty, d.qty, d.px);
            if d.inst == B1 {
                let lv = h.s.engine().oms.legs(AC2, G2).view;
                assert_eq!(
                    (lv.leg[0].residual, lv.leg[1].residual),
                    (Qty(0), Qty(0)),
                    "remainders"
                );
            }
        }
        answered = ds.len();
        if h.s
            .engine()
            .exec
            .cell(AC2, G2)
            .is_some_and(|c| c.view.state == hft_strategy_sdk::TargetState::Idle)
        {
            break;
        }
    }
    assert!(!firsts.is_empty());
    for x in &firsts {
        assert_eq!(x.0 % 1_000_000, 0, "first leg a multiple of pair_lot: {firsts:?}");
    }
    assert_eq!(firsts.iter().map(|x| x.0).sum::<i64>(), q("0.13").0, "{firsts:?}");
    // Every second leg equals its slice's first-leg net, a multiple of b1's lot.
    let seconds: Vec<Qty> = decs(&h, m0)
        .into_iter()
        .filter(|d| d.account == AC2 && d.inst == B1 && d.reject == 0)
        .map(|d| d.qty)
        .collect();
    assert_eq!(seconds, firsts);
    assert!(
        exec_decs(&h, m0)
            .iter()
            .any(|x| x.action == hft_exec::decision::action::STATE && x.state_to == 1),
        "ExecDecision{{State → Idle}}"
    );
    let lv = h.s.engine().oms.legs(AC2, G2).view;
    assert_eq!((lv.leg[0].net, lv.leg[1].net), (q("0.13"), q("-0.13")));
}

/// LT-1.1–1.3: SetParams adds g5 on X (lots 0.003 / 0.002, lcm 0.006, perp mark 60 000 → 360
/// USDT): `LotMismatch` at `force_slice.max_chunk_notional` 300 and 359.999999, `Applied` at 360.
#[test]
fn lt_1_1_set_params_lcm_vs_force_chunk() {
    fn c300(s: &mut hft_sequencer::EngineSetup) {
        s.exec.default.force_slice.max_chunk_notional = n("300");
    }
    fn c360(s: &mut hft_sequencer::EngineSetup) {
        s.exec.default.force_slice.max_chunk_notional = n("360");
    }
    fn c359(s: &mut hft_sequencer::EngineSetup) {
        s.exec.default.force_slice.max_chunk_notional = n("359.999999");
    }
    let rows: [(fn(&mut hft_sequencer::EngineSetup), bool); 3] = [(c300, false), (c360, true), (c359, false)];
    for (edit, ok) in rows {
        reset_script();
        let mut b = b0();
        for id in 23..=26u32 {
            let i = InstId(id);
            let s = match id {
                25 => inst_spec(i, A, "XUSDT", false, ETH, ("0.01", 1), ("0.003", 300_000)),
                26 => inst_spec(i, B, "X_USDT", true, ETH, ("0.01", 1), ("0.002", 200_000)),
                _ => inst_spec(
                    i,
                    A,
                    &format!("F{id}USDT"),
                    false,
                    FILL,
                    ("0.01", 1),
                    ("0.001", 100_000),
                ),
            };
            b.rd.insts.push(s);
        }
        b.setup_edit.push(edit);
        let mut h = b.boot();
        h.steady();
        let mut hd = <hft_event::MarketHdr as zerocopy::FromZeros>::new_zeroed();
        hd.venue = B;
        hd.inst = InstId(26);
        hd.rx_tsc = h.now();
        h.push(
            RingId::Md(1),
            &hft_event::MarkPrice {
                h: hd,
                px: px("60000"),
            },
        );
        h.run();
        let mut p = Spb::s1(2);
        p.groups = vec![
            gspec(G1),
            group(
                hft_types::GroupId(0x0100_0005),
                ETH,
                &[leg(InstId(25), A), leg(InstId(26), B)],
            ),
        ];
        let id = h.set_params_as(&p, "pipeline", ADMIN, true, |_| {});
        h.run();
        let want = if ok {
            (hft_sequencer::codes::status::APPLIED, 0)
        } else {
            (
                hft_sequencer::codes::status::INVALID_ARGS,
                u32::from(hft_sequencer::codes::detail::LOT_MISMATCH),
            )
        };
        assert_eq!(h.res(id), want);
        let v = h.s.instances().get(S1).unwrap().params_version();
        assert_eq!(v, if ok { 2 } else { 1 }, "old parameters stay on rejection");
    }
}
