//! Schema export, `index.json` and the committed goldens (TESTS.md REG-005, REG-006, REG-008–REG-010,
//! REG-014). `HFT_BLESS=1` rewrites `fixtures/strategy-schemas/` from the current export.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::type_complexity
)] // allow-list: test code

use std::path::{Path, PathBuf};

use hft_strategy_registry::{Strategies, TEST_KIND_IDS, export_schemas};
use hft_strategy_sdk::{ModelLayoutDesc, Strategy, StrategySet, params};
use latency_probe::LatencyProbe;

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/strategy-schemas")
}

/// Golden comparison failure.
#[derive(Debug, PartialEq, Eq)]
enum GoldenErr {
    /// The digest changed but `schema_version` did not.
    VersionNotBumped { kind_id: u16 },
    /// A file differs, is missing or is extra.
    GoldenStale { file: String },
}

/// `(kind_id, schema_version, schema_digest)` of every `index.json` element.
fn index_entries(index: &str) -> Vec<(u16, u64, String)> {
    let v: serde_json::Value = serde_json::from_str(index).unwrap();
    v.as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["kind_id"].as_u64().unwrap() as u16,
                e["schema_version"].as_u64().unwrap(),
                e["schema_digest"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

/// Compares an export with a golden directory: a digest change without a version bump first, then
/// file by file (missing, extra or different files are stale).
fn compare_golden(export: &[(String, String)], dir: &Path) -> Result<(), GoldenErr> {
    let index = |files: &[(String, String)]| files.iter().find(|f| f.0 == "index.json").map(|f| f.1.clone());
    let mut golden: Vec<(String, String)> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let text = std::fs::read_to_string(e.path()).unwrap();
            (name, text)
        })
        .collect();
    golden.sort();
    // The committed catalogue contains every implemented optional kind. A default build
    // compares its projection; unknown/extra files still fail the ordinary comparison.
    if dir == golden_dir() && !cfg!(feature = "cross-taker") {
        golden.retain(|(n, _)| n != "3-cross-taker.schema.json");
        if let Some((_, index)) = golden.iter_mut().find(|(n, _)| n == "index.json") {
            let rows: Vec<serde_json::Value> = serde_json::from_str(index).unwrap();
            let rows: Vec<String> = rows.iter().filter(|r| r["kind_id"] != 3).map(|r| format!(
                "{{\"kind_id\":{},\"name\":{},\"file\":{},\"schema_id\":{},\"schema_version\":{},\"schema_digest\":{}}}",
                r["kind_id"], r["name"], r["file"], r["schema_id"], r["schema_version"], r["schema_digest"]
            )).collect();
            *index = format!("[{}]", rows.join(","));
        }
    }
    if let (Some(a), Some(b)) = (index(export), index(&golden)) {
        let (a, b) = (index_entries(&a), index_entries(&b));
        for (k, ver, dig) in &a {
            if let Some((_, gver, gdig)) = b.iter().find(|x| x.0 == *k)
                && gdig != dig
                && gver == ver
            {
                return Err(GoldenErr::VersionNotBumped { kind_id: *k });
            }
        }
    }
    let mut names: Vec<&String> = export
        .iter()
        .map(|f| &f.0)
        .chain(golden.iter().map(|f| &f.0))
        .collect();
    names.sort();
    names.dedup();
    // Strategy files before index.json.
    names.sort_by_key(|n| (n.as_str() == "index.json", (*n).clone()));
    for n in names {
        let a = export.iter().find(|f| &f.0 == n).map(|f| &f.1);
        let b = golden.iter().find(|f| &f.0 == n).map(|f| &f.1);
        if a != b {
            return Err(GoldenErr::GoldenStale { file: n.clone() });
        }
    }
    Ok(())
}

