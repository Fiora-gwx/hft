//! Linked legs and order ageing (SDK-154–SDK-157).

use std::cell::Cell;
use std::rc::Rc;

use hft_event::{OrderDecision, OrderFlags};
use hft_strategy_sdk::{
    EventRef, HedgeKind, HedgePlan, IntentClass, IntentHandle, IntentOutcome, LegLink, LegLinkOpt, LinkRole,
    OnFail, OrderIntent, OrderUpdate, OrderUpdateKind, PxMode, ResidualPolicy,
};
use hft_types::{Px, Qty, Rate, Side, Tif};

use super::views::oref;
use super::*;
use crate::common::*;

/// The spec's hedge plan: second leg b1 (`side`), IOC, ratio 1:1.
fn plan(kind: HedgeKind, on_fail: OnFail, side: Side, reduce_only: u8) -> HedgePlan {
    HedgePlan {
        kind,
        px_mode: PxMode::Cross,
        on_fail,
        tif: Tif::Ioc,
        venue: B,
        account: AC1,
        side,
        reduce_only,
        inst: B1,
        ratio_num: 1,
        ratio_den: 1,
        residual: ResidualPolicy::Defer,
        _pad: [0; 3],
        px_param: Rate(1_000_000),
        max_slip: Rate(2_000_000),
        limit_px: Px(0),
    }
}

/// A first leg on a1 in GT1 at the touch with `hedge`.
fn first_leg(hx: &Hx<'_>, side: Side, class: IntentClass, qty: &str, hedge: HedgePlan) -> OrderIntent {
    let p = touch(hx, A1, side);
    let mut it = order_in(hx, GT1, A1, side, Tif::Ioc, class, p, q(qty));
    it.link = LegLinkOpt::some(LegLink {
        group: GT1,
        role: LinkRole::First,
        _pad: [0; 3],
        tolerance: Rate(10_000_000),
        hedge,
    });
    it
}

/// s1 places `make(hx)` on the next a1 Bbo with ask `ask`; returns the handle and the decisions
/// of that Input (journal order).
fn place_on(
    h: &mut H,
    ask: &str,
    make: impl Fn(&Hx<'_>) -> OrderIntent + 'static,
) -> (IntentHandle, Vec<OrderDecision>) {
    let a = px(ask);
    let hd: Rc<Cell<Option<IntentHandle>>> = Rc::new(Cell::new(None));
    let hd2 = hd.clone();
    set_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
            && b.ask_px == a
        {
            hd2.set(ctx.place(make(hx)).ok());
        }
    });
    let m = h.mark();
    h.quote(A1, "50000", ask);
    h.run();
    clear_hook();
    (hd.get().expect("placed"), decs(&h.since(m)))
}

/// Captures s1's `Order` deliveries.
fn order_capture() -> Cap<(i64, OrderUpdate)> {
    let c: Cap<(i64, OrderUpdate)> = cap();
    let c2 = c.clone();
    add_hook(move |hx, ev, _| {
        if let EventRef::Order { u, .. } = ev
            && hx.s.strategy == S1
        {
            c2.borrow_mut().push((hx.v.now().engine_ns, **u));
        }
    });
    c
}

#[test]
fn sdk_154_second_leg_by_oms() {
    let mut h = rig();
    let orders = order_capture();
    let (_, ds) = place_on(&mut h, "50001", |hx| {
        first_leg(
            hx,
            Side::Buy,
            IntentClass::Open,
            "0.02",
            plan(HedgeKind::OnFill, OnFail::Retry, Side::Sell, 0),
        )
    });
    let d1 = *ds
        .iter()
        .find(|d| d.strategy == S1 && d.inst == A1)
        .expect("first leg");
    assert_eq!(d1.reject, 0, "{d1:?}");
    assert_eq!(d1.link_role, 1);
    h.ack(&d1);
    let mut seconds = Vec::new();
    for cum in ["0.01", "0.02"] {
        let m = h.mark();
        h.fill(&d1, q("0.01"), q(cum), d1.px);
        let s: Vec<_> = decs(&h.since(m))
            .into_iter()
            .filter(|d| d.link_role == 2)
            .collect();
        assert_eq!(s.len(), 1, "one second leg per fill increment: {s:?}");
        seconds.push(s[0]);
    }
    for s in &seconds {
        assert_eq!(
            (s.side, s.inst, s.qty, s.tif),
            (Side::Sell, B1, Qty(1_000_000), Tif::Ioc),
            "{s:?}"
        );
        assert_eq!(s.target_slot, d1.slot);
        assert_eq!(s.oflags & OrderFlags::REDUCE_ONLY.0, 0);
        assert_eq!(s.reject, 0, "{s:?}");
    }
    // Only the first leg reaches the strategy as `Order`.
    let o = orders.borrow();
    assert!(!o.is_empty());
    let r1 = oref(&d1);
    assert!(o.iter().all(|(_, u)| u.r == r1), "{o:?}");
}

