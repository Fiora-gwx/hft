//! Ring element types and byte-ring frames (EVT-121, EVT-122, EVT-124, EVT-140 – EVT-143).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use core::mem::offset_of;
use hft_event::*;
use hft_types::*;
use std::any::TypeId;

fn hdr(len: u16) -> AdviceHdr {
    AdviceHdr {
        strategy: StrategyId(3),
        account: AccountId(0),
        local_kind: 2,
        group: GROUP_NONE,
        len,
        _pad: [0; 6],
        basis_seq: 900,
        basis_tsc: Tsc(12_345),
    }
}

fn payload(n: usize) -> Vec<u8> {
    (0..n).map(|i| i as u8).collect()
}

#[test]
fn evt_121_advice_record_roundtrip() {
    for (row, n, want) in [
        ("EVT-121.1", 0usize, 32usize),
        ("EVT-121.2", 1, 40),
        ("EVT-121.3", 7, 40),
        ("EVT-121.4", 8, 40),
        ("EVT-121.5", 9, 48),
        ("EVT-121.6", 4096, 4128),
    ] {
        let p = payload(n);
        let rec = AdviceRecord {
            hdr: hdr(n as u16),
            payload: &p,
        };
        assert_eq!(rec.encoded_len(), want, "{row}");
        let mut out = vec![0xAAu8; 8192];
        assert_eq!(rec.encode_into(&mut out), Ok(want), "{row}");
        assert!(out[32 + n..want].iter().all(|&b| b == 0), "{row} padding");
        assert!(out[want..].iter().all(|&b| b == 0xAA), "{row} past end");
        let back = AdviceRecord::read(&out[..want]).unwrap();
        assert_eq!(back.hdr, rec.hdr, "{row}");
        assert_eq!(back.payload, &p[..], "{row}");
    }
}

#[test]
fn evt_122_advice_record_errors() {
    let p = payload(4097);
    let mut out = vec![0u8; 8192];
    assert_eq!(
        AdviceRecord {
            hdr: hdr(4097),
            payload: &p
        }
        .encode_into(&mut out),
        Err(EncodeErr::TooLong),
        "EVT-122.1"
    );
    assert_eq!(
        AdviceRecord {
            hdr: hdr(10),
            payload: &p[..9]
        }
        .encode_into(&mut out),
        Err(EncodeErr::Count),
        "EVT-122.2"
    );
    assert_eq!(
        AdviceRecord {
            hdr: hdr(1),
            payload: &p[..1]
        }
        .encode_into(&mut out[..39]),
        Err(EncodeErr::BufTooSmall),
        "EVT-122.3"
    );
    assert_eq!(
        AdviceRecord::read(&out[..31]).unwrap_err(),
        DecodeErr::Short,
        "EVT-122.4"
    );
    let mut b = vec![0u8; 128];
    b[8..10].copy_from_slice(&100u16.to_le_bytes());
    assert_eq!(AdviceRecord::read(&b).unwrap_err(), DecodeErr::Short, "EVT-122.5");
    let mut b = vec![0u8; 4136];
    b[8..10].copy_from_slice(&4097u16.to_le_bytes());
    assert_eq!(
        AdviceRecord::read(&b).unwrap_err(),
        DecodeErr::TooLong,
        "EVT-122.6"
    );
}

fn note(source: NoteSource) -> Note {
    Note {
        code: note_code(NoteSource::Advisor, 3),
        level: NoteLevel::P1,
        source,
        strategy: StrategyId(3),
        _pad: 0,
        group: GROUP_NONE,
        inst: INST_NONE,
        v: [1, 2, 3, 4, 0, 0, 0, 0],
    }
}

