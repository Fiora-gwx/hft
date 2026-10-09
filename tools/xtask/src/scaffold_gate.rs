//! `cargo xtask scaffold-gate [--keep]` (tools/README.md "cargo xtask scaffold-gate"): the template
//! probe check of gate ⑧.
//!
//! 1. Renders the `new-strategy` template with `--with-model --with-pipeline` as `ci-probe` (kind_id
//!    in the test range) into `target/decoupling/strategies/ci-probe/`; checks its shape (Model
//!    skeleton `src/model/`, pipeline plugin `pipeline/pyproject.toml`) and its direct
//!    dependencies (only those `deps.toml` allows `strategies/*`).
//! 2. Writes the temporary workspace `target/decoupling/Cargo.toml`: the registry copy
//!    (`registry/`, `strategy_set!` + `CiProbe`, a dependency on the probe, the DST short set
//!    `tests/ci_probe_dst.rs`), the probe, and manifest copies of `apps/hft-core` and `apps/replayer`
//!    whose sources stay in the repository and whose registry dependency points at the copy
//!    (Cargo's `[patch]` cannot replace a path dependency, so the copies carry the redirection).
//!    Every build runs with `CARGO_TARGET_DIR` = the main target directory.
//! 3. Runs the probe's scenario test, the DST short set, builds `hft-core` and runs
//!    `hft-core --check --config <generated sim config>`.
//! 4. Fails on any change under `crates/`, `apps/`, `tools/hft-cfg/`, `python/`
//!    (`git status --porcelain`, untracked files included), then removes `target/decoupling/`
//!    unless `--keep`.
//!
//! Skipped (exit 0, reason in the summary and JSON) while the template, the registry or
//! `apps/hft-core` is not in the repository yet.

use std::path::{Component, Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

use crate::exec::{Cmd, Exec};
use crate::new_strategy::{self, Opts};
use crate::{EXIT_ENV, EXIT_FAIL, EXIT_OK, EnvError, write_json};

pub const PROBE: &str = "ci-probe";
/// kind_id of the probe, in the registry's test range 0xFF00–0xFFFF.
pub const PROBE_KIND_ID: u16 = 0xFFF0;
/// Seeds of the DST short set.
pub const DST_SEEDS: u32 = 100;
/// Paths that must stay unchanged (tools/README.md step 4).
pub const ENGINE_PATHS: [&str; 4] = ["crates", "apps", "tools/hft-cfg", "python"];
/// xtask-owned templates of the registry DST short set and the sim instance check config.
pub const DST_TEMPLATE: &str = "tools/xtask/scaffold/registry_dst.rs.tmpl";
pub const CONFIG_TEMPLATE: &str = "tools/xtask/scaffold/sim-check.toml";
const APPS: [&str; 2] = ["apps/hft-core", "apps/replayer"];

fn probe() -> Opts {
    Opts {
        name: PROBE.into(),
        kind_id: PROBE_KIND_ID,
        with_model: true,
        with_pipeline: true,
    }
}

/// Lexically normalized absolute path.
fn norm(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            c => out.push(c),
        }
    }
    out
}

/// `to` relative to the directory `from` (both absolute), `/`-separated.
pub fn relative(from: &Path, to: &Path) -> String {
    let (f, t) = (norm(from), norm(to));
    let fc: Vec<_> = f.components().collect();
    let tc: Vec<_> = t.components().collect();
    let common = fc.iter().zip(&tc).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> = vec!["..".into(); fc.len() - common];
    parts.extend(
        tc[common..]
            .iter()
            .map(|c| c.as_os_str().to_string_lossy().into_owned()),
    );
    if parts.is_empty() {
        ".".into()
    } else {
        parts.join("/")
    }
}

