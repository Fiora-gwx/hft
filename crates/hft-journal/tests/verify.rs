//! Offline verification (JNL-089, JNL-090). Linux rows use tmpfs; elsewhere the heap backend.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod common;

use std::path::PathBuf;

use common::harness::*;
use hft_event::{EventKind, PartHdr, RecordFlags, SnapshotHdr};
use hft_journal::*;
use hft_types::Tsc;
use zerocopy::{FromZeros, IntoBytes};

fn l2_keyframe() -> Vec<u8> {
    let mut v = vec![0u8; 72];
    v[64] = 2; // origin = Keyframe
    v
}

/// Segments 1, 2 sealed with `.seal`, segment 3 active; segment 1 holds `TCrc` records and,
/// with `broken_group`, a multi-part group missing its last part; segment 2's boot block holds a
/// `StateSnapshot{Boot, n_books = 2}` and two `L2Snapshot{Keyframe}`.
fn base(broken_group: bool) -> H {
    let mut h = H::open0(linux_c0());
    for i in 0..5 {
        h.w().append(F0, Tsc(1), &tin(i)).unwrap();
        h.w().append(F0, Tsc(1), &tcrc(i)).unwrap();
        h.w().end_event();
    }
    if broken_group {
        let body = multi_body(3 * 1000);
        for p in 0..2u16 {
            let ph = PartHdr {
                total_len: 3000,
                chunk_len: 1000,
                part: p,
                parts: 3,
                _rsv: 0,
            };
            let c = &body[p as usize * 1000..(p as usize + 1) * 1000];
            h.w()
                .append_part_raw(EventKind::T_MULTI, RecordFlags(PART), Tsc(1), ph, c)
                .unwrap();
        }
        h.w().append(F0, Tsc(1), &tin(9)).unwrap();
        h.w().end_event();
    }
    h.w().publish();
    let mut b = h.roll_boot(RollReason::Hourly);
    let mut sh = SnapshotHdr::new_zeroed();
    sh.purpose = 1;
    sh.n_books = 2;
    b.append_var(EventKind::STATE_SNAPSHOT, Tsc(1), &[sh.as_bytes()])
        .unwrap();
    b.append_var(EventKind::L2_SNAPSHOT, Tsc(1), &[&l2_keyframe()])
        .unwrap();
    b.append_var(EventKind::L2_SNAPSHOT, Tsc(1), &[&l2_keyframe()])
        .unwrap();
    b.finish().unwrap();
    h.events(5);
    h.roll(RollReason::Hourly);
    h.events(5);
    h.persist_all();
    h
}

fn cold(h: &H, no: u64, ext: &str) -> PathBuf {
    h.cold_stem(no).with_extension(ext)
}

fn patch(p: &PathBuf, off: usize, bytes: &[u8]) {
    let mut b = std::fs::read(p).unwrap();
    b[off..off + bytes.len()].copy_from_slice(bytes);
    std::fs::write(p, b).unwrap();
}

fn flip(p: &PathBuf, off: usize) {
    let b = std::fs::read(p).unwrap()[off];
    patch(p, off, &[b ^ 1]);
}

/// Patches a header field and recomputes `header_crc32c`.
fn patch_hdr(p: &PathBuf, off: usize, bytes: &[u8]) {
    patch(p, off, bytes);
    let b = std::fs::read(p).unwrap();
    patch(p, 3068, &crc32c::crc32c(&b[..3068]).to_le_bytes());
}

fn run(h: &H) -> VerifyReport {
    verify(&h.cold_jdir(), &VerifyOpts::default())
}

#[test]
fn jnl_089_verify_clean() {
    let h = base(false);
    let r = run(&h);
    assert!(r.issues.is_empty(), "{:?}", r.issues);
    assert!(r.missing.is_empty() && r.recovery_gaps.is_empty());
    assert_eq!(r.unsealed, vec![3]);
}

fn rec_off(h: &H, no: u64, pick: impl Fn(usize, &hft_event::RecordHeader, &[u8]) -> bool) -> usize {
    let img = std::fs::read(cold(h, no, "jrn")).unwrap();
    records(&img, img.len())
        .into_iter()
        .find(|(o, r)| pick(*o, r, &img))
        .map(|(o, _)| o)
        .unwrap()
}

