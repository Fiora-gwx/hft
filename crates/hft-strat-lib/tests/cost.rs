//! `CostLine` and `fee_round_trip` (TESTS.md SLB-004–SLB-010).

mod rf;

use hft_refdata::FeeAssetRule;
use hft_strat_lib::{CostErr, CostItem, CostLine, FeeErr, FeeLeg, Liquidity, cost_flags, fee_round_trip};
use hft_strategy_sdk::RefView;
use hft_types::{AccountId, InstId, Rate};
use rf::{A1, A2, I1, I2, I3};

use cost_flags::{FIXED, OUT_OF_QUOTE as OOQ, ROUND_TRIP as RT};

fn item(rate: i64, label: u16, flags: u8) -> CostItem {
    CostItem::new(Rate(rate), label, flags)
}

/// SLB-004 items.
fn items4() -> Vec<CostItem> {
    vec![
        item(3_000_000, 1, RT | OOQ),
        item(500_000, 2, FIXED),
        item(200_000, 3, OOQ),
        item(1_000_000, 4, RT),
        item(9_000_000, 5, 0),
    ]
}

fn totals(c: &CostLine) -> (i64, i64, i64) {
    (c.round_trip().0, c.open_floor().0, c.out_of_quote().0)
}

#[test]
fn slb_004_set_sums_by_flag() {
    let mut c = CostLine::default();
    let it = items4();
    assert_eq!(c.set(&it, Rate(2_000_000)), Ok(()));
    assert_eq!(totals(&c), (4_000_000, 4_500_000, 3_200_000));
    assert_eq!(c.n, 5);
    assert_eq!(&c.items[..5], &it[..]);
    // label does not change the totals
    let relabeled: Vec<CostItem> = it.iter().map(|x| CostItem { label: 999, ..*x }).collect();
    let mut d = CostLine::default();
    d.set(&relabeled, Rate(2_000_000)).unwrap();
    assert_eq!(totals(&d), totals(&c));
}

#[test]
fn slb_005_rt_floor_boundary() {
    let rows = [
        ("SLB-005.1", 3_999_999, (4_000_000, 4_500_000)),
        ("SLB-005.2", 4_000_000, (4_000_000, 4_500_000)),
        ("SLB-005.3", 4_000_001, (4_000_001, 4_500_001)),
        ("SLB-005.4", -1, (4_000_000, 4_500_000)),
    ];
    for (row, floor, (rt, of)) in rows {
        let mut c = CostLine::default();
        c.set(&items4(), Rate(floor)).unwrap();
        assert_eq!((c.round_trip().0, c.open_floor().0), (rt, of), "{row}");
    }
}

