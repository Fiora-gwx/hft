#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
mod common;

use common::*;
use hft_event::{NoteLevel, OrderAction, OrderEventKind};
use hft_oms::notes;
use hft_oms::*;
use hft_strategy_sdk::OrderUpdateKind;
use hft_types::{Tif, Tsc};

#[test]
fn oms_132_pending_timeouts() {
    use OrderState::*;
    for (row, st, tif, ack_to, now, unknown) in [
        ("OMS-132.1", PendingNew, Tif::Gtc, 2000u32, 1999u64, false),
        ("OMS-132.2", PendingNew, Tif::Gtc, 2000, 2000, false),
        ("OMS-132.3", PendingNew, Tif::Gtc, 2000, 2001, true),
        ("OMS-132.4", PendingCancel, Tif::Gtc, 2000, 2000, false),
        ("OMS-132.5", PendingCancel, Tif::Gtc, 2000, 2001, true),
        ("OMS-132.6", PendingAmend, Tif::Gtc, 2000, 2001, true),
        ("OMS-132.7", Live, Tif::Ioc, 2000, 1000, false),
        ("OMS-132.8", Live, Tif::Ioc, 2000, 1001, true),
        ("OMS-132.9", Live, Tif::Gtc, 2000, 10_000_000, false),
        ("OMS-132.10", PendingNew, Tif::Gtc, 500, 501, true),
    ] {
        let mut h = H::with(|c| c.ack_timeout_ms = PerVenue::new(ack_to));
        let mut it = buy_x();
        it.intent.tif = tif;
        let s = h.place_ok(&it, 0);
        h.o.test_slot_mut(VS, A1, s.slot, |sl, _| {
            sl.state = st;
            sl.t_state = Tsc(0);
            if st != PendingNew {
                sl.sflags.0 |= SlotFlags::ACKED;
            }
        });
        h.sweep(now);
        assert_eq!(h.state(s) == Unknown, unknown, "{row}");
        if unknown {
            assert_eq!(transitions(&h)[0].2, 16, "{row}");
        }
    }
}

#[test]
fn oms_150_unknown_alarm() {
    let p0n = |h: &H| {
        h.notes(notes::UNKNOWN)
            .iter()
            .filter(|n| n.level == NoteLevel::P0)
            .count()
    };
    let mk = || {
        let mut h = H::new();
        let s = h.place_ok(&buy_x(), 0);
        h.what(s, OrderEventKind::OutcomeUnknown, 0);
        (h, s)
    };
    let (mut h, s) = mk();
    h.sweep(30_000);
    assert_eq!(p0n(&h), 0, "OMS-150.1");
    assert!(h.flags(s) & SlotFlags::ALARMED == 0);
    let (mut h, s) = mk();
    h.sweep(30_001);
    let n = h.notes(notes::UNKNOWN);
    assert_eq!((p0n(&h), n[0].v[1]), (1, u64::from(s.slot)), "OMS-150.2");
    assert!(h.flags(s) & SlotFlags::ALARMED != 0);
    h.sweep(31_001);
    assert_eq!(p0n(&h), 0, "OMS-150.3");
    h.query(s, 1, hft_types::Qty(0), hft_types::Px(0), 40_000);
    assert!(h.flags(s) & SlotFlags::ALARMED == 0, "OMS-150.4");
    h.what(s, OrderEventKind::OutcomeUnknown, 40_000);
    h.o.test_slot_mut(VS, A1, s.slot, |sl, _| sl.sflags.0 &= !SlotFlags::VERIFY);
    // OutcomeUnknown on Live sets VERIFY only; drive it into Unknown through a timeout instead.
    h.o.test_slot_mut(VS, A1, s.slot, |sl, _| {
        sl.state = OrderState::Unknown;
        sl.prev_state = OrderState::Live;
        sl.t_state = ms(40_000);
    });
    h.sweep(70_001);
    assert_eq!(p0n(&h), 1, "OMS-150.4");
}

