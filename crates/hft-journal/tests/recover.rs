//! Recovery steps 1–6 (JNL-055 – JNL-068). Linux rows use tmpfs; elsewhere the heap backend.

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
use hft_types::{Seq, Tsc};
use sha2::{Digest, Sha256};
use zerocopy::{FromZeros, IntoBytes};

fn masked(b: &[u8]) -> Vec<u8> {
    let mut v = b.to_vec();
    for x in v.iter_mut().take(3200).skip(3072) {
        *x = 0;
    }
    v
}

/// One event `[TIn, TOut]`, `end_event`, optionally published.
fn event(h: &mut H, publish: bool) {
    let w = h.w();
    w.append(F0, Tsc(1), &tin(1)).unwrap();
    w.append(F0, Tsc(1), &tout(1)).unwrap();
    w.end_event();
    if publish {
        w.publish();
    }
}

#[test]
fn jnl_055_preparing_deleted() {
    let mut h = H::open0(mem_c0());
    h.write_to_seq(20);
    assert_eq!(h.prep.poll(), PrepState::Prepared);
    assert!(testing::exists(&h.seg_path(2)));
    h.crash();
    let rep = h.open().unwrap();
    assert!(!testing::exists(&h.seg_path(2)));
    assert_eq!(rep.boot_reason, RollReason::Recovery);
    h.roll(rep.boot_reason);
    assert_eq!(h.w().segment_no(), 2);
}

#[test]
fn jnl_056_incomplete_boot_removed() {
    let mut h = H::open0(linux_c0());
    h.write_to_seq(30);
    {
        let mut b = h.roll_boot(RollReason::Hourly);
        let mut sh = SnapshotHdr::new_zeroed();
        sh.purpose = 1;
        let mut body = sh.as_bytes().to_vec();
        body.extend(multi_body(70_000));
        b.append_var(EventKind::STATE_SNAPSHOT, Tsc(1), &[&body]).unwrap();
        std::mem::forget(b);
    }
    h.persist_all();
    let cold2 = h.cold_stem(2).with_extension("jrn");
    assert!(cold2.exists());
    let s1 = masked(&h.seg(1));
    let end_seq = rh(&h.seg(1), h.committed(1) as usize - 72).seq;
    h.crash();
    let rep = h.open().unwrap();
    assert!(!testing::exists(&h.seg_path(2)));
    assert!(!cold2.exists());
    assert_eq!(masked(&h.seg(1)), s1);
    assert!(rep.repaired_segments >= 1);
    h.roll(rep.boot_reason);
    let f = h.hdr(2);
    assert_eq!(
        (h.w().segment_no(), f.first_seq, f.roll_reason),
        (2, end_seq + 1, 4)
    );
}

#[test]
fn jnl_057_truncate_half_event() {
    let mut h = H::open0(mem_c0());
    event(&mut h, true);
    let c1 = h.committed(1) as usize;
    event(&mut h, false);
    let e2_last = h.w().next_seq().0 - 1;
    h.w().append(F0, Tsc(1), &tin(1)).unwrap();
    h.w().append(F0, Tsc(1), &tout(1)).unwrap();
    h.crash();
    let rep = h.open().unwrap();
    let s = h.seg(1);
    let end = c1 + 184;
    assert_eq!(rh(&s, end).kind, EventKind::SEGMENT_ROLL.0);
    assert_eq!(h.committed(1) as usize, end + 72);
    assert!(s[end + 72..end + 184 + 72].iter().all(|b| *b == 0));
    assert_eq!((rep.bytes_discarded, rep.last_seq), (184, Seq(e2_last + 1)));
}