#[test]
fn evt_124_advice_msg_frames() {
    let p = payload(9);
    let mut out = vec![0xAAu8; 8192];
    let m = AdviceMsg::Advice(AdviceRecord {
        hdr: hdr(9),
        payload: &p,
    });
    assert_eq!(m.encode_into(&mut out), Ok(56), "EVT-124.1");
    assert_eq!(&out[0..2], &0x0503u16.to_le_bytes(), "EVT-124.1");
    assert_eq!(&out[4..8], &48u32.to_le_bytes(), "EVT-124.1");
    match AdviceMsg::read(&out[..56]).unwrap() {
        AdviceMsg::Advice(a) => assert_eq!((a.hdr, a.payload), (hdr(9), &p[..]), "EVT-124.1"),
        x => panic!("EVT-124.1 {x:?}"),
    }
    let n = note(NoteSource::Advisor);
    assert_eq!(AdviceMsg::Note(n).encode_into(&mut out), Ok(88), "EVT-124.2");
    assert_eq!(&out[0..2], &0x0506u16.to_le_bytes(), "EVT-124.2");
    assert_eq!(&out[4..8], &80u32.to_le_bytes(), "EVT-124.2");
    match AdviceMsg::read(&out[..88]).unwrap() {
        AdviceMsg::Note(x) => assert_eq!(x, n, "EVT-124.2"),
        x => panic!("EVT-124.2 {x:?}"),
    }
    let mut out2 = vec![0u8; 256];
    assert_eq!(
        AdviceMsg::Note(note(NoteSource::Housekeeping)).encode_into(&mut out2),
        Err(EncodeErr::Count),
        "EVT-124.3"
    );
    let mut boot = out[..88].to_vec();
    boot[8 + 5] = NoteSource::Boot as u8;
    assert_eq!(
        AdviceMsg::read(&boot).unwrap_err(),
        DecodeErr::InvalidEnum,
        "EVT-124.4"
    );
    let mut k = out[..88].to_vec();
    k[0..2].copy_from_slice(&0x0401u16.to_le_bytes());
    assert_eq!(
        AdviceMsg::read(&k).unwrap_err(),
        DecodeErr::KindMismatch,
        "EVT-124.5"
    );
    let mut s = out[..88].to_vec();
    s[4..8].copy_from_slice(&72u32.to_le_bytes());
    assert_eq!(AdviceMsg::read(&s).unwrap_err(), DecodeErr::Short, "EVT-124.6");
    let mut l = out[..88].to_vec();
    l.extend([0u8; 8]);
    l[4..8].copy_from_slice(&88u32.to_le_bytes());
    assert_eq!(AdviceMsg::read(&l).unwrap_err(), DecodeErr::TooLong, "EVT-124.7");
    assert_eq!(
        AdviceMsg::read(&out[..7]).unwrap_err(),
        DecodeErr::Short,
        "EVT-124.8"
    );
    assert_eq!(
        AdviceMsg::Note(n).encode_into(&mut out2[..87]),
        Err(EncodeErr::BufTooSmall),
        "EVT-124.9"
    );
}

fn assert_copy<T: Copy>() {}

#[test]
fn evt_140_ring_element_sizes() {
    assert!(size_of::<FeedMsg>() <= 128, "EVT-140.1");
    assert_copy::<FeedMsg>();
    assert!(size_of::<RepMsg>() <= 144, "EVT-140.2");
    assert_copy::<RepMsg>();
    assert!(size_of::<HkMsg>() <= 128, "EVT-140.3");
    assert_copy::<HkMsg>();
    assert!(size_of::<FeedCmd>() <= 16, "EVT-140.4");
    assert_copy::<FeedCmd>();
    assert_eq!((size_of::<GwCmd>(), align_of::<GwCmd>()), (64, 8), "EVT-140.5");
    assert_copy::<GwCmd>();
    assert_eq!(TypeId::of::<OutMsg>(), TypeId::of::<GwCmd>(), "EVT-140.6");
}