/// Kind ids in the test-strategy range found in an `index.json`.
fn check_no_test_range(index: &str) -> Result<(), Vec<u16>> {
    let bad: Vec<u16> = index_entries(index)
        .into_iter()
        .map(|e| e.0)
        .filter(|k| TEST_KIND_IDS.contains(k))
        .collect();
    if bad.is_empty() { Ok(()) } else { Err(bad) }
}

fn lp_json() -> String {
    let d = <LatencyProbe as Strategy>::DESC;
    params::schema_json(&d, &ModelLayoutDesc::EMPTY, d.schema.version)
}

#[test]
fn reg_005_export_list() {
    let e = export_schemas();
    assert_eq!(e.len(), Strategies::kinds().len() + 1);
    let mut kinds: Vec<u16> = Strategies::kinds().iter().map(|d| d.kind_id).collect();
    kinds.sort_unstable();
    for (f, k) in e.iter().zip(&kinds) {
        assert!(f.0.starts_with(&format!("{k}-")), "{} for kind {k}", f.0);
    }
    assert_eq!(e.last().unwrap().0, "index.json");
    let lp = e.iter().find(|f| f.0 == "5-latency-probe.schema.json").unwrap();
    assert_eq!(lp.1, lp_json());
}

#[test]
fn reg_006_index_format() {
    let e = export_schemas();
    let idx = &e.last().unwrap().1;
    assert!(idx.starts_with('['));
    let mut in_str = false;
    for c in idx.chars() {
        if c == '"' {
            in_str = !in_str;
        }
        assert!(in_str || !c.is_whitespace(), "whitespace outside strings");
    }
    let v: serde_json::Value = serde_json::from_str(idx).unwrap();
    let arr = v.as_array().unwrap();
    let mut last = 0u64;
    for el in arr {
        let keys: Vec<&String> = el.as_object().unwrap().keys().collect();
        assert_eq!(keys.len(), 6);
        last = last.max(el["kind_id"].as_u64().unwrap());
    }
    // Key order as written.
    let k5 = idx.find("{\"kind_id\":5,").unwrap();
    let el = &idx[k5..idx[k5..].find('}').unwrap() + k5 + 1];
    let order = [
        "\"kind_id\"",
        "\"name\"",
        "\"file\"",
        "\"schema_id\"",
        "\"schema_version\"",
        "\"schema_digest\"",
    ];
    let pos: Vec<usize> = order.iter().map(|k| el.find(k).unwrap()).collect();
    assert!(pos.windows(2).all(|w| w[0] < w[1]), "{el}");
    let lp = arr.iter().find(|e| e["kind_id"] == 5).unwrap();
    assert_eq!(lp["name"], "latency-probe");
    assert_eq!(lp["file"], "5-latency-probe.schema.json");
    assert_eq!(
        (lp["schema_id"].as_u64(), lp["schema_version"].as_u64()),
        (Some(5), Some(1))
    );
    let d = lp["schema_digest"].as_str().unwrap();
    assert_eq!(
        d,
        format!(
            "{:016x}",
            params::schema_digest(&<LatencyProbe as Strategy>::DESC, 1)
        )
    );
    assert!(
        d.len() == 16
            && d.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    );
    let ids: Vec<u64> = arr.iter().map(|e| e["kind_id"].as_u64().unwrap()).collect();
    assert!(ids.windows(2).all(|w| w[0] < w[1]));
    assert!(last >= 5);
}

#[test]
fn reg_008_golden() {
    let e = export_schemas();
    let dir = golden_dir();
    if std::env::var_os("HFT_BLESS").is_some() {
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (n, t) in &e {
            std::fs::write(dir.join(n), t).unwrap();
        }
    }
    assert_eq!(compare_golden(&e, &dir), Ok(()));
}

