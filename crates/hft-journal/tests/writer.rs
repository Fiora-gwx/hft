//! Writer: seq assignment, record bytes, publish, event boundaries, Full/TooLarge, multi-part,
//! reserve_var, Sealed/InBoot, snapshots, raw and CRC policy (JNL-013 – JNL-027, JNL-038, JNL-104,
//! JNL-121).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod common;

use common::harness::*;
use hft_event::{EventKind, PartHdr, RawHdr, RecordFlags, SnapshotHdr};
use hft_journal::*;
use hft_types::{Seq, Tsc};
use zerocopy::{FromBytes, FromZeros, IntoBytes};

#[test]
fn jnl_013_seq_is_assigned_by_append() {
    let mut h = H::open0(mem_c0());
    let w = h.w();
    let a = w.append(F0, Tsc(1000), &tin(1)).unwrap();
    let b = w.append(F0, Tsc(1000), &tout(2)).unwrap();
    let c = w.append(F0, Tsc(1001), &tmeta(3)).unwrap();
    assert_eq!((a, b, c), (Seq(3), Seq(4), Seq(5)));
    assert_eq!(w.next_seq(), Seq(6));
    let img = h.seg(1);
    let hs: Vec<(u64, u64)> = [4240, 4344, 4424]
        .iter()
        .map(|o| (rh(&img, *o).seq, rh(&img, *o).tsc))
        .collect();
    assert_eq!(hs, vec![(3, 1000), (4, 1000), (5, 1001)]);
}

#[test]
fn jnl_014_record_bytes() {
    let mut h = H::open0(mem_c0());
    let w = h.w();
    w.append(F0, Tsc(1), &tin(0x11)).unwrap();
    w.append(F0, Tsc(1), &tcrc(0x22)).unwrap();
    w.end_event();
    w.publish();
    let b = h.seg(1);
    let a = rh(&b, 4240);
    assert_eq!((a.len, a.kind, a.flags, a.seq), (104, 0x7F01, 0, 3));
    assert!(b[4264..4344].iter().all(|x| *x == 0x11));
    let c = rh(&b, 4344);
    assert_eq!((c.len, c.kind, c.flags, c.seq), (72, 0x7F04, CRC | EVENT_END, 4));
    assert!(b[4368..4408].iter().all(|x| *x == 0x22));
    assert!(b[4408..4412].iter().all(|x| *x == 0));
    assert_eq!(u32_at(&b, 4412), crc32c::crc32c(&b[4344..4412]));
    assert!(b[4416..4416 + 4096].iter().all(|x| *x == 0));
}

#[test]
fn jnl_015_publish_moves_committed() {
    let mut h = H::open0(mem_c0());
    let mut r = h.reader(StartAt::Tail);
    h.w().append(F0, Tsc(1), &tin(1)).unwrap();
    h.w().append(F0, Tsc(1), &tout(1)).unwrap();
    assert!(matches!(r.poll(), Next::Pending));
    assert_eq!((h.committed(1), h.last_seq(1)), (4240, 2));
    h.w().end_event();
    h.w().publish();
    assert_eq!((h.committed(1), h.last_seq(1)), (4424, 4));
    let s: Vec<u64> = (0..2)
        .map(|_| match r.poll() {
            Next::Record(v) => v.hdr.seq,
            o => panic!("{o:?}"),
        })
        .collect();
    assert_eq!(s, vec![3, 4]);
}

#[test]
fn jnl_016_publish_midevent_stops_at_boundary() {
    let mut h = H::open0(mem_c0());
    h.events(1);
    assert_eq!(h.committed(1), 4424);
    h.w().append(F0, Tsc(1), &tin(1)).unwrap();
    h.w().publish();
    assert_eq!((h.committed(1), h.last_seq(1)), (4424, 4));
    h.w().append(F0, Tsc(1), &tout(1)).unwrap();
    h.w().end_event();
    h.w().publish();
    assert_eq!((h.committed(1), h.last_seq(1)), (4608, 6));
    let b = h.seg(1);
    assert_eq!(rh(&b, 4424).flags & EVENT_END, 0);
    assert_eq!(rh(&b, 4528).flags & EVENT_END, EVENT_END);
}

