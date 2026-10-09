//! Size valves, roll products, boot blocks, NextNotReady/Io, seal_and_close, config digest and
//! self-sufficient boot blocks (JNL-028 – JNL-037, JNL-120).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::common;

use common::harness::*;
use hft_event::{BlobHdr, BlobKind, BlobRef, CmdScope, EventKind, SnapshotHdr};
use hft_journal::testing;
use hft_journal::*;
use hft_rt::CalibPair;
use hft_types::{Seq, Tsc};
use sha2::{Digest, Sha256};
use zerocopy::{FromZeros, IntoBytes};

#[test]
fn jnl_028_size_valves_seq() {
    for (row, c, want) in [
        ("JNL-028.1", 786_136usize, None),
        ("JNL-028.2", 786_144, Some((RollReason::Size, RollAt::IdleRound))),
        ("JNL-028.3", 943_424, Some((RollReason::Size, RollAt::IdleRound))),
        (
            "JNL-028.4",
            943_432,
            Some((RollReason::Size, RollAt::EventBoundary)),
        ),
        (
            "JNL-028.5",
            1_048_288,
            Some((RollReason::Size, RollAt::EventBoundary)),
        ),
    ] {
        let mut h = H::open0(mem_c0());
        assert_eq!(h.w().reserve_bytes(), 288);
        h.push_to(c);
        assert_eq!(h.committed(1), c as u64, "{row}");
        assert_eq!(h.w().needs_roll(Tsc(123)), want, "{row}");
    }
}

#[test]
fn jnl_029_raw_valve_and_deadline() {
    let raw = || H::open0(r0(&mem_hot(), &cold_dir()));
    for (row, c, want) in [
        ("JNL-029.1", 900_000usize, None),
        ("JNL-029.2", 943_552, None),
        (
            "JNL-029.3",
            943_560,
            Some((RollReason::Size, RollAt::EventBoundary)),
        ),
    ] {
        let mut h = raw();
        assert_eq!(h.w().reserve_bytes(), 160);
        h.push_to(c);
        assert_eq!(h.w().needs_roll(Tsc(0)), want, "{row}");
    }
    let mut h = raw();
    h.push_to(10_000);
    h.w().set_roll_deadline(Tsc(10_000));
    assert_eq!(h.w().needs_roll(Tsc(9_999)), None, "JNL-029.4");
    assert_eq!(
        h.w().needs_roll(Tsc(10_000)),
        Some((RollReason::Hourly, RollAt::EventBoundary)),
        "JNL-029.5"
    );
    h.push_to(943_560);
    assert_eq!(
        h.w().needs_roll(Tsc(10_000)),
        Some((RollReason::Size, RollAt::EventBoundary)),
        "JNL-029.6"
    );
}

#[test]
fn jnl_030_reserve_follows_boot_size() {
    for (c, want) in [
        (775_160, None),
        (775_168, Some((RollReason::Size, RollAt::IdleRound))),
    ] {
        let mut h = H::open0(mem_c0());
        let mut b = h.roll_boot(RollReason::Hourly);
        for i in 0..98 {
            b.append(Tsc(1), &tmeta(i)).unwrap();
        }
        b.finish().unwrap();
        assert_eq!(h.w().reserve_bytes(), 11_264);
        h.push_to(c);
        assert_eq!(h.w().needs_roll(Tsc(0)), want, "committed {c}");
    }
}

fn blob(i: u8) -> BlobRef {
    BlobRef {
        kind: BlobKind::Refdata,
        encoding: 1,
        len: 100 + u32::from(i),
        version: u64::from(i),
        sha256: [i; 32],
    }
}

