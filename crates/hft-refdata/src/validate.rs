//! `RefData::load`: load steps 1–11 (README "加载校验") and construction of the lookup tables.
//!
//! Each step collects every error it finds, in rule order and, within a rule, by
//! (section, row); a step with errors ends the load.

use hft_types::{
    AccountId, AssetExp, AssetId, InstId, KillBits, MAX_RAW, MarketKind, Notional, Px, Qty, Rate, StrategyId,
    VenueId, parse_exact,
};
use sha2::{Digest, Sha256};

use crate::RefData;
use crate::blob::{
    AccountRow, AccountVenueRow, AliasRow, AssetRow, CapsRow, EndpointRow, FeeOverrideRow, FeeRow, InstRow,
    InstanceRow, Kind, MaintRow, MmrTierRow, RateLimitRow, RdbHeader, Sect, SimAccountRow, SimBalanceRow,
    StrRef, TokenRow, VenueRow, parse_container, sec,
};
use crate::caps::{
    DeadManCap, FeeAssetRule, FundingSettleKind, LiqMarker, MarginData, MarginMode, PositionMode,
    TestnetKind, VenueCaps, opt_bool_decode,
};
use crate::fees::{FeeRows, FeeTable, VENUE_SLOTS};
use crate::symbols::SymbolIndex;
use crate::types::{
    AccountKind, AccountMeta, AssetClass, AssetMeta, ChainToken, Endpoint, EndpointEnv, EndpointRole,
    Exchange, InstHot, InstKind, InstMeta, InstStatus, InstanceMeta, LimitBucket, LimitScope, LoadErr,
    LoadErrCode as C, LoadReport, MAX_SYMBOL_LEN, MaintKind, MaintWindow, MmrTier, PriceBand, RateLimit,
    SimAccountCfg, SimBalance, StaleGuardKind, TsUnit, UidPolicy, VenueMeta,
};

/// 100 % as a `Rate` (exponent −9).
const RATE_ONE: i64 = 1_000_000_000;
const LIVE: u8 = 1;
const SIM: u8 = 2;

#[inline]
fn push(e: &mut Vec<LoadErr>, code: C, section: u16, row: usize) {
    e.push(LoadErr::new(code, section, row as u32));
}

fn done(e: &mut Vec<LoadErr>) -> Result<(), LoadReport> {
    if e.is_empty() {
        Ok(())
    } else {
        Err(LoadReport {
            errors: core::mem::take(e),
        })
    }
}

const fn market_of(code: u8) -> Option<MarketKind> {
    match code {
        1 => Some(MarketKind::Spot),
        2 => Some(MarketKind::LinearPerp),
        _ => None,
    }
}

const fn product_market(product: u8) -> Option<MarketKind> {
    match product {
        1 => Some(MarketKind::Spot),
        2 => Some(MarketKind::LinearPerp),
        _ => None,
    }
}

#[inline]
const fn tradable(status: u8) -> bool {
    status == InstStatus::Trading as u8 || status == InstStatus::ReduceOnly as u8
}

const fn is_chain_venue(v: u8) -> bool {
    v >= 32 && VenueId(v).name().is_some()
}

fn valid_symbol(s: &str) -> bool {
    (1..=MAX_SYMBOL_LEN).contains(&s.len()) && s.chars().all(|c| !c.is_whitespace() && !c.is_control())
}

/// Decoded rows of every known RDB1 section.
pub(crate) struct Tables<'a> {
    pub hdr: RdbHeader,
    pub assets: Vec<AssetRow>,
    pub venues: Vec<VenueRow>,
    pub insts: Vec<InstRow>,
    pub accounts: Vec<AccountRow>,
    pub fees: Vec<FeeRow>,
    pub fee_overrides: Vec<FeeOverrideRow>,
    pub limits: Vec<RateLimitRow>,
    pub endpoints: Vec<EndpointRow>,
    pub maint: Vec<MaintRow>,
    pub tokens: Vec<TokenRow>,
    pub aliases: Vec<AliasRow>,
    pub strings: &'a [u8],
    pub caps: Vec<CapsRow>,
    pub account_venues: Vec<AccountVenueRow>,
    pub instances: Vec<InstanceRow>,
    pub sim_accounts: Vec<SimAccountRow>,
    pub sim_balances: Vec<SimBalanceRow>,
    pub mmr: Vec<MmrTierRow>,
    pub strrefs: Vec<StrRef>,
}

/// `VenueId` → row of the Venues / VenueCaps sections (valid after step 3).
struct Index {
    venue_row: [Option<usize>; 256],
    caps_row: [Option<usize>; 256],
}

