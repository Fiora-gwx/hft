//! The test writer example writes journals readers can read back (JNL-123). Linux tmpfs.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![cfg(target_os = "linux")]

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use common::harness::*;
use hft_event::Bbo;
use hft_journal::*;

fn testwriter() -> PathBuf {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let target = crate_dir.join("../../target/jnl-123");
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
        .arg(crate_dir.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", &target)
        .status()
        .unwrap();
    assert!(st.success(), "build testwriter");
    target.join("debug/examples/testwriter")
}

fn run(bin: &Path, hot: Option<&Path>, cold: &Path, seal: bool) -> i32 {
    let mut c = Command::new(bin);
    if let Some(h) = hot {
        c.arg("--hot").arg(h);
    }
    c.arg("--cold").arg(cold).args(["--instance", "prod"]);
    c.args([
        "--synth-bbo",
        "1000",
        "--inst",
        "101",
        "--bid",
        "6000000",
        "--ask",
        "6000001",
    ]);
    c.args([
        "--qty",
        "50000000",
        "--t0-ns",
        "1759276810000000000",
        "--step-ns",
        "1000000",
    ]);
    c.args([
        "--publish-every",
        "10",
        "--segment-bytes",
        "1048576",
        "--sync-interval-ms",
        "50",
    ]);
    if seal {
        c.arg("--seal");
    }
    c.status().unwrap().code().unwrap_or(-1)
}

fn read_bbos(hot: &Path, cold: &Path) -> Vec<(Bbo, u16)> {
    let mut rc = ReaderConfig::new(hot.into(), cold.into(), InstanceName::Prod, "seq".into());
    rc.end = EndAt::LastCompleteEvent;
    let mut r = JournalReader::open(&rc).unwrap();
    let mut v = Vec::new();
    loop {
        match r.poll() {
            Next::Record(x) if x.hdr.kind == 0x0101 => v.push((x.read::<Bbo>().unwrap(), x.hdr.flags)),
            Next::Record(_) | Next::Event(_) => {}
            Next::End => return v,
            o => panic!("{o:?}"),
        }
    }
}

#[test]
fn jnl_123_testwriter_roundtrip() {
    let bin = testwriter();
    let (h1, c1) = (shm_hot(), cold_dir());
    assert_eq!(run(&bin, Some(&h1), &c1, true), 0, "step 1");
    let jdir = c1.join("prod/seq");
    for day in std::fs::read_dir(&jdir).unwrap().flatten() {
        for f in std::fs::read_dir(day.path()).unwrap().flatten() {
            let p = f.path();
            if p.extension().is_some_and(|e| e == "jrn") {
                assert!(p.with_extension("seal").exists(), "step 1: {p:?} sealed");
            }
        }
    }
    let got = read_bbos(&h1, &c1);
    assert_eq!(got.len(), 1000, "step 2");
    let t0 = 1_759_276_810_000_000_000i64;
    for (i, (b, flags)) in got.iter().enumerate() {
        let ex = t0 + i as i64 * 1_000_000 - 2_000_000;
        assert_eq!(
            (b.h.inst.0, b.bid_px.0, b.ask_px.0, b.bid_qty.0, b.ask_qty.0),
            (101, 6_000_000, 6_000_001, 50_000_000, 50_000_000)
        );
        assert_eq!(b.h.ex_ts_ns, ex, "step 2 #{i}");
        assert_eq!(
            b.h.hw_rx_ns,
            if i % 2 == 0 { ex + 1_000_000 } else { 0 },
            "step 2 #{i}"
        );
        assert_eq!(flags & EVENT_END, EVENT_END, "step 2 #{i}");
    }
    let (h2, c2) = (shm_hot(), cold_dir());
    assert_eq!(run(&bin, Some(&h2), &c2, false), 0, "step 3");
    let files = hft_journal::testing::hot_files(&h2.join("prod/seq"));
    let last = std::fs::read(files.last().unwrap()).unwrap();
    assert_eq!(u32_at(&last, 3088), 1, "step 3: last segment Open");
    assert_eq!(read_bbos(&h2, &c2).len(), 1000, "step 3");
    assert_eq!(run(&bin, None, &cold_dir(), false), 1, "step 4");
    assert_eq!(
        run(&bin, Some(&disk_dir("ext4hot")), &cold_dir(), false),
        2,
        "step 5"
    );
}
