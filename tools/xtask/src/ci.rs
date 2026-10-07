//! `cargo xtask ci [--pure-only] [--fast]` (tools/README.md "cargo xtask ci"): the twelve steps in
//! order, stopping at the first failure. Every step runs through [`Exec`]. A step whose component is not
//! in the workspace yet is skipped and the reason is written to the summary and the JSON (README "ci"
//! skip rules): golden steps without any `tests/golden.rs`, the DST steps without the hft-testkit DST
//! framework (`crates/hft-testkit/src/dst/`), the Python tests without any package `tests/` directory.

use std::path::Path;

use serde_json::{Value, json};

use crate::exec::{Cmd, Exec};
use crate::workspace::{Member, members, readme_fm};
use crate::{EXIT_ENV, EXIT_FAIL, EXIT_OK, EnvError, write_json};

/// PR runs check the first K seeds of the consistency set (docs/testing.md §6).
pub const HASHTRAIL_FIRST: u32 = 50;
/// PR DST smoke: commit-derived seeds plus the regression set (docs/testing.md §6).
pub const DST_SMOKE_SEEDS: u32 = 200;
/// Optional components (tools/README.md "范围").
pub const SCOPE: &str = "tools/xtask/scope.toml";
/// Where the DST framework lives once it exists (crates/hft-testkit README §6).
pub const DST_DIR: &str = "crates/hft-testkit/src/dst";

#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    pub pure_only: bool,
    pub fast: bool,
}

/// One step: name, the commands to run (empty = skipped with `note`).
#[derive(Debug, Clone)]
pub struct Step {
    pub name: &'static str,
    pub cmds: Vec<Cmd>,
    pub note: Option<String>,
}

fn step(name: &'static str, cmds: Vec<Cmd>) -> Step {
    Step {
        name,
        cmds,
        note: None,
    }
}

fn skip(name: &'static str, note: String) -> Step {
    Step {
        name,
        cmds: Vec::new(),
        note: Some(note),
    }
}

fn python_program() -> &'static str {
    if cfg!(windows) { "python" } else { "python3" }
}

/// Python packages: `python/<pkg>/` and `strategies/<name>/pipeline/` with a README frontmatter;
/// `(dir, tier, has tests/)`.
fn python_packages(root: &Path) -> Vec<(String, String, bool)> {
    let mut out = Vec::new();
    let mut dirs = Vec::new();
    for base in ["python", "strategies"] {
        if let Ok(rd) = std::fs::read_dir(root.join(base)) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().into_owned();
                if base == "python" {
                    dirs.push(format!("python/{n}"));
                } else {
                    dirs.push(format!("strategies/{n}/pipeline"));
                }
            }
        }
    }
    dirs.sort();
    for d in dirs {
        if let Some(t) = readme_fm(root, &d, "tier") {
            out.push((d.clone(), t, root.join(&d).join("tests").is_dir()));
        }
    }
    out
}

/// Optional components of `scope.toml`: `(component, member?)`.
fn optional(root: &Path, ms: &[Member]) -> Result<Vec<(String, bool)>, EnvError> {
    let p = root.join(SCOPE);
    let text = std::fs::read_to_string(&p).map_err(|e| EnvError(format!("read {SCOPE}: {e}")))?;
    let t: toml::Table = text
        .parse()
        .map_err(|e| EnvError(format!("parse {SCOPE}: {e}")))?;
    let list = t
        .get("optional")
        .and_then(|v| v.as_array())
        .ok_or_else(|| EnvError(format!("{SCOPE}: missing `optional` list")))?;
    Ok(list
        .iter()
        .filter_map(|v| v.as_str())
        .map(|c| (c.to_string(), ms.iter().any(|m| m.dir == c)))
        .collect())
}

/// `scope.toml` optional components with whether each is a workspace member.
pub type Optional = Vec<(String, bool)>;

