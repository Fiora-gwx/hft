//! `OmsConfig` (ECF1 section 1 `oms`; change = restart) and its per-venue / per-table resolution.

use hft_refdata::{RefData, StaleGuardKind};
use hft_types::{AccountId, Rate, VenueId};

/// A value with a default and per-venue overrides.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PerVenue<T: Copy> {
    pub default: T,
    pub overrides: Vec<(VenueId, T)>,
}

impl<T: Copy> PerVenue<T> {
    pub fn new(default: T) -> Self {
        PerVenue {
            default,
            overrides: Vec::new(),
        }
    }
    /// Value for `v` (the last override for `v` wins).
    pub fn get(&self, v: VenueId) -> T {
        self.overrides
            .iter()
            .rev()
            .find(|(x, _)| *x == v)
            .map_or(self.default, |(_, t)| *t)
    }
    pub fn set(&mut self, v: VenueId, t: T) {
        self.overrides.push((v, t));
    }
}

/// A value with a default and per-(venue, account) overrides.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PerTable<T: Copy> {
    pub default: T,
    pub overrides: Vec<(VenueId, AccountId, T)>,
}

impl<T: Copy> PerTable<T> {
    pub fn new(default: T) -> Self {
        PerTable {
            default,
            overrides: Vec::new(),
        }
    }
    pub fn get(&self, v: VenueId, a: AccountId) -> T {
        self.overrides
            .iter()
            .rev()
            .find(|(x, y, _)| *x == v && *y == a)
            .map_or(self.default, |(_, _, t)| *t)
    }
    pub fn set(&mut self, v: VenueId, a: AccountId, t: T) {
        self.overrides.push((v, a, t));
    }
}

/// OMS configuration (README "配置项"). Built with [`OmsConfig::new`] (README defaults) and
/// adjusted field by field; resolved once by `Oms::new`/`Oms::restore`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OmsConfig {
    /// Current `engine_epoch` (slots carry its low 15 bits).
    pub engine_epoch: u32,
    /// TSC frequency of this run (Hz × 1000); every `*_ms` value converts with it.
    pub tsc_hz_milli: u64,
    pub slots: PerTable<u32>,
    pub links_per_account: u32,
    /// LegTracker capacity (number of (account, group) entries).
    pub max_groups: u32,
    pub ack_timeout_ms: PerVenue<u32>,
    pub cancel_timeout_ms: PerVenue<u32>,
    pub ioc_timeout_ms: PerVenue<u32>,
    pub unknown_query_interval_ms: u32,
    pub unknown_max_queries: u8,
    pub unknown_alarm_ms: u32,
    /// Default `max_out_per_event × 4` when unset.
    pub out_quota: PerTable<Option<u16>>,
    /// Default: the venue's expiry window + 500, or 2 000 without expiry protection.
    pub not_found_grace_ms: PerVenue<Option<u32>>,
    /// Expiry window of each venue's order requests (ECF1 `oms` row; `recvWindow`/`expTime`).
    pub expire_window_ms: PerVenue<Option<u32>>,
    pub smoke_far_bp: Rate,
    pub reuse_quarantine_ms: u32,
    pub tombstone_gens: u8,
    pub tombstone_keep_ms: u32,
    pub sweep_batch: u16,
    pub max_out_per_event: u16,
    pub hedge_retry_max: u8,
    pub hedge_retry_min_ms: u32,
    pub hedge_deadline_ms: u32,
    pub resume_hold_max_ms: u32,
    pub foreign_fill_track: PerTable<u16>,
    pub ext_held_track: PerTable<u16>,
    pub decode_fail_down_n: u8,
}

impl OmsConfig {
    /// README defaults for a run with this epoch and TSC frequency.
    pub fn new(engine_epoch: u32, tsc_hz_milli: u64) -> OmsConfig {
        OmsConfig {
            engine_epoch,
            tsc_hz_milli,
            slots: PerTable::new(4096),
            links_per_account: 4096,
            max_groups: 4096,
            ack_timeout_ms: PerVenue::new(2000),
            cancel_timeout_ms: PerVenue::new(2000),
            ioc_timeout_ms: PerVenue::new(1000),
            unknown_query_interval_ms: 1000,
            unknown_max_queries: 5,
            unknown_alarm_ms: 30_000,
            out_quota: PerTable::new(None),
            not_found_grace_ms: PerVenue::new(None),
            expire_window_ms: PerVenue::new(None),
            smoke_far_bp: Rate(30_000_000),
            reuse_quarantine_ms: 5000,
            tombstone_gens: 2,
            tombstone_keep_ms: 3_600_000,
            sweep_batch: 256,
            max_out_per_event: 16,
            hedge_retry_max: 3,
            hedge_retry_min_ms: 100,
            hedge_deadline_ms: 3000,
            resume_hold_max_ms: 30_000,
            foreign_fill_track: PerTable::new(64),
            ext_held_track: PerTable::new(64),
            decode_fail_down_n: 3,
        }
    }

    /// Effective `out_quota` of a table.
    pub fn out_quota_of(&self, v: VenueId, a: AccountId) -> u16 {
        self.out_quota
            .get(v, a)
            .unwrap_or_else(|| self.max_out_per_event.saturating_mul(4))
    }

    /// Effective `not_found_grace_ms` of a venue: the override, else the expiry window + 500 for a
    /// venue with expiry protection, else 2 000.
    pub fn not_found_grace_of(&self, v: VenueId, rd: &RefData) -> u32 {
        if let Some(g) = self.not_found_grace_ms.get(v) {
            return g;
        }
        let guarded = !matches!(rd.venue(v).stale_guard, StaleGuardKind::None);
        match (guarded, self.expire_window_ms.get(v)) {
            (true, Some(w)) => w.saturating_add(500),
            _ => 2000,
        }
    }

    /// TSC ticks per millisecond of this run.
    pub fn tsc_per_ms(&self) -> u64 {
        (self.tsc_hz_milli / 1_000_000).max(1)
    }
}
