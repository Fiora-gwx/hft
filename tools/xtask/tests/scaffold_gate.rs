//! `cargo xtask scaffold-gate` (tools/TESTS.md TOOL-020–TOOL-023, TOOL-025–TOOL-029) on the
//! `strat_repo` fixture. Builds, tests and `hft-core --check` go through the recording executor;
//! the engine-side diff is read from the fixture's real git status.

mod common;

use std::path::Path;

use common::strat_repo::{ENGINE_FILES, status, strat_repo};
use common::{Fixture, Script};

const DEC: &str = "target/decoupling";

fn hft_core(target: &Path) -> String {
    let exe = if cfg!(windows) { "hft-core.exe" } else { "hft-core" };
    target.join("debug").join(exe).display().to_string()
}

/// Runs the gate with `CARGO_TARGET_DIR = target` (default: the fixture's `target/`).
fn gate(f: &Fixture, s: &Script, keep: bool, target: Option<&str>) -> common::Run {
    let t = target.map_or_else(|| f.path("target").display().to_string(), str::to_string);
    let mut args = vec!["scaffold-gate"];
    if keep {
        args.push("--keep");
    }
    f.xtask_script(&args, s, &[("CARGO_TARGET_DIR", Some(&t))])
}

/// `(command line, env)` of every recorded command.
fn log(s: &Script) -> Vec<(String, serde_json::Value)> {
    std::fs::read_to_string(&s.log)
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .zip(s.calls())
        .map(|(v, c)| (c, v.get("env").cloned().unwrap_or_default()))
        .collect()
}

#[test]
fn tool_020_probe_generated_in_test_range() {
    let f = strat_repo("sg-020");
    let s = Script::new(&f, &[], 0);
    let r = gate(&f, &s, true, None);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let p = f.path(DEC).join("strategies/ci-probe");
    for rel in ["src/lib.rs", "src/model/mod.rs", "tests/scenario.rs"] {
        assert!(p.join(rel).is_file(), "missing {rel}");
    }
    assert!(p.join("pipeline").is_dir());
    let j = r.json.unwrap();
    let args: Vec<String> = j["new_strategy_args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    for want in ["ci-probe", "--with-model", "--with-pipeline"] {
        assert!(args.iter().any(|a| a == want), "{want} not in {args:?}");
    }
    let i = args.iter().position(|a| a == "--kind-id").unwrap();
    let k = u32::from_str_radix(args[i + 1].trim_start_matches("0x"), 16).unwrap();
    assert!((0xFF00..=0xFFFF).contains(&k), "kind_id {k:#x}");
}

#[test]
fn tool_021_registry_copy_and_shared_target() {
    let f = strat_repo("sg-021");
    let s = Script::new(&f, &[], 0);
    // The main target directory ("/w/target" in the spec), absolute on every host.
    let main_target = f.path("w").join("target").display().to_string();
    let r = gate(&f, &s, true, Some(&main_target));
    assert_eq!(r.code, 0, "{}", r.stderr);
    // Registry copy: the repository list plus one last line `ci_probe::CiProbe`.
    let orig: Vec<String> = f
        .read("strategies/registry/src/lib.rs")
        .lines()
        .map(str::to_string)
        .collect();
    let copy: Vec<String> = f
        .read(&format!("{DEC}/registry/src/lib.rs"))
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(copy.len(), orig.len() + 1);
    let added = copy.iter().position(|l| l.contains("CiProbe")).unwrap();
    assert_eq!(copy[added].trim(), "ci_probe::CiProbe,");
    let mut without = copy.clone();
    without.remove(added);
    assert_eq!(without, orig, "the other lines are unchanged");
    assert!(copy[added + 1].trim() == "}", "CiProbe is the last entry");
    // Temporary workspace: registry copy, probe and both app manifest copies; the apps'
    // registry dependency points at the copy.
    let ws: toml::Table = f.read(&format!("{DEC}/Cargo.toml")).parse().unwrap();
    let members: Vec<&str> = ws["workspace"]["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    for m in [
        "registry",
        "strategies/ci-probe",
        "apps/hft-core",
        "apps/replayer",
    ] {
        assert!(members.contains(&m), "{m} not in {members:?}");
    }
    assert_eq!(
        ws["workspace"]["dependencies"]["hft-strategy-registry"]["path"].as_str(),
        Some("registry")
    );
    let rep: toml::Table = f
        .read(&format!("{DEC}/apps/replayer/Cargo.toml"))
        .parse()
        .unwrap();
    assert_eq!(
        rep["dependencies"]["hft-strategy-registry"]["path"].as_str(),
        Some("../../registry")
    );
    let core: toml::Table = f
        .read(&format!("{DEC}/apps/hft-core/Cargo.toml"))
        .parse()
        .unwrap();
    assert_eq!(
        core["dependencies"]["hft-strategy-registry"]["workspace"].as_bool(),
        Some(true)
    );
    // Every build and test runs on the main target directory; none under target/decoupling/.
    let calls = log(&s);
    let cargo: Vec<_> = calls.iter().filter(|(c, _)| c.starts_with("cargo ")).collect();
    assert!(cargo.len() >= 3, "{calls:?}");
    for (c, env) in &cargo {
        assert_eq!(
            env["CARGO_TARGET_DIR"].as_str(),
            Some(main_target.as_str()),
            "{c}"
        );
    }
    assert!(!f.path(DEC).join("target").exists());
}

