#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
mod common;

use common::*;
use hft_event::OrderEventKind;
use hft_oms::*;
use hft_strategy_sdk::{GroupTable, OrderRef};
use hft_testkit::CountingAlloc;
use hft_types::{GROUP_NONE, GroupId, Qty, Rate, Side, StrategyId, Tif};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

#[test]
fn oms_007_groups_changed_rebuilds_map() {
    let mut o = Oms::new(&p0(), rd()).unwrap();
    o.on_groups_changed(&groups_s1(false));
    let both = groups_s1(true);
    let scope = CountingAlloc::scope();
    o.on_groups_changed(&both);
    assert_eq!(scope.allocs(), 0);
    let mut h = H {
        o,
        out: OmsOut::new(),
        seq: 1,
    };
    h.conn(
        VG,
        A1,
        hft_event::ConnRole::Order,
        hft_event::ConnStateKind::Up,
        0,
    );
    let z = h.place_ok(
        &intent(VG, A1, Z, Side::Buy, Tif::Ioc, p("60000.0"), q("0.002"), g2()),
        0,
    );
    let mut e = h.oe(z, OrderEventKind::Fill);
    e.cum_filled = q("0.002");
    e.avg_px = p("60000.0");
    e.fee_asset = USDT;
    h.ev(&e, 1);
    assert_eq!(h.ext(z).leg_ix, 1);
    assert_eq!(h.legs(A1, g2()).view.leg[1].net, Qty(200_000));
    assert!(h.legs(A1, g1()).view.leg.iter().all(|l| l.net == Qty(0)));
}

#[test]
fn oms_248_naked_groups_batches() {
    let mut o = Oms::new(&p0(), rd()).unwrap();
    let specs: Vec<_> = (1..=100u32)
        .map(|n| group_spec(GroupId::compose(StrategyId(1), n).unwrap(), &[(X, VS), (Y, VX)]))
        .collect();
    o.on_groups_changed(&GroupTable::new(&specs, 128));
    let mut h = H {
        o,
        out: OmsOut::new(),
        seq: 1,
    };
    let mut want = Vec::new();
    for n in 1..=100u32 {
        let g = GroupId::compose(StrategyId(1), n).unwrap();
        if n <= 70 {
            h.o.test_set_net(A1, g, X, q("0.001"));
            want.push(g);
        } else if n <= 75 {
            h.conn(
                VS,
                A1,
                hft_event::ConnRole::Order,
                hft_event::ConnStateKind::Up,
                0,
            );
            h.place_ok(
                &intent(VS, A1, X, Side::Buy, Tif::Ioc, p("60000.00"), q("0.01"), g),
                0,
            );
            assert_ne!(h.legs(A1, g).view.leg[0].inflight_immediate, Qty(0));
        }
    }
    let mut buf = FixedVec::<GroupId, 64>::new();
    let mut cur = 0;
    let snap = h.o.legs(A1, want[0]).view;
    assert!(h.o.naked_groups(A1, &mut buf, &mut cur));
    let mut got: Vec<_> = buf.to_vec();
    assert_eq!(buf.len(), 64);
    buf.clear();
    assert!(!h.o.naked_groups(A1, &mut buf, &mut cur));
    assert_eq!(buf.len(), 6);
    got.extend(buf.iter());
    assert_eq!(got, want);
    assert_eq!(h.o.legs(A1, want[0]).view, snap);
}

#[test]
fn oms_252_adopt_adjust() {
    let setup = || {
        let mut h = H::new();
        h.o.test_set_net(A1, g1(), X, q("0.012"));
        h.o.test_set_net(A1, g1(), Y, q("-0.012"));
        h
    };
    let mut h = setup();
    h.o.adopt_adjust(A1, g1(), Y, q("0.003"));
    let v = h.legs(A1, g1()).view;
    assert_eq!(
        (v.leg[1].net, v.naked_qty),
        (q("-0.009"), q("0.003")),
        "OMS-252.1"
    );
    assert_eq!(h.o.table_view(VX, A1).unwrap().free_len, 64, "OMS-252.1");
    let mut h = setup();
    h.o.adopt_adjust(A1, g1(), X, q("-0.012"));
    let v = h.legs(A1, g1()).view;
    assert_eq!((v.leg[0].net, v.leg[1].net), (Qty(0), q("-0.012")), "OMS-252.2");
    let mut h = setup();
    let before = h.legs(A1, g1()).view;
    h.o.adopt_adjust(A1, GROUP_NONE, Y, q("0.003"));
    assert_eq!(h.legs(A1, g1()).view, before, "OMS-252.3");
}

