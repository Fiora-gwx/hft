//! `SetParams` checks: binding and group consistency (REF-058 – REF-060).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::common;

use common::*;
use hft_refdata::LoadErrCode::{
    AccountBound, KindMismatch, LegAssetMismatch, LegNotTradable, LegVenueNotInAccount, QuoteMismatch,
    SameExchangeLegs,
};
use hft_refdata::{GroupSpecView, InstKind, InstStatus, LegView, LoadErrCode};
use hft_types::{AccountId, AssetId, InstId, StrategyId, VenueId};

#[test]
fn ref_058_check_binding() {
    let rd = ok(&f0());
    let mut s = f0_spec();
    s.account_mut(1).bound_instance = StrategyId(0);
    let rd0 = ok(&s.build());
    let code = |r: Result<(), hft_refdata::LoadErr>| r.err().map(|e| e.code);
    assert_eq!(
        rd.check_binding(AccountId(1), StrategyId(1), 0xFF00),
        Ok(()),
        "REF-058.1"
    );
    assert_eq!(
        code(rd.check_binding(AccountId(1), StrategyId(3), 0xFF01)),
        Some(AccountBound),
        "REF-058.2"
    );
    assert_eq!(
        code(rd0.check_binding(AccountId(1), StrategyId(1), 0xFF00)),
        Some(AccountBound),
        "REF-058.3"
    );
    assert_eq!(
        code(rd.check_binding(AccountId(1), StrategyId(1), 0xFF01)),
        Some(KindMismatch),
        "REF-058.4"
    );
    assert_eq!(
        code(rd.check_binding(AccountId(9), StrategyId(1), 0xFF00)),
        Some(AccountBound),
        "REF-058.5"
    );
}

fn leg(inst: u32, venue: u8) -> LegView {
    LegView {
        inst: InstId(inst),
        venue: VenueId(venue),
    }
}

#[test]
fn ref_059_check_group_ok() {
    let rd = ok(&f0());
    let legs = [leg(1, 1), leg(4, 6)];
    let g = GroupSpecView {
        asset: BTC,
        legs: &legs,
    };
    assert_eq!(rd.check_group(AccountId(1), &g), Ok(AssetId(2)));
    let g1 = GroupSpecView {
        asset: BTC,
        legs: &legs[..1],
    };
    assert_eq!(rd.check_group(AccountId(1), &g1), Ok(AssetId(2)));
}

#[test]
fn ref_060_check_group_rejects() {
    let rd_with = |status: InstStatus| {
        let mut s = f0_spec();
        s.inst_mut(4).status = status;
        let mut i6 = inst(
            6,
            "BTC-USDC",
            V6,
            InstKind::Spot,
            BTC,
            ("0.1", 10_000_000),
            ("0.00000001", 1),
            ("0.00001", 1_000),
        );
        i6.quote = USDC;
        s.insts.push(i6);
        ok(&s.build())
    };
    let run = |status: InstStatus, asset: AssetId, legs: &[LegView]| -> Result<AssetId, LoadErrCode> {
        rd_with(status)
            .check_group(AccountId(1), &GroupSpecView { asset, legs })
            .map_err(|e| e.code)
    };
    use InstStatus::*;
    assert_eq!(
        run(Trading, BTC, &[leg(1, 1), leg(99, 6)]),
        Err(LegNotTradable),
        "REF-060.1"
    );
    assert_eq!(
        run(Halted, BTC, &[leg(1, 1), leg(4, 6)]),
        Err(LegNotTradable),
        "REF-060.2"
    );
    assert_eq!(
        run(Delisted, BTC, &[leg(1, 1), leg(4, 6)]),
        Err(LegNotTradable),
        "REF-060.3"
    );
    assert_eq!(
        run(PreTrading, BTC, &[leg(1, 1), leg(4, 6)]),
        Err(LegNotTradable),
        "REF-060.4"
    );
    assert_eq!(
        run(Maintenance, BTC, &[leg(1, 1), leg(4, 6)]),
        Err(LegNotTradable),
        "REF-060.5"
    );
    assert_eq!(
        run(ReduceOnly, BTC, &[leg(1, 1), leg(4, 6)]),
        Ok(USDT),
        "REF-060.6"
    );
    assert_eq!(
        run(Trading, BTC, &[leg(1, 1), leg(4, 5)]),
        Err(LegVenueNotInAccount),
        "REF-060.7"
    );
    assert_eq!(
        run(Trading, BTC, &[leg(1, 1), leg(3, 5)]),
        Err(LegVenueNotInAccount),
        "REF-060.8"
    );
    assert_eq!(
        run(Trading, ETH, &[leg(1, 1), leg(4, 6)]),
        Err(LegAssetMismatch),
        "REF-060.9"
    );
    assert_eq!(
        run(Trading, BTC, &[leg(1, 1), leg(6, 6)]),
        Err(QuoteMismatch),
        "REF-060.10"
    );
    assert_eq!(
        run(Trading, BTC, &[leg(1, 1), leg(1, 1)]),
        Err(SameExchangeLegs),
        "REF-060.11"
    );
    assert_eq!(
        run(Trading, BTC, &[leg(99, 1), leg(4, 5)]),
        Err(LegNotTradable),
        "REF-060.12"
    );
    assert_eq!(
        run(Trading, ETH, &[leg(1, 1), leg(1, 1)]),
        Err(LegAssetMismatch),
        "REF-060.13"
    );
}
