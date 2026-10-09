// SDK-114.11: report field v_ix = 8
#[path = "support/mod.rs"]
mod support;
use support::*;

const NOTES: &[NoteDesc] = &[
    NoteDesc { code: 1, name: "EchoFire", level: NoteLevel::Info, v: &["mid_px", "open_th"], doc: "" },
    NoteDesc { code: 2, name: "EchoOverflow", level: NoteLevel::P2, v: &[], doc: "" },
    NoteDesc { code: 3, name: "EchoNoFeed", level: NoteLevel::P2, v: &["inst"], doc: "" },
];
const REPORT: &[ReportFieldDesc] = &[
    ReportFieldDesc { name: "fires", unit: Unit::Count, agg: ReportAgg::Count, scope: ReportScope::Group, note: 1, v_ix: 8, doc: "" },
];
const CMDS: &[CmdDesc] = &[
    CmdDesc { op: 1, name: "open", reduce_only: false, fields: &[FieldDesc { name: "notional", unit: Unit::Notional, min: 0, max: 10_000_000_000, default: 0, per_group: false, structural: false, manual_only: false, relax: Relax::None, minor_change: MinorChange::None, since: 1, doc: "" }], doc: "" },
    CmdDesc { op: 2, name: "close", reduce_only: true, fields: &[], doc: "" },
    CmdDesc { op: 3, name: "set_mode", reduce_only: false, fields: &[FieldDesc { name: "m", unit: Unit::Enum(&["x", "y"]), min: 0, max: 1, default: 0, per_group: false, structural: false, manual_only: false, relax: Relax::None, minor_change: MinorChange::None, since: 1, doc: "" }], doc: "" },
];
const DESC: StrategyDesc = StrategyDesc {
    name: "echo_probe",
    kind_id: 0xFF01,
    version: 3,
    non_prod_only: false,
    schema: &S1,
    notes: NOTES,
    report: REPORT,
    cmds: CMDS,
};

const _: () = hft_strategy_sdk::check::validate_desc(&DESC);

fn main() {}