#[test]
fn jnl_031_roll_products() {
    let u = UTC0 + 3_600_000_000_000;
    let mut h = H::bare(mem_c0(), true);
    h.open().unwrap();
    h.roll(RollReason::Startup);
    assert_eq!(h.tap.as_mut().unwrap().try_pop().map(|r| r.segment_no), None);
    h.write_to_seq(10);
    h.utc = u;
    h.calib = CalibPair {
        tsc: Tsc(5_000),
        realtime_ns: u,
        phc_ns: 0,
        err_ns: 40,
    };
    h.tsc_hz_milli = 2_500_000_000_000;
    h.configs = vec![blob(1), blob(2), blob(3)];
    h.roll(RollReason::Hourly);
    let s1 = h.seg(1);
    let c1 = h.committed(1) as usize;
    let end = rh(&s1, c1 - 72);
    let er = roll_body(&s1, c1 - 72);
    assert_eq!(
        (
            er.mark,
            er.segment_no,
            er.other_segment_no,
            er.seq_mark,
            er.reason
        ),
        (1, 1, 2, 10, 1)
    );
    assert_eq!((end.seq, end.flags & EVENT_END), (11, EVENT_END));
    assert_eq!((h.state(1), h.last_seq(1)), (2, 11));
    let f = h.hdr(2);
    let mut d = Sha256::new();
    for b in &h.configs {
        d.update(b.as_bytes());
    }
    let digest: [u8; 32] = d.finalize().into();
    assert_eq!(
        (
            f.segment_no,
            f.first_seq,
            f.prev_last_seq,
            f.created_utc_ns,
            f.calib_tsc,
            f.calib_err_ns
        ),
        (2, 12, 11, u, 5_000, 40)
    );
    assert_eq!(
        (f.tsc_hz_milli, f.roll_reason, f.flags, f.n_config),
        (2_500_000_000_000, 1, 0, 3)
    );
    assert_eq!(f.config_digest, digest);
    let s2 = h.seg(2);
    assert_eq!(f.header_crc32c, crc32c::crc32c(&s2[..3068]));
    assert_eq!(h.state(2), 1);
    let b = rh(&s2, 4096);
    let br = roll_body(&s2, 4096);
    assert_eq!(
        (br.mark, br.segment_no, br.other_segment_no, br.seq_mark),
        (2, 2, 1, 11)
    );
    assert_eq!((b.seq, b.flags & EVENT_END), (12, 0));
    let tap = h.tap.as_mut().unwrap();
    assert_eq!(tap.try_pop().map(|r| r.segment_no), Some(1));
    assert!(tap.try_pop().is_none());
}

fn blob_body(i: u8) -> Vec<u8> {
    let content = vec![i; 40];
    let h = BlobHdr {
        blob: BlobRef { len: 40, ..blob(i) },
        scope: CmdScope::new_zeroed(),
        boot: 1,
        _pad: [0; 7],
    };
    let mut v = h.as_bytes().to_vec();
    v.extend(content);
    v
}

fn snapshot_body(purpose: u8, tail: usize) -> Vec<u8> {
    let mut sh = SnapshotHdr::new_zeroed();
    sh.purpose = purpose;
    let mut v = sh.as_bytes().to_vec();
    v.extend(multi_body(tail));
    v
}

#[test]
fn jnl_032_boot_block_close() {
    let mut h = H::open0(mem_c0());
    h.write_to_seq(10);
    let mut b = h.roll_boot(RollReason::Hourly);
    b.append(Tsc(1), &tmeta(1)).unwrap();
    b.append(Tsc(1), &tmeta(2)).unwrap();
    for i in 0..3 {
        b.append_var(EventKind::CONFIG_BLOB, Tsc(1), &[&blob_body(i)])
            .unwrap();
    }
    b.append_var(
        EventKind::STATE_SNAPSHOT,
        Tsc(1),
        &[&snapshot_body(1, 2 * 65_488 - 96 + 1)],
    )
    .unwrap();
    let done = b.finish().unwrap();
    h.w().publish();
    let s = h.seg(2);
    let rs = records(&s, h.committed(2) as usize);
    assert_eq!(rs.len(), 10);
    let last = rs.last().unwrap();
    assert_eq!(roll_body(&s, last.0).boot_records, 10);
    assert_eq!(done, Seq(last.1.seq));
    let ee: Vec<bool> = rs.iter().map(|(_, r)| r.flags & EVENT_END != 0).collect();
    assert_eq!(ee, [vec![false; 9], vec![true]].concat());
    let parts: Vec<u16> = rs[6..9].iter().map(|(_, r)| r.flags & PART).collect();
    assert_eq!(parts, vec![PART, PART, 0]);
}

