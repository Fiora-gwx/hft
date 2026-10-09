//! Shared fixtures of the hft-strat-lib tests (TESTS.md "共用夹具").
#![allow(dead_code)]

use hft_strat_lib::{ConfirmWindow, Expiring, Throttle, TriggerPx};
use hft_strategy_sdk::{EngineClockAnchor, Now, SnapshotWriter};
use hft_types::Tsc;

/// TSC ticks per ms of the common `Now` anchor.
pub const TSC_PER_MS: u64 = 3_000_000;
/// engine_ns of TSC 0 in the common anchor (`ens_of(t) = 5e9 + t / 3`).
pub const BASE_ENS: i64 = 5_000_000_000;

/// The common anchor: tsc 0 ↔ engine_ns 5 000 000 000, 3 000 000 ticks per ms.
pub fn anchor() -> EngineClockAnchor {
    anchor_with(0, TSC_PER_MS, BASE_ENS)
}

/// Anchor with TSC origin `tsc0` (↔ `ens0`) and `tsc_per_ms`.
pub fn anchor_with(tsc0: u64, tsc_per_ms: u64, ens0: i64) -> EngineClockAnchor {
    EngineClockAnchor {
        base_engine_ns: ens0,
        base_tsc: Tsc(tsc0),
        tsc_hz_milli: tsc_per_ms * 1_000_000,
        utc_tsc: Tsc(tsc0),
        utc_ns: 1_759_708_800_000_000_000,
    }
}

/// `Now` at TSC `t` of `clk`.
pub fn now_tsc(clk: EngineClockAnchor, t: u64, seq: u64, boot_seq: u64) -> Now {
    Now::new(Tsc(t), seq, boot_seq, true, clk)
}

/// The TSC of `clk` whose engine time is exactly `ens` (the anchors here are exact).
pub fn tsc_at(clk: EngineClockAnchor, ens: i64) -> u64 {
    clk.tsc_of(ens).0
}

/// `Now{engine_ns, boot_seq}` on the common anchor.
pub fn now(ens: i64, boot_seq: u64) -> Now {
    // engine_ns before the common anchor's base: same rate, base 0 (only the value matters).
    let clk = if ens >= BASE_ENS {
        anchor()
    } else {
        anchor_with(0, TSC_PER_MS, 0)
    };
    let n = now_tsc(clk, tsc_at(clk, ens), boot_seq + 1, boot_seq);
    assert_eq!(n.engine_ns, ens);
    n
}

pub fn snap_throttle(t: &Throttle) -> Vec<u8> {
    let mut w = SnapshotWriter::new(64);
    t.snapshot(&mut w);
    w.as_bytes().to_vec()
}

pub fn snap_confirm(c: &ConfirmWindow) -> Vec<u8> {
    let mut w = SnapshotWriter::new(64);
    c.snapshot(&mut w);
    w.as_bytes().to_vec()
}

pub fn snap_trigger(t: &TriggerPx) -> Vec<u8> {
    let mut w = SnapshotWriter::new(64);
    t.snapshot(&mut w);
    w.as_bytes().to_vec()
}

pub fn snap_expiring(e: &Expiring<i64>) -> Vec<u8> {
    let mut w = SnapshotWriter::new(64);
    e.snapshot(&mut w, |v, w| w.put_i64(*v));
    w.as_bytes().to_vec()
}

fn i64_at(b: &[u8], at: usize) -> i64 {
    i64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}
fn u64_at(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}
fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(b[at..at + 2].try_into().unwrap())
}

/// Decoded `Throttle` snapshot (35 B).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ThrottleRow {
    pub cooldown_until_ens: i64,
    pub in_flight: u8,
    pub consec_fail: u8,
    pub fallback_until_ens: i64,
    pub backoff_until_ens: i64,
    pub halted: u8,
    pub hard_rearm_until_ens: i64,
}

pub fn throttle_row(t: &Throttle) -> ThrottleRow {
    let b = snap_throttle(t);
    assert_eq!(b.len(), 35);
    ThrottleRow {
        cooldown_until_ens: i64_at(&b, 0),
        in_flight: b[8],
        consec_fail: b[9],
        fallback_until_ens: i64_at(&b, 10),
        backoff_until_ens: i64_at(&b, 18),
        halted: b[26],
        hard_rearm_until_ens: i64_at(&b, 27),
    }
}

/// Decoded `ConfirmWindow` snapshot (29 B).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ConfirmRow {
    pub since_ens: i64,
    pub n_updates: u16,
    pub n_slow: u16,
    pub first_slow_seq: u64,
    pub last_slow_seq: u64,
    pub state: u8,
}

pub fn confirm_row(c: &ConfirmWindow) -> ConfirmRow {
    let b = snap_confirm(c);
    assert_eq!(b.len(), 29);
    ConfirmRow {
        since_ens: i64_at(&b, 0),
        n_updates: u16_at(&b, 8),
        n_slow: u16_at(&b, 10),
        first_slow_seq: u64_at(&b, 12),
        last_slow_seq: u64_at(&b, 20),
        state: b[28],
    }
}
