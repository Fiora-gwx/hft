//! The two code tables (README "结果未知码表"; hft-venue-proto contract 8): codes registered as
//! definite rejections; everything else — the outcome-unknown table and every unregistered code —
//! is "outcome unknown". Source: the official UTA REST error-code page (README "延伸阅读").

/// Class of a response code.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum CodeClass {
    /// Registered definite rejection: the request was not executed.
    Reject,
    /// Outcome unknown (registered or unregistered).
    Unknown,
}

/// Definite rejections: parameter, balance/margin, price-band, reduce-only and symbol-state
/// errors, refused before matching. Not here on purpose: `25212` (duplicate `clientOid`: the
/// first order may be live), timeouts and system errors.
pub const DEFINITE_REJECT: &[i64] = &[
    25004, // operation too frequent (rate limited, not processed)
    25100, // trading pair does not exist
    25101, // trading pair not open for trading
    25102, // trading pair under maintenance
    25104, // contract delisted
    25105, // contract does not support opening positions
    25200, // validation error
    25202, // insufficient balance
    25203, // insufficient margin
    25205, // price below the band
    25206, // price above the band
    25207, // quantity below minimum
    25208, // quantity above maximum
    25211, // maximum open orders reached
    25230, // quantity exceeds maximum open quantity
    25231, // close quantity exceeds position
    25232, // reduce-only conflicts with existing orders
    25244, // price not a multiple of the tick
    25574, // reduce-only order protection
    25610, // amount precision
    25611, // below minimum order amount
    40017, // parameter verification failed
    40762, // order size greater than max open size (sample on the place-order page)
];

/// Outcome-unknown codes named for the record (unregistered codes are unknown anyway).
pub const OUTCOME_UNKNOWN: &[i64] = &[
    25000, // system error
    25001, // operation timed out
    25003, // concurrent operation
    40010, // request timed out
    40725, // service returned an error
    45001, // unknown error
];

/// Order does not exist (cancel / amend → `GONE`, query → `NotFound`).
pub(crate) const ORDER_GONE: i64 = 25204;
/// Request timestamp expired (→ `ClockSkew`).
pub(crate) const CLOCK_SKEW: i64 = 40008;
/// Rate limited (→ `RateLimited`).
pub(crate) const RATE_LIMITED: i64 = 25004;

/// Class of a business code, or of an HTTP status when `code` is `None`.
pub fn classify_code(code: Option<i64>, status: u16) -> CodeClass {
    if matches!(status, 403 | 408 | 418 | 429) || status >= 500 {
        return CodeClass::Unknown;
    }
    match code {
        Some(c) if DEFINITE_REJECT.contains(&c) => CodeClass::Reject,
        Some(_) => CodeClass::Unknown,
        None => CodeClass::Unknown,
    }
}
