//! `RdbBuilder` / `EcfBuilder`: the only encoders of RDB1 and ECF1 (tools/hft-cfg and test
//! fixtures use them). Output is a pure function of the calls: same calls in the same order →
//! same bytes.

use hft_types::{AccountId, AssetId, HostClass, InstId, Notional, Qty, StrategyInstanceId, VenueId};

use crate::blob::{
    AccountRow, AccountVenueRow, AliasRow, AssetRow, CapsRow, ECF_MAGIC, EndpointRow, FORMAT, FeeOverrideRow,
    FeeRow, InstRow, InstanceRow, MaintRow, MmrTierRow, RDB_MAGIC, RateLimitRow, RawBlob, RawSection,
    SimAccountRow, SimBalanceRow, StrRef, TokenRow, VenueRow, sec,
};
use crate::caps::{VenueCaps, opt_bool_code};
use crate::engine::EngineSection;
use crate::types::{
    AssetMeta, ChainToken, Endpoint, FeePair, InstKind, InstStatus, InstanceMeta, MaintWindow, MmrTier,
    PriceBand, RateLimit, UidPolicy, VenueMeta,
};
use zerocopy::{Immutable, IntoBytes};

/// Builder input of an asset row.
pub type AssetSpec = AssetMeta;
/// Builder input of a venue row.
pub type VenueSpec = VenueMeta;
/// Builder input of a strategy instance row.
pub type InstanceSpec = InstanceMeta;

/// Builder input of an inst row (hot and cold fields; wire exponents are computed at load).
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct InstSpec {
    pub id: InstId,
    pub venue: VenueId,
    pub symbol: String,
    pub status: InstStatus,
    pub kind: InstKind,
    pub flags: u8,
    pub asset: AssetId,
    pub quote: AssetId,
    pub px_unit_shift: i8,
    pub qty_unit_shift: i8,
    pub tick: i64,
    pub lot: i64,
    pub min_qty: Qty,
    pub max_qty: Qty,
    pub min_notional: Notional,
    pub book_window: u32,
    pub l2_depth: u16,
    pub crossed_tolerance: u16,
    pub venue_num_id: u64,
    pub tick_str: String,
    pub lot_str: String,
    pub min_qty_str: String,
    pub funding_interval_s: u32,
    pub funding_anchor_s: u32,
    pub max_leverage: u16,
    pub listed_utc_ns: i64,
    pub delivery_utc_ns: i64,
    pub price_band: PriceBand,
}

impl Default for InstSpec {
    fn default() -> Self {
        InstSpec {
            id: InstId(0),
            venue: VenueId(0),
            symbol: String::new(),
            status: InstStatus::Trading,
            kind: InstKind::Spot,
            flags: 0,
            asset: AssetId(0),
            quote: AssetId(0),
            px_unit_shift: 0,
            qty_unit_shift: 0,
            tick: 0,
            lot: 0,
            min_qty: Qty(0),
            max_qty: Qty(0),
            min_notional: Notional(0),
            book_window: 0,
            l2_depth: 0,
            crossed_tolerance: 0,
            venue_num_id: 0,
            tick_str: String::new(),
            lot_str: String::new(),
            min_qty_str: String::new(),
            funding_interval_s: 0,
            funding_anchor_s: 0,
            max_leverage: 0,
            listed_utc_ns: 0,
            delivery_utc_ns: 0,
            price_band: PriceBand::default(),
        }
    }
}

/// Builder input of an account row (`uid_policy` defaults to `Verified`).
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct AccountSpec {
    pub id: AccountId,
    /// `false` = Live, `true` = Sim (its `SimAccountCfg` comes from `sim` / `sim_balance`).
    pub sim: bool,
    pub bound_instance: StrategyInstanceId,
    pub is_prod: bool,
    pub venues: u64,
    pub label: String,
    pub uid_policy: UidPolicy,
}

impl Default for AccountSpec {
    fn default() -> Self {
        AccountSpec {
            id: AccountId(0),
            sim: false,
            bound_instance: hft_types::StrategyId(0),
            is_prod: false,
            venues: 0,
            label: String::new(),
            uid_policy: UidPolicy::Verified,
        }
    }
}

