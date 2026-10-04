//! Reader: tailing, poll interval, start positions, boot visibility, reassembly, corrupt groups,
//! cross-segment and cold switching, gaps, read errors, LastCompleteEvent, unmapping, mapping
//! limits and in-process slots (JNL-074 – JNL-086). Linux rows use tmpfs; elsewhere the heap backend.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod common;

use common::harness::*;
use hft_event::{EventKind, PartHdr, RecordFlags, SnapshotHdr};
use hft_journal::testing::{self, Op};
use hft_journal::*;
use hft_types::Tsc;
use zerocopy::{FromZeros, IntoBytes};

fn seq_of(n: Next<'_>) -> u64 {
    match n {
        Next::Record(v) => v.hdr.seq,
        Next::Event(e) => e.seq,
        o => panic!("expected a record, got {o:?}"),
    }
}

#[test]
fn jnl_074_tail_bounded_by_committed() {
    let mut h = H::open0(mem_c0());
    let mut r = h.reader(StartAt::Tail);
    assert!(matches!(r.poll(), Next::Pending));
    h.w().append(F0, Tsc(1), &tin(1)).unwrap();
    assert!(matches!(r.poll(), Next::Pending));
    h.w().end_event();
    h.w().publish();
    assert_eq!(seq_of(r.poll()), 3);
    assert!(matches!(r.poll(), Next::Pending));
    assert_eq!((r.position().segment_no, r.position().seq), (1, 3));
}

#[test]
fn jnl_075_tail_poll_interval() {
    let mut h = H::open0(mem_c0());
    for i in 0..100 {
        h.w().append(F0, Tsc(i), &tin(1)).unwrap();
        h.w().end_event();
    }
    h.w().publish();
    let mut c = h.reader_cfg(StartAt::Oldest);
    c.tail_poll_us = 50;
    let mut r = JournalReader::open(&c).unwrap();
    let t = 1_000_000_000u64;
    r.set_clock_ns(t);
    assert_eq!(seq_of(r.poll()), 1);
    let base = r.writer_row_reads();
    for _ in 0..100 {
        seq_of(r.poll());
    }
    assert_eq!(r.writer_row_reads() - base, 0, "JNL-075.1");
    seq_of(r.poll());
    assert!(matches!(r.poll(), Next::Pending));
    let base = r.writer_row_reads();
    r.set_clock_ns(t + 49_999);
    for _ in 0..1000 {
        assert!(matches!(r.poll(), Next::Pending), "JNL-075.2");
    }
    assert_eq!(r.writer_row_reads() - base, 0, "JNL-075.2");
    r.set_clock_ns(t + 50_000);
    assert!(matches!(r.poll(), Next::Pending));
    assert_eq!(r.writer_row_reads() - base, 1, "JNL-075.3");
}

fn ins(h: &mut H, n: u64) {
    let w = h.w();
    while w.next_seq().0 <= n {
        w.append(F0, Tsc(1), &tin(1)).unwrap();
        w.end_event();
    }
    w.publish();
}

fn keyframe(h: &mut H) {
    let mut sh = SnapshotHdr::new_zeroed();
    sh.purpose = 2;
    let w = h.w();
    w.append_var(EventKind::STATE_SNAPSHOT, F0, Tsc(1), &[sh.as_bytes()])
        .unwrap();
    w.end_event();
    w.publish();
}

fn boot4(h: &mut H) {
    let mut b = h.roll_boot(RollReason::Hourly);
    b.append(Tsc(1), &tmeta(1)).unwrap();
    b.append(Tsc(1), &tmeta(2)).unwrap();
    b.finish().unwrap();
}

/// Segment 1: seq 1–100 (TMulti 40–42); segment 2: boot 101–104, events to 150 (keyframe 120);
/// segment 3: boot 151–154, events to 170 (keyframe at 160 when `kf160`).
fn three(kf160: bool) -> H {
    let mut h = H::open0(mem_c0());
    ins(&mut h, 39);
    h.w()
        .append_var(EventKind::T_MULTI, F0, Tsc(1), &[&multi_body(2 * 65_488 + 1)])
        .unwrap();
    h.w().end_event();
    ins(&mut h, 99);
    boot4(&mut h);
    ins(&mut h, 119);
    keyframe(&mut h);
    ins(&mut h, 149);
    boot4(&mut h);
    if kf160 {
        ins(&mut h, 159);
        keyframe(&mut h);
    }
    ins(&mut h, 170);
    assert_eq!(h.w().next_seq().0, 171);
    h
}