#[test]
fn jnl_058_scan_stops_at_invalid() {
    type Tamper = Box<dyn Fn(&H, usize, usize)>;
    let poke = |h: &H, off: usize, b: &[u8]| testing::poke(&h.seg_path(1), off, b).unwrap();
    let rows: Vec<(&str, Tamper, bool)> = vec![
        ("JNL-058.1", Box::new(|_, _, _| {}), true),
        (
            "JNL-058.2",
            Box::new(move |h, e3, _| poke(h, e3, &0u32.to_le_bytes())),
            false,
        ),
        (
            "JNL-058.3",
            Box::new(move |h, e3, _| poke(h, e3, &100u32.to_le_bytes())),
            false,
        ),
        (
            "JNL-058.4",
            Box::new(move |h, e3, _| poke(h, e3, &65_544u32.to_le_bytes())),
            false,
        ),
        (
            "JNL-058.5",
            Box::new(move |h, e3, _| poke(h, e3, &16u32.to_le_bytes())),
            false,
        ),
        (
            "JNL-058.6",
            Box::new(move |h, e3, _| {
                let s = u64_at(&h.seg(1), e3 + 8);
                poke(h, e3 + 8, &(s + 1).to_le_bytes())
            }),
            false,
        ),
        (
            "JNL-058.7",
            Box::new(move |h, e3, _| poke(h, e3 + 4, &0x7FEEu16.to_le_bytes())),
            false,
        ),
        (
            "JNL-058.8",
            Box::new(move |h, e3, _| poke(h, e3 + 4, &0x0601u16.to_le_bytes())),
            false,
        ),
        (
            "JNL-058.9",
            Box::new(move |h, _, crc_end| {
                let b = h.seg(1)[crc_end - 1];
                poke(h, crc_end - 1, &[b ^ 1])
            }),
            false,
        ),
    ];
    for (row, tamper, keep_e3) in rows {
        let mut h = H::open0(mem_c0());
        event(&mut h, true);
        event(&mut h, false);
        let e2_end = h.w().offset() as usize;
        h.w().append(F0, Tsc(1), &tin(1)).unwrap();
        h.w().append(F0, Tsc(1), &tcrc(1)).unwrap();
        h.w().end_event();
        let e3_end = h.w().offset() as usize;
        tamper(&h, e2_end, e3_end);
        h.crash();
        h.open().unwrap();
        let want = if keep_e3 { e3_end } else { e2_end };
        assert_eq!(h.committed(1) as usize, want + 72, "{row}");
        assert_eq!(rh(&h.seg(1), want).kind, EventKind::SEGMENT_ROLL.0, "{row}");
    }
}

#[test]
fn jnl_059_truncation_not_below_committed() {
    let mut h = H::open0(mem_c0());
    let m;
    {
        let w = h.w();
        w.append(F0, Tsc(1), &tin(1)).unwrap();
        w.append(F0, Tsc(1), &tout(1)).unwrap();
        m = w.offset() as usize;
        w.append_meta(Tsc(1), &tmeta(1)).unwrap();
        w.end_event();
        w.publish();
    }
    let c = h.committed(1);
    assert_eq!(c as usize, m + 56);
    h.w().append(F0, Tsc(1), &tin(9)).unwrap();
    h.crash();
    h.open().unwrap();
    let s = h.seg(1);
    assert_eq!(rh(&s, c as usize).kind, EventKind::SEGMENT_ROLL.0);
    assert_eq!(rh(&s, m).flags, 0);
}

#[test]
fn jnl_060_incomplete_group_dropped() {
    let mut h = H::open0(mem_c0());
    event(&mut h, true);
    let c1 = h.committed(1) as usize;
    h.w().append(F0, Tsc(1), &tin(1)).unwrap();
    let body = multi_body(3 * 65_488);
    for p in 0..2u16 {
        let ph = PartHdr {
            total_len: body.len() as u32,
            chunk_len: 65_488,
            part: p,
            parts: 3,
            _rsv: 0,
        };
        let chunk = &body[p as usize * 65_488..(p as usize + 1) * 65_488];
        h.w()
            .append_part_raw(EventKind::T_MULTI, RecordFlags(PART), Tsc(1), ph, chunk)
            .unwrap();
    }
    let dirty = h.w().offset() as usize;
    h.crash();
    let rep = h.open().unwrap();
    let s = h.seg(1);
    assert_eq!(rh(&s, c1).kind, EventKind::SEGMENT_ROLL.0);
    assert!(s[c1 + 72..dirty].iter().all(|b| *b == 0));
    assert_eq!(rep.bytes_discarded, 104 + 2 * 65_528);
}

