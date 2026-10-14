//! Strategy family names out of the SDK and engine sources (crates/hft-strategy-sdk/TESTS.md SDK-010).
//! Fixture: `docs-clean` plus a pure `crates/hft-strategy-sdk` (README and two source files), a strategy
//! source file, and the real `strategy-name` rules of tools/xtask/doc-check.toml appended to the
//! fixture's doc-check configuration.

use crate::common;

use common::Fixture;
use common::repos::{docs_clean, repo_root};

const SDK_README: &str = "crates/hft-strategy-sdk/README.md";
const VIEWS: &str = "crates/hft-strategy-sdk/src/views.rs";
const PARAMS: &str = "crates/hft-strategy-sdk/src/params.rs";
const FOO: &str = "strategies/foo/src/lib.rs";

/// The `[[rule]]` entries with `check = "strategy-name"` of the real doc-check.toml, as TOML text.
fn real_name_rules() -> String {
    let text = std::fs::read_to_string(repo_root().join("tools/xtask/doc-check.toml")).unwrap();
    let t: toml::Table = text.parse().unwrap();
    let rules: Vec<toml::Value> = t["rule"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r.get("check").and_then(|c| c.as_str()) == Some("strategy-name"))
        .cloned()
        .collect();
    assert!(
        rules.iter().any(|r| r["id"].as_str() == Some("STRATNAME")),
        "real STRATNAME rule"
    );
    let mut out = toml::Table::new();
    out.insert("rule".into(), toml::Value::Array(rules));
    toml::to_string(&out).unwrap()
}

fn fixture(tag: &str) -> Fixture {
    let f = docs_clean(&format!("sdk010-{tag}"));
    f.append("tools/xtask/doc-check.toml", &format!("\n{}", real_name_rules()));
    f.append(
        "README.md",
        "| [hft-strategy-sdk](crates/hft-strategy-sdk/README.md) | pure | core | 夹具组件 |\n",
    );
    let mut readme = String::from(
        "---\ndescription: \"策略接口\"\nkind: crate\ntier: pure\nscope: core\nstatus: spec\n---\n\n# hft-strategy-sdk\n",
    );
    for h in [
        "概要",
        "目录",
        "使用本组件",
        "实现说明",
        "延迟与资源契约",
        "测试与验收",
        "依赖",
        "延伸阅读",
        "已知限制与后续工作",
    ] {
        readme += &format!("\n## {h}\n\nhft-strategy-sdk 的{h}。\n");
    }
    readme += "\n### 开发备忘\n\n无。\n";
    f.write(SDK_README, &readme);
    f.write(
        VIEWS,
        "//! Read-only views.\n\n/// Quote of one inst.\npub struct BboView {\n    pub bid: i64,\n}\n",
    );
    f.write(
        PARAMS,
        "//! Parameter codec.\n\npub const UNIT: &str = \"rate\";\n",
    );
    f.write(FOO, "//! foo strategy.\n\npub struct Foo;\n");
    f
}

#[test]
fn sdk_010_family_names_out_of_engine() {
    // (row, file, text appended, expected exit, the reported location)
    let rows: [(&str, &str, &str, i32, &str); 5] = [
        ("SDK-010.1", "", "", 0, ""),
        (
            "SDK-010.2",
            SDK_README,
            "\n与 portfolio-carry 的约定相同。\n",
            1,
            "crates/hft-strategy-sdk/README.md:",
        ),
        (
            "SDK-010.3",
            VIEWS,
            "\n// shared with the cdex engine\npub struct Mark;\n",
            1,
            "crates/hft-strategy-sdk/src/views.rs:",
        ),
        (
            "SDK-010.4",
            PARAMS,
            "pub const DEFAULT_FAMILY: &str = \"maker-taker\";\n",
            1,
            "crates/hft-strategy-sdk/src/params.rs:",
        ),
        (
            "SDK-010.5",
            FOO,
            "// tuned like portfolio-carry\npub struct Bar;\n",
            0,
            "",
        ),
    ];
    let mut fails = Vec::new();
    for (row, file, text, want, at) in rows {
        let f = fixture(&row.to_lowercase().replace('.', "-"));
        if !file.is_empty() {
            f.append(file, text);
        }
        let run = f.xtask(&["doc-check"]);
        let out = format!("{}{}", run.stdout, run.stderr);
        if run.code != want {
            fails.push(format!("{row}: exit {} (want {want}):\n{out}", run.code));
            continue;
        }
        if want == 1 {
            // `<file>:<line>: [<rule id>] …` with the line of the appended text.
            let line = f.read(file).lines().count();
            let hit = out.lines().any(|l| {
                l.contains(at)
                    && l.contains("STRAT")
                    && (l.contains(&format!("{at}{line}:")) || l.contains(&format!("{at}{}:", line - 1)))
            });
            if !hit {
                fails.push(format!("{row}: no family-name finding at {at}<line>:\n{out}"));
            }
        }
    }
    assert!(fails.is_empty(), "{}", fails.join("\n\n"));
}