#[test]
fn jnl_076_start_positions() {
    let mut h = three(false);
    let first = |h: &H, s: StartAt| -> u64 {
        let mut r = h.reader(s);
        seq_of(r.poll())
    };
    assert_eq!(first(&h, StartAt::Oldest), 1, "JNL-076.1");
    assert_eq!(first(&h, StartAt::Seq(77)), 77, "JNL-076.2");
    let mut r = h.reader(StartAt::Seq(41));
    match r.poll() {
        Next::Event(e) => assert_eq!((e.seq, e.parts), (40, 3), "JNL-076.3"),
        o => panic!("JNL-076.3 {o:?}"),
    }
    let mut r = h.reader(StartAt::Seq(171));
    assert!(matches!(r.poll(), Next::Pending), "JNL-076.4");
    ins(&mut h, 171);
    assert_eq!(seq_of(r.poll()), 171, "JNL-076.4");
    assert_eq!(
        JournalReader::open(&h.reader_cfg(StartAt::Seq(173))).err(),
        Some(OpenErr::SeqUnavailable),
        "JNL-076.5"
    );
    let mut r = h.reader(StartAt::SegmentBoot(2));
    assert_eq!(seq_of(r.poll()), 101, "JNL-076.6");
    assert!(r.in_boot_block(), "JNL-076.6");
    assert_eq!(first(&h, StartAt::LatestBoot), 151, "JNL-076.7");
    assert_eq!(first(&h, StartAt::LatestKeyframe), 151, "JNL-076.8");
    let h9 = three(true);
    assert_eq!(first(&h9, StartAt::LatestKeyframe), 160, "JNL-076.9");
    let mut r = h.reader(StartAt::Tail);
    assert!(matches!(r.poll(), Next::Pending), "JNL-076.10");
    testing::unlink(&h.seg_path(1)).unwrap();
    assert_eq!(
        JournalReader::open(&h.reader_cfg(StartAt::Seq(50))).err(),
        Some(OpenErr::SeqUnavailable),
        "JNL-076.11"
    );
    let empty = mem_c0();
    let c = ReaderConfig::new(empty.hot_dir, empty.cold_root, InstanceName::Prod, "seq".into());
    assert_eq!(
        JournalReader::open(&c).err(),
        Some(OpenErr::NotFound),
        "JNL-076.12"
    );
}

#[test]
fn jnl_077_boot_block_visibility() {
    let mut h = H::open0(mem_c0());
    ins(&mut h, 10);
    let mut r = h.reader(StartAt::Oldest);
    assert_eq!(drain(&mut r).last().unwrap().0, 10);
    let mut sh = SnapshotHdr::new_zeroed();
    sh.purpose = 1;
    let mut snap = sh.as_bytes().to_vec();
    snap.extend(multi_body(70_000));
    {
        let mut b = h.roll_boot(RollReason::Hourly);
        b.append(Tsc(1), &tmeta(1)).unwrap();
        b.append_var(EventKind::STATE_SNAPSHOT, Tsc(1), &[&snap]).unwrap();
        let got = drain(&mut r);
        assert_eq!(got.len(), 1, "only End of segment 1: {got:?}");
        assert!(matches!(r.poll(), Next::Pending));
        b.finish().unwrap();
    }
    h.w().publish();
    let mut kinds = Vec::new();
    for _ in 0..4 {
        let k = match r.poll() {
            Next::Record(v) => v.hdr.kind,
            Next::Event(e) => e.kind.0,
            o => panic!("{o:?}"),
        };
        kinds.push((k, r.in_boot_block()));
    }
    assert_eq!(
        kinds,
        vec![(0x0509, true), (0x7F03, true), (0x0504, true), (0x0509, true)]
    );
    let n = h.w().next_seq().0;
    ins(&mut h, n);
    seq_of(r.poll());
    assert!(!r.in_boot_block());
}

