//! `cargo xtask test-trace` (tools/TESTS.md TOOL-086–TOOL-091, TOOL-180; rules: docs/tests/README.md §6).
//! Report-only: the three lists never change the exit code; hard spec errors do (TOOL-090.4–.6).

use crate::common;

use common::{Fixture, Run};
use serde_json::{Value, json};

const A_TESTS: &str = "crates/a/TESTS.md";
const A_X: &str = "crates/a/tests/x.rs";
const CONFIG: &str = "tools/xtask/test-trace.toml";

fn case(id: &str, level: &str, ms: &str, extra: &str, implementation: &str) -> String {
    format!(
        "### {id} case {id}\n\n- 规则:[r](README.md#r)\n- 级别:{level}\n- 里程碑:{ms}\n{extra}- 实现:{implementation}\n\n"
    )
}

/// Fixture `trace-clean` (TOOL-086).
fn trace_clean() -> Fixture {
    let f = Fixture::empty("trace-clean");
    f.write(CONFIG, "current_milestone = \"M2\"\nenabled_extensions = []\n");
    let mut spec =
        String::from("---\ncomponent: a\nprefix: AA\nspec: README.md\n---\n\n# a 测试规格\n\n## 用例\n\n");
    spec += &case("AA-001", "unit", "M1", "", "`crates/a/tests/x.rs::aa_001_ok`");
    spec += &case(
        "AA-002",
        "dst",
        "M2",
        "",
        "`crates/hft-testkit/src/scenarios/a.rs::aa_002_scn`",
    );
    spec += &case(
        "AA-003",
        "bench",
        "M2",
        "- 基准名:`a/hot`\n",
        "`crates/a/benches/hot.rs::aa_003_hot`",
    );
    spec += &case("AA-004", "host", "M2", "", "`tests/host/AA-004.sh`");
    f.write(A_TESTS, &spec);
    f.write(
        "docs/tests/scenarios.md",
        &(String::from("# 跨组件场景\n\n## 场景\n\n")
            + &case(
                "B-1",
                "dst",
                "M2",
                "",
                "`crates/hft-testkit/src/scenarios/b.rs::b_1_scn`",
            )),
    );
    f.write(A_X, "#[test]\nfn aa_001_ok() {\n    assert_eq!(1, 1);\n}\n");
    f.write(
        "crates/hft-testkit/src/scenarios/a.rs",
        "pub fn aa_002_scn() -> Scenario {\n    Scenario::new(\"AA-002\", |_| ())\n}\n",
    );
    f.write(
        "crates/hft-testkit/src/scenarios/b.rs",
        "pub fn b_1_scn() -> Scenario {\n    Scenario::new(\"B-1\", |_| ())\n}\n",
    );
    f.write(
        "crates/a/benches/hot.rs",
        "fn aa_003_hot(c: &mut Criterion) {\n    c.bench_function(\"a/hot\", |b| b.iter(|| 1));\n}\n",
    );
    f.write("tests/host/AA-004.sh", "#!/bin/sh\nexit 0\n");
    f
}

fn list(r: &Run, key: &str) -> Vec<Value> {
    r.json.as_ref().expect("test-trace.json written")[key]
        .as_array()
        .expect("list")
        .clone()
}

fn ids(r: &Run, key: &str) -> Vec<String> {
    list(r, key)
        .iter()
        .map(|v| match v {
            Value::String(s) => s.clone(),
            other => other["id"].as_str().unwrap().to_string(),
        })
        .collect()
}

#[test]
fn tool_086_clean_passes() {
    let f = trace_clean();
    let r = f.xtask(&["test-trace"]);
    assert_eq!(r.code, 0, "stderr:\n{}", r.stderr);
    for key in ["unimplemented", "extra", "inconsistent"] {
        assert!(list(&r, key).is_empty(), "{key} not empty: {:?}", r.json);
    }
}