/// The step plan for `opts` on the tree at `root`.
pub fn plan(root: &Path, opts: Options) -> Result<(Vec<Step>, Optional), EnvError> {
    let ms = members(root)?;
    let opt = optional(root, &ms)?;
    // Package selection.
    let selected: Vec<&Member> = if opts.pure_only {
        ms.iter()
            .filter(|m| readme_fm(root, &m.dir, "tier").as_deref() == Some("pure"))
            .collect()
    } else {
        ms.iter().collect()
    };
    let sel_args = |base: &[&str]| -> Cmd {
        let mut c = Cmd::new("cargo", base);
        if opts.pure_only {
            for m in &selected {
                c = c.arg("-p").arg(m.name.clone());
            }
        } else {
            c = c.arg("--workspace");
        }
        c
    };
    let mut steps = Vec::new();
    steps.push(step(
        "fmt --check",
        vec![Cmd::new("cargo", &["fmt", "--all", "--check"])],
    ));
    let clippy_note = "clippy --workspace --all-targets -D warnings";
    if opts.pure_only && selected.is_empty() {
        steps.push(skip(clippy_note, "no tier = pure crate in the workspace".into()));
    } else {
        let c = sel_args(&["clippy"])
            .arg("--all-targets")
            .arg("--all-features")
            .arg("--")
            .arg("-D")
            .arg("warnings");
        steps.push(step(clippy_note, vec![c]));
    }
    steps.push(step(
        "deny check licenses bans",
        vec![Cmd::new("cargo", &["deny", "check", "licenses", "bans"])],
    ));
    steps.push(step("deps", vec![Cmd::new("xtask", &["deps"])]));
    steps.push(step("hot-check", vec![Cmd::new("xtask", &["hot-check"])]));
    steps.push(step(
        "fixtures check",
        vec![Cmd::new("xtask", &["fixtures", "check"])],
    ));
    // Unit/property tests: Rust + Python.
    let mut unit = Vec::new();
    let mut unit_notes = Vec::new();
    if opts.pure_only && selected.is_empty() {
        unit_notes.push("no tier = pure crate in the workspace".to_string());
    } else {
        unit.push(
            sel_args(&["test", "--profile", "ci"])
                .arg("--all-features")
                .arg("--no-fail-fast"),
        );
    }
    let py: Vec<String> = python_packages(root)
        .into_iter()
        .filter(|(_, tier, tests)| *tests && (!opts.pure_only || tier == "pure"))
        .map(|(d, _, _)| format!("{d}/tests"))
        .collect();
    if py.is_empty() {
        unit_notes.push("Python tests skipped: no selected Python package has a tests/ directory".into());
    } else {
        let mut c = Cmd::new(python_program(), &["-m", "pytest", "-q"]);
        for d in py {
            c = c.arg(d);
        }
        unit.push(c);
    }
    let mut s = step("unit/property (cargo test --profile ci)", unit);
    if !unit_notes.is_empty() {
        s.note = Some(unit_notes.join("; "));
    }
    steps.push(s);
    if !opts.fast {
        // One command with the unit step's package selection and features, so it reuses that
        // build (a per-package `-p` would resolve features differently and rebuild).
        let has_golden = selected
            .iter()
            .any(|m| root.join(&m.dir).join("tests").join("golden.rs").is_file());
        let golden: Vec<Cmd> = if has_golden {
            vec![
                sel_args(&["test", "--profile", "ci"])
                    .arg("--all-features")
                    .arg("--test")
                    .arg("golden"),
            ]
        } else {
            Vec::new()
        };
        if golden.is_empty() {
            steps.push(skip(
                "golden",
                "no golden test target (tests/golden.rs) in the selected workspace members yet".into(),
            ));
        } else {
            steps.push(step("golden", golden));
        }
        let dst = root.join(DST_DIR).is_dir() && ms.iter().any(|m| m.dir == "crates/hft-testkit");
        let seeds = DST_SMOKE_SEEDS.to_string();
        let first = HASHTRAIL_FIRST.to_string();
        if dst {
            steps.push(step(
                "dst smoke",
                vec![Cmd::new("xtask", &["dst", "--seeds", &seeds, "--regressions"])],
            ));
            steps.push(step(
                "dst --hashtrail check",
                vec![Cmd::new(
                    "xtask",
                    &["dst", "--hashtrail", "check", "--first", &first],
                )],
            ));
        } else {
            let why = format!("DST framework ({DST_DIR}/) is not in the workspace yet");
            steps.push(skip("dst smoke", why.clone()));
            steps.push(skip("dst --hashtrail check", why));
        }
    }
    steps.push(step(
        "gen-schema --check",
        vec![Cmd::new("xtask", &["gen-schema", "--check"])],
    ));
    steps.push(step("doc-check", vec![Cmd::new("xtask", &["doc-check"])]));
    Ok((steps, opt))
}