#[test]
fn jnl_078_reassembly() {
    let rows: [(&str, usize, Option<EventKind>, usize); 6] = [
        ("JNL-078.1", 130_977, None, 256 << 20),
        ("JNL-078.2", 1000, None, 256 << 20),
        ("JNL-078.3", 1000, Some(EventKind::T_VAR), 256 << 20),
        ("JNL-078.4", 130_977, None, 0),
        ("JNL-078.5", 130_977, None, 130_976),
        ("JNL-078.6", 130_977, None, 130_977),
    ];
    for (row, l, var, cap) in rows {
        let mut h = H::open0(mem_c0());
        let mut c = h.reader_cfg(StartAt::Tail);
        c.reassembly_cap = cap;
        let mut r = JournalReader::open(&c).unwrap();
        let body = if var.is_some() { tvar(l, 3) } else { multi_body(l) };
        h.w()
            .append_var(var.unwrap_or(EventKind::T_MULTI), F0, Tsc(1), &[&body])
            .unwrap();
        h.w().end_event();
        h.w().publish();
        match row {
            "JNL-078.1" | "JNL-078.6" => {
                match r.poll() {
                    Next::Event(e) => {
                        assert_eq!((e.seq, e.parts, e.kind.0), (3, 3, 0x7F05), "{row}");
                        assert_eq!(e.body, &body[..], "{row}");
                        assert!(e.flags.contains(RecordFlags::EVENT_END), "{row}");
                    }
                    o => panic!("{row}: {o:?}"),
                }
                assert_eq!(r.position().seq, 5, "{row}");
            }
            "JNL-078.2" => assert!(matches!(r.poll(), Next::Event(e) if e.parts == 1), "{row}"),
            "JNL-078.3" => assert!(matches!(r.poll(), Next::Record(_)), "{row}"),
            "JNL-078.4" => {
                for _ in 0..3 {
                    match r.poll() {
                        Next::Record(v) => assert!(v.body.len() >= 16, "{row}"),
                        o => panic!("{row}: {o:?}"),
                    }
                }
            }
            _ => assert!(matches!(r.poll(), Next::Err(ReadErr::ReassemblyCap)), "{row}"),
        }
    }
}

#[test]
fn jnl_079_corrupt_groups() {
    let l = 2 * 65_488 + 100;
    let body = multi_body(l);
    let ph = |p: u16, parts: u16, chunk: usize| PartHdr {
        total_len: l as u32,
        chunk_len: chunk as u32,
        part: p,
        parts,
        _rsv: 0,
    };
    let chunk = |p: usize| &body[p * 65_488..((p + 1) * 65_488).min(l)];
    for row in ["JNL-079.1", "JNL-079.2", "JNL-079.3", "JNL-079.4"] {
        let mut h = H::open0(mem_c0());
        let w = h.w();
        let part = |w: &mut JournalWriter, hd: PartHdr, c: &[u8], last: bool| {
            let f = if last { RecordFlags(0) } else { RecordFlags(PART) };
            w.append_part_raw(EventKind::T_MULTI, f, Tsc(1), hd, c).unwrap();
        };
        match row {
            "JNL-079.1" => {
                part(w, ph(0, 3, 65_488), chunk(0), false);
                part(w, ph(1, 3, 65_488), chunk(1), false);
                part(w, ph(2, 3, 92), &chunk(2)[..92], true);
            }
            "JNL-079.2" => {
                part(w, ph(1, 3, 65_488), chunk(0), false);
                part(w, ph(0, 3, 65_488), chunk(1), false);
                part(w, ph(2, 3, 100), chunk(2), true);
            }
            "JNL-079.3" => {
                part(w, ph(0, 3, 65_488), chunk(0), false);
                w.append(F0, Tsc(1), &tin(1)).unwrap();
                part(w, ph(1, 3, 65_488), chunk(1), false);
                part(w, ph(2, 3, 100), chunk(2), true);
            }
            _ => {
                part(w, ph(0, 3, 65_488), chunk(0), false);
                part(w, ph(1, 4, 65_488), chunk(1), false);
                part(w, ph(2, 3, 100), chunk(2), true);
            }
        }
        w.end_event();
        w.append(F0, Tsc(1), &tin(1)).unwrap();
        w.end_event();
        w.publish();
        let mut r = h.reader(StartAt::Oldest);
        let mut out = None;
        for _ in 0..10 {
            match r.poll() {
                Next::Err(e) => {
                    out = Some(e);
                    break;
                }
                Next::Pending => break,
                _ => {}
            }
        }
        assert_eq!(out, Some(ReadErr::Corrupt), "{row}");
    }
}

/// Segments 1, 2 sealed and persisted; 3 active (Linux rows).
fn three_linux() -> H {
    let mut h = H::open0(linux_c0());
    h.events(20);
    h.roll(RollReason::Hourly);
    h.events(20);
    h.roll(RollReason::Hourly);
    h.events(20);
    h.persist_all();
    h
}

fn all_seqs(r: &mut JournalReader) -> Vec<u64> {
    drain(r).iter().map(|x| x.0).collect()
}

