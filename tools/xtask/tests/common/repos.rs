//! Fixture repos shared by several test files: `clean` (deps), `reverse-dep`, the hot-check `clean`
//! tree, the `docs-clean` copy and small helpers.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::Fixture;

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// Copies the real repository file `rel` into the fixture.
pub fn copy_real(f: &Fixture, rel: &str) {
    let text = std::fs::read_to_string(repo_root().join(rel)).unwrap();
    f.write(rel, &text);
}

fn krate(f: &Fixture, dir: &str, name: &str, deps: &[(&str, &str)]) {
    let mut toml =
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n");
    for (n, p) in deps {
        toml += &format!("{n} = {{ path = \"{p}\" }}\n");
    }
    f.write(&format!("{dir}/Cargo.toml"), &toml);
    f.write(&format!("{dir}/src/lib.rs"), "//! fixture crate\n");
}

/// Fixture `clean` for `deps`: hft-types and hft-refdata (no edges) with the real deps.toml.
pub fn deps_clean(tag: &str) -> Fixture {
    let f = Fixture::empty(tag);
    f.write(
        "Cargo.toml",
        "[workspace]\nresolver = \"2\"\nmembers = [\"crates/hft-types\", \"crates/hft-refdata\"]\n",
    );
    krate(&f, "crates/hft-types", "hft-types", &[]);
    krate(&f, "crates/hft-refdata", "hft-refdata", &[]);
    copy_real(&f, "tools/xtask/deps.toml");
    f
}

/// Fixture `reverse-dep`: `clean` plus `hft-types → hft-refdata`.
pub fn reverse_dep(tag: &str) -> Fixture {
    let f = deps_clean(tag);
    krate(
        &f,
        "crates/hft-types",
        "hft-types",
        &[("hft-refdata", "../hft-refdata")],
    );
    f
}

/// The `docs-clean` fixture copied to a temporary directory (without `schedule.md`).
pub fn docs_clean(tag: &str) -> Fixture {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/docs-clean");
    let f = Fixture::empty(tag);
    for e in walkdir::WalkDir::new(&src)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let rel = e.path().strip_prefix(&src).unwrap();
        if rel == Path::new("schedule.md") {
            continue;
        }
        let to = f.root.join(rel);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(e.path(), to).unwrap();
    }
    f
}

