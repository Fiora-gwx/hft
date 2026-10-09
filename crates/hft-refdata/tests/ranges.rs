//! Load step 8: value ranges, symbol uniqueness, dust, MMR tiers, price bands
//! (REF-029, REF-030, REF-090 – REF-092).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::common;

use common::*;
use hft_refdata::LoadErrCode::{DanglingRef, DuplicateSymbol, ValueRange};
use hft_refdata::{AliasSpec, MmrTier, MmrTierSpec, PriceBand};
use hft_types::{AssetId, InstId, Notional, Px, Qty, Rate, VenueId};

const P60: i64 = 1 << 60;

#[test]
fn ref_029_value_range() {
    let btc = |f: &dyn Fn(&mut hft_refdata::AssetSpec)| {
        let mut s = f0_spec();
        f(s.asset_mut(1));
        s.build()
    };
    let i1 = |f: &dyn Fn(&mut hft_refdata::InstSpec)| {
        let mut s = f0_spec();
        f(s.inst_mut(1));
        s.build()
    };
    let vr = |sec: u16, row: u32| e(ValueRange, sec, row);
    ok(&btc(&|a| a.px_max = Px(P60 - 1))); // REF-029.1
    assert_eq!(errs(&btc(&|a| a.px_max = Px(P60))), vec![vr(1, 0)], "REF-029.2");
    ok(&btc(&|a| a.qty_max = Qty(P60 - 1))); // REF-029.3
    assert_eq!(errs(&btc(&|a| a.qty_max = Qty(P60))), vec![vr(1, 0)], "REF-029.4");
    ok(&i1(&|i| i.max_qty = Qty(100_000_000_000_000))); // REF-029.5
    assert_eq!(
        errs(&i1(&|i| i.max_qty = Qty(100_000_000_000_001))),
        vec![vr(3, 0)],
        "REF-029.6"
    );
    assert_eq!(errs(&i1(&|i| i.book_window = 63)), vec![vr(3, 0)], "REF-029.7");
    ok(&i1(&|i| i.book_window = 64)); // REF-029.8
    ok(&i1(&|i| i.book_window = 65_536)); // REF-029.9
    assert_eq!(
        errs(&i1(&|i| i.book_window = 65_537)),
        vec![vr(3, 0)],
        "REF-029.10"
    );
    ok(&btc(&|a| a.px_max = Px(40_960_000_000))); // REF-029.11
    assert_eq!(
        errs(&btc(&|a| a.px_max = Px(40_959_999_999))),
        vec![vr(3, 0), vr(3, 2), vr(3, 3)],
        "REF-029.12"
    );
    ok(&btc(&|a| a.px_max = Px(P60 - 2))); // REF-029.13
}

#[test]
fn ref_030_duplicate_symbol() {
    let rd = ok(&f0());
    assert_eq!(
        rd.symbols(VenueId(1)).get(b"BTCUSDT"),
        Some(InstId(1)),
        "REF-030.1"
    );
    assert_eq!(
        rd.symbols(VenueId(2)).get(b"BTCUSDT"),
        Some(InstId(2)),
        "REF-030.1"
    );
    let edit = |f: &dyn Fn(&mut Spec)| {
        let mut s = f0_spec();
        f(&mut s);
        s.build()
    };
    let ds = |sec: u16, row: u32| e(DuplicateSymbol, sec, row);
    assert_eq!(
        errs(&edit(&|s| s.inst_mut(5).symbol = "BTCUSDT".into())),
        vec![ds(3, 4)],
        "REF-030.2"
    );
    let b = edit(&|s| {
        s.inst_mut(1).venue_num_id = 777;
        s.inst_mut(5).venue_num_id = 777;
    });
    assert_eq!(errs(&b), vec![ds(3, 4)], "REF-030.3");
    ok(&edit(&|s| s.inst_mut(4).venue_num_id = 10_459)); // REF-030.4
    ok(&edit(&|s| {
        s.inst_mut(1).venue_num_id = 0;
        s.inst_mut(5).venue_num_id = 0;
    })); // REF-030.5
    assert_eq!(
        errs(&edit(&|s| s.aliases[0].alias = "BTCUSDT".into())),
        vec![ds(11, 0)],
        "REF-030.6"
    );
    let b = edit(&|s| {
        s.aliases.push(AliasSpec {
            venue: V2,
            alias: "BTC_USDT".into(),
            inst: InstId(2),
        })
    });
    assert_eq!(errs(&b), vec![ds(11, 1)], "REF-030.7");
    let rd = ok(&edit(&|s| {
        s.aliases.push(AliasSpec {
            venue: V6,
            alias: "BTCUSDT".into(),
            inst: InstId(4),
        })
    }));
    assert_eq!(
        rd.symbols(VenueId(6)).get(b"BTCUSDT"),
        Some(InstId(4)),
        "REF-030.8"
    );
}