#[test]
fn sdk_155_close_link_second_reduce_only() {
    // The two legs are opened one after the other: the exposure gate is off in between.
    let mut h = super::views::rig_risk(super::views::no_naked_gate);
    open_in(&mut h, S1, GT1, A1, Side::Buy, "0.01");
    open_in(&mut h, S1, GT1, B1, Side::Sell, "0.01");
    let (_, ds) = place_on(&mut h, "50002", |hx| {
        first_leg(
            hx,
            Side::Sell,
            IntentClass::Reduce,
            "0.01",
            plan(HedgeKind::OnFill, OnFail::Retry, Side::Buy, 1),
        )
    });
    let d1 = *ds
        .iter()
        .find(|d| d.strategy == S1 && d.inst == A1)
        .expect("first leg");
    assert_eq!(d1.reject, 0, "{d1:?}");
    h.ack(&d1);
    let m = h.mark();
    h.fill(&d1, d1.qty, d1.qty, d1.px);
    let s: Vec<_> = decs(&h.since(m))
        .into_iter()
        .filter(|d| d.link_role == 2)
        .collect();
    assert_eq!(s.len(), 1, "{s:?}");
    assert_eq!((s[0].side, s[0].qty), (Side::Buy, Qty(1_000_000)));
    assert_ne!(s[0].oflags & OrderFlags::REDUCE_ONLY.0, 0, "{:?}", s[0]);
}

#[test]
fn sdk_156_simultaneous_same_event() {
    for (row, on_fail) in [("SDK-156.1", OnFail::Reverse), ("SDK-156.2", OnFail::Hold)] {
        let mut h = rig();
        let (hd, ds) = place_on(&mut h, "50003", move |hx| {
            first_leg(
                hx,
                Side::Buy,
                IntentClass::Open,
                "0.01",
                plan(HedgeKind::Simultaneous, on_fail, Side::Sell, 0),
            )
        });
        let ix = ds
            .iter()
            .position(|d| d.strategy == S1 && d.link_role == 1)
            .unwrap_or_else(|| panic!("{row}: first leg {ds:?}"));
        let first = ds[ix];
        assert_eq!(first.reject, 0, "{row} {first:?}");
        // OMS output drain order (hft-sequencer "process" 3): `hedges` before `decisions`, so the
        // second leg's decision is appended right before the first leg's.
        assert!(ix >= 1, "{row}: {ds:?}");
        let second = ds[ix - 1];
        assert_eq!(
            (second.link_role, second.inst, second.qty),
            (2, B1, Qty(1_000_000)),
            "{row} {second:?}"
        );
        assert_eq!(second.trigger_seq, first.trigger_seq, "{row}");
        assert_eq!(second.reject, 0, "{row} {second:?}");
        // The outcome, read in the next event.
        let oc: Cap<Option<IntentOutcome>> = cap();
        let oc2 = oc.clone();
        set_hook(move |hx, ev, ctx| {
            if let EventRef::Bbo { inst, .. } = ev
                && *inst == A1
                && hx.s.strategy == S1
            {
                oc2.borrow_mut().push(ctx.outcome(hd));
            }
        });
        h.quote(A1, "50000", "50001");
        h.run();
        clear_hook();
        let o = oc.borrow().first().copied().flatten();
        assert_eq!(o, Some(IntentOutcome::Placed(oref(&first))), "{row}");
    }
}

#[test]
fn sdk_157_max_rest_aged() {
    let mut h = rig();
    let orders = order_capture();
    let t0 = h.now();
    let (_, ds) = place_on(&mut h, "50004", |hx| {
        let mut it = order_in(
            hx,
            GT1,
            A1,
            Side::Buy,
            Tif::Gtc,
            IntentClass::Open,
            px("49900"),
            q("0.001"),
        );
        it.max_rest_ms = 3_000;
        it
    });
    let d = *ds.iter().find(|d| d.strategy == S1 && d.inst == A1).expect("gtc");
    assert_eq!(d.reject, 0, "{d:?}");
    h.ack(&d);
    for k in 1..=60u64 {
        h.at(t0 + k * 100 * MS);
        h.run();
    }
    let o = orders.borrow();
    // The GTC was placed and acked at t0 (no time passes before the loop).
    let e0 = o
        .iter()
        .find(|(_, u)| u.kind == OrderUpdateKind::Acked)
        .map(|x| x.0)
        .expect("Acked");
    let aged: Vec<_> = o
        .iter()
        .filter(|(_, u)| u.kind == OrderUpdateKind::Aged)
        .collect();
    assert_eq!(aged.len(), 1, "{o:?}");
    let at = aged[0].0 - e0;
    assert!(
        (3_000 * MS as i64..=4_000 * MS as i64).contains(&at),
        "aged at +{} ms",
        at / MS as i64
    );
    let before: Vec<_> = o
        .iter()
        .take_while(|(_, u)| u.kind != OrderUpdateKind::Aged)
        .collect();
    assert!(
        before.iter().all(|(_, u)| u.kind == OrderUpdateKind::Acked),
        "{before:?}"
    );
}
