//! Measurement mask (EVT-100 – EVT-102).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod common;

use common::*;
use hft_event::*;
use hft_types::{Px, Tsc};
use std::collections::BTreeSet;
use zerocopy::{FromBytes, IntoBytes};

#[test]
fn evt_100_measure_mask_entries() {
    assert_eq!(MEASURE_MASK.len(), 5);
    let rows = [
        ("EVT-100.1", (EventKind::ORDER_DECISION, 8, 8)),
        ("EVT-100.2", (EventKind::SWAP_DECISION, 24, 8)),
        ("EVT-100.3", (EventKind::STATE_SNAPSHOT, 56, 8)),
        ("EVT-100.4", (EventKind::STATE_SNAPSHOT, 64, 16)),
        ("EVT-100.5", (EventKind::STATE_SNAPSHOT, 80, 8)),
    ];
    for (row, e) in rows {
        assert!(MEASURE_MASK.contains(&e), "{row}");
    }
    for k in [
        EventKind::EXEC_DECISION,
        EventKind::CMD_RESULT,
        EventKind::L2_SNAPSHOT,
        EventKind::CONFIG_BLOB,
        EventKind::FUNDING_SETTLE,
        EventKind::NOTE,
    ] {
        assert!(MEASURE_MASK.iter().all(|m| m.0 != k), "EVT-100.6 {k:?}");
    }
}

fn measured(base: u16, fields: &'static [FieldDesc], out: &mut BTreeSet<(u16, u16)>) {
    for f in fields {
        if let FieldTy::Nested(inner) = f.ty {
            measured(base + f.offset, inner, out);
        } else if f.measure {
            out.insert((base + f.offset, f.ty.width()));
        }
    }
}

#[test]
fn evt_101_measure_flags_consistent() {
    for k in KINDS
        .iter()
        .filter(|k| matches!(k.role, Role::Output | Role::ByBody))
    {
        let mut got = BTreeSet::new();
        measured(0, k.fields, &mut got);
        let want: BTreeSet<(u16, u16)> = MEASURE_MASK
            .iter()
            .filter(|m| m.0 == k.kind)
            .map(|m| (m.1, m.2))
            .collect();
        assert_eq!(got, want, "{}", k.name);
    }
    assert!(!field(FundingSettle::FIELDS, "rx_tsc").measure);
}

/// Zeroes header tsc and the masked body bytes of an Output record, then compares.
fn masked_eq(kind: EventKind, a: &[u8], b: &[u8], body_at: usize) -> bool {
    let norm = |r: &[u8]| {
        let mut r = r.to_vec();
        if body_at == 24 {
            r[16..24].fill(0);
        }
        for m in MEASURE_MASK.iter().filter(|m| m.0 == kind) {
            let o = body_at + m.1 as usize;
            r[o..o + m.2 as usize].fill(0);
        }
        r
    };
    norm(a) == norm(b)
}

#[test]
fn evt_102_masked_compare() {
    let od = order_decision();
    let a = enc(&od, 1, 100, RecordFlags(0));
    let b = enc(
        &OrderDecision {
            decided_tsc: Tsc(999),
            ..od
        },
        1,
        200,
        RecordFlags(0),
    );
    assert!(masked_eq(EventKind::ORDER_DECISION, &a, &b, 24), "EVT-102.1");
    let c = enc(
        &OrderDecision {
            px: Px(od.px.0 + 1),
            ..od
        },
        1,
        100,
        RecordFlags(0),
    );
    assert!(!masked_eq(EventKind::ORDER_DECISION, &a, &c, 24), "EVT-102.2");

    let snap = SnapshotHdr::read_from_bytes(&canonical(SnapshotHdr::FIELDS)[..]).unwrap();
    let logical = |h: &SnapshotHdr| {
        let mut l = h.as_bytes().to_vec();
        l.extend([5u8; 40]);
        l
    };
    let s1 = logical(&snap);
    let s2 = logical(&SnapshotHdr {
        snap_tsc: 1,
        boot_id: [9; 16],
        snap_utc_ns: -3,
        ..snap
    });
    assert!(masked_eq(EventKind::STATE_SNAPSHOT, &s1, &s2, 0), "EVT-102.3");
    let s3 = logical(&SnapshotHdr {
        at_seq: snap.at_seq + 1,
        ..snap
    });
    assert!(!masked_eq(EventKind::STATE_SNAPSHOT, &s1, &s3, 0), "EVT-102.4");
    let s4 = logical(&SnapshotHdr {
        engine_ns: snap.engine_ns + 1,
        ..snap
    });
    assert!(!masked_eq(EventKind::STATE_SNAPSHOT, &s1, &s4, 0), "EVT-102.5");

    let ed = ExecDecision::read_from_bytes(&canonical(ExecDecision::FIELDS)[..]).unwrap();
    let e1 = enc(&ed, 1, 1, RecordFlags(0));
    let e2 = enc(
        &ExecDecision {
            depth_qty: hft_types::Qty(ed.depth_qty.0 + 1),
            ..ed
        },
        1,
        1,
        RecordFlags(0),
    );
    assert!(!masked_eq(EventKind::EXEC_DECISION, &e1, &e2, 24), "EVT-102.6");

    let sd: SwapDecision =
        zerocopy::TryFromBytes::try_read_from_bytes(&canonical(SwapDecision::FIELDS)[..]).unwrap();
    let w1 = enc(&sd, 1, 1, RecordFlags(0));
    let w2 = enc(
        &SwapDecision {
            decided_tsc: Tsc(1),
            ..sd
        },
        1,
        1,
        RecordFlags(0),
    );
    assert!(masked_eq(EventKind::SWAP_DECISION, &w1, &w2, 24), "EVT-102.7");

    let fs = FundingSettle::read_from_bytes(&canonical(FundingSettle::FIELDS)[..]).unwrap();
    let f1 = enc(
        &FundingSettle {
            synth: 1,
            rx_tsc: Tsc(0),
            ..fs
        },
        1,
        1,
        RecordFlags(0),
    );
    let f2 = enc(
        &FundingSettle {
            synth: 1,
            rx_tsc: Tsc(5),
            ..fs
        },
        1,
        1,
        RecordFlags(0),
    );
    assert!(!masked_eq(EventKind::FUNDING_SETTLE, &f1, &f2, 24), "EVT-102.8");
}
