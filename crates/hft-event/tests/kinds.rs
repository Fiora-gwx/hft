//! Record flags, the `KINDS` registry, `kind_info`, `classify` and field tables
//! (EVT-002, EVT-004 – EVT-007, EVT-171).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![allow(clippy::type_complexity)]

use crate::bodies;
use crate::common;

use common::*;
use hft_event::*;
use std::collections::HashSet;

#[test]
fn evt_002_record_flag_bits() {
    let all = [
        ("EVT-002.1", RecordFlags::CONFLATED, 0x0001),
        ("EVT-002.2", RecordFlags::SHADOW, 0x0002),
        ("EVT-002.3", RecordFlags::HW_TS, 0x0004),
        ("EVT-002.4", RecordFlags::SW_TS, 0x0008),
        ("EVT-002.5", RecordFlags::PART, 0x0010),
        ("EVT-002.6", RecordFlags::CRC, 0x0020),
        ("EVT-002.7", RecordFlags::EVENT_END, 0x0040),
    ];
    let mut or = 0;
    for (row, f, v) in all {
        assert_eq!(f.0, v, "{row}");
        or |= f.0;
    }
    for (i, a) in all.iter().enumerate() {
        for b in &all[i + 1..] {
            assert_eq!(a.1.0 & b.1.0, 0, "EVT-002.8 {} {}", a.0, b.0);
        }
    }
    assert_eq!(or, 0x007F, "EVT-002.8");
}

#[derive(Clone, Copy)]
enum L {
    F(u16),
    V(u16, u32),
    M(u16),
}