/// Rewrites every dependency `path` of a dependency table for a manifest moved from `from` to
/// `to`; a path resolving to `redirect.0` is sent to `redirect.1`.
fn relocate_deps(t: &mut toml::Table, from: &Path, to: &Path, redirect: &(PathBuf, PathBuf)) {
    for (_, dep) in t.iter_mut() {
        if let Some(d) = dep.as_table_mut()
            && let Some(p) = d.get("path").and_then(|v| v.as_str())
        {
            let abs = norm(&from.join(p));
            let target = if abs == norm(&redirect.0) {
                redirect.1.clone()
            } else {
                abs
            };
            d.insert("path".into(), toml::Value::String(relative(to, &target)));
        }
    }
}

fn relocate_all_deps(t: &mut toml::Table, from: &Path, to: &Path, redirect: &(PathBuf, PathBuf)) {
    for k in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(d) = t.get_mut(k).and_then(|v| v.as_table_mut()) {
            relocate_deps(d, from, to, redirect);
        }
    }
    if let Some(targets) = t.get_mut("target").and_then(|v| v.as_table_mut()) {
        for (_, tt) in targets.iter_mut() {
            if let Some(tt) = tt.as_table_mut() {
                relocate_all_deps(tt, from, to, redirect);
            }
        }
    }
}

