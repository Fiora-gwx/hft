//! VEN-005: the `ConnState.reason` code table and its gen-schema export.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)]

use hft_event::ConnStateKind;
use hft_venue::codes::{CONN_REASONS, conn_reason};

#[test]
fn ven_005_conn_reason_codes() {
    use ConnStateKind::{Degraded as D, Resync as R};
    let want: [(u32, &str, ConnStateKind); 18] = [
        (1, "GAP", R),
        (2, "DECODE", R),
        (3, "SNAPSHOT_FAIL", D),
        (4, "SUB_REJECTED", D),
        (5, "RACE_SINGLE", D),
        (6, "BBO_REFRESH", R),
        (7, "BBO_SILENT", D),
        (8, "DEADMAN", D),
        (9, "CLOCK_SKEW", D),
        (10, "INFLIGHT_TIMEOUT", D),
        (11, "RATE_LIMIT", D),
        (12, "REPORT_DECODE", D),
        (13, "MIRROR", D),
        (14, "ACCT_QUERY", D),
        (15, "CONN_LOST", R),
        (16, "NEW_SUB", R),
        (17, "REQUESTED", R),
        (18, "RACE_SLOW", D),
    ];
    for (row, r) in [
        ("VEN-005.1", 0..4),
        ("VEN-005.2", 4..8),
        ("VEN-005.3", 8..12),
        ("VEN-005.4", 12..18),
    ] {
        for &(c, n, s) in &want[r] {
            assert_eq!(conn_reason(c), Some((n, s)), "{row}: code {c}");
        }
    }
    // .5: 0 and 19 are not codes; the export has exactly the 18 rows.
    assert_eq!(conn_reason(0), None, "VEN-005.5");
    assert_eq!(conn_reason(19), None, "VEN-005.5");
    assert_eq!(CONN_REASONS.len(), 18);
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/conn-reason.json");
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let rows = v["reasons"].as_array().unwrap();
    assert_eq!(rows.len(), 18, "VEN-005.5");
    for (r, &(c, n, s)) in rows.iter().zip(want.iter()) {
        assert_eq!(r["code"].as_u64(), Some(u64::from(c)));
        assert_eq!(r["name"].as_str(), Some(n));
        assert_eq!(r["state"].as_str(), Some(format!("{s:?}").as_str()));
    }
}
