//! Note name registry, `note_code`, `VField` and CapRecord in `Note.v` (EVT-075 – EVT-078, EVT-091).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::common;

use common::*;
use hft_event::*;
use hft_types::*;
use std::collections::HashSet;
use zerocopy::{FromBytes, IntoBytes};

type Row = (
    &'static str,
    &'static str,
    NoteSource,
    Option<NoteLevel>,
    &'static [(u16, u16)],
);

#[test]
fn evt_075_note_registry_rows() {
    use NoteLevel::*;
    use NoteSource::*;
    let s64 = |n: u16| -> &'static [(u16, u16)] {
        match n {
            2 => &[(0, 64), (64, 64)],
            3 => &[(0, 64), (64, 64), (128, 64)],
            4 => &[(0, 64), (64, 64), (128, 64), (192, 64)],
            _ => &[(0, 64), (64, 64), (128, 64), (192, 64), (256, 64)],
        }
    };
    let va: &[(u16, u16)] = &[(0, 8), (8, 8)];
    // README table order; EVT-075.85 – .88 sit at the end of their source groups.
    let rows: Vec<Row> = vec![
        ("EVT-075.1", "EpochStart", Sequencer, Some(Info), s64(3)),
        ("EVT-075.2", "Conflated", Sequencer, Some(Info), &[]),
        ("EVT-075.3", "DrainTimeout", Sequencer, None, &[]),
        (
            "EVT-075.5",
            "RecoverMismatch",
            Sequencer,
            Some(P0),
            &[(0, 8), (8, 8), (64, 64)],
        ),
        ("EVT-075.6", "AdviceMisrouted", Sequencer, Some(P2), &[]),
        ("EVT-075.7", "AdviceStale", Sequencer, Some(P2), &[]),
        ("EVT-075.8", "StartupGate", Sequencer, Some(Info), s64(4)),
        (
            "EVT-075.9",
            "ShadowDeferred",
            Sequencer,
            Some(Info),
            &[(0, 8), (8, 8), (64, 64), (128, 64)],
        ),
        ("EVT-075.10", "IdentityMismatch", Sequencer, Some(P0), va),
        ("EVT-075.11", "RefdataRecheckFailed", Sequencer, Some(P1), &[]),
        ("EVT-075.89", "TimersCancelled", Sequencer, Some(Info), &[]),
        ("EVT-075.90", "TargetCleared", Sequencer, Some(Info), &[]),
        (
            "EVT-075.91",
            "PositionDiff",
            Sequencer,
            Some(P1),
            &[(0, 8), (8, 8), (64, 64), (128, 64)],
        ),
        (
            "EVT-075.93",
            "ReconcileCash",
            Sequencer,
            Some(Info),
            &[(0, 64), (64, 8), (72, 8), (128, 64), (192, 64), (256, 64)],
        ),
        (
            "EVT-075.94",
            "ReconcileRejected",
            Sequencer,
            Some(P0),
            &[(0, 64), (64, 8), (72, 8), (128, 64), (192, 64)],
        ),
        ("EVT-075.12", "RateLimitHit", Risk, Some(P2), &[]),
        ("EVT-075.13", "CapSet", Risk, Some(Info), &[(0, 384)]),
        ("EVT-075.14", "CapCleared", Risk, Some(Info), &[(0, 384)]),
        (
            "EVT-075.15",
            "MarginLevel",
            Risk,
            None,
            &[(0, 8), (8, 8), (64, 64), (128, 64), (192, 64), (256, 64)],
        ),
        ("EVT-075.16", "RecoveryHeldSim", Risk, Some(P1), &[]),
        ("EVT-075.17", "IdentityUnverified", Risk, Some(P1), va),
        ("EVT-075.18", "OmsTransition", Oms, Some(Info), &[]),
        ("EVT-075.19", "OmsUnknown", Oms, None, &[]),
        ("EVT-075.20", "OmsUnknownStuck", Oms, Some(P0), &[]),
        ("EVT-075.21", "OmsVenueReady", Oms, Some(Info), &[]),
        ("EVT-075.22", "OmsStaleGenFill", Oms, Some(P0), &[]),
        ("EVT-075.23", "OmsLateFill", Oms, Some(P0), &[]),
        ("EVT-075.24", "OmsOverfill", Oms, Some(P0), &[]),
        ("EVT-075.25", "OmsForeignOrder", Oms, Some(P1), &[]),
        ("EVT-075.26", "OmsForeignFillViaOrderEvent", Oms, Some(P1), &[]),
        ("EVT-075.27", "OmsExtDedup", Oms, Some(P1), &[]),
        ("EVT-075.28", "OmsForeignFill", Oms, Some(Info), &[]),
        ("EVT-075.29", "OmsHedgeBroken", Oms, Some(P0), &[]),
        ("EVT-075.30", "OmsHedgePxStale", Oms, Some(P1), &[]),
        ("EVT-075.31", "OmsHedgePxGated", Oms, Some(P1), &[]),
        ("EVT-075.32", "OmsHedgeSlipCapped", Oms, Some(P1), &[]),
        ("EVT-075.33", "OmsHedgeNakedCapped", Oms, Some(P1), &[]),
        ("EVT-075.34", "OmsResumeRetry", Oms, Some(P1), &[]),
        ("EVT-075.35", "OmsResumeTimeout", Oms, None, &[]),
        ("EVT-075.36", "OmsFillPxEstimated", Oms, Some(P1), &[]),
        ("EVT-075.37", "OmsFeeUnattributed", Oms, Some(P1), &[]),
        ("EVT-075.38", "OmsCancelNotSent", Oms, Some(P2), &[]),
        ("EVT-075.39", "OmsNeedsRestQuery", Oms, Some(Info), &[]),
        ("EVT-075.40", "OmsAnomaly", Oms, None, &[]),
        ("EVT-075.96", "OmsHedgeResidual", Oms, Some(P1), &[]),
        ("EVT-075.97", "OmsLinkCapacity", Oms, Some(P1), &[]),
        ("EVT-075.41", "RoundClosed", Ledger, Some(Info), &[]),
        (
            "EVT-075.42",
            "Adjusted",
            Ledger,
            Some(Info),
            &[
                (0, 8),
                (8, 8),
                (64, 64),
                (128, 64),
                (192, 64),
                (256, 64),
                (320, 64),
            ],
        ),
        ("EVT-075.43", "BaselineSet", Ledger, Some(Info), &[]),
        ("EVT-075.44", "BalanceInit", Ledger, Some(Info), &[]),
        ("EVT-075.45", "SimReset", Ledger, Some(Info), &[]),
        ("EVT-075.46", "ResidualWrittenOff", Ledger, Some(P1), &[]),
        ("EVT-075.47", "ExternalFill", Ledger, None, &[]),
        ("EVT-075.48", "LedgerAutoHeldOffset", Ledger, Some(P0), &[]),
        ("EVT-075.49", "BalanceDrift", Ledger, Some(P1), &[]),
        ("EVT-075.50", "InvShortfall", Ledger, Some(P1), &[]),
        ("EVT-075.51", "NetQtyMismatch", Ledger, Some(P1), &[]),
        ("EVT-075.52", "FundingMismatch", Ledger, Some(P1), &[]),
        ("EVT-075.53", "FundingCorrected", Ledger, Some(P1), &[]),
        ("EVT-075.54", "ChainReorg", Ledger, Some(P1), &[]),
        ("EVT-075.55", "ChainReorgNaked", Ledger, Some(P0), &[]),
        (
            "EVT-075.92",
            "BalanceCheck",
            Ledger,
            Some(Info),
            &[(0, 8), (8, 8), (64, 64), (128, 64), (192, 64), (256, 64)],
        ),
        (
            "EVT-075.95",
            "ReceiptMismatch",
            Ledger,
            Some(P0),
            &[
                (0, 8),
                (8, 8),
                (64, 64),
                (128, 64),
                (192, 64),
                (256, 64),
                (320, 64),
            ],
        ),
        (
            "EVT-075.96",
            "DayPnlCarried",
            Ledger,
            Some(Info),
            &[(0, 8), (64, 64)],
        ),
        ("EVT-075.56", "ExecBroken", Exec, Some(P0), &[]),
        ("EVT-075.57", "ExecExternalDefault", Exec, None, &[]),
        ("EVT-075.58", "ExecAutoFixFailed", Exec, Some(P0), &[]),
        ("EVT-075.59", "ExecStuck", Exec, Some(P1), &[]),
        ("EVT-075.60", "ExecForceStep", Exec, Some(P1), &[]),
        ("EVT-075.61", "ExecAutoFixDone", Exec, Some(P1), &[]),
        ("EVT-075.62", "ExecSerialFallback", Exec, Some(P0), &[]),
        ("EVT-075.63", "ExecEngineOrderRejected", Exec, Some(P0), &[]),
        ("EVT-075.64", "ExecManualExternal", Exec, None, &[]),
        ("EVT-075.65", "ExecManualHold", Exec, None, &[]),
        ("EVT-075.66", "ExecUngroupedReduce", Exec, Some(P0), &[]),
        ("EVT-075.67", "ExecGateHeldNaked", Exec, Some(P0), &[]),
        ("EVT-075.68", "ExecChainLegExcess", Exec, Some(P0), &[]),
        (
            "EVT-075.69",
            "CtlAudit",
            CtlAudit,
            Some(Info),
            &[
                (0, 128),
                (128, 16),
                (144, 8),
                (152, 8),
                (160, 8),
                (168, 8),
                (176, 8),
                (192, 64),
                (256, 192),
                (448, 32),
            ],
        ),
        ("EVT-075.70", "PartialEventDropped", Boot, Some(P1), &[]),
        ("EVT-075.71", "TailSkipped", Boot, Some(P0), s64(3)),
        ("EVT-075.72", "RecoverFromSnapshot", Boot, Some(P1), s64(5)),
        ("EVT-075.73", "KillSelfTest", Boot, Some(Info), s64(2)),
        ("EVT-075.74", "InputQuarantined", Boot, Some(P0), s64(2)),
        ("EVT-075.75", "StartupStep", Boot, Some(Info), &[]),
        ("EVT-075.76", "HostDegraded", Boot, Some(P1), &[]),
        ("EVT-075.77", "JournalTailLost", Boot, Some(P1), &[]),
        ("EVT-075.85", "RefdataWarning", Boot, Some(P1), s64(3)),
        ("EVT-075.86", "StopMarkerCorrupt", Boot, Some(P0), &[]),
        ("EVT-075.87", "EndpointOverride", Boot, Some(P1), &[(0, 8)]),
        (
            "EVT-075.78",
            "ModelCheckpointDiscarded",
            Advisor,
            Some(P1),
            s64(2),
        ),
        ("EVT-075.79", "ModelPathReset", Advisor, Some(P2), s64(5)),
        ("EVT-075.80", "AdvisorLag", Advisor, Some(P1), s64(4)),
        ("EVT-075.81", "ModelGroupReset", Advisor, Some(P2), s64(2)),
        ("EVT-075.82", "AdvisorStats", Advisor, Some(Info), &[]),
        ("EVT-075.88", "ModelCkptSkipped", Advisor, Some(P1), &[]),
    ];
    assert_eq!(NOTE_NAMES.len(), 95);
    assert_eq!(rows.len(), 95);
    for (i, (row, name, src, lvl, v)) in rows.iter().enumerate() {
        let n = note_name(name).unwrap_or_else(|| panic!("{row}"));
        assert_eq!(NOTE_NAMES[i].name, *name, "{row} order");
        assert_eq!((n.source, n.level), (*src, *lvl), "{row}");
        let got: Vec<(u16, u16)> = n.v.iter().map(|f| (f.bit, f.bits)).collect();
        assert_eq!(got, v.to_vec(), "{row}");
    }
    assert!(note_name("ConfigDiffers").is_none(), "EVT-075.4");
    assert!(note_name("NoSuchNote").is_none(), "EVT-075.83");
    assert!(note_name("VenueReconnects").is_none(), "EVT-075.84");
}

