//! Load step 5: the capability table (REF-020 – REF-024).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::common;

use common::*;
use hft_refdata::LoadErrCode::{
    CapsMarketMismatch, CapsValue, DeadManRange, MissingCaps, SpotBorrowRequired,
};
use hft_refdata::{DeadManCap, FundingSettleKind, InstKind, MarginData, MarginMode, PositionMode, VenueCaps};
use hft_types::{MarketKind, VenueId};

/// F0 with caps row `row` edited.
fn caps_edit(row: usize, f: impl FnOnce(&mut VenueCaps)) -> Vec<u8> {
    let mut s = f0_spec();
    f(&mut s.caps[row]);
    s.build()
}

#[test]
fn ref_020_missing_caps() {
    let mut s = f0_spec();
    s.caps.retain(|c| c.venue != V5);
    assert_single(&s.build(), MissingCaps, "REF-020.1");
    s.inst_mut(3).venue = V1;
    let rd = ok(&s.build());
    assert_eq!(rd.caps(VenueId(5)).flags.0, 0, "REF-020.2");
}

#[test]
fn ref_021_caps_market_mismatch() {
    let cmm = |sec: u16, row: u32| e(CapsMarketMismatch, sec, row);
    let cases: Vec<(&str, Vec<u8>, Vec<hft_refdata::LoadErr>)> = vec![
        (
            "REF-021.1",
            caps_edit(1, |c| c.market = MarketKind::LinearPerp),
            vec![cmm(13, 1)],
        ),
        (
            "REF-021.2",
            caps_edit(0, |c| c.market = MarketKind::Spot),
            vec![cmm(13, 0)],
        ),
        (
            "REF-021.3",
            {
                let mut s = f0_spec();
                s.venue_mut(2).product = InstKind::Perp;
                s.build()
            },
            vec![cmm(13, 1), cmm(3, 1)],
        ),
        (
            "REF-021.4",
            caps_edit(1, |c| c.required_position_mode = PositionMode::OneWay),
            vec![cmm(13, 1)],
        ),
        (
            "REF-021.5",
            caps_edit(1, |c| c.required_margin_mode = MarginMode::Cross),
            vec![cmm(13, 1)],
        ),
        (
            "REF-021.6",
            caps_edit(1, |c| c.funding_settle = FundingSettleKind::PerSymbolPush),
            vec![cmm(13, 1)],
        ),
        (
            "REF-021.7",
            caps_edit(1, |c| c.margin_data = MarginData::Push),
            vec![cmm(13, 1)],
        ),
        (
            "REF-021.8",
            caps_edit(1, |c| c.margin_data = MarginData::Poll { interval_ms: 60_000 }),
            vec![cmm(13, 1)],
        ),
        (
            "REF-021.9",
            caps_edit(1, |c| c.required_multi_asset = Some(false)),
            vec![cmm(13, 1)],
        ),
        (
            "REF-021.10",
            caps_edit(0, |c| c.required_spot_borrow = Some(false)),
            vec![cmm(13, 0)],
        ),
        (
            "REF-021.11",
            caps_edit(0, |c| c.funding_settle = FundingSettleKind::NotApplicable),
            vec![cmm(13, 0)],
        ),
        (
            "REF-021.12",
            caps_edit(0, |c| c.required_margin_mode = MarginMode::Mixed),
            vec![cmm(13, 0)],
        ),
        (
            "REF-021.13",
            caps_edit(0, |c| c.required_margin_mode = MarginMode::Isolated),
            vec![],
        ),
        (
            "REF-021.14",
            caps_edit(0, |c| c.required_position_mode = PositionMode::NotApplicable),
            vec![],
        ),
        (
            "REF-021.15",
            {
                let mut s = f0_spec();
                s.inst_mut(2).kind = InstKind::Perp;
                s.build()
            },
            vec![cmm(3, 1)],
        ),
    ];
    for (case, b, want) in cases {
        assert_eq!(outcome(load(&b)), want, "{case}");
    }
}

#[test]
fn ref_022_spot_borrow() {
    assert_eq!(
        errs(&caps_edit(3, |c| c.required_spot_borrow = Some(true))),
        vec![e(SpotBorrowRequired, 13, 3)],
        "REF-022.1"
    );
    ok(&caps_edit(3, |c| c.required_spot_borrow = Some(false))); // REF-022.2
    ok(&caps_edit(3, |c| c.required_spot_borrow = None)); // REF-022.3
}

#[test]
fn ref_023_poll_interval() {
    let poll = |ms: u32| caps_edit(0, |c| c.margin_data = MarginData::Poll { interval_ms: ms });
    assert_eq!(errs(&poll(0)), vec![e(CapsValue, 13, 0)], "REF-023.1");
    assert_eq!(errs(&poll(999)), vec![e(CapsValue, 13, 0)], "REF-023.2");
    let rd = ok(&poll(1_000));
    assert_eq!(
        rd.caps(VenueId(1)).margin_data,
        MarginData::Poll { interval_ms: 1000 },
        "REF-023.3"
    );
    ok(&poll(600_000)); // REF-023.4
    assert_eq!(errs(&poll(600_001)), vec![e(CapsValue, 13, 0)], "REF-023.5");
}

#[test]
fn ref_024_dead_man_range() {
    use DeadManCap::*;
    let cases: [(&str, DeadManCap, u32, u32, u32, u32, bool); 13] = [
        (
            "REF-024.1",
            CountdownAccount,
            10_000,
            120_000,
            20_000,
            10_000,
            true,
        ),
        (
            "REF-024.2",
            CountdownAccount,
            10_000,
            120_000,
            20_000,
            10_001,
            false,
        ),
        (
            "REF-024.3",
            CountdownAccount,
            10_000,
            120_000,
            10_000,
            5_000,
            true,
        ),
        (
            "REF-024.4",
            CountdownAccount,
            10_000,
            120_000,
            9_999,
            4_000,
            false,
        ),
        (
            "REF-024.5",
            CountdownAccount,
            10_000,
            120_000,
            120_000,
            10_000,
            true,
        ),
        (
            "REF-024.6",
            CountdownAccount,
            10_000,
            120_000,
            120_001,
            10_000,
            false,
        ),
        ("REF-024.7", CountdownPerInst, 0, 0, 3_600_000, 1_000, true),
        ("REF-024.8", OnDisconnect, 0, 0, 10_000, 0, false),
        ("REF-024.9", None, 0, 0, 0, 0, true),
        ("REF-024.10", None, 1, 0, 0, 0, false),
        ("REF-024.11", None, 0, 1, 0, 0, false),
        ("REF-024.12", None, 0, 0, 1, 0, false),
        ("REF-024.13", None, 0, 0, 0, 1, false),
    ];
    for (case, dms, min, max, timeout, refresh, pass) in cases {
        let b = caps_edit(2, |c| {
            c.dms = dms;
            c.dms_min_ms = min;
            c.dms_max_ms = max;
            c.dms_timeout_ms = timeout;
            c.dms_refresh_ms = refresh;
        });
        let want = if pass {
            vec![]
        } else {
            vec![e(DeadManRange, 13, 2)]
        };
        assert_eq!(outcome(load(&b)), want, "{case}");
    }
}