enum A {
    In,
    Out,
    M,
    Multi3,
}

fn run(h: &mut H, seq: &[A]) -> Vec<usize> {
    let mut offs = Vec::new();
    for a in seq {
        offs.push(h.w().offset() as usize);
        let w = h.w();
        match a {
            A::In => {
                w.append(F0, Tsc(1), &tin(1)).unwrap();
            }
            A::Out => {
                w.append(F0, Tsc(1), &tout(1)).unwrap();
            }
            A::M => {
                w.append_meta(Tsc(1), &tmeta(1)).unwrap();
            }
            A::Multi3 => {
                let body = multi_body(2 * 65_488 + 1);
                w.append_var(EventKind::T_MULTI, F0, Tsc(1), &[&body]).unwrap();
            }
        }
    }
    h.w().end_event();
    offs
}

fn ends(h: &H, from: usize) -> Vec<bool> {
    let b = h.seg(1);
    let end = b.iter().rposition(|x| *x != 0).map_or(from, |p| p + 1).max(from);
    records(&b, end)
        .into_iter()
        .filter(|(o, _)| *o >= from)
        .map(|(_, r)| r.flags & EVENT_END != 0)
        .collect()
}

#[test]
fn jnl_017_end_event_placement() {
    use A::*;
    for (row, s, want) in [
        ("JNL-017.1", vec![In, Out, Out], vec![false, false, true]),
        ("JNL-017.2", vec![In, Out, M], vec![false, true, false]),
        ("JNL-017.3", vec![In, M, Out], vec![false, false, true]),
        ("JNL-017.4", vec![In], vec![true]),
        ("JNL-017.5", vec![In, Multi3], vec![false, false, false, true]),
    ] {
        let mut h = H::open0(mem_c0());
        let from = h.w().offset() as usize;
        run(&mut h, &s);
        assert_eq!(ends(&h, from), want, "{row}");
        if row == "JNL-017.5" {
            let b = h.seg(1);
            let parts: Vec<u16> = records(&b, b.len())
                .iter()
                .skip(3)
                .map(|(_, r)| r.flags & PART)
                .collect();
            assert_eq!(parts, vec![PART, PART, 0], "{row}");
        }
    }
    let mut h = H::open0(mem_c0());
    let before = h.seg(1);
    h.w().end_event();
    assert_eq!(h.seg(1), before, "JNL-017.6");
    run(&mut h, &[In, Out, Out]);
    let once = h.seg(1);
    h.w().end_event();
    assert_eq!(h.seg(1), once, "JNL-017.7");
}

#[test]
fn jnl_018_meta_event_end() {
    let mut h = H::open0(mem_c0());
    let o = h.w().offset() as usize;
    h.w().append_meta(Tsc(1), &tmeta(1)).unwrap();
    h.w().publish();
    assert_eq!(rh(&h.seg(1), o).flags & EVENT_END, EVENT_END, "JNL-018.1");
    assert_eq!(h.committed(1), o as u64 + 56, "JNL-018.1");
    let o2 = h.w().offset() as usize;
    h.w().append(F0, Tsc(1), &tin(1)).unwrap();
    h.w().end_event();
    let b = h.seg(1);
    assert_eq!(
        (rh(&b, o).flags & EVENT_END, rh(&b, o2).flags & EVENT_END),
        (EVENT_END, EVENT_END),
        "JNL-018.3"
    );

    let mut h = H::open0(mem_c0());
    let a = h.w().offset() as usize;
    h.w().append(F0, Tsc(1), &tin(1)).unwrap();
    let m = h.w().offset() as usize;
    h.w().append_meta(Tsc(1), &tmeta(1)).unwrap();
    assert_eq!(rh(&h.seg(1), m).flags & EVENT_END, 0, "JNL-018.2");
    h.w().end_event();
    let b = h.seg(1);
    assert_eq!(
        (rh(&b, a).flags & EVENT_END, rh(&b, m).flags & EVENT_END),
        (EVENT_END, 0),
        "JNL-018.2"
    );
}

