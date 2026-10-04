//! Cold disk full: the writer is unaffected, lag grows, and catches up after space returns
//! (JNL-048). Linux, CI prerequisite: `sudo mount -t tmpfs -o size=2M none /mnt/cold-small`
//! (path in `HFT_TEST_COLD_SMALL`) and password-less `sudo` for the remount (`HFT_TEST_SUDO=1`).
#![cfg(target_os = "linux")]

#[path = "../common/mod.rs"]
mod common;

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use common::harness::*;
use common::proc::*;
use hft_journal::*;
use hft_types::Tsc;

const T: &str = "jnl_048_cold_write_failure";

fn cfg(hot: &str, cold: &str) -> JournalConfig {
    let mut c = c0(&PathBuf::from(hot), &PathBuf::from(cold));
    c.segment_bytes = 16 << 20;
    c
}

/// Writes 1 000 events/s for 10 s; reports any append error.
fn child() -> ! {
    let mut h = H::open0(cfg(&var("JNL_HOT"), &var("JNL_COLD")));
    say("ready");
    let t0 = Instant::now();
    let mut n = 0u64;
    let mut last = 0u64;
    while t0.elapsed() < Duration::from_secs(10) {
        let due = t0.elapsed().as_millis() as u64;
        while n < due {
            let w = h.w();
            if w.append(F0, Tsc(n), &tin(1)).is_err() || w.append(F0, Tsc(n), &tout(1)).is_err() {
                say("append-error");
                exit(3);
            }
            w.end_event();
            w.publish();
            let c = h.committed(1);
            if c < last {
                say("committed-went-back");
                exit(3);
            }
            last = c;
            n += 1;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    say(&format!("done {n}"));
    std::mem::forget(h);
    exit(0)
}

#[test]
fn jnl_048_cold_write_failure() {
    if role().is_some() {
        child();
    }
    let (Some(cold), Some(_)) = (
        prereq("HFT_TEST_COLD_SMALL", "2 MiB tmpfs as cold root"),
        prereq("HFT_TEST_SUDO", "sudo for the remount"),
    ) else {
        return;
    };
    let hot = shm_hot();
    let c = cfg(hot.to_str().unwrap(), &cold);
    let mut kid = spawn(
        T,
        "writer",
        &[
            ("JNL_HOT", hot.to_string_lossy().into()),
            ("JNL_COLD", cold.clone()),
        ],
    );
    assert_eq!(kid.hear(), "ready");
    let mut p = Persister::new(PersistConfig::default());
    p.add(&c);
    let t0 = Instant::now();
    let mut mono = 0u64;
    let mut lags = Vec::new();
    let mut errors = 0;
    while t0.elapsed() < Duration::from_secs(10) {
        mono += 20_000_000;
        errors += p.poll(mono, UTC0).errors;
        lags.push(p.lag()[0].bytes);
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(errors > 0, "ENOSPC must surface as errors");
    let tail = &lags[lags.len() / 2..];
    assert!(
        tail.windows(2).all(|w| w[1] >= w[0]),
        "lag never shrinks while the disk is full"
    );
    let seal1 = hft_journal::testing::cold_stem(&c.cold_journal_dir(), UTC0, 1, 1).with_extension("seal");
    assert!(!seal1.exists());
    let line = kid.hear();
    assert!(line.starts_with("done"), "{line}");
    assert_eq!(kid.wait(), Ok(0));
    // Replace the cold mount by a large one at the same path.
    let sh = |args: &[&str]| {
        assert!(
            Command::new("sudo").args(args).status().unwrap().success(),
            "{args:?}"
        )
    };
    sh(&["umount", "-l", &cold]);
    sh(&["mount", "-t", "tmpfs", "-o", "size=256M", "none", &cold]);
    sh(&["chown", &format!("{}", unsafe_uid()), &cold]);
    let t1 = Instant::now();
    while t1.elapsed() < Duration::from_secs(5) {
        mono += 20_000_000;
        p.poll(mono, UTC0);
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(p.lag()[0].bytes, 0);
}

fn unsafe_uid() -> u32 {
    // SAFETY: getuid has no preconditions.
    unsafe { libc::getuid() }
}