#[test]
fn tool_087_unimplemented() {
    type Edit = fn(&Fixture);
    /// (row, edit, exit code, exact `unimplemented`, id it must contain)
    type Row = (
        &'static str,
        Edit,
        i32,
        Option<&'static [&'static str]>,
        &'static str,
    );
    let rows: [Row; 6] = [
        (
            "TOOL-087.1",
            |f| {
                f.remove(A_X);
            },
            0,
            Some(&["AA-001"]),
            "AA-001",
        ),
        (
            "TOOL-087.2",
            |f| {
                f.replace(A_TESTS, "`crates/a/tests/x.rs::aa_001_ok`", "待实现");
            },
            0,
            None,
            "AA-001",
        ),
        (
            "TOOL-087.3",
            |f| {
                f.replace(
                    "crates/hft-testkit/src/scenarios/b.rs",
                    "Scenario::new(\"B-1\", |_| ())",
                    "()",
                );
            },
            0,
            Some(&["B-1"]),
            "B-1",
        ),
        (
            "TOOL-087.4",
            |f| {
                f.replace("crates/a/benches/hot.rs", "\"a/hot\"", "\"a/cold\"");
            },
            0,
            Some(&["AA-003"]),
            "AA-003",
        ),
        (
            "TOOL-087.5",
            |f| {
                f.replace(A_X, "#[test]\n", "#[test]\n#[ignore]\n");
            },
            0,
            Some(&["AA-001"]),
            "AA-001",
        ),
        (
            "TOOL-087.6",
            |f| {
                f.replace(A_X, "#[test]\n", "#[test]\n#[ignore]\n");
                f.replace(
                    A_TESTS,
                    "- 里程碑:M1\n",
                    "- 里程碑:M1\n- 状态:隔离:[flaky](../../.agents/notes/proposed/process/x.md)\n",
                );
            },
            0,
            Some(&[]),
            "",
        ),
    ];
    for (row, edit, code, exact, contains) in rows {
        let f = trace_clean();
        edit(&f);
        let r = f.xtask(&["test-trace"]);
        assert_eq!(r.code, code, "{row}: stderr:\n{}", r.stderr);
        let got = ids(&r, "unimplemented");
        if let Some(want) = exact {
            assert_eq!(got, want, "{row}");
        }
        if !contains.is_empty() {
            assert!(got.iter().any(|x| x == contains), "{row}: {got:?}");
        }
    }
}

#[test]
fn tool_088_not_required() {
    let aa005 = case("AA-005", "unit", "M3", "", "待实现");
    let aa006 = case("AA-006", "unit", "M1.5", "", "待实现");
    /// (row, appended case, current_milestone, drop host script, exit code, `unimplemented`)
    type Row = (
        &'static str,
        String,
        &'static str,
        bool,
        i32,
        &'static [&'static str],
    );
    let rows: [Row; 4] = [
        ("TOOL-088.1", aa005.clone(), "M2", false, 0, &[]),
        ("TOOL-088.2", aa005, "M3", false, 0, &["AA-005"]),
        ("TOOL-088.3", aa006, "M1", false, 0, &[]),
        ("TOOL-088.4", String::new(), "M2", true, 0, &[]),
    ];
    for (row, add, ms, drop_script, code, want) in rows {
        let f = trace_clean();
        f.append(A_TESTS, &add);
        f.write(
            CONFIG,
            &format!("current_milestone = \"{ms}\"\nenabled_extensions = []\n"),
        );
        if drop_script {
            f.remove("tests/host/AA-004.sh");
        }
        let r = f.xtask(&["test-trace"]);
        assert_eq!(r.code, code, "{row}: stderr:\n{}", r.stderr);
        assert_eq!(ids(&r, "unimplemented"), want, "{row}");
    }
}

#[test]
fn tool_089_extra() {
    let rows: [(&str, &str, i32, &[&str]); 5] = [
        (
            "TOOL-089.1",
            "\n#[test]\nfn aa_099_orphan() {}\n",
            0,
            &["aa_099_orphan"],
        ),
        (
            "TOOL-089.2",
            "\n#[test]\nfn zz_001_unknown_prefix() {}\n",
            0,
            &["zz_001_unknown_prefix"],
        ),
        (
            "TOOL-089.3",
            "\nproptest! {\n    fn aa_098_prop(x in 0u8..4) {\n        prop_assert!(x < 4);\n    }\n}\n",
            0,
            &["aa_098_prop"],
        ),
        ("TOOL-089.4", "\n#[test]\nfn helper_case() {}\n", 0, &[]),
        ("TOOL-089.5", "\nfn aa_097_helper() {}\n", 0, &[]),
    ];
    for (row, code_text, code, want) in rows {
        let f = trace_clean();
        f.append(A_X, code_text);
        let r = f.xtask(&["test-trace"]);
        assert_eq!(r.code, code, "{row}: stderr:\n{}", r.stderr);
        let got = ids(&r, "extra");
        for w in want {
            assert!(got.iter().any(|x| x == w), "{row}: {got:?}");
        }
        if want.is_empty() {
            assert!(got.is_empty(), "{row}: {got:?}");
        }
    }
}

