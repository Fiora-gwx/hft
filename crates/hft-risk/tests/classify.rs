//! Classification table, SecondRetry, Amend and global properties (RSK-010–RSK-013).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::type_complexity,
    clippy::too_many_arguments
)] // allow-list: test code, not a hot path

#[path = "model/classify_ref.rs"]
mod classify_ref;
use crate::common;

use classify_ref::{Kind, draft_parts, reference};
use common::*;
use hft_event::OrderFlags;
use hft_risk::{Classification, IntentDraft, Origin, PosFacts, PosView, Risk, classify_amend};
use hft_strategy_sdk::{IntentClass, LinkRole};
use hft_types::{InstId, Qty, Side, Tif};
use proptest::prelude::*;

fn caps_of(i: InstId) -> &'static hft_refdata::VenueCaps {
    let rd = r0_refdata();
    rd.caps(rd.inst(i).venue)
}

fn draft(
    inst: InstId,
    side: Side,
    qty: Qty,
    role: Option<LinkRole>,
    sim: bool,
    hro: bool,
    class: IntentClass,
    flags: u16,
) -> IntentDraft {
    IntentDraft {
        account: acc(1),
        inst,
        side,
        qty,
        tif: Tif::Ioc,
        class,
        link_role: role,
        simultaneous: sim,
        hedge_reduce_only: hro,
        flags,
    }
}

fn pos(p: &str, pend_buy: &str, pend_sell: &str) -> PosFacts {
    PosFacts {
        pos: q(p),
        pending_buy: q(pend_buy),
        pending_sell: q(pend_sell),
        ..PosFacts::default()
    }
}

fn cls(class: IntentClass, rr: bool, ro: bool) -> Classification {
    Classification {
        class,
        risk_reducing: rr,
        reduce_only: ro,
    }
}

fn check_unlinked_strategy_hedge_reduces_group() {
    for (net, side, pending, qty, local, want, ro) in [
        (10, Side::Sell, 0, 10, 0, true, false),
        (-10, Side::Buy, 0, 10, 0, true, false),
        (10, Side::Sell, 4, 6, 10, true, true),
        (-10, Side::Buy, 4, 6, -10, true, true),
        (10, Side::Sell, 4, 7, 10, false, false),
        (10, Side::Sell, 0, 11, 10, false, false),
        (10, Side::Buy, 0, 1, 0, false, false),
        (0, Side::Buy, 0, 1, -10, false, false),
        (10, Side::Sell, 10, 1, 0, false, false),
    ] {
        let p = PosFacts {
            pos: Qty(local),
            group_net: net,
            group_pending_buy: pending,
            group_pending_sell: pending,
            ..PosFacts::default()
        };
        let d = draft(B1, side, Qty(qty), None, false, false, IntentClass::Hedge, 0);
        assert_eq!(
            Risk::classify(&d, Origin::Strategy, &p, caps_of(B1)),
            cls(
                if want {
                    IntentClass::Hedge
                } else {
                    IntentClass::Open
                },
                want,
                ro
            ),
            "net={net} pending={pending} side={side:?} qty={qty}"
        );
        assert_eq!(
            Risk::classify(&d, Origin::Exec, &p, caps_of(B1)),
            cls(IntentClass::Open, false, false)
        );
    }
    let p = PosFacts {
        group_net: i128::MIN,
        ..PosFacts::default()
    };
    assert_eq!(p.hedge_cap_qty(Side::Buy), Qty(i64::MAX));
}

