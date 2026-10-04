//! Shared-memory ring across processes (RT-020 – RT-027). Linux.

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

use std::io::{Seek, SeekFrom, Write};
use std::mem::offset_of;
use std::path::{Path, PathBuf};

use common::proc::*;
use hft_rt::{
    PageSize, PeerLine, RingSide, ShmConsumer, ShmErr, ShmProducer, ShmRingHdr, ShmSpsc, cpu_relax,
};

type E64 = [u64; 8];
const LID: u64 = 0x1234;

fn create_p(path: &Path, cap: usize) -> ShmProducer<E64> {
    ShmSpsc::create::<E64>(path, cap, PageSize::Normal, RingSide::Producer, LID)
        .unwrap()
        .into_producer()
        .unwrap()
}

fn create_c(path: &Path, cap: usize) -> ShmConsumer<E64> {
    ShmSpsc::create::<E64>(path, cap, PageSize::Normal, RingSide::Consumer, LID)
        .unwrap()
        .into_consumer()
        .unwrap()
}

fn attach_p(path: &Path) -> Result<ShmProducer<E64>, ShmErr> {
    ShmSpsc::attach::<E64>(path, RingSide::Producer, LID).map(|e| e.into_producer().unwrap())
}

fn attach_c(path: &Path) -> Result<ShmConsumer<E64>, ShmErr> {
    ShmSpsc::attach::<E64>(path, RingSide::Consumer, LID).map(|e| e.into_consumer().unwrap())
}

fn push(p: &mut ShmProducer<E64>, v: u64) {
    while p.try_push([v; 8]).is_err() {
        cpu_relax();
    }
}

fn drain(c: &mut ShmConsumer<E64>) -> Vec<u64> {
    std::iter::from_fn(|| c.try_pop().map(|e| e[0])).collect()
}

fn poke(path: &Path, off: usize, bytes: &[u8]) {
    let mut f = std::fs::OpenOptions::new().write(true).open(path).unwrap();
    f.seek(SeekFrom::Start(off as u64)).unwrap();
    f.write_all(bytes).unwrap();
}

fn peek_bytes(path: &Path, off: usize, n: usize) -> Vec<u8> {
    std::fs::read(path).unwrap()[off..off + n].to_vec()
}

fn path_env() -> PathBuf {
    PathBuf::from(env("RING"))
}

fn err_code(r: Result<(), ShmErr>) -> i32 {
    match r {
        Ok(()) => 0,
        Err(e) => e as i32,
    }
}

#[test]
fn rt_020_shm_round_trip() {
    if role().as_deref() == Some("consumer") {
        let mut c = attach_c(&path_env()).unwrap();
        let mut want = 0u64;
        while want < 10_000 {
            match c.try_pop() {
                Some(e) if e[0] == want => want += 1,
                Some(_) => exit(1),
                None => cpu_relax(),
            }
        }
        exit(0);
    }
    let dir = shm_dir();
    let path = dir.join("ring");
    let mut p = create_p(&path, 1024);
    let h = p.header();
    assert_eq!((h.elem_size, h.capacity, h.layout_id), (64, 1024, LID));
    assert_eq!(
        h.state.load(std::sync::atomic::Ordering::Acquire),
        ShmRingHdr::READY
    );
    let kid = spawn(
        "rt_020_shm_round_trip",
        "consumer",
        &[("RING", path.display().to_string())],
    );
    for v in 0..10_000 {
        push(&mut p, v);
    }
    assert_eq!(kid.code(), 0);
    let _ = std::fs::remove_dir_all(dir);
}

const VERSION: usize = offset_of!(ShmRingHdr, version);
const STATE: usize = offset_of!(ShmRingHdr, state);
const LAYOUT: usize = offset_of!(ShmRingHdr, layout_id);
const EPOCH: usize = offset_of!(ShmRingHdr, epoch);
const CONSUMER: usize = offset_of!(ShmRingHdr, consumer);

