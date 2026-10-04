//! HeartbeatBlock create / open_ro / claim / set_state (RT-095 – RT-098). Linux.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![cfg(all(target_os = "linux", not(loom)))]

#[allow(dead_code)]
mod common {
    pub mod proc;
}

use std::mem::offset_of;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use common::proc::*;
use hft_rt::{HeartbeatBlock, HkLine, RtErr};

fn path_env() -> PathBuf {
    PathBuf::from(env("HB"))
}

fn boot_id() -> [u8; 16] {
    let s = std::fs::read_to_string("/proc/sys/kernel/random/boot_id").unwrap();
    let hex: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    let mut out = [0u8; 16];
    for (i, o) in out.iter_mut().enumerate() {
        *o = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap();
    }
    out
}

#[test]
fn rt_095_create_and_open_ro() {
    match role().as_deref() {
        Some("reader") => {
            let b = HeartbeatBlock::open_ro(&path_env()).unwrap();
            let mut last = 0;
            for _ in 0..100 {
                let v = b.sequencer().read();
                if v < last {
                    exit(1);
                }
                last = v;
                std::thread::sleep(Duration::from_millis(10));
            }
            exit(if last >= 500 { 0 } else { 2 });
        }
        Some("writer") => {
            let b = HeartbeatBlock::open_ro(&path_env()).unwrap();
            // Writes to the first bytes of the PROT_READ mapping: the kernel kills us (SIGSEGV).
            b.sequencer().beat();
            exit(0);
        }
        _ => {}
    }
    let dir = shm_dir();
    let path = dir.join("heartbeat");
    let b = HeartbeatBlock::create(&path, 1).unwrap();
    let hk = &b.hk.0;
    assert_eq!(hk.magic.to_le_bytes(), *b"HFTHB\0\x01\0");
    assert_eq!(hk.layout_version, 1);
    assert_eq!(hk.pid, std::process::id());
    assert_eq!(hk.instance_id, 1);
    assert_eq!(hk.state.load(Ordering::Acquire), HkLine::STARTING);
    assert_eq!(hk.n_slots.load(Ordering::Acquire), 0);
    assert_eq!(hk.boot_id, boot_id());
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o640
    );
    let envs = [("HB", path.display().to_string())];
    let reader = spawn("rt_095_create_and_open_ro", "reader", &envs);
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_millis(1300) {
        b.sequencer().beat();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(reader.code(), 0, "step 2");
    let mut w = spawn("rt_095_create_and_open_ro", "writer", &envs);
    let st = w.child.wait().unwrap();
    assert_eq!(st.signal(), Some(libc::SIGSEGV), "step 3: {st:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn rt_096_open_ro_errors() {
    let dir = shm_dir();
    let path = dir.join("heartbeat");
    HeartbeatBlock::create(&path, 1).unwrap();
    let orig = std::fs::read(&path).unwrap();
    let ver_off = 128 + offset_of!(HkLine, layout_version);
    let mut bad_magic = orig.clone();
    bad_magic[128..136].fill(0);
    let mut bad_ver = orig.clone();
    bad_ver[ver_off..ver_off + 4].copy_from_slice(&2u32.to_le_bytes());
    std::fs::write(&path, &bad_magic).unwrap();
    assert_eq!(
        HeartbeatBlock::open_ro(&path).err(),
        Some(RtErr::BadMagic),
        "RT-096.1"
    );
    std::fs::write(&path, &bad_ver).unwrap();
    assert_eq!(
        HeartbeatBlock::open_ro(&path).err(),
        Some(RtErr::BadVersion(2)),
        "RT-096.2"
    );
    assert_eq!(
        HeartbeatBlock::open_ro(&dir.join("missing")).err(),
        Some(RtErr::Os(2)),
        "RT-096.3"
    );
    std::fs::write(&path, &orig).unwrap();
    assert!(HeartbeatBlock::open_ro(&path).is_ok(), "RT-096.4");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn rt_097_claim_limits() {
    let dir = shm_dir();
    let b = HeartbeatBlock::create(&dir.join("hb1"), 1).unwrap();
    let lo = b.slots.as_ptr() as usize;
    let hi = lo + std::mem::size_of_val(&b.slots);
    let mut addrs = Vec::new();
    for i in 0..30u16 {
        let s = b.claim(&format!("t{i}"), i, 2).expect("slot");
        let a = s as *const _ as usize;
        assert!(a >= lo && a < hi);
        addrs.push(a);
    }
    addrs.sort();
    addrs.dedup();
    assert_eq!(addrs.len(), 30);
    assert_eq!(b.hk.n_slots.load(Ordering::Acquire), 30);
    assert!(b.claim("x", 0, 2).is_none());
    assert_eq!(b.hk.n_slots.load(Ordering::Acquire), 30);
    let b2 = HeartbeatBlock::create(&dir.join("hb2"), 1).unwrap();
    let s = b2.claim("abcdefghijklmnopqrst", 5, 10).unwrap();
    assert_eq!(s.name_bytes(), *b"abcdefghijklmnop");
    assert_eq!(s.core.load(Ordering::Acquire), 5);
    assert_eq!(s.kind.load(Ordering::Acquire), 10);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn rt_098_set_state() {
    if role().as_deref() == Some("reader") {
        let b = HeartbeatBlock::open_ro(&path_env()).unwrap();
        say("READY");
        loop {
            let cmd = listen();
            if cmd.is_empty() || cmd == "END" {
                exit(0);
            }
            say(&b.hk.state.load(Ordering::Acquire).to_string());
        }
    }
    let dir = shm_dir();
    let path = dir.join("heartbeat");
    let b = HeartbeatBlock::create(&path, 2).unwrap();
    let mut kid = spawn(
        "rt_098_set_state",
        "reader",
        &[("HB", path.display().to_string())],
    );
    assert_eq!(kid.hear(), "READY");
    for (row, s) in [
        ("RT-098.1", 2u8),
        ("RT-098.2", 3),
        ("RT-098.3", 4),
        ("RT-098.4", 5),
    ] {
        b.set_state(s);
        kid.tell("READ");
        assert_eq!(kid.hear(), s.to_string(), "{row}");
    }
    kid.tell("END");
    assert_eq!(kid.code(), 0);
    let _ = std::fs::remove_dir_all(dir);
}