/// Exchange UID of a Live account on a venue (AccountVenues).
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct AccountVenueSpec {
    pub account: AccountId,
    pub venue: VenueId,
    pub uid: String,
}

/// Fixed part of a Sim account (SimAccounts).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct SimSpec {
    pub account: AccountId,
    pub fee_tier: u8,
    pub latency_model: u16,
    pub margin_model: u16,
}

/// One initial balance of a Sim account (SimBalances).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct SimBalanceSpec {
    pub account: AccountId,
    pub venue: VenueId,
    pub asset: AssetId,
    pub qty: Qty,
}

/// Fees row `(venue, tier)`.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct FeeSpec {
    pub venue: VenueId,
    pub tier: u8,
    pub fee: FeePair,
}

/// FeeOverrides row; `inst = InstId(0)` = every inst of the venue.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct FeeOverrideSpec {
    pub account: AccountId,
    pub venue: VenueId,
    pub inst: InstId,
    pub fee: FeePair,
}

/// Aliases row.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct AliasSpec {
    pub venue: VenueId,
    pub alias: String,
    pub inst: InstId,
}

/// MmrTiers row.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct MmrTierSpec {
    pub inst: InstId,
    pub tier: MmrTier,
}

/// The RDB1 encoder.
#[derive(Clone, Debug)]
pub struct RdbBuilder {
    version: u64,
    created_utc_ns: i64,
    strings: Vec<u8>,
    strrefs: Vec<StrRef>,
    assets: Vec<AssetRow>,
    venues: Vec<VenueRow>,
    insts: Vec<InstRow>,
    accounts: Vec<AccountRow>,
    fees: Vec<FeeRow>,
    fee_overrides: Vec<FeeOverrideRow>,
    limits: Vec<RateLimitRow>,
    endpoints: Vec<EndpointRow>,
    maint: Vec<MaintRow>,
    tokens: Vec<TokenRow>,
    aliases: Vec<AliasRow>,
    caps: Vec<CapsRow>,
    account_venues: Vec<AccountVenueRow>,
    instances: Vec<InstanceRow>,
    sim_accounts: Vec<SimAccountRow>,
    sim_balances: Vec<SimBalanceRow>,
    mmr: Vec<MmrTierRow>,
}

fn section<T: IntoBytes + Immutable>(id: u16, rows: &[T]) -> Option<RawSection> {
    (!rows.is_empty()).then(|| RawSection {
        id,
        row_ver: 1,
        row_size: size_of::<T>() as u32,
        count: rows.len() as u32,
        flags: 0,
        data: rows.as_bytes().to_vec(),
    })
}

impl RdbBuilder {
    pub fn new(version: u64) -> Self {
        RdbBuilder {
            version,
            created_utc_ns: 0,
            strings: Vec::new(),
            strrefs: Vec::new(),
            assets: Vec::new(),
            venues: Vec::new(),
            insts: Vec::new(),
            accounts: Vec::new(),
            fees: Vec::new(),
            fee_overrides: Vec::new(),
            limits: Vec::new(),
            endpoints: Vec::new(),
            maint: Vec::new(),
            tokens: Vec::new(),
            aliases: Vec::new(),
            caps: Vec::new(),
            account_venues: Vec::new(),
            instances: Vec::new(),
            sim_accounts: Vec::new(),
            sim_balances: Vec::new(),
            mmr: Vec::new(),
        }
    }

    /// Header `created_utc_ns` (supplied by the caller; the builder reads no clock).
    pub fn created_utc_ns(&mut self, ns: i64) -> &mut Self {
        self.created_utc_ns = ns;
        self
    }

    fn s(&mut self, s: &str) -> StrRef {
        let r = StrRef {
            off: self.strings.len() as u32,
            len: s.len() as u32,
        };
        self.strings.extend_from_slice(s.as_bytes());
        r
    }

    pub fn asset(&mut self, a: AssetSpec) -> AssetId {
        let name = self.s(&a.name);
        self.assets.push(AssetRow {
            id: a.id.0,
            class: a.class as u8,
            px_exp: a.exp.px_exp,
            qty_exp: a.exp.qty_exp,
            _pad: [0; 3],
            name,
            px_max: a.px_max.0,
            qty_max: a.qty_max.0,
            dust: a.dust.0,
        });
        a.id
    }

