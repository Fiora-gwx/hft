//! Record codec (EVT-003, EVT-010, EVT-012 – EVT-024, EVT-026, EVT-030, EVT-041, EVT-072).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::common;

use common::*;
use hft_event::*;
use hft_types::*;
use zerocopy::{FromBytes, IntoBytes};

#[test]
fn evt_028_exact_body_preserves_payload_zeros() {
    use zerocopy::FromZeros;
    for crc in [false, true] {
        let flags = if crc { RecordFlags::CRC } else { RecordFlags(0) };
        let od = od_basic();
        let mut wire = vec![0; record_len(size_of::<OrderDecision>(), crc) as usize];
        encode_fixed(&mut wire, 1, 2, flags, &od);
        assert_eq!(decode_record(&wire).unwrap().exact_body().unwrap(), od.as_bytes());
        let mut advice = AdviceHdr::new_zeroed();
        advice.len = 5;
        let payload = [7, 0, 2, 0, 0];
        let mut wire = vec![0; record_len(37, crc) as usize];
        encode_var(&mut wire, 3, 4, flags, &advice, &payload).unwrap();
        let record = decode_record(&wire).unwrap();
        let logical = record.exact_body().unwrap();
        assert_eq!(logical.len(), 37);
        assert_eq!(&logical[32..], &payload);
        // Construct views after transport validation to isolate alignment validation from CRC.
        let mut body = record.body.to_vec();
        body[37] = 1;
        let bad = RecordView {
            hdr: record.hdr,
            body: &body,
        };
        assert_eq!(bad.exact_body(), Err(DecodeErr::BadPadding));
        let mut hdr = record.hdr;
        hdr.len += 8;
        body.resize(body.len() + 8, 0);
        assert_eq!(
            RecordView { hdr, body: &body }.exact_body(),
            Err(DecodeErr::TooLong)
        );
        hdr.len -= 16;
        assert_eq!(
            RecordView {
                hdr,
                body: &body[..body.len() - 16]
            }
            .exact_body(),
            Err(DecodeErr::Short)
        );
        let mut hdr = record.hdr;
        hdr.kind = EventKind::STATE_SNAPSHOT.0;
        assert_eq!(
            RecordView {
                hdr,
                body: record.body
            }
            .exact_body(),
            Err(DecodeErr::BadPart)
        );
        hdr.kind = 0xffff;
        assert_eq!(
            RecordView {
                hdr,
                body: record.body
            }
            .exact_body(),
            Err(DecodeErr::UnknownKind)
        );
    }
}

fn od_basic() -> OrderDecision {
    OrderDecision {
        trigger_seq: 0,
        decided_tsc: Tsc(0),
        px: Px(100),
        qty: Qty(3),
        group: GroupId(0),
        inst: InstId(0),
        slot: SlotId(0),
        r#gen: 0,
        target_slot: SlotId(0),
        reject: 0,
        strategy: StrategyId(0),
        venue: VenueId(0),
        account: AccountId(0),
        action: OrderAction::New,
        side: Side::Buy,
        tif: Tif::Gtc,
        oflags: 0,
        reason: 0,
        intent_no: 0,
        req_px: Px(0),
        link_role: 0,
        _pad: [0; 7],
    }
}

fn canon<B: Body>(fields: &[FieldDesc]) -> B {
    B::try_read_from_bytes(&canonical(fields)[..]).unwrap()
}

#[test]
fn evt_003_unknown_flag_bits_ignored() {
    let b = bbo();
    let mut buf = [0u8; 128];
    let flags = RecordFlags(RecordFlags::EVENT_END.0 | 0xFF80);
    let n = encode_fixed(&mut buf, 7, 1000, flags, &b);
    let r = decode_record(&buf[..n as usize]).unwrap();
    assert_eq!(r.hdr.flags, 0xFFC0);
    assert_eq!(r.hdr.len, 120);
    assert_eq!(r.read::<Bbo>().unwrap(), b);
}

