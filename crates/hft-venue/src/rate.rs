//! Send-side rate counters per (venue, account) (README §4 step 1.2): the last line before the
//! exchange's hard limit. Fixed windows from refdata `RateLimitTable` (scope `Account`, bucket
//! `Orders`); `RateObs` in a response replaces the local count; `ResetRate` zeroes it.

use hft_refdata::{LimitBucket, LimitScope, RateLimit};

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct RateCounters {
    /// 0 = no order limit for this venue.
    limit: u32,
    window_ticks: u64,
    cost: u32,
    start: u64,
    used: u32,
}

impl RateCounters {
    /// From the venue's effective limits; `ms` converts milliseconds to ticks.
    pub fn new(limits: &[RateLimit], ms: impl Fn(u64) -> u64) -> RateCounters {
        let mut c = RateCounters::default();
        if let Some(l) = limits
            .iter()
            .find(|l| l.scope == LimitScope::Account && l.bucket == LimitBucket::Orders)
        {
            c.limit = l.limit;
            c.window_ticks = ms(u64::from(l.window_ms));
            c.cost = u32::from(l.default_cost.max(1));
        }
        c
    }

    fn roll(&mut self, now: u64) {
        if now.saturating_sub(self.start) >= self.window_ticks {
            self.start = now;
            self.used = 0;
        }
    }

    /// Whether one more order fits; if so it is counted.
    #[inline]
    pub fn take(&mut self, now: u64) -> bool {
        if self.limit == 0 {
            return true;
        }
        self.roll(now);
        if self.used.saturating_add(self.cost) > self.limit {
            return false;
        }
        self.used += self.cost;
        true
    }

    /// A response reported the exchange's own count.
    pub fn observe(&mut self, used: u32, now: u64) {
        if self.limit == 0 {
            return;
        }
        self.roll(now);
        self.used = used;
    }

    /// `GwCmd{ResetRate}`.
    pub fn reset(&mut self) {
        self.used = 0;
    }

    pub fn used(&self) -> u32 {
        self.used
    }
}