#[test]
fn jnl_061_zero_to_clean_page() {
    let mut h = H::open0(mem_c0());
    event(&mut h, true);
    let c1 = h.committed(1) as usize;
    testing::poke(&h.seg_path(1), c1, &[0xFF; 6000]).unwrap();
    h.crash();
    h.open().unwrap();
    let s = h.seg(1);
    assert_eq!(rh(&s, c1).kind, EventKind::SEGMENT_ROLL.0);
    assert!(s[c1 + 72..c1 + 6000].iter().all(|b| *b == 0));
}

#[test]
fn jnl_062_recopy_before_writes() {
    let mut h = H::open0(linux_c0());
    h.events(2000);
    let c = h.committed(1);
    h.persister(PersistConfig {
        chunk_bytes: (c / 2) as u32,
        sync_bytes: 1,
        sync_interval_ms: 20,
    });
    h.pers.poll(1, h.utc);
    let half = h.persisted(1);
    assert_eq!(h.synced(1), half);
    h.crash();
    testing::oplog_start();
    let rep = h.open().unwrap();
    let ops = testing::oplog_take();
    let sync_at = ops.iter().position(|o| *o == Op::Sync).unwrap();
    let end_at = ops
        .iter()
        .position(|o| matches!(o, Op::Append(0x0509, _)))
        .unwrap();
    assert!(sync_at < end_at, "{ops:?}");
    let cold = std::fs::read(h.cold_stem(1).with_extension("jrn")).unwrap();
    let hot = h.seg(1);
    assert_eq!(masked(&cold[..c as usize]), masked(&hot[..c as usize]));
    assert_eq!(rep.bytes_copied, c - half);
}

#[test]
fn jnl_063_end_recovery() {
    let mut h = H::open0(mem_c0());
    h.write_to_seq(40);
    h.crash();
    let rep = h.open().unwrap();
    h.roll(rep.boot_reason);
    let s = h.seg(1);
    let c = h.committed(1) as usize;
    let e = rh(&s, c - 72);
    let r = roll_body(&s, c - 72);
    assert_eq!(
        (
            r.mark,
            r.segment_no,
            r.other_segment_no,
            r.seq_mark,
            r.reason,
            e.seq
        ),
        (1, 1, 2, 40, 4, 41)
    );
    assert_eq!(h.state(1), 2);
    assert_eq!((rep.boot_reason, rep.last_seq), (RollReason::Recovery, Seq(41)));
    let f = h.hdr(2);
    assert_eq!((f.roll_reason, f.first_seq, f.flags), (4, 42, 0));
}

fn delete_hot(h: &H) {
    for p in testing::hot_files(&h.hot_jdir()) {
        testing::unlink(&p).unwrap();
    }
}

#[test]
fn jnl_064_cold_sealed_clean() {
    let mut h = H::open0(linux_c0());
    h.write_to_seq(60);
    h.w.take().unwrap().seal_and_close().unwrap();
    h.persist_all();
    let stem = h.cold_stem(1);
    assert!(stem.with_extension("seal").exists());
    delete_hot(&h);
    h.crash();
    let rep = h.open().unwrap();
    assert_eq!(
        (rep.gap_possible, rep.boot_reason, rep.last_seq),
        (false, RollReason::Startup, Seq(61))
    );
    h.roll(rep.boot_reason);
    let f = h.hdr(2);
    let cold = std::fs::read(stem.with_extension("jrn")).unwrap();
    let sha: [u8; 32] = Sha256::digest(&cold[..3072]).into();
    assert_eq!((f.flags, f.first_seq, f.prev_header_sha256), (0, 62, sha));
}