#[test]
fn jnl_019_raw_auto_event_end() {
    let mut h = H::open0(r0(&mem_hot(), &cold_dir()));
    h.w()
        .append_var(EventKind::RAW_RX, F0, Tsc(1), &[&raw_body(512, 0, 1)])
        .unwrap();
    h.w()
        .append_var(EventKind::RAW_RX, F0, Tsc(1), &[&raw_body(70_000, 0, 2)])
        .unwrap();
    h.w().publish();
    let b = h.seg(1);
    let rs = records(&b, h.committed(1) as usize);
    let fl: Vec<u16> = rs.iter().map(|(_, r)| r.flags).collect();
    assert_eq!(
        fl,
        vec![EVENT_END | CRC, EVENT_END | CRC, PART | CRC, EVENT_END | CRC]
    );
    assert_eq!(rs[0].1.len, 80);
    h.w().end_event();
    assert_eq!(h.seg(1), b);
}

#[test]
fn jnl_020_full_boundary() {
    for (row, off, meta, ok) in [
        ("JNL-020.1", 1_048_184usize, false, true),
        ("JNL-020.2", 1_048_192, false, false),
        ("JNL-020.3", 1_048_192, true, true),
        ("JNL-020.4", 1_048_288, true, false),
    ] {
        let mut h = H::open0(mem_c0());
        assert_eq!(h.w().reserve_bytes(), 288);
        h.push_to(off);
        let seq = h.w().next_seq();
        let r = if meta {
            h.w().append_meta(Tsc(1), &tmeta(1))
        } else {
            h.w().append(F0, Tsc(1), &tin(1))
        };
        if ok {
            assert!(r.is_ok(), "{row}");
            if row == "JNL-020.1" {
                assert_eq!(h.w().offset(), 1_048_288, "{row}");
            }
        } else {
            assert_eq!(r, Err(AppendErr::Full), "{row}");
            assert_eq!(h.w().next_seq(), seq, "{row}");
            assert!(h.seg(1)[off..].iter().all(|x| *x == 0), "{row}");
        }
    }
    // Rows 5–6: inside the boot block the reserve is usable.
    let mut h = H::open0(mem_c0());
    let mut b = h.roll_boot(RollReason::Size);
    let push = |b: &mut BootWriter<'_>, cur: &mut usize, target: usize| {
        while *cur < target {
            let d = target - *cur;
            let mut rec = d.min(65_040);
            if d - rec != 0 && d - rec < 40 {
                rec -= 40;
            }
            b.append_var(EventKind::T_VAR, Tsc(1), &[&tvar(rec - 40, 0)])
                .unwrap();
            *cur += rec;
        }
    };
    let mut cur = 4096 + 72;
    push(&mut b, &mut cur, 1_048_192);
    assert!(b.append(Tsc(1), &tin(1)).is_ok(), "JNL-020.5");
    cur += 104;
    push(&mut b, &mut cur, 1_048_512);
    assert_eq!(b.append(Tsc(1), &tin(1)), Err(AppendErr::Full), "JNL-020.6");
}

#[test]
fn jnl_021_too_large() {
    let mut h = H::open0(mem_c0());
    let o = h.w().offset();
    assert!(
        h.w()
            .append_var(EventKind::T_VAR, F0, Tsc(1), &[&tvar(65_000, 1)])
            .is_ok(),
        "JNL-021.1"
    );
    assert_eq!(h.w().offset() - o, 65_040, "JNL-021.1");
    let o = h.w().offset();
    assert_eq!(
        h.w()
            .append_var(EventKind::T_VAR, F0, Tsc(1), &[&tvar(65_001, 1)]),
        Err(AppendErr::TooLarge),
        "JNL-021.2"
    );
    assert_eq!(h.w().offset(), o, "JNL-021.2");
    let mut h = H::open0(mem_c0());
    assert!(
        h.w()
            .append_var(EventKind::T_MULTI, F0, Tsc(1), &[&multi_body(15 * 65_488)])
            .is_ok(),
        "JNL-021.3"
    );
    let mut h = H::open0(mem_c0());
    assert_eq!(
        h.w()
            .append_var(EventKind::T_MULTI, F0, Tsc(1), &[&multi_body(16 * 65_488)]),
        Err(AppendErr::TooLarge),
        "JNL-021.4"
    );
    let o = h.w().offset();
    assert!(
        matches!(
            h.w().reserve_var(EventKind::T_MULTI, F0, Tsc(1), u32::MAX),
            Err(AppendErr::TooLarge)
        ),
        "JNL-021.5"
    );
    assert_eq!(h.w().offset(), o, "JNL-021.5");
}

