//! Wire codes the sequencer writes: `CmdResult.status` / `detail` (control-plane result and detail
//! tables; the same values as hft-ctl-proto `codes`), the sequencer's Note codes and
//! `RefdataRecheckFailed` stages.

use hft_event::{NoteSource, note_code};

/// `CmdResult.status` (control-plane result codes).
pub mod status {
    pub const APPLIED: u8 = 0;
    pub const NO_OP: u8 = 1;
    pub const DUPLICATE: u8 = 2;
    pub const EXPIRED: u8 = 3;
    pub const FORBIDDEN: u8 = 11;
    pub const NEEDS_CONFIRM: u8 = 12;
    pub const CONFIRM_INVALID: u8 = 13;
    pub const INVALID_ARGS: u8 = 20;
    pub const UNKNOWN_TARGET: u8 = 21;
    pub const VERSION_CONFLICT: u8 = 22;
    pub const STALE_VERSION: u8 = 23;
    pub const CAPACITY_EXCEEDED: u8 = 24;
    pub const PRECONDITION_FAILED: u8 = 25;
    pub const ROLE_INSUFFICIENT: u8 = 26;
    pub const REJECTED: u8 = 27;
    pub const INTERNAL: u8 = 40;
    /// Every status the sequencer writes (CmdResult decoding checks membership).
    pub const ALL: &[u8] = &[
        0, 1, 2, 3, 10, 11, 12, 13, 20, 21, 22, 23, 24, 25, 26, 27, 30, 31, 40,
    ];
}

/// `CmdResult.detail` codes (`detail = code | arg << 16`).
pub mod detail {
    pub const SHADOW_DEFERRED: u16 = 1;
    pub const UNKNOWN_DISCRIMINANT: u16 = 100;
    pub const ACCOUNT_MISMATCH: u16 = 103;
    pub const ACCOUNT_BOUND: u16 = 105;
    pub const KIND_MISMATCH: u16 = 106;
    pub const SCHEMA_UNSUPPORTED: u16 = 108;
    pub const PARAM_INVALID: u16 = 110;
    pub const GROUP_ID_DUP: u16 = 111;
    pub const BAD_GROUP: u16 = 112;
    pub const SAME_EXCHANGE_LEGS: u16 = 113;
    pub const LOT_MISMATCH: u16 = 114;
    pub const UNKNOWN_GROUP: u16 = 115;
    pub const NOT_SIM: u16 = 117;
    pub const NOT_LIVE: u16 = 118;
    pub const INST_OWNED: u16 = 120;
    pub const EXEC_RISK_MISMATCH: u16 = 121;
    pub const BAD_COMBINATION: u16 = 122;
    pub const NOT_RESUMABLE: u16 = 123;
    pub const GROUP_CHECK: u16 = 124;
    pub const BAD_VALUE: u16 = 125;
    pub const DUST_BELOW_MIN_NOTIONAL: u16 = 126;
    pub const STATE_CHANGED: u16 = 200;
    pub const NO_VALID_PREVIEW: u16 = 201;
    pub const GATE_NOT_RELEASED: u16 = 300;
    pub const DRAINING: u16 = 301;
    pub const RECON_NOT_CLEAN: u16 = 302;
    pub const RESUME_LIMIT: u16 = 303;
    pub const SIM_NOT_IDLE: u16 = 304;
    pub const INSUFFICIENT_BALANCE: u16 = 305;
    pub const UNKNOWN_PRESENT: u16 = 306;
    pub const NAKED_NOT_CLEARED: u16 = 307;
    pub const GROUP_HAS_POSITION: u16 = 308;
    pub const NO_MATCHING_CAP: u16 = 309;
    pub const STALE_REPORT: u16 = 310;
    pub const NO_BALANCE_VIEW: u16 = 311;
    pub const SMOKE_ACTIVE: u16 = 313;
    pub const SMOKE_NO_PRICE: u16 = 314;
    pub const NOT_QUIESCENT: u16 = 316;
    pub const HARD: u16 = 317;
    pub const NOTHING_TO_FIX: u16 = 318;
    pub const QUOTE_UNAVAILABLE: u16 = 319;
    pub const CHAIN_LEG: u16 = 320;
    pub const BROKEN_REMAINING: u16 = 400;
    pub const REFDATA_RECHECK: u16 = 401;
    pub const SMOKE_RISK_REJECT: u16 = 403;

