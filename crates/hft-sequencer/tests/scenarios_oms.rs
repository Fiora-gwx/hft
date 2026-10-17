//! OMS state and report scenarios on the real engine (docs/tests/scenarios.md: B4, FP-3, FP-4,
//! FP-9, FP-10, FP-12), fixed-step sequences on the B0 rig like `scenarios.rs` (same W1
//! mapping; B0 prices and thresholds, W1 numbers recomputed from them where a case quotes one).

use hft_event::{EventKind, NoteLevel, OrderAction, OrderDecision, OrderEventKind};
use hft_oms::{LinkState, OrderState, SlotFlags, TransitionCause, notes};
use hft_sequencer::driver::live::RingId;
use hft_strategy_sdk::{EventRef, LinkRole, OrderIntent};
use hft_testkit::dst::Scenario;
use hft_types::{AccountId, InstId, Px, Qty, RejectBits, Side};

use crate::common::*;
use crate::scenarios::*;
use crate::scenarios_hedge::{case, exec_decs};

#[test]
fn seq_175_fee_residual_link_reuse_real_engine_replay() {
    reset_script();
    let mut b = b0();
    b.caps.max_timers = 4096;
    b.setup_edit.push(|s| {
        s.oms.links_per_account = 16;
        s.oms.slots = hft_oms::PerTable::new(128);
    });
    let mut h = b.boot();
    h.steady();
    let start = h.keyframe().seq;
    set_hook(|hx, ev, ctx| {
        if hx.s.strategy == S1 && matches!(ev, EventRef::Bbo { inst, .. } if *inst == A1) {
            let mut it = pair_first(hx, q("0.001"));
            it.venue = B;
            it.inst = B1;
            it.side = Side::Sell;
            it.px = hx.v.bbo(B1).bid_px;
            let mut link = *it.link.get().unwrap();
            link.hedge = onfill(AC1, A1, Side::Buy);
            it.link = hft_strategy_sdk::LegLinkOpt::some(link);
            let _ = ctx.place(it);
        }
    });
    for cycle in 0..48 {
        let m = h.mark();
        h.quote(A1, "50000", "50003");
        h.run();
        let first = decs(&h, m).into_iter().find(|d| d.inst == B1).unwrap();
        assert_eq!(first.reject, 0, "cycle {cycle}: {first:?}");
        h.ack(&first);
        h.fill(&first, first.qty, first.qty, first.px);
        let second = decs(&h, m).into_iter().find(|d| d.inst == A1).unwrap();
        assert_eq!(second.reject, 0);
        h.ack(&second);
        let mut ev = order_event(&second, OrderEventKind::Fill, h.epoch());
        ev.cum_filled = second.qty;
        ev.avg_px = second.px;
        ev.fee_asset = BTC;
        ev.fee = Qty(10);
        h.push(RingId::Rep(0), &ev);
        h.run();
        let lv = &h.s.engine().oms.legs(AC1, G1).view;
        assert_eq!(lv.open_links, 0, "cycle {cycle}");
        assert_eq!(lv.naked_qty, Qty((cycle + 1) * 10));
        assert_eq!(lv.leg[0].residual, Qty((cycle + 1) * 10));
        assert!(notes_of(&h, m, notes::HEDGE_RESIDUAL).len() == 1);
        let t = h.now() + 100 * MS;
        keep_alive_to(&mut h, t);
    }
    assert!(h.verify_from(Some(start)).is_empty());
    clear_hook();
}