#[test]
fn tool_090_inconsistent() {
    // TOOL-090.1: moved to y.rs.
    let f = trace_clean();
    let body = f.read(A_X);
    f.remove(A_X).write("crates/a/tests/y.rs", &body);
    let r = f.xtask(&["test-trace"]);
    assert_eq!(r.code, 0, "TOOL-090.1: {}", r.stderr);
    assert_eq!(
        list(&r, "inconsistent"),
        vec![json!({
            "id": "AA-001",
            "spec": "crates/a/tests/x.rs::aa_001_ok",
            "found": ["crates/a/tests/y.rs::aa_001_ok"],
        })],
        "TOOL-090.1"
    );

    // TOOL-090.2: renamed.
    let f = trace_clean();
    f.replace(A_X, "fn aa_001_ok", "fn aa_001_renamed");
    let r = f.xtask(&["test-trace"]);
    assert_eq!(r.code, 0, "TOOL-090.2: {}", r.stderr);
    assert_eq!(ids(&r, "inconsistent"), ["AA-001"], "TOOL-090.2");

    // TOOL-090.3: defined twice.
    let f = trace_clean();
    let body = f.read(A_X);
    f.write("crates/a/tests/y.rs", &body);
    let r = f.xtask(&["test-trace"]);
    assert_eq!(r.code, 0, "TOOL-090.3: {}", r.stderr);
    assert_eq!(ids(&r, "inconsistent"), ["AA-001"], "TOOL-090.3");

    // TOOL-090.4..6: hard spec errors fail the run; the report lists them under `errors`.
    type Edit = fn(&Fixture);
    let rows: [(&str, Edit, &str); 3] = [
        (
            "TOOL-090.4",
            |f| {
                f.replace(A_TESTS, "`crates/a/tests/x.rs::aa_001_ok`", "见 x.rs");
            },
            "AA-001: 实现",
        ),
        (
            "TOOL-090.5",
            |f| {
                f.append(A_TESTS, &case("AA-001", "unit", "M1", "", "待实现"));
            },
            "AA-001: case id defined twice",
        ),
        (
            "TOOL-090.6",
            |f| {
                f.replace(A_TESTS, "- 里程碑:M1\n", "- 里程碑:第二期\n");
            },
            "AA-001: unparsable 里程碑",
        ),
    ];
    for (row, edit, msg) in rows {
        let f = trace_clean();
        edit(&f);
        let r = f.xtask(&["test-trace"]);
        assert_eq!(r.code, 1, "{row}: {}", r.stderr);
        let errors = list(&r, "errors");
        assert_eq!(errors.len(), 1, "{row}: {errors:?}");
        assert!(errors[0].as_str().unwrap().contains(msg), "{row}: {errors:?}");
        assert!(r.stderr.contains(msg), "{row}: {}", r.stderr);
    }
}

