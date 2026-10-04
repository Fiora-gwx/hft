//! `cargo xtask gen-schema [--check]` (tools/README.md "cargo xtask gen-schema"; outputs and failure
//! conditions: crates/hft-event/README.md "实现说明").
//!
//! Flow: build the [`Model`] → choose the baseline lock → internal checks (Note const tables, `n = 0`,
//! hft-ext-proto numbers) and the append-only comparison → on any violation exit 1 without writing →
//! `--check`: compare the rendered outputs with the tree (exit 1 on any difference, nothing written);
//! otherwise write the outputs that changed and delete stale Arrow files.

pub mod lock;
pub mod model;
pub mod render;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::json;

use crate::{EXIT_ENV, EXIT_FAIL, EXIT_OK, EnvError, write_json};
pub use model::{Model, Strategies};

/// Where the baseline lock comes from.
#[derive(Clone, Debug)]
pub enum Baseline {
    /// From git: `pr_base = Some(branch)` (a CI pull-request run) → the lock at the merge base of HEAD
    /// and that branch; `None` → the lock committed at HEAD.
    Git { pr_base: Option<String> },
    /// Given directly (tests); `None` = no baseline (first generation).
    Text(Option<String>),
}

impl Baseline {
    /// From the environment: `GITHUB_EVENT_NAME = pull_request` with `GITHUB_BASE_REF` → merge base.
    pub fn from_env() -> Baseline {
        let pr = std::env::var("GITHUB_EVENT_NAME").is_ok_and(|v| v == "pull_request");
        let base = std::env::var("GITHUB_BASE_REF").ok().filter(|b| !b.is_empty());
        Baseline::Git {
            pr_base: if pr { base } else { None },
        }
    }
}

/// Options of one run.
#[derive(Clone, Debug)]
pub struct Options {
    pub check: bool,
    pub baseline: Baseline,
}

/// Outcome of one run (also written to `target/xtask/gen-schema.json`).
#[derive(Clone, Debug, Default)]
pub struct Report {
    pub code: i32,
    /// Human-readable lines (also printed to stderr).
    pub lines: Vec<String>,
    pub violations: Vec<String>,
    /// `--check`: generated files that differ from the tree.
    pub diffs: Vec<String>,
    /// Files written (write mode).
    pub written: Vec<String>,
    pub baseline: String,
    pub strategies: String,
}

impl Report {
    fn say(&mut self, s: impl Into<String>) {
        let s = s.into();
        eprintln!("gen-schema: {s}");
        self.lines.push(s);
    }

    /// All human-readable output joined (tests search it).
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| format!("git {}: {e}", args.join(" ")))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// Resolves the baseline: `(lock text or None, description)`.
pub fn resolve_baseline(root: &Path, b: &Baseline) -> Result<(Option<String>, String), EnvError> {
    match b {
        Baseline::Text(t) => Ok((
            t.clone(),
            if t.is_some() {
                "given".into()
            } else {
                "none (first generation)".into()
            },
        )),
        Baseline::Git { pr_base } => {
            let rev = match pr_base {
                Some(base) => {
                    let mut mb = None;
                    let mut tried = Vec::new();
                    for cand in [format!("origin/{base}"), base.clone()] {
                        match git(root, &["merge-base", "HEAD", &cand]) {
                            Ok(s) => {
                                mb = Some((s.trim().to_string(), cand));
                                break;
                            }
                            Err(e) => tried.push(e),
                        }
                    }
                    let Some((sha, cand)) = mb else {
                        return Err(EnvError(format!(
                            "pull-request run: merge base of HEAD and {base} not found ({})",
                            tried.join("; ")
                        )));
                    };
                    (
                        sha.clone(),
                        format!("the merge base {} of HEAD and {cand}", &sha[..sha.len().min(12)]),
                    )
                }
                None => {
                    if git(root, &["rev-parse", "--verify", "HEAD"]).is_err() {
                        return Ok((None, "none (not a git work tree with commits)".into()));
                    }
                    ("HEAD".to_string(), "HEAD".to_string())
                }
            };
            match git(root, &["show", &format!("{}:{}", rev.0, lock::LOCK_PATH)]) {
                Ok(text) => Ok((Some(text), format!("{} committed at {}", lock::LOCK_PATH, rev.1))),
                Err(_) => Ok((
                    None,
                    format!("none: {} absent at {} (first generation)", lock::LOCK_PATH, rev.1),
                )),
            }
        }
    }
}

