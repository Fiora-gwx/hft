//! Readers map read-only and never slow the writer (JNL-087, JNL-088). Linux tmpfs.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![cfg(target_os = "linux")]

#[path = "../common/mod.rs"]
mod common;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use common::harness::*;
use common::proc::*;
use hft_journal::*;
use hft_types::Tsc;

fn reader_cfg() -> ReaderConfig {
    ReaderConfig::new(
        var("JNL_HOT").into(),
        var("JNL_COLD").into(),
        InstanceName::Prod,
        "seq".into(),
    )
}

fn envs(hot: &std::path::Path, cold: &std::path::Path) -> Vec<(&'static str, String)> {
    vec![
        ("JNL_HOT", hot.to_string_lossy().into()),
        ("JNL_COLD", cold.to_string_lossy().into()),
    ]
}

/// Writer rows (3072..3200) after each of 100 deterministic steps.
fn workload(h: &mut H) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for _ in 0..100 {
        h.events(5);
        h.mono += 5_000_000;
        h.pers.poll(h.mono, h.utc);
        out.push(h.seg(1)[3072..3200].to_vec());
        std::thread::sleep(Duration::from_millis(100));
    }
    out
}

#[test]
fn jnl_087_reader_read_only() {
    match role().as_deref() {
        Some("poke") => {
            let mut r = JournalReader::open(&reader_cfg()).unwrap();
            let _ = r.poll();
            let p = r.debug_map_addr() as *mut u8;
            say("mapped");
            // SAFETY: deliberately writes through the reader's PROT_READ mapping; the kernel must
            // stop this with SIGSEGV.
            unsafe { p.add(3072).write_volatile(1) };
            exit(0);
        }
        Some("ro") => {
            let mut r = JournalReader::open(&reader_cfg()).unwrap();
            let t = Instant::now();
            while t.elapsed() < Duration::from_secs(10) {
                if matches!(r.poll(), Next::Pending) {
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
            exit(0);
        }
        _ => {}
    }
    let hot = shm_hot();
    let cold = cold_dir();
    let mut h = H::open0(c0(&hot, &cold));
    h.events(1);
    let a = spawn("jnl_087_reader_read_only", "poke", &envs(&hot, &cold));
    assert_eq!(a.wait(), Err(libc::SIGSEGV));
    let b = spawn("jnl_087_reader_read_only", "ro", &envs(&hot, &cold));
    let with_b = workload(&mut h);
    let _ = b.wait();
    let mut h2 = H::open0(c0(&shm_hot(), &cold_dir()));
    h2.events(1);
    let without = workload(&mut h2);
    assert_eq!(
        with_b, without,
        "only the writer and persist change the shared rows"
    );
}

#[test]
fn jnl_088_slow_reader() {
    if role().is_some() {
        let mut r = JournalReader::open(&reader_cfg()).unwrap();
        say("open");
        let want: u64 = listen().parse().unwrap();
        let mut n = 0u64;
        let mut last = 0;
        loop {
            match r.poll() {
                Next::Record(v) => {
                    n += 1;
                    last = v.hdr.seq;
                }
                Next::Event(e) => {
                    n += 1;
                    last = e.seq;
                }
                Next::Pending => {
                    if last >= want {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                o => panic!("{o:?}"),
            }
        }
        say(&format!("read {n} {last}"));
        exit(0);
    }
    let hot = shm_hot();
    let cold = cold_dir();
    let mut cfg = c0(&hot, &cold);
    cfg.segment_bytes = 4 << 20;
    cfg.hot_keep = 1;
    let mut h = H::open0(cfg);
    let mut kid = spawn("jnl_088_slow_reader", "reader", &envs(&hot, &cold));
    assert_eq!(kid.hear(), "open");
    kid.signal(libc::SIGSTOP);
    // "Blocked" is judged by what the writer thread itself did inside its `append` calls, not by
    // wall time: on a shared runner the wall time of a ~100 ns call also holds preemption by other
    // runnable threads and vCPU steal (README "延迟与资源契约": tmpfs hot segments only record
    // times; testing.md §8: no time thresholds on shared CI runners). A call that waits gives up
    // the CPU (voluntary context switch) or waits for I/O (major fault).
    let mut inside = ThreadUsage::default();
    let mut worst_wall = Duration::ZERO;
    let mut worst_cpu = Duration::ZERO;
    for i in 0..200_000u64 {
        let u0 = ThreadUsage::now();
        let t = Instant::now();
        let r1 = h.w().append(F0, Tsc(i), &tin(1));
        let r2 = h.w().append(F0, Tsc(i), &tout(1));
        let wall = t.elapsed();
        let d = u0.until(ThreadUsage::now());
        worst_wall = worst_wall.max(wall);
        worst_cpu = worst_cpu.max(d.cpu);
        inside.nvcsw += d.nvcsw;
        inside.nivcsw += d.nivcsw;
        inside.minflt += d.minflt;
        inside.majflt += d.majflt;
        r1.unwrap();
        r2.unwrap();
        h.w().end_event();
        h.w().publish();
        if h.w().needs_roll(Tsc(0)).is_some() {
            h.roll(RollReason::Size);
        }
        if i % 1000 == 0 {
            h.mono += 25_000_000;
            h.utc += 1_000_000_000;
            h.pers.poll(h.mono, h.utc);
            h.ret.poll(h.utc);
        }
    }
    h.persist_all();
    // Recorded, not judged (the latency budgets are JNL-108/JNL-110 on the reference machine).
    eprintln!(
        "jnl_088: worst append pair wall {worst_wall:?}, thread CPU {worst_cpu:?}; preempted {} times, minor faults {}",
        inside.nivcsw, inside.minflt
    );
    assert_eq!(
        (inside.nvcsw, inside.majflt),
        (0, 0),
        "append blocked: voluntary context switches, major faults"
    );
    let last = h.w().next_seq().0 - 1;
    kid.signal(libc::SIGCONT);
    kid.tell(&last.to_string());
    let line = kid.hear();
    assert_eq!(line, format!("read {last} {last}"), "complete sequence");
    let _ = kid.wait();
    let _: PathBuf = hot;
}