#[test]
fn jnl_033_raw_roll() {
    let mut h = H::open0(r0(&mem_hot(), &cold_dir()));
    for i in 0..5 {
        h.w()
            .append_var(EventKind::RAW_RX, F0, Tsc(1), &[&raw_body(100, 0, i)])
            .unwrap();
    }
    let b = h.roll_boot(RollReason::Hourly);
    let s = b.finish().unwrap();
    let s1 = h.seg(1);
    let c1 = h.committed(1) as usize;
    assert_eq!(roll_body(&s1, c1 - 80).mark, 1);
    let s2 = h.seg(2);
    let rs = records(&s2, s2.len());
    assert_eq!(rs.len(), 1);
    assert_eq!(
        (rs[0].1.kind, rs[0].1.flags & EVENT_END, rs[0].1.seq),
        (0x0509, EVENT_END, s.0)
    );
    assert_eq!(roll_body(&s2, 4096).mark, 2);
}

#[test]
fn jnl_034_next_not_ready() {
    let mut h = H::open0(mem_c0());
    assert!(matches!(h.try_roll(RollReason::Size), Err(RollErr::NextNotReady)));
    let s1 = h.seg(1);
    let rs = records(&s1, s1.len());
    assert_ne!(roll_body(&s1, rs.last().unwrap().0).mark, 1);
    assert_eq!(h.state(1), 1);
    let o = h.w().offset() as usize;
    h.w().append(F0, Tsc(1), &tin(1)).unwrap();
    h.w().end_event();
    assert_eq!(rh(&h.seg(1), o).kind, 0x7F01);
    h.roll(RollReason::Size);
    let s1 = h.seg(1);
    let end = rh(&s1, h.committed(1) as usize - 72);
    assert_eq!(h.hdr(2).first_seq, end.seq + 1);
}

#[test]
fn jnl_035_bad_prepared_segment() {
    let mut h = H::open0(mem_c0());
    let p2 = h.seg_path(2);
    let mut name = [0u8; 32];
    name[..15].copy_from_slice(b"raw-gw-okx-swap");
    let rows: [(&str, usize, Vec<u8>); 4] = [
        ("JNL-035.1", 64, 3u64.to_le_bytes().to_vec()),
        ("JNL-035.2", 16, name.to_vec()),
        ("JNL-035.3", 3088, 1u32.to_le_bytes().to_vec()),
        ("JNL-035.4", 0, b"HFTJRNL2".to_vec()),
    ];
    for (row, off, bytes) in rows {
        assert_eq!(h.prep.poll(), PrepState::Prepared, "{row}");
        testing::poke(&p2, off, &bytes).unwrap();
        assert!(matches!(h.try_roll(RollReason::Size), Err(RollErr::Io)), "{row}");
        assert_eq!(h.state(1), 1, "{row}");
        let s1 = h.seg(1);
        let rs = records(&s1, s1.len());
        assert_ne!(roll_body(&s1, rs.last().unwrap().0).mark, 1, "{row}");
        assert!(
            matches!(h.try_roll(RollReason::Size), Err(RollErr::NextNotReady)),
            "{row}: channel empty"
        );
        testing::unlink(&p2).unwrap();
    }
    assert_eq!(h.prep.poll(), PrepState::Prepared, "JNL-035.5");
    assert!(h.try_roll(RollReason::Size).is_ok(), "JNL-035.5");
}

#[test]
fn jnl_036_seal_and_close() {
    let mut h = H::open0(mem_c0());
    h.write_to_seq(7);
    h.w.take().unwrap().seal_and_close().unwrap();
    let s1 = h.seg(1);
    let c = h.committed(1) as usize;
    let e = rh(&s1, c - 72);
    let r = roll_body(&s1, c - 72);
    assert_eq!(
        (r.mark, r.segment_no, r.other_segment_no, r.seq_mark, r.reason),
        (1, 1, 0, 7, 5)
    );
    assert_eq!((e.seq, e.flags & EVENT_END, h.state(1)), (8, EVENT_END, 2));
    h.crash();
    let rep = h.open().unwrap();
    assert_eq!(
        (
            rep.last_seq,
            rep.repaired_segments,
            rep.bytes_discarded,
            rep.gap_possible,
            rep.boot_reason
        ),
        (Seq(8), 0, 0, false, RollReason::Startup)
    );
}

