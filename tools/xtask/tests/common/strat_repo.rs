//! Fixture repo for `new-strategy` and `scaffold-gate` (tools/TESTS.md TOOL-020–TOOL-029,
//! TOOL-069–TOOL-071): a registry whose kind_id ledger has real rows 1–4 (4 retired), the
//! `strategy_set!` list, a template (`base/`, `model/`, `pipeline/`), engine paths that the gate
//! watches, `apps/hft-core` and `apps/replayer`, the real deps.toml and scaffold templates; one
//! commit.

use super::Fixture;
use super::repos::{copy_real, git};

pub const LEDGER: &str = "---\ndescription: \"x\"\nkind: crate\ntier: hot\nscope: core\nstatus: spec\n---\n\n# hft-strategy-registry\n\n## 使用本组件\n\n**kind_id 登记**:\n\n| kind_id | crate | scope | Cargo feature |\n|---|---|---|---|\n| 1 | [a](../a/README.md) | core | 默认 |\n| 2 | b | optional | `b` |\n| 3 | c | optional | `c` |\n| 4 | 退役:old | — | — |\n| 0xFF00–0xFFFF | 测试策略 | — | — |\n\n新策略取下一个编号。\n";

pub const REG_LIB: &str = "//! registry\n\nhft_strategy_sdk::strategy_set! {\n    pub struct Strategies {\n        a::A,                                  // kind_id 1\n        #[cfg(feature = \"b\")] b::B,          // 2\n        #[cfg(feature = \"c\")] c::C,          // 3\n    }\n}\n";

pub const REG_TOML: &str = "[package]\nname = \"hft-strategy-registry\"\nversion.workspace = true\nedition.workspace = true\n\n[dependencies]\nhft-strategy-sdk.workspace = true\na = { path = \"../a\" }\n\n[dev-dependencies]\nhft-testkit = { path = \"../../crates/hft-testkit\" }\n";

pub const ROOT_TOML: &str = "[workspace]\nresolver = \"2\"\nmembers = [\n    \"strategies/registry\",\n    \"strategies/a\",\n    \"apps/hft-core\",\n    \"apps/replayer\",\n]\n\n[workspace.package]\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace.dependencies]\nhft-strategy-sdk = { path = \"crates/hft-strategy-sdk\" }\nhft-strat-lib = { path = \"crates/hft-strat-lib\" }\nhft-strategy-registry = { path = \"strategies/registry\" }\n\n[profile.ci]\ninherits = \"release\"\n";

pub const BASE_TOML: &str = "[package]\nname = \"{{name}}\"\nversion.workspace = true\nedition.workspace = true\n\n[dependencies]\nhft-strategy-sdk.workspace = true\nhft-strat-lib.workspace = true\n";

/// Files the engine-path rows of TOOL-027 touch.
pub const ENGINE_FILES: [&str; 4] = [
    "crates/hft-oms/src/lib.rs",
    "apps/hft-core/src/main.rs",
    "tools/hft-cfg/src/strategy.rs",
    "python/pipeline/hft_pipeline/registry.py",
];

pub fn strat_repo(tag: &str) -> Fixture {
    let f = Fixture::empty(tag);
    f.write("Cargo.toml", ROOT_TOML);
    f.write(".gitignore", "target/\n");
    f.write("strategies/registry/README.md", LEDGER);
    f.write("strategies/registry/src/lib.rs", REG_LIB);
    f.write("strategies/registry/Cargo.toml", REG_TOML);
    f.write("strategies/a/Cargo.toml", "[package]\nname = \"a\"\n");
    f.write("strategies/a/src/lib.rs", "");
    // Template.
    let t = "strategies/_template";
    f.write(&format!("{t}/base/Cargo.toml"), BASE_TOML);
    f.write(
        &format!("{t}/base/src/lib.rs"),
        "//! {{name}} (kind_id {{kind_id}})\npub struct {{Type}};\n",
    );
    f.write(
        &format!("{t}/base/tests/scenario.rs"),
        "//! {{name}} scenarios\n#[test]\nfn scenario_smoke() {\n    let _ = {{crate}}::{{Type}};\n}\n",
    );
    f.write(
        &format!("{t}/base/README.md"),
        "---\ndescription: \"{{name}}\"\nkind: crate\ntier: hot\nscope: core\nstatus: spec\n---\n\n# {{name}}\n",
    );
    f.write(
        &format!("{t}/model/src/lib.rs"),
        "//! {{name}} (kind_id {{kind_id}})\nmod model;\npub struct {{Type}};\n",
    );
    f.write(
        &format!("{t}/model/src/model/mod.rs"),
        "//! Model of {{name}}\npub fn mem_bytes(n_groups: u64) -> u64 { n_groups << 20 }\npub fn snapshot_bytes(n_groups: u64) -> u64 { n_groups << 10 }\n",
    );
    f.write(
        &format!("{t}/pipeline/pipeline/pyproject.toml"),
        "[project]\nname = \"hft-plugin-{{name}}\"\n\n[project.entry-points.\"hft_pipeline.plugins\"]\n{{crate}} = \"hft_plugin_{{crate}}:PLUGIN\"\n",
    );
    f.write(
        &format!("{t}/pipeline/pipeline/src/hft_plugin_{{{{crate}}}}/__init__.py"),
        "PLUGIN = None\n",
    );
    f.write(
        &format!("{t}/pipeline/pipeline/README.md"),
        "---\ndescription: \"{{name}} plugin\"\nkind: python-package\ntier: cold\nscope: core\nstatus: spec\n---\n\n# {{name}} plugin\n",
    );
    // Apps (hft-core via the workspace entry, replayer via a direct path).
    f.write(
        "apps/hft-core/Cargo.toml",
        "[package]\nname = \"hft-core\"\nversion.workspace = true\nedition.workspace = true\n\n[dependencies]\nhft-strategy-registry.workspace = true\n",
    );
    f.write(
        "apps/replayer/Cargo.toml",
        "[package]\nname = \"replayer\"\nversion.workspace = true\nedition.workspace = true\n\n[dependencies]\nhft-strategy-registry = { path = \"../../strategies/registry\" }\n",
    );
    f.write("apps/replayer/src/main.rs", "fn main() {}\n");
    for p in ENGINE_FILES {
        f.write(p, "// fixture\n");
    }
    copy_real(&f, "tools/xtask/deps.toml");
    copy_real(&f, "tools/xtask/scaffold/registry_dst.rs.tmpl");
    copy_real(&f, "tools/xtask/scaffold/sim-check.toml");
    git(&f, &["init", "-q"]);
    git(&f, &["add", "-A"]);
    git(&f, &["commit", "-q", "-m", "fixture"]);
    f
}

/// Paths of `git status --porcelain --untracked-files=all [-- paths]`.
pub fn status(f: &Fixture, paths: &[&str]) -> Vec<String> {
    let mut args = vec!["status", "--porcelain", "--untracked-files=all"];
    if !paths.is_empty() {
        args.push("--");
        args.extend_from_slice(paths);
    }
    let s = git(f, &args);
    s.lines()
        .filter_map(|l| l.split_whitespace().last())
        .map(str::to_string)
        .collect()
}