/// Internal consistency checks that need no baseline.
pub fn validate(m: &Model) -> Vec<String> {
    let mut errs = Vec::new();
    // Note const tables vs NOTE_NAMES.
    let prefix_of = |src: &str| {
        model::PREFIX_SOURCES
            .iter()
            .find(|(s, _)| *s == src)
            .map(|(_, p)| *p)
    };
    let mut seen: BTreeMap<(String, u32), &model::NoteConst> = BTreeMap::new();
    let mut seen_name: BTreeMap<(String, String), &model::NoteConst> = BTreeMap::new();
    for c in &m.note_consts {
        let Some(sv) = model::note_source_value(&c.source) else {
            errs.push(format!(
                "{}: `{}` has unknown source `{}`",
                c.origin, c.name, c.source
            ));
            continue;
        };
        if c.source == "Strategy" {
            errs.push(format!(
                "{}: `{}` uses source Strategy (strategy codes live in StrategyDesc.notes)",
                c.origin, c.name
            ));
        }
        if c.n == 0 {
            errs.push(format!(
                "{}: ({}, n = 0) `{}`: n = 0 is reserved in every source",
                c.origin, c.source, c.name
            ));
        }
        if c.n >= 1 << 24 {
            errs.push(format!(
                "{}: ({}, {}) `{}`: n must be < 2^24",
                c.origin, c.source, c.n, c.name
            ));
        }
        if let Some(prev) = seen.insert((c.source.clone(), c.n), c)
            && prev.name != c.name
        {
            errs.push(format!(
                "({}, {}) assigned twice: `{}` ({}) and `{}` ({})",
                c.source, c.n, prev.name, prev.origin, c.name, c.origin
            ));
        }
        if let Some(prev) = seen_name.insert((c.source.clone(), c.name.clone()), c)
            && prev.n != c.n
        {
            errs.push(format!(
                "`{}` ({}) listed twice: n = {} ({}) and n = {} ({})",
                c.name, c.source, prev.n, prev.origin, c.n, c.origin
            ));
        }
        if let Some(p) = prefix_of(&c.source) {
            if !c.name.starts_with(p) {
                errs.push(format!(
                    "{}: `{}` (source {}) must start with `{p}`",
                    c.origin, c.name, c.source
                ));
            }
            continue;
        }
        match m.notes.iter().find(|r| r.name == c.name) {
            None => errs.push(format!(
                "{}: `{}` ({}, {}) is not in the hft-event Note registry NOTE_NAMES",
                c.origin, c.name, c.source, c.n
            )),
            Some(r) if r.source != c.source => {
                let rv = model::note_source_value(&r.source).unwrap_or(0);
                errs.push(format!(
                    "{}: `{}` registered with source {} (code 0x{:08X}) but its const table row has source {} (code 0x{:08X})",
                    c.origin,
                    c.name,
                    r.source,
                    (rv << 24) | c.n,
                    c.source,
                    (sv << 24) | c.n
                ));
            }
            Some(_) => {}
        }
    }
    // Registry names missing from the const tables of a source that has a table.
    let sources_with_table: BTreeSet<&str> = m.note_consts.iter().map(|c| c.source.as_str()).collect();
    for r in &m.notes {
        if prefix_of(&r.source).is_some() || !sources_with_table.contains(r.source.as_str()) {
            continue;
        }
        if !m
            .note_consts
            .iter()
            .any(|c| c.name == r.name && c.source == r.source)
        {
            errs.push(format!(
                "Note registry name `{}` (source {}) has no row in the {} source crate's NOTE_CODES table",
                r.name, r.source, r.source
            ));
        }
    }
    // hft-ext-proto kinds.
    if let Some(ext) = &m.ext_kinds {
        let mut nums = BTreeSet::new();
        for k in ext {
            if !(0x0310..=0x033F).contains(&k.kind) {
                errs.push(format!(
                    "hft-ext-proto kind 0x{:04X} {} outside its segment 0x0310..=0x033F",
                    k.kind, k.name
                ));
            }
            if m.kinds.iter().any(|b| b.kind == k.kind) {
                errs.push(format!(
                    "hft-ext-proto kind 0x{:04X} {} overlaps KINDS",
                    k.kind, k.name
                ));
            }
            if !nums.insert(k.kind) {
                errs.push(format!("hft-ext-proto kind 0x{:04X} listed twice", k.kind));
            }
        }
    }
    for k in &m.kinds {
        if (0x0310..=0x033F).contains(&k.kind) {
            errs.push(format!(
                "KINDS kind 0x{:04X} {} is inside the hft-ext-proto segment 0x0310..=0x033F",
                k.kind, k.name
            ));
        }
    }
    // Strategy descriptors: code 0 reserved, numbers unique per kind.
    if let Strategies::Exported(v) = &m.strategies {
        let mut ids = BTreeSet::new();
        for s in v {
            if !ids.insert(s.kind_id) {
                errs.push(format!("strategy kind_id {} exported twice", s.kind_id));
            }
            for (what, list) in [("Note", &s.notes), ("op", &s.ops), ("tag", &s.tags)] {
                let mut nums = BTreeSet::new();
                for c in list {
                    if what == "Note" && c.value == 0 {
                        errs.push(format!(
                            "strategy kind_id {} `{}`: NoteDesc.code 0 `{}` (code 0 is reserved)",
                            s.kind_id, s.name, c.name
                        ));
                    }
                    if !nums.insert(c.value) {
                        errs.push(format!(
                            "strategy kind_id {}: ({}, {what} {}) listed twice",
                            s.kind_id, s.kind_id, c.value
                        ));
                    }
                }
            }
        }
    }
    errs
}