/// Runs git in the fixture (identity fixed, no global config).
pub fn git(f: &Fixture, args: &[&str]) -> String {
    let out = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(&f.root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// ------------------------------------------------------------------------------- hot-check `clean`

pub const OMS_LIB: &str = "crates/hft-oms/src/lib.rs";
pub const OMS_A: &str = "crates/hft-oms/src/a.rs";
pub const RT_A: &str = "crates/hft-rt/src/a.rs";
pub const X_LIB: &str = "strategies/x/src/lib.rs";

const HOT_BLOCK: &str = "#![forbid(unsafe_code)]\n#![deny(\n    clippy::float_arithmetic,\n    clippy::unwrap_used,\n    clippy::expect_used,\n    clippy::panic,\n    clippy::unreachable,\n    clippy::todo,\n    clippy::unimplemented\n)]\n";
const RT_BLOCK: &str = "#![deny(\n    clippy::unwrap_used,\n    clippy::expect_used,\n    clippy::panic,\n    clippy::unreachable,\n    clippy::todo,\n    clippy::unimplemented,\n    clippy::undocumented_unsafe_blocks\n)]\n";

const DEV_MD: &str = "# 开发\n\n## 目标 CPU\n\n| 剖面 | 发布目标三元组 | `target-cpu` | 运行在 | 说明 |\n|---|---|---|---|---|\n| 基准形态(core) | `aarch64-unknown-linux-gnu` | `neoverse-v1` | dev2 | x |\n\n## 构建剖面\n\n| 剖面 | 用途 | 设置 |\n|---|---|---|\n| `dev` | 本地 | 默认 |\n| `release` | 发布 | `lto = \"thin\"`、`codegen-units = 16`、`panic = \"abort\"` |\n| `release-hot` | hot | `inherits = \"release\"`;`lto = \"fat\"`、`codegen-units = 1` |\n| `ci` | CI | `inherits = \"release\"`;`lto = false`、**`panic = \"unwind\"`** |\n| `bench` | 基准 | 与 `release-hot` 相同(`inherits = \"release-hot\"`) |\n";

const ROOT_TOML: &str = "[workspace]\nresolver = \"2\"\nmembers = []\n\n[profile.release]\nlto = \"thin\"\ncodegen-units = 16\npanic = \"abort\"\n\n[profile.release-hot]\ninherits = \"release\"\nlto = \"fat\"\ncodegen-units = 1\n\n[profile.ci]\ninherits = \"release\"\nlto = false\npanic = \"unwind\"\n\n[profile.bench]\nlto = \"fat\"\ncodegen-units = 1\n";

/// Hot-check fixture `clean`: hft-oms (hot, N3), hft-rt (hot, unsafe allowed), hft-strategy-sdk (pure,
/// with `model_math`), strategy `x` (fast part + `src/model/`), apps/watchtower (cold); real deps.toml
/// and lint templates; an empty allow-list; profiles and target-cpu matching the fixture
/// docs/development.md.
pub fn hot_clean(tag: &str) -> Fixture {
    let f = Fixture::empty(tag);
    f.write("Cargo.toml", ROOT_TOML);
    f.write("docs/development.md", DEV_MD);
    f.write(
        ".cargo/config.toml",
        "[target.aarch64-unknown-linux-gnu]\nrustflags = [\"-C\", \"target-cpu=neoverse-v1\"]\n",
    );
    copy_real(&f, "tools/xtask/deps.toml");
    for t in [
        "tools/lint/clippy-hot.toml",
        "tools/lint/clippy-pure.toml",
        "tools/lint/attrs-hot.rs",
        "tools/lint/attrs-pure.rs",
        "tools/lint/attrs-unsafe.rs",
    ] {
        copy_real(&f, t);
    }
    f.write("tools/xtask/allow-list.toml", "# fixture allow-list\n");
    let hot = std::fs::read_to_string(repo_root().join("tools/lint/clippy-hot.toml")).unwrap();
    let pure = std::fs::read_to_string(repo_root().join("tools/lint/clippy-pure.toml")).unwrap();
    let pkg = |dir: &str, name: &str| {
        f.write(
            &format!("{dir}/Cargo.toml"),
            &format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
        );
    };
    pkg("crates/hft-oms", "hft-oms");
    f.write("crates/hft-oms/clippy.toml", &hot);
    f.write(OMS_LIB, &format!("//! oms\n\n{HOT_BLOCK}\nmod a;\n"));
    f.write(
        OMS_A,
        "pub struct Slot {\n    pub px: i64,\n}\n\npub fn fill(s: &mut Slot, q: i64) {\n    s.px += q;\n}\n",
    );
    pkg("crates/hft-rt", "hft-rt");
    f.write("crates/hft-rt/clippy.toml", &hot);
    f.write(
        "crates/hft-rt/src/lib.rs",
        &format!("//! rt\n\n{RT_BLOCK}\nmod a;\n"),
    );
    f.write(RT_A, "pub fn noop() {}\n");
    pkg("crates/hft-strategy-sdk", "hft-strategy-sdk");
    f.write("crates/hft-strategy-sdk/clippy.toml", &pure);
    f.write(
        "crates/hft-strategy-sdk/src/lib.rs",
        &format!("//! sdk\n\n{HOT_BLOCK}\npub mod model_math;\n"),
    );
    f.write(
        "crates/hft-strategy-sdk/src/model_math.rs",
        "#![allow(clippy::float_arithmetic)]\npub fn ln(x: f64) -> f64 {\n    x\n}\n",
    );
    pkg("strategies/x", "x");
    f.write("strategies/x/clippy.toml", &hot);
    f.write(
        X_LIB,
        &format!("//! x\n\n{HOT_BLOCK}\nmod model;\n\npub struct X {{\n    pub n: i64,\n}}\n"),
    );
    f.write(
        "strategies/x/src/model/mod.rs",
        "pub struct M {\n    pub w: i64,\n}\n",
    );
    pkg("apps/watchtower", "watchtower");
    f.write("apps/watchtower/src/main.rs", "fn main() {}\n");
    f
}