#[test]
fn evt_004_kinds_registry() {
    use CrcPolicy::{Always as A, Never as N};
    use Domain::*;
    use Role::{ByBody as B, Input as I, Meta as Me, Output as O};
    let s = JournalMask::SEQ;
    let sc = JournalMask::SEQ | JournalMask::CHAIN;
    let src = JournalMask::SEQ | JournalMask::RAW | JournalMask::CHAIN;
    let r = JournalMask::RAW;
    let rows: &[(u16, &str, Domain, L, Role, u16, JournalMask, CrcPolicy, bool)] = &[
        (0x0101, "Bbo", Market, L::F(96), I, 1, s, N, false),
        (0x0102, "L2Delta", Market, L::F(96), I, 1, s, N, false),
        (0x0103, "L2Snapshot", Market, L::M(72), B, 1, s, N, false),
        (0x0104, "Trade", Market, L::F(88), I, 1, s, N, false),
        (0x0105, "Funding", Market, L::F(80), I, 1, s, N, false),
        (0x0106, "MarkPrice", Market, L::F(64), I, 1, s, N, false),
        (0x0107, "IndexPrice", Market, L::F(64), I, 1, s, N, false),
        (0x0108, "Liquidation", Market, L::F(80), I, 1, s, N, false),
        (0x0109, "L2Batch", Market, L::V(80, 64_000), I, 1, s, N, false),
        (0x0201, "OrderDecision", Exec, L::F(88), O, 1, s, N, false),
        (0x0202, "SendReport", Exec, L::F(48), I, 1, s, N, false),
        (0x0203, "OrderEvent", Exec, L::F(136), I, 4, s, N, false),
        (0x0204, "PositionReport", Exec, L::F(56), I, 1, s, N, false),
        (0x0205, "BalanceReport", Exec, L::F(40), I, 1, s, N, false),
        (0x0206, "FundingSettle", Exec, L::F(56), B, 2, s, N, false),
        (0x0207, "ExternalFill", Exec, L::F(96), I, 3, s, N, false),
        (0x0208, "MarginReport", Exec, L::V(64, 16_384), I, 1, s, N, false),
        (
            0x0209,
            "AccountConfigReport",
            Exec,
            L::V(64, 16_384),
            I,
            1,
            s,
            N,
            false,
        ),
        (0x020A, "ExecDecision", Exec, L::F(104), O, 1, s, N, false),
        (0x0301, "ChainHead", Chain, L::F(80), I, 1, s, N, true),
        (0x0302, "PoolQuote", Chain, L::F(96), I, 1, s, N, true),
        (0x0303, "SwapDecision", Chain, L::F(128), O, 1, s, N, true),
        (0x0304, "TxLifecycle", Chain, L::F(152), I, 1, s, N, true),
        (0x0401, "ControlCmd", Control, L::M(128), I, 1, s, A, false),
        (0x0402, "CmdResult", Control, L::F(144), O, 3, s, N, false),
        (0x0403, "ConfigBlob", Control, L::M(72), B, 1, sc, A, false),
        (0x0404, "ReconcileReport", Control, L::F(104), I, 3, s, N, false),
        (
            0x0405,
            "ReconcileBatch",
            Control,
            L::V(16, 51_200),
            I,
            1,
            s,
            N,
            false,
        ),
        (0x0501, "Timer", System, L::F(24), I, 1, s, N, false),
        (0x0502, "ConnState", System, L::F(24), I, 2, s, N, false),
        (0x0503, "Advice", System, L::V(32, 4_096), I, 1, s, N, false),
        (0x0504, "StateSnapshot", System, L::M(96), O, 1, sc, A, false),
        (0x0505, "LatencySample", System, L::F(136), Me, 3, s, N, false),
        (0x0506, "Note", System, L::F(80), B, 1, s, N, false),
        (0x0507, "HostProfile", System, L::F(176), Me, 1, sc, N, false),
        (0x0508, "ClockStatus", System, L::F(56), Me, 1, sc, N, false),
        (0x0509, "SegmentRoll", System, L::F(48), Me, 1, src, N, false),
        (0x050A, "EngineClock", System, L::F(32), I, 1, s, N, false),
        (0x0601, "RawRx", Raw, L::M(24), Me, 1, r, A, false),
        (0x0602, "RawTx", Raw, L::M(24), Me, 1, r, A, false),
        (0x050B, "UtcAnchor", System, L::F(32), I, 1, s, N, false),
    ];
    for (i, &(k, name, dom, lay, role, ver, j, crc, opt)) in rows.iter().enumerate() {
        let row = format!("EVT-004.{}", i + 1);
        let e = kind_info(EventKind(k)).unwrap_or_else(|| panic!("{row}"));
        assert_eq!(e.kind, EventKind(k), "{row}");
        assert_eq!(e.name, name, "{row}");
        assert_eq!(e.domain, dom, "{row}");
        let want = match lay {
            L::F(size) => Layout::Fixed { size },
            L::V(prefix, max) => Layout::Var { prefix, max },
            L::M(prefix) => Layout::Multi { prefix },
        };
        assert_eq!(e.layout, want, "{row}");
        assert_eq!(e.role, role, "{row}");
        assert_eq!(e.schema_version, ver, "{row}");
        assert_eq!(e.journals, j, "{row}");
        assert_eq!(e.crc, crc, "{row}");
        assert_eq!(e.optional, opt, "{row}");
    }
    if !cfg!(feature = "dev-kinds") {
        assert_eq!(KINDS.len(), 41);
    }
    assert_eq!(KINDS.iter().filter(|k| k.domain != Domain::Test).count(), 41);
}

#[test]
fn evt_005_kinds_match_bodies() {
    let bodies = bodies::bodies();
    for b in &bodies {
        assert!(kind_info(b.0).is_some(), "Body {:?} not registered", b.0);
    }
    for k in KINDS.iter().filter(|k| k.domain != Domain::Test) {
        match k.layout {
            Layout::Fixed { size } => {
                let m: Vec<_> = bodies.iter().filter(|b| b.0 == k.kind && b.3 == 0).collect();
                assert_eq!(m.len(), 1, "{}", k.name);
                assert_eq!(m[0].2, size as usize, "{}", k.name);
                assert_eq!(m[0].1, k.schema_version, "{}", k.name);
            }
            Layout::Var { prefix, .. } => {
                let m: Vec<_> = bodies.iter().filter(|b| b.0 == k.kind && b.3 == 1).collect();
                assert_eq!(m.len(), 1, "{}", k.name);
                assert_eq!(m[0].2, prefix as usize, "{}", k.name);
            }
            Layout::Multi { .. } => {}
        }
        let dom = match k.kind.0 >> 8 {
            0x01 => Domain::Market,
            0x02 => Domain::Exec,
            0x03 => Domain::Chain,
            0x04 => Domain::Control,
            0x05 => Domain::System,
            0x06 => Domain::Raw,
            _ => panic!("{} out of range", k.name),
        };
        assert_eq!(k.domain, dom, "{}", k.name);
        assert!(!(0x0310..=0x033F).contains(&k.kind.0), "{}", k.name);
        assert!(!k.fields.is_empty(), "{}", k.name);
        let last = k.fields.last().unwrap();
        let end = last.offset + last.ty.width();
        let want = match k.layout {
            Layout::Fixed { size } => size,
            Layout::Var { prefix, .. } | Layout::Multi { prefix } => prefix,
        };
        assert_eq!(end, want, "{}", k.name);
    }
    let set: HashSet<u16> = KINDS.iter().map(|k| k.kind.0).collect();
    assert_eq!(set.len(), KINDS.len());
}

