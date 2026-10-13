//! Planted defects: test-only switches that make this crate misbehave on purpose, so the DST can
//! prove its invariant checkers and scenario assertions catch the bug on the real engine (feature
//! `planted`, enabled only through hft-testkit's own `planted` feature; never part of a
//! production build; crates/hft-testkit TESTS.md TK-053, TK-179). The switch is per thread: a DST
//! world runs its engine, rebuilds and cold verifiers on one thread, like one buggy binary, and
//! parallel tests of other threads keep the correct code.
//! [`DEFECTS`] maps every defect name to its crate and bit; [`set`] switches a set of names on.

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

/// `hedge-px-book-age`: hedge pricing judges the quote by the age of its last change instead of
/// the connection's liveness evidence.
pub const HEDGE_PX_BOOK_AGE: u32 = 1 << 0;

/// The crate a defect lives in.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Crate {
    Sequencer,
    Oms,
    Risk,
    Exec,
    Ledger,
}

/// Every planted defect of the engine crates: name (as `DstConfig::planted` spells it), crate,
/// bit.
pub const DEFECTS: &[(&str, Crate, u32)] = &[
    (
        "risk-no-exposure-limit",
        Crate::Risk,
        hft_risk::planted::NO_POSITION_LIMIT,
    ),
    (
        "margin-ratio-only",
        Crate::Risk,
        hft_risk::planted::MARGIN_RATIO_ONLY,
    ),
    ("snapshot-field", Crate::Exec, hft_exec::planted::SNAPSHOT_FIELD),
    (
        "broken-never-autofix",
        Crate::Exec,
        hft_exec::planted::BROKEN_NEVER_AUTOFIX,
    ),
    (
        "broken-repeat-autofix",
        Crate::Exec,
        hft_exec::planted::BROKEN_REPEAT_AUTOFIX,
    ),
    ("ignore-fee", Crate::Ledger, hft_ledger::planted::IGNORE_FEE),
    (
        "retry-reduce-only",
        Crate::Oms,
        hft_oms::planted::RETRY_REDUCE_ONLY,
    ),
    ("hedge-px-book-age", Crate::Sequencer, HEDGE_PX_BOOK_AGE),
];

/// The switch bits of every engine crate on this thread (sequencer, OMS, risk, exec, ledger).
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct Switches(pub [u32; 5]);

fn swap_all(s: Switches) -> Switches {
    Switches([
        swap(s.0[0]),
        hft_oms::planted::swap(s.0[1]),
        hft_risk::planted::swap(s.0[2]),
        hft_exec::planted::swap(s.0[3]),
        hft_ledger::planted::swap(s.0[4]),
    ])
}

/// Switches exactly the named defects on for this thread; returns the previous switches (hand
/// them to [`restore`]). An unknown name is returned as the error and changes nothing.
pub fn set<'a>(names: &[&'a str]) -> Result<Switches, &'a str> {
    let mut s = Switches::default();
    for n in names {
        let Some((_, c, bit)) = DEFECTS.iter().find(|(d, _, _)| d == n) else {
            return Err(n);
        };
        let ix = match c {
            Crate::Sequencer => 0,
            Crate::Oms => 1,
            Crate::Risk => 2,
            Crate::Exec => 3,
            Crate::Ledger => 4,
        };
        s.0[ix] |= bit;
    }
    Ok(swap_all(s))
}

/// Puts back the switches [`set`] returned.
pub fn restore(prev: Switches) {
    swap_all(prev);
}