/// Every generated file (path → content), lock included.
pub fn generate(m: &Model, baseline: Option<&lock::Lock>) -> BTreeMap<String, String> {
    let mut out = render::outputs(m);
    out.insert(lock::LOCK_PATH.to_string(), lock::render(m, baseline));
    out
}

/// Arrow files in the tree that the model no longer generates.
fn stale_arrow(root: &Path, outs: &BTreeMap<String, String>) -> Vec<String> {
    let dir = root.join(render::ARROW_DIR);
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut v: Vec<String> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter(|n| n.ends_with(".json"))
        .map(|n| format!("{}/{n}", render::ARROW_DIR))
        .filter(|p| !outs.contains_key(p))
        .collect();
    v.sort();
    v
}

/// Runs gen-schema on `model` (already loaded) with `opts`; writes `target/xtask/gen-schema.json`.
pub fn run_model(root: &Path, opts: &Options, model: &Model) -> Report {
    let mut r = Report {
        strategies: match &model.strategies {
            Strategies::Skipped(why) => format!("skipped: {why}"),
            Strategies::Exported(v) => format!("{} strategy kinds from hft-core schemas", v.len()),
        },
        ..Report::default()
    };
    let st = r.strategies.clone();
    r.say(format!("strategy schemas: {st}"));
    let (base_text, base_desc) = match resolve_baseline(root, &opts.baseline) {
        Ok(x) => x,
        Err(e) => {
            r.say(format!("error: {e}"));
            r.code = EXIT_ENV;
            finish(root, &mut r);
            return r;
        }
    };
    r.baseline = base_desc.clone();
    r.say(format!("baseline: {base_desc}"));
    let base = match base_text.as_deref().map(lock::parse).transpose() {
        Ok(b) => b,
        Err(e) => {
            r.say(format!("error: baseline lock: {e}"));
            r.code = EXIT_ENV;
            finish(root, &mut r);
            return r;
        }
    };
    let mut violations = validate(model);
    if let Some(b) = &base {
        violations.extend(lock::compare(b, model));
    }
    if !violations.is_empty() {
        for v in &violations {
            r.say(format!("violation: {v}"));
        }
        r.say(format!(
            "{} violation(s); no generated file was written -> FAIL",
            violations.len()
        ));
        r.violations = violations;
        r.code = EXIT_FAIL;
        finish(root, &mut r);
        return r;
    }
    let outs = generate(model, base.as_ref());
    let stale = stale_arrow(root, &outs);
    if opts.check {
        for (p, content) in &outs {
            let cur = std::fs::read(root.join(p)).ok();
            if cur.as_deref() != Some(content.as_bytes()) {
                r.diffs.push(p.clone());
            }
        }
        r.diffs.extend(stale.iter().map(|p| format!("{p} (stale)")));
        if r.diffs.is_empty() {
            r.say(format!(
                "--check: {} generated files match the tree -> PASS",
                outs.len()
            ));
            r.code = EXIT_OK;
        } else {
            for d in r.diffs.clone() {
                r.say(format!("differs: {d}"));
            }
            r.say(format!(
                "--check: {} generated file(s) differ; run `cargo xtask gen-schema` and commit -> FAIL",
                r.diffs.len()
            ));
            r.code = EXIT_FAIL;
        }
    } else {
        for (p, content) in &outs {
            let path = root.join(p);
            if std::fs::read(&path).ok().as_deref() == Some(content.as_bytes()) {
                continue;
            }
            if let Some(dir) = path.parent()
                && let Err(e) = std::fs::create_dir_all(dir)
            {
                r.say(format!("error: create {}: {e}", dir.display()));
                r.code = EXIT_ENV;
                finish(root, &mut r);
                return r;
            }
            if let Err(e) = std::fs::write(&path, content) {
                r.say(format!("error: write {p}: {e}"));
                r.code = EXIT_ENV;
                finish(root, &mut r);
                return r;
            }
            r.written.push(p.clone());
        }
        for p in &stale {
            let _ = std::fs::remove_file(root.join(p));
            r.written.push(format!("{p} (deleted)"));
        }
        let n = r.written.len();
        r.say(format!("wrote {n} file(s) of {} -> PASS", outs.len()));
        r.code = EXIT_OK;
    }
    finish(root, &mut r);
    r
}