#[test]
fn jnl_065_cold_unsealed_gap() {
    let mut h = H::open0(linux_c0());
    h.events(3000);
    let c = h.committed(1);
    // Sync up to s, then copy the rest without syncing.
    h.persister(PersistConfig {
        chunk_bytes: 300_000,
        sync_bytes: 1,
        sync_interval_ms: 20,
    });
    h.pers.poll(1, h.utc);
    let s = h.synced(1) as usize;
    h.persister(PersistConfig {
        chunk_bytes: 4 << 20,
        sync_bytes: 1 << 30,
        sync_interval_ms: 1_000_000,
    });
    h.pers.poll(2, h.utc);
    assert_eq!((h.persisted(1), h.synced(1) as usize), (c, s));
    let hot = h.seg(1);
    let stem = h.cold_stem(1);
    let jrn = stem.with_extension("jrn");
    // The cold copy's header row says synced_len = s (as after power loss); one page zeroed.
    let f = std::fs::OpenOptions::new().write(true).open(&jrn).unwrap();
    write_at(&f, (s + 4000) as u64, &[0u8; 4096]);
    write_at(&f, 3144, &(s as u64).to_le_bytes());
    drop(f);
    // Expected cut: end of the last complete event wholly before s + 4000.
    let mut cut = 4096;
    for (o, r) in records(&hot, c as usize) {
        if o + r.len as usize <= s + 4000 && r.flags & EVENT_END != 0 {
            cut = o + r.len as usize;
        }
    }
    delete_hot(&h);
    h.crash();
    let rep = h.open().unwrap();
    let cold = std::fs::read(&jrn).unwrap();
    assert_eq!(cold.len(), cut + 72);
    assert_eq!(roll_body(&cold, cut).reason, 4);
    assert!(stem.with_extension("seal").exists());
    assert_eq!((rep.gap_possible, rep.boot_reason), (true, RollReason::Recovery));
    h.roll(rep.boot_reason);
    assert_eq!(h.hdr(2).flags, RECOVERY_GAP);
}

fn write_at(f: &std::fs::File, off: u64, b: &[u8]) {
    #[cfg(unix)]
    std::os::unix::fs::FileExt::write_all_at(f, b, off).unwrap();
    #[cfg(windows)]
    {
        let mut done = 0;
        while done < b.len() {
            done += std::os::windows::fs::FileExt::seek_write(f, &b[done..], off + done as u64).unwrap();
        }
    }
}

fn boot_with_snapshot(h: &mut H, reason: RollReason, finish: bool) {
    let mut b = h.roll_boot(reason);
    let mut sh = SnapshotHdr::new_zeroed();
    sh.purpose = 1;
    b.append_var(EventKind::STATE_SNAPSHOT, Tsc(1), &[sh.as_bytes()])
        .unwrap();
    if finish {
        b.finish().unwrap();
    } else {
        std::mem::forget(b);
    }
}

fn keyframe(h: &mut H) -> Position {
    let mut sh = SnapshotHdr::new_zeroed();
    sh.purpose = 2;
    let w = h.w();
    let p = Position {
        segment_no: w.segment_no(),
        offset: w.offset(),
        seq: w.next_seq().0,
    };
    w.append_var(EventKind::STATE_SNAPSHOT, F0, Tsc(1), &[sh.as_bytes()])
        .unwrap();
    w.end_event();
    w.publish();
    p
}

#[test]
fn jnl_066_rebuild_from() {
    let begin1 = Position {
        segment_no: 1,
        offset: 4096,
        seq: 1,
    };
    let mut h = H::new(mem_c0());
    boot_with_snapshot(&mut h, RollReason::Startup, true);
    h.events(100);
    h.crash();
    assert_eq!(h.open().unwrap().rebuild_from, begin1, "JNL-066.1");

    let build = |more: Option<bool>| {
        let mut h = H::new(mem_c0());
        boot_with_snapshot(&mut h, RollReason::Startup, true);
        h.events(50);
        let p = keyframe(&mut h);
        h.events(50);
        if let Some(complete) = more {
            boot_with_snapshot(&mut h, RollReason::Hourly, complete);
            if complete {
                h.events(10);
            }
        }
        h.crash();
        (h.open().unwrap().rebuild_from, p)
    };
    let (r, p) = build(None);
    assert_eq!(r, p, "JNL-066.2");
    let (r, _) = build(Some(true));
    assert_eq!(
        r,
        Position {
            segment_no: 2,
            offset: 4096,
            seq: r.seq
        },
        "JNL-066.3"
    );
    let (r, p) = build(Some(false));
    assert_eq!(r, p, "JNL-066.4");
}

/// Segments 1–5 (1–4 sealed and persisted, 5 active and persisted).
fn five() -> H {
    let mut h = H::open0(linux_c0());
    for _ in 0..4 {
        h.events(3);
        h.roll(RollReason::Hourly);
    }
    h.events(3);
    h.persist_all();
    h
}

fn reheader(h: &H, no: u64, off: usize, bytes: &[u8]) {
    let p = h.seg_path(no);
    testing::poke(&p, off, bytes).unwrap();
    let img = testing::read(&p).unwrap();
    testing::poke(&p, 3068, &crc32c::crc32c(&img[..3068]).to_le_bytes()).unwrap();
}