#[test]
fn evt_010_record_len() {
    let rows = [
        (0, false, 24),
        (1, false, 32),
        (8, false, 32),
        (80, false, 104),
        (80, true, 112),
        (84, true, 112),
        (85, true, 120),
        (0, true, 32),
        (65_512, false, 65_536),
        (65_508, true, 65_536),
    ];
    for (i, (b, c, want)) in rows.into_iter().enumerate() {
        assert_eq!(record_len(b, c), want, "EVT-010.{}", i + 1);
    }
}

#[test]
fn evt_012_encode_fixed_header_and_bounds() {
    let mut buf = [0xAAu8; 256];
    let n = encode_fixed(&mut buf, 42, 9, RecordFlags(0), &od_basic());
    assert_eq!(n, 112);
    assert_eq!(u32::from_le_bytes(buf[0..4].try_into().unwrap()), 112);
    assert_eq!(u16::from_le_bytes(buf[4..6].try_into().unwrap()), 0x0201);
    assert_eq!(&buf[6..8], &[0, 0]);
    assert_eq!(u64::from_le_bytes(buf[8..16].try_into().unwrap()), 42);
    assert_eq!(u64::from_le_bytes(buf[16..24].try_into().unwrap()), 9);
    assert!(buf[112..].iter().all(|&b| b == 0xAA));
}

#[test]
fn evt_013_crc_placement() {
    let mut buf = [0xAAu8; 256];
    let n = encode_fixed(&mut buf, 1, 2, RecordFlags::CRC | RecordFlags::EVENT_END, &bbo()) as usize;
    assert_eq!(n, 128);
    assert_eq!(u32::from_le_bytes(buf[0..4].try_into().unwrap()), 128);
    assert_eq!(&buf[120..124], &[0; 4]);
    let c = crc32c::crc32c(&buf[..n - 4]);
    assert_eq!(u32::from_le_bytes(buf[124..128].try_into().unwrap()), c);
    assert!(buf[128..].iter().all(|&b| b == 0xAA));
    assert_eq!(decode_record(&buf[..128]).unwrap().body.len(), 100);
}

#[cfg(debug_assertions)]
#[test]
#[should_panic]
fn evt_014_encode_fixed_short_buffer_asserts() {
    let mut buf = [0xAAu8; 130];
    let b = bbo();
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        encode_fixed(&mut buf[..119], 1, 1, RecordFlags(0), &b);
    }));
    assert!(buf[119..].iter().all(|&x| x == 0xAA), "wrote outside [0, 119)");
    std::panic::resume_unwind(r.expect_err("no debug_assert"));
}

fn timer_rec() -> Vec<u8> {
    enc(&timer(), 1, 2, RecordFlags(0))
}

#[test]
fn evt_015_decode_record_bounds() {
    let r = timer_rec();
    assert_eq!(r.len(), 48);
    assert_eq!(decode_record(&r).unwrap().body.len(), 24, "EVT-015.1");
    assert_eq!(
        decode_record(&r[..23]).unwrap_err(),
        DecodeErr::Short,
        "EVT-015.2"
    );
    assert_eq!(
        decode_record(&with_len(r.clone(), 0)).unwrap_err(),
        DecodeErr::Short,
        "EVT-015.3"
    );
    assert_eq!(
        decode_record(&with_len(r.clone(), 16)).unwrap_err(),
        DecodeErr::Short,
        "EVT-015.4"
    );
    assert_eq!(
        decode_record(&with_len(r.clone(), 56)).unwrap_err(),
        DecodeErr::Short,
        "EVT-015.5"
    );
    assert_eq!(
        decode_record(&with_len(r.clone(), 44)).unwrap_err(),
        DecodeErr::Misaligned,
        "EVT-015.6"
    );
    let mut big = vec![0u8; 65_544];
    big[..48].copy_from_slice(&r);
    assert_eq!(
        decode_record(&with_len(big, 65_544)).unwrap_err(),
        DecodeErr::TooLong,
        "EVT-015.7"
    );
    let max = raw_record(0x010A, 0, 1, 1, &vec![0u8; 65_512]);
    assert_eq!(max.len(), 65_536);
    assert_eq!(decode_record(&max).unwrap().body.len(), 65_512, "EVT-015.8");
    let crc24 = raw_record(EventKind::TIMER.0, RecordFlags::CRC.0, 1, 1, &[]);
    let crc24 = with_len(crc24[..24].to_vec(), 24);
    assert_eq!(decode_record(&crc24).unwrap_err(), DecodeErr::Short, "EVT-015.9");
    let empty = raw_record(EventKind::TIMER.0, 0, 1, 1, &[]);
    assert_eq!(decode_record(&empty).unwrap().body.len(), 0, "EVT-015.10");
}

