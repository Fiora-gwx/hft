//! `ConnState.reason` code table (README "产生与消费的事件"; this crate is its single owner,
//! append-only). `Down`/`Rotated` carry hft-net `ErrBits`, `Up`/`Alive` 0, `Resync`/`Degraded` a code
//! below. `cargo xtask gen-schema` reads `CONN_REASONS` (text scan) into `schemas/conn-reason.json`
//! and the event catalog.

use hft_event::ConnStateKind;

pub const GAP: u32 = 1;
pub const DECODE: u32 = 2;
pub const SNAPSHOT_FAIL: u32 = 3;
pub const SUB_REJECTED: u32 = 4;
pub const RACE_SINGLE: u32 = 5;
pub const BBO_REFRESH: u32 = 6;
pub const BBO_SILENT: u32 = 7;
pub const DEADMAN: u32 = 8;
pub const CLOCK_SKEW: u32 = 9;
pub const INFLIGHT_TIMEOUT: u32 = 10;
pub const RATE_LIMIT: u32 = 11;
pub const REPORT_DECODE: u32 = 12;
pub const MIRROR: u32 = 13;
pub const ACCT_QUERY: u32 = 14;
pub const CONN_LOST: u32 = 15;
pub const NEW_SUB: u32 = 16;
pub const REQUESTED: u32 = 17;

/// `(code, name, state)` rows, ascending; never reuse a code.
pub const CONN_REASONS: &[(u32, &str, ConnStateKind)] = &[
    (1, "GAP", ConnStateKind::Resync),
    (2, "DECODE", ConnStateKind::Resync),
    (3, "SNAPSHOT_FAIL", ConnStateKind::Degraded),
    (4, "SUB_REJECTED", ConnStateKind::Degraded),
    (5, "RACE_SINGLE", ConnStateKind::Degraded),
    (6, "BBO_REFRESH", ConnStateKind::Resync),
    (7, "BBO_SILENT", ConnStateKind::Degraded),
    (8, "DEADMAN", ConnStateKind::Degraded),
    (9, "CLOCK_SKEW", ConnStateKind::Degraded),
    (10, "INFLIGHT_TIMEOUT", ConnStateKind::Degraded),
    (11, "RATE_LIMIT", ConnStateKind::Degraded),
    (12, "REPORT_DECODE", ConnStateKind::Degraded),
    (13, "MIRROR", ConnStateKind::Degraded),
    (14, "ACCT_QUERY", ConnStateKind::Degraded),
    (15, "CONN_LOST", ConnStateKind::Resync),
    (16, "NEW_SUB", ConnStateKind::Resync),
    (17, "REQUESTED", ConnStateKind::Resync),
];

/// Name and state of `code`; 0 and unknown codes → `None`.
pub fn conn_reason(code: u32) -> Option<(&'static str, ConnStateKind)> {
    CONN_REASONS
        .iter()
        .find(|(c, _, _)| *c == code)
        .map(|&(_, n, s)| (n, s))
}