#[test]
fn jnl_022_multipart_split() {
    for (row, l, lens) in [
        ("JNL-022.1", 65_488usize, vec![65_528u32]),
        ("JNL-022.2", 65_489, vec![65_528, 48]),
        ("JNL-022.3", 130_977, vec![65_528, 65_528, 48]),
        ("JNL-022.4", 0, vec![40]),
    ] {
        let mut h = H::open0(mem_c0());
        let body = multi_body(l);
        let (a, rest) = body.split_at(l / 3);
        let (b2, c) = rest.split_at(rest.len() / 2);
        let from = h.w().offset() as usize;
        let s = h
            .w()
            .append_var(EventKind::T_MULTI, F0, Tsc(9), &[a, b2, c])
            .unwrap();
        assert_eq!(s, Seq(3), "{row}");
        let img = h.seg(1);
        let rs: Vec<_> = records(&img, img.len())
            .into_iter()
            .filter(|(o, _)| *o >= from)
            .collect();
        assert_eq!(rs.iter().map(|(_, r)| r.len).collect::<Vec<_>>(), lens, "{row}");
        let n = rs.len();
        let mut logical = Vec::new();
        for (i, (o, r)) in rs.iter().enumerate() {
            let ph = PartHdr::read_from_bytes(&img[o + 24..o + 40]).unwrap();
            assert_eq!(
                (ph.total_len as usize, ph.part as usize, ph.parts as usize),
                (l, i, n),
                "{row}"
            );
            assert_eq!(r.flags & PART != 0, i + 1 < n, "{row}");
            assert_eq!((r.seq, r.kind, r.tsc), (3 + i as u64, 0x7F05, 9), "{row}");
            logical.extend_from_slice(&img[o + 40..o + 40 + ph.chunk_len as usize]);
        }
        assert_eq!(logical, body, "{row}");
        if row == "JNL-022.2" {
            let c1 = PartHdr::read_from_bytes(&img[rs[1].0 + 24..rs[1].0 + 40])
                .unwrap()
                .chunk_len;
            assert_eq!(c1, 1, "{row}");
        }
    }
}

#[test]
fn jnl_023_multipart_never_spans_segments() {
    let mut h = H::open0(mem_c0());
    h.push_to(1_000_000);
    assert_eq!(h.prep.poll(), PrepState::Prepared);
    let seq = h.w().next_seq();
    let before = h.seg(1);
    let body = multi_body(2 * 65_488);
    assert_eq!(
        h.w().append_var(EventKind::T_MULTI, F0, Tsc(1), &[&body]),
        Err(AppendErr::Full)
    );
    assert_eq!((h.w().next_seq(), h.w().offset()), (seq, 1_000_000));
    assert_eq!(h.seg(1), before);
    h.roll(RollReason::Size);
    assert_eq!(h.w().offset(), 4096 + 144);
    h.w()
        .append_var(EventKind::T_MULTI, F0, Tsc(1), &[&body])
        .unwrap();
    let img = h.seg(2);
    let rs = records(&img, img.len());
    assert_eq!(rs.len(), 4);
    assert_eq!((rs[2].0, rs[2].1.kind, rs[3].1.kind), (4240, 0x7F05, 0x7F05));
}