/// A package manifest moved from `from` to `to`, its library and binaries still built from the
/// sources in `from` (tests, benches and examples dropped).
pub fn relocate_package(
    text: &str,
    from: &Path,
    to: &Path,
    redirect: &(PathBuf, PathBuf),
) -> Result<String, EnvError> {
    let mut t: toml::Table = text
        .parse()
        .map_err(|e| EnvError(format!("{}/Cargo.toml: {e}", from.display())))?;
    relocate_all_deps(&mut t, from, to, redirect);
    let name = t
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let src = |rel: &str| toml::Value::String(relative(to, &from.join(rel)));
    if let Some(pkg) = t.get_mut("package").and_then(|v| v.as_table_mut()) {
        for k in ["autobins", "autoexamples", "autotests", "autobenches"] {
            pkg.insert(k.into(), toml::Value::Boolean(false));
        }
        match pkg.get("build").and_then(|v| v.as_str()).map(str::to_string) {
            Some(b) => {
                pkg.insert("build".into(), src(&b));
            }
            None if !pkg.contains_key("build") && from.join("build.rs").is_file() => {
                pkg.insert("build".into(), src("build.rs"));
            }
            None => {}
        }
    }
    let lib_path = t
        .get("lib")
        .and_then(|l| l.get("path"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .or_else(|| {
            from.join("src/lib.rs")
                .is_file()
                .then(|| "src/lib.rs".to_string())
        });
    if let Some(lp) = lib_path {
        let lib = t
            .entry("lib")
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        if let Some(l) = lib.as_table_mut() {
            l.insert("path".into(), src(&lp));
        }
    }
    let mut bins: Vec<toml::Value> = Vec::new();
    if let Some(arr) = t.get("bin").and_then(|v| v.as_array()) {
        for b in arr {
            let mut b = b.as_table().cloned().unwrap_or_default();
            let bname = b
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(&name)
                .to_string();
            let p = b
                .get("path")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| {
                    if bname == name {
                        "src/main.rs".into()
                    } else {
                        format!("src/bin/{bname}.rs")
                    }
                });
            b.insert("path".into(), src(&p));
            bins.push(toml::Value::Table(b));
        }
    } else {
        if from.join("src/main.rs").is_file() {
            let mut b = toml::Table::new();
            b.insert("name".into(), toml::Value::String(name.clone()));
            b.insert("path".into(), src("src/main.rs"));
            bins.push(toml::Value::Table(b));
        }
        if let Ok(rd) = std::fs::read_dir(from.join("src/bin")) {
            let mut extra: Vec<String> = rd
                .flatten()
                .filter_map(|e| e.file_name().to_str().map(str::to_string))
                .filter(|n| n.ends_with(".rs"))
                .collect();
            extra.sort();
            for f in extra {
                let mut b = toml::Table::new();
                b.insert(
                    "name".into(),
                    toml::Value::String(f.trim_end_matches(".rs").into()),
                );
                b.insert("path".into(), src(&format!("src/bin/{f}")));
                bins.push(toml::Value::Table(b));
            }
        }
    }
    t.remove("bin");
    if !bins.is_empty() {
        t.insert("bin".into(), toml::Value::Array(bins));
    }
    for k in ["test", "bench", "example"] {
        t.remove(k);
    }
    toml::to_string(&t).map_err(|e| EnvError(format!("encode manifest: {e}")))
}

/// The temporary workspace manifest: the root manifest's `[workspace]` settings, dependency
/// paths re-rooted at `dec`, `hft-strategy-registry` → `registry`, plus the probe.
pub fn workspace_manifest(
    root_toml: &str,
    root: &Path,
    dec: &Path,
    members: &[String],
) -> Result<String, EnvError> {
    let mut t: toml::Table = root_toml
        .parse()
        .map_err(|e| EnvError(format!("Cargo.toml: {e}")))?;
    let redirect = (root.join(new_strategy::REGISTRY), dec.join("registry"));
    let mut out = toml::Table::new();
    let mut ws = toml::Table::new();
    if let Some(w) = t.get_mut("workspace").and_then(|v| v.as_table_mut()) {
        if let Some(r) = w.get("resolver") {
            ws.insert("resolver".into(), r.clone());
        }
        for k in ["package", "lints"] {
            if let Some(v) = w.get(k) {
                ws.insert(k.into(), v.clone());
            }
        }
        if let Some(d) = w.get_mut("dependencies").and_then(|v| v.as_table_mut()) {
            relocate_deps(d, root, dec, &redirect);
            let mut p = toml::Table::new();
            p.insert("path".into(), toml::Value::String(format!("strategies/{PROBE}")));
            d.insert(PROBE.into(), toml::Value::Table(p));
            ws.insert("dependencies".into(), toml::Value::Table(d.clone()));
        }
    }
    ws.insert(
        "members".into(),
        toml::Value::Array(members.iter().map(|m| toml::Value::String(m.clone())).collect()),
    );
    out.insert("workspace".into(), toml::Value::Table(ws));
    if let Some(p) = t.get("profile") {
        out.insert("profile".into(), p.clone());
    }
    let body = toml::to_string(&out).map_err(|e| EnvError(format!("encode manifest: {e}")))?;
    Ok(format!(
        "# scaffold-gate temporary workspace (never committed; tools/README.md \"cargo xtask scaffold-gate\")\n{body}"
    ))
}

fn copy_tree(from: &Path, to: &Path) -> Result<(), EnvError> {
    for e in walkdir::WalkDir::new(from)
        .into_iter()
        .filter_entry(|e| e.file_name() != "target")
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let rel = e.path().strip_prefix(from).unwrap_or(e.path());
        let dst = to.join(rel);
        if let Some(d) = dst.parent() {
            std::fs::create_dir_all(d).map_err(|er| EnvError(format!("create {}: {er}", d.display())))?;
        }
        std::fs::copy(e.path(), &dst).map_err(|er| EnvError(format!("copy {}: {er}", e.path().display())))?;
    }
    Ok(())
}

fn rd(p: &Path) -> Result<String, EnvError> {
    std::fs::read_to_string(p).map_err(|e| EnvError(format!("read {}: {e}", p.display())))
}

fn wr(p: &Path, text: &str) -> Result<(), EnvError> {
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| EnvError(format!("create {}: {e}", d.display())))?;
    }
    std::fs::write(p, text).map_err(|e| EnvError(format!("write {}: {e}", p.display())))
}

