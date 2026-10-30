//! The latency-condition tracker (README "条件延迟"): the `Cond` of a decision from the market
//! records the simulation is fed. Venue load = the previous complete UTC minute's median of
//! per-second md-lag p90 on the tracked venues (against `load_thr_ns`); move = the reference
//! quote's mid change over the last 1 s (≤ 0.25 / ≤ 1 / > 1 bp); phase = outside the first
//! `phase_s` of each `window_s`. Shared by research replay and the sim-gw.

use std::collections::VecDeque;

use hft_event::{Bbo, EventKind, MarketHdr};
use hft_types::{InstId, Tsc};
use zerocopy::FromBytes;

use crate::Cond;

/// Thresholds and window lengths of the condition buckets.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CondCfg {
    /// Load bucket = the number of thresholds below the minute value.
    pub load_thr_ns: Vec<i64>,
    pub window_s: u32,
    pub phase_s: u32,
}

impl Default for CondCfg {
    fn default() -> Self {
        CondCfg {
            load_thr_ns: Vec::new(),
            window_s: 300,
            phase_s: 5,
        }
    }
}

/// Rolling state of the condition inputs.
#[derive(Debug)]
pub struct CondTracker {
    pub cfg: CondCfg,
    /// Tracked venues (bit per `VenueId`, low 6 bits); market data of other venues feeds the
    /// reference quote.
    venues: u64,
    /// `tsc_hz_milli` of the run.
    hz_milli: u64,
    sec: i64,
    lags: Vec<i64>,
    minute: i64,
    p90s: Vec<i64>,
    /// Load bucket of the previous complete minute (1 until one is known).
    load: u8,
    ref_inst: Option<InstId>,
    /// `(record tsc, bid + ask)` of the reference over the last 2 s.
    ref_hist: VecDeque<(u64, i128)>,
}

impl CondTracker {
    /// `venues`: bit per `VenueId` (refdata `AccountMeta.venues` of the tracked accounts, OR-ed).
    pub fn new(cfg: CondCfg, venues: u64, tsc_hz_milli: u64) -> CondTracker {
        CondTracker {
            cfg,
            venues,
            hz_milli: tsc_hz_milli,
            sec: 0,
            lags: Vec::with_capacity(4096),
            minute: 0,
            p90s: Vec::with_capacity(64),
            load: 1,
            ref_inst: None,
            ref_hist: VecDeque::with_capacity(4096),
        }
    }

    /// One market record (`kind`, logical `body`, record-header `tsc`), in record order.
    pub fn on_market(&mut self, kind: EventKind, body: &[u8], tsc: u64) {
        let Ok((h, _)) = MarketHdr::read_from_prefix(body) else {
            return;
        };
        let on_venue = self.venues & (1u64 << (h.venue.0 & 63)) != 0;
        if on_venue
            && matches!(
                kind,
                EventKind::L2_BATCH | EventKind::L2_SNAPSHOT | EventKind::TRADE
            )
        {
            if h.sw_rx_ns > 0 && h.ex_ts_ns > 0 {
                self.lag(h.sw_rx_ns, h.sw_rx_ns - h.ex_ts_ns);
            }
        } else if !on_venue && kind == EventKind::BBO {
            let Some(b) = crate::glue::body_of::<Bbo>(body) else {
                return;
            };
            if self.ref_inst.is_none() {
                self.ref_inst = Some(b.h.inst);
            }
            if self.ref_inst == Some(b.h.inst) && b.bid_px.0 > 0 && b.ask_px.0 > 0 {
                self.ref_hist
                    .push_back((tsc, i128::from(b.bid_px.0) + i128::from(b.ask_px.0)));
                let keep = self.hz_milli.saturating_mul(2) / 1_000;
                while self
                    .ref_hist
                    .front()
                    .is_some_and(|x| tsc.saturating_sub(x.0) > keep)
                {
                    self.ref_hist.pop_front();
                }
            }
        }
    }

    fn lag(&mut self, rx_ns: i64, lag_ns: i64) {
        let sec = rx_ns.div_euclid(1_000_000_000);
        if sec != self.sec {
            if !self.lags.is_empty() {
                self.lags.sort_unstable();
                let k = (self.lags.len() * 9 / 10).min(self.lags.len() - 1);
                if let Some(v) = self.lags.get(k) {
                    self.p90s.push(*v);
                }
                self.lags.clear();
            }
            let minute = sec.div_euclid(60);
            if minute != self.minute {
                if !self.p90s.is_empty() {
                    self.p90s.sort_unstable();
                    if let Some(med) = self.p90s.get(self.p90s.len() / 2) {
                        let n = self.cfg.load_thr_ns.iter().filter(|x| *med > **x).count();
                        self.load = u8::try_from(n).unwrap_or(u8::MAX);
                    }
                    self.p90s.clear();
                }
                self.minute = minute;
            }
            self.sec = sec;
        }
        self.lags.push(lag_ns);
    }

    /// The condition of a decision at record time `t` and engine UTC `utc_ns`.
    pub fn condition(&self, t: Tsc, utc_ns: i64) -> Cond {
        let one_s = self.hz_milli / 1_000;
        let mv = match (
            self.ref_hist.iter().find(|x| t.0.saturating_sub(x.0) <= one_s),
            self.ref_hist.back(),
        ) {
            (Some(a), Some(b)) if a.1 > 0 => {
                let d = (b.1 - a.1).abs();
                if d * 40_000 <= a.1 {
                    0
                } else if d * 10_000 <= a.1 {
                    1
                } else {
                    2
                }
            }
            _ => 0,
        };
        let s = utc_ns.div_euclid(1_000_000_000);
        let phase =
            u8::from(s.rem_euclid(i64::from(self.cfg.window_s.max(1))) >= i64::from(self.cfg.phase_s));
        Cond {
            load: self.load,
            mv,
            phase,
        }
    }
}