#[test]
fn tool_091_reads_docs_tests() {
    let f = trace_clean();
    f.write(
        "docs/tests/invariants.md",
        &(String::from("# 不变量\n\n## 不变量\n\n")
            + &case(
                "INV-1",
                "prop",
                "M1",
                "- 性质:p\n- 生成:g\n- 参考:r\n- 例数:1\n",
                "待实现",
            )),
    );
    let r = f.xtask(&["test-trace"]);
    assert_eq!(r.code, 0, "report-only: {}", r.stderr);
    assert_eq!(ids(&r, "unimplemented"), ["INV-1"]);
    let obj = r.json.as_ref().unwrap().as_object().unwrap();
    let mut keys: Vec<&String> = obj.keys().filter(|k| obj[*k].is_array()).collect();
    keys.sort();
    assert_eq!(keys, ["errors", "extra", "inconsistent", "unimplemented"]);
    assert_eq!(obj.len(), 5);
    // Per milestone: M1 = AA-001 + INV-1 (one unimplemented), M2 = AA-002, AA-003, B-1.
    assert_eq!(
        obj["by_milestone"],
        json!({
            "M1": {"cases": 2, "implemented": 1, "unimplemented": 1},
            "M2": {"cases": 3, "implemented": 3, "unimplemented": 0},
        })
    );
    assert!(
        r.stderr
            .contains("unimplemented=1 extra=0 inconsistent=0 (report-only) errors=0"),
        "{}",
        r.stderr
    );
    assert!(r.stderr.contains("M1    cases=2"), "{}", r.stderr);
    assert!(r.stdout.is_empty(), "stdout: {}", r.stdout);
}

#[test]
fn tool_180_optional_and_python() {
    const PY: &str = "python/p/tests/test_x.py";
    let setup = |exts: &str| {
        let f = trace_clean();
        f.write(
            CONFIG,
            &format!("current_milestone = \"M2\"\nenabled_extensions = {exts}\n"),
        );
        f.append(
            A_TESTS,
            &case(
                "AA-010",
                "unit",
                "M2",
                "- 状态:可选扩展:incr-depth\n",
                "`crates/a/tests/x.rs::aa_010_incr`",
            ),
        );
        f.write(
            "python/p/TESTS.md",
            &(String::from("# p 测试规格\n\n## 用例\n\n")
                + &case(
                    "PP-001",
                    "unit",
                    "M2",
                    "",
                    "`python/p/tests/test_x.py::test_pp_001_ok`",
                )),
        );
        f.write(PY, "def test_pp_001_ok():\n    assert True\n");
        f
    };
    let with_incr = "\n#[test]\nfn aa_010_incr() {}\n";
    type Edit = fn(&Fixture, &str);
    /// (row, enabled_extensions, edit, exit code, list key, list)
    type Row = (
        &'static str,
        &'static str,
        Edit,
        i32,
        &'static str,
        &'static [&'static str],
    );
    let rows: [Row; 8] = [
        ("TOOL-180.1", "[]", |_, _| {}, 0, "", &[]),
        (
            "TOOL-180.2",
            "[\"incr-depth\"]",
            |_, _| {},
            0,
            "unimplemented",
            &["AA-010"],
        ),
        (
            "TOOL-180.3",
            "[\"incr-depth\"]",
            |f, t| {
                f.append(A_X, t);
            },
            0,
            "",
            &[],
        ),
        (
            "TOOL-180.4",
            "[]",
            |f, t| {
                f.append(A_X, t);
            },
            0,
            "",
            &[],
        ),
        (
            "TOOL-180.5",
            "[]",
            |f, _| {
                f.remove(PY);
            },
            0,
            "unimplemented",
            &["PP-001"],
        ),
        (
            "TOOL-180.6",
            "[]",
            |f, _| {
                f.append(PY, "\n\ndef test_pp_099_orphan():\n    assert True\n");
            },
            0,
            "extra",
            &["test_pp_099_orphan"],
        ),
        (
            "TOOL-180.7",
            "[]",
            |f, _| {
                f.append(PY, "\n\ndef pp_098_helper():\n    return 1\n");
            },
            0,
            "",
            &[],
        ),
        (
            "TOOL-180.8",
            "[]",
            |f, _| {
                let body = f.read(PY);
                f.remove(PY).write("python/p/tests/test_y.py", &body);
            },
            0,
            "inconsistent",
            &["PP-001"],
        ),
    ];
    for (row, exts, edit, code, key, want) in rows {
        let f = setup(exts);
        edit(&f, with_incr);
        let r = f.xtask(&["test-trace"]);
        assert_eq!(r.code, code, "{row}: stderr:\n{}", r.stderr);
        if key.is_empty() {
            for k in ["unimplemented", "extra", "inconsistent"] {
                assert!(ids(&r, k).is_empty(), "{row}: {k}");
            }
        } else {
            assert_eq!(ids(&r, key), want, "{row}");
        }
    }
}