/// Shape violations of a rendered probe.
pub fn shape(dir: &Path) -> Vec<String> {
    let mut v = Vec::new();
    let model = dir.join("src/model");
    let has_model = (model.is_dir() || dir.join("src/model.rs").is_file())
        && walkdir::WalkDir::new(&model)
            .into_iter()
            .filter_map(|e| e.ok())
            .chain(
                walkdir::WalkDir::new(dir.join("src/model.rs"))
                    .into_iter()
                    .filter_map(|e| e.ok()),
            )
            .filter(|e| e.file_type().is_file())
            .any(|e| std::fs::read_to_string(e.path()).is_ok_and(|t| t.contains("mem_bytes")));
    if !has_model {
        v.push(format!(
            "probe shape: {PROBE} has no Model skeleton (src/model/ declaring mem_bytes)"
        ));
    }
    if !dir.join("pipeline/pyproject.toml").is_file() {
        v.push(format!(
            "probe shape: {PROBE} has no pipeline plugin package (pipeline/pyproject.toml)"
        ));
    }
    v
}

/// Direct dependencies of the probe that `deps.toml` does not allow `strategies/*`.
pub fn dep_violations(root: &Path, dir: &Path) -> Result<Vec<String>, EnvError> {
    let cfg = crate::deps::Config::load(root)?;
    let allowed: Vec<String> = cfg
        .rule_for(&format!("strategies/{PROBE}"))
        .map(|r| {
            r.deps
                .iter()
                .map(|d| d.rsplit('/').next().unwrap_or(d).to_string())
                .collect()
        })
        .unwrap_or_default();
    let t: toml::Table = rd(&dir.join("Cargo.toml"))?
        .parse()
        .map_err(|e| EnvError(format!("{PROBE}/Cargo.toml: {e}")))?;
    let mut v = Vec::new();
    for (k, val) in t
        .get("dependencies")
        .and_then(|d| d.as_table())
        .into_iter()
        .flatten()
    {
        let internal = val
            .as_table()
            .is_some_and(|x| x.contains_key("path") || x.contains_key("workspace"))
            || k.starts_with("hft-")
            || k.starts_with("venue-");
        if internal && !allowed.contains(k) {
            v.push(format!(
                "deps: {PROBE} → {k} (strategies/* may depend only on {})",
                allowed.join(", ")
            ));
        }
    }
    Ok(v)
}