#[test]
fn slb_006_set_errors() {
    let d = |n: usize| vec![item(100_000, 0, RT); n];
    let half = i64::MAX / 2;
    type Check = fn(&CostLine) -> bool;
    type Row = (&'static str, Vec<CostItem>, Result<(), CostErr>, Check);
    let rows: Vec<Row> = vec![
        ("SLB-006.1", d(8), Ok(()), |c| c.round_trip().0 == 800_000),
        ("SLB-006.2", d(9), Err(CostErr::TooMany), |_| true),
        ("SLB-006.3", d(0), Ok(()), |c| totals(c) == (0, 0, 0)),
        (
            "SLB-006.4",
            vec![item(100_000, 0, 8)],
            Err(CostErr::BadFlags),
            |_| true,
        ),
        (
            "SLB-006.5",
            vec![item(100_000, 0, RT | FIXED)],
            Err(CostErr::BadFlags),
            |_| true,
        ),
        ("SLB-006.6", vec![item(100_000, 0, FIXED | OOQ)], Ok(()), |c| {
            c.open_floor().0 == 100_000 && c.out_of_quote().0 == 100_000
        }),
        (
            "SLB-006.7",
            vec![item(half + 1, 0, RT); 2],
            Err(CostErr::Overflow),
            |_| true,
        ),
        ("SLB-006.8", vec![item(half, 0, RT); 2], Ok(()), |c| {
            c.round_trip().0 == i64::MAX - 1
        }),
        (
            "SLB-006.9",
            vec![item(i64::MAX, 0, RT), item(1, 0, FIXED)],
            Err(CostErr::Overflow),
            |_| true,
        ),
    ];
    for (row, items, want, check) in rows {
        let mut c = CostLine::default();
        assert_eq!(c.set(&items, Rate(0)), want, "{row}");
        assert!(check(&c), "{row}");
    }
}

#[test]
fn slb_007_set_error_keeps_state() {
    let mut c = CostLine::default();
    c.set(&items4(), Rate(2_000_000)).unwrap();
    let before = c;
    assert!(c.set(&[item(100_000, 0, RT); 9], Rate(0)).is_err());
    assert!(c.set(&[item(100_000, 0, 3)], Rate(0)).is_err());
    assert_eq!(c, before);
    assert_eq!(totals(&c), (4_000_000, 4_500_000, 3_200_000));
    assert_eq!((c.n, c.rt_floor), (5, Rate(2_000_000)));
}

fn leg(inst: u32, account: AccountId, liq: Liquidity) -> FeeLeg {
    FeeLeg {
        inst: InstId(inst),
        account,
        liq,
    }
}

#[test]
fn slb_008_fee_round_trip() {
    use Liquidity::{Maker, Taker};
    let rd = rf::rf1();
    let v = RefView::new(&rd, A1);
    let rows: &[(&str, Vec<FeeLeg>, i64)] = &[
        (
            "SLB-008.1",
            vec![leg(I1.0, A1, Taker), leg(I2.0, A1, Taker)],
            3_000_000,
        ),
        (
            "SLB-008.2",
            vec![leg(I1.0, A1, Taker), leg(I2.0, A1, Maker)],
            1_900_000,
        ),
        ("SLB-008.3", vec![leg(I1.0, A1, Maker)], 0),
        ("SLB-008.4", vec![], 0),
    ];
    for (row, legs, want) in rows {
        assert_eq!(fee_round_trip(&v, legs), Ok(Rate(*want)), "{row}");
    }
}

#[test]
fn slb_009_fee_errors() {
    use FeeErr::{NoFeeTier, UnknownInst};
    use Liquidity::Taker;
    let rd = rf::rf1();
    let v = RefView::new(&rd, A1);
    let rows: &[(&str, Vec<FeeLeg>, FeeErr)] = &[
        ("SLB-009.1", vec![leg(999, A1, Taker)], UnknownInst),
        ("SLB-009.2", vec![leg(I1.0, A2, Taker)], NoFeeTier),
        ("SLB-009.3", vec![leg(I3.0, A1, Taker)], NoFeeTier),
        (
            "SLB-009.4",
            vec![leg(I1.0, A2, Taker), leg(999, A1, Taker)],
            NoFeeTier,
        ),
        (
            "SLB-009.5",
            vec![leg(999, A1, Taker), leg(I1.0, A2, Taker)],
            UnknownInst,
        ),
        (
            "SLB-009.6",
            vec![leg(I1.0, A1, Taker), leg(999, A1, Taker)],
            UnknownInst,
        ),
    ];
    for (row, legs, want) in rows {
        assert_eq!(fee_round_trip(&v, legs), Err(*want), "{row}");
    }
}

#[test]
fn slb_010_fee_asset_rule_ignored() {
    use Liquidity::Taker;
    let legs = [leg(I1.0, A1, Taker), leg(I2.0, A1, Taker)];
    let a = rf::rf1();
    // RF1': only V1 deducts fees in the base asset.
    let b = rf::build(FeeAssetRule::Configured(rf::BTC), &[]);
    assert_ne!(a.caps(rf::V1).fee_asset_rule, b.caps(rf::V1).fee_asset_rule);
    for rd in [&a, &b] {
        assert_eq!(fee_round_trip(&RefView::new(rd, A1), &legs), Ok(Rate(3_000_000)));
    }
}
