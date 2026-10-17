#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
use crate::common;

use common::r as rate;
use common::*;
use hft_oms::notes;
use hft_oms::*;
use hft_strategy_sdk::{BboView, HedgePlan, IntentClass, LinkRole, PxMode};
use hft_types::{Px, Side, StrategyId, Tif, Tsc, cap_cross_px};

const NOW: u64 = 100_000;

#[test]
fn oms_220_closing_retry_uses_live_marketable_bounded_ioc_price() {
    for side in [Side::Buy, Side::Sell] {
        let mut rq = req(side, Px(0), LinkRole::SecondRetry);
        rq.reduce_only = true;
        let mut pl = plan(PxMode::Limit, "0.001", p("100"));
        pl.kind = hft_strategy_sdk::HedgeKind::Simultaneous;
        pl.reduce_only = 1;
        let mut b = book("59990", "60010");
        let got = ph(&rq, &pl, &b, 10).unwrap();
        match side {
            Side::Buy => assert!(got.px >= b.ask_px && got.px <= got.cap),
            Side::Sell => assert!(got.px <= b.bid_px && got.px >= got.cap),
        }
        b.conn_up = false;
        assert_eq!(ph(&rq, &pl, &b, 10), Err(HedgePxErr::StaleQuote));
    }
}

fn req(side: Side, ref_px: Px, role: LinkRole) -> HedgeRequest {
    HedgeRequest {
        link_id: 1,
        role,
        venue: VX,
        account: A1,
        side,
        reduce_only: false,
        inst: Y,
        group: g1(),
        strategy: StrategyId(1),
        qty: q("0.012"),
        basis: q("0.012"),
        hedge_cap_qty: q("0.013"),
        ref_px,
        peer_venue: VS,
        peer_account: A1,
        trigger_seq: 1,
    }
}

fn plan(mode: PxMode, param: &str, limit: Px) -> HedgePlan {
    let mut p = plan_l1();
    p.px_mode = mode;
    p.px_param = rate(param);
    p.limit_px = limit;
    p
}

fn book(bid: &str, ask: &str) -> BboView {
    bbo(p(bid), p(ask), ms(NOW))
}

fn ph(rq: &HedgeRequest, pl: &HedgePlan, b: &BboView, tick: i64) -> Result<HedgePx, HedgePxErr> {
    price_hedge(rq, pl, b, ms(NOW), &lim(), tick, MS)
}

#[test]
fn oms_220_modes() {
    let b = book("59990.0", "60010.0");
    let mid = p("60000.0");
    let rows: [(&str, Side, PxMode, &str, &str, &str, &str, bool); 8] = [
        (
            "OMS-220.1",
            Side::Sell,
            PxMode::Cross,
            "0.001",
            "0",
            "60010.0",
            "59930.1",
            false,
        ),
        (
            "OMS-220.2",
            Side::Sell,
            PxMode::Mid,
            "0.001",
            "0",
            "60010.0",
            "59940.0",
            false,
        ),
        (
            "OMS-220.3",
            Side::Sell,
            PxMode::Limit,
            "0.001",
            "59950.0",
            "60010.0",
            "59950.0",
            false,
        ),
        (
            "OMS-220.4",
            Side::Sell,
            PxMode::Limit,
            "0.001",
            "59600.0",
            "60010.0",
            "59700.0",
            true,
        ),
        (
            "OMS-220.5",
            Side::Buy,
            PxMode::Cross,
            "0.001",
            "0",
            "59990.0",
            "60070.0",
            false,
        ),
        (
            "OMS-220.6",
            Side::Buy,
            PxMode::Mid,
            "0.001",
            "0",
            "59990.0",
            "60060.0",
            false,
        ),
        (
            "OMS-220.7",
            Side::Buy,
            PxMode::Limit,
            "0.001",
            "60500.0",
            "59990.0",
            "60300.0",
            true,
        ),
        (
            "OMS-220.8",
            Side::Buy,
            PxMode::Cross,
            "0.01",
            "0",
            "59990.0",
            "60300.0",
            true,
        ),
    ];
    for (row, side, mode, param, limit, refp, want, capped) in rows {
        let pl = plan(mode, param, p(limit));
        let r = ph(&req(side, p(refp), LinkRole::Second), &pl, &b, 10).unwrap();
        assert_eq!((r.px, r.capped, r.slipped), (p(want), capped, false), "{row}");
        let touch = if side == Side::Buy { b.ask_px } else { b.bid_px };
        match mode {
            PxMode::Cross => assert_eq!(
                r.px,
                cap_cross_px(side, touch, mid, rate(param), &lim(), 10),
                "{row}"
            ),
            PxMode::Mid => assert_eq!(
                r.px,
                cap_cross_px(side, mid, mid, rate(param), &lim(), 10),
                "{row}"
            ),
            PxMode::Limit => {}
        }
    }
}