/// Changed or untracked files under the engine paths (`git status --porcelain`).
pub fn engine_changes(root: &Path) -> Result<Vec<String>, EnvError> {
    let out = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=all", "--"])
        .args(ENGINE_PATHS)
        .current_dir(root)
        .output()
        .map_err(|e| EnvError(format!("run git: {e}")))?;
    if !out.status.success() {
        return Err(EnvError(format!(
            "git status: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.len() > 3)
        .map(|l| {
            let p = &l[3..];
            p.rsplit(" -> ").next().unwrap_or(p).trim_matches('"').to_string()
        })
        .collect())
}

fn target_dir(root: &Path) -> PathBuf {
    match std::env::var_os("CARGO_TARGET_DIR") {
        Some(t) => {
            let t = PathBuf::from(t);
            if t.is_absolute() { t } else { root.join(t) }
        }
        None => root.join("target"),
    }
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// The Sim account of the generated config: the `[[sim_accounts]]` entry (`label`, `kind`,
/// `bound_instance`).
fn sim_account(cfg: &str) -> Value {
    let t: toml::Table = cfg.parse().unwrap_or_default();
    t.get("sim_accounts")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|a| a.as_table())
        .map(|a| {
            let s = |k: &str| a.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_string();
            json!({"label": s("label"), "kind": s("kind"), "bound_instance": s("bound_instance")})
        })
        .unwrap_or(Value::Null)
}

#[derive(Debug, Default)]
struct Report {
    violations: Vec<String>,
    steps: Vec<Value>,
    engine_changes: Vec<String>,
    sim_account: Value,
}

/// Steps 1–3 (everything except the diff check and the cleanup).
fn build_and_check(root: &Path, dec: &Path, exec: &mut dyn Exec, r: &mut Report) -> Result<(), EnvError> {
    let o = probe();
    let pdir = dec.join("strategies").join(PROBE);
    new_strategy::render(&root.join(new_strategy::TEMPLATE), &pdir, &o)?;
    r.violations.extend(shape(&pdir));
    r.violations.extend(dep_violations(root, &pdir)?);
    if !r.violations.is_empty() {
        return Ok(());
    }
    // Registry copy.
    let reg = root.join(new_strategy::REGISTRY);
    let rcopy = dec.join("registry");
    copy_tree(&reg, &rcopy)?;
    let lib = rd(&rcopy.join("src/lib.rs"))?;
    let lib = new_strategy::add_set_line(&lib, &format!("{}::{}", o.krate(), o.ty()))
        .ok_or_else(|| EnvError("registry src/lib.rs: no strategy_set! struct body".into()))?;
    wr(&rcopy.join("src/lib.rs"), &lib)?;
    let none = (PathBuf::new(), PathBuf::new());
    // The copy keeps its own sources: only dependency paths move.
    let mut t: toml::Table = rd(&reg.join("Cargo.toml"))?
        .parse()
        .map_err(|e| EnvError(format!("registry Cargo.toml: {e}")))?;
    relocate_all_deps(&mut t, &reg, &rcopy, &none);
    let rt = toml::to_string(&t).map_err(|e| EnvError(format!("encode manifest: {e}")))?;
    let rt = new_strategy::add_dependency(&rt, &format!("{PROBE} = {{ path = \"../strategies/{PROBE}\" }}"));
    wr(&rcopy.join("Cargo.toml"), &rt)?;
    let dst = rd(&root.join(DST_TEMPLATE))?
        .replace("{{name}}", PROBE)
        .replace("{{crate}}", &o.krate())
        .replace("{{Type}}", &o.ty())
        .replace("{{seeds}}", &DST_SEEDS.to_string());
    let dst_test = format!("{}_dst", o.krate());
    wr(&rcopy.join("tests").join(format!("{dst_test}.rs")), &dst)?;
    // App manifest copies and the temporary workspace.
    let redirect = (reg.clone(), rcopy.clone());
    let mut members = vec!["registry".to_string(), format!("strategies/{PROBE}")];
    for app in APPS {
        let from = root.join(app);
        if !from.join("Cargo.toml").is_file() {
            continue;
        }
        let to = dec.join(app);
        wr(
            &to.join("Cargo.toml"),
            &relocate_package(&rd(&from.join("Cargo.toml"))?, &from, &to, &redirect)?,
        )?;
        members.push(app.to_string());
    }
    let manifest = dec.join("Cargo.toml");
    wr(
        &manifest,
        &workspace_manifest(&rd(&root.join("Cargo.toml"))?, root, dec, &members)?,
    )?;
    let cfg_text = rd(&root.join(CONFIG_TEMPLATE))?
        .replace("{{name}}", PROBE)
        .replace("{{kind_id}}", &PROBE_KIND_ID.to_string());
    let cfg = dec.join("sim-check.toml");
    wr(&cfg, &cfg_text)?;
    r.sim_account = sim_account(&cfg_text);
    // Step 3: scenario, DST short set, hft-core build and --check, all on the main target dir.
    let tdir = target_dir(root);
    let tenv = vec![("CARGO_TARGET_DIR".to_string(), tdir.display().to_string())];
    let mp = manifest.display().to_string();
    let bin = tdir.join("debug").join(exe("hft-core"));
    let steps: Vec<(&str, Cmd)> = vec![
        (
            "scenario",
            Cmd::new(
                "cargo",
                &["test", "--manifest-path", &mp, "-p", PROBE, "--test", "scenario"],
            ),
        ),
        (
            "dst short set",
            Cmd::new(
                "cargo",
                &[
                    "test",
                    "--manifest-path",
                    &mp,
                    "-p",
                    "hft-strategy-registry",
                    "--test",
                    &dst_test,
                ],
            ),
        ),
        (
            "build hft-core",
            Cmd::new("cargo", &["build", "--manifest-path", &mp, "-p", "hft-core"]),
        ),
        (
            "hft-core --check",
            Cmd::new(&bin.display().to_string(), &["--check", "--config"]).arg(cfg.display().to_string()),
        ),
    ];
    for (name, mut c) in steps {
        c.env = tenv.clone();
        let o = exec.run(&c)?;
        r.steps
            .push(json!({"step": name, "command": c.line(), "code": o.code}));
        if !o.ok() {
            r.violations
                .push(format!("{name}: `{}` exited {}", c.line(), o.code));
            break;
        }
    }
    Ok(())
}

/// Runs `scaffold-gate` with `exec`; returns the exit code.
pub fn run_with(root: &Path, args: &[String], exec: &mut dyn Exec) -> i32 {
    let mut keep = false;
    for a in args {
        match a.as_str() {
            "--keep" => keep = true,
            x => {
                eprintln!(
                    "scaffold-gate: error: unknown argument {x} (usage: cargo xtask scaffold-gate [--keep])"
                );
                return EXIT_ENV;
            }
        }
    }
    let missing: Vec<&str> = [
        (
            new_strategy::TEMPLATE,
            root.join(new_strategy::TEMPLATE).join("base").is_dir(),
        ),
        (
            new_strategy::REGISTRY,
            root.join(new_strategy::REGISTRY).join("Cargo.toml").is_file(),
        ),
        ("apps/hft-core", root.join("apps/hft-core/Cargo.toml").is_file()),
    ]
    .iter()
    .filter(|(_, ok)| !ok)
    .map(|(p, _)| *p)
    .collect();
    if !missing.is_empty() {
        let note = format!("{} not in the repository yet; skipped", missing.join(", "));
        eprintln!("scaffold-gate: {note}");
        return match write_json(root, "scaffold-gate", &json!({"result": "skipped", "note": note})) {
            Ok(_) => EXIT_OK,
            Err(e) => {
                eprintln!("scaffold-gate: error: {e}");
                EXIT_ENV
            }
        };
    }
    let dec = root.join("target").join("decoupling");
    let _ = std::fs::remove_dir_all(&dec);
    let mut r = Report::default();
    let mut code = match build_and_check(root, &dec, exec, &mut r) {
        Ok(()) => EXIT_OK,
        Err(e) => {
            eprintln!("scaffold-gate: error: {e}");
            EXIT_ENV
        }
    };
    if code == EXIT_OK {
        match engine_changes(root) {
            Ok(c) => {
                for p in &c {
                    r.violations.push(format!("engine-side change: {p}"));
                }
                r.engine_changes = c;
            }
            Err(e) => {
                eprintln!("scaffold-gate: error: {e}");
                code = EXIT_ENV;
            }
        }
    }
    if code == EXIT_OK && !r.violations.is_empty() {
        code = EXIT_FAIL;
    }
    for v in &r.violations {
        eprintln!("scaffold-gate: {v}");
    }
    if !keep {
        let _ = std::fs::remove_dir_all(&dec);
    }
    let j = json!({
        "probe": PROBE,
        "kind_id": format!("{PROBE_KIND_ID:#06x}"),
        "new_strategy_args": probe().args(),
        "dst_seeds": DST_SEEDS,
        "sim_account": r.sim_account,
        "steps": r.steps,
        "engine_changes": r.engine_changes,
        "violations": r.violations,
        "kept": keep,
        "result": match code { EXIT_OK => "pass", EXIT_FAIL => "fail", _ => "error" },
    });
    if let Err(e) = write_json(root, "scaffold-gate", &j) {
        eprintln!("scaffold-gate: error: {e}");
        return EXIT_ENV;
    }
    eprintln!(
        "scaffold-gate: -> {}",
        if code == EXIT_OK { "PASS" } else { "FAIL" }
    );
    code
}

/// Entry point.
pub fn run(root: &Path, args: &[String]) -> i32 {
    match crate::exec::from_env(root) {
        Ok(mut e) => run_with(root, args, e.as_mut()),
        Err(e) => {
            eprintln!("scaffold-gate: error: {e}");
            EXIT_ENV
        }
    }
}
