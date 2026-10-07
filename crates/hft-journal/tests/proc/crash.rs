//! Crash survival and recovery with real processes (JNL-069 – JNL-072). Linux tmpfs.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![cfg(target_os = "linux")]

#[path = "../common/mod.rs"]
mod common;

use std::path::PathBuf;
use std::time::Duration;

use common::harness::*;
use common::proc::*;
use hft_event::{EventKind, SnapshotHdr};
use hft_journal::*;
use hft_types::Tsc;
use zerocopy::{FromZeros, IntoBytes};

fn dirs() -> (PathBuf, PathBuf) {
    (shm_hot(), cold_dir())
}

fn envs(hot: &std::path::Path, cold: &std::path::Path) -> Vec<(&'static str, String)> {
    vec![
        ("JNL_HOT", hot.to_string_lossy().into()),
        ("JNL_COLD", cold.to_string_lossy().into()),
    ]
}

fn child_cfg() -> JournalConfig {
    c0(&PathBuf::from(var("JNL_HOT")), &PathBuf::from(var("JNL_COLD")))
}

fn kill_self() -> ! {
    // SAFETY: SIGKILL to our own pid.
    unsafe { libc::kill(libc::getpid(), libc::SIGKILL) };
    exit(9)
}

#[test]
fn jnl_069_segment_survives_kill() {
    if role().is_some() {
        let mut h = H::open0(child_cfg());
        h.events(4_000);
        kill_self();
    }
    let (hot, cold) = dirs();
    let kid = spawn("jnl_069_segment_survives_kill", "writer", &envs(&hot, &cold));
    assert_eq!(kid.wait(), Err(libc::SIGKILL));
    let b = std::fs::read(hot.join("prod/seq/0000000001.jrn")).unwrap();
    assert_eq!(u32_at(&b, 3088), 1, "state Open");
    let c = u64_at(&b, 3072) as usize;
    let last = u64_at(&b, 3080);
    let rs = records(&b, c);
    for (i, (_, r)) in rs.iter().enumerate() {
        assert_eq!(r.seq, i as u64 + 1);
    }
    assert_eq!(rs.last().unwrap().1.seq, last);
    assert_eq!(rs.last().unwrap().1.flags & EVENT_END, EVENT_END);
    assert_eq!(rs.iter().map(|(_, r)| r.len as usize).sum::<usize>() + 4096, c);
}

#[test]
fn jnl_070_kill_mid_event() {
    if role().is_some() {
        let mut h = H::open0(child_cfg());
        h.events(99);
        h.w().append(F0, Tsc(1), &tin(7)).unwrap();
        h.w().append(F0, Tsc(1), &tout(7)).unwrap();
        say(&format!("half {}", h.w().offset()));
        let _ = listen();
        exit(0);
    }
    let (hot, cold) = dirs();
    let mut kid = spawn("jnl_070_kill_mid_event", "writer", &envs(&hot, &cold));
    let line = kid.hear();
    let end: usize = line.strip_prefix("half ").unwrap().parse().unwrap();
    kid.kill();
    let mut h = H::bare(c0(&hot, &cold), false);
    let rep = h.open().unwrap();
    assert_eq!(rep.bytes_discarded, 184);
    let b = h.seg(1);
    let e99 = end - 184;
    assert_eq!(
        rh(&b, e99).kind,
        EventKind::SEGMENT_ROLL.0,
        "End{{Recovery}} replaces event 100"
    );
    assert!(b[e99 + 72..end + 4096].iter().all(|x| *x == 0));
    let mut rc = h.reader_cfg(StartAt::Oldest);
    rc.end = EndAt::LastCompleteEvent;
    let mut r = JournalReader::open(&rc).unwrap();
    let got = drain(&mut r);
    let last_event = got.iter().rev().find(|x| x.1 == 0x7F02).unwrap().0;
    assert_eq!(last_event, 2 + 99 * 2, "event 99's TOut is the last event record");
    assert!(matches!(r.poll(), Next::End));
    let lce = r.last_complete_event().unwrap();
    assert_eq!((lce.segment_no, lce.offset as usize), (1, e99 + 72));
}

#[test]
fn jnl_071_kill_mid_boot() {
    match role().as_deref() {
        Some("writer") => {
            let mut h = H::open0(child_cfg());
            h.events(20);
            let mut b = h.roll_boot(RollReason::Hourly);
            let mut sh = SnapshotHdr::new_zeroed();
            sh.purpose = 1;
            let mut body = sh.as_bytes().to_vec();
            body.extend(multi_body(6 * 65_488));
            b.append_var(EventKind::STATE_SNAPSHOT, Tsc(1), &[&body]).unwrap();
            say("boot");
            let _ = listen();
            exit(0);
        }
        Some("reader") => {
            let rc = ReaderConfig::new(
                var("JNL_HOT").into(),
                var("JNL_COLD").into(),
                InstanceName::Prod,
                "seq".into(),
            );
            let mut r = JournalReader::open(&rc).unwrap();
            let mut max = 0;
            loop {
                match r.poll() {
                    Next::Record(v) => max = max.max(v.hdr.seq),
                    Next::Event(e) => max = max.max(e.seq),
                    Next::Pending => {
                        if std::path::Path::new(&var("JNL_STOP")).exists() {
                            say(&format!("max {max}"));
                            exit(0);
                        }
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    o => panic!("{o:?}"),
                }
            }
        }
        _ => {}
    }
    let (hot, cold) = dirs();
    let mut w = spawn("jnl_071_kill_mid_boot", "writer", &envs(&hot, &cold));
    assert_eq!(w.hear(), "boot");
    let c = c0(&hot, &cold);
    let mut p = Persister::new(PersistConfig::default());
    p.add(&c);
    for i in 0..10 {
        p.poll(i * 25_000_000, UTC0);
    }
    let stop = hot.join("stop");
    let mut ev = envs(&hot, &cold);
    ev.push(("JNL_STOP", stop.to_string_lossy().into()));
    let mut rd = spawn("jnl_071_kill_mid_boot", "reader", &ev);
    std::thread::sleep(Duration::from_millis(200));
    let s1_end = {
        let b = std::fs::read(hot.join("prod/seq/0000000001.jrn")).unwrap();
        u64_at(&b, 3080)
    };
    w.kill();
    std::fs::write(&stop, b"").unwrap();
    let line = rd.hear();
    let _ = rd.wait();
    assert_eq!(
        line,
        format!("max {s1_end}"),
        "the reader never delivered segment 2"
    );
    let s1 = std::fs::read(hot.join("prod/seq/0000000001.jrn")).unwrap();
    let mut h = H::bare(c.clone(), false);
    let rep = h.open().unwrap();
    assert!(!hft_journal::testing::exists(&h.seg_path(2)));
    let cold2: Vec<_> = std::fs::read_dir(h.cold_jdir().join("20261001"))
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("0000000002"))
        .collect();
    assert!(cold2.is_empty());
    let mut a = s1.clone();
    a[3072..3200].fill(0);
    let mut b = h.seg(1);
    b[3072..3200].fill(0);
    assert_eq!(a, b, "segment 1 unchanged");
    h.roll(rep.boot_reason);
    assert_eq!(h.w().segment_no(), 2);
}

