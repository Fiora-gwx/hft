//! Segment preparation (JNL-039 – JNL-042). Linux rows use tmpfs; elsewhere the heap backend.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod common;

use common::harness::*;
use hft_event::{KINDS, Layout};
use hft_journal::testing;
use hft_journal::*;
use sha2::{Digest, Sha256};

#[test]
fn jnl_039_disk_backed_rejected() {
    let disk = disk_dir("ext4hot");
    let cfg = c0(&disk, &cold_dir());
    let mut h = H::bare(cfg.clone(), false);
    assert_eq!(h.open().err(), Some(OpenErr::DiskBacked), "JNL-039.1");
    assert_eq!(h.prep.poll(), PrepState::Failed, "JNL-039.1");
    assert_eq!(
        std::fs::read_dir(&disk).unwrap().count(),
        0,
        "JNL-039.1: no new files"
    );
    let mut h = H::bare(c0(&linux_hot(), &cold_dir()), false);
    assert!(h.open().is_ok(), "JNL-039.2");
    let mut h = H::bare(c0(&scratch("missing"), &cold_dir()), false);
    assert_eq!(h.open().err(), Some(OpenErr::Io), "JNL-039.3");
}

#[cfg(target_os = "linux")]
fn vm_lck_kib() -> u64 {
    let s = std::fs::read_to_string("/proc/self/status").unwrap();
    let l = s.lines().find(|l| l.starts_with("VmLck:")).unwrap();
    l.split_whitespace().nth(1).unwrap().parse().unwrap()
}

#[test]
fn jnl_040_prepared_segment_header() {
    let mut h = H::open0(linux_c0());
    #[cfg(target_os = "linux")]
    let before = vm_lck_kib();
    assert_eq!(h.prep.poll(), PrepState::Prepared);
    let p = h.seg_path(2);
    let img = testing::read(&p).unwrap();
    assert_eq!(img.len(), 1_048_576);
    let f = fixed(&img);
    assert_eq!(&f.magic, b"HFTJRNL1");
    assert_eq!(
        (f.format_version, f.journal_kind, f.venue.0, f.header_len),
        (1, 2, 0, 4096)
    );
    assert_eq!(&f.journal_name[..4], b"seq\0");
    assert_eq!(&f.instance[..5], b"prod\0");
    assert_eq!((f.segment_no, f.segment_bytes, f.writer_epoch), (2, 1_048_576, 7));
    let s1 = h.seg(1);
    let sha: [u8; 32] = Sha256::digest(&s1[..3072]).into();
    assert_eq!(f.prev_header_sha256, sha);
    assert_eq!((f.host_profile_sha256, f.build_git), ([0xCD; 32], [0xAB; 20]));
    assert_eq!(f.n_schema as usize, KINDS.len());
    for (e, k) in f.schema.iter().zip(KINDS.iter()) {
        let size = match k.layout {
            Layout::Fixed { size } => u32::from(size),
            Layout::Var { prefix, .. } | Layout::Multi { prefix } => u32::from(prefix),
        };
        assert_eq!((e.kind, e.version, e.size), (k.kind.0, k.schema_version, size));
    }
    assert_eq!(u32_at(&img, 3088), 0, "state = Preparing");
    assert!(img[4096..].iter().all(|b| *b == 0));
    #[cfg(target_os = "linux")]
    assert!(vm_lck_kib() >= before + 1024, "VmLck grew by the segment");
}

#[test]
fn jnl_041_first_segment() {
    let h = H::open0(mem_c0());
    let f = h.hdr(1);
    assert_eq!(
        (
            f.segment_no,
            f.prev_header_sha256,
            f.first_seq,
            f.prev_last_seq,
            f.roll_reason
        ),
        (1, [0u8; 32], 1, 0, 3)
    );
}

#[test]
fn jnl_042_one_prepared_at_a_time() {
    let mut h = H::open0(linux_c0());
    let dir = h.hot_jdir();
    assert_eq!(h.prep.poll(), PrepState::Prepared, "JNL-042.1");
    assert_eq!(testing::hot_files(&dir).len(), 2, "JNL-042.1");
    assert_eq!(h.prep.poll(), PrepState::Ready, "JNL-042.2");
    assert_eq!(testing::hot_files(&dir).len(), 2, "JNL-042.2");
    h.try_roll(RollReason::Hourly).unwrap().finish().unwrap();
    assert_eq!(h.prep.poll(), PrepState::Prepared, "JNL-042.3");
    assert!(testing::exists(&h.seg_path(3)), "JNL-042.3");

    let mut h = H::open0(linux_c0());
    let dir = h.hot_jdir();
    h.roll(RollReason::Hourly);
    let foreign = h.seg_path(3);
    testing::create(&foreign, b"not a segment").unwrap();
    assert_eq!(h.prep.poll(), PrepState::Failed, "JNL-042.4");
    assert_eq!(
        testing::read(&foreign).unwrap(),
        b"not a segment".to_vec(),
        "JNL-042.4"
    );
    assert_eq!(testing::hot_files(&dir).len(), 3, "JNL-042.4");
    testing::unlink(&foreign).unwrap();
    assert_eq!(h.prep.poll(), PrepState::Prepared, "JNL-042.5");
}
