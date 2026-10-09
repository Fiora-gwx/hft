//! Descriptors (TESTS.md LPR-001–LPR-003).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::type_complexity
)] // allow-list: test code

use hft_strategy_sdk::params::{RawParams, Unit};
use hft_strategy_sdk::{Model, NoteLevel, ReportAgg, ReportScope, Strategy};
use latency_probe::{LatencyProbe, ProbeParams};

type M = <LatencyProbe as Strategy>::Model;

#[test]
fn lpr_001_desc_constants() {
    let d = LatencyProbe::DESC;
    assert_eq!(
        (d.name, d.kind_id, d.version, d.non_prod_only),
        ("latency-probe", 5, 1, true)
    );
    assert_eq!((d.schema.id, d.schema.version), (5, 1));
    assert!(d.cmds.is_empty());
    assert_eq!((d.notes.len(), d.report.len()), (9, 8));
    assert_eq!((M::LAYOUT_VERSION, M::PATH_VERSION, M::WARMUP), (0, 0, false));
    let raw = RawParams {
        defaults: ProbeParams::SCHEMA.fields.iter().map(|f| f.default).collect(),
        overrides: vec![],
    };
    let p = ProbeParams::from_raw(&raw, 0);
    assert_eq!((M::mem_bytes(&p, 4), M::snapshot_bytes(&p, 4)), (0, 0));
}

#[test]
fn lpr_002_note_table() {
    use NoteLevel::{Info, P1, P2};
    let rows: &[(&str, u32, &str, NoteLevel, usize)] = &[
        ("LPR-002.1", 1, "ProbeFilled", P2, 3),
        ("LPR-002.2", 2, "ProbeRejected", Info, 3),
        ("LPR-002.3", 3, "ProbeGated", Info, 2),
        ("LPR-002.4", 4, "PairNoFill", Info, 1),
        ("LPR-002.5", 5, "Skipped", Info, 2),
        ("LPR-002.6", 6, "FlattenIncomplete", P1, 2),
        ("LPR-002.7", 7, "ProbeHalted", P1, 2),
        ("LPR-002.8", 8, "TargetStuck", P2, 2),
        ("LPR-002.9", 9, "TargetRejected", P2, 1),
    ];
    let notes = LatencyProbe::DESC.notes;
    for &(row, code, name, level, nv) in rows {
        let n = notes
            .iter()
            .find(|n| n.code == code)
            .unwrap_or_else(|| panic!("{row}"));
        assert_eq!((n.name, n.level, n.v.len()), (name, level, nv), "{row}");
    }
    assert!(!notes.iter().any(|n| n.code == 10), "LPR-002.10");
}

#[test]
fn lpr_003_report_fields() {
    use ReportAgg::{Count, Sum};
    let rows: &[(&str, &str, ReportAgg, u32)] = &[
        ("LPR-003.1", "probe_fills", Count, 1),
        ("LPR-003.2", "rejects", Sum, 2),
        ("LPR-003.3", "gated", Sum, 3),
        ("LPR-003.4", "pair_nofill", Sum, 4),
        ("LPR-003.5", "skipped_spread", Sum, 5),
        ("LPR-003.6", "flatten_incomplete", Count, 6),
        ("LPR-003.7", "target_stuck", Count, 8),
        ("LPR-003.8", "target_rejected", Count, 9),
    ];
    for &(row, name, agg, note) in rows {
        let r = LatencyProbe::DESC
            .report
            .iter()
            .find(|r| r.name == name)
            .unwrap_or_else(|| panic!("{row}"));
        assert_eq!(r.unit, Unit::Count, "{row}");
        assert_eq!(
            (r.agg, r.scope, r.note, r.v_ix),
            (agg, ReportScope::Group, note, 0),
            "{row}"
        );
    }
}
