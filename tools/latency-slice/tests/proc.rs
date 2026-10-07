//! SLC-041–SLC-043: the binary (Linux).
#![cfg(target_os = "linux")]

mod support;

use std::process::Command;

use support::{CFG0, ED1_SEED};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_latency-slice"))
}

#[test]
fn slc_041_binary_dry_run() {
    let root = support::tmp("proc");
    let hot = std::path::PathBuf::from(format!("/dev/shm/slc-041-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&hot);
    let cfg = root.join("cfg.toml");
    std::fs::write(
        &cfg,
        format!(
            "{CFG0}\n[journal]\nhot_dir = {:?}\n[report]\ncal_max_err_ns = 1000000\n",
            hot.display().to_string()
        ),
    )
    .unwrap();
    let out = root.join("out");
    let o = bin()
        .args(["--dry-run", "--seconds", "2", "--config"])
        .arg(&cfg)
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));
    let j: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("report.json")).unwrap()).unwrap();
    assert!(!j["samples"]["hist"].as_array().unwrap().is_empty());
    assert!(out.join("report.txt").is_file());
    let seq = out.join("journal").join("prod").join("seq");
    let n = walk(&seq)
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "jrn"))
        .count();
    assert!(n >= 1, "segment files under {}", seq.display());
    let _ = std::fs::remove_dir_all(&hot);
}

fn walk(p: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut v = Vec::new();
    if let Ok(rd) = std::fs::read_dir(p) {
        for e in rd.flatten() {
            let q = e.path();
            if q.is_dir() {
                v.extend(walk(&q));
            } else {
                v.push(q);
            }
        }
    }
    v
}

#[test]
fn slc_042_check_config_exit_codes() {
    let root = support::tmp("check");
    let cfg = root.join("cfg.toml");
    std::fs::write(&cfg, CFG0).unwrap();
    let run = || {
        bin()
            .args(["--check-config", "--config"])
            .arg(&cfg)
            .env("HFT_SLICE_API_KEY", "TESTKEY")
            .env("HFT_SLICE_ED25519", ED1_SEED)
            .output()
            .unwrap()
    };
    let o = run();
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));
    std::fs::write(&cfg, format!("{CFG0}\n[orders]\nhost = \"ws-api.binance.com\"\n")).unwrap();
    let o = run();
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("mainnet"));
}

#[test]
fn slc_043_loopback_probes() {
    let o = bin().args(["--two-packet-stamp-probe", "5"]).output().unwrap();
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));
    let s = String::from_utf8_lossy(&o.stdout).into_owned();
    assert_eq!(
        s.lines()
            .filter(|l| l.starts_with("i=") && l.contains("sw_rx_ns="))
            .count(),
        5,
        "{s}"
    );
    assert!(
        s.lines()
            .last()
            .unwrap_or("")
            .starts_with("within_50us_of_second="),
        "{s}"
    );
    let o = bin().args(["--loopback-stream-minutes", "0"]).output().unwrap();
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));
    let s = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(s.lines().last().unwrap_or("").starts_with("reads="), "{s}");
}
