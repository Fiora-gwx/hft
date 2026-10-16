//! Protocol defaults: `VenueCaps` (README "VenueCaps 默认值"), rate rules (README "限速默认值"),
//! expiry, dead man and heartbeats (README "心跳与 BBO 探测").

use hft_refdata::{
    CapFlags, DeadManCap, FeeAssetRule, FundingSettleKind, LimitBucket, LimitScope, LiqMarker, MarginData,
    MarginMode, PositionMode, TestnetKind, VenueCaps,
};
use hft_types::{AssetId, MarketKind};
use hft_venue_proto::{DeadManSpec, HeartbeatSpec, PingKind, RateRule, Role, StaleGuard};

use crate::Line;

/// `ASSET_NONE`: an asset name not in refdata (contract 15).
const ASSET_NONE: AssetId = AssetId(0);

/// Own-liquidation `tradeSide` values (README "外部成交识别"; official enumeration page): hedge
/// mode, then one-way mode.
pub(crate) const LIQ_MARKER_ARG: &str = "burst_close_long|burst_close_short|reduce_close_long|reduce_close_short|\
dte_sys_adl_close_long|dte_sys_adl_close_short|delivery_close_long|delivery_close_short|\
burst_buy_single|burst_sell_single|reduce_buy_single|reduce_sell_single|\
dte_sys_adl_buy_in_single_side_mode|dte_sys_adl_sell_in_single_side_mode|delivery_buy_single|delivery_sell_single";

pub(crate) fn default_caps(line: Line, asset: impl Fn(&str) -> Option<AssetId>) -> VenueCaps {
    let mut c = VenueCaps::none(line.venue());
    // No AMEND: a `modify-order` answer only acknowledges receipt; the outcome is asynchronous and
    // a failure is never reported (README "下单:UTA V3 WS").
    c.flags = CapFlags::POST_ONLY | CapFlags::IOC | CapFlags::FOK;
    c.testnet = TestnetKind::Demo;
    c.dms = DeadManCap::None;
    match line {
        Line::Fut => {
            c.flags = c.flags | CapFlags::POSITION_CLOSE | CapFlags::POSITION_CLOSE_ALL;
            c.market = MarketKind::LinearPerp;
            c.fee_asset_rule = FeeAssetRule::Quote;
            c.reduce_only = true;
            c.required_position_mode = PositionMode::OneWay;
            c.required_margin_mode = MarginMode::Cross;
            c.required_multi_asset = Some(false);
            c.required_spot_borrow = None;
            c.funding_settle = FundingSettleKind::RateOnly;
            c.margin_data = MarginData::Poll { interval_ms: 5_000 };
            c.own_liquidation_marker = LiqMarker::OrderTypeField;
            c.liq_marker_arg = LIQ_MARKER_ARG.into();
        }
        Line::Spot => {
            c.market = MarketKind::Spot;
            c.fee_asset_rule = FeeAssetRule::ReceivedAssetUnlessDiscount {
                token: asset("BGB").unwrap_or(ASSET_NONE),
            };
            c.reduce_only = false;
            c.required_spot_borrow = Some(false);
            c.own_liquidation_marker = LiqMarker::NotIdentifiable;
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

static RULES: [RateRule; 3] = [
    rule(LimitScope::Uid, LimitBucket::Orders, 1_000, 10),
    rule(LimitScope::Ip, LimitBucket::NewConns, 300_000, 300),
    rule(LimitScope::Conn, LimitBucket::WsInMsgs, 1_000, 10),
];

pub(crate) fn rate_rules() -> &'static [RateRule] {
    &RULES
}

pub(crate) fn stale_guard() -> StaleGuard {
    StaleGuard::None
}

pub(crate) fn dead_man() -> DeadManSpec {
    DeadManSpec::None
}

/// Application `ping` every 25 s whatever the inbound traffic (README "实现说明" 1): the server
/// resets connections that sent no `ping` for ~150 s even under full traffic.
pub(crate) fn heartbeat(_role: Role) -> HeartbeatSpec {
    HeartbeatSpec {
        ping: PingKind::AppText,
        interval_ms: 25_000,
        unconditional: true,
        pong_timeout_ms: 10_000,
        idle_timeout_ms: 30_000,
    }
}

/// The application ping frame.
pub(crate) const PING: &[u8] = b"ping";