#[test]
fn jnl_024_reserve_var() {
    let mut h = H::open0(r0(&mem_hot(), &cold_dir()));
    let seq = h.w().next_seq();
    let o = h.w().offset() as usize;
    let mut s = h.w().reserve_var(EventKind::RAW_RX, F0, Tsc(1), 600).unwrap();
    s.buf().fill(0x5A);
    assert_eq!(s.commit(), seq, "JNL-024.1");
    let img = h.seg(1);
    let ph = PartHdr::read_from_bytes(&img[o + 24..o + 40]).unwrap();
    assert_eq!(ph.total_len, 600, "JNL-024.1");
    assert!(img[o + 40..o + 640].iter().all(|x| *x == 0x5A), "JNL-024.1");

    let mut h = H::open0(mem_c0());
    let seq = h.w().next_seq();
    let o = h.w().offset();
    {
        let mut s = h.w().reserve_var(EventKind::T_MULTI, F0, Tsc(1), 100).unwrap();
        s.buf().fill(0x77);
    }
    assert_eq!((h.w().next_seq(), h.w().offset()), (seq, o), "JNL-024.2");
    let img = h.seg(1);
    assert!(
        img[o as usize..o as usize + 4096].iter().all(|x| *x == 0),
        "JNL-024.2"
    );
    assert_eq!(h.w().append(F0, Tsc(1), &tin(3)), Ok(seq), "JNL-024.3");
    let img = h.seg(1);
    let r = rh(&img, o as usize);
    assert_eq!((r.len, r.seq), (104, seq.0), "JNL-024.3");
    assert!(
        img[o as usize + 104..o as usize + 4096].iter().all(|x| *x == 0),
        "JNL-024.3"
    );
}

#[test]
fn jnl_025_var_single_record() {
    let mut h = H::open0(mem_c0());
    let mut r = h.reader(StartAt::Tail);
    let o = h.w().offset() as usize;
    let body = tvar(1000, 5);
    h.w()
        .append_var(EventKind::T_VAR, F0, Tsc(1), &[&body[..16], &body[16..]])
        .unwrap();
    h.w().end_event();
    h.w().publish();
    let x = rh(&h.seg(1), o);
    assert_eq!((x.len, x.flags & PART), (1040, 0));
    match r.poll() {
        Next::Record(v) => assert_eq!(v.body.len(), 1016),
        o => panic!("{o:?}"),
    }
}

#[test]
fn jnl_026_sealed_before_first_roll() {
    let cfg = mem_c0();
    let mut h = H::open0(cfg.clone());
    h.events(2);
    h.w.take().unwrap().seal_and_close().unwrap();
    h.crash();
    let rep = h.open().unwrap();
    let before = h.seg(1);
    assert_eq!(h.w().append(F0, Tsc(1), &tin(1)), Err(AppendErr::Sealed));
    assert_eq!(h.seg(1), before);
    assert_eq!(rep.boot_reason, RollReason::Startup);
    h.roll(rep.boot_reason);
    assert!(h.w().append(F0, Tsc(1), &tin(1)).is_ok());
    assert_eq!(h.w().segment_no(), 2);
}

#[test]
fn jnl_027_dropped_bootwriter() {
    let mut h = H::open0(mem_c0());
    {
        let mut b = h.roll_boot(RollReason::Hourly);
        b.append(Tsc(1), &tmeta(1)).unwrap();
    }
    assert_eq!(h.w().append(F0, Tsc(1), &tin(1)), Err(AppendErr::InBoot));
    assert!(matches!(h.try_roll(RollReason::Hourly), Err(RollErr::InBoot)));
    let img = h.seg(2);
    let rs = records(&img, img.len());
    assert_eq!(
        rs.iter().map(|(_, r)| r.kind).collect::<Vec<_>>(),
        vec![0x0509, 0x7F03]
    );
    assert_eq!(h.committed(2), 4096);
}

#[test]
fn jnl_038_snapshot_passthrough() {
    for (row, purpose, hash) in [
        ("JNL-038.1", 2u8, [0u8; 16]),
        ("JNL-038.2", 1, [0x3C; 16]),
        ("JNL-038.3", 2, [0x01; 16]),
    ] {
        let mut h = H::open0(mem_c0());
        let mut r = h.reader(StartAt::Tail);
        let mut sh = SnapshotHdr::new_zeroed();
        sh.purpose = purpose;
        sh.n_sections = 2;
        sh.state_hash = hash;
        let mut body = sh.as_bytes().to_vec();
        body.extend(multi_body(200_000));
        h.w()
            .append_var(EventKind::STATE_SNAPSHOT, F0, Tsc(1), &[&body])
            .unwrap();
        h.w().end_event();
        h.w().publish();
        match r.poll() {
            Next::Event(e) => {
                assert_eq!(e.body, &body[..], "{row}");
                assert_eq!(&e.body[16..32], &hash, "{row}");
            }
            o => panic!("{row}: {o:?}"),
        }
    }
}