#[test]
fn jnl_080_cross_segments() {
    let h = three_linux();
    let mut r = h.reader(StartAt::Oldest);
    let base = all_seqs(&mut r);
    let last = h.last_seq(3);
    assert_eq!(base, (1..=last).collect::<Vec<_>>(), "JNL-080.1");
    drop(r);

    let h2 = three_linux();
    testing::unlink(&h2.seg_path(1)).unwrap();
    let mut r = h2.reader(StartAt::Oldest);
    assert_eq!(all_seqs(&mut r), base, "JNL-080.2");
    drop(r);

    let h3 = three_linux();
    let mut r = h3.reader(StartAt::Oldest);
    let mut got = Vec::new();
    while r.position().segment_no < 2 || r.position().seq < h3.hdr(2).first_seq + 10 {
        got.push(seq_of(r.poll()));
    }
    testing::poke(&h3.seg_path(2), 3152, &1u32.to_le_bytes()).unwrap();
    got.extend(all_seqs(&mut r));
    assert_eq!(got, base, "JNL-080.3");
}

/// Segments 1–4 (4 active), all persisted.
fn four(gap: Option<bool>) -> H {
    let mut h = H::open0(linux_c0());
    h.events(5);
    h.roll(RollReason::Hourly);
    h.events(5);
    if let Some(flag) = gap {
        h.w().debug_gap_on_roll(4, flag);
    }
    h.roll(RollReason::Hourly);
    h.events(5);
    h.roll(RollReason::Hourly);
    h.events(5);
    h.persist_all();
    h
}

fn remove_seg(h: &H, no: u64) {
    let stem = h.cold_stem(no);
    for e in ["jrn", "seal"] {
        let _ = std::fs::remove_file(stem.with_extension(e));
    }
    testing::unlink(&h.seg_path(no)).unwrap();
}

fn gap_cfg(h: &H, on_gap: OnGap) -> ReaderConfig {
    let mut c = h.reader_cfg(StartAt::Oldest);
    c.on_gap = on_gap;
    c
}

fn until_gap(r: &mut JournalReader) -> GapInfo {
    for _ in 0..1000 {
        if let Next::Gap(g) = r.poll() {
            return g;
        }
    }
    panic!("no gap");
}

#[test]
fn jnl_081_gaps() {
    for (row, on_gap) in [("JNL-081.1", OnGap::Error), ("JNL-081.2", OnGap::SkipToNextBoot)] {
        let h = four(None);
        let mut r = JournalReader::open(&gap_cfg(&h, on_gap)).unwrap();
        remove_seg(&h, 2);
        let g = until_gap(&mut r);
        assert_eq!((g.reason, g.resume_segment_no), (GapReason::Deleted, 3), "{row}");
        if on_gap == OnGap::Error {
            assert!(matches!(r.poll(), Next::Gap(x) if x == g), "{row}");
            assert!(matches!(r.poll(), Next::Gap(x) if x == g), "{row}");
        } else {
            assert_eq!(seq_of(r.poll()), h.hdr(3).first_seq, "{row}");
            assert!(r.in_boot_block(), "{row}");
        }
    }
    for (row, on_gap) in [("JNL-081.3", OnGap::Error), ("JNL-081.4", OnGap::SkipToNextBoot)] {
        let h = four(Some(true));
        let last2 = h.last_seq(2);
        assert_eq!(h.hdr(3).first_seq, last2 + 5, "{row}");
        let mut r = JournalReader::open(&gap_cfg(&h, on_gap)).unwrap();
        let g = until_gap(&mut r);
        assert_eq!(
            (g.reason, g.expected_seq),
            (GapReason::RecoveryGap, last2 + 1),
            "{row}"
        );
        if on_gap == OnGap::Error {
            assert!(matches!(r.poll(), Next::Gap(x) if x == g), "{row}");
        } else {
            assert_eq!(seq_of(r.poll()), last2 + 5, "{row}");
        }
    }
    let h = four(Some(false));
    let mut r = JournalReader::open(&gap_cfg(&h, OnGap::SkipToNextBoot)).unwrap();
    assert_eq!(until_gap(&mut r).reason, GapReason::ChainBroken, "JNL-081.5");
    let h = four(None);
    remove_seg(&h, 3);
    let mut r = JournalReader::open(&gap_cfg(&h, OnGap::SkipToNextBoot)).unwrap();
    let g = until_gap(&mut r);
    assert_eq!(
        (g.reason, g.resume_segment_no),
        (GapReason::MissingSegment, 4),
        "JNL-081.6"
    );
    assert_eq!(seq_of(r.poll()), h.hdr(4).first_seq, "JNL-081.6");
}