fn crc_bbo() -> Vec<u8> {
    enc(&bbo(), 1, 2, RecordFlags::CRC | RecordFlags::EVENT_END)
}

#[test]
fn evt_016_crc_check() {
    let r = crc_bbo();
    assert_eq!(r.len(), 128);
    assert!(decode_record(&r).is_ok(), "EVT-016.1");
    let flip = |o: usize, m: u8| {
        let mut x = r.clone();
        x[o] ^= m;
        decode_record(&x).map(|_| ())
    };
    assert_eq!(flip(24 + 30, 1), Err(DecodeErr::BadCrc), "EVT-016.2");
    assert_eq!(flip(8, 1), Err(DecodeErr::BadCrc), "EVT-016.3");
    assert_eq!(flip(125, 0x10), Err(DecodeErr::BadCrc), "EVT-016.4");
    let mut pad = r.clone();
    pad[120..124].fill(1);
    assert_eq!(
        decode_record(&pad).map(|_| ()),
        Err(DecodeErr::BadCrc),
        "EVT-016.5"
    );
    let mut plain = enc(&bbo(), 1, 2, RecordFlags(0));
    let n = plain.len();
    plain[n - 4..].copy_from_slice(&[1, 2, 3, 4]);
    assert!(decode_record(&plain).is_ok(), "EVT-016.6");
}

#[test]
fn evt_017_unknown_kind_skippable() {
    let mut buf = timer_rec();
    buf.extend(raw_record(0x010A, 0, 2, 2, &[0x5A; 16]));
    buf.extend(enc(&bbo(), 3, 3, RecordFlags(0)));
    let mut off = 0;
    let mut seen = Vec::new();
    while off < buf.len() {
        let r = decode_record(&buf[off..]).unwrap();
        match kind_info(EventKind(r.hdr.kind)) {
            None => seen.push(("unknown", off)),
            Some(_) if r.hdr.kind == EventKind::BBO.0 => {
                assert_eq!(r.read::<Bbo>().unwrap(), bbo());
                seen.push(("bbo", off));
            }
            Some(_) => seen.push(("known", off)),
        }
        off += r.hdr.len as usize;
    }
    assert_eq!(seen, vec![("known", 0), ("unknown", 48), ("bbo", 88)]);
    assert_eq!(off, 208);
}

#[test]
fn evt_018_kind_mismatch() {
    let t: Trade = canon(Trade::FIELDS);
    let tr = Aligned::new(&enc(&t, 1, 1, RecordFlags(0)));
    let r = decode_record(tr.bytes()).unwrap();
    assert_eq!(r.read::<Bbo>().unwrap_err(), DecodeErr::KindMismatch);
    assert_eq!(r.view::<Bbo>().unwrap_err(), DecodeErr::KindMismatch);
    let br = Aligned::new(&enc(&bbo(), 1, 1, RecordFlags(0)));
    let r = decode_record(br.bytes()).unwrap();
    assert_eq!(r.var::<AdviceHdr>().unwrap_err(), DecodeErr::KindMismatch);
}

