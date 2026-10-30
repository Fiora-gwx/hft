//! `hft-refdata`: format, load-time validation and lookups of the reference data (RDB1:
//! insts, assets, venues, `VenueCaps`, accounts, strategy instances, fees, limits, endpoints,
//! maintenance windows, chain tokens, symbol tables) and of the engine configuration
//! container (ECF1).
//!
//! `RefData::load` is a pure, deterministic cold-path function; every query afterwards is an
//! array lookup or a bounded search that never allocates. No I/O, no threads, no clocks.
//! Specification: `crates/hft-refdata/README.md`.
#![forbid(unsafe_code)]
#![deny(
    clippy::float_arithmetic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

#[cfg(test)]
extern crate self as hft_refdata;

mod accounts;
pub mod blob;
mod builder;
mod caps;
#[cfg(feature = "tools")]
mod dump;
mod engine;
mod fees;
pub mod schema;
mod symbols;
mod types;
mod validate;

pub use builder::{
    AccountSpec, AccountVenueSpec, AliasSpec, AssetSpec, EcfBuilder, FeeOverrideSpec, FeeSpec, InstSpec,
    InstanceSpec, MmrTierSpec, RdbBuilder, SimBalanceSpec, SimSpec, VenueSpec,
};
pub use caps::{
    AssemblyFacts, CapFlags, DeadManCap, FeeAssetRule, FundingSettleKind, LiqMarker, MarginData, MarginMode,
    MultiAsset, PositionMode, ProtocolFacts, TestnetKind, VenueCaps, check_assembly, opt_bool_code,
};
#[cfg(feature = "tools")]
pub use dump::{DiffItem, diff, dump};
pub use engine::{EngineConfig, EngineSection, EngineValidator, SectionView};
pub use symbols::{SymEntry, SymbolIndex};
pub use types::{
    AccountKind, AccountMeta, AssetClass, AssetMeta, ChainToken, DEFAULT_MAX_UNIVERSE_INSTS, Endpoint,
    EndpointEnv, EndpointRole, Exchange, FeePair, GroupSpecView, InstHot, InstKind, InstMeta, InstStatus,
    InstanceMeta, LegView, LimitBucket, LimitScope, LoadErr, LoadErrCode, LoadReport, MAX_SYMBOL_LEN,
    MaintKind, MaintWindow, MmrTier, PriceBand, RateLimit, SimAccountCfg, SimBalance, StaleGuardKind, TsUnit,
    UidPolicy, VenueMeta,
};

use hft_types::{AccountId, AssetId, InstId, StrategyInstanceId, VenueId};

use crate::fees::{FeeTable, VENUE_SLOTS};

static NONE_INST: InstHot = InstHot::NONE;
const EMPTY_INSTS: &[InstId] = &[];

/// NONE rows returned for id 0 and out-of-range ids.
#[derive(Clone, Debug)]
pub(crate) struct NoneRows {
    inst_meta: InstMeta,
    asset: AssetMeta,
    venue: VenueMeta,
    caps: VenueCaps,
    account: AccountMeta,
    symbols: SymbolIndex,
}

impl NoneRows {
    pub(crate) fn new() -> NoneRows {
        NoneRows {
            inst_meta: InstMeta::none(),
            asset: AssetMeta::none(),
            venue: VenueMeta::none(),
            caps: VenueCaps::none(VenueId(0)),
            account: AccountMeta::none(),
            symbols: SymbolIndex::default(),
        }
    }
}

