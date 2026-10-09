//! `LedgerConfig` (the `ledger` section of `EngineConfig`, ECF1 section 2), its row format and
//! validator, and the engine capacities the ledger sizes its tables from.

use hft_refdata::{
    EngineConfig, EngineSection, EngineValidator, InstKind, InstStatus, LoadErr, LoadErrCode, RefData,
};
use hft_types::{AssetId, InstId, Notional, Rate};
use zerocopy::{FromBytes, FromZeros, Immutable, IntoBytes, KnownLayout};

use crate::fixed::FixedVec;

/// Fixed 16-byte asset name (ASCII, right-padded with 0); config refers to assets by name so the
/// defaults do not depend on refdata numbering.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct AssetName(pub [u8; 16]);

impl AssetName {
    /// Name from a string; longer than 16 bytes is truncated.
    pub fn new(s: &str) -> AssetName {
        let mut b = [0u8; 16];
        for (d, s) in b.iter_mut().zip(s.bytes()) {
            *d = s;
        }
        AssetName(b)
    }

    /// The name bytes without padding.
    pub fn as_bytes(&self) -> &[u8] {
        let n = self.0.iter().position(|b| *b == 0).unwrap_or(16);
        self.0.get(..n).unwrap_or(&[])
    }

    pub fn matches(&self, name: &str) -> bool {
        self.as_bytes() == name.as_bytes()
    }
}

/// Spot valuation price.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum SpotVal {
    Bid,
    Mid,
}

/// Perpetual valuation price.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum PerpVal {
    Mark,
    /// Long at the best bid, short at the best ask.
    ExitSide,
}

/// Valuation rule of total equity and unrealized PnL.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct Valuation {
    pub spot: SpotVal,
    pub perp: PerpVal,
    pub include_baseline: bool,
    pub include_unassigned: bool,
}

/// The `ledger` section of `EngineConfig` (README "配置").
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LedgerConfig {
    /// Explicit asset → reference inst overrides; other assets use `default_ref_inst`.
    pub ref_inst: FixedVec<(AssetId, InstId), 32>,
    /// Always valued at 1.0.
    pub par_assets: FixedVec<AssetName, 8>,
    /// Valued at 1.0 with `RATE_STALE` when their reference quote is missing or stale.
    pub stable_par_fallback: FixedVec<AssetName, 8>,
    pub rate_max_age_ms: u32,
    pub valuation: Valuation,
    pub round_dust_notional: Notional,
    pub balance_check_quiet_ms: u32,
    pub funding_tol_abs: Notional,
    pub funding_tol_rel: Rate,
    pub funding_recon_lag_ms: u32,
    pub funding_recon_window_ms: u64,
    pub funding_hist_cap: u32,
    pub funding_push_seen_cap: u16,
    pub ext_dedup_cap: u32,
}

impl Default for LedgerConfig {
    fn default() -> Self {
        let mut par = FixedVec::new();
        par.push(AssetName::new("USDT"));
        let mut fb = FixedVec::new();
        for n in ["USDC", "USD1", "FDUSD"] {
            fb.push(AssetName::new(n));
        }
        LedgerConfig {
            ref_inst: FixedVec::new(),
            par_assets: par,
            stable_par_fallback: fb,
            rate_max_age_ms: 60_000,
            valuation: Valuation {
                spot: SpotVal::Bid,
                perp: PerpVal::Mark,
                include_baseline: false,
                include_unassigned: true,
            },
            round_dust_notional: Notional(5_000_000),
            balance_check_quiet_ms: 2_000,
            funding_tol_abs: Notional(50_000),
            funding_tol_rel: Rate(5_000_000),
            funding_recon_lag_ms: 600_000,
            funding_recon_window_ms: 172_800_000,
            funding_hist_cap: 4_096,
            funding_push_seen_cap: 64,
            ext_dedup_cap: 1_024,
        }
    }
}

/// `true` when `a` is one of `names` in refdata.
pub(crate) fn in_names(rd: &RefData, a: AssetId, names: &FixedVec<AssetName, 8>) -> bool {
    let n = &rd.asset(a).name;
    names.iter().any(|x| x.matches(n))
}

/// Default reference inst of `asset`: the lowest-id tradable spot inst with base `asset` whose
/// quote is a par asset of `par`; none → `INST_NONE`.
pub fn default_ref_inst(rd: &RefData, par: &FixedVec<AssetName, 8>, asset: AssetId) -> InstId {
    for &i in rd.insts_of_asset(asset) {
        let h = rd.inst(i);
        if h.kind == InstKind::Spot
            && matches!(h.status, InstStatus::Trading | InstStatus::ReduceOnly)
            && in_names(rd, h.quote, par)
        {
            return i;
        }
    }
    InstId(0)
}

