//! Exported layout description of the RDB1 rows and the `LoadErrCode` table (read by
//! `gen-schema` and the Python read-only parser). Offsets come from `offset_of!`, so the export
//! cannot drift from the row structs.

use crate::blob::{
    AccountRow, AccountVenueRow, AliasRow, AssetRow, CapsRow, EndpointRow, FeeOverrideRow, FeeRow, InstRow,
    InstanceRow, MaintRow, MmrTierRow, RateLimitRow, SimAccountRow, SimBalanceRow, StrRef, TokenRow,
    VenueRow, sec,
};
use crate::types::LoadErrCode;

/// One field of a row.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct FieldDesc {
    /// `"<Row>.<field>"`.
    pub name: &'static str,
    pub offset: usize,
    pub size: usize,
    /// Rust type as written in the row struct.
    pub ty: &'static str,
}

/// One RDB1 row format.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct RowDesc {
    pub section: u16,
    pub name: &'static str,
    pub row_ver: u16,
    pub size: usize,
    pub fields: &'static [FieldDesc],
}

macro_rules! desc {
    ($section:expr, $row:ident, $name:literal { $($f:ident : $t:ty),* $(,)? }) => {
        RowDesc {
            section: $section,
            name: $name,
            row_ver: 1,
            size: size_of::<$row>(),
            fields: &[$(FieldDesc {
                name: concat!($name, ".", stringify!($f)),
                offset: core::mem::offset_of!($row, $f),
                size: size_of::<$t>(),
                ty: stringify!($t),
            }),*],
        }
    };
}

/// Every RDB1 row format (Strings rows are bytes and have no description).
pub const RDB1_ROWS: &[RowDesc] = &[
    desc!(sec::ASSETS, AssetRow, "AssetMeta" {
        id: u16, class: u8, px_exp: i8, qty_exp: i8, _pad: [u8; 3], name: StrRef, px_max: i64, qty_max: i64, dust: i64,
    }),
    desc!(sec::VENUES, VenueRow, "VenueMeta" {
        id: u8, exchange: u8, product: u8, ts_unit: u8, stale_guard: u8, client_id_max_len: u8, max_batch: u8,
        max_inflight_amend: u8, ws_max_streams_per_conn: u16, ws_max_in_msgs_per_s: u16,
        ws_new_conn_per_5min_ip: u16, app_ping_interval_s: u16, ws_max_conn_age_s: u32,
        crossed_tolerance_default: u16, _pad: u16, client_id_prefix: StrRef,
    }),
    desc!(sec::INSTS, InstRow, "InstMeta" {
        id: u32, venue: u8, status: u8, kind: u8, flags: u8, asset: u16, quote: u16, book_window: u32,
        px_unit_shift: i8, qty_unit_shift: i8, l2_depth: u16, crossed_tolerance: u16, max_leverage: u16,
        tick: i64, lot: i64, min_qty: i64, max_qty: i64, min_notional: i64, venue_num_id: u64,
        symbol: StrRef, tick_str: StrRef, lot_str: StrRef, min_qty_str: StrRef,
        funding_interval_s: u32, funding_anchor_s: u32, listed_utc_ns: i64, delivery_utc_ns: i64,
        band_up: i64, band_down: i64,
    }),
    desc!(sec::ACCOUNTS, AccountRow, "AccountMeta" {
        id: u8, kind: u8, bound_instance: u8, is_prod: u8, uid_policy: u8, _pad: [u8; 3], venues: u64, label: StrRef,
    }),
    desc!(sec::FEES, FeeRow, "Fee" { venue: u8, tier: u8, _pad: [u8; 6], maker: i64, taker: i64 }),
    desc!(sec::FEE_OVERRIDES, FeeOverrideRow, "FeeOverride" {
        account: u8, venue: u8, _pad: [u8; 2], inst: u32, maker: i64, taker: i64,
    }),
    desc!(sec::RATE_LIMITS, RateLimitRow, "RateLimit" {
        venue: u8, scope: u8, bucket: u8, _pad: u8, limit: u32, window_ms: u32, default_cost: u16, _pad2: u16,
    }),
    desc!(sec::ENDPOINTS, EndpointRow, "Endpoint" {
        venue: u8, role: u8, env: u8, conn_group: u8, _pad: u32, url: StrRef,
    }),
    desc!(sec::MAINT, MaintRow, "MaintWindow" {
        venue: u8, kind: u8, _pad: [u8; 2], inst: u32, start_ns: i64, end_ns: i64,
    }),
    desc!(sec::TOKENS, TokenRow, "ChainToken" { chain: u8, decimals: u8, asset: u16, _pad: u32, address: [u8; 32] }),
    desc!(sec::ALIASES, AliasRow, "Alias" { venue: u8, _pad: [u8; 3], inst: u32, alias: StrRef }),
    desc!(sec::VENUE_CAPS, CapsRow, "VenueCaps" {
        venue: u8, market: u8, fee_rule: u8, reduce_only: u8, position_mode: u8, margin_mode: u8,
        multi_asset: u8, spot_borrow: u8, funding_settle: u8, margin_data: u8, liq_marker: u8, dms: u8,
        testnet: u8, amend_qty_down_only: u8, fee_asset: u16, margin_poll_ms: u32, flags: u32,
        dms_min_ms: u32, dms_max_ms: u32, dms_timeout_ms: u32, dms_refresh_ms: u32,
        liq_marker_arg: StrRef, events_off: u32, events_count: u32,
    }),
    desc!(sec::ACCOUNT_VENUES, AccountVenueRow, "AccountVenue" { account: u8, venue: u8, _pad: [u8; 6], uid: StrRef }),
    desc!(sec::INSTANCES, InstanceRow, "InstanceMeta" { id: u8, _pad: u8, kind_id: u16, _pad2: u32, name: StrRef }),
    desc!(sec::SIM_ACCOUNTS, SimAccountRow, "SimAccount" {
        account: u8, fee_tier: u8, latency_model: u16, margin_model: u16, _pad: u16,
    }),
    desc!(sec::SIM_BALANCES, SimBalanceRow, "SimBalance" { account: u8, venue: u8, asset: u16, _pad: u32, qty: i64 }),
    desc!(sec::MMR_TIERS, MmrTierRow, "MmrTier" { inst: u32, _pad: u32, notional_max: i64, mmr: i64 }),
    desc!(sec::STR_REFS, StrRef, "StrRef" { off: u32, len: u32 }),
];

/// `(name, value)` of every `LoadErrCode`, in value order.
pub fn load_err_codes() -> impl Iterator<Item = (&'static str, u16)> {
    LoadErrCode::ALL.iter().map(|c| (c.name(), *c as u16))
}
