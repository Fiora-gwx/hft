//! Protocol defaults: `VenueCaps` (README "VenueCaps 默认值"), rate rules, expiry and dead man.

use hft_refdata::{CapFlags, DeadManCap, FeeAssetRule, LimitBucket, LimitScope, LiqMarker, TestnetKind, VenueCaps};
use hft_types::{AssetId, MarketKind, VenueId};
use hft_venue_proto::{DeadManSpec, RateRule, StaleGuard};

/// Outcome tokens trade like spot: buy with the quote (USDC), sell only what is held.
pub(crate) fn default_caps(venue: VenueId, _asset: impl Fn(&str) -> Option<AssetId>) -> VenueCaps {
    let mut c = VenueCaps::none(venue);
    c.market = MarketKind::Spot;
    c.flags = CapFlags::POST_ONLY | CapFlags::IOC | CapFlags::FOK;
    c.fee_asset_rule = FeeAssetRule::Quote;
    c.reduce_only = false;
    c.required_spot_borrow = Some(false);
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

/// Order entry is not built yet; only the connection and inbound-message limits apply.
static RULES: [RateRule; 2] = [
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