/// Seeded event stream: (records per event, kinds) — `TIn`/`TOut`/`TMeta`/`TMulti(1–3 parts)`.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn random_writer(seed: u64) -> ! {
    let mut cfg = child_cfg();
    cfg.segment_bytes = 256 << 10;
    let mut h = H::open0(cfg);
    let mut rng = Rng(seed | 1);
    loop {
        let n = 1 + rng.below(6);
        let first = h.w().next_seq().0;
        for _ in 0..n {
            let w = h.w();
            // Meta records only as stand-alone events (an in-event Meta carries no EVENT_END).
            match rng.below(4) {
                0 => {
                    w.append(F0, Tsc(1), &tin(1)).unwrap();
                }
                1 => {
                    w.append(F0, Tsc(1), &tout(1)).unwrap();
                }
                2 if n == 1 => {
                    w.append_meta(Tsc(1), &tmeta(1)).unwrap();
                }
                2 => {
                    w.append(F0, Tsc(1), &tin(2)).unwrap();
                }
                _ => {
                    let parts = 1 + rng.below(3) as usize;
                    let l = (parts - 1) * 65_488 + 100;
                    w.append_var(EventKind::T_MULTI, F0, Tsc(1), &[&multi_body(l)])
                        .unwrap();
                }
            }
        }
        h.w().end_event();
        let last = h.w().next_seq().0 - 1;
        say(&format!("R {first} {last}"));
        if rng.below(10) == 0 {
            h.w().publish();
            say(&format!("P {last}"));
        }
        if h.w().needs_roll(Tsc(0)).is_some() {
            h.w().publish();
            say(&format!("P {last}"));
            h.roll(RollReason::Size);
        }
    }
}

#[test]
fn jnl_072_random_kill() {
    if role().is_some() {
        random_writer(var("JNL_SEED").parse().unwrap());
    }
    let seeds: u64 = std::env::var("JNL_072_SEEDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(50);
    for seed in 1..=seeds {
        let (hot, cold) = dirs();
        let mut ev = envs(&hot, &cold);
        ev.push(("JNL_SEED", seed.to_string()));
        let mut kid = spawn("jnl_072_random_kill", "writer", &ev);
        let mut ended: Vec<(u64, u64)> = Vec::new();
        let mut published = 0u64;
        let stop_after = 50 + (seed * 7919) % 2000;
        while let Some(l) = kid.try_hear() {
            let f: Vec<u64> = l[2..].split(' ').map(|x| x.parse().unwrap()).collect();
            if l.starts_with('R') {
                ended.push((f[0], f[1]));
            } else {
                published = f[0];
            }
            if ended.len() as u64 >= stop_after {
                break;
            }
        }
        kid.kill();
        // The preparer copies the configuration in `bare`: set the segment size before.
        let mut cfg = c0(&hot, &cold);
        cfg.segment_bytes = 256 << 10;
        let mut h = H::bare(cfg, false);
        let rep = h.open().unwrap();
        h.roll(rep.boot_reason);
        let mut r = h.reader(StartAt::Oldest);
        // Event ends seen by the reader (records with EVENT_END that are not roll/boot records).
        let mut ends = Vec::new();
        loop {
            match r.poll() {
                Next::Record(v) => {
                    if v.hdr.flags & EVENT_END != 0 && v.hdr.kind != 0x0509 {
                        ends.push(v.hdr.seq);
                    }
                }
                Next::Event(e) => {
                    if e.flags.0 & EVENT_END != 0 {
                        ends.push(e.seq + u64::from(e.parts) - 1);
                    }
                }
                Next::Pending => break,
                o => panic!("seed {seed}: {o:?}"),
            }
        }
        // Meta outside events also carries EVENT_END: keep only ends of recorded events.
        let lasts: Vec<u64> = ended.iter().map(|e| e.1).collect();
        let ends: Vec<u64> = ends.into_iter().filter(|s| lasts.contains(s)).collect();
        assert!(ends.len() <= lasts.len(), "seed {seed}");
        assert_eq!(
            ends[..],
            lasts[..ends.len()],
            "seed {seed}: prefix of the ended events"
        );
        let p_events = lasts.iter().filter(|l| **l <= published).count();
        assert!(ends.len() >= p_events, "seed {seed}: all published events kept");
    }
}
