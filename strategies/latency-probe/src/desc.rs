//! Note codes and daily-report fields (README "Note 码表与报表字段").

use hft_strategy_sdk::params::Unit;
use hft_strategy_sdk::{NoteDesc, NoteLevel, ReportAgg, ReportFieldDesc, ReportScope};

pub const PROBE_FILLED: u32 = 1;
pub const PROBE_REJECTED: u32 = 2;
pub const PROBE_GATED: u32 = 3;
pub const PAIR_NOFILL: u32 = 4;
pub const SKIPPED: u32 = 5;
pub const FLATTEN_INCOMPLETE: u32 = 6;
pub const PROBE_HALTED: u32 = 7;
pub const TARGET_STUCK: u32 = 8;
pub const TARGET_REJECTED: u32 = 9;

const fn note(
    code: u32,
    name: &'static str,
    level: NoteLevel,
    v: &'static [&'static str],
    doc: &'static str,
) -> NoteDesc {
    NoteDesc {
        code,
        name,
        level,
        v,
        doc,
    }
}

pub static NOTES: [NoteDesc; 9] = [
    note(
        PROBE_FILLED,
        "ProbeFilled",
        NoteLevel::P2,
        &["leg_ix", "fill qty", "fill px"],
        "a quote probe filled unexpectedly",
    ),
    note(
        PROBE_REJECTED,
        "ProbeRejected",
        NoteLevel::Info,
        &["count", "last RejectBits", "last venue_code"],
        "exchange rejects (not gates), counted per 10 min",
    ),
    note(
        PROBE_GATED,
        "ProbeGated",
        NoteLevel::Info,
        &["count", "last RejectBits"],
        "gate rejects, counted per 10 min",
    ),
    note(
        PAIR_NOFILL,
        "PairNoFill",
        NoteLevel::Info,
        &["count"],
        "pair first-leg IOC without fill, counted per 10 min",
    ),
    note(
        SKIPPED,
        "Skipped",
        NoteLevel::Info,
        &["spread too wide", "quote unhealthy"],
        "due but not probed, counted per 10 min",
    ),
    note(
        FLATTEN_INCOMPLETE,
        "FlattenIncomplete",
        NoteLevel::P1,
        &["residual notional", "leg_ix"],
        "pair/target still not flat after two flatten attempts",
    ),
    note(
        PROBE_HALTED,
        "ProbeHalted",
        NoteLevel::P1,
        &["cause (1 fails, 2 residual)", "count or notional"],
        "request_halt",
    ),
    note(
        TARGET_STUCK,
        "TargetStuck",
        NoteLevel::P2,
        &["StuckReason code", "1 open / 2 close target"],
        "Exec Stuck",
    ),
    note(
        TARGET_REJECTED,
        "TargetRejected",
        NoteLevel::P2,
        &["TargetReject code"],
        "the target was rejected",
    ),
];

const fn field(name: &'static str, agg: ReportAgg, note: u32, doc: &'static str) -> ReportFieldDesc {
    ReportFieldDesc {
        name,
        unit: Unit::Count,
        agg,
        scope: ReportScope::Group,
        note,
        v_ix: 0,
        doc,
    }
}

pub static REPORT: [ReportFieldDesc; 8] = [
    field(
        "probe_fills",
        ReportAgg::Count,
        PROBE_FILLED,
        "unexpected probe fills",
    ),
    field("rejects", ReportAgg::Sum, PROBE_REJECTED, "exchange rejects"),
    field("gated", ReportAgg::Sum, PROBE_GATED, "gate rejects"),
    field(
        "pair_nofill",
        ReportAgg::Sum,
        PAIR_NOFILL,
        "pair first legs without fill",
    ),
    field(
        "skipped_spread",
        ReportAgg::Sum,
        SKIPPED,
        "due probes skipped for a wide spread",
    ),
    field(
        "flatten_incomplete",
        ReportAgg::Count,
        FLATTEN_INCOMPLETE,
        "incomplete flattens",
    ),
    field("target_stuck", ReportAgg::Count, TARGET_STUCK, "stuck targets"),
    field(
        "target_rejected",
        ReportAgg::Count,
        TARGET_REJECTED,
        "rejected targets",
    ),
];
