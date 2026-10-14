//! Live funding-rate statistics. These estimates never book account cash.
use hft_strategy_sdk::FundingView;
use hft_types::Rate;

pub const FUNDING_RING: usize = 256;
pub mod funding_flags {
    pub const INTERVAL_CHANGED: u32 = 1;
    pub const STALE: u32 = 2;
    pub const GAP: u32 = 4;
    pub const NO_DATA: u32 = 8;
    pub const TRUNCATED: u32 = 16;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct SettleRec {
    pub at_ns: i64,
    pub rate: Rate,
    pub flags: u32,
    pub _pad: u32,
}
impl Default for SettleRec {
    fn default() -> Self {
        Self {
            at_ns: 0,
            rate: Rate(0),
            flags: 0,
            _pad: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct FundingSummary {
    pub predicted: Rate,
    pub predicted_daily: Rate,
    pub realized_sum: Rate,
    pub realized_daily: Rate,
    pub min_rate: Rate,
    pub max_rate: Rate,
    pub n_settles: u16,
    pub n_positive: u16,
    pub n_negative: u16,
    pub neg_streak: u16,
    pub interval_s: u32,
    pub flags: u32,
    pub next_settle_ns: i64,
    pub interval_changed_at_ns: i64,
}
impl Default for FundingSummary {
    fn default() -> Self {
        Self {
            predicted: Rate(0),
            predicted_daily: Rate(0),
            realized_sum: Rate(0),
            realized_daily: Rate(0),
            min_rate: Rate(0),
            max_rate: Rate(0),
            n_settles: 0,
            n_positive: 0,
            n_negative: 0,
            neg_streak: 0,
            interval_s: 0,
            flags: 0,
            next_settle_ns: 0,
            interval_changed_at_ns: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FundingObs {
    Normal,
    Settled(SettleRec),
    IntervalChanged { old_s: u32, new_s: u32 },
    SettledChanged { rec: SettleRec, old_s: u32, new_s: u32 },
}

#[derive(Clone, Debug)]
pub struct FundingStats {
    ring: [SettleRec; FUNDING_RING],
    head: usize,
    len: usize,
    last_rate: Rate,
    last_next_ns: i64,
    interval_s: u32,
    interval_changed_at_ns: i64,
    last_event_ns: i64,
}

fn rounded(n: i128, d: i128) -> i128 {
    if d <= 0 {
        return 0;
    }
    let q = n / d;
    let r = n % d;
    let twice = r.abs() * 2;
    q + if twice > d || (twice == d && q % 2 != 0) {
        n.signum()
    } else {
        0
    }
}
fn rate(n: i128) -> Rate {
    Rate(n.clamp(-i128::from(i64::MAX), i128::from(i64::MAX)) as i64)
}

impl Default for FundingStats {
    fn default() -> Self {
        Self::new()
    }
}
impl FundingStats {
    pub fn new() -> Self {
        Self {
            ring: [SettleRec::default(); FUNDING_RING],
            head: 0,
            len: 0,
            last_rate: Rate(0),
            last_next_ns: 0,
            interval_s: 0,
            interval_changed_at_ns: 0,
            last_event_ns: 0,
        }
    }
    /// O(1), zero allocation. One instance observes exactly one instrument.
    pub fn observe(&mut self, f: &FundingView, now_ns: i64) -> FundingObs {
        self.last_event_ns = now_ns;
        if f.next_settle_ns < self.last_next_ns {
            return FundingObs::Normal;
        }
        let jump = i128::from(f.next_settle_ns) - i128::from(self.last_next_ns);
        let settled = if self.last_next_ns != 0 && jump > 0 {
            let rec = SettleRec {
                at_ns: self.last_next_ns,
                rate: self.last_rate,
                flags: if f.interval_s != 0 && jump >= i128::from(f.interval_s) * 2_000_000_000 {
                    funding_flags::GAP
                } else {
                    0
                },
                _pad: 0,
            };
            if let Some(slot) = self.ring.get_mut(self.head) {
                *slot = rec;
            }
            self.head = (self.head + 1) % FUNDING_RING;
            self.len = (self.len + 1).min(FUNDING_RING);
            Some(rec)
        } else {
            None
        };
        let interval = if f.interval_s != 0 {
            f.interval_s
        } else if settled.is_some() {
            (rounded(jump, 60_000_000_000) * 60).clamp(0, i128::from(u32::MAX)) as u32
        } else {
            self.interval_s
        };
        let old_s = self.interval_s;
        let changed = old_s != 0 && interval != old_s;
        if changed {
            self.interval_changed_at_ns = now_ns;
        }
        self.interval_s = interval;
        self.last_rate = f.rate;
        self.last_next_ns = f.next_settle_ns;
        match (settled, changed) {
            (Some(rec), true) => FundingObs::SettledChanged {
                rec,
                old_s,
                new_s: interval,
            },
            (Some(rec), false) => FundingObs::Settled(rec),
            (None, true) => FundingObs::IntervalChanged {
                old_s,
                new_s: interval,
            },
            (None, false) => FundingObs::Normal,
        }
    }
    /// O(256), zero allocation; UTC window includes its lower boundary.
    pub fn summary(&self, window_ns: i64, now_ns: i64, stale_ns: i64) -> FundingSummary {
        let start = now_ns.saturating_sub(window_ns);
        let mut out = FundingSummary {
            predicted: self.last_rate,
            predicted_daily: Self::daily(self.last_rate, self.interval_s),
            interval_s: self.interval_s,
            next_settle_ns: self.last_next_ns,
            interval_changed_at_ns: self.interval_changed_at_ns,
            ..FundingSummary::default()
        };
        if self.interval_changed_at_ns != 0 && self.interval_changed_at_ns >= start {
            out.flags |= funding_flags::INTERVAL_CHANGED;
        }
        if self.last_event_ns == 0 && self.len == 0 {
            out.flags |= funding_flags::NO_DATA;
        } else if self.last_event_ns == 0 || now_ns.saturating_sub(self.last_event_ns) > stale_ns {
            out.flags |= funding_flags::STALE;
        }
        let mut sum = 0i128;
        for i in 0..self.len {
            let ix = (self.head + FUNDING_RING - self.len + i) % FUNDING_RING;
            let Some(rec) = self.ring.get(ix) else { continue };
            if i == 0 && self.len == FUNDING_RING && rec.at_ns > start {
                out.flags |= funding_flags::TRUNCATED;
            }
            if rec.at_ns < start {
                continue;
            }
            if out.n_settles == 0 {
                out.min_rate = rec.rate;
                out.max_rate = rec.rate;
            }
            out.n_settles += 1;
            out.n_positive += u16::from(rec.rate.0 > 0);
            out.n_negative += u16::from(rec.rate.0 < 0);
            out.neg_streak = if rec.rate.0 < 0 { out.neg_streak + 1 } else { 0 };
            out.min_rate = Rate(out.min_rate.0.min(rec.rate.0));
            out.max_rate = Rate(out.max_rate.0.max(rec.rate.0));
            out.flags |= rec.flags & funding_flags::GAP;
            sum += i128::from(rec.rate.0);
        }
        out.realized_sum = rate(sum);
        out.realized_daily = rate(rounded(sum * 86_400_000_000_000, i128::from(window_ns)));
        out
    }
    /// O(1), i128 HalfEven with symmetric saturation; unknown interval gives zero.
    pub fn daily(value: Rate, interval_s: u32) -> Rate {
        rate(rounded(i128::from(value.0) * 86_400, i128::from(interval_s)))
    }
}
