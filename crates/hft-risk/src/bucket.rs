//! Token buckets per (venue, account) (10 s and 1 min windows from `RateLimitTable`, minus
//! `rate_headroom`) and per (account, inst) (1 s, `inst_bucket_per_s`), the risk-reducing
//! reserve and the exponential backoff (README "令牌桶"). Tokens are kept in nano-tokens
//! (1 token = 1e9); every time is `engine_ns`.

use hft_types::{AccountId, InstId, Rate, VenueId};

/// One nano-token scale.
pub(crate) const TOKEN: i64 = 1_000_000_000;
const NS_PER_MS: i64 = 1_000_000;

/// State of one (venue, account) pair of window buckets.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub(crate) struct VenueBucket {
    pub tokens_10s: i64,
    pub tokens_min: i64,
    pub last_ens: i64,
    pub backoff_until_ens: i64,
    pub backoff_k: u32,
}

/// State of one (account, inst) 1 s bucket.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub(crate) struct InstBucket {
    pub tokens: i64,
    pub last_ens: i64,
}

/// Capacity (nano-tokens) and window (ns) of one bucket; `cap = None` = unlimited.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub(crate) struct Window {
    pub cap: Option<i64>,
    pub window_ns: i64,
}

impl Window {
    /// `limit × (1 − headroom)` tokens over `window_ms`.
    pub fn of(limit: Option<u32>, window_ms: i64, headroom: Rate) -> Window {
        Window {
            cap: limit.map(|l| {
                let c =
                    i128::from(l) * (i128::from(TOKEN) - i128::from(headroom.0).clamp(0, i128::from(TOKEN)));
                i64::try_from(c).unwrap_or(i64::MAX)
            }),
            window_ns: window_ms.saturating_mul(NS_PER_MS),
        }
    }

    /// Tokens at `now` after lazy refill from `(tokens, last)`; unlimited → `i64::MAX`.
    #[inline]
    pub fn at(&self, tokens: i64, last: i64, now: i64) -> i64 {
        let Some(cap) = self.cap else { return i64::MAX };
        if self.window_ns <= 0 {
            return cap;
        }
        let dt = i128::from(now).saturating_sub(i128::from(last)).max(0);
        let add = dt.saturating_mul(i128::from(cap)) / i128::from(self.window_ns);
        let t = (i128::from(tokens) + add).min(i128::from(cap));
        i64::try_from(t).unwrap_or(cap)
    }

    /// Reserve (nano-tokens) for non risk-reducing orders: `cap × rr_reserve`.
    #[inline]
    pub fn reserve(&self, rr_reserve: Rate) -> i64 {
        match self.cap {
            None => 0,
            Some(c) => {
                let r = i128::from(c) * i128::from(rr_reserve.0.max(0)) / i128::from(TOKEN);
                i64::try_from(r).unwrap_or(i64::MAX)
            }
        }
    }

    pub fn full(&self) -> i64 {
        self.cap.unwrap_or(0)
    }

    /// Earliest engine time with `need` nano-tokens; rounds up, never retries early.
    pub fn ready_at(&self, tokens: i64, last: i64, now: i64, need: i64) -> i64 {
        let Some(cap) = self.cap else { return now };
        if need > cap || cap <= 0 {
            return i64::MAX;
        }
        if self.at(tokens, last, now) >= need {
            return now;
        }
        let deficit = (i128::from(need) - i128::from(tokens)).max(0);
        let ns = (deficit * i128::from(self.window_ns.max(0)) + i128::from(cap) - 1) / i128::from(cap);
        i128::from(last)
            .saturating_add(ns)
            .clamp(i128::from(now), i128::from(i64::MAX)) as i64
    }
}

/// Whether `tokens` admits one order: risk-reducing ≥ 1 token; otherwise ≥ 1 token above the reserve.
#[inline]
pub(crate) fn admits(tokens: i64, reserve: i64, risk_reducing: bool) -> bool {
    if risk_reducing {
        tokens >= TOKEN
    } else {
        i128::from(tokens) - i128::from(reserve) >= i128::from(TOKEN)
    }
}

/// Backoff of the k-th consecutive hit: `min(base × 2^k, max)` ms.
pub(crate) fn backoff_ms(base_ms: u32, max_ms: u32, k: u32) -> u32 {
    let d = u64::from(base_ms).checked_shl(k.min(40)).unwrap_or(u64::MAX);
    u32::try_from(d.min(u64::from(max_ms))).unwrap_or(max_ms)
}

/// Dense bucket storage.
#[derive(Clone, Debug)]
pub(crate) struct Buckets {
    /// account × 64 + venue.
    pub venue: Vec<VenueBucket>,
    /// account × (n_insts + 1) + inst.
    pub inst: Vec<InstBucket>,
    pub n_insts: usize,
    /// venue → (10 s, 1 min) limits from refdata.
    pub limits: Vec<(Option<u32>, Option<u32>)>,
}

impl Buckets {
    pub fn vix(a: AccountId, v: VenueId) -> usize {
        usize::from(a.0) * 64 + usize::from(v.0 & 63)
    }
    pub fn iix(&self, a: AccountId, i: InstId) -> usize {
        usize::from(a.0)
            .saturating_mul(self.n_insts + 1)
            .saturating_add(i.0 as usize)
    }
    pub fn windows(&self, v: VenueId, headroom: Rate) -> (Window, Window) {
        let (l10, l60) = self
            .limits
            .get(usize::from(v.0 & 63))
            .copied()
            .unwrap_or((None, None));
        (
            Window::of(l10, 10_000, headroom),
            Window::of(l60, 60_000, headroom),
        )
    }
    pub fn inst_window(per_s: u32) -> Window {
        Window {
            cap: Some(i64::from(per_s).saturating_mul(TOKEN)),
            window_ns: 1_000_000_000,
        }
    }
}