fn slot_of(h: &H, d: &OrderDecision) -> hft_oms::OrderSlot {
    let r = hft_strategy_sdk::OrderRef::new(d.venue, d.account, d.slot.0, d.r#gen);
    *h.s.engine().oms.slot(r).expect("slot")
}

/// Transition notes of `d`'s slot since `m`: `(from, to, cause)`.
fn transitions(h: &H, m: usize, d: &OrderDecision) -> Vec<(u8, u8, u8)> {
    let code = notes::code_of(notes::TRANSITION);
    h.notes_since(m)
        .into_iter()
        .filter(|n| n.code == code && n.v[1] == u64::from(d.slot.0) && n.v[2] == u64::from(d.r#gen))
        .map(|n| {
            (
                ((n.v[0] >> 16) & 0xFF) as u8,
                ((n.v[0] >> 24) & 0xFF) as u8,
                ((n.v[0] >> 32) & 0xFF) as u8,
            )
        })
        .collect()
}

fn st(s: OrderState) -> u8 {
    s as u8
}

/// A report of `what` for `d` with `cum_filled`, at `d.px`.
fn report(h: &mut H, d: &OrderDecision, what: OrderEventKind, cum: Qty, fee: Qty, asset: hft_types::AssetId) {
    let mut ev = order_event(d, what, h.epoch());
    ev.cum_filled = cum;
    ev.qty = cum;
    ev.px = d.px;
    ev.avg_px = d.px;
    ev.fee = fee;
    ev.fee_asset = asset;
    let r = h.rep_ring(d.account);
    h.push(r, &ev);
    h.run();
}

fn query_result(h: &mut H, d: &OrderDecision, status: u8, cum: Qty, avg: Px) {
    let mut ev = order_event(d, OrderEventKind::QueryResult, h.epoch());
    ev.detail = status;
    ev.cum_filled = cum;
    ev.avg_px = avg;
    let r = h.rep_ring(d.account);
    h.push(r, &ev);
    h.run();
}

/// The FP-3 link: a1 buy 0.01 IOC with `HedgePlan{Cross, px_param 0.0005, max_slip 0.0005}`.
fn fp3_link(hx: &Hx<'_>) -> OrderIntent {
    let mut it = pair_first(hx, q("0.01"));
    if let Some(l0) = it.link.get() {
        let mut l = *l0;
        l.hedge.px_param = r("0.0005");
        l.hedge.max_slip = r("0.0005");
        it.link = hft_strategy_sdk::LegLinkOpt::some(l);
    }
    it
}

fn fp3_open(h: &mut H) -> OrderDecision {
    h.quote(B1, "50010", "50010.1");
    h.run();
    let m = place_on_a1(h, &[S1], fp3_link);
    let d = decs(h, m).into_iter().find(|d| d.inst == A1).expect("first leg");
    assert_eq!(d.reject, 0);
    d
}

fn usdt_free(h: &H) -> i64 {
    h.s.engine().ledger.cash_view(AC1, A, USDT).free.0
}

fn second_px(h: &H) -> Px {
    let e = h.s.engine();
    let lim = e.risk.hedge_px_limits(AC1, B, B1);
    hft_types::cap_cross_px(
        Side::Sell,
        px("50010"),
        Px::mid_floor(px("50010"), px("50010.1")),
        r("0.0005"),
        &lim,
        h.rd.inst(B1).tick,
    )
}

// ------------------------------------------------------------------------------------- FP-3

#[test]
fn fp_3_fill_before_ack_goes_filled() {
    case(Scenario::new(
        "FP-3",
        "fill before Ack goes Filled, the late Ack is a duplicate",
    ));
    let mut h = steady();
    let first = fp3_open(&mut h);
    let cash0 = usdt_free(&h);
    let m = h.mark();
    report(&mut h, &first, OrderEventKind::Fill, first.qty, q("0.6"), USDT);
    let sl = slot_of(&h, &first);
    assert_eq!(sl.state, OrderState::Filled);
    assert!(sl.sflags.has(SlotFlags::FILL_BEFORE_ACK) && sl.sflags.has(SlotFlags::ACKED));
    let tr: Vec<_> = transitions(&h, m, &first)
        .into_iter()
        .filter(|t| t.2 != TransitionCause::Place as u8)
        .collect();
    assert_eq!(
        tr,
        vec![(
            st(OrderState::PendingNew),
            st(OrderState::Filled),
            TransitionCause::Fill as u8
        )]
    );
    assert_eq!(h.s.engine().ledger.pos(AC1, A1).qty, q("0.01"));
    // 0.01 × price (USDT at qty exponent −8) + 0.6 USDT fee.
    let cost = (i128::from(q("0.01").0) * i128::from(first.px.0) / 100) as i64;
    assert_eq!(
        cash0 - usdt_free(&h),
        cost + q("0.6").0,
        "quantity × price + 0.6 USDT"
    );
    let second = decs(&h, m)
        .into_iter()
        .find(|d| d.inst == B1)
        .expect("second leg");
    assert_eq!(
        (second.side, second.qty, second.px, second.tif, second.link_role),
        (
            Side::Sell,
            q("0.01"),
            second_px(&h),
            hft_types::Tif::Ioc,
            LinkRole::Second as u8
        )
    );
    let lat = h
        .since(m)
        .iter()
        .filter(|r| r.kind == EventKind::LATENCY_SAMPLE)
        .count();
    assert_eq!(lat, 1, "one LatencySample synthesised");
    // The late Ack: a duplicate only.
    let dup0 = h.s.engine().oms.stats().dup;
    let pos0 = h.s.engine().ledger.pos(AC1, A1).qty;
    let legs0 = h.s.engine().oms.legs(AC1, G1).view;
    let m = h.mark();
    h.ack(&first);
    assert!(transitions(&h, m, &first).is_empty());
    assert_eq!(h.s.engine().oms.stats().dup, dup0 + 1);
    assert!(decs(&h, m).is_empty());
    assert!(h.since(m).iter().all(|r| r.kind != EventKind::LATENCY_SAMPLE));
    assert_eq!(h.s.engine().ledger.pos(AC1, A1).qty, pos0);
    assert_eq!(h.s.engine().oms.legs(AC1, G1).view, legs0);
}

/// FP-3.1: PartialFill 0.004 → Ack → Fill 0.01; FP-3.2: the same Fill twice.
#[test]
fn fp_3_1_partial_before_ack_and_duplicate_fill() {
    let mut h = steady();
    let first = fp3_open(&mut h);
    let m = h.mark();
    report(
        &mut h,
        &first,
        OrderEventKind::PartialFill,
        q("0.004"),
        Qty(0),
        USDT,
    );
    assert_eq!(slot_of(&h, &first).state, OrderState::Live);
    assert!(slot_of(&h, &first).sflags.has(SlotFlags::FILL_BEFORE_ACK));
    let s1: Vec<_> = decs(&h, m).into_iter().filter(|d| d.inst == B1).collect();
    assert_eq!(s1.iter().map(|d| d.qty).collect::<Vec<_>>(), vec![q("0.004")]);
    let dup0 = h.s.engine().oms.stats().dup;
    h.ack(&first);
    assert_eq!(h.s.engine().oms.stats().dup, dup0 + 1);
    let m2 = h.mark();
    report(&mut h, &first, OrderEventKind::Fill, q("0.01"), Qty(0), USDT);
    assert_eq!(slot_of(&h, &first).state, OrderState::Filled);
    let s2: Vec<_> = decs(&h, m2).into_iter().filter(|d| d.inst == B1).collect();
    assert_eq!(s2.iter().map(|d| d.qty).collect::<Vec<_>>(), vec![q("0.006")]);

    let mut h = steady();
    let first = fp3_open(&mut h);
    let m = h.mark();
    report(&mut h, &first, OrderEventKind::Fill, q("0.01"), Qty(0), USDT);
    let dup0 = h.s.engine().oms.stats().dup;
    report(&mut h, &first, OrderEventKind::Fill, q("0.01"), Qty(0), USDT);
    assert_eq!(h.s.engine().oms.stats().dup, dup0 + 1);
    let seconds: Vec<_> = decs(&h, m).into_iter().filter(|d| d.inst == B1).collect();
    assert_eq!(seconds.len(), 1);
    assert_eq!(seconds[0].qty, q("0.01"));
}

// ------------------------------------------------------------------------------------- FP-4

/// s1's resting a1 PostOnly buy 0.01 (no group), acked.
fn resting_buy(h: &mut H) -> OrderDecision {
    let m = place_on_a1(h, &[S1], |hx| {
        let mut it = intent(
            hx,
            A1,
            Side::Buy,
            hft_types::Tif::PostOnly,
            hft_strategy_sdk::IntentClass::Open,
            px("49999.99"),
            q("0.01"),
        );
        it.group = hft_types::GROUP_NONE;
        it
    });
    let d = decs(h, m)[0];
    assert_eq!(d.reject, 0, "{d:?}");
    h.ack(&d);
    assert_eq!(slot_of(h, &d).state, OrderState::Live);
    d
}

/// s1 cancels `d` on the next a1 Bbo.
fn cancel_it(h: &mut H, d: &OrderDecision) {
    let r = hft_strategy_sdk::OrderRef::new(d.venue, d.account, d.slot.0, d.r#gen);
    set_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            let _ = ctx.cancel(r);
        }
    });
    h.quote(A1, "50000", "50003");
    h.run();
    clear_hook();
}

fn cancels(h: &H, m: usize, d: &OrderDecision) -> usize {
    decs(h, m)
        .iter()
        .filter(|x| x.action == OrderAction::Cancel && x.target_slot == d.slot)
        .count()
}

fn cancel_reject(h: &mut H, d: &OrderDecision, gone: bool) {
    let mut ev = order_event(d, OrderEventKind::CancelReject, h.epoch());
    ev.detail = if gone { 1 } else { 2 };
    h.push(RingId::Rep(0), &ev);
    h.run();
}

#[test]
fn fp_4_cancel_reject_filled_no_retry() {
    case(Scenario::new(
        "FP-4",
        "cancel rejected because filled: book the fill, no retry",
    ));
    let mut h = steady();
    let d = resting_buy(&mut h);
    let m0 = h.mark();
    cancel_it(&mut h, &d);
    let sl = slot_of(&h, &d);
    assert_eq!(sl.state, OrderState::PendingCancel);
    assert!(sl.sflags.has(SlotFlags::CANCEL_SENT));
    let imm0 = h.s.engine().oms.inflight().pos(AC1, A1).imm_buy;
    h.adv(5 * MS);
    let m = h.mark();
    cancel_reject(&mut h, &d, true);
    let sl = slot_of(&h, &d);
    assert_eq!(sl.state, OrderState::Unknown);
    assert!(!sl.sflags.has(SlotFlags::CANCEL_SENT));
    assert_eq!(
        decs(&h, m)
            .iter()
            .filter(|x| x.action == OrderAction::Query && x.slot == d.slot)
            .count(),
        1
    );
    assert_eq!(cancels(&h, m, &d), 0);
    assert_eq!(
        h.s.engine().oms.inflight().pos(AC1, A1).imm_buy,
        Qty(imm0.0 + q("0.01").0),
        "Unknown counts as immediate"
    );
    // The delayed fill: Unknown → Filled, booked (GROUP_NONE), s1 told.
    clear_log();
    h.adv(95 * MS);
    let m = h.mark();
    report(&mut h, &d, OrderEventKind::Fill, q("0.01"), Qty(0), USDT);
    assert_eq!(slot_of(&h, &d).state, OrderState::Filled);
    assert_eq!(h.s.engine().ledger.pos(AC1, A1).qty, q("0.01"));
    assert!(
        transitions(&h, m, &d)
            .iter()
            .any(|t| t.1 == st(OrderState::Filled))
    );
    h.run();
    assert!(
        log()
            .iter()
            .any(|g| g.s == S1 && g.kind == "Order" && g.text.contains("Filled"))
    );
    // The later QueryResult{Filled}: terminal, nothing to apply (hft-oms 回报处理表: a terminal
    // slot only applies a fill increment, here none): no transition, no FillOut.
    let pos0 = h.s.engine().ledger.pos(AC1, A1).qty;
    let m = h.mark();
    query_result(&mut h, &d, 3, q("0.01"), d.px);
    assert!(transitions(&h, m, &d).is_empty());
    assert_eq!(h.s.engine().ledger.pos(AC1, A1).qty, pos0);
    assert_eq!(cancels(&h, m0, &d), 1, "one Cancel in the whole scenario");

    // FP-4.1: the fill first, then CancelReject{GONE}: PendingCancel → Filled; the reject is a
    // duplicate; no Query.
    let mut h = steady();
    let d = resting_buy(&mut h);
    let m0 = h.mark();
    cancel_it(&mut h, &d);
    report(&mut h, &d, OrderEventKind::Fill, q("0.01"), Qty(0), USDT);
    assert_eq!(slot_of(&h, &d).state, OrderState::Filled);
    let dup0 = h.s.engine().oms.stats().dup;
    let m = h.mark();
    cancel_reject(&mut h, &d, true);
    assert_eq!(h.s.engine().oms.stats().dup, dup0 + 1);
    assert!(decs(&h, m).iter().all(|x| x.action != OrderAction::Query));
    assert_eq!(cancels(&h, m0, &d), 1);

    // FP-4.2: not filled, CancelReject for another reason: PendingCancel → Live, CANCEL_SENT
    // cleared, the OMS does not cancel again.
    let mut h = steady();
    let d = resting_buy(&mut h);
    let m0 = h.mark();
    cancel_it(&mut h, &d);
    cancel_reject(&mut h, &d, false);
    let sl = slot_of(&h, &d);
    assert_eq!(sl.state, OrderState::Live);
    assert!(!sl.sflags.has(SlotFlags::CANCEL_SENT));
    let t = h.now();
    keep_alive_to(&mut h, t + 5 * SEC);
    assert_eq!(cancels(&h, m0, &d), 1);
}

// ------------------------------------------------------------------------------------- FP-9

/// `OrderEvent{OutcomeUnknown, reject_code}` for `d`.
fn outcome_unknown(h: &mut H, d: &OrderDecision, code: u32) {
    let mut ev = order_event(d, OrderEventKind::OutcomeUnknown, h.epoch());
    ev.reject_code = code;
    let r = h.rep_ring(d.account);
    h.push(r, &ev);
    h.run();
}

#[test]
fn fp_9_unregistered_code_goes_unknown() {
    case(Scenario::new(
        "FP-9",
        "unregistered exchange code goes Unknown, exposure kept",
    ));
    let mut h = steady();
    let first = fp3_open(&mut h);
    let m0 = h.mark();
    report(&mut h, &first, OrderEventKind::Fill, first.qty, Qty(0), USDT);
    let second = decs(&h, m0)
        .into_iter()
        .find(|d| d.inst == B1)
        .expect("second leg");
    let m = h.mark();
    outcome_unknown(&mut h, &second, 59_999);
    assert_eq!(slot_of(&h, &second).state, OrderState::Unknown);
    assert_eq!(
        transitions(&h, m, &second),
        vec![(
            st(OrderState::PendingNew),
            st(OrderState::Unknown),
            TransitionCause::OutcomeUnknown as u8
        )]
    );
    let un = notes_of(&h, m, notes::UNKNOWN);
    assert_eq!((un.len(), un[0].level), (1, NoteLevel::P1));
    assert_eq!(
        decs(&h, m)
            .iter()
            .filter(|x| x.action == OrderAction::Query && x.slot == second.slot)
            .count(),
        1
    );
    assert!(
        !transitions(&h, m, &second)
            .iter()
            .any(|t| t.1 == st(OrderState::Rejected))
    );
    assert_eq!(h.s.engine().oms.inflight().pos(AC1, B1).imm_sell, q("0.01"));
    let lv = h.s.engine().oms.legs(AC1, G1).view;
    assert_eq!(lv.leg[1].inflight_immediate, q("-0.01"));
    // While Unknown: no retry, no reverse; B not ready, an Open there is VENUE_NOT_READY.
    assert!(!h.s.engine().oms.venue_ready(B, AC1));
    let m2 = place_on_a1(&mut h, &[S1], b1_sell_open);
    assert_eq!(decs(&h, m2)[0].reject, RejectBits::VENUE_NOT_READY.0);
    let t = h.now();
    keep_alive_to(&mut h, t + 400 * MS);
    assert!(
        decs(&h, m)
            .iter()
            .all(|x| x.link_role != LinkRole::SecondRetry as u8 && x.link_role != LinkRole::Reverse as u8)
    );
    // The query finds it filled.
    let m3 = h.mark();
    query_result(&mut h, &second, 3, q("0.01"), px("50010"));
    assert_eq!(slot_of(&h, &second).state, OrderState::Filled);
    assert_eq!(last_link(&h, AC1).map(|l| l.state), Some(LinkState::Done));
    assert_eq!(notes_of(&h, m3, notes::VENUE_READY).len(), 1);
    assert_eq!(h.s.engine().ledger.pos(AC1, B1).qty, q("-0.01"));
}

/// FP-9.1: the query finds it rejected: only now one counted failure and the SecondRetry.
/// FP-9.2: a one-leg Open with an unregistered code keeps its in-flight until the query says
/// rejected.
#[test]
fn fp_9_1_query_rejected_then_retry() {
    let mut h = steady();
    let first = fp3_open(&mut h);
    let m0 = h.mark();
    report(&mut h, &first, OrderEventKind::Fill, first.qty, Qty(0), USDT);
    let second = decs(&h, m0)
        .into_iter()
        .find(|d| d.inst == B1)
        .expect("second leg");
    outcome_unknown(&mut h, &second, 59_999);
    assert_eq!(last_link(&h, AC1).map(|l| l.retries), Some(0));
    query_result(&mut h, &second, 5, Qty(0), Px(0));
    assert_eq!(slot_of(&h, &second).state, OrderState::Rejected);
    assert_eq!(last_link(&h, AC1).map(|l| l.retries), Some(1));
    let due = hedge_retry_timers(&h).first().map(|x| x.0).expect("HedgeRetry");
    let m = h.mark();
    keep_alive_to(&mut h, due);
    let r = decs(&h, m)
        .into_iter()
        .find(|d| d.link_role == LinkRole::SecondRetry as u8)
        .expect("SecondRetry");
    assert_eq!((r.qty, r.reject), (q("0.01"), 0));

    let mut h = steady();
    let m = place_on_a1(&mut h, &[S1], a1_buy_open);
    let d = decs(&h, m)[0];
    outcome_unknown(&mut h, &d, 59_999);
    assert_eq!(h.s.engine().oms.inflight().pos(AC1, A1).imm_buy, d.qty);
    query_result(&mut h, &d, 5, Qty(0), Px(0));
    assert_eq!(h.s.engine().oms.inflight().pos(AC1, A1).imm_buy, Qty(0));
}

// ------------------------------------------------------------------------------------- FP-10

fn query_seqs(h: &H, m: usize, d: &OrderDecision) -> Vec<u64> {
    decisions(&h.since(m))
        .into_iter()
        .filter(|x| x.1.action == OrderAction::Query && x.1.slot == d.slot)
        .map(|x| x.0)
        .collect()
}

/// Answers every Query of `d` with `SendReport{Written}` + NotFound up to `t`.
fn not_found_until(h: &mut H, d: &OrderDecision, t: u64) {
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
                    hft_event::ConnStateKind::Alive,
                    hft_event::ConnRole::Md,
                    InstId(0),
                    AccountId(0),
                    now,
                ),
            );
        }
        h.run();
        for seq in query_seqs(h, m, d) {
            let now = h.now();
            h.push(RingId::Rep(0), &send_report(d, hft_event::GwOp::Query, seq, now));
            h.run();
            query_result(h, d, 7, Qty(0), Px(0));
        }
    }
}