fn finish(root: &Path, r: &mut Report) {
    let v = json!({
        "code": r.code,
        "baseline": r.baseline,
        "strategies": r.strategies,
        "violations": r.violations,
        "diffs": r.diffs,
        "written": r.written,
    });
    if let Err(e) = write_json(root, "gen-schema", &v) {
        eprintln!("gen-schema: error: {e}");
        if r.code == EXIT_OK {
            r.code = EXIT_ENV;
        }
    }
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// The built `hft-core` binary: `HFT_CORE_BIN`, else `target/{debug,release,ci}/hft-core`.
pub fn find_hft_core(root: &Path) -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("HFT_CORE_BIN") {
        let p = PathBuf::from(p);
        return p.is_file().then_some(p);
    }
    ["debug", "release", "ci"]
        .iter()
        .map(|prof| root.join("target").join(prof).join(exe("hft-core")))
        .find(|p| p.is_file())
}

/// Byte comparison of an `hft-core schemas` export with `fixtures/strategy-schemas/`.
pub fn compare_schema_dirs(exported: &Path, committed: &Path) -> Vec<String> {
    let list = |d: &Path| -> BTreeSet<String> {
        std::fs::read_dir(d)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter_map(|e| e.file_name().to_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    let (a, b) = (list(exported), list(committed));
    let mut out = Vec::new();
    for f in a.union(&b) {
        match (a.contains(f), b.contains(f)) {
            (true, false) => out.push(format!(
                "fixtures/strategy-schemas/{f}: missing (exported by hft-core)"
            )),
            (false, true) => out.push(format!("fixtures/strategy-schemas/{f}: not exported by hft-core")),
            _ => {
                if std::fs::read(exported.join(f)).ok() != std::fs::read(committed.join(f)).ok() {
                    out.push(format!(
                        "fixtures/strategy-schemas/{f}: differs from the hft-core export"
                    ));
                }
            }
        }
    }
    out
}

/// The real model: linked crates + tree scans + (when `apps/hft-core` is in the tree) the strategy
/// schemas exported by the built `hft-core`. `Err((code, message))` for environment errors, and for
/// a strategy-schema mismatch with `fixtures/strategy-schemas/` (exit 1).
pub fn load(root: &Path) -> Result<Model, (i32, String)> {
    let mut m = model::from_crates(root);
    if root.join("crates/hft-ext-proto/Cargo.toml").is_file() {
        return Err((
            EXIT_ENV,
            "crates/hft-ext-proto is in the tree but gen-schema does not link its KINDS_EXT yet (optional extension)".into(),
        ));
    }
    if !root.join("apps/hft-core/Cargo.toml").is_file() {
        m.strategies = Strategies::Skipped("apps/hft-core is not in the workspace yet".into());
        return Ok(m);
    }
    let Some(bin) = find_hft_core(root) else {
        return Err((
            EXIT_ENV,
            "built hft-core binary not found (HFT_CORE_BIN or target/{debug,release,ci}/hft-core); build it first".into(),
        ));
    };
    let out_dir = root.join("target").join("schemas");
    let _ = std::fs::remove_dir_all(&out_dir);
    let status = Command::new(&bin)
        .arg("schemas")
        .arg("--out")
        .arg(&out_dir)
        .status();
    match status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            return Err((
                EXIT_ENV,
                format!("{} schemas --out {}: exit {s}", bin.display(), out_dir.display()),
            ));
        }
        Err(e) => return Err((EXIT_ENV, format!("run {}: {e}", bin.display()))),
    }
    let rows = model::read_strategy_export(&out_dir).map_err(|e| (EXIT_ENV, e.0))?;
    let diffs = compare_schema_dirs(&out_dir, &root.join("fixtures/strategy-schemas"));
    if !diffs.is_empty() {
        return Err((EXIT_FAIL, diffs.join("\n")));
    }
    m.strategies = Strategies::Exported(rows);
    Ok(m)
}

/// Entry point: `cargo xtask gen-schema [--check]`.
pub fn run(root: &Path, args: &[String]) -> i32 {
    let mut check = false;
    for a in args {
        match a.as_str() {
            "--check" => check = true,
            other => {
                eprintln!("gen-schema: unknown argument '{other}' (usage: cargo xtask gen-schema [--check])");
                return EXIT_ENV;
            }
        }
    }
    let model = match load(root) {
        Ok(m) => m,
        Err((code, msg)) => {
            for l in msg.lines() {
                eprintln!(
                    "gen-schema: {}{l}",
                    if code == EXIT_ENV {
                        "error: "
                    } else {
                        "violation: "
                    }
                );
            }
            let _ = write_json(root, "gen-schema", &json!({ "code": code, "error": msg }));
            return code;
        }
    };
    let opts = Options {
        check,
        baseline: Baseline::from_env(),
    };
    run_model(root, &opts, &model).code
}
