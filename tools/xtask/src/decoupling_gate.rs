//! `cargo xtask decoupling-gate --base <sha> --head <sha> [--pr <n>]` (tools/README.md
//! "cargo xtask decoupling-gate"). Reads git and GitHub only, through [`Exec`]:
//!
//! 1. `git rev-parse --verify --quiet <sha>^{commit}` for both ends (missing → exit 2);
//! 2. `git diff --name-only <base>..<head>`, classified by the `[layers]` table of `tools/xtask/deps.toml`
//!    (`strategy`, `engine`, `app`; `tool` and everything else are neutral);
//! 3. strategy files together with engine or app files → fail, unless `--pr` carries the
//!    `engine-change` label and an Admin (`.github/hft-admins.txt`) approved it — or an Admin authored
//!    it (an Admin cannot approve their own PR): `gh api` reads the PR (labels, author) and its
//!    reviews; a reviewer's latest non-comment review decides (`APPROVED`).
//!    No `GITHUB_TOKEN`/`GH_TOKEN`, an API failure or a missing admin list → exit 2.
//!
//! Exemptions (and failures) are appended to `$GITHUB_STEP_SUMMARY` and written to
//! `target/xtask/decoupling-gate.json`.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::exec::{Cmd, Exec};
use crate::{EXIT_ENV, EXIT_FAIL, EXIT_OK, EnvError, write_json};

pub const LABEL: &str = "engine-change";
pub const ADMINS: &str = ".github/hft-admins.txt";

/// What the gate reads from the process environment.
#[derive(Debug, Clone, Default)]
pub struct GateEnv {
    pub token: Option<String>,
    /// `owner/repo` (`GITHUB_REPOSITORY`); `{owner}/{repo}` (resolved by `gh`) when unset.
    pub repo: Option<String>,
    pub step_summary: Option<PathBuf>,
}

impl GateEnv {
    pub fn from_process() -> GateEnv {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        GateEnv {
            token: var("GITHUB_TOKEN").or_else(|| var("GH_TOKEN")),
            repo: var("GITHUB_REPOSITORY"),
            step_summary: var("GITHUB_STEP_SUMMARY").map(PathBuf::from),
        }
    }
}

#[derive(Debug, Default)]
struct Args {
    base: String,
    head: String,
    pr: Option<u64>,
}

fn parse(args: &[String]) -> Result<Args, EnvError> {
    let mut a = Args::default();
    let mut it = args.iter();
    while let Some(k) = it.next() {
        let mut val = || {
            it.next()
                .cloned()
                .ok_or_else(|| EnvError(format!("{k} needs a value")))
        };
        match k.as_str() {
            "--base" => a.base = val()?,
            "--head" => a.head = val()?,
            "--pr" => {
                let v = val()?;
                a.pr = Some(v.parse().map_err(|_| EnvError(format!("bad --pr {v}")))?);
            }
            x => return Err(EnvError(format!("unknown argument {x}"))),
        }
    }
    if a.base.is_empty() || a.head.is_empty() {
        return Err(EnvError("--base and --head are required".into()));
    }
    Ok(a)
}

/// `[layers]` of deps.toml: layer → path patterns (`crates/*`).
fn layers(root: &Path) -> Result<BTreeMap<String, Vec<String>>, EnvError> {
    let p = root.join(crate::deps::CONFIG);
    let text = std::fs::read_to_string(&p).map_err(|e| EnvError(format!("read {}: {e}", p.display())))?;
    let t: toml::Table = text
        .parse()
        .map_err(|e| EnvError(format!("parse {}: {e}", p.display())))?;
    let l = t
        .get("layers")
        .and_then(|v| v.as_table())
        .ok_or_else(|| EnvError(format!("{}: missing [layers]", crate::deps::CONFIG)))?;
    Ok(l.iter()
        .map(|(k, v)| {
            let pats = v
                .as_array()
                .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            (k.clone(), pats)
        })
        .collect())
}

/// Whether `file` lies under the layer pattern `pat` (`crates/*` = any file below `crates/<x>/`).
fn under(pat: &str, file: &str) -> bool {
    match pat.strip_suffix("/*") {
        Some(base) => file
            .strip_prefix(base)
            .and_then(|r| r.strip_prefix('/'))
            .is_some_and(|r| r.contains('/')),
        None => file == pat || file.starts_with(&format!("{pat}/")),
    }
}