impl LedgerConfig {
    /// Reference inst of `asset`: the explicit override, else `default_ref_inst`.
    pub fn ref_inst_for(&self, rd: &RefData, asset: AssetId) -> InstId {
        match self.ref_inst.iter().find(|(a, _)| *a == asset) {
            Some((_, i)) => *i,
            None => default_ref_inst(rd, &self.par_assets, asset),
        }
    }

    /// Encodes this configuration as the ECF1 section row (`LedgerConfigRow`, row version 1).
    pub fn to_row(&self) -> LedgerConfigRow {
        let mut r = LedgerConfigRow::new_zeroed();
        r.rate_max_age_ms = self.rate_max_age_ms;
        r.balance_check_quiet_ms = self.balance_check_quiet_ms;
        r.funding_recon_lag_ms = self.funding_recon_lag_ms;
        r.funding_hist_cap = self.funding_hist_cap;
        r.ext_dedup_cap = self.ext_dedup_cap;
        r.funding_push_seen_cap = self.funding_push_seen_cap;
        r.val_spot = match self.valuation.spot {
            SpotVal::Bid => 1,
            SpotVal::Mid => 2,
        };
        r.val_perp = match self.valuation.perp {
            PerpVal::Mark => 1,
            PerpVal::ExitSide => 2,
        };
        r.include_baseline = u8::from(self.valuation.include_baseline);
        r.include_unassigned = u8::from(self.valuation.include_unassigned);
        r.n_par = self.par_assets.len() as u8;
        r.n_fallback = self.stable_par_fallback.len() as u8;
        r.n_ref = self.ref_inst.len() as u8;
        r.round_dust_notional = self.round_dust_notional.0;
        r.funding_tol_abs = self.funding_tol_abs.0;
        r.funding_tol_rel = self.funding_tol_rel.0;
        r.funding_recon_window_ms = self.funding_recon_window_ms;
        for (d, s) in r.par.iter_mut().zip(self.par_assets.iter()) {
            *d = s.0;
        }
        for (d, s) in r.fallback.iter_mut().zip(self.stable_par_fallback.iter()) {
            *d = s.0;
        }
        for (k, (a, i)) in self.ref_inst.iter().enumerate() {
            if let Some(x) = r.ref_asset.get_mut(k) {
                *x = a.0;
            }
            if let Some(x) = r.ref_inst.get_mut(k) {
                *x = i.0;
            }
        }
        r
    }

    /// Decodes a `LedgerConfigRow`; illegal enum bytes, booleans, counts or padding → `None`.
    pub fn from_row(r: &LedgerConfigRow) -> Option<LedgerConfig> {
        let spot = match r.val_spot {
            1 => SpotVal::Bid,
            2 => SpotVal::Mid,
            _ => return None,
        };
        let perp = match r.val_perp {
            1 => PerpVal::Mark,
            2 => PerpVal::ExitSide,
            _ => return None,
        };
        if r.include_baseline > 1 || r.include_unassigned > 1 || r._pad != [0; 3] {
            return None;
        }
        if r.n_par > 8 || r.n_fallback > 8 || r.n_ref > 32 {
            return None;
        }
        let mut c = LedgerConfig {
            ref_inst: FixedVec::new(),
            par_assets: FixedVec::new(),
            stable_par_fallback: FixedVec::new(),
            rate_max_age_ms: r.rate_max_age_ms,
            valuation: Valuation {
                spot,
                perp,
                include_baseline: r.include_baseline == 1,
                include_unassigned: r.include_unassigned == 1,
            },
            round_dust_notional: Notional(r.round_dust_notional),
            balance_check_quiet_ms: r.balance_check_quiet_ms,
            funding_tol_abs: Notional(r.funding_tol_abs),
            funding_tol_rel: Rate(r.funding_tol_rel),
            funding_recon_lag_ms: r.funding_recon_lag_ms,
            funding_recon_window_ms: r.funding_recon_window_ms,
            funding_hist_cap: r.funding_hist_cap,
            funding_push_seen_cap: r.funding_push_seen_cap,
            ext_dedup_cap: r.ext_dedup_cap,
        };
        for n in r.par.iter().take(usize::from(r.n_par)) {
            c.par_assets.push(AssetName(*n));
        }
        for n in r.fallback.iter().take(usize::from(r.n_fallback)) {
            c.stable_par_fallback.push(AssetName(*n));
        }
        for k in 0..usize::from(r.n_ref) {
            let a = r.ref_asset.get(k).copied().unwrap_or(0);
            let i = r.ref_inst.get(k).copied().unwrap_or(0);
            c.ref_inst.push((AssetId(a), InstId(i)));
        }
        Some(c)
    }