fn slot_in(h: &mut H, tif: Tif, st: OrderState, acked: bool, filled: &str) -> OrderRef {
    let mut it = buy_x();
    it.intent.tif = tif;
    let s = h.place_ok(&it, 0);
    h.force(s, st, acked, OrderState::PendingNew);
    let f = q(filled);
    h.o.test_slot_mut(VS, A1, s.slot, |sl, _| sl.filled = f);
    s
}

#[test]
fn oms_290_inflight_classes() {
    use OrderState::*;
    let rows: [(&str, Tif, OrderState, bool, &str, &str, &str, &str, &str); 11] = [
        (
            "OMS-290.1",
            Tif::Ioc,
            PendingNew,
            false,
            "0",
            "0.01",
            "0",
            "0.01",
            "0.01",
        ),
        (
            "OMS-290.2",
            Tif::Ioc,
            Live,
            true,
            "0",
            "0.01",
            "0",
            "0.01",
            "0.01",
        ),
        (
            "OMS-290.3",
            Tif::Fok,
            Live,
            true,
            "0",
            "0.01",
            "0",
            "0.01",
            "0.01",
        ),
        (
            "OMS-290.4",
            Tif::Gtc,
            PendingNew,
            false,
            "0",
            "0.01",
            "0",
            "0.01",
            "0.01",
        ),
        (
            "OMS-290.5",
            Tif::PostOnly,
            PendingNew,
            false,
            "0",
            "0.01",
            "0",
            "0.01",
            "0.01",
        ),
        (
            "OMS-290.6",
            Tif::Gtc,
            PendingCancel,
            false,
            "0",
            "0.01",
            "0",
            "0.01",
            "0.01",
        ),
        ("OMS-290.7", Tif::Gtc, Live, true, "0", "0", "0.01", "0", "0.01"),
        (
            "OMS-290.8",
            Tif::Gtc,
            PendingCancel,
            true,
            "0",
            "0",
            "0.01",
            "0",
            "0.01",
        ),
        (
            "OMS-290.9",
            Tif::PostOnly,
            PendingAmend,
            true,
            "0",
            "0",
            "0.01",
            "0",
            "0.01",
        ),
        (
            "OMS-290.10",
            Tif::Gtc,
            Unknown,
            true,
            "0",
            "0.01",
            "0",
            "0.01",
            "0.01",
        ),
        (
            "OMS-290.11",
            Tif::Gtc,
            Live,
            true,
            "0.004",
            "0",
            "0.006",
            "0",
            "0.006",
        ),
    ];
    for (row, tif, st, acked, filled, imm, rest, imm_buy, open_buy) in rows {
        let mut h = H::new();
        slot_in(&mut h, tif, st, acked, filled);
        let l = h.legs(A1, g1()).view.leg[0];
        let ib = h.o.inflight().pos(A1, X);
        assert_eq!(
            (l.inflight_immediate, l.inflight_resting, ib.imm_buy, ib.open_buy),
            (q(imm), q(rest), q(imm_buy), q(open_buy)),
            "{row}"
        );
    }
}

fn snap(h: &H) -> (Qty, Qty, Qty, Qty) {
    let l = h.legs(A1, g1()).view.leg[0];
    let ib = h.o.inflight().pos(A1, X);
    (l.inflight_immediate, l.inflight_resting, ib.imm_buy, ib.open_buy)
}

#[test]
fn oms_291_class_moves() {
    let mut h = H::new();
    let s = h.place_ok(&buy_x(), 0);
    let mut seen = vec![snap(&h)];
    h.ack(s, 1, 1);
    seen.push(snap(&h));
    h.cancel(s, 2).unwrap();
    seen.push(snap(&h));
    h.conn(
        VS,
        A1,
        hft_event::ConnRole::Order,
        hft_event::ConnStateKind::Down,
        3,
    );
    seen.push(snap(&h));
    h.conn(
        VS,
        A1,
        hft_event::ConnRole::Order,
        hft_event::ConnStateKind::Up,
        4,
    );
    h.query(s, 1, Qty(0), hft_types::Px(0), 4);
    seen.push(snap(&h));
    h.what(s, OrderEventKind::CancelAck, 5);
    seen.push(snap(&h));
    let a = (q("0.01"), Qty(0), q("0.01"), q("0.01"));
    let b = (Qty(0), q("0.01"), Qty(0), q("0.01"));
    let z = (Qty(0), Qty(0), Qty(0), Qty(0));
    assert_eq!(seen, vec![a, b, b, a, b, z]);
}

#[test]
fn oms_292_resting_not_cover() {
    let mut h = H::new();
    let mut y = sell_y();
    y.intent.qty = q("1.0");
    let ys = h.place_ok(&y, 0);
    h.ack(ys, 1, 0);
    let (f, _l) = place_first(&mut h, q("1.0"), Tif::Ioc, plan_l1(), 0);
    h.fill(f, q("1.0"), p("60000.00"), 1);
    assert_eq!(one_hedge(&h).qty, q("1.0"));
    assert_eq!(h.legs(A1, g1()).view.naked_qty, q("1.0"));
}