fn layer_of<'a>(ls: &'a BTreeMap<String, Vec<String>>, file: &str) -> Option<&'a str> {
    ls.iter()
        .find(|(_, pats)| pats.iter().any(|p| under(p, file)))
        .map(|(k, _)| k.as_str())
}

/// `crates/hft-oms/src/lib.rs` → `crates/hft-oms`.
fn component(file: &str) -> String {
    file.splitn(3, '/').take(2).collect::<Vec<_>>().join("/")
}

fn git(exec: &mut dyn Exec, args: &[&str]) -> Result<crate::exec::Output, EnvError> {
    exec.run(&Cmd::capture("git", args))
}

/// Outcome of a gate run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub code: i32,
    pub json: Value,
}

fn gh_json(exec: &mut dyn Exec, env: &GateEnv, path: &str) -> Result<Value, EnvError> {
    let mut c = Cmd::capture("gh", &["api", "-H", "Accept: application/vnd.github+json", path]);
    if let Some(t) = &env.token {
        c.env.push(("GH_TOKEN".into(), t.clone()));
    }
    let o = exec.run(&c)?;
    if !o.ok() {
        return Err(EnvError(format!(
            "gh api {path} failed (exit {}): {}",
            o.code,
            o.stderr.trim()
        )));
    }
    serde_json::from_str(&o.stdout).map_err(|e| EnvError(format!("gh api {path}: bad JSON: {e}")))
}