fn resolved(
    h: &H,
    d: &OrderDecision,
    checked: u8,
    exch: Qty,
    status: u8,
    id: u64,
) -> hft_event::ReconcileReport {
    let mut x = recon_summary(
        d.account,
        d.venue,
        id,
        h.j().records().last().map_or(0, |r| r.seq),
    );
    x.item = 4;
    x.inst = d.inst;
    x.slot = d.slot;
    x.r#gen = d.r#gen;
    x.epoch = h.epoch();
    x.order_status = status;
    x.history_checked = checked;
    x.exch_qty = exch;
    x
}

/// One-leg a1 buy 0.01 IOC whose Place hit `WriteError`; returns it and its sent time.
fn write_error_open(h: &mut H, f: fn(&Hx<'_>) -> OrderIntent) -> (OrderDecision, u64) {
    let m = place_on_a1(h, &[S1], f);
    let (seq, d) = decisions(&h.since(m))
        .into_iter()
        .find(|x| x.1.inst == A1)
        .expect("decision");
    assert_eq!(d.reject, 0);
    let t = h.now() + MS;
    h.at(t);
    let m = h.mark();
    h.push(
        RingId::Rep(0),
        &send_report_st(
            &d,
            hft_event::GwOp::Place,
            seq,
            t,
            hft_event::SendStatus::WriteError,
        ),
    );
    h.run();
    assert_eq!(slot_of(h, &d).state, OrderState::Unknown);
    assert!(
        transitions(h, m, &d)
            .iter()
            .any(|x| x.2 == TransitionCause::WriteError as u8)
    );
    (d, t)
}

fn a1_buy_001(hx: &Hx<'_>) -> OrderIntent {
    let mut it = open_buy(hx, A1);
    it.qty = q("0.01");
    it.group = hft_types::GROUP_NONE;
    it
}

fn steady_guard(g: hft_refdata::StaleGuardKind) -> H {
    reset_script();
    let mut b = b0();
    b.rd.stale_guards.push((A, g));
    let mut h = b.boot();
    h.steady();
    h
}

#[test]
fn fp_10_notfound_waits_trade_history() {
    case(Scenario::new(
        "FP-10",
        "NotFound without expiry protection waits for trade history",
    ));
    let mut h = steady_guard(hft_refdata::StaleGuardKind::None);
    clear_log();
    let (d, t_sent) = write_error_open(&mut h, a1_buy_001);
    let qd = query_seqs(&h, 0, &d);
    assert!(!qd.is_empty(), "Query with the WriteError");
    // NotFound within the 2 000 ms grace: still Unknown, no HIST_WAIT, no trades = 1 note.
    let m = h.mark();
    not_found_until(&mut h, &d, t_sent + 1_950 * MS);
    let sl = slot_of(&h, &d);
    assert_eq!(sl.state, OrderState::Unknown);
    assert!(!sl.sflags.has(SlotFlags::HIST_WAIT));
    assert!(
        notes_of(&h, m, notes::NEEDS_REST_QUERY)
            .iter()
            .all(|n| (n.v[0] >> 16) & 1 == 0)
    );
    // Past the grace: HIST_WAIT and the trades = 1 note; then the timer rewrites it, no Query.
    not_found_until(&mut h, &d, t_sent + 3_100 * MS);
    let sl = slot_of(&h, &d);
    assert!(sl.sflags.has(SlotFlags::HIST_WAIT));
    let qc = sl.query_count;
    let m = h.mark();
    not_found_until(&mut h, &d, t_sent + 6_000 * MS);
    assert!(query_seqs(&h, m, &d).is_empty());
    let nr = notes_of(&h, m, notes::NEEDS_REST_QUERY);
    assert!(!nr.is_empty() && nr.iter().all(|n| (n.v[0] >> 16) & 1 == 1));
    assert_eq!(slot_of(&h, &d).query_count, qc);
    h.push(RingId::Rep(0), &resolved(&h, &d, 0, Qty(0), 7, 88_001));
    h.run();
    assert_eq!(slot_of(&h, &d).state, OrderState::Unknown);
    not_found_until(&mut h, &d, t_sent + 9_900 * MS);
    assert_eq!(
        slot_of(&h, &d).state,
        OrderState::Unknown,
        "never terminal before 10 s"
    );
    assert_eq!(h.s.engine().oms.inflight().pos(AC1, A1).imm_buy, q("0.01"));
    let m = h.mark();
    h.push(RingId::Rep(0), &resolved(&h, &d, 1, Qty(0), 7, 88_002));
    h.run();
    let sl = slot_of(&h, &d);
    assert_eq!((sl.state, sl.reject_code), (OrderState::Rejected, 0));
    assert!(
        transitions(&h, m, &d)
            .iter()
            .any(|x| x.2 == TransitionCause::Reconcile as u8)
    );
    assert_eq!(h.s.engine().oms.inflight().pos(AC1, A1).imm_buy, Qty(0));
    h.run();
    assert!(log().iter().any(|g| g.s == S1
        && g.kind == "Order"
        && g.text.contains("Rejected")
        && g.text.contains("venue_code: 0")));
}

/// FP-10.2: `ExpireAt`, non-linked: the first NotFound past the grace rejects
/// (`NotFoundExpire`), no history request. FP-10.3: the same for a linked first leg follows the
/// history path instead.
#[test]
fn fp_10_2_expire_at_rejects_unlinked_only() {
    let mut h = steady_guard(hft_refdata::StaleGuardKind::ExpireAt);
    let (d, t_sent) = write_error_open(&mut h, a1_buy_001);
    let m = h.mark();
    not_found_until(&mut h, &d, t_sent + 3_100 * MS);
    let sl = slot_of(&h, &d);
    assert_eq!(sl.state, OrderState::Rejected);
    assert!(
        transitions(&h, m, &d)
            .iter()
            .any(|x| x.2 == TransitionCause::NotFoundExpire as u8)
    );
    assert!(
        notes_of(&h, m, notes::NEEDS_REST_QUERY)
            .iter()
            .all(|n| (n.v[0] >> 16) & 1 == 0)
    );

    let mut h = steady_guard(hft_refdata::StaleGuardKind::ExpireAt);
    let (d, t_sent) = write_error_open(&mut h, fp3_link);
    let m = h.mark();
    not_found_until(&mut h, &d, t_sent + 3_100 * MS);
    let sl = slot_of(&h, &d);
    assert_eq!(sl.state, OrderState::Unknown);
    assert!(sl.sflags.has(SlotFlags::HIST_WAIT));
    assert!(
        transitions(&h, m, &d)
            .iter()
            .all(|x| x.2 != TransitionCause::NotFoundExpire as u8)
    );
}

// ------------------------------------------------------------------------------------- FP-12 / B4

/// The open link of s1 with the first leg of `qty` filled at once with `fee` (asset `asset`).
fn base_fee_link(
    h: &mut H,
    qty: &'static str,
    fee: &str,
    asset: hft_types::AssetId,
) -> (OrderDecision, usize) {
    fn mk(hx: &Hx<'_>, qty: Qty) -> OrderIntent {
        pair_first(hx, qty)
    }
    let qx = q(qty);
    set_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            let _ = ctx.place(mk(hx, qx));
        }
    });
    let m = h.mark();
    h.quote(A1, "50000", "50003");
    h.run();
    clear_hook();
    let first = decs(h, m).into_iter().find(|d| d.inst == A1).expect("first leg");
    assert_eq!(first.reject, 0, "{first:?}");
    h.ack(&first);
    h.trace_on();
    let m = h.mark();
    report(h, &first, OrderEventKind::Fill, first.qty, q(fee), asset);
    (first, m)
}

