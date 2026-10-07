//! BIN-170–BIN-172: rate rules, capability defaults, expiry and dead man.

mod support;

use hft_refdata::{
    CapFlags, DeadManCap, FeeAssetRule, FundingSettleKind, LimitBucket as B, LimitScope as S, LiqMarker,
    MarginData, MarginMode, PositionMode, TestnetKind, VenueCaps,
};
use hft_types::{MarketKind, VenueId};
use hft_venue_proto::{DeadManSpec, RateRule, StaleGuard, VenueProtocol, Via};
use support::BNB;
use venue_binance::{Spot, Usdm};

fn r(scope: S, bucket: B, window_ms: u32, limit: u32) -> RateRule {
    RateRule {
        scope,
        bucket,
        window_ms,
        limit,
    }
}

#[test]
fn bin_170_rate_rules() {
    let u = Usdm::rate_rules();
    let s = Spot::rate_rules();
    assert_eq!((u.len(), s.len()), (5, 5));
    let rows = [
        ("BIN-170.1", u, r(S::Account, B::Orders, 10_000, 300)),
        ("BIN-170.2", u, r(S::Account, B::Orders, 60_000, 1_200)),
        ("BIN-170.3", u, r(S::Ip, B::Weight, 60_000, 2_400)),
        ("BIN-170.4", u, r(S::Conn, B::WsInMsgs, 1_000, 10)),
        ("BIN-170.5", u, r(S::Ip, B::NewConns, 300_000, 300)),
        ("BIN-170.6", s, r(S::Account, B::Orders, 10_000, 100)),
        ("BIN-170.7", s, r(S::Account, B::Orders, 86_400_000, 200_000)),
        ("BIN-170.8", s, r(S::Ip, B::Weight, 60_000, 6_000)),
        ("BIN-170.9", s, r(S::Conn, B::WsInMsgs, 1_000, 5)),
        ("BIN-170.10", s, r(S::Ip, B::NewConns, 300_000, 300)),
    ];
    for (row, table, want) in rows {
        assert!(table.contains(&want), "{row}");
    }
}

fn caps() -> (VenueCaps, VenueCaps) {
    let asset = |n: &str| (n == "BNB").then_some(BNB);
    (Usdm::default_caps(asset), Spot::default_caps(asset))
}

#[test]
fn bin_171_default_caps() {
    let (u, s) = caps();
    assert_eq!(
        (u.venue, u.market, s.venue, s.market),
        (
            VenueId::BINANCE_USDM,
            MarketKind::LinearPerp,
            VenueId::BINANCE_SPOT,
            MarketKind::Spot
        ),
        "BIN-171.1"
    );
    assert_eq!(
        (u.fee_asset_rule, s.fee_asset_rule),
        (
            FeeAssetRule::Quote,
            FeeAssetRule::ReceivedAssetUnlessDiscount { token: BNB }
        ),
        "BIN-171.2"
    );
    assert_eq!((u.reduce_only, s.reduce_only), (true, false), "BIN-171.3");
    assert_eq!(
        (u.required_position_mode, s.required_position_mode),
        (PositionMode::OneWay, PositionMode::NotApplicable),
        "BIN-171.4"
    );
    assert_eq!(
        (u.required_margin_mode, s.required_margin_mode),
        (MarginMode::Cross, MarginMode::NotApplicable),
        "BIN-171.5"
    );
    assert_eq!(
        (u.required_multi_asset, s.required_multi_asset),
        (Some(false), None),
        "BIN-171.6"
    );
    assert_eq!(
        (u.required_spot_borrow, s.required_spot_borrow),
        (None, None),
        "BIN-171.7"
    );
    assert_eq!(
        (u.funding_settle, s.funding_settle),
        (
            FundingSettleKind::BalanceOnlyPush,
            FundingSettleKind::NotApplicable
        ),
        "BIN-171.8"
    );
    assert_eq!(
        (u.margin_data, s.margin_data),
        (MarginData::Poll { interval_ms: 5000 }, MarginData::NotApplicable),
        "BIN-171.9"
    );
    assert_eq!(
        (u.own_liquidation_marker, &*u.liq_marker_arg),
        (
            LiqMarker::ClientIdPrefix,
            "autoclose-|adl_autoclose|settlement_autoclose-"
        ),
        "BIN-171.10"
    );
    assert_eq!(
        (s.own_liquidation_marker, &*s.liq_marker_arg),
        (LiqMarker::NotIdentifiable, ""),
        "BIN-171.10"
    );
    let names = |c: &VenueCaps| {
        c.ignorable_user_events
            .iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(&u),
        [
            "TRADE_LITE",
            "STRATEGY_UPDATE",
            "GRID_UPDATE",
            "CONDITIONAL_ORDER_TRIGGER_REJECT"
        ],
        "BIN-171.11"
    );
    assert_eq!(
        names(&s),
        ["balanceUpdate", "listStatus", "externalLockUpdate"],
        "BIN-171.11"
    );
    assert_eq!((u.flags, s.flags), (CapFlags(31), CapFlags(31)), "BIN-171.12");
    assert_eq!(
        (
            u.dms,
            u.dms_min_ms,
            u.dms_max_ms,
            u.dms_timeout_ms,
            u.dms_refresh_ms
        ),
        (DeadManCap::CountdownPerInst, 5000, 0, 30_000, 10_000),
        "BIN-171.13"
    );
    assert_eq!(
        (
            s.dms,
            s.dms_min_ms,
            s.dms_max_ms,
            s.dms_timeout_ms,
            s.dms_refresh_ms
        ),
        (DeadManCap::None, 0, 0, 0, 0),
        "BIN-171.13"
    );
    assert_eq!(
        (u.testnet, s.testnet),
        (TestnetKind::Demo, TestnetKind::Demo),
        "BIN-171.14"
    );
    assert_eq!(
        (u.amend_qty_down_only, s.amend_qty_down_only),
        (false, true),
        "BIN-171.15"
    );
}

#[test]
fn bin_172_stale_guard_dead_man() {
    assert_eq!(
        Usdm::stale_guard(),
        StaleGuard::RecvWindow { default_ms: 5000 },
        "BIN-172.1"
    );
    assert_eq!(
        Spot::stale_guard(),
        StaleGuard::RecvWindow { default_ms: 5000 },
        "BIN-172.2"
    );
    let dm = Usdm::dead_man();
    assert_eq!(
        dm,
        DeadManSpec::CountdownPerInst {
            min_ms: 5000,
            timeout_ms: 30_000,
            refresh_ms: 10_000,
            via: Via::Rest
        },
        "BIN-172.3"
    );
    if let DeadManSpec::CountdownPerInst {
        min_ms,
        timeout_ms,
        refresh_ms,
        ..
    } = dm
    {
        assert!(timeout_ms >= min_ms.max(2 * refresh_ms), "BIN-172.3");
    }
    assert_eq!(Spot::dead_man(), DeadManSpec::None, "BIN-172.4");
    let (u, _) = caps();
    if let DeadManSpec::CountdownPerInst {
        min_ms,
        timeout_ms,
        refresh_ms,
        ..
    } = dm
    {
        assert_eq!(
            (u.dms_min_ms, u.dms_timeout_ms, u.dms_refresh_ms, u.dms_max_ms),
            (min_ms, timeout_ms, refresh_ms, 0),
            "BIN-172.5"
        );
    }
}