fn parse(args: &[String]) -> Result<Options, EnvError> {
    let mut o = Options::default();
    for a in args {
        match a.as_str() {
            "--pure-only" => o.pure_only = true,
            "--fast" => o.fast = true,
            x => return Err(EnvError(format!("unknown argument {x}"))),
        }
    }
    Ok(o)
}

/// Runs `ci` with `exec`; returns the exit code.
pub fn run_with(root: &Path, args: &[String], exec: &mut dyn Exec) -> i32 {
    let (steps, opt) = match parse(args).and_then(|o| plan(root, o).map(|p| (o, p))) {
        Ok((_, p)) => p,
        Err(e) => {
            eprintln!("ci: error: {e}");
            return EXIT_ENV;
        }
    };
    for (c, present) in &opt {
        if *present {
            eprintln!("ci: optional component {c}: in the workspace, included");
        } else {
            eprintln!("ci: optional component {c}: not in the workspace, skipped");
        }
    }
    let n = steps.len();
    let mut rows: Vec<Value> = Vec::new();
    let mut code = EXIT_OK;
    for (i, s) in steps.iter().enumerate() {
        let lines: Vec<String> = s.cmds.iter().map(Cmd::line).collect();
        if code != EXIT_OK {
            rows.push(json!({"step": s.name, "status": "not-run", "commands": lines}));
            continue;
        }
        if s.cmds.is_empty() {
            eprintln!(
                "ci: [{}/{n}] {}: skipped ({})",
                i + 1,
                s.name,
                s.note.clone().unwrap_or_default()
            );
            rows.push(json!({"step": s.name, "status": "skipped", "note": s.note}));
            continue;
        }
        eprintln!("ci: [{}/{n}] {}", i + 1, s.name);
        let mut st = EXIT_OK;
        for c in &s.cmds {
            match exec.run(c) {
                Ok(o) if o.ok() => {}
                Ok(o) => {
                    eprintln!("ci: `{}` exited {}", c.line(), o.code);
                    st = if o.code == EXIT_ENV { EXIT_ENV } else { EXIT_FAIL };
                    break;
                }
                Err(e) => {
                    eprintln!("ci: error: {e}");
                    st = EXIT_ENV;
                    break;
                }
            }
        }
        let status = if st == EXIT_OK { "pass" } else { "fail" };
        if let Some(note) = &s.note {
            eprintln!("ci:   note: {note}");
        }
        eprintln!("ci: [{}/{n}] {}: {status}", i + 1, s.name);
        rows.push(json!({"step": s.name, "status": status, "commands": lines, "note": s.note}));
        code = st;
    }
    let j = json!({
        "steps": rows,
        "optional": opt.iter().map(|(c, p)| json!({"component": c, "in_workspace": p})).collect::<Vec<_>>(),
        "result": if code == EXIT_OK { "pass" } else { "fail" },
    });
    if let Err(e) = write_json(root, "ci", &j) {
        eprintln!("ci: error: {e}");
        return EXIT_ENV;
    }
    eprintln!("ci: -> {}", if code == EXIT_OK { "PASS" } else { "FAIL" });
    code
}

/// Entry point.
pub fn run(root: &Path, args: &[String]) -> i32 {
    match crate::exec::from_env(root) {
        Ok(mut e) => run_with(root, args, e.as_mut()),
        Err(e) => {
            eprintln!("ci: error: {e}");
            EXIT_ENV
        }
    }
}
