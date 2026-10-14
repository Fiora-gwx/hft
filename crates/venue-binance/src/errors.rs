//! The two code tables (README "结果未知码表"; hft-venue-proto contract 8): codes registered as
//! definite rejections; everything else — the outcome-unknown table and every unregistered code —
//! is "outcome unknown".

/// HTTP status bit of a raw code without a business code (contract 8).
pub(crate) const HTTP_BIT: u32 = 0x2000_0000;

/// Class of a response code.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum CodeClass {
    /// Registered definite rejection: the request was not executed.
    Reject,
    /// Outcome unknown (registered or unregistered).
    Unknown,
}

/// Definite rejections (business codes).
const REJECT: &[i64] = &[
    -1013, // filter failure
    -1021, // timestamp outside recvWindow
    -1111, // precision
    -2010, // spot new order rejected (incl. insufficient balance)
    -2019, // margin insufficient
    -2022, // reduceOnly rejected
    -4164, // below minimum notional
    -5022, // GTX would take
    -4061, // position side mismatch
    -1003, // rate limited (request not processed)
    -1015, // order rate limited
];
// Outcome-unknown table: -1007 (timeout), -1001 (internal error / disconnected), -1008 (overload),
// status >= 500 — and every code not in `REJECT`. `-2011` on a cancel and `-2013` on a query are
// decoded by operation (README "结果未知码表"), not through this table.

/// Class of a business code, or of an HTTP status when `code` is `None`.
pub fn classify_code(code: Option<i64>, status: u16) -> CodeClass {
    if matches!(status, 403 | 408 | 418 | 429) || status >= 500 {
        return CodeClass::Unknown;
    }
    match code {
        Some(c) if REJECT.contains(&c) => CodeClass::Reject,
        Some(_) => CodeClass::Unknown,
        None => CodeClass::Unknown,
    }
}

/// Raw code of contract 8: the business code as i32 two's complement, else `HTTP_BIT | status`.
pub(crate) fn raw_code(code: Option<i64>, status: u16) -> u32 {
    match code {
        Some(c) => match i32::try_from(c) {
            Ok(v) => v as u32,
            // A numeric code outside i32 is treated as non-numeric text (contract 8); Binance codes
            // are small negatives, so this only guards malformed answers.
            Err(_) => 0x4000_0000,
        },
        None => HTTP_BIT | u32::from(status),
    }
}

/// `-1021` → clock skew.
pub(crate) const CLOCK_SKEW: i64 = -1021;
/// Request-weight limit (`-1003`) and order-rate limit (`-1015`).
pub(crate) const WEIGHT_LIMIT: i64 = -1003;
pub(crate) const ORDER_LIMIT: i64 = -1015;
/// Cancel of an unknown order (already filled or gone).
pub(crate) const CANCEL_GONE: i64 = -2011;
/// Query: order does not exist.
pub(crate) const QUERY_NOT_FOUND: i64 = -2013;