#[test]
fn rsk_010_classification_table() {
    check_unlinked_strategy_hedge_reduces_group();
    use IntentClass::*;
    let ca = caps_of(A1);
    let cb = caps_of(B1);
    let flat = pos("0", "0", "0");
    let k = |d: &IntentDraft, o: Origin, p: &PosFacts, c: &hft_refdata::VenueCaps| Risk::classify(d, o, p, c);
    let one = q("0.01");
    let role = |r: LinkRole, hro: bool, i: InstId| {
        draft(
            i,
            Side::Buy,
            one,
            Some(r),
            false,
            hro,
            Hedge,
            OrderFlags::LINKED.0,
        )
    };
    assert_eq!(
        k(&role(LinkRole::Second, false, B1), Origin::Oms, &flat, cb),
        cls(Hedge, true, false),
        "RSK-010.1"
    );
    assert_eq!(
        k(&role(LinkRole::SecondRetry, false, B1), Origin::Oms, &flat, cb),
        cls(Hedge, true, false),
        "RSK-010.2"
    );
    assert_eq!(
        k(&role(LinkRole::Second, true, B1), Origin::Oms, &flat, cb),
        cls(Hedge, true, true),
        "RSK-010.3"
    );
    assert_eq!(
        k(&role(LinkRole::Second, true, A1), Origin::Oms, &flat, ca),
        cls(Hedge, true, false),
        "RSK-010.4"
    );
    assert_eq!(
        k(&role(LinkRole::SecondRetry, true, B1), Origin::Oms, &flat, cb),
        cls(Hedge, true, true),
        "RSK-010.5"
    );
    let sim = draft(
        B1,
        Side::Buy,
        one,
        Some(LinkRole::Second),
        true,
        false,
        Hedge,
        OrderFlags::LINKED.0,
    );
    assert_eq!(
        k(&sim, Origin::Oms, &flat, cb),
        cls(Open, false, false),
        "RSK-010.6"
    );
    let reducing = IntentDraft {
        hedge_reduce_only: true,
        ..sim
    };
    assert_eq!(
        k(&reducing, Origin::Oms, &pos("-0.02", "0", "0"), cb),
        cls(Reduce, true, true)
    );
    assert_eq!(
        k(&reducing, Origin::Oms, &pos("-0.02", "0", "0"), ca),
        cls(Reduce, true, false)
    );
    assert_eq!(k(&reducing, Origin::Oms, &flat, cb), cls(Reduce, false, true));
    assert_eq!(
        k(&role(LinkRole::Reverse, false, B1), Origin::Oms, &flat, cb),
        cls(Hedge, true, true),
        "RSK-010.7 B"
    );
    assert_eq!(
        k(&role(LinkRole::Reverse, false, A1), Origin::Oms, &flat, ca),
        cls(Hedge, true, false),
        "RSK-010.7 A"
    );
    // .8: five HedgeFix producers share the same draft shape (role HedgeFix, origin Exec).
    for (sub, hint) in [Hedge, Hedge, Reduce, Flatten, Hedge].into_iter().enumerate() {
        let d = draft(
            B1,
            Side::Buy,
            one,
            Some(LinkRole::HedgeFix),
            false,
            false,
            hint,
            OrderFlags::HEDGE_FIX.0,
        );
        assert_eq!(
            k(&d, Origin::Exec, &flat, cb),
            cls(Hedge, true, true),
            "RSK-010.8 sub {sub}"
        );
    }
    let fl = |i: InstId| draft(i, Side::Buy, one, None, false, false, Flatten, 0);
    assert_eq!(
        k(&fl(B1), Origin::Strategy, &flat, cb),
        cls(Flatten, true, true),
        "RSK-010.9 B"
    );
    assert_eq!(
        k(&fl(A1), Origin::Strategy, &flat, ca),
        cls(Flatten, true, false),
        "RSK-010.9 A"
    );
    for sub in 0..3 {
        assert_eq!(
            k(&fl(B1), Origin::Exec, &flat, cb),
            cls(Flatten, true, true),
            "RSK-010.10 sub {sub}"
        );
    }
    let short = pos("-0.5", "0.1", "0");
    let red = |i: InstId, s: Side, x: &str| draft(i, s, q(x), None, false, false, Reduce, 0);
    assert_eq!(
        k(&red(B1, Side::Buy, "0.3"), Origin::Strategy, &short, cb),
        cls(Reduce, true, true),
        "RSK-010.11"
    );
    assert_eq!(
        k(&red(B1, Side::Buy, "0.4"), Origin::Strategy, &short, cb),
        cls(Reduce, true, true),
        "RSK-010.12"
    );
    assert_eq!(
        k(&red(B1, Side::Buy, "0.401"), Origin::Strategy, &short, cb),
        cls(Reduce, false, true),
        "RSK-010.13"
    );
    assert_eq!(
        k(&red(B1, Side::Buy, "0.4"), Origin::Exec, &short, cb),
        cls(Reduce, true, true),
        "RSK-010.14"
    );
    let inv = pos("0.5", "0", "0");
    assert_eq!(
        k(&red(A1, Side::Sell, "0.5"), Origin::Strategy, &inv, ca),
        cls(Reduce, true, false),
        "RSK-010.15"
    );
    assert_eq!(
        k(&red(A1, Side::Sell, "0.5001"), Origin::Strategy, &inv, ca),
        cls(Reduce, false, false),
        "RSK-010.16"
    );
    let smoke = draft(
        B1,
        Side::Buy,
        one,
        None,
        false,
        false,
        Open,
        OrderFlags::SMOKE.0 | OrderFlags::POST_ONLY.0,
    );
    assert_eq!(
        k(&smoke, Origin::Command, &flat, cb),
        cls(Open, false, false),
        "RSK-010.17"
    );
    let op = draft(B1, Side::Buy, one, None, false, false, Open, 0);
    assert_eq!(
        k(&op, Origin::Strategy, &flat, cb),
        cls(Open, false, false),
        "RSK-010.18"
    );
    assert_eq!(
        k(&op, Origin::Exec, &flat, cb),
        cls(Open, false, false),
        "RSK-010.19"
    );
    let hint = draft(
        B1,
        Side::Buy,
        one,
        Some(LinkRole::HedgeFix),
        false,
        false,
        Open,
        OrderFlags::HEDGE_FIX.0,
    );
    assert_eq!(
        k(&hint, Origin::Exec, &flat, cb),
        cls(Hedge, true, true),
        "RSK-010.20"
    );
}