#[test]
fn evt_141_gwcmd_layout() {
    let o = [
        offset_of!(GwCmd, decision_seq),
        offset_of!(GwCmd, decided_tsc),
        offset_of!(GwCmd, px),
        offset_of!(GwCmd, qty),
        offset_of!(GwCmd, inst),
        offset_of!(GwCmd, slot),
        offset_of!(GwCmd, r#gen),
        offset_of!(GwCmd, target_slot),
        offset_of!(GwCmd, op),
        offset_of!(GwCmd, account),
        offset_of!(GwCmd, side),
        offset_of!(GwCmd, tif),
        offset_of!(GwCmd, oflags),
        offset_of!(GwCmd, _pad),
        offset_of!(GwCmd, arg),
        offset_of!(GwCmd, arg2),
    ];
    assert_eq!(o, [0, 8, 16, 24, 32, 36, 40, 44, 48, 49, 50, 51, 52, 54, 56, 60]);
}

/// Every field not listed is zero (`side`/`tif` carry the placeholders Buy/Gtc).
fn rest_zero(c: &GwCmd) -> bool {
    c.decision_seq == 0
        && c.decided_tsc == Tsc(0)
        && c.oflags == 0
        && c._pad == [0; 2]
        && c.side == Side::Buy
        && c.tif == Tif::Gtc
}

#[test]
fn evt_142_gwcmd_ctors() {
    let c = GwCmd::mirror_restore(AccountId(2), InstId(77), SlotId(9), 3, 0x7FFF, u64::MAX);
    assert_eq!(c.op, GwOp::MirrorRestore, "EVT-142.1");
    assert_eq!(
        (c.account, c.inst, c.slot, c.r#gen, c.qty),
        (AccountId(2), InstId(77), SlotId(9), 3, Qty(0))
    );
    assert_eq!(
        (c.arg, c.decision_seq, c.oflags),
        (0x0000_7FFF, 0, 0),
        "EVT-142.1"
    );
    assert_eq!((c.exch_order_id(), c.epoch()), (u64::MAX, 0x7FFF), "EVT-142.1");
    let c = GwCmd::mirror_restore(AccountId(2), InstId(77), SlotId(9), 3, 1, 0x8000_0000_0000_0001);
    assert_eq!(c.exch_order_id(), 0x8000_0000_0000_0001, "EVT-142.2");
    let c = GwCmd::mirror_restore(AccountId(2), InstId(77), SlotId(9), 3, 0, 0);
    assert_eq!((c.arg, c.epoch(), c.exch_order_id()), (0, 0, 0), "EVT-142.3");
    let c = GwCmd::set_epoch(0x1234, true, 0x1200);
    assert_eq!(
        (c.op, c.arg, c.arg2),
        (GwOp::SetEpoch, 0x0000_1234, 0x1200_0001),
        "EVT-142.4"
    );
    assert!(
        rest_zero(&c) && c.px == Px(0) && c.qty == Qty(0) && c.inst == InstId(0) && c.account == AccountId(0)
    );
    assert_eq!(
        (c.epoch(), c.accept_prev(), c.prev_epoch()),
        (0x1234, true, 0x1200),
        "EVT-142.4"
    );
    let c = GwCmd::sim_reset(AccountId(9));
    assert_eq!((c.op, c.account), (GwOp::SimReset, AccountId(9)), "EVT-142.5");
    assert!(
        rest_zero(&c) && c.px == Px(0) && c.qty == Qty(0) && c.arg == 0 && c.arg2 == 0,
        "EVT-142.5"
    );
    let c = GwCmd::sim_transfer(
        AccountId(9),
        VenueId(1),
        VenueId(2),
        AssetId(0xFFFF),
        Qty(5_000_000),
    );
    assert_eq!(
        (c.op, c.qty, c.arg, c.px),
        (GwOp::SimTransfer, Qty(5_000_000), 0x0201_FFFF, Px(0)),
        "EVT-142.6"
    );
    let c = GwCmd::sim_transfer(AccountId(9), VenueId(255), VenueId(255), AssetId(0), Qty(1));
    assert_eq!(c.arg, 0xFFFF_0000, "EVT-142.7");
    let c = GwCmd::set_epoch(70, false, 5);
    assert_eq!(
        (c.arg2, c.accept_prev(), c.prev_epoch()),
        (0x0005_0000, false, 5),
        "EVT-142.8"
    );
    let c = GwCmd::set_epoch(u16::MAX, true, u16::MAX);
    assert_eq!((c.arg, c.arg2), (0x0000_FFFF, 0xFFFF_0001), "EVT-142.9");
    let c = GwCmd::sim_fault(AccountId(9), VenueId(3), 4, 3_000);
    assert_eq!(
        (c.op, c.account, c.arg, c.arg2),
        (GwOp::SimFault, AccountId(9), 0x0000_0403, 3_000),
        "EVT-142.10"
    );
    assert!(rest_zero(&c) && c.px == Px(0) && c.qty == Qty(0), "EVT-142.10");
    let mut id = [0u8; 16];
    for (i, b) in id.iter_mut().enumerate() {
        *b = 0xF0 | i as u8;
    }
    let c = GwCmd::bind_wire(InstId(77), 1, &id);
    assert_eq!(
        (c.op, c.account, c.inst, c.arg, c.arg2),
        (GwOp::BindWire, AccountId(0), InstId(77), 1, 0),
        "EVT-142.11"
    );
    assert_eq!(
        (c.px, c.qty),
        (
            Px(i64::from_be_bytes([0xF0, 0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7])),
            Qty(i64::from_be_bytes([0xF8, 0xF9, 0xFA, 0xFB, 0xFC, 0xFD, 0xFE, 0xFF]))
        ),
        "EVT-142.11"
    );
    assert!(rest_zero(&c) && c.slot == SlotId(0) && c.r#gen == 0, "EVT-142.11");
    assert_eq!(c.wire_chunk(), id, "EVT-142.11");
    assert_eq!(GwCmd::bind_wire(InstId(1), 0, &[0; 16]).wire_chunk(), [0; 16], "EVT-142.12");
}

#[test]
fn evt_143_ctl_record() {
    let body: Vec<u8> = (0..130).map(|i| i as u8 + 1).collect();
    let rec = CtlRecord {
        kind: EventKind::CONTROL_CMD,
        flags: RecordFlags(0),
        body: &body,
    };
    let mut out = vec![0xAAu8; 1024];
    assert_eq!(rec.encoded_len(), 144, "EVT-143.1");
    assert_eq!(rec.encode_into(&mut out), Ok(144), "EVT-143.1");
    assert_eq!(&out[0..2], &0x0401u16.to_le_bytes());
    assert_eq!(&out[2..4], &[0, 0]);
    assert_eq!(&out[4..8], &130u32.to_le_bytes());
    assert!(out[138..144].iter().all(|&b| b == 0), "EVT-143.1 padding");
    let back = CtlRecord::read(&out[..144]).unwrap();
    assert_eq!(
        (back.kind, back.flags, back.body),
        (EventKind::CONTROL_CMD, RecordFlags(0), &body[..])
    );
    assert_eq!(
        CtlRecord {
            kind: EventKind::CONTROL_CMD,
            flags: RecordFlags(0),
            body: &[]
        }
        .encoded_len(),
        8,
        "EVT-143.2"
    );
    let p = CtlRecord {
        kind: EventKind::CONFIG_BLOB,
        flags: RecordFlags::PART,
        body: &body,
    };
    p.encode_into(&mut out).unwrap();
    assert_eq!(
        CtlRecord::read(&out).unwrap().flags,
        RecordFlags::PART,
        "EVT-143.3"
    );
    assert_eq!(
        rec.encode_into(&mut out[..143]),
        Err(EncodeErr::BufTooSmall),
        "EVT-143.4"
    );
    assert_eq!(
        CtlRecord::read(&out[..7]).unwrap_err(),
        DecodeErr::Short,
        "EVT-143.5"
    );
    rec.encode_into(&mut out).unwrap();
    let mut s = out[..144].to_vec();
    s[4..8].copy_from_slice(&200u32.to_le_bytes());
    assert_eq!(CtlRecord::read(&s).unwrap_err(), DecodeErr::Short, "EVT-143.6");
    let mut k = out[..144].to_vec();
    k[0..2].copy_from_slice(&0x0101u16.to_le_bytes());
    assert_eq!(
        CtlRecord::read(&k).unwrap_err(),
        DecodeErr::KindMismatch,
        "EVT-143.7"
    );
    for kind in [0x0403u16, 0x0404, 0x0506] {
        k[0..2].copy_from_slice(&kind.to_le_bytes());
        assert!(CtlRecord::read(&k).is_ok(), "EVT-143.8 {kind:#06x}");
    }
}