/// A temporary copy of the goldens edited by `f`.
fn with_golden(tag: &str, f: impl Fn(&Path)) -> Result<(), GoldenErr> {
    let dir = std::env::temp_dir().join(format!("reg-009-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (n, t) in export_schemas() {
        std::fs::write(dir.join(n), t).unwrap();
    }
    f(&dir);
    let r = compare_golden(&export_schemas(), &dir);
    let _ = std::fs::remove_dir_all(&dir);
    r
}

fn replace_in(dir: &Path, file: &str, from: &str, to: &str) {
    let p = dir.join(file);
    let t = std::fs::read_to_string(&p).unwrap();
    assert!(t.contains(from), "{file}: {from}");
    std::fs::write(&p, t.replacen(from, to, 1)).unwrap();
}

#[test]
fn reg_009_golden_discipline() {
    let lp = "5-latency-probe.schema.json";
    let digest = format!(
        "{:016x}",
        params::schema_digest(&<LatencyProbe as Strategy>::DESC, 1)
    );
    assert_eq!(with_golden("1", |_| {}), Ok(()), "REG-009.1");
    let r = with_golden("2", |d| replace_in(d, "index.json", &digest, "0000000000000001"));
    assert_eq!(r, Err(GoldenErr::VersionNotBumped { kind_id: 5 }), "REG-009.2");
    let r = with_golden("3", |d| {
        replace_in(d, "index.json", &digest, "0000000000000001");
        replace_in(
            d,
            "index.json",
            "\"schema_version\":1,\"schema_digest\":\"0000000000000001\"",
            "\"schema_version\":0,\"schema_digest\":\"0000000000000001\"",
        );
        replace_in(d, lp, "\"schema_version\":1", "\"schema_version\":0");
    });
    assert_eq!(r, Err(GoldenErr::GoldenStale { file: lp.into() }), "REG-009.3");
    let r = with_golden("4", |d| {
        replace_in(d, lp, "\"name\":\"ProbeFilled\"", "\"name\":\"ProbeFill\"")
    });
    assert_eq!(r, Err(GoldenErr::GoldenStale { file: lp.into() }), "REG-009.4");
    let r = with_golden("5", |d| {
        std::fs::write(d.join("9-foo.schema.json"), "{}").unwrap()
    });
    assert_eq!(
        r,
        Err(GoldenErr::GoldenStale {
            file: "9-foo.schema.json".into()
        }),
        "REG-009.5"
    );
}

#[test]
fn reg_010_no_test_range() {
    let el = |k: u32| {
        format!(
            "{{\"kind_id\":{k},\"name\":\"x\",\"file\":\"x\",\"schema_id\":{k},\"schema_version\":1,\"schema_digest\":\"0000000000000000\"}}"
        )
    };
    let idx = |ks: &[u32]| format!("[{}]", ks.iter().map(|k| el(*k)).collect::<Vec<_>>().join(","));
    assert_eq!(check_no_test_range(&idx(&[5])), Ok(()), "REG-010.1");
    assert_eq!(
        check_no_test_range(&idx(&[5, 0xFF00])),
        Err(vec![65_280]),
        "REG-010.2"
    );
    assert_eq!(
        check_no_test_range(&idx(&[5, 0xFFFF])),
        Err(vec![65_535]),
        "REG-010.3"
    );
    assert_eq!(check_no_test_range(&idx(&[5, 0xFEFF])), Ok(()), "REG-010.4");
    let e = export_schemas();
    assert_eq!(check_no_test_range(&e.last().unwrap().1), Ok(()), "REG-010.5");
    let g = std::fs::read_to_string(golden_dir().join("index.json")).unwrap();
    assert_eq!(check_no_test_range(&g), Ok(()), "REG-010.5");
}

#[test]
fn reg_014_get_schema_same() {
    let e = export_schemas();
    let lp = &e.iter().find(|f| f.0 == "5-latency-probe.schema.json").unwrap().1;
    assert_eq!(Strategies::schema_json(5).as_ref(), Some(lp), "REG-014.1");
    assert_eq!(Strategies::schema_json(0xFF01), None, "REG-014.2");
    assert_eq!(Strategies::schema_json(8), None, "REG-014.3");
}