#[test]
fn evt_006_kind_info_lookup() {
    let bbo = kind_info(EventKind(0x0101)).unwrap();
    assert_eq!((bbo.name, bbo.kind), ("Bbo", EventKind(0x0101)), "EVT-006.1");
    assert_eq!(
        kind_info(EventKind(0x050A)).unwrap().name,
        "EngineClock",
        "EVT-006.2"
    );
    for (row, k) in [
        ("EVT-006.3", 0x0000u16),
        ("EVT-006.4", 0x010A),
        ("EVT-006.5", 0x020B),
        ("EVT-006.6", 0x0310),
        ("EVT-006.7", 0x050C),
        ("EVT-006.9", 0xFFFF),
    ] {
        assert!(kind_info(EventKind(k)).is_none(), "{row}");
    }
    if !cfg!(feature = "dev-kinds") {
        assert!(kind_info(EventKind(0x7F01)).is_none(), "EVT-006.8");
    }
}

fn body_with(len: usize, at: usize, v: u8) -> Vec<u8> {
    let mut b = vec![0u8; len];
    b[at] = v;
    b
}

#[test]
fn evt_007_classify() {
    use Role::*;
    let z = |k: EventKind| {
        let n = match kind_info(k).unwrap().layout {
            Layout::Fixed { size } => size,
            Layout::Var { prefix, .. } | Layout::Multi { prefix } => prefix,
        };
        vec![0u8; n as usize]
    };
    assert_eq!(
        classify(EventKind::BBO, &z(EventKind::BBO)),
        Ok(Input),
        "EVT-007.1"
    );
    assert_eq!(
        classify(EventKind::ORDER_DECISION, &z(EventKind::ORDER_DECISION)),
        Ok(Output),
        "EVT-007.2"
    );
    assert_eq!(
        classify(EventKind::EXEC_DECISION, &z(EventKind::EXEC_DECISION)),
        Ok(Output),
        "EVT-007.3"
    );
    assert_eq!(
        classify(EventKind::LATENCY_SAMPLE, &z(EventKind::LATENCY_SAMPLE)),
        Ok(Meta),
        "EVT-007.4"
    );
    assert_eq!(
        classify(EventKind::SEGMENT_ROLL, &z(EventKind::SEGMENT_ROLL)),
        Ok(Meta),
        "EVT-007.5"
    );
    assert_eq!(
        classify(EventKind::RAW_RX, &z(EventKind::RAW_RX)),
        Ok(Meta),
        "EVT-007.6"
    );
    assert_eq!(
        classify(EventKind::ENGINE_CLOCK, &z(EventKind::ENGINE_CLOCK)),
        Ok(Input),
        "EVT-007.7"
    );
    let l2 = |o| classify(EventKind::L2_SNAPSHOT, &body_with(72, 64, o));
    assert_eq!(l2(1), Ok(Input), "EVT-007.8");
    assert_eq!(l2(2), Ok(Output), "EVT-007.9");
    assert_eq!(l2(3), Ok(Input), "EVT-007.10");
    assert_eq!(l2(0), Err(DecodeErr::InvalidEnum), "EVT-007.11");
    assert_eq!(l2(4), Err(DecodeErr::InvalidEnum), "EVT-007.12");
    let blob = |o| classify(EventKind::CONFIG_BLOB, &body_with(72, 64, o));
    assert_eq!(blob(0), Ok(Input), "EVT-007.13");
    assert_eq!(blob(1), Ok(Output), "EVT-007.14");
    assert_eq!(blob(2), Err(DecodeErr::InvalidEnum), "EVT-007.15");
    let fs = |o| classify(EventKind::FUNDING_SETTLE, &body_with(56, 48, o));
    assert_eq!(fs(0), Ok(Input), "EVT-007.16");
    assert_eq!(fs(1), Ok(Output), "EVT-007.17");
    assert_eq!(fs(2), Err(DecodeErr::InvalidEnum), "EVT-007.18");
    let note = |s| classify(EventKind::NOTE, &body_with(80, 5, s));
    for s in [1, 2, 3, 4, 9, 10] {
        assert_eq!(note(s), Ok(Output), "EVT-007.19 source {s}");
    }
    for s in [5, 6, 8, 12] {
        assert_eq!(note(s), Ok(Meta), "EVT-007.20 source {s}");
    }
    for s in [7, 11] {
        assert_eq!(note(s), Ok(Meta), "EVT-007.21 source {s}");
    }
    assert_eq!(note(0), Err(DecodeErr::InvalidEnum), "EVT-007.22");
    assert_eq!(note(13), Err(DecodeErr::InvalidEnum), "EVT-007.23");
    assert_eq!(
        classify(EventKind(0x010A), &[7u8; 16]),
        Err(DecodeErr::UnknownKind),
        "EVT-007.24"
    );
    assert_eq!(
        classify(EventKind::FUNDING_SETTLE, &[9u8; 48]),
        Ok(Input),
        "EVT-007.25"
    );
}