    pub fn venue(&mut self, v: VenueSpec) -> VenueId {
        let client_id_prefix = self.s(&v.client_id_prefix);
        self.venues.push(VenueRow {
            id: v.id.0,
            exchange: v.exchange as u8,
            product: v.product as u8,
            ts_unit: v.ts_unit as u8,
            stale_guard: v.stale_guard as u8,
            client_id_max_len: v.client_id_max_len,
            max_batch: v.max_batch,
            max_inflight_amend: v.max_inflight_amend,
            ws_max_streams_per_conn: v.ws_max_streams_per_conn,
            ws_max_in_msgs_per_s: v.ws_max_in_msgs_per_s,
            ws_new_conn_per_5min_ip: v.ws_new_conn_per_5min_ip,
            app_ping_interval_s: v.app_ping_interval_s,
            ws_max_conn_age_s: v.ws_max_conn_age_s,
            crossed_tolerance_default: v.crossed_tolerance_default,
            _pad: 0,
            client_id_prefix,
        });
        v.id
    }

    /// The effective capability row of a venue (protocol defaults with TOML overrides applied).
    pub fn caps(&mut self, c: VenueCaps) {
        let liq_marker_arg = self.s(&c.liq_marker_arg);
        let events_off = self.strrefs.len() as u32;
        for ev in c.ignorable_user_events.iter() {
            let r = self.s(ev);
            self.strrefs.push(r);
        }
        let poll = match c.margin_data {
            crate::caps::MarginData::Poll { interval_ms } => interval_ms,
            _ => 0,
        };
        self.caps.push(CapsRow {
            venue: c.venue.0,
            market: c.market as u8,
            fee_rule: c.fee_asset_rule.code(),
            reduce_only: u8::from(c.reduce_only),
            position_mode: c.required_position_mode as u8,
            margin_mode: c.required_margin_mode as u8,
            multi_asset: opt_bool_code(c.required_multi_asset),
            spot_borrow: opt_bool_code(c.required_spot_borrow),
            funding_settle: c.funding_settle as u8,
            margin_data: c.margin_data.code(),
            liq_marker: c.own_liquidation_marker as u8,
            dms: c.dms as u8,
            testnet: c.testnet as u8,
            amend_qty_down_only: u8::from(c.amend_qty_down_only),
            fee_asset: c.fee_asset_rule.asset().0,
            margin_poll_ms: poll,
            flags: c.flags.0,
            dms_min_ms: c.dms_min_ms,
            dms_max_ms: c.dms_max_ms,
            dms_timeout_ms: c.dms_timeout_ms,
            dms_refresh_ms: c.dms_refresh_ms,
            liq_marker_arg,
            events_off,
            events_count: c.ignorable_user_events.len() as u32,
        });
    }

    pub fn inst(&mut self, i: InstSpec) -> InstId {
        let symbol = self.s(&i.symbol);
        let tick_str = self.s(&i.tick_str);
        let lot_str = self.s(&i.lot_str);
        let min_qty_str = self.s(&i.min_qty_str);
        self.insts.push(InstRow {
            id: i.id.0,
            venue: i.venue.0,
            status: i.status as u8,
            kind: i.kind as u8,
            flags: i.flags,
            asset: i.asset.0,
            quote: i.quote.0,
            book_window: i.book_window,
            px_unit_shift: i.px_unit_shift,
            qty_unit_shift: i.qty_unit_shift,
            l2_depth: i.l2_depth,
            crossed_tolerance: i.crossed_tolerance,
            max_leverage: i.max_leverage,
            tick: i.tick,
            lot: i.lot,
            min_qty: i.min_qty.0,
            max_qty: i.max_qty.0,
            min_notional: i.min_notional.0,
            venue_num_id: i.venue_num_id,
            symbol,
            tick_str,
            lot_str,
            min_qty_str,
            funding_interval_s: i.funding_interval_s,
            funding_anchor_s: i.funding_anchor_s,
            listed_utc_ns: i.listed_utc_ns,
            delivery_utc_ns: i.delivery_utc_ns,
            band_up: i.price_band.up.0,
            band_down: i.price_band.down.0,
        });
        i.id
    }