#[test]
fn evt_019_read_short_long_bodies() {
    let full = bbo().as_bytes().to_vec();
    let r = raw_record(EventKind::BBO.0, 0, 1, 1, &full[..88]);
    let b = decode_record(&r).unwrap().read::<Bbo>().unwrap();
    assert_eq!(b.as_bytes()[..88], full[..88], "EVT-019.1");
    assert_eq!(b.update_id, 0, "EVT-019.1");
    let mut long = full.clone();
    long.extend([0xFF; 8]);
    let a = Aligned::new(&raw_record(EventKind::BBO.0, 0, 1, 1, &long));
    let r = decode_record(a.bytes()).unwrap();
    assert_eq!(r.read::<Bbo>().unwrap(), bbo(), "EVT-019.2");
    assert_eq!(r.view::<Bbo>().unwrap_err(), DecodeErr::TooLong, "EVT-019.2");
    let r = raw_record(EventKind::BBO.0, 0, 1, 1, &[]);
    let b = decode_record(&r).unwrap().read::<Bbo>().unwrap();
    assert!(b.as_bytes().iter().all(|&x| x == 0), "EVT-019.3");
    let od = canonical(OrderDecision::FIELDS);
    let r = raw_record(EventKind::ORDER_DECISION.0, 0, 1, 1, &od[..56]);
    assert_eq!(
        decode_record(&r).unwrap().read::<OrderDecision>().unwrap_err(),
        DecodeErr::InvalidEnum,
        "EVT-019.4"
    );
    let r = raw_record(EventKind::TIMER.0, 0, 1, 1, &timer().as_bytes()[..16]);
    assert_eq!(
        decode_record(&r).unwrap().read::<Timer>().unwrap().deadline_tsc,
        0,
        "EVT-019.5"
    );
}

#[test]
fn evt_020_view_exact_len() {
    let a = Aligned::new(&enc(&bbo(), 1, 1, RecordFlags(0)));
    let r = decode_record(a.bytes()).unwrap();
    let v = r.view::<Bbo>().unwrap();
    assert_eq!(
        v as *const Bbo as *const u8,
        a.bytes()[24..].as_ptr(),
        "EVT-020.1"
    );
    let full = bbo().as_bytes().to_vec();
    let a = Aligned::new(&raw_record(EventKind::BBO.0, 0, 1, 1, &full[..88]));
    assert_eq!(
        decode_record(a.bytes()).unwrap().view::<Bbo>().unwrap_err(),
        DecodeErr::Short,
        "EVT-020.2"
    );
    let mut long = full.clone();
    long.extend([0; 8]);
    let a = Aligned::new(&raw_record(EventKind::BBO.0, 0, 1, 1, &long));
    assert_eq!(
        decode_record(a.bytes()).unwrap().view::<Bbo>().unwrap_err(),
        DecodeErr::TooLong,
        "EVT-020.3"
    );
    let a = Aligned::new(&enc(&bbo(), 1, 1, RecordFlags::CRC));
    assert_eq!(a.bytes().len(), 128);
    assert_eq!(
        *decode_record(a.bytes()).unwrap().view::<Bbo>().unwrap(),
        bbo(),
        "EVT-020.4"
    );
    let ef = canonical(ExternalFill::FIELDS);
    let a = Aligned::new(&raw_record(EventKind::EXTERNAL_FILL.0, 0, 1, 1, &ef[..72]));
    assert_eq!(a.bytes().len(), 96);
    let r = decode_record(a.bytes()).unwrap();
    assert_eq!(
        r.view::<ExternalFill>().unwrap_err(),
        DecodeErr::Short,
        "EVT-020.5"
    );
    assert_eq!(r.read::<ExternalFill>().unwrap().fill_seq, 0, "EVT-020.5");
}

#[test]
fn evt_021_view_misaligned() {
    let rec = enc(&bbo(), 1, 1, RecordFlags(0));
    let mut store = [0u64; 20];
    let bytes: &mut [u8] = store.as_mut_bytes();
    bytes[4..4 + rec.len()].copy_from_slice(&rec);
    let r = decode_record(&bytes[4..]).unwrap();
    assert_eq!(r.view::<Bbo>().unwrap_err(), DecodeErr::Misaligned);
    assert_eq!(r.read::<Bbo>().unwrap(), bbo());
}