#[test]
fn evt_076_note_registry_invariants() {
    let mut names = HashSet::new();
    for n in NOTE_NAMES {
        assert!(names.insert(n.name), "duplicate {}", n.name);
        match n.source {
            NoteSource::Oms => assert!(n.name.starts_with("Oms"), "{}", n.name),
            NoteSource::Exec => assert!(n.name.starts_with("Exec"), "{}", n.name),
            NoteSource::Strategy | NoteSource::Venue | NoteSource::Measure | NoteSource::Housekeeping => {
                panic!("{} has a source that is not registered here", n.name)
            }
            _ => {}
        }
        for (i, a) in n.v.iter().enumerate() {
            assert!(a.bit + a.bits <= 512, "{}.{}", n.name, a.name);
            if a.bits <= 64 {
                assert_eq!(
                    a.bit / 64,
                    (a.bit + a.bits - 1) / 64,
                    "{}.{} crosses",
                    n.name,
                    a.name
                );
            } else {
                assert!(a.bit % 64 == 0 && a.bits % 64 == 0, "{}.{}", n.name, a.name);
            }
            for b in &n.v[i + 1..] {
                let overlap = a.bit < b.bit + b.bits && b.bit < a.bit + a.bits;
                assert!(!overlap, "{}: {} overlaps {}", n.name, a.name, b.name);
            }
        }
    }
}