    pub fn instance(&mut self, i: InstanceSpec) -> StrategyInstanceId {
        let name = self.s(&i.name);
        self.instances.push(InstanceRow {
            id: i.id.0,
            _pad: 0,
            kind_id: i.kind_id,
            _pad2: 0,
            name,
        });
        i.id
    }

    pub fn account(&mut self, a: AccountSpec) -> AccountId {
        let label = self.s(&a.label);
        self.accounts.push(AccountRow {
            id: a.id.0,
            kind: if a.sim { 2 } else { 1 },
            bound_instance: a.bound_instance.0,
            is_prod: u8::from(a.is_prod),
            uid_policy: a.uid_policy as u8,
            _pad: [0; 3],
            venues: a.venues,
            label,
        });
        a.id
    }

    pub fn account_venue(&mut self, r: AccountVenueSpec) {
        let uid = self.s(&r.uid);
        self.account_venues.push(AccountVenueRow {
            account: r.account.0,
            venue: r.venue.0,
            _pad: [0; 6],
            uid,
        });
    }

    pub fn sim(&mut self, s: SimSpec) {
        self.sim_accounts.push(SimAccountRow {
            account: s.account.0,
            fee_tier: s.fee_tier,
            latency_model: s.latency_model,
            margin_model: s.margin_model,
            _pad: 0,
        });
    }

    pub fn sim_balance(&mut self, b: SimBalanceSpec) {
        self.sim_balances.push(SimBalanceRow {
            account: b.account.0,
            venue: b.venue.0,
            asset: b.asset.0,
            _pad: 0,
            qty: b.qty.0,
        });
    }

    pub fn fee(&mut self, f: FeeSpec) {
        self.fees.push(FeeRow {
            venue: f.venue.0,
            tier: f.tier,
            _pad: [0; 6],
            maker: f.fee.maker.0,
            taker: f.fee.taker.0,
        });
    }

    pub fn fee_override(&mut self, f: FeeOverrideSpec) {
        self.fee_overrides.push(FeeOverrideRow {
            account: f.account.0,
            venue: f.venue.0,
            _pad: [0; 2],
            inst: f.inst.0,
            maker: f.fee.maker.0,
            taker: f.fee.taker.0,
        });
    }

    pub fn limit(&mut self, venue: VenueId, l: RateLimit) {
        self.limits.push(RateLimitRow {
            venue: venue.0,
            scope: l.scope as u8,
            bucket: l.bucket as u8,
            _pad: 0,
            limit: l.limit,
            window_ms: l.window_ms,
            default_cost: l.default_cost,
            _pad2: 0,
        });
    }

    pub fn endpoint(&mut self, venue: VenueId, e: Endpoint) {
        let url = self.s(&e.url);
        self.endpoints.push(EndpointRow {
            venue: venue.0,
            role: e.role as u8,
            env: e.env as u8,
            conn_group: e.conn_group,
            _pad: 0,
            url,
        });
    }

    pub fn maint(&mut self, w: MaintWindow) {
        self.maint.push(MaintRow {
            venue: w.venue.0,
            kind: w.kind as u8,
            _pad: [0; 2],
            inst: w.inst.0,
            start_ns: w.start_ns,
            end_ns: w.end_ns,
        });
    }

    pub fn token(&mut self, t: ChainToken) {
        self.tokens.push(TokenRow {
            chain: t.chain.0,
            decimals: t.decimals,
            asset: t.asset.0,
            _pad: 0,
            address: t.address,
        });
    }

    pub fn alias(&mut self, a: AliasSpec) {
        let alias = self.s(&a.alias);
        self.aliases.push(AliasRow {
            venue: a.venue.0,
            _pad: [0; 3],
            inst: a.inst.0,
            alias,
        });
    }

    pub fn mmr_tier(&mut self, m: MmrTierSpec) {
        self.mmr.push(MmrTierRow {
            inst: m.inst.0,
            _pad: 0,
            notional_max: m.tier.notional_max.0,
            mmr: m.tier.mmr.0,
        });
    }