/// (read, view) results after setting `body[off] = v` in the canonical record of `B`.
fn patched<B: Body + core::fmt::Debug + PartialEq>(
    off: usize,
    v: u8,
) -> (Result<(), DecodeErr>, Result<(), DecodeErr>) {
    let fields = kind_info(B::KIND).unwrap().fields;
    let mut body = canonical(fields);
    body[off] = v;
    let a = Aligned::new(&raw_record(B::KIND.0, 0, 1, 1, &body));
    let r = decode_record(a.bytes()).unwrap();
    (r.read::<B>().map(|_| ()), r.view::<B>().map(|_| ()))
}

fn bad<B: Body + core::fmt::Debug + PartialEq>(row: &str, off: usize, vals: &[u8], ok: Option<u8>) {
    for &v in vals {
        let (a, b) = patched::<B>(off, v);
        assert_eq!(a, Err(DecodeErr::InvalidEnum), "{row} read {v}");
        assert_eq!(b, Err(DecodeErr::InvalidEnum), "{row} view {v}");
    }
    if let Some(v) = ok {
        assert_eq!(patched::<B>(off, v), (Ok(()), Ok(())), "{row} ok {v}");
    }
}

#[test]
fn evt_022_invalid_enums() {
    bad::<OrderDecision>("EVT-022.1", 59, &[0, 6], Some(5));
    bad::<OrderDecision>("EVT-022.2", 60, &[0, 3], Some(2));
    bad::<OrderDecision>("EVT-022.3", 61, &[0, 5], Some(4));
    bad::<SendReport>("EVT-022.4", 35, &[0, 9], Some(8));
    bad::<SendReport>("EVT-022.5", 36, &[0, 18], Some(17));
    bad::<OrderEvent>("EVT-022.6", 2, &[0, 13], Some(12));
    bad::<ExternalFill>("EVT-022.7", 2, &[0], None);
    bad::<ExternalFill>("EVT-022.7", 3, &[0, 6], Some(5));
    bad::<L2Delta>("EVT-022.8", 88, &[0], None);
    bad::<Trade>("EVT-022.8", 80, &[3], None);
    bad::<Liquidation>("EVT-022.8", 72, &[0], None);
    bad::<Note>("EVT-022.9", 4, &[0, 5], Some(4));
    bad::<Note>("EVT-022.10", 5, &[0, 13], Some(12));
    bad::<ConnState>("EVT-022.11", 2, &[0, 7], Some(6));
    bad::<ConnState>("EVT-022.12", 3, &[0, 5], Some(4));
    bad::<TxLifecycle>("EVT-022.13", 8, &[0, 7], Some(6));
    bad::<SwapDecision>("EVT-022.14", 107, &[0], None);
}

fn var_rec(kind: EventKind, prefix: usize, counts: &[(usize, u16)], body_len: usize) -> Aligned {
    let mut body = vec![0u8; body_len.max(prefix)];
    for &(o, v) in counts {
        body[o..o + 2].copy_from_slice(&v.to_le_bytes());
    }
    body.truncate(body_len);
    Aligned::new(&raw_record(kind.0, 0, 1, 1, &body))
}

