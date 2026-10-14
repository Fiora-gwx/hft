//! Planted defects: test-only switches that make this crate misbehave on purpose, so the DST can
//! prove its invariant checkers and scenario assertions catch the bug on the real engine (feature
//! `planted`, enabled only through hft-testkit's own `planted` feature; never part of a
//! production build; crates/hft-testkit TESTS.md TK-053, TK-179). The switch is per thread: a DST
//! world runs its engine, rebuilds and cold verifiers on one thread, like one buggy binary, and
//! parallel tests of other threads keep the correct code.
//! `hft_sequencer::planted` maps the defect names to these bits.

use std::thread::ThreadId;

/// Switch bits per thread (few threads; test builds only). A per-thread table instead of a
/// `thread_local!`: the lint exception is registered in tools/xtask/allow-list.toml (D3).
#[allow(clippy::disallowed_types)]
static ON: std::sync::Mutex<Vec<(ThreadId, u32)>> = std::sync::Mutex::new(Vec::new());

/// Replaces this thread's switch bits of the crate; returns the previous bits.
pub fn swap(bits: u32) -> u32 {
    let me = std::thread::current().id();
    let mut t = ON.lock().unwrap_or_else(|e| e.into_inner());
    let old = t.iter().find(|(id, _)| *id == me).map_or(0, |(_, b)| *b);
    t.retain(|(id, _)| *id != me);
    if bits != 0 {
        t.push((me, bits));
    }
    old
}

/// Whether planted defect `bit` is on for the calling thread.
#[inline]
pub fn on(bit: u32) -> bool {
    let me = std::thread::current().id();
    let t = ON.lock().unwrap_or_else(|e| e.into_inner());
    t.iter().any(|(id, b)| *id == me && b & bit != 0)
}

/// `risk-no-exposure-limit`: the check chain lets Open intents past the position limit (#7).
pub const NO_POSITION_LIMIT: u32 = 1 << 0;
/// `margin-ratio-only`: MarginStress tiers follow the margin ratio alone, never the distance to
/// liquidation.
pub const MARGIN_RATIO_ONLY: u32 = 1 << 1;
