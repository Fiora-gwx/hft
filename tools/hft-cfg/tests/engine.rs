//! TOOL-136, TOOL-137, TOOL-144: `build --kind engine` and the three hft-core config values.

mod common;

use common::*;
use hft_refdata::{EngineConfig, EngineSection};

fn fx(name: &str) -> std::path::PathBuf {
    fixtures().join("engine").join(name)
}

fn u32_at(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(b[off..off + 4].try_into().unwrap())
}

#[test]
fn tool_136_core_config_fields() {
    let d = scratch("tool_136");
    let out = d.join("e.blob");
    let r = run(&[
        "build",
        "--kind",
        "engine",
        "--in",
        p(&fx("engine.toml")),
        "--core-config",
        p(&fx("core.toml")),
        "--out",
        p(&out),
    ]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let blob = std::fs::read(&out).unwrap();
    let ec = EngineConfig::load(&blob, None).unwrap();
    let s5 = ec.section(EngineSection::Sequencer).unwrap();
    assert_eq!((s5.row_ver, s5.row_size, s5.count), (1, 72, 1));
    assert_eq!(u32_at(s5.bytes, 48), 256, "advisor_mem_budget_mib");
    assert_eq!(u32_at(s5.bytes, 52), 64, "advisor_ckpt_max_mib");
    assert_eq!(s5.bytes[65], 0, "allow_missing_standby");
    assert_eq!(s5.bytes[64], 1, "auto_resume_after_clean_drain from the params");
    assert_eq!(u32_at(s5.bytes, 32), 50_000, "md_round_ns from the params");
}

#[test]
fn tool_137_core_config_missing_key() {
    let core = std::fs::read_to_string(fx("core.toml")).unwrap();
    for (row, line) in [
        ("TOOL-137.1", "mem_budget_mib = 256"),
        ("TOOL-137.2", "checkpoint_max_mib = 64"),
        ("TOOL-137.3", "allow_missing_standby = false"),
    ] {
        let d = scratch(&format!("tool_137_{row}"));
        assert!(core.contains(line), "{row}");
        std::fs::write(d.join("core.toml"), core.replace(line, "")).unwrap();
        let out = d.join("e.blob");
        let r = run(&[
            "build",
            "--kind",
            "engine",
            "--in",
            p(&fx("engine.toml")),
            "--core-config",
            p(&d.join("core.toml")),
            "--out",
            p(&out),
        ]);
        assert_eq!(r.code, 2, "{row}: {}", r.stderr);
        assert!(!out.exists(), "{row}: no blob");
    }
}

#[test]
fn tool_144_core_keys_not_in_params() {
    let d = scratch("tool_144");
    let params = std::fs::read_to_string(fx("engine.toml"))
        .unwrap()
        .replace("[sequencer]\n", "[sequencer]\nadvisor_mem_budget_mib = 128\n");
    std::fs::write(d.join("engine.toml"), params).unwrap();
    let out = d.join("e.blob");
    let r = run(&[
        "build",
        "--kind",
        "engine",
        "--in",
        p(&d.join("engine.toml")),
        "--core-config",
        p(&fx("core.toml")),
        "--out",
        p(&out),
    ]);
    assert_eq!(r.code, 1, "{}", r.stderr);
    assert!(r.stderr.contains("advisor_mem_budget_mib"), "{}", r.stderr);
    assert!(r.stderr.contains("--core-config"), "{}", r.stderr);
    assert!(!out.exists());
}
