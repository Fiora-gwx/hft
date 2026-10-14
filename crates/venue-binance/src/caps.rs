//! Protocol defaults: `VenueCaps` (README "VenueCaps 默认值"), rate rules (README "限速默认值"),
//! expiry, dead man and heartbeats (README "心跳与 BBO 探测").

use hft_refdata::{
    CapFlags, DeadManCap, FeeAssetRule, FundingSettleKind, LimitBucket, LimitScope, LiqMarker, MarginData,
    MarginMode, PositionMode, TestnetKind, VenueCaps,
};
use hft_types::{AssetId, MarketKind};
use hft_venue_proto::{DeadManSpec, HeartbeatSpec, PingKind, RateRule, Role, StaleGuard, Via};

use crate::Line;

/// `ASSET_NONE`: an asset name not in refdata (contract 15).
const ASSET_NONE: AssetId = AssetId(0);

const USDM_DMS: (u32, u32, u32) = (5_000, 30_000, 10_000);

pub(crate) fn default_caps(line: Line, asset: impl Fn(&str) -> Option<AssetId>) -> VenueCaps {
    let mut c = VenueCaps::none(line.venue());
    c.flags = CapFlags::AMEND | CapFlags::POST_ONLY | CapFlags::IOC | CapFlags::FOK | CapFlags::BATCH_CANCEL;
    c.testnet = TestnetKind::Demo;
    match line {
        Line::Usdm => {
            c.market = MarketKind::LinearPerp;
            c.fee_asset_rule = FeeAssetRule::Quote;
            c.reduce_only = true;
            c.required_position_mode = PositionMode::OneWay;
            c.required_margin_mode = MarginMode::Cross;
            c.required_multi_asset = Some(false);
            c.required_spot_borrow = None;
            c.funding_settle = FundingSettleKind::RateOnly;
            c.margin_data = MarginData::Poll { interval_ms: 5_000 };
            c.own_liquidation_marker = LiqMarker::ClientIdPrefix;
            c.liq_marker_arg = "autoclose-|adl_autoclose|settlement_autoclose-".into();
            c.ignorable_user_events = [
                "TRADE_LITE",
                "STRATEGY_UPDATE",
                "GRID_UPDATE",
                "CONDITIONAL_ORDER_TRIGGER_REJECT",
            ]
            .iter()
            .map(|s| (*s).into())
            .collect();
            c.amend_qty_down_only = false;
            c.dms = DeadManCap::CountdownPerInst;
            c.dms_min_ms = USDM_DMS.0;
            c.dms_max_ms = 0;
            c.dms_timeout_ms = USDM_DMS.1;
            c.dms_refresh_ms = USDM_DMS.2;
        }
        Line::Spot => {
            c.market = MarketKind::Spot;
            c.fee_asset_rule = FeeAssetRule::ReceivedAssetUnlessDiscount {
                token: asset("BNB").unwrap_or(ASSET_NONE),
            };
            c.reduce_only = false;
            c.ignorable_user_events = ["balanceUpdate", "listStatus", "externalLockUpdate"]
                .iter()
                .map(|s| (*s).into())
                .collect();
            c.amend_qty_down_only = true;
            c.dms = DeadManCap::None;
        }
    }
    c
}

const fn rule(scope: LimitScope, bucket: LimitBucket, window_ms: u32, limit: u32) -> RateRule {
    RateRule {
        scope,
        bucket,
        window_ms,
        limit,
    }
}

static USDM_RULES: [RateRule; 5] = [
    rule(LimitScope::Account, LimitBucket::Orders, 10_000, 300),
    rule(LimitScope::Account, LimitBucket::Orders, 60_000, 1_200),
    rule(LimitScope::Ip, LimitBucket::Weight, 60_000, 2_400),
    rule(LimitScope::Conn, LimitBucket::WsInMsgs, 1_000, 10),
    rule(LimitScope::Ip, LimitBucket::NewConns, 300_000, 300),
];

static SPOT_RULES: [RateRule; 5] = [
    rule(LimitScope::Account, LimitBucket::Orders, 10_000, 100),
    rule(LimitScope::Account, LimitBucket::Orders, 86_400_000, 200_000),
    rule(LimitScope::Ip, LimitBucket::Weight, 60_000, 6_000),
    rule(LimitScope::Conn, LimitBucket::WsInMsgs, 1_000, 5),
    rule(LimitScope::Ip, LimitBucket::NewConns, 300_000, 300),
];

pub(crate) fn rate_rules(line: Line) -> &'static [RateRule] {
    match line {
        Line::Usdm => &USDM_RULES,
        Line::Spot => &SPOT_RULES,
    }
}

pub(crate) fn stale_guard() -> StaleGuard {
    StaleGuard::RecvWindow { default_ms: 5_000 }
}

pub(crate) fn dead_man(line: Line) -> DeadManSpec {
    match line {
        Line::Usdm => DeadManSpec::CountdownPerInst {
            min_ms: USDM_DMS.0,
            timeout_ms: USDM_DMS.1,
            refresh_ms: USDM_DMS.2,
            via: Via::Rest,
        },
        Line::Spot => DeadManSpec::None,
    }
}

pub(crate) fn heartbeat(line: Line, role: Role) -> HeartbeatSpec {
    let idle_timeout_ms = match (line, role) {
        // fstream (market and user stream): server ping every 3 min → twice that.
        (Line::Usdm, Role::Md | Role::UserStream) => 360_000,
        // ws-fapi (until verified), spot stream and ws-api: server ping every 20 s → 40 s.
        _ => 40_000,
    };
    HeartbeatSpec {
        ping: PingKind::WsPing,
        interval_ms: 0,
        unconditional: false,
        pong_timeout_ms: 10_000,
        idle_timeout_ms,
    }
}