#[test]
fn oms_221_slip_capped() {
    let b = book("59900.0", "59920.0");
    for (row, mode, limit) in [("OMS-221.1", PxMode::Cross, "0"), ("OMS-221.2", PxMode::Mid, "0")] {
        let r = ph(
            &req(Side::Sell, p("60010.0"), LinkRole::Second),
            &plan(mode, "0.001", p(limit)),
            &b,
            10,
        )
        .unwrap();
        assert_eq!(
            (r.px, r.capped, r.slipped, r.touch, r.cap),
            (p("59610.5"), true, true, p("59900.0"), p("59610.5")),
            "{row}"
        );
    }
    let r = ph(
        &req(Side::Sell, p("60010.0"), LinkRole::Second),
        &plan(PxMode::Limit, "0.001", p("59950.0")),
        &b,
        10,
    )
    .unwrap();
    assert_eq!(
        (r.px, r.capped, r.slipped),
        (p("59950.0"), false, false),
        "OMS-221.3"
    );
    let b = book("59950.0", "59970.0");
    let r = ph(
        &req(Side::Sell, p("60010.0"), LinkRole::Second),
        &plan(PxMode::Cross, "0.001", Px(0)),
        &b,
        10,
    )
    .unwrap();
    assert!(!r.slipped, "OMS-221.4");
    assert_eq!(
        r.px,
        cap_cross_px(Side::Sell, p("59950.0"), p("59960.0"), rate("0.001"), &lim(), 10),
        "OMS-221.4"
    );
}

#[test]
fn oms_222_slip_note_once() {
    let mut h = H::new();
    let (f, l) = place_first(&mut h, q("0.05"), Tif::Ioc, plan_l1(), 0);
    h.fill(f, q("0.012"), p("60000.00"), 1);
    let rq = one_hedge(&h);
    let pl = h.o.link_plan(l).unwrap().1;
    let slipped = HedgePx {
        px: p("59610.5"),
        capped: true,
        slipped: true,
        touch: p("59900.0"),
        cap: p("59610.5"),
    };
    let place = |h: &mut H, hp: HedgePx| {
        let it = ApprovedIntent::hedge(&rq, &pl, hp, IntentClass::Hedge);
        h.place(&it, 2).unwrap();
        h.notes(notes::HEDGE_SLIP_CAPPED)
    };
    let n = place(&mut h, slipped);
    assert_eq!((n.len(), n[0].level), (1, hft_event::NoteLevel::P1));
    assert_eq!(&n[0].v[..4], &[u64::from(l), 6_001_000, 5_990_000, 5_961_050]);
    assert!(h.link(l).flags & link_flags::SLIP_NOTED != 0);
    assert!(place(&mut h, slipped).is_empty());
    assert!(
        place(
            &mut h,
            HedgePx {
                slipped: false,
                ..slipped
            }
        )
        .is_empty()
    );
    assert!(h.link(l).flags & link_flags::SLIP_NOTED == 0);
    assert_eq!(place(&mut h, slipped).len(), 1);
}

#[test]
fn oms_223_beyond_cross() {
    let b = book("59600.0", "60400.0");
    let pl = plan(PxMode::Cross, "0.001", Px(0));
    assert_eq!(
        ph(&req(Side::Buy, Px(0), LinkRole::Second), &pl, &b, 10),
        Err(HedgePxErr::BeyondCross),
        "OMS-223.1"
    );
    assert_eq!(
        ph(&req(Side::Sell, Px(0), LinkRole::Second), &pl, &b, 10),
        Err(HedgePxErr::BeyondCross),
        "OMS-223.2"
    );
    let b = book("59600.0", "60300.0");
    let b = BboView {
        bid_px: p("59700.0"),
        ..b
    };
    let r = ph(&req(Side::Buy, Px(0), LinkRole::Second), &pl, &b, 10).unwrap();
    assert_eq!(r.px, p("60300.0"), "OMS-223.3");
}

#[test]
fn oms_224_quiet_book_prices() {
    let mut b = book("59990.0", "60010.0");
    b.rx_tsc = ms(NOW - 10_000);
    b.conn_live_tsc = ms(NOW - 400);
    let r = ph(
        &req(Side::Sell, p("60010.0"), LinkRole::Second),
        &plan(PxMode::Cross, "0.001", Px(0)),
        &b,
        10,
    )
    .unwrap();
    assert_eq!(r.px, p("59930.1"));
}