#[test]
fn rt_021_attach_errors() {
    if role().as_deref() == Some("attach") {
        let path = path_env();
        let r = match env("ROW").as_str() {
            "2" => ShmSpsc::attach::<[u64; 4]>(&path, RingSide::Consumer, LID).map(|_| ()),
            "5" => attach_c(&path.with_file_name("missing")).map(|_| ()),
            "8" => attach_p(&path).map(|_| ()),
            "1" => ShmSpsc::attach::<E64>(&path, RingSide::Consumer, 0x9999).map(|_| ()),
            _ => attach_c(&path).map(|_| ()),
        };
        exit(err_code(r));
    }
    let dir = shm_dir();
    let path = dir.join("ring");
    let _p = create_p(&path, 1024);
    let orig = std::fs::read(&path).unwrap()[..4096].to_vec();
    type Edit = fn(&Path);
    let rows: [(&str, &str, Edit, Result<(), ShmErr>); 9] = [
        ("RT-021.1", "1", |_| {}, Err(ShmErr::LayoutMismatch)),
        ("RT-021.2", "2", |_| {}, Err(ShmErr::LayoutMismatch)),
        (
            "RT-021.3",
            "3",
            |p| poke(p, VERSION, &2u32.to_le_bytes()),
            Err(ShmErr::VersionMismatch),
        ),
        (
            "RT-021.4",
            "4",
            |p| poke(p, 0, b"XXXXXXXX"),
            Err(ShmErr::BadMagic),
        ),
        ("RT-021.5", "5", |_| {}, Err(ShmErr::Io)),
        (
            "RT-021.6",
            "6",
            |p| poke(p, STATE, &0u32.to_le_bytes()),
            Err(ShmErr::NotReady),
        ),
        (
            "RT-021.7",
            "7",
            |p| poke(p, STATE, &2u32.to_le_bytes()),
            Err(ShmErr::Closed),
        ),
        ("RT-021.8", "8", |_| {}, Err(ShmErr::SideTaken)),
        ("RT-021.9", "9", |_| {}, Ok(())),
    ];
    for (row, id, edit, want) in rows {
        poke(&path, 0, &orig);
        edit(&path);
        let t0 = std::time::Instant::now();
        let kid = spawn(
            "rt_021_attach_errors",
            "attach",
            &[("RING", path.display().to_string()), ("ROW", id.into())],
        );
        assert_eq!(kid.code(), err_code(want), "{row}");
        assert!(
            t0.elapsed() < std::time::Duration::from_secs(10),
            "{row} must not wait"
        );
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn rt_022_producer_killed() {
    match role().as_deref() {
        Some("a") => {
            let mut p = attach_p(&path_env()).unwrap();
            for b in 0..50u64 {
                let mut wb = p.write_batch(100);
                for i in 0..100 {
                    assert!(wb.push([b * 100 + i; 8]));
                }
                wb.commit();
            }
            let mut wb = p.write_batch(100);
            for i in 0..50 {
                assert!(wb.push([5_000 + i; 8]));
            }
            say("READY");
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
        Some("b") => {
            let mut p = match attach_p(&path_env()) {
                Ok(p) => p,
                Err(_) => exit(2),
            };
            for v in 1_000_000..1_000_100 {
                push(&mut p, v);
            }
            exit(0);
        }
        _ => {}
    }
    let dir = shm_dir();
    let path = dir.join("ring");
    let mut c = create_c(&path, 8192);
    let envs = [("RING", path.display().to_string())];
    let mut a = spawn("rt_022_producer_killed", "a", &envs);
    assert_eq!(a.hear(), "READY");
    a.kill();
    assert_eq!(drain(&mut c), (0..5_000).collect::<Vec<u64>>());
    let b = spawn("rt_022_producer_killed", "b", &envs);
    assert_eq!(b.code(), 0, "B attaches over the dead pid line");
    assert_eq!(drain(&mut c), (1_000_000..1_000_100).collect::<Vec<u64>>());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn rt_023_consumer_killed_redelivers() {
    match role().as_deref() {
        Some("a") => {
            let mut c = attach_c(&path_env()).unwrap();
            let b = c.read_batch(100);
            let (x, y) = b.slices();
            let got: Vec<u64> = x.iter().chain(y.iter()).map(|e| e[0]).collect();
            assert_eq!(got, (0..100).collect::<Vec<u64>>());
            say("READY");
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
        Some("b") => {
            let mut c = attach_c(&path_env()).unwrap();
            let got = drain(&mut c);
            exit(if got == (0..200).collect::<Vec<u64>>() {
                0
            } else {
                1
            });
        }
        _ => {}
    }
    let dir = shm_dir();
    let path = dir.join("ring");
    let mut p = create_p(&path, 1024);
    for v in 0..200 {
        push(&mut p, v);
    }
    let envs = [("RING", path.display().to_string())];
    let mut a = spawn("rt_023_consumer_killed_redelivers", "a", &envs);
    assert_eq!(a.hear(), "READY");
    a.kill();
    let b = spawn("rt_023_consumer_killed_redelivers", "b", &envs);
    assert_eq!(b.code(), 0);
    let _ = std::fs::remove_dir_all(dir);
}

fn my_start() -> u64 {
    let s = std::fs::read_to_string("/proc/self/stat").unwrap();
    s[s.rfind(')').unwrap() + 1..]
        .split_whitespace()
        .nth(19)
        .unwrap()
        .parse()
        .unwrap()
}

#[test]
fn rt_024_takeover_by_pid_start() {
    if role().as_deref() == Some("attach") {
        exit(err_code(attach_c(&path_env()).map(|_| ())));
    }
    let dir = shm_dir();
    let path = dir.join("ring");
    let _p = create_p(&path, 1024);
    let dead = {
        let mut ch = std::process::Command::new("true").spawn().unwrap();
        let id = ch.id();
        ch.wait().unwrap();
        id
    };
    let me = std::process::id();
    let start = my_start();
    let pid_off = CONSUMER + offset_of!(PeerLine, pid);
    let start_off = CONSUMER + offset_of!(PeerLine, start);
    let rows: [(&str, u32, u64, Result<(), ShmErr>); 4] = [
        ("RT-024.1", 0, 0, Ok(())),
        ("RT-024.2", dead, 0, Ok(())),
        ("RT-024.3", me, start, Err(ShmErr::SideTaken)),
        ("RT-024.4", me, start + 1, Ok(())),
    ];
    for (row, pid, st, want) in rows {
        poke(&path, pid_off, &pid.to_le_bytes());
        poke(&path, start_off, &st.to_le_bytes());
        let kid = spawn(
            "rt_024_takeover_by_pid_start",
            "attach",
            &[("RING", path.display().to_string())],
        );
        assert_eq!(kid.code(), err_code(want), "{row}");
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn rt_025_creator_rebuild() {
    match role().as_deref() {
        Some("c") => {
            let _p = create_p(&path_env(), 1024);
            say("READY");
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
        Some("c2") => {
            let mut p = create_p(&path_env(), 1024);
            say("READY");
            assert_eq!(listen(), "GO");
            for v in 0..10 {
                push(&mut p, v);
            }
            say("DONE");
            exit(0);
        }
        _ => {}
    }
    let dir = shm_dir();
    let path = dir.join("ring");
    let envs = [("RING", path.display().to_string())];
    let mut c = spawn("rt_025_creator_rebuild", "c", &envs);
    assert_eq!(c.hear(), "READY");
    let cons = attach_c(&path).unwrap();
    let e0 = cons.peer().epoch;
    c.kill();
    assert!(!cons.replaced(), "step 1: same inode");
    let mut c2 = spawn("rt_025_creator_rebuild", "c2", &envs);
    assert_eq!(c2.hear(), "READY");
    assert!(cons.replaced(), "step 2: new inode");
    drop(cons);
    let mut cons = attach_c(&path).unwrap();
    assert_eq!(cons.peer().epoch, e0 + 1);
    c2.tell("GO");
    assert_eq!(c2.hear(), "DONE");
    assert_eq!(c2.code(), 0);
    assert_eq!(drain(&mut cons), (0..10).collect::<Vec<u64>>());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn rt_026_epoch_rule() {
    let dir = shm_dir();
    let epoch = |p: &Path| create_p(p, 1024).header().epoch;
    let p1 = dir.join("r1");
    assert_eq!(epoch(&p1), 1, "RT-026.1");
    let p2 = dir.join("r2");
    drop(create_p(&p2, 1024));
    poke(&p2, EPOCH, &7u64.to_le_bytes());
    assert_eq!(
        u64::from_le_bytes(peek_bytes(&p2, EPOCH, 8).try_into().unwrap()),
        7
    );
    assert_eq!(epoch(&p2), 8, "RT-026.2");
    let p3 = dir.join("r3");
    let mut x = 0x2545_F491_4F6C_DD1Du64;
    let junk: Vec<u8> = (0..4096)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect();
    std::fs::write(&p3, &junk).unwrap();
    assert_eq!(epoch(&p3), 1, "RT-026.3");
    let _ = LAYOUT;
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn rt_027_peer_status() {
    match role().as_deref() {
        Some("k1") => {
            let c = attach_c(&path_env()).unwrap();
            for _ in 0..3 {
                c.beat();
            }
            say("READY");
            assert_eq!(listen(), "EXIT");
            drop(c); // clean exit releases the pid line
            exit(0);
        }
        Some("k2") => {
            let c = attach_c(&path_env()).unwrap();
            say("READY");
            assert_eq!(listen(), "CHECK");
            exit(if c.peer().alive { 1 } else { 0 });
        }
        _ => {}
    }
    let dir = shm_dir();
    let path = dir.join("ring");
    let p = create_p(&path, 1024);
    let envs = [("RING", path.display().to_string())];
    let mut k1 = spawn("rt_027_peer_status", "k1", &envs);
    assert_eq!(k1.hear(), "READY");
    let s = p.peer();
    assert!(s.alive);
    assert_eq!((s.pid, s.last_beat), (k1.pid(), 3));
    k1.tell("EXIT");
    assert_eq!(k1.code(), 0);
    assert!(!p.peer().alive, "step 2");
    let mut k2 = spawn("rt_027_peer_status", "k2", &envs);
    assert_eq!(k2.hear(), "READY");
    drop(p); // creator's clean close: state = Closed
    k2.tell("CHECK");
    assert_eq!(k2.code(), 0, "step 3");
    let _ = std::fs::remove_dir_all(dir);
}
