//! `ExecDecision.detail` packing and every sub-code table, `ExecDecision.action` codes, the
//! `Note{source = Exec}` code table and the Note `v[]` helpers (README "`ExecDecision.detail`
//! 打包与子码", "消费与产生的事件").

use hft_event::NoteSource;

/// `detail = table | code << 8 | arg << 16`.
#[inline]
pub const fn pack(table: u8, code: u8, arg: u16) -> u32 {
    (table as u32) | ((code as u32) << 8) | ((arg as u32) << 16)
}

/// Inverse of [`pack`]: `(table, code, arg)`.
#[inline]
pub const fn unpack(detail: u32) -> (u8, u8, u16) {
    (
        (detail & 0xFF) as u8,
        ((detail >> 8) & 0xFF) as u8,
        (detail >> 16) as u16,
    )
}

/// `detail` tables.
pub mod table {
    pub const NONE: u8 = 0;
    pub const TARGET_REJECT: u8 = 1;
    pub const STUCK: u8 = 2;
    pub const BROKEN: u8 = 3;
    pub const AUTO_FIX: u8 = 4;
    pub const ENGINE: u8 = 5;
    pub const EXTERNAL: u8 = 6;
}

/// Table 4 (`AutoFix`) codes.
pub mod autofix {
    pub const START: u8 = 1;
    pub const SLICE: u8 = 2;
    pub const DONE: u8 = 3;
    pub const FAILED: u8 = 4;
}

/// Table 5 (`Engine`) codes.
pub mod engine {
    pub const STEP: u8 = 1;
    pub const SERIAL_FALLBACK: u8 = 2;
    pub const RISK_REJECT: u8 = 3;
    pub const UNGROUPED: u8 = 4;
    pub const CANCEL_RESTING: u8 = 5;
    pub const HARD_ALIGN: u8 = 6;
}

/// `SerialFallback` reasons (table 5 `arg`; 2 and 3 reserved).
pub mod serial {
    pub const STALE_QUOTE: u16 = 1;
    pub const NOT_READY: u16 = 4;
    pub const RESUME_HOLD: u16 = 5;
    pub const FIRST_UNKNOWN: u16 = 6;
}

/// Table 6 (`External`) handling (`arg`).
pub mod external {
    pub const TIMED: u16 = 1;
    pub const MANUAL_HOLD: u16 = 2;
    pub const ENGINE_OWNED: u16 = 3;
    pub const BROKEN_KEPT: u16 = 4;
    pub const CLEARED: u16 = 5;
}

/// `ExecDecision.action` codes (hft-event `ExecAction`).
pub mod action {
    pub const TARGET_SET: u8 = 1;
    pub const TARGET_REPLACED: u8 = 2;
    pub const TARGET_CLEARED: u8 = 3;
    pub const TARGET_REJECTED: u8 = 4;
    pub const SLICE: u8 = 5;
    pub const STATE: u8 = 6;
    pub const HEDGE_FIX: u8 = 7;
    pub const RESIDUAL_TRIM: u8 = 8;
    pub const EXTERNAL_SEEN: u8 = 9;
    pub const EXTERNAL_DEFAULT: u8 = 10;
    pub const FORCE_STEP: u8 = 11;
    pub const CMD_HEDGE_FIX: u8 = 12;
    pub const CHAIN_LEG_EXCESS: u8 = 13;
    pub const AUTO_FIX: u8 = 14;
}

/// `ExecDecision.algo` codes (hft-event `ExecAlgo`).
pub mod algo {
    pub const NONE: u8 = 0;
    pub const TAKER_SLICE: u8 = 1;
    pub const PAIR_ON_FILL: u8 = 2;
    pub const SIMULTANEOUS: u8 = 3;
    pub const MAKER_FIRST: u8 = 4;
    pub const ENGINE: u8 = 5;
}

/// `ExecDecision.eflags` bits (hft-event `ExecFlags`).
pub mod eflags {
    pub const SHADOW: u8 = 1;
    pub const CLEARED_BY_ENGINE: u8 = 2;
}

/// `Note{ExecManualHold}` release reasons (`v[2]`).
pub mod hold_release {
    pub const STRATEGY: u64 = 1;
    pub const HEDGE_FIX_CMD: u64 = 2;
    pub const ENGINE_TARGET: u64 = 3;
    pub const ACCOUNT_CLEARED: u64 = 4;
}

/// Wait reasons of `Note{ExecGateHeldNaked}` (`v[2]`; smallest code of the conditions holding).
pub mod wait {
    pub const VENUE: u64 = 1;
    pub const NOT_QUIESCENT: u64 = 2;
    pub const REORG: u64 = 3;
    pub const QUOTE: u64 = 4;
    pub const RATE_BACKOFF: u64 = 5;
    pub const BEYOND_CROSS: u64 = 6;
    pub const BALANCE: u64 = 7;
}

/// Note `n` of this crate (`code = Exec << 24 | n`); names and default levels are registered in
/// hft-event `NOTE_NAMES`.
pub mod note {
    pub const EXEC_BROKEN: u32 = 1;
    pub const EXEC_EXTERNAL_DEFAULT: u32 = 2;
    pub const EXEC_AUTO_FIX_FAILED: u32 = 3;
    pub const EXEC_STUCK: u32 = 4;
    pub const EXEC_FORCE_STEP: u32 = 5;
    pub const EXEC_AUTO_FIX_DONE: u32 = 6;
    pub const EXEC_SERIAL_FALLBACK: u32 = 7;
    pub const EXEC_ENGINE_ORDER_REJECTED: u32 = 8;
    pub const EXEC_MANUAL_EXTERNAL: u32 = 9;
    pub const EXEC_MANUAL_HOLD: u32 = 10;
    pub const EXEC_UNGROUPED_REDUCE: u32 = 11;
    pub const EXEC_GATE_HELD_NAKED: u32 = 12;
    pub const EXEC_CHAIN_LEG_EXCESS: u32 = 13;
}

/// The Note const table of this crate (`gen-schema --check` compares it with hft-event `NOTE_NAMES`).
pub const NOTE_CODES: &[(NoteSource, u32, &str)] = &[
    (NoteSource::Exec, 1, "ExecBroken"),
    (NoteSource::Exec, 2, "ExecExternalDefault"),
    (NoteSource::Exec, 3, "ExecAutoFixFailed"),
    (NoteSource::Exec, 4, "ExecStuck"),
    (NoteSource::Exec, 5, "ExecForceStep"),
    (NoteSource::Exec, 6, "ExecAutoFixDone"),
    (NoteSource::Exec, 7, "ExecSerialFallback"),
    (NoteSource::Exec, 8, "ExecEngineOrderRejected"),
    (NoteSource::Exec, 9, "ExecManualExternal"),
    (NoteSource::Exec, 10, "ExecManualHold"),
    (NoteSource::Exec, 11, "ExecUngroupedReduce"),
    (NoteSource::Exec, 12, "ExecGateHeldNaked"),
    (NoteSource::Exec, 13, "ExecChainLegExcess"),
];

/// `Note.code` of `n`.
#[inline]
pub const fn note_code(n: u32) -> u32 {
    ((NoteSource::Exec as u32) << 24) | (n & 0xFF_FFFF)
}

/// `account | group << 8` (Note `v[0]` of cell-scoped notes).
#[inline]
pub const fn acct_group(account: u8, group: u32) -> u64 {
    (account as u64) | ((group as u64) << 8)
}
