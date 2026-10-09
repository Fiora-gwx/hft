//! Batch memory bound of `events()` (TESTS.md PY-161). Its own test binary: the anonymous resident
//! memory of the process is sampled while the scan runs, so no other test may run beside it.

#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use hft_py::journal::{Events, Journal, QueryArgs, kind_by};

const D: i64 = 1_790_812_800_000_000_000;

/// hft-journal's test writer: `HFT_PY_TOOLS`, the `examples/` directory next to this test
/// binary's profile directory, or a build into `target/hft-py-tw`.
fn testwriter() -> PathBuf {
    if let Ok(d) = std::env::var("HFT_PY_TOOLS") {
        return PathBuf::from(d).join("testwriter");
    }
    let exe = std::env::current_exe().unwrap();
    if let Some(p) = exe
        .parent()
        .and_then(Path::parent)
        .map(|d| d.join("examples").join("testwriter"))
        && p.exists()
    {
        return p;
    }
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let target = crate_dir.join("../../target/hft-py-tw");
    let st = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args([
            "build",
            "--quiet",
            "--example",
            "testwriter",
            "--features",
            "writer",
            "--manifest-path",
        ])
        .arg(crate_dir.join("../hft-journal/Cargo.toml"))
        .env("CARGO_TARGET_DIR", &target)
        .status()
        .unwrap();
    assert!(st.success(), "build testwriter");
    target.join("debug/examples/testwriter")
}

/// A sealed single-segment journal of `n` synthetic Bbo (cold copy only); returns the journal path.
fn journal(n: u64, tag: &str) -> PathBuf {
    let id = format!("hftpy-mem-{}-{tag}", std::process::id());
    let hot = PathBuf::from("/dev/shm").join(&id);
    let cold = std::env::temp_dir().join(&id);
    let _ = std::fs::remove_dir_all(&hot);
    let _ = std::fs::remove_dir_all(&cold);
    std::fs::create_dir_all(&hot).unwrap();
    let mut c = Command::new(testwriter());
    c.args(["--hot"]).arg(&hot).arg("--cold").arg(&cold);
    c.args([
        "--instance",
        "prod",
        "--script",
        "-",
        "--segment-bytes",
        "536870912",
        "--publish-every",
        "4096",
    ]);
    c.args([
        "--tsc-hz-milli",
        "3000000000000",
        "--calib-utc-ns",
        &D.to_string(),
    ]);
    c.stdin(std::process::Stdio::piped());
    let mut ch = c.spawn().unwrap();
    {
        use std::io::Write;
        let mut si = ch.stdin.take().unwrap();
        writeln!(
            si,
            "synth {n} 101 6000000 6000001 50000000 {} 1000000\nseal",
            D + 10_000_000_000
        )
        .unwrap();
    }
    assert!(ch.wait().unwrap().success(), "testwriter");
    let _ = std::fs::remove_dir_all(&hot);
    cold.join("prod").join("seq")
}

fn rss_anon_bytes() -> u64 {
    let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    s.lines()
        .find(|l| l.starts_with("RssAnon:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse::<u64>().ok())
        .map_or(0, |k| k * 1024)
}

/// Peak growth of anonymous resident memory while every batch of `events("Bbo")` is read and
/// dropped; returns `(peak growth, row width)`.
fn scan_peak(path: &Path, batch_rows: i64) -> (u64, u64) {
    let j = Arc::new(Journal::open(path.to_str().unwrap(), None).unwrap());
    let q = j
        .query(
            kind_by(Some("Bbo"), None).unwrap(),
            &QueryArgs {
                batch_rows: Some(batch_rows),
                ..QueryArgs::default()
            },
        )
        .unwrap();
    let width: u64 = q
        .plan
        .schema
        .fields
        .iter()
        .map(|f| f.ty.width().unwrap_or(0) as u64)
        .sum();
    let base = rss_anon_bytes();
    let peak = Arc::new(AtomicU64::new(base));
    let stop = Arc::new(AtomicBool::new(false));
    let sampler = {
        let (peak, stop) = (peak.clone(), stop.clone());
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                peak.fetch_max(rss_anon_bytes(), Ordering::Relaxed);
                std::thread::sleep(Duration::from_micros(200));
            }
        })
    };
    let mut ev = Events::new(&j, q);
    let mut rows = 0;
    while let Some(b) = ev.next_batch().unwrap() {
        rows += b.rows;
        peak.fetch_max(rss_anon_bytes(), Ordering::Relaxed);
        drop(b);
    }
    stop.store(true, Ordering::Relaxed);
    sampler.join().unwrap();
    assert!(rows > 0);
    (peak.load(Ordering::Relaxed).saturating_sub(base), width)
}

#[test]
fn py_161_batch_memory_bound() {
    let j1 = journal(1_000_000, "1m");
    let j2 = journal(2_000_000, "2m");
    // Warm-up pass: glibc raises its mmap threshold after the first large free, so the first scan
    // of a process moves later batches from mmap to the heap; only the steady state is compared.
    let _ = scan_peak(&j1, 100_000);
    let (p1, w) = scan_peak(&j1, 100_000);
    let bound = 100_000 * w * 2;
    assert!(p1 <= bound, "PY-161.1: peak {p1} > {bound}");
    let (p2, _) = scan_peak(&j2, 100_000);
    assert!(p2 <= bound, "PY-161.2: peak {p2} > {bound}");
    let diff = p1.abs_diff(p2) as f64 / p1.max(1) as f64;
    assert!(diff < 0.05, "PY-161.2: {p1} vs {p2}");
    let (p3, _) = scan_peak(&j1, 1_000_000);
    let bound3 = 1_000_000 * w * 2;
    assert!(p3 <= bound3, "PY-161.3: peak {p3} > {bound3}");
    for p in [j1, j2] {
        let _ = std::fs::remove_dir_all(p.parent().unwrap().parent().unwrap());
    }
}
