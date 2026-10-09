//! EXE-430–EXE-432, EXE-434: detail packing, SDK enum codes, order kind codes, Note table.
use hft_event::{NOTE_NAMES, NoteLevel, NoteSource};
use hft_exec::*;
use hft_strategy_sdk::{BrokenReason, ExternalCause, StuckReason, TargetReject};
use hft_types::RejectBits;

#[test]
fn exe_430_detail_roundtrip() {
    let mut rows: Vec<(&str, u8, u8, u16, u32)> = vec![("EXE-430.1", 0, 0, 0, 0)];
    for c in 1..=10u8 {
        rows.push(("EXE-430.2", 1, c, 0, 1 | (u32::from(c) << 8)));
    }
    for c in 1..=8u8 {
        rows.push(("EXE-430.3", 2, c, 0, 2 | (u32::from(c) << 8)));
    }
    for a in 1..=6u16 {
        rows.push(("EXE-430.4", 3, 1, a, 3 | 1 << 8 | (u32::from(a) << 16)));
    }
    for c in 2..=4u8 {
        rows.push(("EXE-430.5", 3, c, 0, 3 | (u32::from(c) << 8)));
    }
    for c in 1..=3u8 {
        rows.push(("EXE-430.6", 4, c, 0, 4 | (u32::from(c) << 8)));
    }
    for a in [1u16, 2, 4, 6, 7, 8, 9] {
        rows.push(("EXE-430.7", 4, 4, a, 4 | 4 << 8 | (u32::from(a) << 16)));
    }
    for c in [1u8, 3, 4, 6] {
        rows.push(("EXE-430.8", 5, c, 0, 5 | (u32::from(c) << 8)));
    }
    for a in [1u16, 4, 5, 6] {
        rows.push(("EXE-430.9", 5, 2, a, 5 | 2 << 8 | (u32::from(a) << 16)));
    }
    rows.push(("EXE-430.10", 5, 5, 0b1011, 5 | 5 << 8 | 0b1011 << 16));
    for c in 1..=5u8 {
        for a in 1..=5u16 {
            rows.push((
                "EXE-430.11",
                6,
                c,
                a,
                6 | (u32::from(c) << 8) | (u32::from(a) << 16),
            ));
        }
    }
    for (row, t, c, a, want) in rows {
        assert_eq!(pack(t, c, a), want, "{row}");
        assert_eq!(unpack(pack(t, c, a)), (t, c, a), "EXE-430.12 ({row})");
    }
}

#[test]
fn exe_431_sdk_enum_codes() {
    let rejects = [
        TargetReject::NotOwner,
        TargetReject::BadLegs,
        TargetReject::SpecInvalid,
        TargetReject::AlgoUnsupported,
        TargetReject::SpotShort,
        TargetReject::CellBroken,
        TargetReject::ForcedReduceActive,
        TargetReject::TooManyLegs,
        TargetReject::FlattenLocked,
        TargetReject::LotMismatch,
    ];
    for (k, r) in rejects.iter().enumerate() {
        assert_eq!(r.code() as usize, k + 1, "EXE-431.1");
    }
    let stuck = [
        StuckReason::Gated(RejectBits(0)),
        StuckReason::Shadow,
        StuckReason::NoLiquidity,
        StuckReason::StaleQuote,
        StuckReason::SliceFails,
        StuckReason::External,
        StuckReason::EngineOverride,
        StuckReason::MinOrder,
    ];
    for (k, s) in stuck.iter().enumerate() {
        assert_eq!(s.code() as usize, k + 1, "EXE-431.2");
    }
    let broken = [
        BrokenReason::LinkBroken,
        BrokenReason::ReverseFailed,
        BrokenReason::FixExhausted,
        BrokenReason::HedgeVeto,
    ];
    for (k, b) in broken.iter().enumerate() {
        assert_eq!(b.code() as usize, k + 1, "EXE-431.3");
    }
    let causes = [
        ExternalCause::Liquidation,
        ExternalCause::Adl,
        ExternalCause::Manual,
        ExternalCause::Delivery,
        ExternalCause::Unknown,
    ];
    for (k, c) in causes.iter().enumerate() {
        assert_eq!(*c as usize, k + 1, "EXE-431.4");
    }
}

#[test]
fn exe_432_exec_order_kind() {
    let rows = [
        ("EXE-432.1", ExecOrderKind::Slice, 1),
        ("EXE-432.2", ExecOrderKind::PairFirst, 2),
        ("EXE-432.3", ExecOrderKind::SimulFirst, 3),
        ("EXE-432.4", ExecOrderKind::HedgeFix, 5),
        ("EXE-432.5", ExecOrderKind::ResidualTrim, 6),
        ("EXE-432.6", ExecOrderKind::ExternalDefault, 7),
        ("EXE-432.7", ExecOrderKind::ForceStep, 8),
        ("EXE-432.8", ExecOrderKind::CmdHedgeFix, 9),
        ("EXE-432.9", ExecOrderKind::AutoFix, 10),
        ("EXE-432.10", ExecOrderKind::MakerFirst, 4),
    ];
    for (row, k, c) in rows {
        assert_eq!(k.code(), c, "{row}");
        assert_eq!(ExecOrderKind::from_code(c), Some(k), "{row}");
    }
}

#[test]
fn exe_434_note_table() {
    let want: [(&str, Option<NoteLevel>); 13] = [
        ("ExecBroken", Some(NoteLevel::P0)),
        ("ExecExternalDefault", None),
        ("ExecAutoFixFailed", Some(NoteLevel::P0)),
        ("ExecSerialFallback", Some(NoteLevel::P0)),
        ("ExecEngineOrderRejected", Some(NoteLevel::P0)),
        ("ExecUngroupedReduce", Some(NoteLevel::P0)),
        ("ExecGateHeldNaked", Some(NoteLevel::P0)),
        ("ExecAutoFixDone", Some(NoteLevel::P1)),
        ("ExecStuck", Some(NoteLevel::P1)),
        ("ExecForceStep", Some(NoteLevel::P1)),
        ("ExecManualExternal", None),
        ("ExecManualHold", None),
        ("ExecChainLegExcess", Some(NoteLevel::P0)),
    ];
    for (k, (name, lvl)) in want.iter().enumerate() {
        let reg = NOTE_NAMES
            .iter()
            .find(|n| n.name == *name)
            .unwrap_or_else(|| panic!("{name}"));
        assert_eq!(
            (reg.source, reg.level),
            (NoteSource::Exec, *lvl),
            "EXE-434.{}",
            k + 1
        );
        assert!(
            NOTE_CODES
                .iter()
                .any(|(s, _, n)| *s == NoteSource::Exec && n == name),
            "EXE-434.{}",
            k + 1
        );
    }
    let mut ns: Vec<u32> = NOTE_CODES.iter().map(|x| x.1).collect();
    ns.sort_unstable();
    ns.dedup();
    assert_eq!(ns.len(), NOTE_CODES.len(), "EXE-434.14");
    let mut ours: Vec<&str> = NOTE_CODES.iter().map(|x| x.2).collect();
    let mut reg: Vec<&str> = NOTE_NAMES
        .iter()
        .filter(|n| n.source == NoteSource::Exec)
        .map(|n| n.name)
        .collect();
    ours.sort_unstable();
    reg.sort_unstable();
    assert_eq!(ours, reg, "EXE-434.14");
}