/// Immutable reference-data snapshot. Every table is allocated once by `load`.
#[derive(Clone, Debug)]
pub struct RefData {
    pub(crate) version: u64,
    pub(crate) created_utc_ns: i64,
    pub(crate) sha256: [u8; 32],
    pub(crate) blob: Box<[u8]>,
    pub(crate) insts: Box<[InstHot]>,
    pub(crate) inst_meta: Box<[InstMeta]>,
    pub(crate) assets: Box<[AssetMeta]>,
    pub(crate) venues: Box<[VenueMeta]>,
    pub(crate) venue_order: Box<[VenueId]>,
    pub(crate) venue_row: [u8; VENUE_SLOTS],
    pub(crate) caps: Box<[VenueCaps]>,
    pub(crate) caps_row: [u8; VENUE_SLOTS],
    pub(crate) symbols: Box<[SymbolIndex]>,
    pub(crate) accounts: Box<[AccountMeta]>,
    pub(crate) instances: Box<[InstanceMeta]>,
    pub(crate) instance_account: Box<[u8]>,
    pub(crate) instance_accounts: Box<[Box<[AccountId]>]>,
    pub(crate) fees: FeeTable,
    pub(crate) insts_of_asset: Box<[Box<[InstId]>]>,
    pub(crate) insts_of_venue: Box<[Box<[InstId]>]>,
    pub(crate) rate_limits: Box<[Box<[RateLimit]>]>,
    pub(crate) endpoints: Box<[Box<[Endpoint]>]>,
    pub(crate) maint: Box<[Box<[MaintWindow]>]>,
    pub(crate) tokens: Box<[ChainToken]>,
    pub(crate) account_venues: Box<[(AccountId, VenueId, Box<str>)]>,
    pub(crate) uid_index: Box<[(u8, u8, u32)]>,
    pub(crate) venue_account_ix: Box<[u8]>,
    pub(crate) warnings: Box<[LoadErr]>,
    pub(crate) none: NoneRows,
}

impl RefData {
    /// Loads and validates an RDB1 blob (README "加载校验"); `current` enables step 10.
    /// Cold, pure and deterministic.
    pub fn load(blob: &[u8], current: Option<&RefData>) -> Result<RefData, LoadReport> {
        validate::load(blob, current)
    }

    pub fn version(&self) -> u64 {
        self.version
    }
    /// SHA-256 of the blob bytes (= `ConfigBlob.sha256`).
    pub fn sha256(&self) -> &[u8; 32] {
        &self.sha256
    }
    /// The blob bytes this snapshot was loaded from.
    pub fn blob(&self) -> &[u8] {
        &self.blob
    }
    pub fn created_utc_ns(&self) -> i64 {
        self.created_utc_ns
    }

    // ---- hot queries ----

    /// 64-byte row; 0 / out of range → NONE row (`status = Invalid`).
    #[inline]
    pub fn inst(&self, id: InstId) -> &InstHot {
        self.insts.get(id.0 as usize).unwrap_or(&NONE_INST)
    }
    /// Symbol table of a venue (fetch once per feed thread and cache).
    #[inline]
    pub fn symbols(&self, v: VenueId) -> &SymbolIndex {
        self.symbols.get(usize::from(v.0)).unwrap_or(&self.none.symbols)
    }
    /// Fee of account `a` for inst `id` (override per inst > override per venue > tier row);
    /// unknown → zero rates.
    #[inline]
    pub fn fees(&self, a: AccountId, id: InstId) -> FeePair {
        let venue = self.inst(id).venue;
        self.fees.get(a.0, venue.0, id.0).unwrap_or(FeePair {
            maker: hft_types::Rate(0),
            taker: hft_types::Rate(0),
        })
    }
    /// Capability row; unregistered venue → NONE row (all bits 0).
    #[inline]
    pub fn caps(&self, v: VenueId) -> &VenueCaps {
        self.caps.get(usize::from(v.0)).unwrap_or(&self.none.caps)
    }
    /// Account row; 0 / out of range → NONE row.
    #[inline]
    pub fn account(&self, a: AccountId) -> &AccountMeta {
        self.accounts.get(usize::from(a.0)).unwrap_or(&self.none.account)
    }
    /// Non-fatal findings of a successful load (`DuplicateVenueUid` between two non-prod accounts).
    pub fn warnings(&self) -> &[LoadErr] {
        &self.warnings
    }