impl<'a> Tables<'a> {
    fn decode(hdr: RdbHeader, sects: &[Sect<'a>]) -> Tables<'a> {
        let get = |id: u16| sects.iter().find(|s| s.id == id);
        macro_rules! rows {
            ($id:expr) => {
                get($id).map(|s| s.rows()).unwrap_or_default()
            };
        }
        Tables {
            hdr,
            assets: rows!(sec::ASSETS),
            venues: rows!(sec::VENUES),
            insts: rows!(sec::INSTS),
            accounts: rows!(sec::ACCOUNTS),
            fees: rows!(sec::FEES),
            fee_overrides: rows!(sec::FEE_OVERRIDES),
            limits: rows!(sec::RATE_LIMITS),
            endpoints: rows!(sec::ENDPOINTS),
            maint: rows!(sec::MAINT),
            tokens: rows!(sec::TOKENS),
            aliases: rows!(sec::ALIASES),
            strings: get(sec::STRINGS).map_or(&[][..], |s| s.data),
            caps: rows!(sec::VENUE_CAPS),
            account_venues: rows!(sec::ACCOUNT_VENUES),
            instances: rows!(sec::INSTANCES),
            sim_accounts: rows!(sec::SIM_ACCOUNTS),
            sim_balances: rows!(sec::SIM_BALANCES),
            mmr: rows!(sec::MMR_TIERS),
            strrefs: rows!(sec::STR_REFS),
        }
    }

    // ---- lookups (valid once ids are checked) ----

    fn str(&self, r: StrRef) -> Option<&'a str> {
        let start = r.off as usize;
        let b = self.strings.get(start..start.checked_add(r.len as usize)?)?;
        core::str::from_utf8(b).ok()
    }
    fn s(&self, r: StrRef) -> &'a str {
        self.str(r).unwrap_or("")
    }
    /// Resolved string (`""` when the reference is invalid).
    #[cfg(feature = "tools")]
    pub(crate) fn text(&self, r: StrRef) -> &'a str {
        self.s(r)
    }
    fn events(&self, c: &CapsRow) -> Option<&[StrRef]> {
        let start = c.events_off as usize;
        self.strrefs
            .get(start..start.checked_add(c.events_count as usize)?)
    }
    fn asset(&self, id: u16) -> Option<&AssetRow> {
        self.assets.get(usize::from(id).checked_sub(1)?)
    }
    fn inst(&self, id: u32) -> Option<&InstRow> {
        self.insts.get((id as usize).checked_sub(1)?)
    }
    fn account(&self, id: u8) -> Option<&AccountRow> {
        self.accounts.get(usize::from(id).checked_sub(1)?)
    }
    fn instance_exists(&self, id: u8) -> bool {
        id != 0 && usize::from(id) <= self.instances.len()
    }

    // ---- step 1: discriminants ----

    fn step1_enums(&self, e: &mut Vec<LoadErr>) {
        for (i, r) in self.assets.iter().enumerate() {
            if AssetClass::from_u8(r.class).is_none() {
                push(e, C::Structure, sec::ASSETS, i);
            }
        }
        for (i, r) in self.venues.iter().enumerate() {
            if Exchange::from_u8(r.exchange).is_none()
                || InstKind::from_u8(r.product).is_none()
                || TsUnit::from_u8(r.ts_unit).is_none()
                || StaleGuardKind::from_u8(r.stale_guard).is_none()
            {
                push(e, C::Structure, sec::VENUES, i);
            }
        }
        for (i, r) in self.insts.iter().enumerate() {
            if !(1..=6).contains(&r.status)
                || InstKind::from_u8(r.kind).is_none()
                || r.flags & !InstHot::NO_AMEND != 0
            {
                push(e, C::Structure, sec::INSTS, i);
            }
        }
        for (i, r) in self.accounts.iter().enumerate() {
            if !(LIVE..=SIM).contains(&r.kind) || r.is_prod > 1 || UidPolicy::from_u8(r.uid_policy).is_none()
            {
                push(e, C::Structure, sec::ACCOUNTS, i);
            }
        }
        for (i, r) in self.limits.iter().enumerate() {
            if LimitScope::from_u8(r.scope).is_none() || LimitBucket::from_u8(r.bucket).is_none() {
                push(e, C::Structure, sec::RATE_LIMITS, i);
            }
        }
        for (i, r) in self.endpoints.iter().enumerate() {
            if EndpointRole::from_u8(r.role).is_none() || EndpointEnv::from_u8(r.env).is_none() {
                push(e, C::Structure, sec::ENDPOINTS, i);
            }
        }
        for (i, r) in self.maint.iter().enumerate() {
            if MaintKind::from_u8(r.kind).is_none() {
                push(e, C::Structure, sec::MAINT, i);
            }
        }
        for (i, r) in self.caps.iter().enumerate() {
            if market_of(r.market).is_none()
                || FeeAssetRule::decode(r.fee_rule, r.fee_asset).is_none()
                || r.reduce_only > 1
                || PositionMode::from_u8(r.position_mode).is_none()
                || MarginMode::from_u8(r.margin_mode).is_none()
                || opt_bool_decode(r.multi_asset).is_none()
                || opt_bool_decode(r.spot_borrow).is_none()
                || FundingSettleKind::from_u8(r.funding_settle).is_none()
                || MarginData::decode(r.margin_data, r.margin_poll_ms).is_none()
                || LiqMarker::from_u8(r.liq_marker).is_none()
                || DeadManCap::from_u8(r.dms).is_none()
                || TestnetKind::from_u8(r.testnet).is_none()
                || r.amend_qty_down_only > 1
                || r.flags & !crate::caps::CapFlags::ALL.0 != 0
            {
                push(e, C::Structure, sec::VENUE_CAPS, i);
            }
        }
    }

    // ---- step 2: strings ----

    fn step2_strings(&self, e: &mut Vec<LoadErr>) {
        let ok = |r: StrRef| self.str(r).is_some();
        // Rule A: references in bounds and UTF-8.
        let mut inst_ok = vec![true; self.insts.len()];
        let mut alias_ok = vec![true; self.aliases.len()];
        for (i, r) in self.assets.iter().enumerate() {
            if !ok(r.name) {
                push(e, C::Strings, sec::ASSETS, i);
            }
        }
        for (i, r) in self.venues.iter().enumerate() {
            if !ok(r.client_id_prefix) {
                push(e, C::Strings, sec::VENUES, i);
            }
        }
        for (i, r) in self.insts.iter().enumerate() {
            if !(ok(r.symbol) && ok(r.tick_str) && ok(r.lot_str) && ok(r.min_qty_str)) {
                push(e, C::Strings, sec::INSTS, i);
                if let Some(f) = inst_ok.get_mut(i) {
                    *f = false;
                }
            }
        }
        for (i, r) in self.accounts.iter().enumerate() {
            if !ok(r.label) {
                push(e, C::Strings, sec::ACCOUNTS, i);
            }
        }
        for (i, r) in self.endpoints.iter().enumerate() {
            if !ok(r.url) {
                push(e, C::Strings, sec::ENDPOINTS, i);
            }
        }
        for (i, r) in self.aliases.iter().enumerate() {
            if !ok(r.alias) {
                push(e, C::Strings, sec::ALIASES, i);
                if let Some(f) = alias_ok.get_mut(i) {
                    *f = false;
                }
            }
        }
        for (i, r) in self.caps.iter().enumerate() {
            let events_ok = self.events(r).is_some_and(|ev| ev.iter().all(|s| ok(*s)));
            if !ok(r.liq_marker_arg) || !events_ok {
                push(e, C::Strings, sec::VENUE_CAPS, i);
            }
        }
        for (i, r) in self.account_venues.iter().enumerate() {
            if !ok(r.uid) {
                push(e, C::Strings, sec::ACCOUNT_VENUES, i);
            }
        }
        for (i, r) in self.instances.iter().enumerate() {
            if !ok(r.name) {
                push(e, C::Strings, sec::INSTANCES, i);
            }
        }
        // Rule B: symbol and alias syntax.
        for (i, r) in self.insts.iter().enumerate() {
            if inst_ok.get(i).copied().unwrap_or(false) && !valid_symbol(self.s(r.symbol)) {
                push(e, C::Strings, sec::INSTS, i);
            }
        }
        for (i, r) in self.aliases.iter().enumerate() {
            if alias_ok.get(i).copied().unwrap_or(false) && !valid_symbol(self.s(r.alias)) {
                push(e, C::Strings, sec::ALIASES, i);
            }
        }
        // Rule C: account labels and instance names are unique (later rows reported).
        let dups = |names: Vec<(&str, usize)>| -> Vec<usize> {
            let mut names = names;
            names.sort();
            let mut out: Vec<usize> = names
                .windows(2)
                .filter_map(|w| match w {
                    [a, b] if a.0 == b.0 => Some(b.1),
                    _ => None,
                })
                .collect();
            out.sort_unstable();
            out
        };
        let labels = self
            .accounts
            .iter()
            .enumerate()
            .filter_map(|(i, r)| Some((self.str(r.label)?, i)))
            .collect();
        for i in dups(labels) {
            push(e, C::Strings, sec::ACCOUNTS, i);
        }
        let names = self
            .instances
            .iter()
            .enumerate()
            .filter_map(|(i, r)| Some((self.str(r.name)?, i)))
            .collect();
        for i in dups(names) {
            push(e, C::Strings, sec::INSTANCES, i);
        }
    }

    // ---- step 3: ids ----

    fn step3_ids(&self, e: &mut Vec<LoadErr>) {
        fn dense(ids: impl Iterator<Item = u64>, section: u16, e: &mut Vec<LoadErr>) {
            for (i, id) in ids.enumerate() {
                if id != i as u64 + 1 {
                    push(e, C::Ids, section, i);
                    return;
                }
            }
        }
        fn keyed(ids: &[u8], section: u16, e: &mut Vec<LoadErr>) {
            for (i, w) in ids.windows(2).enumerate() {
                if let [a, b] = w {
                    if a == b {
                        push(e, C::Ids, section, i + 1);
                        return;
                    }
                    if a > b {
                        push(e, C::Ids, section, i);
                        return;
                    }
                }
            }
        }
        dense(self.assets.iter().map(|r| u64::from(r.id)), sec::ASSETS, e);
        dense(self.insts.iter().map(|r| u64::from(r.id)), sec::INSTS, e);
        dense(self.accounts.iter().map(|r| u64::from(r.id)), sec::ACCOUNTS, e);
        dense(self.instances.iter().map(|r| u64::from(r.id)), sec::INSTANCES, e);
        let venue_ids: Vec<u8> = self.venues.iter().map(|r| r.id).collect();
        let caps_ids: Vec<u8> = self.caps.iter().map(|r| r.venue).collect();
        keyed(&venue_ids, sec::VENUES, e);
        keyed(&caps_ids, sec::VENUE_CAPS, e);
        for (i, v) in venue_ids.iter().enumerate() {
            if VenueId(*v).name().is_none() {
                push(e, C::UnknownVenue, sec::VENUES, i);
            }
        }
        for (i, v) in caps_ids.iter().enumerate() {
            if VenueId(*v).name().is_none() {
                push(e, C::UnknownVenue, sec::VENUE_CAPS, i);
            }
        }
        for (i, r) in self.instances.iter().enumerate() {
            if r.id > 254 {
                push(e, C::InstanceIdRange, sec::INSTANCES, i);
            }
        }
    }

    fn index(&self) -> Index {
        let mut ix = Index {
            venue_row: [None; 256],
            caps_row: [None; 256],
        };
        for (i, r) in self.venues.iter().enumerate() {
            if let Some(s) = ix.venue_row.get_mut(usize::from(r.id)) {
                s.get_or_insert(i);
            }
        }
        for (i, r) in self.caps.iter().enumerate() {
            if let Some(s) = ix.caps_row.get_mut(usize::from(r.venue)) {
                s.get_or_insert(i);
            }
        }
        ix
    }

    fn venue(&self, ix: &Index, v: u8) -> Option<&VenueRow> {
        self.venues.get((*ix.venue_row.get(usize::from(v))?)?)
    }
    fn exchange(&self, ix: &Index, v: u8) -> Option<u8> {
        self.venue(ix, v).map(|r| r.exchange)
    }

    // ---- step 4: references ----

    fn step4_refs(&self, ix: &Index, e: &mut Vec<LoadErr>) {
        let venue_ok = |v: u8| self.venue(ix, v).is_some();
        let asset_ok = |a: u16| self.asset(a).is_some();
        let account_ok = |a: u8| self.account(a).is_some();
        let inst_on = |id: u32, v: u8| self.inst(id).is_some_and(|r| r.venue == v);
        for (i, r) in self.insts.iter().enumerate() {
            if !venue_ok(r.venue) || !asset_ok(r.asset) || !asset_ok(r.quote) {
                push(e, C::DanglingRef, sec::INSTS, i);
            }
        }
        for (i, r) in self.insts.iter().enumerate() {
            if self
                .asset(r.quote)
                .is_some_and(|q| q.class != AssetClass::Stable as u8)
            {
                push(e, C::QuoteNotStable, sec::INSTS, i);
            }
        }
        for (i, r) in self.fees.iter().enumerate() {
            if !venue_ok(r.venue) {
                push(e, C::DanglingRef, sec::FEES, i);
            }
        }
        for (i, r) in self.fee_overrides.iter().enumerate() {
            if !account_ok(r.account) || !venue_ok(r.venue) || (r.inst != 0 && !inst_on(r.inst, r.venue)) {
                push(e, C::DanglingRef, sec::FEE_OVERRIDES, i);
            }
        }
        for (i, r) in self.limits.iter().enumerate() {
            if !venue_ok(r.venue) {
                push(e, C::DanglingRef, sec::RATE_LIMITS, i);
            }
        }
        for (i, r) in self.endpoints.iter().enumerate() {
            if !venue_ok(r.venue) {
                push(e, C::DanglingRef, sec::ENDPOINTS, i);
            }
        }
        for (i, r) in self.maint.iter().enumerate() {
            if !venue_ok(r.venue) || (r.inst != 0 && !inst_on(r.inst, r.venue)) {
                push(e, C::DanglingRef, sec::MAINT, i);
            }
        }
        for (i, r) in self.tokens.iter().enumerate() {
            if !venue_ok(r.chain) || !is_chain_venue(r.chain) || !asset_ok(r.asset) {
                push(e, C::DanglingRef, sec::TOKENS, i);
            }
        }
        for (i, r) in self.aliases.iter().enumerate() {
            if !venue_ok(r.venue) || !inst_on(r.inst, r.venue) {
                push(e, C::DanglingRef, sec::ALIASES, i);
            }
        }
        for (i, r) in self.caps.iter().enumerate() {
            let rule_asset = FeeAssetRule::decode(r.fee_rule, r.fee_asset).map_or(AssetId(0), |f| f.asset());
            if !venue_ok(r.venue)
                || (rule_asset.0 != 0 && !asset_ok(rule_asset.0))
                || (r.fee_rule >= 3 && rule_asset.0 == 0)
            {
                push(e, C::DanglingRef, sec::VENUE_CAPS, i);
            }
        }
        for (i, r) in self.accounts.iter().enumerate() {
            let venues_ok = (0..64u8).all(|v| r.venues & (1u64 << v) == 0 || venue_ok(v));
            if (r.bound_instance != 0 && !self.instance_exists(r.bound_instance)) || !venues_ok {
                push(e, C::DanglingRef, sec::ACCOUNTS, i);
            }
        }
        let mut bound_by: Vec<bool> = vec![false; 256];
        for (i, r) in self.accounts.iter().enumerate() {
            if r.bound_instance == 0 {
                continue;
            }
            if let Some(b) = bound_by.get_mut(usize::from(r.bound_instance)) {
                if *b {
                    push(e, C::InstanceMultiBound, sec::ACCOUNTS, i);
                }
                *b = true;
            }
        }
        for (i, r) in self.account_venues.iter().enumerate() {
            if !account_ok(r.account) || !venue_ok(r.venue) {
                push(e, C::DanglingRef, sec::ACCOUNT_VENUES, i);
            }
        }
        for (i, r) in self.sim_accounts.iter().enumerate() {
            if !account_ok(r.account) {
                push(e, C::DanglingRef, sec::SIM_ACCOUNTS, i);
            }
        }
        for (i, r) in self.sim_balances.iter().enumerate() {
            if !account_ok(r.account) || !venue_ok(r.venue) || !asset_ok(r.asset) {
                push(e, C::DanglingRef, sec::SIM_BALANCES, i);
            }
        }
        for (i, r) in self.mmr.iter().enumerate() {
            if self.inst(r.inst).is_none() {
                push(e, C::DanglingRef, sec::MMR_TIERS, i);
            }
        }
    }

    // ---- step 5: capability table ----

    fn step5_caps(&self, ix: &Index, e: &mut Vec<LoadErr>) {
        let mut has_inst = [false; 256];
        for r in &self.insts {
            if let Some(h) = has_inst.get_mut(usize::from(r.venue)) {
                *h = true;
            }
        }
        for (i, v) in self.venues.iter().enumerate() {
            let caps_rows = self.caps.iter().filter(|c| c.venue == v.id).count();
            if has_inst.get(usize::from(v.id)).copied().unwrap_or(false) && caps_rows != 1 {
                push(e, C::MissingCaps, sec::VENUES, i);
            }
        }
        for (i, c) in self.caps.iter().enumerate() {
            let m = market_of(c.market);
            let prod = self.venue(ix, c.venue).and_then(|v| product_market(v.product));
            if m.is_none() || m != prod || m != VenueId(c.venue).market() {
                push(e, C::CapsMarketMismatch, sec::VENUE_CAPS, i);
            }
        }
        for (i, r) in self.insts.iter().enumerate() {
            if r.kind == InstKind::Delivery as u8 {
                continue;
            }
            if self.venue(ix, r.venue).is_some_and(|v| v.product != r.kind) {
                push(e, C::CapsMarketMismatch, sec::INSTS, i);
            }
        }
        let is_spot = |c: &CapsRow| VenueId(c.venue).market() == Some(MarketKind::Spot);
        let is_perp = |c: &CapsRow| VenueId(c.venue).market() == Some(MarketKind::LinearPerp);
        for (i, c) in self.caps.iter().enumerate() {
            if is_spot(c)
                && (c.position_mode != PositionMode::NotApplicable as u8
                    || c.margin_mode != MarginMode::NotApplicable as u8
                    || c.funding_settle != FundingSettleKind::NotApplicable as u8
                    || c.margin_data != MarginData::NotApplicable.code()
                    || c.multi_asset != 1)
            {
                push(e, C::CapsMarketMismatch, sec::VENUE_CAPS, i);
            }
        }
        for (i, c) in self.caps.iter().enumerate() {
            if is_spot(c) && c.spot_borrow == 3 {
                push(e, C::SpotBorrowRequired, sec::VENUE_CAPS, i);
            } else if is_perp(c) && c.spot_borrow != 1 {
                push(e, C::CapsMarketMismatch, sec::VENUE_CAPS, i);
            }
        }
        for (i, c) in self.caps.iter().enumerate() {
            if is_perp(c) && c.funding_settle == FundingSettleKind::NotApplicable as u8 {
                push(e, C::CapsMarketMismatch, sec::VENUE_CAPS, i);
            }
        }
        for (i, c) in self.caps.iter().enumerate() {
            if c.margin_data == 2 && !(1_000..=600_000).contains(&c.margin_poll_ms) {
                push(e, C::CapsValue, sec::VENUE_CAPS, i);
            }
        }
        for (i, c) in self.caps.iter().enumerate() {
            let ok = if c.dms == DeadManCap::None as u8 {
                c.dms_min_ms == 0 && c.dms_max_ms == 0 && c.dms_timeout_ms == 0 && c.dms_refresh_ms == 0
            } else {
                let floor = u64::from(c.dms_min_ms).max(2 * u64::from(c.dms_refresh_ms));
                c.dms_refresh_ms > 0
                    && u64::from(c.dms_timeout_ms) >= floor
                    && (c.dms_max_ms == 0 || c.dms_timeout_ms <= c.dms_max_ms)
            };
            if !ok {
                push(e, C::DeadManRange, sec::VENUE_CAPS, i);
            }
        }
        for (i, c) in self.caps.iter().enumerate() {
            if c.margin_mode == MarginMode::Mixed as u8 {
                push(e, C::CapsMarketMismatch, sec::VENUE_CAPS, i);
            }
        }
    }

    // ---- step 6: exponents ----

    /// `(px_wire_exp, qty_wire_exp)` in i16 (no overflow on hostile shifts).
    fn wire_exps(&self, r: &InstRow) -> (i16, i16) {
        let (pe, qe) = self
            .asset(r.asset)
            .map_or((0, 0), |a| (i16::from(a.px_exp), i16::from(a.qty_exp)));
        (pe + i16::from(r.px_unit_shift), qe - i16::from(r.qty_unit_shift))
    }

    fn step6_exps(&self, e: &mut Vec<LoadErr>) {
        let in_range = |x: i16| (-18..=6).contains(&x);
        let mut bad = vec![false; self.assets.len()];
        for (i, a) in self.assets.iter().enumerate() {
            if !in_range(a.px_exp.into()) || !in_range(a.qty_exp.into()) {
                push(e, C::ExpRange, sec::ASSETS, i);
                if let Some(b) = bad.get_mut(i) {
                    *b = true;
                }
            }
        }
        for (i, a) in self.assets.iter().enumerate() {
            if !bad.get(i).copied().unwrap_or(true) && i16::from(a.px_exp) + i16::from(a.qty_exp) > -6 {
                push(e, C::NotionalExp, sec::ASSETS, i);
            }
        }
        for (i, r) in self.insts.iter().enumerate() {
            let (pw, qw) = self.wire_exps(r);
            let shift_ok = (-12..=12).contains(&r.px_unit_shift) && (-12..=12).contains(&r.qty_unit_shift);
            if !shift_ok || !in_range(pw) || !in_range(qw) {
                push(e, C::ExpRange, sec::INSTS, i);
            }
        }
        for (i, r) in self.insts.iter().enumerate() {
            if r.kind == InstKind::Delivery as u8 {
                push(e, C::UnsupportedKind, sec::INSTS, i);
            }
        }
    }

    // ---- step 7: steps re-checked against the exchange text ----

    fn step7_steps(&self, e: &mut Vec<LoadErr>) {
        let recheck = |text: StrRef, exp: i16, raw: i64| -> Option<C> {
            match parse_exact(self.s(text).as_bytes(), exp as i8) {
                Ok(v) if v == raw => None,
                Err(err) if err == hft_types::FxErr::PRECISION => Some(C::AssetExpTooCoarse),
                _ => Some(C::TickMismatch),
            }
        };
        for (i, r) in self.insts.iter().enumerate() {
            if let Some(c) = recheck(r.tick_str, self.wire_exps(r).0, r.tick) {
                push(e, c, sec::INSTS, i);
            }
        }
        for (i, r) in self.insts.iter().enumerate() {
            if let Some(c) = recheck(r.lot_str, self.wire_exps(r).1, r.lot) {
                push(e, c, sec::INSTS, i);
            }
        }
        for (i, r) in self.insts.iter().enumerate() {
            if let Some(c) = recheck(r.min_qty_str, self.wire_exps(r).1, r.min_qty) {
                push(e, c, sec::INSTS, i);
            }
        }
        for (i, r) in self.insts.iter().enumerate() {
            if r.tick <= 0 || r.lot <= 0 || r.min_qty <= 0 {
                push(e, C::TickMismatch, sec::INSTS, i);
            }
        }
        for (i, r) in self.insts.iter().enumerate() {
            if r.lot <= 0 || r.min_qty.checked_rem(r.lot) != Some(0) {
                push(e, C::TickMismatch, sec::INSTS, i);
            }
        }
    }

    // ---- step 8: ranges and uniqueness ----

    fn step8_ranges(&self, e: &mut Vec<LoadErr>) {
        let max = MAX_RAW / 4;
        for (i, a) in self.assets.iter().enumerate() {
            if a.px_max > max || a.qty_max > max {
                push(e, C::ValueRange, sec::ASSETS, i);
            }
        }
        let asset_of = |r: &InstRow| self.asset(r.asset).copied().unwrap_or_default();
        for (i, r) in self.insts.iter().enumerate() {
            if r.max_qty > asset_of(r).qty_max {
                push(e, C::ValueRange, sec::INSTS, i);
            }
        }
        for (i, r) in self.insts.iter().enumerate() {
            if i128::from(r.book_window) * i128::from(r.tick) > i128::from(asset_of(r).px_max) {
                push(e, C::ValueRange, sec::INSTS, i);
            }
        }
        for (i, r) in self.insts.iter().enumerate() {
            if !(64..=65_536).contains(&r.book_window) {
                push(e, C::ValueRange, sec::INSTS, i);
            }
        }
        // Symbols and numeric ids unique per venue; aliases clash neither with symbols nor each other.
        let mut syms: Vec<(u8, &str, usize)> = self
            .insts
            .iter()
            .enumerate()
            .map(|(i, r)| (r.venue, self.s(r.symbol), i))
            .collect();
        syms.sort();
        let mut nums: Vec<(u8, u64, usize)> = self
            .insts
            .iter()
            .enumerate()
            .filter(|(_, r)| r.venue_num_id != 0)
            .map(|(i, r)| (r.venue, r.venue_num_id, i))
            .collect();
        nums.sort();
        let mut dup_rows: Vec<usize> = Vec::new();
        for w in syms.windows(2) {
            if let [a, b] = w
                && (a.0, a.1) == (b.0, b.1)
            {
                dup_rows.push(b.2);
            }
        }
        for w in nums.windows(2) {
            if let [a, b] = w
                && (a.0, a.1) == (b.0, b.1)
            {
                dup_rows.push(b.2);
            }
        }
        dup_rows.sort_unstable();
        for i in dup_rows {
            push(e, C::DuplicateSymbol, sec::INSTS, i);
        }
        let mut aliases: Vec<(u8, &str, usize)> = self
            .aliases
            .iter()
            .enumerate()
            .map(|(i, r)| (r.venue, self.s(r.alias), i))
            .collect();
        aliases.sort();
        let mut alias_dups: Vec<usize> = Vec::new();
        for (k, a) in aliases.iter().enumerate() {
            let clash_symbol = syms.binary_search_by(|s| (s.0, s.1).cmp(&(a.0, a.1))).is_ok();
            let clash_alias = k > 0 && aliases.get(k - 1).is_some_and(|p| (p.0, p.1) == (a.0, a.1));
            if clash_symbol || clash_alias {
                alias_dups.push(a.2);
            }
        }
        alias_dups.sort_unstable();
        for i in alias_dups {
            push(e, C::DuplicateSymbol, sec::ALIASES, i);
        }
        for (i, a) in self.assets.iter().enumerate() {
            if a.dust <= 0 || a.dust > a.qty_max {
                push(e, C::ValueRange, sec::ASSETS, i);
            }
        }
        let mut order: Vec<usize> = (0..self.mmr.len()).collect();
        order.sort_by_key(|&i| (self.mmr.get(i).map_or(0, |r| r.inst), i));
        let mut bad_mmr: Vec<usize> = Vec::new();
        let mut prev: Option<&MmrTierRow> = None;
        for &i in &order {
            let Some(r) = self.mmr.get(i) else { continue };
            let perp = self.inst(r.inst).is_some_and(|x| x.kind == InstKind::Perp as u8);
            let mut ok = perp && r.mmr > 0 && r.mmr < RATE_ONE;
            if let Some(p) = prev
                && p.inst == r.inst
            {
                ok &= r.notional_max > p.notional_max && r.mmr >= p.mmr;
            }
            if !ok {
                bad_mmr.push(i);
            }
            prev = Some(r);
        }
        bad_mmr.sort_unstable();
        for i in bad_mmr {
            push(e, C::ValueRange, sec::MMR_TIERS, i);
        }
        for (i, r) in self.insts.iter().enumerate() {
            let ok = |x: i64| (0..RATE_ONE).contains(&x);
            if !ok(r.band_up) || !ok(r.band_down) {
                push(e, C::ValueRange, sec::INSTS, i);
            }
        }
    }

    // ---- step 9: integrity ----

    fn sim_tier(&self, a: &AccountRow) -> u8 {
        if a.kind == SIM {
            self.sim_accounts
                .iter()
                .find(|s| s.account == a.id)
                .map_or(0, |s| s.fee_tier)
        } else {
            0
        }
    }

    fn step9_integrity(&self, ix: &Index, e: &mut Vec<LoadErr>, warnings: &mut Vec<LoadErr>) {
        let bits = |m: u64| (0..64u8).filter(move |v| m & (1u64 << v) != 0);
        let live = |a: &AccountRow| a.kind == LIVE;
        // SameExchangeMarkets.
        for (i, a) in self.accounts.iter().enumerate() {
            let mut seen = [false; 256];
            let mut clash = false;
            for v in bits(a.venues) {
                if let Some(x) = self.exchange(ix, v)
                    && let Some(s) = seen.get_mut(usize::from(x))
                {
                    clash |= *s;
                    *s = true;
                }
            }
            if clash {
                push(e, C::SameExchangeMarkets, sec::ACCOUNTS, i);
            }
        }
        // DuplicateVenueUid: group rows by (exchange, uid); compare pairs within a group.
        let mut keyed: Vec<(u8, &str, usize)> = self
            .account_venues
            .iter()
            .enumerate()
            .filter_map(|(i, r)| {
                let uid = self.s(r.uid);
                (!uid.is_empty()).then_some((self.exchange(ix, r.venue)?, uid, i))
            })
            .collect();
        keyed.sort();
        let prod = |acc: u8| self.account(acc).is_some_and(|a| a.is_prod == 1);
        let mut dvu: Vec<(usize, bool)> = Vec::new();
        for (k, cur) in keyed.iter().enumerate() {
            let Some(rj) = self.account_venues.get(cur.2) else {
                continue;
            };
            let (mut is_err, mut is_warn) = (false, false);
            for prev in keyed.get(..k).unwrap_or(&[]).iter().rev() {
                if (prev.0, prev.1) != (cur.0, cur.1) {
                    break;
                }
                let Some(ri) = self.account_venues.get(prev.2) else {
                    continue;
                };
                // Same group, earlier row (the group is sorted by row): reported on the later row.
                if ri.account == rj.account {
                    continue;
                }
                if prod(ri.account) || prod(rj.account) || ri.venue == rj.venue {
                    is_err = true;
                } else {
                    is_warn = true;
                }
            }
            if is_err || is_warn {
                dvu.push((cur.2, is_err));
            }
        }
        dvu.sort_unstable();
        for &(row, is_err) in &dvu {
            if is_err {
                push(e, C::DuplicateVenueUid, sec::ACCOUNT_VENUES, row);
            } else {
                push(warnings, C::DuplicateVenueUid, sec::ACCOUNT_VENUES, row);
            }
        }
        // SimCfg: Live accounts own no SimAccounts / SimBalances rows.
        let is_live_id = |acc: u8| self.account(acc).is_some_and(live);
        for (i, r) in self.sim_accounts.iter().enumerate() {
            if is_live_id(r.account) {
                push(e, C::SimCfg, sec::SIM_ACCOUNTS, i);
            }
        }
        for (i, r) in self.sim_balances.iter().enumerate() {
            if is_live_id(r.account) {
                push(e, C::SimCfg, sec::SIM_BALANCES, i);
            }
        }
        // UidPolicyProd.
        for (i, a) in self.accounts.iter().enumerate() {
            if a.uid_policy == UidPolicy::Unverifiable as u8 && (a.is_prod == 1 || !live(a)) {
                push(e, C::UidPolicyProd, sec::ACCOUNTS, i);
            }
        }
        // Empty UID only on non-prod Unverifiable Live accounts.
        for (i, r) in self.account_venues.iter().enumerate() {
            let allowed = self
                .account(r.account)
                .is_some_and(|a| live(a) && a.is_prod == 0 && a.uid_policy == UidPolicy::Unverifiable as u8);
            if self.s(r.uid).is_empty() && !allowed {
                push(e, C::MissingVenueUid, sec::ACCOUNT_VENUES, i);
            }
        }
        // MissingFee.
        let fee_rows = FeeRows::new(&self.fees, &self.fee_overrides);
        for (i, r) in self.insts.iter().enumerate() {
            if !tradable(r.status) {
                continue;
            }
            let missing = self.accounts.iter().any(|a| {
                a.venues & (1u64 << (r.venue & 63)) != 0
                    && r.venue < 64
                    && fee_rows.resolve(a.id, self.sim_tier(a), r.venue, r.id).is_none()
            });
            if missing {
                push(e, C::MissingFee, sec::INSTS, i);
            }
        }
        // MissingVenueUid: every venue of a Live account has an AccountVenues row.
        let mut av: Vec<(u8, u8)> = self.account_venues.iter().map(|r| (r.account, r.venue)).collect();
        av.sort_unstable();
        for (i, a) in self.accounts.iter().enumerate() {
            if live(a) && bits(a.venues).any(|v| av.binary_search(&(a.id, v)).is_err()) {
                push(e, C::MissingVenueUid, sec::ACCOUNTS, i);
            }
        }
        // TooManyAccountsPerVenue.
        for (i, v) in self.venues.iter().enumerate() {
            let n = self
                .accounts
                .iter()
                .filter(|a| live(a) && v.id < 64 && a.venues & (1u64 << v.id) != 0)
                .count();
            if n > usize::from(KillBits::MAX_ACCOUNTS_PER_GW) {
                push(e, C::TooManyAccountsPerVenue, sec::VENUES, i);
            }
        }
        // Sim accounts.
        for (i, a) in self.accounts.iter().enumerate() {
            if !live(a) && a.is_prod == 1 {
                push(e, C::SimIsProd, sec::ACCOUNTS, i);
            }
        }
        let is_sim_id = |acc: u8| self.account(acc).is_some_and(|a| !live(a));
        for (i, r) in self.account_venues.iter().enumerate() {
            if is_sim_id(r.account) {
                push(e, C::SimHasUid, sec::ACCOUNT_VENUES, i);
            }
        }
        for (i, a) in self.accounts.iter().enumerate() {
            if !live(a)
                && (!self.sim_accounts.iter().any(|s| s.account == a.id)
                    || !self.sim_balances.iter().any(|s| s.account == a.id))
            {
                push(e, C::SimCfg, sec::ACCOUNTS, i);
            }
        }
        let mut seen_sim = vec![false; 256];
        for (i, r) in self.sim_accounts.iter().enumerate() {
            if !is_sim_id(r.account) {
                continue;
            }
            if let Some(s) = seen_sim.get_mut(usize::from(r.account)) {
                if *s {
                    push(e, C::SimCfg, sec::SIM_ACCOUNTS, i);
                }
                *s = true;
            }
        }
        for (i, r) in self.sim_balances.iter().enumerate() {
            if let Some(a) = self.account(r.account)
                && !live(a)
                && (r.venue >= 64 || a.venues & (1u64 << r.venue) == 0)
            {
                push(e, C::SimCfg, sec::SIM_BALANCES, i);
            }
        }
    }

    // ---- step 11: build ----

    fn build(&self, ix: &Index, sha256: [u8; 32], blob: &[u8], warnings: Vec<LoadErr>) -> RefData {
        let st = |s: StrRef| -> Box<str> { self.s(s).into() };
        let asset_exp = |a: u16| {
            self.asset(a).map_or(
                AssetExp {
                    px_exp: 0,
                    qty_exp: 0,
                },
                |a| AssetExp {
                    px_exp: a.px_exp,
                    qty_exp: a.qty_exp,
                },
            )
        };
        // MmrTiers grouped by inst (row order within an inst).
        let mut mmr: Vec<(u32, usize)> = self.mmr.iter().enumerate().map(|(i, r)| (r.inst, i)).collect();
        mmr.sort_unstable();
        let tiers_of = |id: u32| -> Box<[MmrTier]> {
            let lo = mmr.partition_point(|(x, _)| *x < id);
            let hi = mmr.partition_point(|(x, _)| *x <= id);
            mmr.get(lo..hi)
                .unwrap_or(&[])
                .iter()
                .filter_map(|(_, i)| self.mmr.get(*i))
                .map(|r| MmrTier {
                    notional_max: Notional(r.notional_max),
                    mmr: Rate(r.mmr),
                })
                .collect()
        };
        let mut insts = Vec::with_capacity(self.insts.len() + 1);
        let mut inst_meta = Vec::with_capacity(self.insts.len() + 1);
        insts.push(InstHot::NONE);
        inst_meta.push(InstMeta::none());
        for r in &self.insts {
            let x = asset_exp(r.asset);
            let (pw, qw) = self.wire_exps(r);
            let kind = InstKind::from_u8(r.kind).unwrap_or(InstKind::Spot);
            insts.push(InstHot {
                venue: VenueId(r.venue),
                status: InstStatus::from_u8(r.status).unwrap_or(InstStatus::Invalid),
                kind,
                flags: r.flags,
                px_exp: x.px_exp,
                qty_exp: x.qty_exp,
                px_wire_exp: pw as i8,
                qty_wire_exp: qw as i8,
                asset: AssetId(r.asset),
                quote: AssetId(r.quote),
                book_window: r.book_window,
                tick: r.tick,
                lot: r.lot,
                min_qty: Qty(r.min_qty),
                max_qty: Qty(r.max_qty),
                min_notional: Notional(r.min_notional),
                l2_depth: r.l2_depth,
                crossed_tolerance: r.crossed_tolerance,
                _pad: 0,
            });
            inst_meta.push(InstMeta {
                id: InstId(r.id),
                symbol: st(r.symbol),
                venue_num_id: r.venue_num_id,
                px_unit_shift: r.px_unit_shift,
                qty_unit_shift: r.qty_unit_shift,
                tick_str: st(r.tick_str),
                lot_str: st(r.lot_str),
                min_qty_str: st(r.min_qty_str),
                funding_interval_s: r.funding_interval_s,
                funding_anchor_s: r.funding_anchor_s,
                max_leverage: r.max_leverage,
                listed_utc_ns: r.listed_utc_ns,
                delivery_utc_ns: r.delivery_utc_ns,
                mmr_tiers: tiers_of(r.id),
                price_band: PriceBand {
                    up: Rate(r.band_up),
                    down: Rate(r.band_down),
                },
                kind,
            });
        }
        let mut assets = vec![AssetMeta::none()];
        assets.extend(self.assets.iter().map(|a| AssetMeta {
            id: AssetId(a.id),
            name: st(a.name),
            class: AssetClass::from_u8(a.class).unwrap_or(AssetClass::Crypto),
            exp: AssetExp {
                px_exp: a.px_exp,
                qty_exp: a.qty_exp,
            },
            px_max: Px(a.px_max),
            qty_max: Qty(a.qty_max),
            dust: Qty(a.dust),
        }));
        let mut venues: Vec<VenueMeta> = (0..VENUE_SLOTS).map(|_| VenueMeta::none()).collect();
        let mut caps: Vec<VenueCaps> = (0..VENUE_SLOTS)
            .map(|v| VenueCaps::none(VenueId(v as u8)))
            .collect();
        let mut venue_row = [u8::MAX; VENUE_SLOTS];
        let mut caps_row = [u8::MAX; VENUE_SLOTS];
        for (i, v) in self.venues.iter().enumerate() {
            if let Some(slot) = venues.get_mut(usize::from(v.id)) {
                *slot = VenueMeta {
                    id: VenueId(v.id),
                    exchange: Exchange::from_u8(v.exchange).unwrap_or(Exchange::Binance),
                    product: InstKind::from_u8(v.product).unwrap_or(InstKind::Spot),
                    ws_max_streams_per_conn: v.ws_max_streams_per_conn,
                    ws_max_in_msgs_per_s: v.ws_max_in_msgs_per_s,
                    ws_max_conn_age_s: v.ws_max_conn_age_s,
                    ws_new_conn_per_5min_ip: v.ws_new_conn_per_5min_ip,
                    app_ping_interval_s: v.app_ping_interval_s,
                    ts_unit: TsUnit::from_u8(v.ts_unit).unwrap_or(TsUnit::Ms),
                    client_id_max_len: v.client_id_max_len,
                    client_id_prefix: st(v.client_id_prefix),
                    max_batch: v.max_batch,
                    max_inflight_amend: v.max_inflight_amend,
                    crossed_tolerance_default: v.crossed_tolerance_default,
                    stale_guard: StaleGuardKind::from_u8(v.stale_guard).unwrap_or(StaleGuardKind::None),
                };
                if let Some(r) = venue_row.get_mut(usize::from(v.id)) {
                    *r = i as u8;
                }
            }
        }
        for (i, c) in self.caps.iter().enumerate() {
            if let Some(slot) = caps.get_mut(usize::from(c.venue)) {
                *slot = self.decode_caps(c);
                if let Some(r) = caps_row.get_mut(usize::from(c.venue)) {
                    *r = i as u8;
                }
            }
        }
        // Symbol tables per venue.
        let mut per_venue_syms: Vec<Vec<(&[u8], InstId)>> = vec![Vec::new(); VENUE_SLOTS];
        let mut per_venue_nums: Vec<Vec<(u64, InstId)>> = vec![Vec::new(); VENUE_SLOTS];
        let mut insts_of_venue: Vec<Vec<InstId>> = vec![Vec::new(); VENUE_SLOTS];
        let mut insts_of_asset: Vec<Vec<InstId>> = vec![Vec::new(); self.assets.len() + 1];
        for r in &self.insts {
            let v = usize::from(r.venue);
            if let Some(s) = per_venue_syms.get_mut(v) {
                s.push((self.s(r.symbol).as_bytes(), InstId(r.id)));
            }
            if let Some(s) = per_venue_nums.get_mut(v) {
                s.push((r.venue_num_id, InstId(r.id)));
            }
            if let Some(s) = insts_of_venue.get_mut(v) {
                s.push(InstId(r.id));
            }
            if let Some(s) = insts_of_asset.get_mut(usize::from(r.asset)) {
                s.push(InstId(r.id));
            }
        }
        for a in &self.aliases {
            if let Some(s) = per_venue_syms.get_mut(usize::from(a.venue)) {
                s.push((self.s(a.alias).as_bytes(), InstId(a.inst)));
            }
        }
        let symbols: Box<[SymbolIndex]> = per_venue_syms
            .iter()
            .zip(&per_venue_nums)
            .map(|(s, n)| {
                if s.is_empty() {
                    SymbolIndex::default()
                } else {
                    SymbolIndex::from_entries(s, n)
                }
            })
            .collect();
        // Accounts.
        let mut accounts = vec![AccountMeta::none()];
        let mut tiers = vec![0u8];
        for a in &self.accounts {
            let kind = if a.kind == SIM {
                let cfg = self.sim_accounts.iter().find(|s| s.account == a.id);
                AccountKind::Sim(SimAccountCfg {
                    initial_balances: self
                        .sim_balances
                        .iter()
                        .filter(|b| b.account == a.id)
                        .map(|b| SimBalance {
                            venue: VenueId(b.venue),
                            asset: AssetId(b.asset),
                            qty: Qty(b.qty),
                        })
                        .collect(),
                    latency_model: cfg.map_or(0, |c| c.latency_model),
                    fee_tier: cfg.map_or(0, |c| c.fee_tier),
                    margin_model: cfg.map_or(0, |c| c.margin_model),
                })
            } else {
                AccountKind::Live
            };
            tiers.push(self.sim_tier(a));
            accounts.push(AccountMeta {
                id: AccountId(a.id),
                kind,
                bound_instance: StrategyId(a.bound_instance),
                is_prod: a.is_prod == 1,
                venues: a.venues,
                label: st(a.label),
                uid_policy: UidPolicy::from_u8(a.uid_policy).unwrap_or(UidPolicy::Verified),
            });
        }
        let instances: Box<[InstanceMeta]> = self
            .instances
            .iter()
            .map(|r| InstanceMeta {
                id: StrategyId(r.id),
                kind_id: r.kind_id,
                name: st(r.name),
            })
            .collect();
        let mut instance_account = vec![0u8; self.instances.len() + 1];
        for a in &self.accounts {
            if let Some(s) = instance_account.get_mut(usize::from(a.bound_instance))
                && a.bound_instance != 0
            {
                *s = a.id;
            }
        }
        let fees = FeeTable::build(&FeeRows::new(&self.fees, &self.fee_overrides), &tiers);
        let mut rate_limits: Vec<Vec<RateLimit>> = vec![Vec::new(); VENUE_SLOTS];
        for r in &self.limits {
            if let Some(s) = rate_limits.get_mut(usize::from(r.venue)) {
                s.push(RateLimit {
                    scope: LimitScope::from_u8(r.scope).unwrap_or(LimitScope::Ip),
                    bucket: LimitBucket::from_u8(r.bucket).unwrap_or(LimitBucket::Orders),
                    limit: r.limit,
                    window_ms: r.window_ms,
                    default_cost: r.default_cost,
                });
            }
        }
        let mut endpoints: Vec<Vec<Endpoint>> = vec![Vec::new(); VENUE_SLOTS * 3];
        for r in &self.endpoints {
            let env = usize::from(r.env).saturating_sub(1);
            if let Some(s) = endpoints.get_mut(usize::from(r.venue) * 3 + env)
                && usize::from(r.venue) < VENUE_SLOTS
            {
                s.push(Endpoint {
                    role: EndpointRole::from_u8(r.role).unwrap_or(EndpointRole::Rest),
                    env: EndpointEnv::from_u8(r.env).unwrap_or(EndpointEnv::Prod),
                    url: st(r.url),
                    conn_group: r.conn_group,
                });
            }
        }
        let mut maint: Vec<Vec<MaintWindow>> = vec![Vec::new(); VENUE_SLOTS];
        for r in &self.maint {
            if let Some(s) = maint.get_mut(usize::from(r.venue)) {
                s.push(MaintWindow {
                    venue: VenueId(r.venue),
                    inst: InstId(r.inst),
                    start_ns: r.start_ns,
                    end_ns: r.end_ns,
                    kind: MaintKind::from_u8(r.kind).unwrap_or(MaintKind::VenueMaintenance),
                });
            }
        }
        let tokens: Box<[ChainToken]> = self
            .tokens
            .iter()
            .map(|t| ChainToken {
                chain: VenueId(t.chain),
                asset: AssetId(t.asset),
                address: t.address,
                decimals: t.decimals,
            })
            .collect();
        let account_venues: Box<[(AccountId, VenueId, Box<str>)]> = self
            .account_venues
            .iter()
            .map(|r| (AccountId(r.account), VenueId(r.venue), st(r.uid)))
            .collect();
        let mut uid_index: Vec<(u8, u8, u32)> = self
            .account_venues
            .iter()
            .enumerate()
            .map(|(i, r)| (r.account, r.venue, i as u32))
            .collect();
        uid_index.sort_unstable();
        let mut venue_account_ix = vec![u8::MAX; (self.accounts.len() + 1) * VENUE_SLOTS];
        for v in 0..VENUE_SLOTS {
            let mut n = 0u8;
            for a in self
                .accounts
                .iter()
                .filter(|a| a.kind == LIVE && a.venues & (1u64 << v) != 0)
            {
                if let Some(s) = venue_account_ix.get_mut(usize::from(a.id) * VENUE_SLOTS + v) {
                    *s = n;
                }
                n = n.saturating_add(1);
            }
        }
        let _ = ix;
        RefData {
            version: self.hdr.version,
            created_utc_ns: self.hdr.created_utc_ns,
            sha256,
            blob: blob.into(),
            insts: insts.into(),
            inst_meta: inst_meta.into(),
            assets: assets.into(),
            venues: venues.into(),
            venue_order: self.venues.iter().map(|v| VenueId(v.id)).collect(),
            venue_row,
            caps: caps.into(),
            caps_row,
            symbols,
            accounts: accounts.into(),
            instances,
            instance_account: instance_account.into(),
            fees,
            insts_of_asset: insts_of_asset.into_iter().map(Into::into).collect(),
            insts_of_venue: insts_of_venue.into_iter().map(Into::into).collect(),
            rate_limits: rate_limits.into_iter().map(Into::into).collect(),
            endpoints: endpoints.into_iter().map(Into::into).collect(),
            maint: maint.into_iter().map(Into::into).collect(),
            tokens,
            account_venues,
            uid_index: uid_index.into(),
            venue_account_ix: venue_account_ix.into(),
            warnings: warnings.into(),
            none: crate::NoneRows::new(),
        }
    }

    fn decode_caps(&self, c: &CapsRow) -> VenueCaps {
        VenueCaps {
            venue: VenueId(c.venue),
            market: market_of(c.market).unwrap_or(MarketKind::Spot),
            fee_asset_rule: FeeAssetRule::decode(c.fee_rule, c.fee_asset).unwrap_or(FeeAssetRule::Quote),
            reduce_only: c.reduce_only == 1,
            required_position_mode: PositionMode::from_u8(c.position_mode)
                .unwrap_or(PositionMode::NotApplicable),
            required_margin_mode: MarginMode::from_u8(c.margin_mode).unwrap_or(MarginMode::NotApplicable),
            required_multi_asset: opt_bool_decode(c.multi_asset).flatten(),
            required_spot_borrow: opt_bool_decode(c.spot_borrow).flatten(),
            funding_settle: FundingSettleKind::from_u8(c.funding_settle)
                .unwrap_or(FundingSettleKind::NotApplicable),
            margin_data: MarginData::decode(c.margin_data, c.margin_poll_ms)
                .unwrap_or(MarginData::NotApplicable),
            own_liquidation_marker: LiqMarker::from_u8(c.liq_marker).unwrap_or(LiqMarker::NotIdentifiable),
            liq_marker_arg: self.s(c.liq_marker_arg).into(),
            ignorable_user_events: self
                .events(c)
                .unwrap_or(&[])
                .iter()
                .map(|s| self.s(*s).into())
                .collect(),
            flags: crate::caps::CapFlags(c.flags),
            amend_qty_down_only: c.amend_qty_down_only == 1,
            dms: DeadManCap::from_u8(c.dms).unwrap_or(DeadManCap::None),
            dms_min_ms: c.dms_min_ms,
            dms_max_ms: c.dms_max_ms,
            dms_timeout_ms: c.dms_timeout_ms,
            dms_refresh_ms: c.dms_refresh_ms,
            testnet: TestnetKind::from_u8(c.testnet).unwrap_or(TestnetKind::None),
            _pad: 0,
        }
    }
}

