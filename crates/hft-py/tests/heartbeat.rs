//! `hft_py.heartbeat`: read-only view of hft-rt's `HeartbeatBlock` (TESTS.md PY-090–PY-094).
//! The block is created with hft-rt's writer API under a temporary root (fixture `HB`).

#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{Seek, SeekFrom, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::Duration;

use hft_py::Error;
use hft_py::heartbeat::{Watch, block_path, mono_ns, read, state_name};
use hft_rt::{HeartbeatBlock, HkLine};
use sha2::{Digest, Sha256};

const D: i64 = 1_790_812_800_000_000_000;
/// Offset of `HkLine` in the 4 KiB block (zone 1).
const HK: u64 = 128;

/// `<tmp>/shm` with `prod/heartbeat` created by `HeartbeatBlock::create(…, 1)`.
fn hb(name: &str) -> (PathBuf, &'static HeartbeatBlock) {
    let root = std::env::temp_dir().join(format!("hftpy-hb-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("prod")).unwrap();
    let b = HeartbeatBlock::create(&root.join("prod").join("heartbeat"), 1).unwrap();
    (root, b)
}

fn patch(p: &Path, off: u64, bytes: &[u8]) {
    let mut f = std::fs::OpenOptions::new().write(true).open(p).unwrap();
    f.seek(SeekFrom::Start(off)).unwrap();
    f.write_all(bytes).unwrap();
}

#[test]
fn py_090_read_fields() {
    let (root, b) = hb("090");
    b.set_state(HkLine::RUNNING);
    for _ in 0..5 {
        b.sequencer().beat();
    }
    b.hk.0.hk_utc_ns.store(D + 60_000_000_000, Ordering::Release);
    let before = mono_ns();
    let r = read("prod", &root).unwrap();
    let after = mono_ns();
    assert_eq!(state_name(r.state), "Running");
    assert_eq!(r.seq_beats, 5);
    assert_eq!(r.instance_id, 1);
    assert_eq!(r.n_slots, 0);
    assert_eq!(r.pid, std::process::id());
    assert_eq!(r.hk_utc_ns, 1_790_812_860_000_000_000);
    let hex = hft_py::heartbeat::hex(&r.boot_id);
    assert_eq!(hex.len(), 32);
    assert!(
        hex.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    );
    assert!(
        before <= r.mono_ns && r.mono_ns <= after,
        "{before} {} {after}",
        r.mono_ns
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn py_091_state_names() {
    let (root, b) = hb("091");
    for (row, code, want) in [
        ("PY-091.1", 1, "Starting"),
        ("PY-091.2", 2, "Warmup"),
        ("PY-091.3", 3, "Running"),
        ("PY-091.4", 4, "Draining"),
        ("PY-091.5", 5, "Stopped"),
    ] {
        b.set_state(code);
        assert_eq!(state_name(read("prod", &root).unwrap().state), want, "{row}");
    }
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn py_092_read_errors() {
    let root = std::env::temp_dir().join(format!("hftpy-hb-{}-092a", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    assert!(matches!(read("prod", &root), Err(Error::NotFound(_))), "PY-092.1");
    let (root2, _) = hb("092b");
    let p = block_path("prod", &root2).unwrap();
    patch(&p, HK, b"HFTHB\0\x02\0");
    assert!(matches!(read("prod", &root2), Err(Error::Schema(_))), "PY-092.2");
    let (root3, _) = hb("092c");
    let p3 = block_path("prod", &root3).unwrap();
    patch(&p3, HK + 8, &2u32.to_le_bytes());
    assert!(matches!(read("prod", &root3), Err(Error::Schema(_))), "PY-092.3");
    let (root4, _) = hb("092d");
    assert!(read("prod", &root4).is_ok(), "PY-092.4");
    for r in [root, root2, root3, root4] {
        let _ = std::fs::remove_dir_all(r);
    }
}

#[test]
fn py_093_watch_age() {
    let (root, b) = hb("093");
    b.set_state(HkLine::RUNNING);
    let mut w = Watch::new("prod", &root).unwrap();
    let r1 = w.check().unwrap();
    assert_eq!(
        (state_name(r1.state).as_str(), r1.advanced, r1.age_ms),
        ("Running", true, 0)
    );
    std::thread::sleep(Duration::from_millis(1500));
    let r2 = w.check().unwrap();
    assert!(!r2.advanced);
    assert!((1500..=1550).contains(&r2.age_ms), "{}", r2.age_ms);
    b.sequencer().beat();
    let r3 = w.check().unwrap();
    assert!(r3.advanced && r3.age_ms <= 5, "{r3:?}");
    let b2 = HeartbeatBlock::create(&root.join("prod").join("heartbeat"), 1).unwrap();
    b2.set_state(HkLine::STARTING);
    let r4 = w.check().unwrap();
    assert!(r4.advanced && r4.age_ms <= 5, "{r4:?}");
    assert_eq!(state_name(r4.state), "Starting");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn py_094_read_is_read_only() {
    let (root, b) = hb("094");
    b.set_state(HkLine::RUNNING);
    b.sequencer().beat();
    let p = block_path("prod", &root).unwrap();
    let digest = |p: &Path| -> [u8; 32] { Sha256::digest(std::fs::read(p).unwrap()).into() };
    let (h0, m0) = (digest(&p), std::fs::metadata(&p).unwrap().modified().unwrap());
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o440)).unwrap();
    let mut w = Watch::new("prod", &root).unwrap();
    for _ in 0..100 {
        read("prod", &root).unwrap();
        w.check().unwrap();
    }
    assert_eq!(digest(&p), h0);
    assert_eq!(std::fs::metadata(&p).unwrap().modified().unwrap(), m0);
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o640)).unwrap();
    let _ = std::fs::remove_dir_all(root);
}