    // ---- cold queries ----

    pub fn inst_meta(&self, id: InstId) -> &InstMeta {
        self.inst_meta.get(id.0 as usize).unwrap_or(&self.none.inst_meta)
    }
    pub fn asset(&self, id: AssetId) -> &AssetMeta {
        self.assets.get(usize::from(id.0)).unwrap_or(&self.none.asset)
    }
    pub fn venue(&self, v: VenueId) -> &VenueMeta {
        self.venues.get(usize::from(v.0)).unwrap_or(&self.none.venue)
    }
    /// UID string of `(a, v)` whatever the account kind (may be empty).
    pub(crate) fn raw_uid(&self, a: AccountId, v: VenueId) -> Option<&str> {
        let i = self
            .uid_index
            .binary_search_by_key(&(a.0, v.0), |(x, y, _)| (*x, *y))
            .ok()?;
        let (_, _, row) = self.uid_index.get(i)?;
        self.account_venues.get(*row as usize).map(|(_, _, u)| &**u)
    }
    /// Exchange UID of a Live account on `v`; Sim, unregistered or empty → `None`.
    pub fn venue_uid(&self, a: AccountId, v: VenueId) -> Option<&str> {
        if !matches!(self.account(a).kind, AccountKind::Live) || a.0 == 0 {
            return None;
        }
        self.raw_uid(a, v).filter(|u| !u.is_empty())
    }
    pub fn instance(&self, s: StrategyInstanceId) -> Option<&InstanceMeta> {
        self.instances.get(usize::from(s.0).checked_sub(1)?)
    }
    /// The instance's primary account (the lowest id bound to it); unbound → `None`.
    pub fn account_of(&self, s: StrategyInstanceId) -> Option<AccountId> {
        let a = *self.instance_account.get(usize::from(s.0))?;
        (s.0 != 0 && a != 0).then_some(AccountId(a))
    }
    /// Every account bound to the instance, ascending (empty when unbound).
    pub fn accounts_of(&self, s: StrategyInstanceId) -> &[AccountId] {
        if s.0 == 0 {
            return &[];
        }
        self.instance_accounts.get(usize::from(s.0)).map_or(&[], |a| a)
    }
    /// Ascending `InstId`s of an asset.
    pub fn insts_of_asset(&self, a: AssetId) -> &[InstId] {
        self.insts_of_asset
            .get(usize::from(a.0))
            .map_or(EMPTY_INSTS, |s| s)
    }
    /// Ascending `InstId`s of a venue.
    pub fn insts_of_venue(&self, v: VenueId) -> &[InstId] {
        self.insts_of_venue
            .get(usize::from(v.0))
            .map_or(EMPTY_INSTS, |s| s)
    }
    pub fn rate_limits(&self, v: VenueId) -> &[RateLimit] {
        self.rate_limits.get(usize::from(v.0)).map_or(&[], |s| s)
    }
    pub fn endpoints(&self, v: VenueId, env: EndpointEnv) -> &[Endpoint] {
        let ix = usize::from(v.0) * 3 + usize::from(env as u8).saturating_sub(1);
        if usize::from(v.0) >= VENUE_SLOTS {
            return &[];
        }
        self.endpoints.get(ix).map_or(&[], |s| s)
    }
    /// Window containing `utc_ns` (`[start, end)`): the inst's own windows first, then
    /// whole-venue windows; within a level the smallest `start_ns`.
    pub fn maintenance(&self, id: InstId, utc_ns: i64) -> Option<&MaintWindow> {
        let h = self.inst(id);
        if h.status == InstStatus::Invalid {
            return None;
        }
        let ws = self.maint.get(usize::from(h.venue.0))?;
        let pick = |want: InstId| {
            ws.iter()
                .filter(|w| w.inst == want && w.start_ns <= utc_ns && utc_ns < w.end_ns)
                .min_by_key(|w| w.start_ns)
        };
        pick(id).or_else(|| pick(InstId(0)))
    }
    /// Token mapping of an asset on a chain venue.
    pub fn token(&self, chain: VenueId, a: AssetId) -> Option<&ChainToken> {
        self.tokens.iter().find(|t| t.chain == chain && t.asset == a)
    }
    /// Dense index 0..11 of a Live account among the enabled Live accounts of `v` (ascending
    /// `AccountId`); Sim or not enabled → `None`.
    #[inline]
    pub fn venue_account_ix(&self, v: VenueId, a: AccountId) -> Option<u8> {
        if usize::from(v.0) >= VENUE_SLOTS {
            return None;
        }
        let x = *self
            .venue_account_ix
            .get(usize::from(a.0) * VENUE_SLOTS + usize::from(v.0))?;
        (x != u8::MAX).then_some(x)
    }