#[test]
fn evt_023_var_tail_length() {
    let adv = |len: u16, body: usize| var_rec(EventKind::ADVICE, 32, &[(8, len)], body);
    let a = adv(0, 32);
    assert_eq!(
        decode_record(a.bytes())
            .unwrap()
            .var::<AdviceHdr>()
            .unwrap()
            .1
            .len(),
        0,
        "EVT-023.1"
    );
    let mut body = vec![0u8; 40];
    body[8] = 5;
    body[32..37].copy_from_slice(&[1, 2, 3, 4, 5]);
    body[37..40].fill(0x77);
    let a = Aligned::new(&raw_record(EventKind::ADVICE.0, 0, 1, 1, &body));
    assert_eq!(
        decode_record(a.bytes()).unwrap().var::<AdviceHdr>().unwrap().1,
        &[1, 2, 3, 4, 5],
        "EVT-023.2"
    );
    let a = adv(5, 32);
    assert_eq!(
        decode_record(a.bytes()).unwrap().var::<AdviceHdr>().unwrap_err(),
        DecodeErr::Short,
        "EVT-023.3"
    );
    let a = adv(5, 48);
    assert_eq!(
        decode_record(a.bytes()).unwrap().var::<AdviceHdr>().unwrap_err(),
        DecodeErr::TooLong,
        "EVT-023.4"
    );
    let m = var_rec(EventKind::MARGIN_REPORT, 64, &[(4, 3)], 112);
    let (_, t) = decode_record(m.bytes())
        .unwrap()
        .var::<MarginReportHdr>()
        .unwrap();
    assert_eq!(t.len(), 48, "EVT-023.5");
    assert_eq!(<[MarginItem]>::ref_from_bytes(t).unwrap().len(), 3, "EVT-023.5");
    let m = var_rec(EventKind::MARGIN_REPORT, 64, &[(4, 3)], 96);
    assert_eq!(
        decode_record(m.bytes())
            .unwrap()
            .var::<MarginReportHdr>()
            .unwrap_err(),
        DecodeErr::Short,
        "EVT-023.6"
    );
    let c = var_rec(EventKind::ACCOUNT_CONFIG_REPORT, 64, &[], 64);
    let (_, t) = decode_record(c.bytes())
        .unwrap()
        .var::<AccountConfigReportHdr>()
        .unwrap();
    assert!(t.is_empty(), "EVT-023.7");
    let c = var_rec(EventKind::ACCOUNT_CONFIG_REPORT, 64, &[(6, 2)], 104);
    assert_eq!(
        decode_record(c.bytes())
            .unwrap()
            .var::<AccountConfigReportHdr>()
            .unwrap_err(),
        DecodeErr::TooLong,
        "EVT-023.8"
    );
    let l = var_rec(EventKind::L2_BATCH, 80, &[(72, 2), (74, 3)], 160);
    assert_eq!(
        decode_record(l.bytes())
            .unwrap()
            .var::<L2BatchHdr>()
            .unwrap()
            .1
            .len(),
        80,
        "EVT-023.9"
    );
    let l = var_rec(EventKind::L2_BATCH, 80, &[(72, 2), (74, 3)], 144);
    assert_eq!(
        decode_record(l.bytes()).unwrap().var::<L2BatchHdr>().unwrap_err(),
        DecodeErr::Short,
        "EVT-023.10"
    );
    let m = var_rec(EventKind::MARGIN_REPORT, 64, &[], 56);
    assert_eq!(
        decode_record(m.bytes())
            .unwrap()
            .var::<MarginReportHdr>()
            .unwrap_err(),
        DecodeErr::Short,
        "EVT-023.11"
    );
}