    pub const PARAM_KIND_STRATEGY: u16 = 1;
    pub const PARAM_KIND_MODEL_MEM_BUDGET: u16 = 2;
    pub const PARAM_KIND_NON_PROD_ONLY: u16 = 3;
    pub const PARAM_FIELD_NONE: u16 = 0xFFF;

    /// `ParamInvalid` arg.
    pub const fn param_arg(kind: u16, field: u16) -> u16 {
        (kind & 0xF) | ((field & 0xFFF) << 4)
    }
    /// `code | arg << 16`.
    pub const fn pack(code: u16, arg: u16) -> u32 {
        (code as u32) | ((arg as u32) << 16)
    }
}

/// `n` of the Sequencer-source Notes (hft-event `NOTE_NAMES`, Sequencer block, in order).
pub mod note {
    pub const EPOCH_START: u32 = 1;
    pub const CONFLATED: u32 = 2;
    pub const DRAIN_TIMEOUT: u32 = 3;
    pub const RECOVER_MISMATCH: u32 = 4;
    pub const ADVICE_MISROUTED: u32 = 5;
    pub const ADVICE_STALE: u32 = 6;
    pub const STARTUP_GATE: u32 = 7;
    pub const SHADOW_DEFERRED: u32 = 8;
    pub const IDENTITY_MISMATCH: u32 = 9;
    pub const REFDATA_RECHECK_FAILED: u32 = 10;
    pub const TIMERS_CANCELLED: u32 = 11;
    pub const TARGET_CLEARED: u32 = 12;
    pub const POSITION_DIFF: u32 = 13;
}

/// The `(source, n, name)` table of the Sequencer Notes.
pub const NOTE_CODES: &[(NoteSource, u32, &str)] = &[
    (NoteSource::Sequencer, note::EPOCH_START, "EpochStart"),
    (NoteSource::Sequencer, note::CONFLATED, "Conflated"),
    (NoteSource::Sequencer, note::DRAIN_TIMEOUT, "DrainTimeout"),
    (NoteSource::Sequencer, note::RECOVER_MISMATCH, "RecoverMismatch"),
    (NoteSource::Sequencer, note::ADVICE_MISROUTED, "AdviceMisrouted"),
    (NoteSource::Sequencer, note::ADVICE_STALE, "AdviceStale"),
    (NoteSource::Sequencer, note::STARTUP_GATE, "StartupGate"),
    (NoteSource::Sequencer, note::SHADOW_DEFERRED, "ShadowDeferred"),
    (NoteSource::Sequencer, note::IDENTITY_MISMATCH, "IdentityMismatch"),
    (
        NoteSource::Sequencer,
        note::REFDATA_RECHECK_FAILED,
        "RefdataRecheckFailed",
    ),
    (NoteSource::Sequencer, note::TIMERS_CANCELLED, "TimersCancelled"),
    (NoteSource::Sequencer, note::TARGET_CLEARED, "TargetCleared"),
    (NoteSource::Sequencer, note::POSITION_DIFF, "PositionDiff"),
];

/// Full code of a Sequencer Note.
pub const fn seq_note(n: u32) -> u32 {
    note_code(NoteSource::Sequencer, n)
}

/// Boot-source Notes the sequencer writes (Meta).
pub mod boot_note {
    /// `Note{Boot, InputQuarantined}` (5th Boot name in hft-event `NOTE_NAMES`).
    pub const INPUT_QUARANTINED: u32 = 5;
}

/// Advisor-source Notes (Meta, through the advice ring).
pub mod advisor_note {
    pub const MODEL_CHECKPOINT_DISCARDED: u32 = 1;
    pub const MODEL_PATH_RESET: u32 = 2;
    pub const ADVISOR_LAG: u32 = 3;
    pub const MODEL_GROUP_RESET: u32 = 4;
    pub const ADVISOR_STATS: u32 = 5;
    pub const MODEL_CKPT_SKIPPED: u32 = 6;
}

/// `ModelCheckpointDiscarded` reasons.
pub mod discard {
    pub const MISSING: u64 = 1;
    pub const CORRUPT: u64 = 2;
    pub const KEY_MISMATCH: u64 = 3;
    pub const SEQ_NOT_IN_JOURNAL: u64 = 4;
    pub const RESTORE_ERR: u64 = 5;
}