    // ---- helpers for check_assembly / tools ----

    /// Number of inst rows.
    pub fn inst_count(&self) -> usize {
        self.insts.len().saturating_sub(1)
    }
    /// Venue ids in Venues-section row order.
    pub fn venues_in_order(&self) -> &[VenueId] {
        &self.venue_order
    }
    /// Instances in row order (id = row + 1).
    pub fn instances_in_order(&self) -> &[InstanceMeta] {
        &self.instances
    }
    /// Row of `v` in the VenueCaps section.
    pub fn caps_row(&self, v: VenueId) -> Option<u32> {
        self.caps_row
            .get(usize::from(v.0))
            .filter(|r| **r != u8::MAX)
            .map(|r| u32::from(*r))
    }
    /// Row of `v` in the Venues section.
    pub fn venue_row(&self, v: VenueId) -> Option<u32> {
        self.venue_row
            .get(usize::from(v.0))
            .filter(|r| **r != u8::MAX)
            .map(|r| u32::from(*r))
    }
    /// AccountVenues rows `(account, venue, uid)` in row order.
    pub fn account_venues(&self) -> &[(AccountId, VenueId, Box<str>)] {
        &self.account_venues
    }

    /// Every internal table rendered to bytes (everything except the blob copy and its hash);
    /// two loads of the same blob must give identical bytes.
    #[doc(hidden)]
    pub fn debug_tables(&self) -> Vec<u8> {
        let mut t = self.clone();
        t.blob = Box::default();
        t.sha256 = [0; 32];
        format!("{t:?}").into_bytes()
    }
}

// Declared at crate level so the path is relative to `src/` (an inline module's `#[path]` is
// relative to the non-existent `src/tests/` directory, which rustfmt cannot resolve on Linux).
#[cfg(test)]
#[path = "../tests/common/f0.rs"]
mod f0_fixture;

#[cfg(test)]
mod tests {
    use super::f0_fixture as f0;

    use super::RefData;

    /// REF-071: three loads (two threads, one after heap churn) give identical tables.
    #[test]
    fn ref_071_load_deterministic() {
        let b = f0::f0();
        let b1 = b.clone();
        let b2 = b.clone();
        let t1 =
            std::thread::spawn(move || RefData::load(&b1, None).map(|r| (r.debug_tables(), *r.sha256())));
        let t2 =
            std::thread::spawn(move || RefData::load(&b2, None).map(|r| (r.debug_tables(), *r.sha256())));
        let r1 = t1.join().ok().and_then(|r| r.ok());
        let r2 = t2.join().ok().and_then(|r| r.ok());
        let churn = vec![0xA5u8; 1 << 20];
        drop(std::hint::black_box(churn));
        let r3 = RefData::load(&b, None)
            .ok()
            .map(|r| (r.debug_tables(), *r.sha256()));
        assert!(r1.is_some(), "load 1 failed");
        assert_eq!(r1, r2);
        assert_eq!(r1, r3);
    }
}
