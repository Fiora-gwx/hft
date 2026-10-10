//! Batch memory bound of `events()` (TESTS.md PY-161). Its own test binary: the anonymous resident
//! memory of the process is sampled while the scan runs, so no other test may run beside it.

#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
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
    // status/RssAnon uses asynchronous RSS counters; their error can exceed the 5% comparison.
    // smaps_rollup walks page tables and sums the same anonymous resident pages precisely.
    let s = std::fs::read_to_string("/proc/self/smaps_rollup").expect("read precise anonymous RSS");
    s.lines()
        .find(|l| l.starts_with("Anonymous:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse::<u64>().ok())
        .expect("Anonymous in smaps_rollup")
        * 1024
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
    let peak = Arc::new(AtomicU64::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let (ready_tx, ready_rx) = mpsc::sync_channel(0);
    let sampler = {
        let (peak, stop) = (peak.clone(), stop.clone());
        std::thread::spawn(move || {
            // Warm the sampler's stack and proc reader before the baseline is captured.
            let _ = rss_anon_bytes();
            ready_tx.send(()).unwrap();
            while !stop.load(Ordering::Relaxed) {
                peak.fetch_max(rss_anon_bytes(), Ordering::Relaxed);
                std::thread::sleep(Duration::from_micros(200));
            }
        })
    };
    ready_rx.recv().expect("sampler ready");
    let base = rss_anon_bytes();
    peak.store(base, Ordering::Relaxed);
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
    let peak = peak.load(Ordering::Relaxed);
    eprintln!("PY-161 sample: rows {rows}, batch {batch_rows}, base {base}, peak {peak}");
    (peak.saturating_sub(base), width)
}

/// Each measurement starts with the same allocator history. Repeated scans in the parent can
/// leave different amounts of resident heap behind, changing the baseline independently of
/// the number of rows being read.
fn isolated_peak(warmup: &Path, path: &Path, batch_rows: i64) -> (u64, u64) {
    let out = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "py_161_batch_memory_bound", "--nocapture"])
        .env("HFT_PY_MEMORY_WARMUP", warmup)
        .env("HFT_PY_MEMORY_SCAN", path)
        .env("HFT_PY_MEMORY_BATCH", batch_rows.to_string())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "memory child failed: {}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    eprint!("{}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8(out.stdout).unwrap();
    let values = stdout
        .lines()
        .find_map(|s| s.strip_prefix("HFT_PY_MEMORY_RESULT "))
        .unwrap();
    let mut values = values.split_whitespace();
    (
        values.next().unwrap().parse().unwrap(),
        values.next().unwrap().parse().unwrap(),
    )
}

#[test]
fn py_161_batch_memory_bound() {
    if let Some(path) = std::env::var_os("HFT_PY_MEMORY_SCAN") {
        let warmup = PathBuf::from(std::env::var_os("HFT_PY_MEMORY_WARMUP").unwrap());
        let batch = std::env::var("HFT_PY_MEMORY_BATCH").unwrap().parse().unwrap();
        let _ = scan_peak(&warmup, 100_000);
        let (peak, width) = scan_peak(&PathBuf::from(path), batch);
        println!("HFT_PY_MEMORY_RESULT {peak} {width}");
        return;
    }
    let j1 = journal(1_000_000, "1m");
    let j2 = journal(2_000_000, "2m");
    // Every child first warms up on the same 1M-row journal. The sampler can miss the short
    // reallocation peaks, so use the largest of three independent measurements per row.
    let peak3 = |p: &Path, rows: i64| {
        (0..3)
            .map(|_| isolated_peak(&j1, p, rows))
            .fold((0, 0), |a, b| (a.0.max(b.0), b.1))
    };
    let (p1, w) = peak3(&j1, 100_000);
    let bound = 100_000 * w * 2;
    assert!(p1 <= bound, "PY-161.1: peak {p1} > {bound}");
    let (p2, _) = peak3(&j2, 100_000);
    assert!(p2 <= bound, "PY-161.2: peak {p2} > {bound}");
    let diff = p1.abs_diff(p2) as f64 / p1.max(1) as f64;
    eprintln!("PY-161: peaks {p1} {p2}, row width {w}");
    assert!(diff < 0.05, "PY-161.2: {p1} vs {p2}");
    let (p3, _) = isolated_peak(&j1, &j1, 1_000_000);
    let bound3 = 1_000_000 * w * 2;
    assert!(p3 <= bound3, "PY-161.3: peak {p3} > {bound3}");
    for p in [j1, j2] {
        let _ = std::fs::remove_dir_all(p.parent().unwrap().parent().unwrap());
    }
}