#[test]
fn tool_022_scenario_dst_check_order() {
    let f = strat_repo("sg-022");
    let s = Script::new(&f, &[], 0);
    let r = gate(&f, &s, false, None);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let calls = s.calls();
    let at = |pat: &str| {
        calls
            .iter()
            .position(|c| c.contains(pat))
            .unwrap_or_else(|| panic!("{pat} not in {calls:?}"))
    };
    let scen = at("-p ci-probe --test scenario");
    let dst = at("-p hft-strategy-registry --test ci_probe_dst");
    let check = at(" --check --config ");
    assert!(scen < dst && dst < check, "{calls:?}");
    assert!(
        calls[check].starts_with(&hft_core(&f.path("target"))),
        "{}",
        calls[check]
    );
    assert!(calls[check].ends_with("sim-check.toml"), "{}", calls[check]);
    let j = r.json.unwrap();
    assert_eq!(j["dst_seeds"], 100);
    let acct = &j["sim_account"];
    assert!(acct["label"].as_str().unwrap().starts_with("sim-"), "{acct}");
    assert_eq!(acct["kind"], "sim");
    assert_eq!(acct["bound_instance"], "ci-probe-sim");
}

#[test]
fn tool_023_check_failure_fails_gate() {
    let f = strat_repo("sg-023");
    let check = format!("{} --check", hft_core(&f.path("target")));
    let s = Script::new(&f, &[(check.as_str(), 2, "", "")], 0);
    let r = gate(&f, &s, false, None);
    assert_eq!(r.code, 1, "{}", r.stderr);
    assert!(r.stderr.contains("hft-core --check"), "{}", r.stderr);
    assert!(r.stderr.contains("exited 2"), "{}", r.stderr);
}

#[test]
fn tool_025_clean_run_leaves_no_diff() {
    let f = strat_repo("sg-025");
    let s = Script::new(&f, &[], 0);
    let r = gate(&f, &s, false, None);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let diff = common::repos::git(
        &f,
        &[
            "diff",
            "--stat",
            "--",
            "crates",
            "apps",
            "tools/hft-cfg",
            "python",
        ],
    );
    assert_eq!(diff, "");
    assert_eq!(status(&f, &["strategies/registry"]), Vec::<String>::new());
    assert!(!f.path(DEC).exists());
}

#[test]
fn tool_026_keep_retains_dir() {
    let f = strat_repo("sg-026");
    let s = Script::new(&f, &[], 0);
    let r = gate(&f, &s, true, None);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(f.path(DEC).join("strategies/ci-probe").is_dir());
    assert!(f.path(DEC).join("registry").is_dir());
}

#[test]
fn tool_027_engine_diff_fails() {
    let rows = [
        ("TOOL-027.1", ENGINE_FILES[0], 1),
        ("TOOL-027.2", ENGINE_FILES[1], 1),
        ("TOOL-027.3", ENGINE_FILES[2], 1),
        ("TOOL-027.4", ENGINE_FILES[3], 1),
        ("TOOL-027.5", "strategies/ci-probe/pipeline/README.md", 0),
    ];
    for (id, file, want) in rows {
        let f = strat_repo(&id.to_lowercase().replace('.', "-"));
        f.append(file, "// touched while generating the probe\n");
        let s = Script::new(&f, &[], 0);
        let r = gate(&f, &s, false, None);
        assert_eq!(r.code, want, "{id}: {}", r.stderr);
        let j = r.json.unwrap();
        let listed: Vec<&str> = j["engine_changes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        if want == 1 {
            assert!(r.stderr.contains(file), "{id}: stderr lacks {file}: {}", r.stderr);
            assert_eq!(listed, vec![file], "{id}");
        } else {
            assert!(listed.is_empty(), "{id}: {listed:?}");
        }
    }
}

#[test]
fn tool_028_probe_depends_on_oms() {
    let f = strat_repo("sg-028");
    f.append(
        "strategies/_template/base/Cargo.toml",
        "hft-oms = { path = \"../../crates/hft-oms\" }\n",
    );
    let s = Script::new(&f, &[], 0);
    let r = gate(&f, &s, false, None);
    assert_eq!(r.code, 1, "{}", r.stderr);
    assert!(r.stderr.contains("ci-probe → hft-oms"), "{}", r.stderr);
}

#[test]
fn tool_029_probe_shape_required() {
    let rows = [
        ("TOOL-029.1", "strategies/_template/model", "no Model skeleton"),
        (
            "TOOL-029.2",
            "strategies/_template/pipeline",
            "no pipeline plugin",
        ),
    ];
    for (id, layer, msg) in rows {
        let f = strat_repo(&id.to_lowercase().replace('.', "-"));
        std::fs::remove_dir_all(f.path(layer)).unwrap();
        let s = Script::new(&f, &[], 0);
        let r = gate(&f, &s, false, None);
        assert_eq!(r.code, 1, "{id}: {}", r.stderr);
        assert!(r.stderr.contains(msg), "{id}: `{msg}` not in {}", r.stderr);
    }
}