fn advice_hdr(len: u16) -> AdviceHdr {
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

fn margin_hdr(n: u16) -> MarginReportHdr {
    let mut h = MarginReportHdr::new_zeroed();
    h.n = n;
    h
}

fn acr_hdr(n: u16) -> AccountConfigReportHdr {
    let mut h = AccountConfigReportHdr::new_zeroed();
    h.n_lev = n;
    h
}

fn batch_hdr(bids: u16, asks: u16) -> L2BatchHdr {
    let mut h = L2BatchHdr::new_zeroed();
    h.n_bids = bids;
    h.n_asks = asks;
    h
}

use zerocopy::FromZeros;

#[test]
fn evt_024_encode_var_errors() {
    let mut out = vec![0u8; 70_000];
    let f = RecordFlags(0);
    assert_eq!(
        encode_var(&mut out, 1, 1, f, &advice_hdr(10), &[1; 10]),
        Ok(72),
        "EVT-024.1"
    );
    assert_eq!(
        encode_var(&mut out, 1, 1, f, &advice_hdr(10), &[1; 11]),
        Err(EncodeErr::Count),
        "EVT-024.2"
    );
    assert_eq!(
        encode_var(&mut out, 1, 1, f, &advice_hdr(4097), &[1; 4097]),
        Err(EncodeErr::TooLong),
        "EVT-024.3"
    );
    assert_eq!(
        encode_var(&mut out[..71], 1, 1, f, &advice_hdr(10), &[1; 10]),
        Err(EncodeErr::BufTooSmall),
        "EVT-024.4"
    );
    assert_eq!(
        encode_var(&mut out, 1, 1, f, &margin_hdr(2), &[0; 16]),
        Err(EncodeErr::Count),
        "EVT-024.5"
    );
    assert_eq!(
        encode_var(&mut out, 1, 1, f, &batch_hdr(1500, 2500), &vec![0; 64_000]),
        Ok(64_104),
        "EVT-024.6"
    );
}

#[test]
fn evt_026_var_max_bounds() {
    let mut out = vec![0u8; 70_000];
    let f = RecordFlags(0);
    let ok = |r: Result<u32, EncodeErr>| r.map(|_| ());
    for (n, want) in [
        (4095usize, Ok(())),
        (4096, Ok(())),
        (4097, Err(EncodeErr::TooLong)),
    ] {
        let r = encode_var(&mut out, 1, 1, f, &advice_hdr(n as u16), &vec![3; n]);
        assert_eq!(ok(r), want, "EVT-026.1 {n}");
    }
    for (n, want) in [(1023u16, Ok(())), (1024, Ok(())), (1025, Err(EncodeErr::TooLong))] {
        let r = encode_var(&mut out, 1, 1, f, &margin_hdr(n), &vec![0; 16 * n as usize]);
        assert_eq!(ok(r), want, "EVT-026.2 {n}");
        let r = encode_var(&mut out, 1, 1, f, &acr_hdr(n), &vec![0; 16 * n as usize]);
        assert_eq!(ok(r), want, "EVT-026.3 {n}");
    }
    for (n, want) in [(3999u16, Ok(())), (4000, Ok(())), (4001, Err(EncodeErr::TooLong))] {
        let r = encode_var(
            &mut out,
            1,
            1,
            f,
            &batch_hdr(n / 2, n - n / 2),
            &vec![0; 16 * n as usize],
        );
        assert_eq!(ok(r), want, "EVT-026.4 {n}");
    }
}

macro_rules! fixed_samples {
    ($($t:ty),*) => {
        fn encode_sample(i: usize, out: &mut Vec<u8>) {
            let types = [$(stringify!($t)),*].len();
            let mut k = 0;
            $(
                if i % types == k {
                    let b: $t = canon(<$t>::FIELDS);
                    let mut buf = [0u8; 256];
                    let n = encode_fixed(&mut buf, i as u64, 7 * i as u64, RecordFlags(0), &b) as usize;
                    out.extend_from_slice(&buf[..n]);
                }
                k += 1;
            )*
            let _ = k;
        }
    };
}
fixed_samples!(
    Bbo,
    L2Delta,
    Trade,
    Funding,
    MarkPrice,
    IndexPrice,
    Liquidation,
    SendReport,
    OrderEvent,
    PositionReport,
    BalanceReport,
    FundingSettle,
    ExternalFill,
    ExecDecision,
    ChainHead,
    PoolQuote,
    SwapDecision,
    TxLifecycle,
    CmdResult,
    ReconcileReport,
    Timer,
    ConnState,
    Note,
    HostProfile,
    ClockStatus,
    SegmentRoll,
    EngineClock
);

fn encode_all() -> Vec<u8> {
    let mut out = Vec::new();
    for i in 0..1000 {
        encode_sample(i, &mut out);
    }
    out
}

#[test]
fn evt_030_stateless_concurrent_encode() {
    let s = encode_all();
    let base = KINDS.as_ptr() as usize;
    let hs: Vec<_> = (0..4)
        .map(|_| {
            std::thread::spawn(move || {
                let mut ok = true;
                let mut ptr = 0;
                for _ in 0..10 {
                    ok &= encode_all() == encode_all();
                    ptr = KINDS.as_ptr() as usize;
                }
                (encode_all(), ok, ptr)
            })
        })
        .collect();
    for h in hs {
        let (x, ok, ptr) = h.join().unwrap();
        assert!(ok);
        assert_eq!(x, s);
        assert_eq!(ptr, base);
    }
}

#[test]
fn evt_041_market_hdr_rx_fields() {
    let check = |row: &str, set: &dyn Fn(&mut Bbo), range: core::ops::Range<usize>, want: &[u8]| {
        let mut b = bbo();
        set(&mut b);
        let rec = enc(&b, 1, 1, RecordFlags(0));
        assert_eq!(&rec[24 + range.start..24 + range.end], want, "{row}");
        assert_eq!(decode_record(&rec).unwrap().read::<Bbo>().unwrap(), b, "{row}");
    };
    check("EVT-041.1", &|b| b.h.rx_empty_gap_ns = 0, 48..52, &[0; 4]);
    check("EVT-041.2", &|b| b.h.rx_empty_gap_ns = 1, 48..52, &[1, 0, 0, 0]);
    check(
        "EVT-041.3",
        &|b| b.h.rx_empty_gap_ns = u32::MAX,
        48..52,
        &[0xFF; 4],
    );
    check("EVT-041.4", &|b| b.h.sw_rx_ns = 0, 40..48, &[0; 8]);
    let v: i64 = 1_759_000_000_123_456_789;
    check("EVT-041.5", &|b| b.h.sw_rx_ns = v, 40..48, &v.to_le_bytes());
    let h = bbo().h;
    let heads: Vec<Vec<u8>> = vec![
        enc(&bbo(), 1, 1, RecordFlags(0)),
        enc(
            &L2Delta {
                h,
                ..canon(L2Delta::FIELDS)
            },
            1,
            1,
            RecordFlags(0),
        ),
        enc(
            &Trade {
                h,
                ..canon(Trade::FIELDS)
            },
            1,
            1,
            RecordFlags(0),
        ),
        enc(
            &Funding {
                h,
                ..canon(Funding::FIELDS)
            },
            1,
            1,
            RecordFlags(0),
        ),
        enc(
            &MarkPrice {
                h,
                ..canon(MarkPrice::FIELDS)
            },
            1,
            1,
            RecordFlags(0),
        ),
        enc(
            &IndexPrice {
                h,
                ..canon(IndexPrice::FIELDS)
            },
            1,
            1,
            RecordFlags(0),
        ),
        enc(
            &Liquidation {
                h,
                ..canon(Liquidation::FIELDS)
            },
            1,
            1,
            RecordFlags(0),
        ),
    ];
    let mut all: Vec<Vec<u8>> = heads.iter().map(|r| r[24..80].to_vec()).collect();
    all.push(
        L2SnapHdr {
            h,
            ..canon(L2SnapHdr::FIELDS)
        }
        .as_bytes()[..56]
            .to_vec(),
    );
    all.push(
        L2BatchHdr {
            h,
            ..canon(L2BatchHdr::FIELDS)
        }
        .as_bytes()[..56]
            .to_vec(),
    );
    for x in &all {
        assert_eq!(x, &all[0], "EVT-041.6");
        assert_eq!(&x[52..56], &[0; 4], "EVT-041.6");
    }
}

#[test]
fn evt_072_latency_action_range() {
    let mut s: LatencySample = canon(LatencySample::FIELDS);
    for a in 0..=2u8 {
        s.action = a;
        let rec = enc(&s, 1, 1, RecordFlags(0));
        assert_eq!(
            decode_record(&rec)
                .unwrap()
                .read::<LatencySample>()
                .unwrap()
                .action,
            a,
            "EVT-072 {a}"
        );
    }
    if cfg!(debug_assertions) {
        for a in [3u8, 255] {
            s.action = a;
            let r = std::panic::catch_unwind(|| enc(&s, 1, 1, RecordFlags(0)));
            assert!(r.is_err(), "EVT-072 {a} must debug_assert");
        }
    }
}