fn flag_subset() -> impl Strategy<Value = u16> {
    any::<u16>().prop_map(|f| f & !OrderFlags::SMOKE.0)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]
    #[test]
    fn rsk_011_second_retry_equals_second(
        hro in any::<bool>(),
        buy in any::<bool>(),
        lots in 1i64..=10_000,
        on_b in any::<bool>(),
        flags in flag_subset(),
        pos_btc in -10i64..=10,
    ) {
        let i = if on_b { B1 } else { A1 };
        let c = caps_of(i);
        let side = if buy { Side::Buy } else { Side::Sell };
        let lot = if on_b { 100_000 } else { 10_000 };
        let p = PosFacts { pos: Qty(pos_btc * 100_000_000), pending_buy: Qty(0), pending_sell: Qty(0), ..PosFacts::default() };
        let mk = |r: LinkRole| draft(i, side, Qty(lots * lot), Some(r), false, hro, IntentClass::Hedge, flags);
        let a = Risk::classify(&mk(LinkRole::SecondRetry), Origin::Oms, &p, c);
        let b = Risk::classify(&mk(LinkRole::Second), Origin::Oms, &p, c);
        prop_assert_eq!(a, b);
    }
}

#[test]
fn rsk_012_amend_classification() {
    use IntentClass::*;
    let o = q("0.3");
    let rows = [
        ("RSK-012.1", cls(Hedge, true, true), "0.3", cls(Hedge, true, true)),
        ("RSK-012.2", cls(Hedge, true, true), "0.2", cls(Hedge, true, true)),
        (
            "RSK-012.3",
            cls(Hedge, true, true),
            "0.301",
            cls(Hedge, false, true),
        ),
        (
            "RSK-012.4",
            cls(Open, false, false),
            "0.2",
            cls(Open, false, false),
        ),
        (
            "RSK-012.5",
            cls(Reduce, true, true),
            "0.3",
            cls(Reduce, true, true),
        ),
    ];
    for (id, orig, nq, want) in rows {
        assert_eq!(classify_amend(orig, o, q(nq)), want, "{id}");
    }
}

fn kind_strategy() -> impl Strategy<Value = (Kind, Origin)> {
    prop_oneof![
        (any::<bool>(), any::<bool>())
            .prop_map(|(retry, reduce_only)| (Kind::Second { retry, reduce_only }, Origin::Oms)),
        Just((Kind::SimultaneousSecond, Origin::Oms)),
        Just((Kind::Reverse, Origin::Oms)),
        Just((Kind::HedgeFix, Origin::Exec)),
        prop_oneof![Just(Origin::Strategy), Just(Origin::Exec)].prop_map(|o| (Kind::Flatten, o)),
        prop_oneof![Just(Origin::Strategy), Just(Origin::Exec)].prop_map(|o| (Kind::Reduce, o)),
        Just((Kind::Smoke, Origin::Command)),
        prop_oneof![Just(Origin::Strategy), Just(Origin::Exec)].prop_map(|o| (Kind::Open, o)),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(5_000))]
    #[test]
    fn rsk_013_classification_properties(
        (kind, origin) in kind_strategy(),
        on_b in any::<bool>(),
        buy in any::<bool>(),
        lots in 1i64..=10_000,
        pos_btc in -10i64..=10,
        pend_btc in 0i64..=10,
    ) {
        let i = if on_b { B1 } else { A1 };
        let c = caps_of(i);
        let lot = if on_b { 100_000 } else { 10_000 };
        let side = if buy { Side::Buy } else { Side::Sell };
        let qty = lots * lot;
        let pos_raw = pos_btc * 100_000_000;
        let pend = pend_btc * 100_000_000;
        let p = PosFacts {
            pos: Qty(pos_raw),
            pending_buy: Qty(if buy { pend } else { 0 }),
            pending_sell: Qty(if buy { 0 } else { pend }),
            ..PosFacts::default()
        };
        let (role, sim, hro, hint) = draft_parts(kind);
        let mut flags = if kind == Kind::Smoke { OrderFlags::SMOKE.0 } else { 0 };
        if role.is_some() {
            flags |= OrderFlags::LINKED.0;
        }
        let d = draft(i, side, Qty(qty), role, sim, hro, hint, flags);
        let got = Risk::classify(&d, origin, &p, c);
        let again = Risk::classify(&d, origin, &p, c);
        prop_assert_eq!(got, again);
        let (rc, rrr, rro) = reference(kind, c.reduce_only, qty, pos_raw, pend);
        prop_assert_eq!((got.class, got.risk_reducing, got.reduce_only), (rc, rrr, rro));
        if got.class == IntentClass::Open {
            prop_assert!(!got.risk_reducing);
        }
        if matches!(got.class, IntentClass::Hedge | IntentClass::Flatten) {
            prop_assert!(got.risk_reducing);
        }
        if got.reduce_only {
            prop_assert!(c.reduce_only);
        }
        if got.class == IntentClass::Reduce {
            prop_assert_eq!(got.risk_reducing, i128::from(qty) <= i128::from(pos_raw).abs() - i128::from(pend));
        }
    }
}