    /// Editable container (sections ascending by id, empty sections omitted). Keyed tables
    /// without ids are ordered by key: Fees (venue, tier), FeeOverrides (account, venue, inst),
    /// AccountVenues (account, venue), SimAccounts / SimBalances / MmrTiers by account / inst
    /// (stable, so the given order is kept within a key).
    pub fn finish_raw(mut self) -> RawBlob {
        self.fees.sort_by_key(|r| (r.venue, r.tier));
        self.fee_overrides.sort_by_key(|r| (r.account, r.venue, r.inst));
        self.account_venues.sort_by_key(|r| (r.account, r.venue));
        self.sim_accounts.sort_by_key(|r| r.account);
        self.sim_balances.sort_by_key(|r| r.account);
        self.mmr.sort_by_key(|r| r.inst);
        let strings = (!self.strings.is_empty()).then(|| RawSection {
            id: sec::STRINGS,
            row_ver: 1,
            row_size: 1,
            count: self.strings.len() as u32,
            flags: 0,
            data: self.strings.clone(),
        });
        let sections = [
            section(sec::ASSETS, &self.assets),
            section(sec::VENUES, &self.venues),
            section(sec::INSTS, &self.insts),
            section(sec::ACCOUNTS, &self.accounts),
            section(sec::FEES, &self.fees),
            section(sec::FEE_OVERRIDES, &self.fee_overrides),
            section(sec::RATE_LIMITS, &self.limits),
            section(sec::ENDPOINTS, &self.endpoints),
            section(sec::MAINT, &self.maint),
            section(sec::TOKENS, &self.tokens),
            section(sec::ALIASES, &self.aliases),
            strings,
            section(sec::VENUE_CAPS, &self.caps),
            section(sec::ACCOUNT_VENUES, &self.account_venues),
            section(sec::INSTANCES, &self.instances),
            section(sec::SIM_ACCOUNTS, &self.sim_accounts),
            section(sec::SIM_BALANCES, &self.sim_balances),
            section(sec::MMR_TIERS, &self.mmr),
            section(sec::STR_REFS, &self.strrefs),
        ]
        .into_iter()
        .flatten()
        .collect();
        RawBlob {
            magic: RDB_MAGIC,
            format: FORMAT,
            version: self.version,
            base_version: 0,
            created_utc_ns: self.created_utc_ns,
            rsv: [0; 16],
            sections,
        }
    }

    /// Encoded RDB1 blob.
    pub fn finish(self) -> Vec<u8> {
        self.finish_raw().encode()
    }
}

/// The ECF1 encoder (section contents are opaque bytes owned by other crates).
#[derive(Clone, Debug)]
pub struct EcfBuilder {
    version: u64,
    host: HostClass,
    created_utc_ns: i64,
    sections: Vec<RawSection>,
}

impl EcfBuilder {
    pub fn new(version: u64, host: HostClass) -> Self {
        EcfBuilder {
            version,
            host,
            created_utc_ns: 0,
            sections: Vec::new(),
        }
    }

    pub fn created_utc_ns(&mut self, ns: i64) -> &mut Self {
        self.created_utc_ns = ns;
        self
    }

    /// Adds a section of `count` rows of `row_size` bytes (`bytes.len()` should be
    /// `row_size × count`).
    pub fn section(
        &mut self,
        s: EngineSection,
        row_ver: u16,
        row_size: u32,
        count: u32,
        bytes: Vec<u8>,
    ) -> &mut Self {
        self.sections.push(RawSection {
            id: s as u16,
            row_ver,
            row_size,
            count,
            flags: 0,
            data: bytes,
        });
        self
    }

    /// Editable container (sections ascending by id).
    pub fn finish_raw(mut self) -> RawBlob {
        self.sections.sort_by_key(|s| s.id);
        let mut rsv = [0u8; 16];
        rsv[0] = self.host as u8;
        RawBlob {
            magic: ECF_MAGIC,
            format: FORMAT,
            version: self.version,
            base_version: 0,
            created_utc_ns: self.created_utc_ns,
            rsv,
            sections: self.sections,
        }
    }

    pub fn finish(self) -> Vec<u8> {
        self.finish_raw().encode()
    }
}