#[test]
fn ref_090_asset_dust() {
    let with = |dust: i64| {
        let mut s = f0_spec();
        s.asset_mut(1).dust = Qty(dust);
        load(&s.build())
    };
    let rd = with(1_000).expect("REF-090.1");
    assert_eq!(rd.asset(AssetId(1)).dust, Qty(1_000), "REF-090.1");
    let vr = vec![e(ValueRange, 1, 0)];
    let cases: [(&str, i64, bool); 6] = [
        ("REF-090.2", 1, true),
        ("REF-090.3", 0, false),
        ("REF-090.4", -1, false),
        ("REF-090.5", 100_000_000_000_000, true),
        ("REF-090.6", 99_999_999_999_999, true),
        ("REF-090.7", 100_000_000_000_001, false),
    ];
    for (case, dust, pass) in cases {
        let want = if pass { vec![] } else { vr.clone() };
        assert_eq!(outcome(with(dust)), want, "{case}");
    }
}

/// `T(i, n USDT, mmr)`.
fn t(inst: u32, usdt: i64, mmr: Rate) -> MmrTierSpec {
    MmrTierSpec {
        inst: InstId(inst),
        tier: MmrTier {
            notional_max: Notional(usdt * 1_000_000),
            mmr,
        },
    }
}

/// Percent in hundredths (`pct(40)` = 0.4 % = Rate(4 000 000)).
fn pct(hundredths: i64) -> Rate {
    Rate(hundredths * 100_000)
}

#[test]
fn ref_091_mmr_tiers() {
    let with = |tiers: Vec<MmrTierSpec>| {
        let mut s = f0_spec();
        s.mmr = tiers;
        load(&s.build())
    };
    let rd = with(vec![]).expect("REF-091.1");
    assert!(rd.inst_meta(InstId(1)).mmr_tiers.is_empty(), "REF-091.1");
    let rd = with(vec![t(1, 50_000, pct(40)), t(1, 600_000, pct(50))]).expect("REF-091.2");
    let want = [
        MmrTier {
            notional_max: Notional(50_000_000_000),
            mmr: Rate(4_000_000),
        },
        MmrTier {
            notional_max: Notional(600_000_000_000),
            mmr: Rate(5_000_000),
        },
    ];
    assert_eq!(&*rd.inst_meta(InstId(1)).mmr_tiers, &want, "REF-091.2");
    assert!(rd.inst_meta(InstId(3)).mmr_tiers.is_empty(), "REF-091.2");
    let vr = |row: u32| vec![e(ValueRange, 18, row)];
    assert_eq!(
        outcome(with(vec![t(1, 50_000, pct(40)), t(1, 50_000, pct(50))])),
        vr(1),
        "REF-091.3"
    );
    assert_eq!(
        outcome(with(vec![t(1, 50_000, pct(50)), t(1, 600_000, pct(40))])),
        vr(1),
        "REF-091.4"
    );
    assert_eq!(
        outcome(with(vec![t(1, 50_000, pct(40)), t(1, 600_000, pct(40))])),
        vec![],
        "REF-091.5"
    );
    assert_eq!(outcome(with(vec![t(1, 50_000, Rate(0))])), vr(0), "REF-091.6");
    assert_eq!(
        outcome(with(vec![t(1, 50_000, Rate(1_000_000_000))])),
        vr(0),
        "REF-091.7"
    );
    assert_eq!(
        outcome(with(vec![t(1, 50_000, Rate(999_999_999))])),
        vec![],
        "REF-091.8"
    );
    assert_eq!(outcome(with(vec![t(2, 50_000, pct(40))])), vr(0), "REF-091.9");
    assert_eq!(
        outcome(with(vec![t(99, 50_000, pct(40))])),
        vec![e(DanglingRef, 18, 0)],
        "REF-091.10"
    );
}

#[test]
fn ref_092_price_band() {
    let with = |up: i64, down: i64| {
        let mut s = f0_spec();
        s.inst_mut(1).price_band = PriceBand {
            up: Rate(up),
            down: Rate(down),
        };
        load(&s.build())
    };
    let rd = with(0, 0).expect("REF-092.1");
    assert_eq!(
        rd.inst_meta(InstId(1)).price_band,
        PriceBand {
            up: Rate(0),
            down: Rate(0)
        },
        "REF-092.1"
    );
    let five = 50_000_000;
    let rd = with(five, five).expect("REF-092.2");
    assert_eq!(
        rd.inst_meta(InstId(1)).price_band,
        PriceBand {
            up: Rate(five),
            down: Rate(five)
        },
        "REF-092.2"
    );
    assert!(with(999_999_999, 999_999_999).is_ok(), "REF-092.3");
    let vr = vec![e(ValueRange, 3, 0)];
    assert_eq!(outcome(with(1_000_000_000, five)), vr, "REF-092.4");
    assert_eq!(outcome(with(five, 1_000_000_000)), vr, "REF-092.5");
    assert_eq!(outcome(with(-1, five)), vr, "REF-092.6");
}
