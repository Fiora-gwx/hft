//! Protocol defaults: `VenueCaps` (README "VenueCaps 默认值") and rate rules (README "限速默认值").

use hft_refdata::{DeadManCap, FeeAssetRule, LimitBucket, LimitScope, LiqMarker, TestnetKind, VenueCaps};
use hft_types::{AssetId, MarketKind};
use hft_venue_proto::RateRule;

use crate::VENUE;

/// Read-only: no capability bit, no dead man, no account requirement.
pub(crate) fn default_caps(_asset: impl Fn(&str) -> Option<AssetId>) -> VenueCaps {
    let mut c = VenueCaps::none(VENUE);
    c.market = MarketKind::Spot;
    c.fee_asset_rule = FeeAssetRule::Quote;
    c.reduce_only = false;
    c.own_liquidation_marker = LiqMarker::NotIdentifiable;
    c.testnet = TestnetKind::None;
    c.dms = DeadManCap::None;
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

/// No published limits; the defaults stay far above what one connection sends (a login, a few
/// subscriptions, a ping every 5 s) and keep reconnects at most one per second.
static RULES: [RateRule; 2] = [
    rule(LimitScope::Conn, LimitBucket::WsInMsgs, 1_000, 10),
    rule(LimitScope::Ip, LimitBucket::NewConns, 1_000, 1),
];

pub(crate) fn rate_rules() -> &'static [RateRule] {
    &RULES
}
