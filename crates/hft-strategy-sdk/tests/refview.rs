//! `RefView` (SDK-208).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod common;

use common::*;
use hft_refdata::InstKind;
use hft_strategy_sdk::RefView;
use hft_types::{InstId, MarketKind};

#[test]
fn sdk_208_ref_view() {
    let rd = gt3_refdata();
    let r = RefView::new(&rd, A1);
    // `InstHot.kind` is the product kind; the market kind of the venue is `VenueCaps.market`.
    assert_eq!(r.inst(I2).kind, InstKind::Perp, "SDK-208.1");
    assert_eq!(
        r.caps(r.inst(I2).venue).market,
        MarketKind::LinearPerp,
        "SDK-208.1"
    );
    assert_eq!(r.insts_of(BTC_ID), &[I1, I2, I3, I4], "SDK-208.2");
    assert_eq!(r.insts_of(ASSET2), &[I5], "SDK-208.3");
    assert_eq!(r.account().id, A1, "SDK-208.4");
    assert_eq!(
        (r.caps(V2).venue, r.caps(V2).market),
        (V2, MarketKind::LinearPerp),
        "SDK-208.5"
    );
    let fee = r.fee(A1, I1);
    assert_eq!((fee.maker.0, fee.taker.0), (200_000, 500_000), "SDK-208.6");
    assert!(
        r.try_inst(I2).is_some_and(|h| core::ptr::eq(h, r.inst(I2))),
        "SDK-208.7"
    );
    assert!(r.try_inst(InstId(999)).is_none(), "SDK-208.8");
    assert_eq!(r.try_fee(A1, I1), Some(fee), "SDK-208.9");
    assert_eq!(r.try_fee(A2, I1), None, "SDK-208.10");
    assert_eq!(r.try_fee(A1, InstId(999)), None, "SDK-208.11");
    // I3 (V3) and I4 (V4) are BTC insts outside A1's venues.
    for i in [I3, I4] {
        assert!(r.insts_of(BTC_ID).contains(&i));
        assert_eq!(r.try_fee(A1, i), None, "SDK-208.12 {i:?}");
    }
}