#[test]
fn jnl_104_raw_writes_all() {
    let mut h = H::open0(r0(&mem_hot(), &cold_dir()));
    let mut r = h.reader(StartAt::Tail);
    r.set_clock_ns(0);
    for i in 0..100u32 {
        let rf = if i % 10 < 3 { RawHdr::SAMPLED } else { 0 };
        h.w()
            .append_var(EventKind::RAW_RX, F0, Tsc(1), &[&raw_body(64, rf, i as u8)])
            .unwrap();
    }
    h.w().publish();
    let mut n = 0;
    let mut sampled = 0;
    loop {
        match r.poll() {
            Next::Event(e) => {
                assert_eq!(e.kind, EventKind::RAW_RX);
                assert!(e.flags.contains(RecordFlags::CRC));
                let rhd = RawHdr::read_from_bytes(&e.body[..24]).unwrap();
                assert_eq!(rhd.payload_len, 64);
                sampled += usize::from(rhd.rflags == RawHdr::SAMPLED);
                n += 1;
            }
            Next::Pending => break,
            o => panic!("{o:?}"),
        }
    }
    assert_eq!((n, sampled), (100, 30));
}

#[test]
fn jnl_121_crc_policy_by_writer() {
    let mut h = H::open0(mem_c0());
    let one = |h: &mut H, f: &dyn Fn(&mut JournalWriter)| -> (usize, Vec<u8>) {
        let o = h.w().offset() as usize;
        f(h.w());
        h.w().end_event();
        h.w().publish();
        let no = h.w().segment_no();
        (o, h.seg(no))
    };
    let (o, b) = one(&mut h, &|w| {
        w.append(RecordFlags(CRC), Tsc(1), &tin(1)).unwrap();
    });
    let x = rh(&b, o);
    assert_eq!((x.len, x.flags & CRC), (104, 0), "JNL-121.1");
    let (o, b) = one(&mut h, &|w| {
        w.append(F0, Tsc(1), &tcrc(1)).unwrap();
    });
    let x = rh(&b, o);
    assert_eq!((x.len, x.flags & CRC), (72, CRC), "JNL-121.2");
    assert_eq!(u32_at(&b, o + 68), crc32c::crc32c(&b[o..o + 68]), "JNL-121.2");
    let (o, b) = one(&mut h, &|w| {
        w.append_var(EventKind::T_MULTI, RecordFlags(CRC), Tsc(1), &[&[7u8; 100]])
            .unwrap();
    });
    let x = rh(&b, o);
    assert_eq!((x.len, x.flags & (CRC | PART)), (24 + 16 + 104, 0), "JNL-121.3");
    let (o, b) = one(&mut h, &|w| {
        w.append(RecordFlags(PART), Tsc(1), &tin(1)).unwrap();
    });
    assert_eq!(rh(&b, o).flags & PART, 0, "JNL-121.5");
    let o1 = h.w().offset() as usize;
    h.w().append(RecordFlags(EVENT_END), Tsc(1), &tin(1)).unwrap();
    let o2 = h.w().offset() as usize;
    h.w().append(F0, Tsc(1), &tout(1)).unwrap();
    h.w().end_event();
    let b = h.seg(1);
    assert_eq!(
        (rh(&b, o1).flags & EVENT_END, rh(&b, o2).flags & EVENT_END),
        (0, EVENT_END),
        "JNL-121.6"
    );

    let mut h = H::open0(r0(&mem_hot(), &cold_dir()));
    let o = h.w().offset() as usize;
    h.w().append(F0, Tsc(1), &tin(1)).unwrap();
    let x = rh(&h.seg(1), o);
    assert_eq!((x.len, x.flags & CRC), (112, CRC), "JNL-121.4");
    let _ = PartHdr::new_zeroed();
}