#[test]
fn oms_170_sweep_cursor() {
    for (row, batch, calls, want) in [
        ("OMS-170.1", 256u16, 1, 256usize),
        ("OMS-170.2", 256, 2, 512),
        ("OMS-170.3", 256, 3, 600),
        ("OMS-170.4", 1, 1, 1),
    ] {
        let mut h = H::with(|c| {
            c.slots.set(VS, A1, 1000);
            c.sweep_batch = batch;
        });
        let rs: Vec<_> = (0..600).map(|_| h.place_ok(&buy_x(), 0)).collect();
        for i in 0..calls {
            h.sweep(3000 + i);
        }
        let n = rs.iter().filter(|r| h.state(**r) == OrderState::Unknown).count();
        assert_eq!(n, want, "{row}");
        if row == "OMS-170.1" {
            assert!(
                rs[..256].iter().all(|r| h.state(*r) == OrderState::Unknown),
                "{row}"
            );
        }
        if row == "OMS-170.3" {
            assert_eq!(h.o.table_view(VS, A1).unwrap().sweep_cursor, 0, "{row}");
        }
    }
}

#[test]
fn oms_171_aged_once() {
    let mk = |max: u32, tif: Tif| {
        let mut h = H::new();
        let mut it = buy_x();
        it.intent.max_rest_ms = max;
        it.intent.tif = tif;
        let s = h.place_ok(&it, 0);
        h.ack(s, 1, 0);
        (h, s)
    };
    let aged = |h: &H| {
        h.out
            .updates
            .iter()
            .filter(|u| u.kind == OrderUpdateKind::Aged)
            .count()
    };
    let (mut h, _) = mk(10_000, Tif::Gtc);
    h.sweep(10_000);
    assert!(h.out.is_empty(), "OMS-171.1");
    let (mut h, s) = mk(10_000, Tif::Gtc);
    h.sweep(10_001);
    assert_eq!(aged(&h), 1, "OMS-171.2");
    assert!(h.flags(s) & SlotFlags::AGED != 0);
    assert!(h.out.decisions.is_empty());
    h.sweep(11_001);
    assert_eq!(aged(&h), 0, "OMS-171.3");
    // .4: re-entering Live clears AGED.
    h.o.test_slot_mut(VS, A1, s.slot, |sl, _| sl.state = OrderState::PendingAmend);
    h.what(s, OrderEventKind::AmendReject, 12_000);
    assert_eq!(h.state(s), OrderState::Live);
    h.sweep(22_001);
    assert_eq!(aged(&h), 1, "OMS-171.4");
    let (mut h, _) = mk(0, Tif::Gtc);
    h.sweep(10_000_000);
    assert_eq!(aged(&h), 0, "OMS-171.5");
    let (mut h, _) = mk(10_000, Tif::Ioc);
    h.o.test_slot_mut(VS, A1, 1, |sl, _| sl.t_state = ms(0));
    h.sweep(10_001);
    assert_eq!(aged(&h), 0, "OMS-171.6");
}

#[test]
fn oms_172_sweep_no_query() {
    let mut h = H::new();
    for _ in 0..10 {
        let s = h.place_ok(&buy_x(), 0);
        h.what(s, OrderEventKind::OutcomeUnknown, 0);
    }
    h.sweep(5000);
    assert!(h.decisions(OrderAction::Query).is_empty());
}

#[test]
fn oms_207_sweep_link_deadline() {
    let mut h = H::new();
    let (f, l) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
    h.fill(f, q("0.012"), p("60000.00"), 0);
    let req = one_hedge(&h);
    let hs = place_hedge(&mut h, &req, 0).unwrap();
    h.what(hs, OrderEventKind::OutcomeUnknown, 0);
    assert_eq!(h.link(l).hedge_open, q("0.012"));
    h.sweep(3000);
    assert_eq!(h.link(l).state, LinkState::Hedging);
    h.sweep(3001);
    assert_eq!(h.link(l).state, LinkState::Broken);
    assert_eq!(h.n_notes(notes::HEDGE_BROKEN), 1);
}