#[test]
fn oms_293_naked_dir() {
    // (hedge side, ratio, net_f, net_h, imm_f, imm_h, rest_h, want)
    let rows: [(&str, Side, u32, &str, &str, &str, &str, &str, &str); 9] = [
        ("OMS-293.1", Side::Sell, 1, "0.012", "0", "0", "0", "0", "0.012"),
        (
            "OMS-293.2",
            Side::Sell,
            1,
            "0.012",
            "0",
            "0",
            "-0.004",
            "0",
            "0.008",
        ),
        (
            "OMS-293.3",
            Side::Sell,
            1,
            "0.012",
            "0",
            "0.005",
            "0",
            "0",
            "0.012",
        ),
        (
            "OMS-293.4",
            Side::Sell,
            1,
            "0.012",
            "0",
            "-0.003",
            "0",
            "0",
            "0.009",
        ),
        (
            "OMS-293.5",
            Side::Sell,
            1,
            "0.012",
            "0",
            "0",
            "0",
            "-0.012",
            "0.012",
        ),
        ("OMS-293.6", Side::Sell, 1, "0.012", "-0.02", "0", "0", "0", "0"),
        ("OMS-293.7", Side::Buy, 1, "-0.012", "0", "0", "0", "0", "0.012"),
        (
            "OMS-293.8",
            Side::Buy,
            1,
            "-0.012",
            "0",
            "0",
            "0.004",
            "0",
            "0.008",
        ),
        (
            "OMS-293.9",
            Side::Sell,
            2,
            "0.005",
            "-0.004",
            "0",
            "0",
            "0",
            "0.006",
        ),
    ];
    for (row, side, num, nf, nh, imf, imh, rsh, want) in rows {
        let mut h = H::new();
        let mut plan = plan_l1();
        plan.side = side;
        plan.ratio_num = num;
        let mut it = first_leg(q("0.012"), Tif::Ioc, plan);
        it.intent.side = side.opposite();
        let f = h.place_ok(&it, 0);
        let l = h.slot(f).link_id;
        // The first leg's own in-flight is replaced by the row's values.
        h.what(f, OrderEventKind::CancelAck, 0);
        let (nf, nh, imf, imh, rsh) = (q(nf), q(nh), q(imf), q(imh), q(rsh));
        h.o.test_leg_mut(A1, g1(), |st| {
            st.view.leg[0].net = nf;
            st.view.leg[1].net = nh;
            st.view.leg[0].inflight_immediate = imf;
            st.view.leg[1].inflight_immediate = imh;
            st.view.leg[1].inflight_resting = rsh;
        });
        assert_eq!(h.o.test_naked_dir(l), Some(q(want).0), "{row}");
    }
}

#[test]
fn oms_294_naked_metrics() {
    let mut h = H::new();
    let (f, _) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
    h.fill(f, q("0.012"), p("60000.00"), 0);
    h.o.test_leg_mut(A1, g1(), |st| {
        st.view.leg[1].net = q("-0.009");
        st.view.leg[1].inflight_immediate = q("-0.0006");
    });
    let v = h.legs(A1, g1()).view;
    assert_eq!(v.naked_qty, q("0.0024"));
    assert_eq!(v.naked_ratio, Rate(200_000_000));
    assert_eq!(v.naked_notional.0, usdt("144.00"));
}

#[test]
fn oms_295_suppress_counters() {
    let mut h = H::new();
    place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
    assert_eq!(h.legs(A1, g1()).view.open_links, 1, "OMS-295.1");
    let mut h = H::new();
    let (f, _) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
    h.fill(f, q("0.012"), p("60000.00"), 0);
    let (f2, _) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
    h.what(f2, OrderEventKind::CancelAck, 1);
    assert_eq!(h.legs(A1, g1()).view.open_links, 1, "OMS-295.2");
    let mut h = H::new();
    let s = h.place_ok(&buy_x(), 0);
    h.what(s, OrderEventKind::OutcomeUnknown, 0);
    assert!(h.legs(A1, g1()).view.has_unknown, "OMS-295.3");
    let mut h = H::new();
    let s = h.place_ok(&buy_x(), 0);
    h.ack(s, 1, 0);
    h.cancel(s, 1).unwrap();
    assert!(h.legs(A1, g1()).view.has_pending, "OMS-295.4");
    let mut h = H::new();
    let s = h.place_ok(&buy_x(), 0);
    h.ack(s, 1, 0);
    let v = h.legs(A1, g1()).view;
    assert_eq!(
        (v.has_pending, v.has_unknown, v.leg[0].inflight_immediate),
        (false, false, Qty(0)),
        "OMS-295.5"
    );
}