/// Step 10: cross-version compatibility of `new` against `cur` (rows of `cur`).
fn step10_versions(new: &RefData, cur: &RefData, e: &mut Vec<LoadErr>) {
    if new.version <= cur.version {
        push(e, C::VersionNotIncreasing, 0, 0);
    }
    let n_new = new.insts.len().saturating_sub(1);
    for (i, m) in cur.inst_meta.iter().enumerate().skip(1) {
        let (old, id) = (cur.inst(m.id), m.id);
        let same = (id.0 as usize) <= n_new
            && new.inst(id).venue == old.venue
            && new.inst(id).asset == old.asset
            && new.inst_meta(id).symbol == m.symbol;
        if !same {
            push(e, C::InstRemoved, sec::INSTS, i - 1);
        }
    }
    let n_assets = new.assets.len().saturating_sub(1);
    for (i, a) in cur.assets.iter().enumerate().skip(1) {
        let b = new.asset(a.id);
        if usize::from(a.id.0) > n_assets || b.name != a.name || b.exp != a.exp {
            push(e, C::AssetChanged, sec::ASSETS, i - 1);
        }
    }
    for (i, m) in cur.inst_meta.iter().enumerate().skip(1) {
        if (m.id.0 as usize) <= n_new {
            let b = new.inst_meta(m.id);
            if b.px_unit_shift != m.px_unit_shift || b.qty_unit_shift != m.qty_unit_shift {
                push(e, C::MultiplierChanged, sec::INSTS, i - 1);
            }
        }
    }
    let n_acc = new.accounts.len().saturating_sub(1);
    for (i, a) in cur.accounts.iter().enumerate().skip(1) {
        let b = new.account(a.id);
        if usize::from(a.id.0) > n_acc || b.is_prod != a.is_prod || b.kind.code() != a.kind.code() {
            push(e, C::AccountKindChanged, sec::ACCOUNTS, i - 1);
        }
    }
    for (i, (acc, venue, uid)) in cur.account_venues.iter().enumerate() {
        let both_live = matches!(cur.account(*acc).kind, AccountKind::Live)
            && usize::from(acc.0) <= n_acc
            && matches!(new.account(*acc).kind, AccountKind::Live);
        if both_live
            && let Some(new_uid) = new.raw_uid(*acc, *venue)
            && new_uid != &**uid
        {
            push(e, C::UidChanged, sec::ACCOUNT_VENUES, i);
        }
    }
    for (i, s) in cur.instances.iter().enumerate() {
        let same = new
            .instance(s.id)
            .is_some_and(|b| b.kind_id == s.kind_id && b.name == s.name);
        if !same {
            push(e, C::InstanceChanged, sec::INSTANCES, i);
        }
    }
}

