//! The published TSC → UTC line for warm and gw readers (`UtcClock`) and its single writer
//! (`UtcPublisher`, housekeeping). README "时间", "warm 读者的 UTC".

use core::cell::Cell;

use crate::mailbox::SeqCell;
use crate::tsc::{CalibPair, TscClock, TscUtc};
use hft_types::Tsc;

/// A published line that is ahead of CLOCK_REALTIME is slowed by at most this much (never
/// stepped back): the kernel's `adjtime` slew limit, as the sequencer's engine UTC.
pub const UTC_SLEW_PPM: u64 = 500;
/// A measured counter rate further than this from the boot calibration is not used (the line
/// keeps the boot rate).
pub const UTC_RATE_TOL_PPM: u64 = 1_000;

/// A reader of the published line. `Clone` gives an independent reader (own fallback line and
/// floor); `Send`, not `Sync`: one reader per thread.
#[derive(Clone)]
pub struct UtcClock {
    cell: &'static SeqCell<TscUtc>,
    /// The last line this reader loaded; used while a store is in progress (never spins).
    seen: Cell<TscUtc>,
    /// The previous `utc_ns` result (`i64::MIN` before the first).
    last: Cell<i64>,
}

impl UtcClock {
    /// A line that is never republished (tests, tools, single-shot probes): leaks one cell.
    pub fn fixed(line: TscUtc) -> UtcClock {
        let cell: &'static SeqCell<TscUtc> = Box::leak(Box::new(SeqCell::new(line)));
        UtcClock::reader(cell, line)
    }

    /// Startup: the process's shared cell, starting at the boot line of `clock`, its readers'
    /// prototype and its only writer. `horizon_ms` = the period of `UtcPublisher::update`
    /// (`clock.interval_ms`): a lead is slewed out within one period where 500 ppm allows.
    pub fn shared(clock: &TscClock, horizon_ms: u32) -> (UtcClock, UtcPublisher) {
        let line = clock.tsc_utc(&clock.base);
        let cell: &'static SeqCell<TscUtc> = Box::leak(Box::new(SeqCell::new(line)));
        let p = UtcPublisher {
            cell,
            cur: line,
            hz_milli: clock.hz_milli,
            horizon_ns: u64::from(horizon_ms.max(1)) * 1_000_000,
        };
        (UtcClock::reader(cell, line), p)
    }

    fn reader(cell: &'static SeqCell<TscUtc>, line: TscUtc) -> UtcClock {
        UtcClock {
            cell,
            seen: Cell::new(line),
            last: Cell::new(i64::MIN),
        }
    }

    /// The current line: one `SeqCell::try_load`; while a store is in progress, the line this
    /// reader loaded last. Wait-free, no allocation.
    #[inline]
    pub fn line(&self) -> TscUtc {
        match self.cell.try_load() {
            Some(l) => {
                self.seen.set(l);
                l
            }
            None => self.seen.get(),
        }
    }

    /// UTC of `t` on the current line, never less than this reader's previous result (callers
    /// pass non-decreasing `t`): `max(previous, line().utc_ns(t))`.
    #[inline]
    pub fn utc_ns(&self, t: Tsc) -> i64 {
        let v = self.line().utc_ns(t).max(self.last.get());
        self.last.set(v);
        v
    }
}

impl core::fmt::Debug for UtcClock {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("UtcClock")
            .field("line", &self.seen.get())
            .finish_non_exhaustive()
    }
}

/// The single writer of the shared line (housekeeping; README "warm 读者的 UTC").
#[derive(Debug)]
pub struct UtcPublisher {
    cell: &'static SeqCell<TscUtc>,
    cur: TscUtc,
    hz_milli: u64,
    horizon_ns: u64,
}

impl UtcPublisher {
    /// The line last published (the boot line before the first `update`).
    pub fn current(&self) -> TscUtc {
        self.cur
    }

    /// One calibration tick: `pair` (CLOCK_REALTIME at `pair.tsc`) and the measured counter rate
    /// (`DriftTracker::hz_milli`; `None` or more than `UTC_RATE_TOL_PPM` from the boot rate →
    /// the boot rate). With `c` = the published line at `pair.tsc`:
    /// - `pair.realtime_ns ≥ c`: the target line (base = the pair, the rate above) — a forward
    ///   step of `realtime_ns − c`;
    /// - otherwise (ahead by `lead`): base `(pair.tsc, c)` with the target rate slowed by
    ///   `min(lead / horizon, UTC_SLEW_PPM)`, which meets the target after one horizon where the
    ///   cap allows; `err_ns` grows by `lead`.
    ///
    /// The published value at `pair.tsc` never decreases. Returns the published line.
    pub fn update(&mut self, pair: &CalibPair, hz_milli: Option<u64>) -> TscUtc {
        let tol = u128::from(self.hz_milli) * u128::from(UTC_RATE_TOL_PPM) / 1_000_000;
        let hz = match hz_milli {
            Some(h) if h != 0 && u128::from(h.abs_diff(self.hz_milli)) <= tol => h,
            _ => self.hz_milli,
        };
        let target = TscClock::from_hz_milli(hz, *pair).tsc_utc(pair);
        let c = self.cur.utc_ns(pair.tsc);
        let next = if pair.realtime_ns >= c {
            target
        } else {
            let lead = (i128::from(c) - i128::from(pair.realtime_ns)) as u128;
            let m = u128::from(target.mult);
            let cut = (m * lead / u128::from(self.horizon_ns)).min(m * u128::from(UTC_SLEW_PPM) / 1_000_000);
            TscUtc {
                base_tsc: pair.tsc,
                base_utc_ns: c,
                mult: target.mult - cut as u64,
                shift: target.shift,
                err_ns: u32::try_from(u128::from(pair.err_ns) + lead).unwrap_or(u32::MAX),
            }
        };
        self.cell.store(next);
        self.cur = next;
        next
    }
}
