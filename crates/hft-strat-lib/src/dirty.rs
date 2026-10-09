//! `DirtyFilter` (README "DirtyFilter"): stateless per-event check of quote health (CEX legs by
//! connection health — quiet is not stale — external-quote legs by the last quote time), resync,
//! absurd spread and a no-quote streak.

use hft_strategy_sdk::BboView;
use hft_types::{Rate, Tsc};

use crate::units::ms_to_tsc;

/// Quote-health input of one leg (TSC values of this run).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct LegFresh {
    pub live_tsc: Tsc,
    pub conn_ok: bool,
    pub resync: bool,
}

impl LegFresh {
    /// CEX leg, the engine's own judgement (`BboView::quote_live`): `live_tsc = conn_live_tsc`,
    /// `conn_ok = conn_up`, `resync = resync`; the quote's last change (`rx_tsc`) is not used.
    #[inline]
    pub const fn from_bbo(b: &BboView) -> LegFresh {
        LegFresh {
            live_tsc: b.conn_live_tsc,
            conn_ok: b.conn_up,
            resync: b.resync,
        }
    }

    /// External-quote leg (quotes per block or request; no new quote means stale):
    /// `live_tsc` = time of the latest quote.
    #[inline]
    pub const fn from_quote(quote_tsc: Tsc, conn_ok: bool, resync: bool) -> LegFresh {
        LegFresh {
            live_tsc: quote_tsc,
            conn_ok,
            resync,
        }
    }
}

/// Both legs plus the caller-maintained slow-side no-quote streak.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Freshness {
    pub fast: LegFresh,
    pub slow: LegFresh,
    pub no_quote_streak: u16,
}

/// Dirty-filter parameters; 0 disables that comparison. For CEX legs `stale_*_ms` is the
/// connection silence limit (not how long the quote has not changed).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct DirtyParams {
    pub stale_fast_ms: u32,
    pub stale_slow_ms: u32,
    pub max_abs_spread: Rate,
    pub junk_after: u16,
}

/// [`DirtyParams`] with the two durations in TSC ticks (`× Now.tsc_per_ms`).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct DirtyParamsTsc {
    pub stale_fast: u64,
    pub stale_slow: u64,
    pub max_abs_spread: Rate,
    pub junk_after: u16,
}

impl DirtyParams {
    #[inline]
    pub const fn tsc(&self, tsc_per_ms: u64) -> DirtyParamsTsc {
        DirtyParamsTsc {
            stale_fast: ms_to_tsc(self.stale_fast_ms, tsc_per_ms),
            stale_slow: ms_to_tsc(self.stale_slow_ms, tsc_per_ms),
            max_abs_spread: self.max_abs_spread,
            junk_after: self.junk_after,
        }
    }
}

/// First failing check.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum DirtyReason {
    Stale,
    Resync,
    AbsSpread,
    Junk,
}

/// The filter (stateless).
#[derive(Clone, Copy, Debug, Default)]
pub struct DirtyFilter;

#[inline]
const fn leg_stale(l: &LegFresh, now: Tsc, limit: u64) -> bool {
    if !l.conn_ok {
        return true;
    }
    if limit == 0 {
        return false;
    }
    // `now < live_tsc` → age 0.
    now.0.saturating_sub(l.live_tsc.0) > limit
}

impl DirtyFilter {
    /// In order, first hit wins: fast leg stale → `Stale`; slow leg stale → `Stale`; either leg
    /// resyncing → `Resync`; `|spread| > max_abs_spread` → `AbsSpread`;
    /// `no_quote_streak ≥ junk_after` → `Junk`. A 0 parameter disables only its comparison
    /// (`conn_ok = false` is `Stale` regardless).
    #[inline]
    pub fn check(spread: Rate, fr: &Freshness, now: Tsc, p: &DirtyParamsTsc) -> Result<(), DirtyReason> {
        if leg_stale(&fr.fast, now, p.stale_fast) || leg_stale(&fr.slow, now, p.stale_slow) {
            return Err(DirtyReason::Stale);
        }
        if fr.fast.resync || fr.slow.resync {
            return Err(DirtyReason::Resync);
        }
        if p.max_abs_spread.0 > 0 && spread.0.unsigned_abs() > p.max_abs_spread.0.unsigned_abs() {
            return Err(DirtyReason::AbsSpread);
        }
        if p.junk_after != 0 && fr.no_quote_streak >= p.junk_after {
            return Err(DirtyReason::Junk);
        }
        Ok(())
    }
}