#[test]
fn evt_077_note_code_packing() {
    assert_eq!(note_code(NoteSource::Sequencer, 1), 0x0100_0001, "EVT-077.1");
    assert_eq!(note_code(NoteSource::Oms, 5), 0x0400_0005, "EVT-077.2");
    assert_eq!(
        note_code(NoteSource::Advisor, 0xFF_FFFF),
        0x0CFF_FFFF,
        "EVT-077.3"
    );
    if cfg!(debug_assertions) {
        assert!(
            std::panic::catch_unwind(|| note_code(NoteSource::Exec, 0)).is_err(),
            "EVT-077.4"
        );
        assert!(
            std::panic::catch_unwind(|| note_code(NoteSource::Risk, 0x100_0000)).is_err(),
            "EVT-077.5"
        );
    }
    assert_eq!(note_code(NoteSource::Boot, 1), 0x0B00_0001, "EVT-077.6");
}

#[test]
fn evt_078_vfield_get_put() {
    const INIT: [u64; 8] = [0x1111_1111_1111_1111; 8];
    let f = |bit, bits| VField { name: "x", bit, bits };
    let mut v = INIT;
    assert_eq!(f(8, 8).put(&mut v, 0xAB), Ok(()), "EVT-078.1");
    assert_eq!(v[0], 0x1111_1111_1111_AB11, "EVT-078.1");
    assert_eq!(v[1..], INIT[1..], "EVT-078.1");
    assert_eq!(f(8, 8).get(&v), 0xAB, "EVT-078.1");
    let mut v = INIT;
    assert_eq!(f(8, 8).put(&mut v, 0xFF), Ok(()), "EVT-078.2");
    assert_eq!(f(8, 8).get(&v), 0xFF, "EVT-078.2");
    let mut v = INIT;
    assert_eq!(f(8, 8).put(&mut v, 0x100), Err(EncodeErr::TooLong), "EVT-078.3");
    assert_eq!(v, INIT, "EVT-078.3");
    let mut v = INIT;
    assert_eq!(f(128, 16).put(&mut v, 0xFFFF), Ok(()), "EVT-078.4");
    assert_eq!(v[2], 0x1111_1111_1111_FFFF, "EVT-078.4");
    let mut v = INIT;
    assert_eq!(f(64, 64).put(&mut v, u64::MAX), Ok(()), "EVT-078.5");
    assert_eq!(v[1], u64::MAX, "EVT-078.5");
    assert_eq!((v[0], &v[2..]), (INIT[0], &INIT[2..]), "EVT-078.5");
    let mut v = INIT;
    f(176, 8).put(&mut v, 0x07).unwrap();
    assert_eq!(f(176, 8).get(&v), 0x07, "EVT-078.6");
    assert_eq!(v[2] ^ INIT[2], (0x07u64 ^ 0x11) << 48, "EVT-078.6");
}

