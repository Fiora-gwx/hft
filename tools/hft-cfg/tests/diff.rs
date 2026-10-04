//! Diff reports: TOOL-138 (engine capacity shrink).

mod common;

use common::*;

fn fx(name: &str) -> std::path::PathBuf {
    fixtures().join("engine").join(name)
}

fn engine_blob(d: &std::path::Path, name: &str, params: &str) -> std::path::PathBuf {
    std::fs::write(d.join(format!("{name}.toml")), params).unwrap();
    let out = d.join(format!("{name}.blob"));
    let r = run(&[
        "build",
        "--kind",
        "engine",
        "--in",
        p(&d.join(format!("{name}.toml"))),
        "--core-config",
        p(&fx("core.toml")),
        "--out",
        p(&out),
    ]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    out
}

#[test]
fn tool_138_capacity_shrink_note() {
    let d = scratch("tool_138");
    let base = std::fs::read_to_string(fx("engine.toml")).unwrap();
    assert!(base.contains("max_insts = 4096"));
    let old = engine_blob(&d, "old", &base);
    let new = engine_blob(
        &d,
        "new",
        &base
            .replace("max_insts = 4096", "max_insts = 2048")
            .replace("version = 3", "version = 4"),
    );
    let r = run(&["diff", "--old", p(&old), "--new", p(&new)]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(
        r.stdout.contains("capacities.max_insts: 4096 → 2048"),
        "{}",
        r.stdout
    );
    assert!(r.stdout.contains("缩容只经 Drain + 重启"), "{}", r.stdout);
    assert!(r.stdout.contains("CapacityShrink"), "{}", r.stdout);
    let j = run(&["diff", "--old", p(&old), "--new", p(&new), "--json"]);
    let v: serde_json::Value = serde_json::from_str(&j.stdout).unwrap();
    let ch = v["changes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["path"] == "capacities.max_insts")
        .unwrap();
    assert_eq!(
        (ch["old"].as_str(), ch["new"].as_str(), ch["shrink"].as_bool()),
        (Some("4096"), Some("2048"), Some(true))
    );
    // Growing a capacity carries no note.
    let r = run(&["diff", "--old", p(&new), "--new", p(&old)]);
    assert!(!r.stdout.contains("CapacityShrink"), "{}", r.stdout);
}

use common::strat::*;

/// TOOL-127 parameters: defaults and group 105's `bins` override; groups 101 (AIN), 103 (BTC),
/// 105 (AIN, `bins` overridden).
fn params(
    version: u64,
    spread_raw: i64,
    cooldown: i64,
    bins: i64,
    whitelist: i64,
    limit_x: i64,
    bins_105: i64,
) -> String {
    let spread = hft_cfg::util::fmt_fixed(spread_raw, -9);
    let mut t = base_toml()
        .replace("version = 1\n", &format!("version = {version}\n"))
        .replace(
            "spread_th = \"0.0005\"\n",
            &format!(
                "spread_th = \"{spread}\"\ncooldown_ms = \"{cooldown}\"\nbins = \"{bins}\"\nwhitelist = \"{whitelist}\"\nlimit_x = \"{limit_x}\"\n"
            ),
        );
    t.push_str(&group(103, "BTC", BTC_LEGS));
    t.push_str(&group(105, "AIN", AIN_LEGS));
    t.push_str(&format!("[groups.overrides]\nbins = \"{bins_105}\"\n"));
    t
}

const BASE: (i64, i64, i64, i64, i64, i64) = (1000, 1000, 16, 3, 7, 32);

/// Builds old (version 1, `BASE`) and new (version 2 against old) and returns the diff output.
fn diff_of(
    env: &Env,
    tag: &str,
    new: (i64, i64, i64, i64, i64, i64),
    old: Option<(i64, i64, i64, i64, i64, i64)>,
    json: bool,
) -> Out {
    let o = old.unwrap_or(BASE);
    let (_, op) = env.ok(
        &format!("{tag}-old"),
        &params(1, o.0, o.1, o.2, o.3, o.4, o.5),
        None,
    );
    let (_, np) = env.ok(
        &format!("{tag}-new"),
        &params(2, new.0, new.1, new.2, new.3, new.4, new.5),
        Some(&op),
    );
    let mut args = vec![
        "diff",
        "--old",
        p(&op),
        "--new",
        p(&np),
        "--schemas",
        p(&env.schemas),
        "--refdata",
        p(&env.refdata),
    ];
    if json {
        args.push("--json");
    }
    let r = run(&args);
    assert_eq!(r.code, 0, "{tag}: {}", r.stderr);
    r
}

#[test]
fn tool_126_pair_lot_in_report() {
    let env = Env::new("tool_126");
    let r = diff_of(&env, "d", (1100, 1000, 16, 3, 7, 32), None, false);
    assert!(
        r.stdout.contains(
            "group 0x01000065: legs binance-usdm:AINUSDT lot 0.01, bitget-uta-spot:AINUSDT lot 0.1; pair_lot 0.1"
        ),
        "{}",
        r.stdout
    );
}

#[test]
fn tool_127_minor_change() {
    let env = Env::new("tool_127");
    // (row, new values, old override, changed field, minor, structural, affected groups)
    type Case = (
        &'static str,
        (i64, i64, i64, i64, i64, i64),
        Option<(i64, i64, i64, i64, i64, i64)>,
        &'static str,
        bool,
        bool,
        &'static [&'static str],
    );
    let cases: [Case; 10] = [
        (
            "TOOL-127.1",
            (1100, 1000, 16, 3, 7, 32),
            None,
            "spread_th",
            true,
            false,
            &[],
        ),
        (
            "TOOL-127.2",
            (1101, 1000, 16, 3, 7, 32),
            None,
            "spread_th",
            false,
            false,
            &[],
        ),
        (
            "TOOL-127.3",
            (900, 1000, 16, 3, 7, 32),
            None,
            "spread_th",
            true,
            false,
            &[],
        ),
        (
            "TOOL-127.4",
            (1, 1000, 16, 3, 7, 32),
            Some((0, 1000, 16, 3, 7, 32)),
            "spread_th",
            false,
            false,
            &[],
        ),
        (
            "TOOL-127.5",
            (1000, 1100, 16, 3, 7, 32),
            None,
            "cooldown_ms",
            true,
            false,
            &[],
        ),
        (
            "TOOL-127.6",
            (1000, 1101, 16, 3, 7, 32),
            None,
            "cooldown_ms",
            false,
            false,
            &[],
        ),
        (
            "TOOL-127.7",
            (1000, 1000, 16, 3, 8, 32),
            None,
            "limit_x",
            false,
            false,
            &[],
        ),
        (
            "TOOL-127.8",
            (1000, 1000, 16, 300, 7, 32),
            None,
            "whitelist",
            true,
            false,
            &[],
        ),
        (
            "TOOL-127.9",
            (1000, 1000, 20, 3, 7, 32),
            None,
            "bins",
            false,
            true,
            &["0x01000065", "0x01000067"],
        ),
        (
            "TOOL-127.10",
            (1000, 1000, 16, 3, 7, 40),
            None,
            "bins",
            false,
            true,
            &["0x01000069"],
        ),
    ];
    for (id, new, old, field, minor, structural, affected) in cases {
        let r = diff_of(&env, id, new, old, true);
        let v: serde_json::Value = serde_json::from_str(&r.stdout).unwrap();
        let ch = v["changes"].as_array().unwrap();
        assert_eq!(ch.len(), 1, "{id}: {}", r.stdout);
        let c = &ch[0];
        assert_eq!(c["field"], field, "{id}");
        assert_eq!(c["minor"], minor, "{id}");
        assert_eq!(c["structural"], structural, "{id}");
        if structural {
            assert_eq!(c["affected_groups"], serde_json::json!(affected), "{id}");
        }
    }
}

#[test]
fn tool_128_text_and_json_agree() {
    let env = Env::new("tool_128");
    let new = (1101, 1000, 20, 3, 7, 32);
    let t = diff_of(&env, "t", new, None, false);
    let j = diff_of(&env, "j", new, None, true);
    let v: serde_json::Value = serde_json::from_str(&j.stdout).unwrap();
    let mut fields: Vec<String> = v["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["field"].as_str().unwrap().to_string())
        .collect();
    fields.sort();
    assert_eq!(fields, vec!["bins", "spread_th"]);
    let lines: Vec<&str> = t.stdout.lines().collect();
    let spread = lines.iter().find(|l| l.starts_with("spread_th ")).unwrap();
    assert!(spread.ends_with("not minor"), "{spread}");
    let bins = lines.iter().find(|l| l.starts_with("bins ")).unwrap();
    assert!(
        bins.contains("structural, affected groups [0x01000065, 0x01000067]"),
        "{bins}"
    );
    let changed: Vec<&str> = lines
        .iter()
        .filter(|l| l.contains(" → "))
        .map(|l| l.split(' ').next().unwrap())
        .collect();
    assert_eq!(changed.len(), 2, "{}", t.stdout);
    assert!(
        t.stdout.contains("升级/降级的权威判定在 sequencer"),
        "{}",
        t.stdout
    );
}