#[test]
fn jnl_090_verify_detects() {
    let has =
        |r: &VerifyReport, no: u64, item: u8| r.issues.iter().any(|i| i.segment_no == no && i.item == item);
    type Row = (&'static str, u64, u8, Box<dyn Fn(&H)>);
    let rows: Vec<Row> = vec![
        (
            "JNL-090.1",
            2,
            1,
            Box::new(|h| patch(&cold(h, 2, "jrn"), 0, b"XFTJRNL1")),
        ),
        (
            "JNL-090.2",
            2,
            1,
            Box::new(|h| patch(&cold(h, 2, "jrn"), 8, &2u16.to_le_bytes())),
        ),
        ("JNL-090.3", 2, 1, Box::new(|h| flip(&cold(h, 2, "jrn"), 200))),
        (
            "JNL-090.5",
            3,
            2,
            Box::new(|h| {
                let p = cold(h, 3, "jrn");
                let b = std::fs::read(&p).unwrap()[160];
                patch_hdr(&p, 160, &[b ^ 1]);
            }),
        ),
        (
            "JNL-090.6",
            3,
            2,
            Box::new(|h| {
                let p = cold(h, 3, "jrn");
                let fs = u64_at(&std::fs::read(&p).unwrap(), 72);
                patch_hdr(&p, 72, &(fs + 1).to_le_bytes());
            }),
        ),
        (
            "JNL-090.8",
            1,
            3,
            Box::new(|h| {
                let o = rec_off(h, 1, |_, r, _| r.kind == 0x7F01);
                patch(&cold(h, 1, "jrn"), o, &100u32.to_le_bytes());
            }),
        ),
        (
            "JNL-090.9",
            1,
            3,
            Box::new(|h| {
                let o = rec_off(h, 1, |_, r, _| r.kind == 0x7F04);
                flip(&cold(h, 1, "jrn"), o + 68);
            }),
        ),
        (
            "JNL-090.11",
            2,
            4,
            Box::new(|h| patch(&cold(h, 2, "jrn"), 4096 + 24 + 32, &[1])),
        ),
        (
            "JNL-090.12",
            2,
            4,
            Box::new(|h| {
                let o = rec_off(h, 2, |o, r, b| r.kind == 0x0509 && b[o + 24 + 32] == 3);
                patch(&cold(h, 2, "jrn"), o + 24 + 32, &[1]);
            }),
        ),
        (
            "JNL-090.13",
            2,
            4,
            Box::new(|h| {
                let o = rec_off(h, 2, |o, r, b| r.kind == 0x0509 && b[o + 24 + 32] == 3);
                let p = cold(h, 2, "jrn");
                let n = u32_at(&std::fs::read(&p).unwrap(), o + 24 + 36);
                patch(&p, o + 24 + 36, &(n + 1).to_le_bytes());
            }),
        ),
        (
            "JNL-090.14",
            2,
            4,
            Box::new(|h| {
                let o = rec_off(h, 2, |_, r, _| r.kind == 0x0504);
                patch(&cold(h, 2, "jrn"), o + 24 + 16 + 32, &3u32.to_le_bytes());
            }),
        ),
        ("JNL-090.15", 1, 5, Box::new(|h| flip(&cold(h, 1, "seal"), 124))),
        ("JNL-090.16", 1, 5, Box::new(|h| flip(&cold(h, 1, "seal"), 48))),
        (
            "JNL-090.17",
            1,
            5,
            Box::new(|h| {
                let p = cold(h, 1, "seal");
                let b = std::fs::read(&p).unwrap();
                patch(&p, 24, &(u64_at(&b, 24) - 1).to_le_bytes());
                let b = std::fs::read(&p).unwrap();
                patch(&p, 124, &crc32c::crc32c(&b[..124]).to_le_bytes());
            }),
        ),
        (
            "JNL-090.18",
            1,
            5,
            Box::new(|h| {
                let p = cold(h, 1, "jrn");
                let n = std::fs::read(&p).unwrap().len();
                patch(&p, n - 72 + 4, &0x7F03u16.to_le_bytes());
            }),
        ),
    ];
    for (row, no, item, f) in rows {
        let h = base(false);
        f(&h);
        let r = run(&h);
        assert!(has(&r, no, item), "{row}: {:?}", r.issues);
    }
    let h = base(false);
    std::fs::remove_file(cold(&h, 2, "jrn")).unwrap();
    std::fs::remove_file(cold(&h, 2, "seal")).unwrap();
    assert_eq!(run(&h).missing, vec![(2, 2)], "JNL-090.4");
    let h = base(false);
    let p = cold(&h, 3, "jrn");
    let fs = u64_at(&std::fs::read(&p).unwrap(), 72);
    patch(&p, 72, &(fs + 1).to_le_bytes());
    patch_hdr(&p, 117, &[RECOVERY_GAP]);
    let r = run(&h);
    assert!(r.issues.is_empty(), "JNL-090.7: {:?}", r.issues);
    assert_eq!(r.recovery_gaps, vec![3], "JNL-090.7");
    let h = base(true);
    assert!(has(&run(&h), 1, 3), "JNL-090.10");
}