#[test]
fn jnl_037_many_configs() {
    let mut h = H::open0(mem_c0());
    h.configs = (0..40).map(blob).collect();
    h.roll(RollReason::Hourly);
    let f = h.hdr(2);
    let mut d = Sha256::new();
    for b in &h.configs {
        d.update(b.as_bytes());
    }
    let digest: [u8; 32] = d.finalize().into();
    assert_eq!((f.n_config, f.config_digest), (40, digest));
}

/// Stub state machine: xxh3-128 over every Input (`TIn`) body, chained.
fn stub_step(state: u128, body: &[u8]) -> u128 {
    let mut b = state.to_le_bytes().to_vec();
    b.extend_from_slice(body);
    xxhash_rust::xxh3::xxh3_128(&b)
}

fn boot_snapshot(state: u128) -> Vec<u8> {
    let mut sh = SnapshotHdr::new_zeroed();
    sh.purpose = 1;
    let mut v = sh.as_bytes().to_vec();
    v.extend_from_slice(&state.to_le_bytes());
    v
}

/// Reads to the end; `from_boot` = restore the stub from the first boot snapshot.
fn replay(r: &mut JournalReader, from_boot: bool) -> u128 {
    let mut st = 0u128;
    let mut restored = !from_boot;
    loop {
        match r.poll() {
            Next::Record(v) if v.hdr.kind == EventKind::T_IN.0 => st = stub_step(st, v.body),
            Next::Event(e) if e.kind == EventKind::STATE_SNAPSHOT && !restored => {
                st = u128::from_le_bytes(e.body[96..112].try_into().unwrap());
                restored = true;
            }
            Next::Record(_) | Next::Event(_) => {}
            Next::Pending | Next::End => return st,
            o => panic!("{o:?}"),
        }
    }
}

#[test]
fn jnl_120_boot_blocks_self_sufficient() {
    let mut cfg = mem_c0();
    cfg.segment_bytes = 256 << 10;
    let mut h = H::new(cfg);
    let mut st = 0u128;
    let boot = |h: &mut H, reason: RollReason, st: u128, prepare: bool| {
        if prepare {
            assert_ne!(h.prep.poll(), PrepState::Failed);
        }
        let mut b = h.try_roll(reason).unwrap();
        b.append_var(EventKind::STATE_SNAPSHOT, Tsc(1), &[&boot_snapshot(st)])
            .unwrap();
        b.finish().unwrap();
    };
    boot(&mut h, RollReason::Startup, st, true);
    let mut not_ready = 0;
    for i in 0..20_000u32 {
        let mut body = tin(0);
        body.bytes[..4].copy_from_slice(&i.to_le_bytes());
        h.w().append(F0, Tsc(u64::from(i)), &body).unwrap();
        h.w().append(F0, Tsc(u64::from(i)), &tout(1)).unwrap();
        h.w().end_event();
        h.w().publish();
        st = stub_step(st, &body.bytes);
        let hourly = i > 0 && i % 3_000 == 0;
        let reason = if hourly {
            Some(RollReason::Hourly)
        } else {
            h.w().needs_roll(Tsc(0)).map(|(r, _)| r)
        };
        if let Some(reason) = reason {
            if not_ready < 2 && i > 5_000 && !hourly {
                assert!(matches!(h.try_roll(reason), Err(RollErr::NextNotReady)));
                not_ready += 1;
            }
            boot(&mut h, reason, st, true);
        }
    }
    assert_eq!(not_ready, 2);
    let mut a = h.reader(StartAt::Oldest);
    let h_a = replay(&mut a, false);
    assert_eq!(h_a, st);
    let n = h.w().segment_no();
    let mut reasons = std::collections::BTreeSet::new();
    for k in 1..=n {
        let s = h.seg(k);
        let rs = records(&s, h.committed(k) as usize);
        assert_eq!(roll_body(&s, rs[0].0).mark, 2, "segment {k} starts with Begin");
        assert!(
            rs.iter()
                .any(|(o, r)| r.kind == 0x0509 && roll_body(&s, *o).mark == 3),
            "segment {k} has BootDone"
        );
        reasons.insert(h.hdr(k).roll_reason);
        let mut b = h.reader(StartAt::SegmentBoot(k));
        assert_eq!(replay(&mut b, true), h_a, "segment {k}");
    }
    assert!(reasons.contains(&2) && reasons.contains(&1), "{reasons:?}");
}