/// Exemption check for `pr`: `Ok(Some(approver))`, `Ok(None)` (not exempt; reasons on stderr).
fn exemption(root: &Path, exec: &mut dyn Exec, env: &GateEnv, pr: u64) -> Result<Option<String>, EnvError> {
    if env.token.is_none() {
        return Err(EnvError(
            "GITHUB_TOKEN is not set: cannot read the PR's labels and reviews".into(),
        ));
    }
    let admins_p = root.join(ADMINS);
    let admins: Vec<String> = std::fs::read_to_string(&admins_p)
        .map_err(|e| EnvError(format!("read {ADMINS}: {e}")))?
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    let repo = env.repo.clone().unwrap_or_else(|| "{owner}/{repo}".into());
    let pull = gh_json(exec, env, &format!("repos/{repo}/pulls/{pr}"))?;
    let reviews = gh_json(
        exec,
        env,
        &format!("repos/{repo}/pulls/{pr}/reviews?per_page=100"),
    )?;
    let labeled = pull.get("labels").and_then(Value::as_array).is_some_and(|a| {
        a.iter()
            .any(|l| l.get("name").and_then(Value::as_str) == Some(LABEL))
    });
    // Latest deciding review per reviewer.
    let mut last: BTreeMap<String, String> = BTreeMap::new();
    for r in reviews.as_array().map(Vec::as_slice).unwrap_or(&[]) {
        let user = r
            .get("user")
            .and_then(|u| u.get("login"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let state = r.get("state").and_then(Value::as_str).unwrap_or("");
        if !user.is_empty() && matches!(state, "APPROVED" | "CHANGES_REQUESTED" | "DISMISSED") {
            last.insert(user.to_string(), state.to_string());
        }
    }
    let approvers: Vec<&String> = last
        .iter()
        .filter(|(_, s)| *s == "APPROVED")
        .map(|(u, _)| u)
        .collect();
    for u in &approvers {
        if !admins.contains(u) {
            eprintln!("decoupling-gate: approval by {u} does not count: {u} 不在 Admin 名单 ({ADMINS})");
        }
    }
    let author = pull
        .get("user")
        .and_then(|u| u.get("login"))
        .and_then(Value::as_str)
        .filter(|a| admins.iter().any(|x| x == a));
    // An Admin's own PR: the label is the Admin's confirmation (GitHub refuses self-approval).
    let admin = approvers
        .iter()
        .find(|u| admins.contains(u))
        .map(|u| u.to_string())
        .or_else(|| author.map(|a| format!("{a} (author)")));
    if !labeled {
        eprintln!("decoupling-gate: PR {pr} has no `{LABEL}` label");
    }
    if admin.is_none() {
        eprintln!(
            "decoupling-gate: PR {pr} has no approving review by an Admin and no Admin author ({ADMINS})"
        );
    }
    Ok(if labeled { admin } else { None })
}

fn summary(env: &GateEnv, text: &str) {
    if let Some(p) = &env.step_summary
        && let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p)
    {
        let _ = writeln!(f, "{text}");
    }
}

fn evaluate(root: &Path, args: &[String], exec: &mut dyn Exec, env: &GateEnv) -> Result<Outcome, EnvError> {
    let a = parse(args)?;
    let ls = layers(root)?;
    for sha in [&a.base, &a.head] {
        let o = git(
            exec,
            &["rev-parse", "--verify", "--quiet", &format!("{sha}^{{commit}}")],
        )?;
        if !o.ok() {
            return Err(EnvError(format!("commit {sha} does not exist")));
        }
    }
    let o = git(exec, &["diff", "--name-only", &format!("{}..{}", a.base, a.head)])?;
    if !o.ok() {
        return Err(EnvError(format!("git diff failed: {}", o.stderr.trim())));
    }
    let files: Vec<String> = o
        .stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    let pick = |layer: &str| -> Vec<String> {
        files
            .iter()
            .filter(|f| layer_of(&ls, f) == Some(layer))
            .cloned()
            .collect()
    };
    let (strat, engine, apps) = (pick("strategy"), pick("engine"), pick("app"));
    let mut j = json!({
        "base": a.base, "head": a.head, "pr": a.pr,
        "files": files.len(), "strategies": strat, "engine": engine, "apps": apps,
    });
    let mixed = !strat.is_empty() && !(engine.is_empty() && apps.is_empty());
    if !mixed {
        eprintln!(
            "decoupling-gate: {} files ({} strategy, {} engine, {} app) -> PASS",
            files.len(),
            strat.len(),
            engine.len(),
            apps.len()
        );
        j["result"] = json!("pass");
        return Ok(Outcome {
            code: EXIT_OK,
            json: j,
        });
    }
    eprintln!("decoupling-gate: strategies and engine crates/apps changed in the same diff:");
    for f in strat.iter().chain(&engine).chain(&apps) {
        eprintln!("  {f}");
    }
    let comps = |v: &[String]| {
        let mut c: Vec<String> = v.iter().map(|f| component(f)).collect();
        c.sort();
        c.dedup();
        c
    };
    let (ec, ac) = (comps(&engine), comps(&apps));
    if let Some(pr) = a.pr
        && let Some(approver) = exemption(root, exec, env, pr)?
    {
        let ex = json!({"pr": pr, "approver": approver, "engine": ec, "apps": ac});
        j["exemption"] = ex;
        j["result"] = json!("exempt");
        summary(
            env,
            &format!(
                "### decoupling-gate: 豁免(exemption)\n\n- PR #{pr}\n- approver: {approver}\n- engine crates: {}\n- apps: {}\n",
                if ec.is_empty() {
                    "-".to_string()
                } else {
                    ec.join(", ")
                },
                if ac.is_empty() {
                    "-".to_string()
                } else {
                    ac.join(", ")
                },
            ),
        );
        eprintln!("decoupling-gate: exempt (PR {pr}, label `{LABEL}`, approved by Admin {approver}) -> PASS");
        return Ok(Outcome {
            code: EXIT_OK,
            json: j,
        });
    }
    summary(
        env,
        &format!(
            "### decoupling-gate: FAIL\n\nstrategies and engine crates/apps changed together\n- strategy files: {}\n- engine/app components: {}\n",
            strat.len(),
            ec.iter().chain(&ac).cloned().collect::<Vec<_>>().join(", ")
        ),
    );
    j["result"] = json!("fail");
    eprintln!("decoupling-gate: -> FAIL");
    Ok(Outcome {
        code: EXIT_FAIL,
        json: j,
    })
}

/// Runs the gate with `exec` and `env`; writes the JSON; returns the exit code.
pub fn run_with(root: &Path, args: &[String], exec: &mut dyn Exec, env: &GateEnv) -> i32 {
    match evaluate(root, args, exec, env) {
        Ok(o) => {
            if let Err(e) = write_json(root, "decoupling-gate", &o.json) {
                eprintln!("decoupling-gate: error: {e}");
                return EXIT_ENV;
            }
            o.code
        }
        Err(e) => {
            eprintln!("decoupling-gate: error: {e}");
            let _ = write_json(root, "decoupling-gate", &json!({"result": "error", "error": e.0}));
            EXIT_ENV
        }
    }
}

/// Entry point.
pub fn run(root: &Path, args: &[String]) -> i32 {
    match crate::exec::from_env(root) {
        Ok(mut e) => run_with(root, args, e.as_mut(), &GateEnv::from_process()),
        Err(e) => {
            eprintln!("decoupling-gate: error: {e}");
            EXIT_ENV
        }
    }
}