#[test]
fn evt_091_cap_record_in_note() {
    let cap = CapRecord {
        source: CapSource::KILL,
        level: 3,
        cflags: CapRecord::HARD,
        _pad: [0; 5],
        scope: CmdScope {
            strategy: StrategyId(0),
            venue: VenueId(0),
            account: AccountId(5),
            _pad: 0,
            group: GROUP_ALL,
            inst: InstId(0),
            _pad2: 0,
        },
        set_seq: 77,
        params: [3, 0],
    };
    let mut v = [0u64; 8];
    v.as_mut_bytes()[..48].copy_from_slice(cap.as_bytes());
    let note = Note {
        code: note_code(NoteSource::Risk, 2),
        level: NoteLevel::Info,
        source: NoteSource::Risk,
        strategy: StrategyId(0),
        _pad: 0,
        group: GROUP_NONE,
        inst: INST_NONE,
        v,
    };
    let rec = enc(&note, 1, 1, RecordFlags(0));
    let back = decode_record(&rec).unwrap().read::<Note>().unwrap();
    let cap2 = CapRecord::read_from_bytes(&back.v.as_bytes()[..48]).unwrap();
    assert_eq!(cap2, cap);
    assert_eq!(cap2.scope.group.0, u32::MAX);
    assert_eq!((back.v[6], back.v[7]), (0, 0));
}
