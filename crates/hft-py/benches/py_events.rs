//! `py_events`: `events()` scan and transposition throughput of a 1 000 000-row sealed `Bbo`
//! segment (TESTS.md PY-160; budget in README "延迟与资源契约", reported only on PRs).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use hft_py::journal::{Events, Journal, QueryArgs, kind_by};

const D: i64 = 1_790_812_800_000_000_000;
const ROWS: u64 = 1_000_000;

/// The journal, written by hft-journal's test writer (`HFT_PY_TOOLS` or `<profile>/examples`).
fn journal() -> Option<PathBuf> {
    let tw = match std::env::var("HFT_PY_TOOLS") {
        Ok(d) => PathBuf::from(d).join("testwriter"),
        Err(_) => std::env::current_exe()
            .ok()?
            .parent()?
            .parent()?
            .join("examples")
            .join("testwriter"),
    };
    if !tw.exists() || !cfg!(target_os = "linux") {
        return None;
    }
    let id = format!("hftpy-bench-{}", std::process::id());
    let hot = PathBuf::from("/dev/shm").join(&id);
    let cold = std::env::temp_dir().join(&id);
    std::fs::create_dir_all(&hot).ok()?;
    let mut c = Command::new(tw);
    c.arg("--hot").arg(&hot).arg("--cold").arg(&cold);
    c.args([
        "--instance",
        "prod",
        "--script",
        "-",
        "--segment-bytes",
        "268435456",
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
    let mut ch = c.spawn().ok()?;
    {
        use std::io::Write;
        let mut si = ch.stdin.take()?;
        writeln!(
            si,
            "synth {ROWS} 101 6000000 6000001 50000000 {} 1000000\nseal",
            D + 10_000_000_000
        )
        .ok()?;
    }
    ch.wait().ok()?.success().then_some(())?;
    let _ = std::fs::remove_dir_all(&hot);
    Some(cold.join("prod").join("seq"))
}

fn py_160_events_scan_throughput(c: &mut Criterion) {
    let Some(path) = journal() else {
        eprintln!("py_events: test writer not built or not Linux; skipped");
        return;
    };
    let j = Arc::new(Journal::open(path.to_str().unwrap(), None).unwrap());
    let q = j
        .query(
            kind_by(Some("Bbo"), None).unwrap(),
            &QueryArgs {
                batch_rows: Some(ROWS as i64),
                ..QueryArgs::default()
            },
        )
        .unwrap();
    let mut g = c.benchmark_group("hft-py");
    g.sample_size(10);
    g.throughput(Throughput::Elements(ROWS));
    g.bench_function("py_events", |b| {
        b.iter(|| {
            let mut ev = Events::new(&j, q.clone());
            let mut n = 0;
            while let Some(batch) = ev.next_batch().unwrap() {
                n += batch.rows;
            }
            assert_eq!(n as u64, ROWS);
        })
    });
    g.finish();
    let _ = std::fs::remove_dir_all(path.parent().and_then(|p| p.parent()).unwrap());
}

criterion_group!(benches, py_160_events_scan_throughput);
criterion_main!(benches);
