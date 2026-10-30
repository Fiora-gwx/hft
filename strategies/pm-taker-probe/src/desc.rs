//! Descriptors of `pm-taker-probe` (README "Note 码表与报表字段"、"策略命令"): Note codes, report
//! fields and strategy commands. Codes are append-only once released.

use hft_strategy_sdk::{CmdDesc, NoteDesc, NoteLevel, ReportFieldDesc};

/// The spend buy was placed.
pub const BUY_SENT: u32 = 1;
/// The spend buy ended (or was not placed).
pub const BUY_END: u32 = 2;
/// The sell-back was placed.
pub const SELL_SENT: u32 = 3;
/// The sell-back ended (or was not placed).
pub const SELL_END: u32 = 4;
/// A step was skipped.
pub const SKIPPED: u32 = 5;

/// `v[0]` of `BuyEnd` / `SellEnd`: how the order ended.
pub mod end {
    /// `Filled{done}`.
    pub const FILLED: u64 = 1;
    /// `Cancelled` (the rest killed).
    pub const CANCELLED: u64 = 2;
    /// `Rejected` by the venue (or a local send failure).
    pub const REJECTED: u64 = 3;
    /// Only recorded (effective mode Shadow).
    pub const SHADOWED: u64 = 4;
    /// Rejected by an engine gate (`RejectBits` in `v[2]`).
    pub const GATED: u64 = 5;
}

/// `v[0]` of `Skipped`: which step and why.
pub mod skip {
    /// The buy quantity rounds to zero.
    pub const BUY_QTY_ZERO: u64 = 1;
    /// No bid for the sell-back.
    pub const NO_BID: u64 = 2;
    /// Nothing to sell back (position rounds to zero).
    pub const NO_POSITION: u64 = 3;
}

const fn note(code: u32, name: &'static str, v: &'static [&'static str], doc: &'static str) -> NoteDesc {
    NoteDesc {
        code,
        name,
        level: NoteLevel::Info,
        v,
        doc,
    }
}

/// Note codes written through `Ctx::note`.
pub static NOTES: [NoteDesc; 5] = [
    note(
        BUY_SENT,
        "BuySent",
        &["px", "qty", "spend", "round"],
        "spend buy (Ioc) at the best ask: qty = spend / px rounded down to the lot",
    ),
    note(
        BUY_END,
        "BuyEnd",
        &[
            "end (1 filled, 2 cancelled, 3 rejected, 4 shadowed, 5 gated)",
            "cum",
            "venue code or RejectBits",
            "round",
        ],
        "the spend buy ended",
    ),
    note(
        SELL_SENT,
        "SellSent",
        &["px", "qty", "position", "round"],
        "sell-back (Ioc) of the position at the best bid",
    ),
    note(
        SELL_END,
        "SellEnd",
        &[
            "end (1 filled, 2 cancelled, 3 rejected, 4 shadowed, 5 gated)",
            "cum",
            "venue code or RejectBits",
            "round",
        ],
        "the sell-back ended; the round is over",
    ),
    note(
        SKIPPED,
        "Skipped",
        &[
            "reason (1 buy qty 0, 2 no bid, 3 no position)",
            "value",
            "ctx full",
            "round",
        ],
        "a step was not sent; the round is over",
    ),
];
/// Strategy-specific daily-report fields (none).
pub static REPORT: [ReportFieldDesc; 0] = [];
/// Accepted `StrategyCmd` payloads (none: the round is armed through `enabled`).
pub static CMDS: [CmdDesc; 0] = [];
