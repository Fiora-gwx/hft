//! Protocol defaults: `VenueCaps` (README "VenueCaps 默认值"), rate rules (README "限速默认值")
//! and heartbeats (README "心跳与 BBO 探测").

use hft_refdata::{DeadManCap, FeeAssetRule, LimitBucket, LimitScope, LiqMarker, TestnetKind, VenueCaps};
use hft_types::{AssetId, MarketKind};
use hft_venue_proto::{HeartbeatSpec, PingKind, RateRule, Role};

use crate::VENUE;

/// Read-only: no capability bit, no dead man, no account requirement.
pub(crate) fn default_caps(_asset: impl Fn(&str) -> Option<AssetId>) -> VenueCaps {
    let mut c = VenueCaps::none(VENUE);
    c.market = MarketKind::Spot;
    c.fee_asset_rule = FeeAssetRule::Quote;
    c.reduce_only = false;
    c.own_liquidation_marker = LiqMarker::NotIdentifiable;
    // The exchange has a sandbox; this crate does not use it (README "VenueCaps 默认值").
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

/// Official "Exchange WebSocket Rate Limits" (2026-11-04 page): 8 requests/s per IP (burst 20),
/// inbound messages 10/s; new connections counted against the 8/s (待核实, README).
static RULES: [RateRule; 3] = [
    rule(LimitScope::Ip, LimitBucket::WsInMsgs, 1_000, 8),
    rule(LimitScope::Conn, LimitBucket::WsInMsgs, 1_000, 10),
    rule(LimitScope::Ip, LimitBucket::NewConns, 1_000, 8),
];

pub(crate) fn rate_rules() -> &'static [RateRule] {
    &RULES
}

/// Md (`matches`) connections: a WS Ping after 5 s without data, answered within 5 s by its Pong
/// or by any `heartbeat` (one per product per second, `ControlHint::Pong`); no idle timeout, a
/// quiet market is not a dead connection. BBO connections use `feed.bbo_ping_ms` / `bbo_dead_ms`
/// instead (hft-venue §3a).
pub(crate) fn heartbeat(_role: Role) -> HeartbeatSpec {
    HeartbeatSpec {
        ping: PingKind::WsPing,
        interval_ms: 5_000,
        unconditional: false,
        pong_timeout_ms: 5_000,
        idle_timeout_ms: 0,
    }
}