fn hedge_cap_of_second(h: &H) -> Option<Qty> {
    h.s.engine()
        .trace
        .views
        .iter()
        .rev()
        .find(|v| v.inst == B1 && v.link_role == Some(LinkRole::Second))
        .map(|v| v.hedge_cap_qty)
}

#[test]
fn fp_12_base_fee_hedges_net_qty() {
    case(Scenario::new(
        "FP-12",
        "base-asset fee: hedge and close by the net",
    ));
    let mut h = steady();
    let (_, m) = base_fee_link(&mut h, "0.01", "0.00001", BTC);
    assert_eq!(h.s.engine().ledger.pos(AC1, A1).qty, q("0.00999"));
    assert_eq!(h.s.engine().ledger.inventory(AC1, A, BTC, G1), q("0.00999"));
    let lv = h.s.engine().oms.legs(AC1, G1).view;
    assert_eq!(lv.leg[0].net, q("0.00999"));
    let second = decs(&h, m)
        .into_iter()
        .find(|d| d.inst == B1)
        .expect("second leg");
    assert_eq!(second.qty, q("0.009"), "0.00999 floored to b1's lot");
    assert_eq!(lv.leg[1].residual, q("0.00099"));
    assert_eq!(
        hedge_cap_of_second(&h),
        Some(q("0.01099")),
        "basis 0.00999 + 1 lot"
    );
    h.ack(&second);
    h.fill(&second, second.qty, second.qty, second.px);
    // A second identical link absorbs the remainder: 0.00999 + 0.00099 → 0.010, 0.00098 left.
    let (_, m) = base_fee_link(&mut h, "0.01", "0.00001", BTC);
    let second = decs(&h, m)
        .into_iter()
        .find(|d| d.inst == B1)
        .expect("second leg");
    assert_eq!(second.qty, q("0.010"));
    assert_eq!(h.s.engine().oms.legs(AC1, G1).view.leg[1].residual, q("0.00098"));
}

