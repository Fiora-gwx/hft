#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
use crate::common;

use common::*;
use hft_event::{NoteLevel, OrderAction, OrderEventKind};
use hft_oms::notes;
use hft_oms::*;
use hft_types::{ASSET_NONE, AssetId, GROUP_NONE, Qty, Side, SlotId, Tif};

#[test]
fn oms_120_net_qty_rules() {
    let rows: [(&str, bool, Side, i64, AssetId, i64, i64); 6] = [
        ("OMS-120.1", true, Side::Buy, 1000, BTC, 999_000, 999_000),
        ("OMS-120.2", true, Side::Sell, 1000, BTC, -1_001_000, 1_001_000),
        ("OMS-120.3", true, Side::Buy, 600_000, USDT, 1_000_000, 1_000_000),
        ("OMS-120.4", true, Side::Buy, 120_000, BNB, 1_000_000, 1_000_000),
        ("OMS-120.5", false, Side::Sell, 1000, BTC, -1_000_000, 1_000_000),
        ("OMS-120.6", true, Side::Buy, 0, ASSET_NONE, 1_000_000, 1_000_000),
    ];
    for (row, spot, side, fee, fa, net, net_filled) in rows {
        let mut h = H::new();
        let it = if spot {
            intent(VS, A1, X, side, Tif::Ioc, p("60000.00"), q("0.01"), g1())
        } else {
            intent(VX, A1, Y, side, Tif::Ioc, p("60000.0"), q("0.01"), g1())
        };
        let s = h.place_ok(&it, 0);
        let mut e = h.oe(s, OrderEventKind::Fill);
        e.cum_filled = q("0.01");
        e.avg_px = p("60000.00");
        e.fee = Qty(fee);
        e.fee_asset = fa;
        h.ev(&e, 1);
        assert_eq!(h.out.fills[0].net_qty, Qty(net), "{row}");
        assert_eq!(h.ext(s).net_filled, Qty(net_filled), "{row}");
    }
}

fn l1_filled_no_fee(h: &mut H) -> (hft_strategy_sdk::OrderRef, u32) {
    let (f, l) = place_first(h, q("0.012"), Tif::Ioc, plan_l1(), 0);
    let mut e = h.oe(f, OrderEventKind::Fill);
    e.cum_filled = q("0.012");
    e.avg_px = p("60000.00");
    e.fee_asset = ASSET_NONE;
    h.ev(&e, 1);
    (f, l)
}

#[test]
fn oms_121_late_fee_net_adjust() {
    let mut h = H::new();
    let (f, l) = l1_filled_no_fee(&mut h);
    assert_eq!(one_hedge(&h).qty, q("0.012"));
    assert_eq!(h.legs(A1, g1()).view.leg[1].residual, Qty(0));
    assert_eq!(h.legs(A1, g1()).last_link, l);
    let mut e = h.oe(f, OrderEventKind::FeeOnly);
    e.fee = Qty(1200);
    e.fee_asset = BTC;
    h.ev(&e, 2);
    let fo = h.out.fees[0];
    assert_eq!(
        (h.out.fees.len(), fo.fee, fo.fee_asset, fo.group),
        (1, 1200, BTC, g1())
    );
    assert!(transitions(&h).is_empty());
    h.o.net_adjust(A1, g1(), X, Qty(-1200));
    assert_eq!(h.legs(A1, g1()).view.leg[0].net, q("0.011988"));
    assert_eq!(h.ext(f).net_filled, q("0.011988"));
    assert_eq!(h.legs(A1, g1()).view.leg[1].residual, Qty(-1200));
}

#[test]
fn oms_122_net_adjust_broken_link() {
    let mut h = H::new();
    let (_f, l) = l1_filled_no_fee(&mut h);
    h.out.clear();
    h.o.hedge_vetoed(l, ms(2), 1, &mut h.out);
    assert_eq!(h.link(l).state, LinkState::Broken);
    let n0 = h.legs(A1, g1()).view.leg[0].net;
    h.o.net_adjust(A1, g1(), X, Qty(-1200));
    assert_eq!(h.legs(A1, g1()).view.leg[0].net, Qty(n0.0 - 1200));
    assert_eq!(h.legs(A1, g1()).view.leg[1].residual, Qty(0));
}

#[test]
fn oms_123_fee_unattributed() {
    let mut h = H::new();
    let s = h.place_ok(&buy_x(), 0);
    let legs = h.legs(A1, g1());
    let mut e = h.oe(s, OrderEventKind::FeeOnly);
    e.slot = SlotId(9);
    e.r#gen = 4;
    e.inst = X;
    e.fee = Qty(300_000);
    e.fee_asset = USDT;
    h.ev(&e, 1);
    let fo = h.out.fees[0];
    assert_eq!(
        (fo.group, fo.inst, fo.side, fo.fee, fo.fee_asset),
        (GROUP_NONE, X, 0, 300_000, USDT)
    );
    assert_eq!(h.notes(notes::FEE_UNATTRIBUTED)[0].level, NoteLevel::P1);
    assert_eq!(h.state(s), OrderState::PendingNew);
    assert_eq!(h.legs(A1, g1()).view, legs.view);
}

#[test]
fn oms_124_no_balance_check() {
    let mut h = H::new();
    let it = intent(VS, A1, X, Side::Sell, Tif::Gtc, p("60000.00"), q("0.5"), g1());
    assert!(h.place(&it, 0).is_ok());
    let d = h.decisions(OrderAction::New);
    assert_eq!((d.len(), d[0].qty), (1, q("0.5")));
}
