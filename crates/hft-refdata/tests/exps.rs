//! Load step 6: exponents, unit shifts, wire exponents, unsupported kinds (REF-025 – REF-027).

mod common;

use common::*;
use hft_refdata::LoadErrCode::{ExpRange, NotionalExp, UnsupportedKind};
use hft_refdata::{AssetClass, InstKind};
use hft_types::{AssetExp, AssetId, InstId, MarketKind, Px, Qty};

#[test]
fn ref_025_asset_exp_range() {
    let cases: [(&str, (i8, i8), Option<hft_refdata::LoadErrCode>); 11] = [
        ("REF-025.1", (-18, -8), None),
        ("REF-025.2", (-19, -8), Some(ExpRange)),
        ("REF-025.3", (6, -12), None),
        ("REF-025.4", (7, -13), Some(ExpRange)),
        ("REF-025.5", (-8, -18), None),
        ("REF-025.6", (-8, -19), Some(ExpRange)),
        ("REF-025.7", (-12, 6), None),
        ("REF-025.8", (-13, 7), Some(ExpRange)),
        ("REF-025.9", (-2, -4), None),
        ("REF-025.10", (-1, -4), Some(NotionalExp)),
        ("REF-025.11", (-8, 6), Some(NotionalExp)),
    ];
    for (case, (p, q), want) in cases {
        let mut s = f0_spec();
        let eth = s.asset_mut(3);
        eth.exp = AssetExp {
            px_exp: p,
            qty_exp: q,
        };
        eth.px_max = Px(1_000);
        eth.qty_max = Qty(1_000);
        let want: Vec<_> = want.map(|c| e(c, 1, 2)).into_iter().collect();
        assert_eq!(outcome(load(&s.build())), want, "{case}");
    }
}

/// F0 + asset 6 `TST` (−4, −8) + inst 6 `TSTUSDT`@BINANCE_SPOT with the given shifts and steps.
fn tst(px_shift: i8, qty_shift: i8, tick: &str, lot: &str) -> Vec<u8> {
    let mut s = f0_spec();
    s.assets.push(asset(
        6,
        "TST",
        AssetClass::Crypto,
        (-4, -8),
        1_000_000_000_000,
        1_000_000_000_000,
    ));
    let mut i = inst(
        6,
        "TSTUSDT",
        V2,
        InstKind::Spot,
        AssetId(6),
        (tick, 1),
        (lot, 1),
        (lot, 1),
    );
    i.px_unit_shift = px_shift;
    i.qty_unit_shift = qty_shift;
    i.book_window = 64;
    i.max_qty = Qty(1_000);
    s.insts.push(i);
    s.build()
}

#[test]
fn ref_026_unit_shift_and_wire_exp() {
    let exp_err = vec![e(ExpRange, 3, 5)];
    let rd = ok(&tst(0, 0, "0.0001", "0.00000001"));
    let h = rd.inst(InstId(6));
    assert_eq!(
        (h.px_wire_exp, h.qty_wire_exp, h.tick, h.lot),
        (-4, -8, 1, 1),
        "REF-026.1"
    );
    let rd = ok(&tst(-12, 0, "0.0000000000000001", "0.00000001"));
    assert_eq!(
        (rd.inst(InstId(6)).px_wire_exp, rd.inst(InstId(6)).tick),
        (-16, 1),
        "REF-026.2"
    );
    assert_eq!(errs(&tst(-13, 0, "0.0001", "0.00000001")), exp_err, "REF-026.3");
    let rd = ok(&tst(10, 0, "1000000", "0.00000001"));
    assert_eq!(
        (rd.inst(InstId(6)).px_wire_exp, rd.inst(InstId(6)).tick),
        (6, 1),
        "REF-026.4"
    );
    assert_eq!(errs(&tst(11, 0, "10000000", "0.00000001")), exp_err, "REF-026.5");
    let rd = ok(&tst(0, 10, "0.0001", "0.000000000000000001"));
    assert_eq!(
        (rd.inst(InstId(6)).qty_wire_exp, rd.inst(InstId(6)).lot),
        (-18, 1),
        "REF-026.6"
    );
    assert_eq!(errs(&tst(0, 11, "0.0001", "0.00000001")), exp_err, "REF-026.7");
    let rd = ok(&tst(0, -12, "0.0001", "10000"));
    assert_eq!(
        (rd.inst(InstId(6)).qty_wire_exp, rd.inst(InstId(6)).lot),
        (4, 1),
        "REF-026.8"
    );
    assert_eq!(errs(&tst(0, -13, "0.0001", "0.00000001")), exp_err, "REF-026.9");
    assert_eq!(errs(&tst(0, 12, "0.0001", "0.00000001")), exp_err, "REF-026.10");
    let rd = ok(&f0());
    let h = rd.inst(InstId(5));
    assert_eq!(
        (h.px_wire_exp, h.qty_wire_exp, h.tick, h.lot),
        (-9, -3, 100, 1_000),
        "REF-026.11"
    );
}

#[test]
fn ref_027_unsupported_kind_and_market() {
    let mut s = f0_spec();
    s.inst_mut(1).kind = InstKind::Delivery;
    assert_eq!(errs(&s.build()), vec![e(UnsupportedKind, 3, 0)], "REF-027.1");
    let rd = ok(&f0());
    assert_eq!(
        rd.inst_meta(InstId(1)).market(),
        MarketKind::LinearPerp,
        "REF-027.2"
    );
    assert_eq!(rd.inst_meta(InstId(2)).market(), MarketKind::Spot, "REF-027.3");
}