/// FP-12.1: the fee arrives as a `FeeOnly`: the second leg goes by the gross 0.010; the FeeOnly
/// moves no slot and corrects the net (`NetAdjust −0.00001`, residual −0.00001).
#[test]
fn fp_12_1_fee_only_corrects_net() {
    let mut h = steady();
    let (first, m) = base_fee_link(&mut h, "0.01", "0", hft_types::AssetId(0));
    let second = decs(&h, m)
        .into_iter()
        .find(|d| d.inst == B1)
        .expect("second leg");
    assert_eq!(second.qty, q("0.010"));
    let m = h.mark();
    let st0 = slot_of(&h, &first).state;
    crate::scenarios_hedge::fee_only(&mut h, &first, q("0.00001"), BTC);
    assert_eq!(slot_of(&h, &first).state, st0);
    assert!(transitions(&h, m, &first).is_empty());
    let lv = h.s.engine().oms.legs(AC1, G1).view;
    assert_eq!(lv.leg[0].net, q("0.00999"));
    assert_eq!(lv.leg[1].residual, q("-0.00001"));
    assert_eq!(h.s.engine().ledger.pos(AC1, A1).qty, q("0.00999"));
}

#[test]
fn b4_base_asset_fee_hedges_net_qty() {
    case(Scenario::new(
        "B4",
        "base-asset fee: position, inventory and hedge by the net",
    ));
    let mut h = steady();
    let m0 = h.mark();
    let (_, m) = base_fee_link(&mut h, "0.05", "0.0001", BTC);
    assert_eq!(h.s.engine().ledger.pos(AC1, A1).qty, q("0.0499"));
    assert_eq!(h.s.engine().ledger.inventory(AC1, A, BTC, G1), q("0.0499"));
    assert_eq!(h.s.engine().oms.legs(AC1, G1).view.leg[0].net, q("0.0499"));
    let second = decs(&h, m)
        .into_iter()
        .find(|d| d.inst == B1)
        .expect("second leg");
    assert_eq!(second.qty, q("0.049"));
    assert_eq!(hedge_cap_of_second(&h), Some(q("0.0509")));
    assert_eq!(h.s.engine().oms.legs(AC1, G1).view.leg[1].residual, q("0.0009"));
    h.ack(&second);
    h.fill(&second, second.qty, second.qty, second.px);
    // Filled naked 0.0009 (≈ 45 USDT > dust) at ratio 0.018 < 0.02: no HedgeFix.
    let t = h.now();
    keep_alive_to(&mut h, t + 15 * SEC);
    assert!(
        exec_decs(&h, m0)
            .iter()
            .all(|x| x.action != hft_exec::decision::action::HEDGE_FIX)
    );
    assert!(
        decs(&h, m0)
            .iter()
            .all(|d| d.link_role != LinkRole::HedgeFix as u8)
    );

    // B4.1: the fills carry no fee; FeeOnly 0.0001 BTC later.
    let mut h = steady();
    let m0 = h.mark();
    let (first, m) = base_fee_link(&mut h, "0.05", "0", hft_types::AssetId(0));
    let second = decs(&h, m)
        .into_iter()
        .find(|d| d.inst == B1)
        .expect("second leg");
    assert_eq!(second.qty, q("0.05"), "by the gross");
    h.ack(&second);
    h.fill(&second, second.qty, second.qty, second.px);
    crate::scenarios_hedge::fee_only(&mut h, &first, q("0.0001"), BTC);
    assert_eq!(h.s.engine().ledger.inventory(AC1, A, BTC, G1), q("0.0499"));
    let lv = h.s.engine().oms.legs(AC1, G1).view;
    assert_eq!((lv.leg[0].net, lv.leg[1].residual), (q("0.0499"), q("-0.0001")));
    let t = h.now();
    keep_alive_to(&mut h, t + 15 * SEC);
    assert!(
        decs(&h, m0)
            .iter()
            .all(|d| d.link_role != LinkRole::HedgeFix as u8)
    );
}
