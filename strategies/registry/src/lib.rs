//! `hft-strategy-registry`: the only list of real strategies (README). One `strategy_set!`
//! invocation generates `Strategies`; `export_schemas()` produces the canonical schema JSON files
//! and `index.json` of `hft-core schemas --out`.
#![forbid(unsafe_code)]
#![deny(
    clippy::float_arithmetic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

use core::fmt::Write as _;

use hft_strategy_sdk::{StrategyDesc, StrategySet, params};

hft_strategy_sdk::strategy_set! {
    pub struct Strategies {
        #[cfg(feature = "cross-taker")] cross_taker::CrossTaker,
        latency_probe::LatencyProbe,
    }
}

/// Test-strategy range of kind_ids (never in the committed list or the release `index.json`).
pub const TEST_KIND_IDS: core::ops::RangeInclusive<u16> = 0xFF00..=0xFFFF;

/// File name of a strategy's schema: `<kind_id>-<name>.schema.json`.
pub fn schema_file(d: &StrategyDesc) -> String {
    let mut s = String::new();
    let _ = write!(s, "{}-{}.schema.json", d.kind_id, d.name);
    s
}

/// `schema_digest` as 16 lower-case hex digits, most significant first, zero padded.
pub fn digest_hex(d: u64) -> String {
    let mut s = String::with_capacity(16);
    let _ = write!(s, "{d:016x}");
    s
}

/// JSON string literal (the names are plain ASCII; quotes and backslashes are escaped anyway).
fn json_str(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// `index.json` (format owned by README "schema 导出"): a top-level array, one object per strategy
/// in ascending kind_id, keys `kind_id`, `name`, `file`, `schema_id`, `schema_version`,
/// `schema_digest`; no whitespace.
pub fn index_json(kinds: &[StrategyDesc]) -> String {
    let mut sorted: Vec<&StrategyDesc> = kinds.iter().collect();
    sorted.sort_by_key(|d| d.kind_id);
    let mut o = String::from("[");
    for (i, d) in sorted.iter().enumerate() {
        if i > 0 {
            o.push(',');
        }
        let _ = write!(
            o,
            "{{\"kind_id\":{},\"name\":{},\"file\":{},\"schema_id\":{},\"schema_version\":{},\"schema_digest\":{}}}",
            d.kind_id,
            json_str(d.name),
            json_str(&schema_file(d)),
            d.schema.id,
            d.schema.version,
            json_str(&digest_hex(params::schema_digest(d, d.schema.version)))
        );
    }
    o.push(']');
    o
}

/// Cold path (`hft-core schemas`): `(file name, content)` of every strategy's canonical schema JSON in
/// ascending kind_id, then `("index.json", …)`.
pub fn export_schemas() -> Vec<(String, String)> {
    let mut kinds: Vec<StrategyDesc> = Strategies::kinds().to_vec();
    kinds.sort_by_key(|d| d.kind_id);
    let mut out: Vec<(String, String)> = kinds
        .iter()
        .map(|d| {
            (
                schema_file(d),
                Strategies::schema_json(d.kind_id).unwrap_or_default(),
            )
        })
        .collect();
    out.push(("index.json".to_string(), index_json(&kinds)));
    out
}

#[cfg(test)]
mod tests {
    use super::digest_hex;

    #[test]
    fn reg_007_digest_hex() {
        let rows: [(&str, u64, &str); 4] = [
            ("REG-007.1", 0, "0000000000000000"),
            ("REG-007.2", 0xAB, "00000000000000ab"),
            ("REG-007.3", 0x0123_4567_89AB_CDEF, "0123456789abcdef"),
            ("REG-007.4", u64::MAX, "ffffffffffffffff"),
        ];
        for (row, d, want) in rows {
            assert_eq!(digest_hex(d), want, "{row}");
        }
    }
}