#[test]
fn oms_225_stale_quote() {
    let base = book("59990.0", "60010.0");
    let rows: [(&str, BboView, bool); 7] = [
        (
            "OMS-225.1",
            BboView {
                conn_up: false,
                ..base
            },
            false,
        ),
        ("OMS-225.2", BboView { resync: true, ..base }, false),
        (
            "OMS-225.3",
            BboView {
                conn_live_tsc: ms(NOW - 1999),
                ..base
            },
            true,
        ),
        (
            "OMS-225.4",
            BboView {
                conn_live_tsc: ms(NOW - 2000),
                ..base
            },
            true,
        ),
        (
            "OMS-225.5",
            BboView {
                conn_live_tsc: ms(NOW - 2001),
                ..base
            },
            false,
        ),
        (
            "OMS-225.6",
            BboView {
                bid_px: Px(0),
                ..base
            },
            false,
        ),
        (
            "OMS-225.7",
            BboView {
                ask_px: Px(0),
                ..base
            },
            false,
        ),
    ];
    for (row, b, live) in rows {
        let rq = req(Side::Sell, p("60010.0"), LinkRole::Second);
        let r0 = ph(&rq, &plan(PxMode::Cross, "0.001", Px(0)), &b, 10);
        let r1 = ph(&rq, &plan(PxMode::Cross, "0.001", p("59950.0")), &b, 10);
        if live {
            assert_eq!(r0.unwrap().px, p("59930.1"), "{row}");
            assert_eq!(r1.unwrap().px, p("59930.1"), "{row}");
        } else {
            assert_eq!(r0, Err(HedgePxErr::StaleQuote), "{row}");
            let r1 = r1.unwrap();
            assert_eq!((r1.px, r1.capped), (p("59950.0"), false), "{row}");
        }
    }
}

#[test]
fn oms_226_tick_from_inst() {
    let b = book("59990.00", "60010.00");
    let pl = plan(PxMode::Cross, "0.001", Px(0));
    let mut rq = req(Side::Buy, Px(0), LinkRole::Second);
    rq.inst = X;
    assert_eq!(ph(&rq, &pl, &b, 1).unwrap().px, p("60070.01"), "OMS-226.1");
    rq.inst = Y;
    assert_eq!(ph(&rq, &pl, &b, 10).unwrap().px, p("60070.0"), "OMS-226.2");
}

#[test]
fn oms_228_zero_ref_px() {
    let b = book("59900.0", "59920.0");
    let r = ph(
        &req(Side::Sell, Px(0), LinkRole::Second),
        &plan(PxMode::Cross, "0.001", Px(0)),
        &b,
        10,
    )
    .unwrap();
    assert!(!r.slipped);
    assert_eq!(
        r.px,
        cap_cross_px(Side::Sell, p("59900.0"), p("59910.0"), rate("0.001"), &lim(), 10)
    );
    assert_eq!(r.px, p("59840.1"));
}

#[test]
fn oms_229_reverse_pricing() {
    let b = book("60000.00", "60000.01");
    let pl = plan(PxMode::Limit, "0.001", p("59950.0"));
    let mut rq = req(Side::Sell, Px(0), LinkRole::Reverse);
    rq.inst = X;
    rq.venue = VS;
    let r = ph(&rq, &pl, &b, 1).unwrap();
    assert_eq!((r.px, r.capped, r.slipped), (p("59940.00"), false, false));
}

#[test]
fn oms_230_gate_note_once() {
    let setup = || {
        let mut h = H::new();
        let (f, l) = place_first(&mut h, q("0.05"), Tif::Ioc, plan_l1(), 0);
        h.fill(f, q("0.012"), p("60000.00"), 1);
        (h, l)
    };
    let gate = |h: &mut H, l: u32, e: HedgePxErr| {
        h.out.clear();
        h.o.hedge_gated(l, HedgeGate::Px(e), Tsc(0), ms(2), 1, &mut h.out);
        (h.n_notes(notes::HEDGE_PX_STALE), h.n_notes(notes::HEDGE_PX_GATED))
    };
    let (mut h, l) = setup();
    let a = gate(&mut h, l, HedgePxErr::BeyondCross).1 + gate(&mut h, l, HedgePxErr::BeyondCross).1;
    assert_eq!(a, 1, "OMS-230.1");
    let (mut h, l) = setup();
    let mut n = gate(&mut h, l, HedgePxErr::BeyondCross).1;
    let rq = HedgeRequest {
        role: LinkRole::SecondRetry,
        ..req(Side::Sell, p("60010.0"), LinkRole::SecondRetry)
    };
    let rq = HedgeRequest { link_id: l, ..rq };
    let pl = h.o.link_plan(l).unwrap().1;
    h.place(
        &ApprovedIntent::hedge(&rq, &pl, HedgePx::default(), IntentClass::Hedge),
        3,
    )
    .unwrap();
    n += gate(&mut h, l, HedgePxErr::BeyondCross).1;
    assert_eq!(n, 2, "OMS-230.2");
    let (mut h, l) = setup();
    let s: usize = (0..3).map(|_| gate(&mut h, l, HedgePxErr::StaleQuote).0).sum();
    assert_eq!(s, 1, "OMS-230.3");
    let (mut h, l) = setup();
    let a = gate(&mut h, l, HedgePxErr::StaleQuote);
    let b = gate(&mut h, l, HedgePxErr::BeyondCross);
    assert_eq!((a.0, b.1), (1, 1), "OMS-230.4");
}