fn flatten(base: u16, fields: &'static [FieldDesc], out: &mut Vec<(u16, FieldDesc)>) {
    for f in fields {
        if let FieldTy::Nested(inner) = f.ty {
            flatten(base + f.offset, inner, out);
        } else {
            out.push((base + f.offset, *f));
        }
    }
}

#[test]
fn evt_171_field_desc_matches_layout() {
    // Offsets against offset_of! for a sample of each form (FIELDS are generated from offset_of!).
    assert_eq!(
        field(Bbo::FIELDS, "update_id").offset as usize,
        core::mem::offset_of!(Bbo, update_id)
    );
    assert_eq!(
        field(OrderDecision::FIELDS, "gen").offset as usize,
        core::mem::offset_of!(OrderDecision, r#gen)
    );
    assert_eq!(
        field(CmdHdr::FIELDS, "cmd_hash").offset as usize,
        core::mem::offset_of!(CmdHdr, cmd_hash)
    );
    for k in KINDS.iter().filter(|k| k.domain != Domain::Test) {
        let mut flat = Vec::new();
        flatten(0, k.fields, &mut flat);
        let mut at = 0u16;
        for (off, f) in &flat {
            assert_eq!(*off, at, "{}.{} not contiguous", k.name, f.name);
            at = off + f.ty.width();
            if f.name.starts_with("_pad") || f.name.starts_with("_rsv") {
                assert!(matches!(f.ty, FieldTy::Pad(_)), "{}.{}", k.name, f.name);
            }
            let n = f.name;
            if n == "px" || n.ends_with("_px") {
                assert_eq!(f.ty, FieldTy::Px, "{}.{}", k.name, n);
            }
            if n == "qty" || n.ends_with("_qty") {
                assert_eq!(f.ty, FieldTy::Qty, "{}.{}", k.name, n);
            }
        }
        let want = match k.layout {
            Layout::Fixed { size } => size,
            Layout::Var { prefix, .. } | Layout::Multi { prefix } => prefix,
        };
        assert_eq!(at, want, "{}", k.name);
    }
    assert_eq!(field(ReconcileReport::FIELDS, "diff").ty, FieldTy::Notional);
    assert_eq!(field(Funding::FIELDS, "rate").ty, FieldTy::Rate);
    assert_eq!(field(PositionReport::FIELDS, "upnl").ty, FieldTy::Notional);
    assert_eq!(field(LeverageItem::FIELDS, "leverage").ty, FieldTy::Rate);
}
