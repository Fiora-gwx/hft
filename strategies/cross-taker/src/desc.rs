//! Registered observability of decisions and hedge failures.
use hft_strategy_sdk::prelude::*;

pub const FIRED: u32 = 1;
pub const HEDGE_FAILED: u32 = 2;
pub const HALTED: u32 = 3;
pub const NOTES: [NoteDesc; 3] = [
    NoteDesc {
        code: FIRED,
        name: "Fired",
        level: NoteLevel::Info,
        v: &["buy leg index", "quantity", "net edge (Rate)", "reduce both legs"],
        doc: "One first-leg IOC and its on-fill hedge plan accepted by Ctx.",
    },
    NoteDesc {
        code: HEDGE_FAILED,
        name: "HedgeFailed",
        level: NoteLevel::P2,
        v: &["consecutive failures"],
        doc: "Hedge rejection, empty expiry or a new Broken episode.",
    },
    NoteDesc {
        code: HALTED,
        name: "Halted",
        level: NoteLevel::P1,
        v: &["consecutive failures"],
        doc: "Group halt requested at the configured hedge failure limit.",
    },
];
pub const REPORT: [ReportFieldDesc; 2] = [
    ReportFieldDesc {
        name: "fires",
        unit: Unit::Count,
        agg: ReportAgg::Count,
        scope: ReportScope::Group,
        note: FIRED,
        v_ix: 0,
        doc: "Linked pair attempts.",
    },
    ReportFieldDesc {
        name: "hedge_failures",
        unit: Unit::Count,
        agg: ReportAgg::Count,
        scope: ReportScope::Group,
        note: HEDGE_FAILED,
        v_ix: 0,
        doc: "Observed hedge failures.",
    },
];