fn remove_cold(h: &H, no: u64) {
    let stem = h.cold_stem(no);
    for e in ["jrn", "seal"] {
        let _ = std::fs::remove_file(stem.with_extension(e));
    }
}

#[test]
fn jnl_067_chain_and_conflict() {
    let reopen = |mut h: H| {
        h.crash();
        h.open().map(|_| ())
    };
    let h = five();
    remove_cold(&h, 4);
    testing::unlink(&h.seg_path(4)).unwrap();
    assert_eq!(reopen(h), Err(OpenErr::ChainBroken), "JNL-067.1");

    let h = five();
    let mut sha = h.hdr(5).prev_header_sha256;
    sha[0] ^= 1;
    reheader(&h, 5, 160, &sha);
    assert_eq!(reopen(h), Err(OpenErr::ChainBroken), "JNL-067.2");

    let h = five();
    let fs = h.hdr(5).first_seq;
    reheader(&h, 5, 72, &(fs + 1).to_le_bytes());
    assert_eq!(reopen(h), Err(OpenErr::ChainBroken), "JNL-067.3");

    let h = five();
    let jrn = h.cold_stem(5).with_extension("jrn");
    reheader(&h, 5, 72, &(fs + 1).to_le_bytes());
    reheader(&h, 5, 117, &[RECOVERY_GAP]);
    // The cold copy carries the same header (it was written at roll time).
    let jrn2 = h.cold_stem(5).with_extension("jrn");
    std::fs::rename(&jrn, &jrn2).unwrap();
    let f = std::fs::OpenOptions::new().write(true).open(&jrn2).unwrap();
    write_at(&f, 0, &h.seg(5)[..3072]);
    drop(f);
    assert_eq!(reopen(h), Ok(()), "JNL-067.4");

    let h = five();
    let b = h.seg(4)[4300];
    testing::poke(&h.seg_path(4), 4300, &[b ^ 1]).unwrap();
    assert_eq!(reopen(h), Err(OpenErr::ColdHotConflict), "JNL-067.5");

    let h = five();
    let jrn = h.cold_stem(4).with_extension("jrn");
    let f = std::fs::OpenOptions::new().write(true).open(&jrn).unwrap();
    write_at(&f, 3100, &[0x5A; 8]);
    drop(f);
    assert_eq!(reopen(h), Ok(()), "JNL-067.6");

    let h = five();
    testing::poke(&h.seg_path(4), 3144, &4240u64.to_le_bytes()).unwrap();
    remove_cold(&h, 4);
    let before = h.seg(4);
    let p4 = h.seg_path(4);
    assert_eq!(reopen(h), Err(OpenErr::ColdHotConflict), "JNL-067.7");
    assert_eq!(testing::read(&p4).unwrap(), before, "JNL-067.7");

    let h = five();
    testing::poke(&h.seg_path(4), 3144, &4240u64.to_le_bytes()).unwrap();
    let jrn = h.cold_stem(4).with_extension("jrn");
    std::fs::OpenOptions::new()
        .write(true)
        .open(&jrn)
        .unwrap()
        .set_len(4232)
        .unwrap();
    assert_eq!(reopen(h), Err(OpenErr::ColdHotConflict), "JNL-067.8");

    let h = five();
    testing::poke(&h.seg_path(4), 3144, &0u64.to_le_bytes()).unwrap();
    remove_cold(&h, 4);
    assert_eq!(reopen(h), Ok(()), "JNL-067.9");
}

#[test]
fn jnl_068_empty_and_disk() {
    let mut h = H::bare(linux_c0(), false);
    let rep = h.open().unwrap();
    assert_eq!(
        (
            rep.last_seq,
            rep.repaired_segments,
            rep.bytes_copied,
            rep.bytes_discarded,
            rep.gap_possible,
            rep.boot_reason
        ),
        (Seq(0), 0, 0, 0, false, RollReason::Startup),
        "JNL-068.1"
    );
    let mut h = H::bare(c0(&disk_dir("ext4hot"), &cold_dir()), false);
    assert_eq!(h.open().err(), Some(OpenErr::DiskBacked), "JNL-068.2");
}