#[test]
fn jnl_082_read_errors() {
    let setup = || {
        let mut h = H::open0(mem_c0());
        let w = h.w();
        let a = w.offset() as usize;
        w.append(F0, Tsc(1), &tin(1)).unwrap();
        let b = w.offset() as usize;
        w.append(F0, Tsc(1), &tcrc(1)).unwrap();
        w.end_event();
        w.append(F0, Tsc(1), &tin(2)).unwrap();
        w.end_event();
        w.publish();
        (h, a, b)
    };
    let first_err = |r: &mut JournalReader| -> Option<ReadErr> {
        for _ in 0..20 {
            match r.poll() {
                Next::Err(e) => return Some(e),
                Next::Pending | Next::End => return None,
                _ => {}
            }
        }
        None
    };
    let (h, a, _) = setup();
    testing::poke(&h.seg_path(1), a + 4, &0x05FFu16.to_le_bytes()).unwrap();
    let mut r = h.reader(StartAt::Oldest);
    let got = drain(&mut r);
    assert_eq!(got.len(), 5, "JNL-082.1");
    assert_eq!(got[2].1, 0x05FF, "JNL-082.1");
    let (h, a, _) = setup();
    testing::poke(&h.seg_path(1), a + 4, &0x0601u16.to_le_bytes()).unwrap();
    assert_eq!(
        first_err(&mut h.reader(StartAt::Oldest)),
        Some(ReadErr::Corrupt),
        "JNL-082.2"
    );
    let (h, _, b) = setup();
    testing::poke(&h.seg_path(1), b + 30, &[0x23]).unwrap();
    assert_eq!(
        first_err(&mut h.reader(StartAt::Oldest)),
        Some(ReadErr::BadCrc),
        "JNL-082.3"
    );
    let (h, _, _) = setup();
    let mut r = h.reader(StartAt::Oldest);
    r.set_production(true);
    assert_eq!(first_err(&mut r), Some(ReadErr::DevKind), "JNL-082.4");
    let (h, _, _) = setup();
    testing::poke(&h.seg_path(1), 8, &2u16.to_le_bytes()).unwrap();
    let img = h.seg(1);
    testing::poke(&h.seg_path(1), 3068, &crc32c::crc32c(&img[..3068]).to_le_bytes()).unwrap();
    assert_eq!(
        first_err(&mut h.reader(StartAt::Oldest)),
        Some(ReadErr::Corrupt),
        "JNL-082.5"
    );
    let (h, _, _) = setup();
    testing::truncate(&h.seg_path(1), 4000).unwrap();
    assert_eq!(
        first_err(&mut h.reader(StartAt::Oldest)),
        Some(ReadErr::Corrupt),
        "JNL-082.6"
    );
    let (h, a, _) = setup();
    let s = u64_at(&h.seg(1), a + 8);
    testing::poke(&h.seg_path(1), a + 8, &(s + 1).to_le_bytes()).unwrap();
    assert_eq!(
        first_err(&mut h.reader(StartAt::Oldest)),
        Some(ReadErr::Corrupt),
        "JNL-082.7"
    );
}

#[test]
fn jnl_083_last_complete_event() {
    let mut h = H::open0(mem_c0());
    for _ in 0..2 {
        h.w().append(F0, Tsc(1), &tin(1)).unwrap();
        h.w().append(F0, Tsc(1), &tout(1)).unwrap();
        h.w().end_event();
    }
    h.w().publish();
    h.w().append(F0, Tsc(1), &tin(3)).unwrap();
    h.w().append(F0, Tsc(1), &tout(3)).unwrap();
    h.w().end_event();
    let e3_end = h.w().offset();
    let e3_last = h.w().next_seq().0 - 1;
    h.w().append(F0, Tsc(1), &tin(4)).unwrap();
    let mut c = h.reader_cfg(StartAt::Oldest);
    c.end = EndAt::LastCompleteEvent;
    let mut r = JournalReader::open(&c).unwrap();
    let got = drain(&mut r);
    assert_eq!(got.last().unwrap().0, e3_last);
    assert!(matches!(r.poll(), Next::End));
    assert_eq!(
        r.last_complete_event(),
        Some(Position {
            segment_no: 1,
            offset: e3_end,
            seq: e3_last
        })
    );
}