    /// The ledger section of `ec` (exactly one `LedgerConfigRow`, row version 1).
    pub fn from_engine(ec: &EngineConfig) -> Result<LedgerConfig, LoadErr> {
        let s = ec.section(EngineSection::Ledger).ok_or(LoadErr::new(
            LoadErrCode::Structure,
            EngineSection::Ledger as u16,
            0,
        ))?;
        let err = LoadErr::new(LoadErrCode::Structure, EngineSection::Ledger as u16, 0);
        if s.row_ver != LEDGER_ROW_VERSION
            || s.count != 1
            || (s.row_size as usize) < size_of::<LedgerConfigRow>()
        {
            return Err(err);
        }
        let b = s.bytes.get(..size_of::<LedgerConfigRow>()).ok_or(err)?;
        let r = LedgerConfigRow::read_from_bytes(b).map_err(|_| err)?;
        LedgerConfig::from_row(&r).ok_or(LoadErr::new(
            LoadErrCode::ValueRange,
            EngineSection::Ledger as u16,
            0,
        ))
    }
}

/// Row version of ECF1 section 2.
pub const LEDGER_ROW_VERSION: u16 = 1;

/// ECF1 section 2 row (one row, 512 B, little-endian).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct LedgerConfigRow {
    pub rate_max_age_ms: u32,
    pub balance_check_quiet_ms: u32,
    pub funding_recon_lag_ms: u32,
    pub funding_hist_cap: u32,
    pub ext_dedup_cap: u32,
    pub funding_push_seen_cap: u16,
    /// Bid 1, Mid 2.
    pub val_spot: u8,
    /// Mark 1, ExitSide 2.
    pub val_perp: u8,
    pub include_baseline: u8,
    pub include_unassigned: u8,
    pub n_par: u8,
    pub n_fallback: u8,
    pub n_ref: u8,
    pub _pad: [u8; 3],
    pub round_dust_notional: i64,
    pub funding_tol_abs: i64,
    pub funding_tol_rel: i64,
    pub funding_recon_window_ms: u64,
    pub par: [[u8; 16]; 8],
    pub fallback: [[u8; 16]; 8],
    pub ref_asset: [u16; 32],
    pub ref_inst: [u32; 32],
}
const _: () = assert!(size_of::<LedgerConfigRow>() == 512);
const _: () = assert!(core::mem::offset_of!(LedgerConfigRow, round_dust_notional) == 32);
const _: () = assert!(core::mem::offset_of!(LedgerConfigRow, par) == 64);
const _: () = assert!(core::mem::offset_of!(LedgerConfigRow, ref_asset) == 320);
const _: () = assert!(core::mem::offset_of!(LedgerConfigRow, ref_inst) == 384);

/// Semantic check of the ledger section: decodes, then `RefData::check_ref_inst` on every
/// explicit `ref_inst` (`RefInstNotSubscribed`).
pub fn check_ledger_section(ec: &EngineConfig, rd: &RefData) -> Result<(), LoadErr> {
    let c = LedgerConfig::from_engine(ec)?;
    for (_, i) in c.ref_inst.iter() {
        rd.check_ref_inst(*i)
            .map_err(|e| LoadErr::new(e.code, EngineSection::Ledger as u16, e.row))?;
    }
    Ok(())
}

/// The ledger section's `EngineValidator` (passed to `EngineConfig::validate`).
pub const LEDGER_VALIDATOR: EngineValidator = EngineValidator {
    section: EngineSection::Ledger,
    check: check_ledger_section,
};

/// Engine capacities (ECF1 section 3 `CapacitiesRow`; field semantics belong to hft-sequencer).
/// The ledger uses `max_subs_per_inst` and `max_cells_total`.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Capacities {
    pub max_insts: u32,
    pub max_cells_total: u32,
    pub max_timers: u32,
    pub recent_cmd_ids: u32,
    pub max_subs_per_inst: u16,
    pub max_groups_per_instance: u16,
    pub max_instances: u16,
}