/// Decoded rows of a structurally valid blob (for the review dump).
#[cfg(feature = "tools")]
pub(crate) fn raw_tables(blob: &[u8]) -> Option<Tables<'_>> {
    let mut e = Vec::new();
    let (hdr, sects) = parse_container(blob, Kind::Rdb, &mut e)?;
    Some(Tables::decode(hdr, &sects))
}

/// `RefData::load` (README "加载校验").
pub(crate) fn load(blob: &[u8], current: Option<&RefData>) -> Result<RefData, LoadReport> {
    let sha256: [u8; 32] = Sha256::digest(blob).into();
    if let Some(c) = current
        && c.sha256 == sha256
    {
        return Err(LoadReport {
            errors: vec![LoadErr::new(C::AlreadyActive, 0, 0)],
        });
    }
    let mut e = Vec::new();
    let Some((hdr, sects)) = parse_container(blob, Kind::Rdb, &mut e) else {
        return Err(LoadReport { errors: e });
    };
    let t = Tables::decode(hdr, &sects);
    t.step1_enums(&mut e);
    done(&mut e)?;
    t.step2_strings(&mut e);
    done(&mut e)?;
    t.step3_ids(&mut e);
    done(&mut e)?;
    let ix = t.index();
    t.step4_refs(&ix, &mut e);
    done(&mut e)?;
    t.step5_caps(&ix, &mut e);
    done(&mut e)?;
    t.step6_exps(&mut e);
    done(&mut e)?;
    t.step7_steps(&mut e);
    done(&mut e)?;
    t.step8_ranges(&mut e);
    done(&mut e)?;
    let mut warnings = Vec::new();
    t.step9_integrity(&ix, &mut e, &mut warnings);
    done(&mut e)?;
    let rd = t.build(&ix, sha256, blob, warnings);
    if let Some(c) = current {
        step10_versions(&rd, c, &mut e);
        done(&mut e)?;
    }
    Ok(rd)
}