#[cfg(target_os = "linux")]
fn maps_contain(needle: &str) -> bool {
    std::fs::read_to_string("/proc/self/maps")
        .unwrap()
        .contains(needle)
}

#[test]
fn jnl_084_unmap_deleted_segments() {
    // Row 1: retired → released in the same poll, continue on the cold copy.
    let h = three_linux();
    let mut r = h.reader(StartAt::SegmentBoot(2));
    r.set_clock_ns(0);
    let mut got: Vec<u64> = (0..5).map(|_| seq_of(r.poll())).collect();
    testing::poke(&h.seg_path(2), 3152, &1u32.to_le_bytes()).unwrap();
    got.push(seq_of(r.poll()));
    #[cfg(target_os = "linux")]
    assert!(!maps_contain(h.seg_path(2).to_str().unwrap()), "JNL-084.1");
    got.extend(all_seqs(&mut r));
    let first = h.hdr(2).first_seq;
    assert_eq!(got, (first..=h.last_seq(3)).collect::<Vec<_>>(), "JNL-084.1");

    // Row 2: unlinked without `retired`: released at the next fstat (+1 s).
    let h = three_linux();
    let mut r = h.reader(StartAt::SegmentBoot(2));
    r.set_clock_ns(0);
    for _ in 0..5 {
        seq_of(r.poll());
    }
    testing::unlink(&h.seg_path(2)).unwrap();
    r.set_clock_ns(1_000_000_000);
    seq_of(r.poll());
    #[cfg(target_os = "linux")]
    assert!(
        !maps_contain("(deleted)") || !maps_contain(h.seg_path(2).to_str().unwrap()),
        "JNL-084.2"
    );

    // Row 3: the cold copy being read is removed by retention.
    let h = three_linux();
    testing::unlink(&h.seg_path(1)).unwrap();
    let mut r = h.reader(StartAt::Oldest);
    r.set_clock_ns(0);
    for _ in 0..5 {
        seq_of(r.poll());
    }
    let rep = cold_retire(
        &h.cold_jdir(),
        &ColdRetention {
            keep_days: 0,
            require_uploaded: false,
        },
        h.utc + 1,
    )
    .unwrap();
    assert!(rep.segments >= 1);
    r.set_clock_ns(1_000_000_000);
    assert!(
        matches!(r.poll(), Next::Gap(g) if g.reason == GapReason::Deleted),
        "JNL-084.3"
    );
}

#[test]
fn jnl_085_two_hot_maps() {
    let mut cfg = linux_c0();
    cfg.hot_keep = 4;
    let mut h = H::open0(cfg);
    for _ in 0..5 {
        h.events(10);
        h.roll(RollReason::Hourly);
    }
    h.events(10);
    let mut r = h.reader(StartAt::Oldest);
    #[allow(unused_mut)]
    let mut max = 0;
    loop {
        match r.poll() {
            Next::Pending => break,
            Next::Record(_) | Next::Event(_) => {}
            o => panic!("{o:?}"),
        }
        #[cfg(target_os = "linux")]
        {
            let maps = std::fs::read_to_string("/proc/self/maps").unwrap();
            let dir = h.hot_jdir();
            let n = maps.lines().filter(|l| l.contains(dir.to_str().unwrap())).count();
            max = max.max(n);
        }
    }
    assert_eq!(r.position().segment_no, 6);
    assert!(max <= 2, "{max}");
}

#[test]
fn jnl_086_fixed_slots() {
    let mut h = H::open0(mem_c0());
    let mut c = h.reader_cfg(StartAt::Oldest);
    c.same_process = true;
    let mut r = JournalReader::open(&c).unwrap();
    let mut ops = Vec::new();
    let mut seqs = Vec::new();
    let poll_all = |r: &mut JournalReader, ops: &mut Vec<Op>, seqs: &mut Vec<u64>| {
        testing::oplog_start();
        seqs.extend(drain(r).iter().map(|x| x.0));
        ops.extend(testing::oplog_take());
    };
    for _ in 0..10 {
        h.events(3);
        poll_all(&mut r, &mut ops, &mut seqs);
        h.roll(RollReason::Hourly);
        poll_all(&mut r, &mut ops, &mut seqs);
    }
    let n = ops.iter().filter(|o| matches!(o, Op::Map | Op::Unmap)).count();
    assert_eq!(n, 0, "{ops:?}");
    assert_eq!(seqs, (1..h.w().next_seq().0).collect::<Vec<_>>());
}
