//! Every effective account/UID order window is enforced before writing an order.
use hft_refdata::{LimitBucket, LimitScope, RateLimit};
use hft_venue_proto::RateObs;
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RateCounters {
    windows: Box<[Window]>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct Window {
    limit: u32,
    ms: u32,
    ticks: u64,
    cost: u32,
    start: u64,
    used: u32,
}
impl Window {
    fn roll(&mut self, now: u64) {
        if now.saturating_sub(self.start) >= self.ticks {
            self.start = now;
            self.used = 0;
        }
    }
}
impl RateCounters {
    /// Effective REST windows shared by account queries, order queries and maintenance.
    pub fn account_rest(limits: &[RateLimit], ms: impl Fn(u64) -> u64) -> Self {
        Self {
            windows: limits
                .iter()
                .filter(|l| l.scope != LimitScope::Ip && l.bucket == LimitBucket::Weight)
                .map(|l| Window {
                    limit: l.limit,
                    ms: l.window_ms,
                    ticks: ms(u64::from(l.window_ms)).max(1),
                    cost: u32::from(l.default_cost.max(1)),
                    start: 0,
                    used: 0,
                })
                .collect(),
        }
    }
    pub fn weight_available(&mut self, weight: u32, urgent: bool, now: u64) -> bool {
        for w in &mut self.windows {
            w.roll(now);
        }
        self.windows.iter().all(|w| {
            let enough = w.used.checked_add(weight).is_some_and(|n| n <= w.limit);
            enough && (urgent || u64::from(w.limit.saturating_sub(w.used)) * 5 >= u64::from(w.limit))
        })
    }
    pub fn charge_weight(&mut self, weight: u32) {
        for w in &mut self.windows {
            w.used = w.used.saturating_add(weight);
        }
    }
    /// Public REST weight limits for the configured market-data source IP.
    pub fn rest(limits: &[RateLimit], ms: impl Fn(u64) -> u64) -> Self {
        Self {
            windows: limits
                .iter()
                .filter(|l| l.scope == LimitScope::Ip && l.bucket == LimitBucket::Weight)
                .map(|l| Window {
                    limit: l.limit,
                    ms: l.window_ms,
                    ticks: ms(u64::from(l.window_ms)).max(1),
                    cost: u32::from(l.default_cost.max(1)),
                    start: 0,
                    used: 0,
                })
                .collect(),
        }
    }
    /// Allocates only during startup; subsequent operations visit the fixed window table.
    pub fn new(limits: &[RateLimit], ms: impl Fn(u64) -> u64) -> Self {
        Self {
            windows: limits
                .iter()
                .filter(|l| {
                    matches!(l.scope, LimitScope::Account | LimitScope::Uid)
                        && l.bucket == LimitBucket::Orders
                })
                .map(|l| Window {
                    limit: l.limit,
                    ms: l.window_ms,
                    ticks: ms(u64::from(l.window_ms)).max(1),
                    cost: u32::from(l.default_cost.max(1)),
                    start: 0,
                    used: 0,
                })
                .collect(),
        }
    }
    pub fn take(&mut self, now: u64) -> bool {
        for w in &mut self.windows {
            w.roll(now);
        }
        if self
            .windows
            .iter()
            .any(|w| w.used.checked_add(w.cost).is_none_or(|n| n > w.limit))
        {
            return false;
        }
        for w in &mut self.windows {
            w.used += w.cost;
        }
        true
    }
    /// A delayed observation may tighten, but never release quota already charged locally.
    pub fn observe_window(&mut self, obs: RateObs, now: u64) {
        if obs.bucket != LimitBucket::Orders {
            return;
        }
        let exact = self.windows.iter().any(|w| w.ms == obs.window_ms);
        for w in &mut self.windows {
            w.roll(now);
            if exact && w.ms != obs.window_ms {
                continue;
            }
            let used = if obs.limit == 0 || obs.limit == w.limit {
                obs.used
            } else {
                u32::try_from((u64::from(obs.used) * u64::from(w.limit)).div_ceil(u64::from(obs.limit)))
                    .unwrap_or(u32::MAX)
            };
            w.used = w.used.max(used);
        }
    }
    /// Compatibility entry for callers without the window; conservatively scales the observation.
    pub fn observe(&mut self, used: u32, limit: u32, now: u64) {
        self.observe_window(
            RateObs {
                bucket: LimitBucket::Orders,
                used,
                limit,
                window_ms: 0,
            },
            now,
        );
    }
    pub fn reset(&mut self) {
        for w in &mut self.windows {
            w.used = 0;
        }
    }
    pub fn used(&self) -> u32 {
        self.windows.first().map_or(0, |w| w.used)
    }
}
