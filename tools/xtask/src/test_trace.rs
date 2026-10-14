//! `cargo xtask test-trace`: traceability between test specs and test code.
//!
//! Rules: docs/tests/README.md §6. Configuration: `tools/xtask/test-trace.toml`
//! (`current_milestone`, `enabled_extensions`).
//!
//! Evidence that a case is implemented, by level:
//! - every automatic level: a test function whose name starts with the case number
//!   (`<prefix>_<nnn>_…`; pytest `test_<prefix>_<nnn>_…`), not `#[ignore]`d unless the case is quarantined;
//! - `dst` additionally: a `Scenario::new("<id>", …)` registration (location = the enclosing function);
//! - `bench`: a string literal equal to `基准名` in a bench file (location = the enclosing function);
//! - `fuzz`: a function named `<prefix>_<nnn>_…` in a fuzz-target file.
//!
//! The `实现` field (`<path>::<fn>`) must equal the single location found.
//!
//! Report-only (owner decision 2026-10-14): `unimplemented`, `extra` and `inconsistent` are printed
//! and written to the JSON but never change the exit code; together with a per-milestone count of
//! implemented / unimplemented cases they measure progress. Only hard spec errors exit 1: a case
//! id defined twice, an unparsable `里程碑` or a `实现` that is neither `待实现` nor `<path>::<fn>`
//! on an automatic-level case.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::json;

use crate::code_scan::{CodeIndex, is_fuzz_file, scan_repo};
use crate::spec::{AUTO_LEVELS, Case, Impl, Milestone, load_cases};
use crate::{EXIT_ENV, EXIT_FAIL, EXIT_OK, EnvError, write_json};

/// Test-function names that carry a case number (docs/tests §6 rule 3).
static NUMBERED_FN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(test_)?([a-z]+)_([0-9]{3})_").unwrap());

/// Path of the configuration file, relative to the repo root.
pub const CONFIG: &str = "tools/xtask/test-trace.toml";

/// `tools/xtask/test-trace.toml`.
#[derive(Debug, Clone)]
pub struct Config {
    pub current_milestone: Milestone,
    pub current_milestone_text: String,
    pub enabled_extensions: Vec<String>,
}

impl Config {
    pub fn load(root: &Path) -> Result<Config, EnvError> {
        let path = root.join(CONFIG);
        let text = std::fs::read_to_string(&path).map_err(|e| EnvError(format!("{CONFIG}: {e}")))?;
        let v: toml::Table = text.parse().map_err(|e| EnvError(format!("{CONFIG}: {e}")))?;
        let ms_text = v
            .get("current_milestone")
            .and_then(|x| x.as_str())
            .ok_or_else(|| EnvError(format!("{CONFIG}: missing string current_milestone")))?
            .to_string();
        let current_milestone = Milestone::parse(&ms_text)
            .ok_or_else(|| EnvError(format!("{CONFIG}: bad current_milestone '{ms_text}'")))?;
        let enabled_extensions = match v.get("enabled_extensions") {
            None => Vec::new(),
            Some(toml::Value::Array(a)) => a
                .iter()
                .map(|x| {
                    x.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| EnvError(format!("{CONFIG}: enabled_extensions must be strings")))
                })
                .collect::<Result<_, _>>()?,
            Some(_) => {
                return Err(EnvError(format!("{CONFIG}: enabled_extensions must be an array")));
            }
        };
        Ok(Config {
            current_milestone,
            current_milestone_text: ms_text,
            enabled_extensions,
        })
    }
}

/// One entry of the `inconsistent` list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inconsistent {
    pub id: String,
    /// The `实现` field as written.
    pub spec: String,
    /// Locations found, `<path>::<fn>`.
    pub found: Vec<String>,
}

/// Result of a trace run.
#[derive(Debug, Default)]
pub struct Report {
    pub spec_files: usize,
    pub cases: usize,
    pub required: usize,
    pub test_fns: usize,
    /// Case ids, with a human note each.
    pub unimplemented: Vec<(String, String)>,
    /// Test-function names, with their location.
    pub extra: Vec<(String, String)>,
    pub inconsistent: Vec<Inconsistent>,
    /// Non-fatal notes.
    pub notes: Vec<String>,
    /// Hard spec errors (the only findings that fail the run).
    pub errors: Vec<String>,
    /// Per milestone (every milestone, not only up to `current_milestone`): automatic-level cases,
    /// and how many of them have no live test.
    pub by_milestone: BTreeMap<Milestone, MilestoneCount>,
}

/// Automatic-level cases of one milestone (deleted cases and disabled extensions excluded).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct MilestoneCount {
    pub cases: usize,
    pub unimplemented: usize,
}

/// `M2`, `M1.5`.
pub fn milestone_text(m: Milestone) -> String {
    if m.1 == 0 {
        format!("M{}", m.0)
    } else {
        format!("M{}.{}", m.0, m.1)
    }
}

impl Report {
    /// Only hard spec errors fail; the three lists are report-only.
    pub fn passed(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn to_json(&self) -> serde_json::Value {
        json!({
            "unimplemented": self.unimplemented.iter().map(|(id, _)| id).collect::<Vec<_>>(),
            "extra": self.extra.iter().map(|(name, _)| name).collect::<Vec<_>>(),
            "inconsistent": self.inconsistent.iter().map(|x| json!({
                "id": x.id, "spec": x.spec, "found": x.found,
            })).collect::<Vec<_>>(),
            "errors": self.errors,
            "by_milestone": self.by_milestone.iter().map(|(m, c)| (milestone_text(*m), json!({
                "cases": c.cases, "implemented": c.cases - c.unimplemented, "unimplemented": c.unimplemented,
            }))).collect::<serde_json::Map<_, _>>(),
        })
    }
}

/// Case id carried by a numbered test-function name (`oms_014_x` → `OMS-014`).
pub fn fn_case_id(name: &str) -> Option<String> {
    let c = NUMBERED_FN.captures(name)?;
    Some(format!("{}-{}", c[2].to_ascii_uppercase(), &c[3]))
}

/// Function-name stem of a case id (`OMS-014` → `oms_014`, `B-1` → `b_1`).
fn stem(id: &str) -> String {
    id.to_ascii_lowercase().replace(['-', '.'], "_")
}

fn named_for(name: &str, stem: &str) -> bool {
    let n = name.strip_prefix("test_").unwrap_or(name);
    n == stem || n.strip_prefix(stem).is_some_and(|rest| rest.starts_with('_'))
}

/// The milestone of an automatic-level, live case whose extension (if any) is enabled; `None` for
/// cases outside the trace. An unparsable milestone is a hard error.
fn traced(case: &Case, cfg: &Config, errors: &mut Vec<String>) -> Option<Milestone> {
    if !AUTO_LEVELS.contains(&case.level().as_str()) || case.is_deleted() {
        return None;
    }
    let Some(ms) = case.milestone() else {
        errors.push(format!(
            "{}:{} {}: unparsable 里程碑 '{}'",
            case.file,
            case.line,
            case.id,
            case.field("里程碑")
        ));
        return None;
    };
    match case.extension() {
        Some(ext) if !cfg.enabled_extensions.contains(&ext) => None,
        _ => Some(ms),
    }
}

/// Locations (`path::fn`) evidencing `case`, with whether each is `#[ignore]`d.
fn evidence(case: &Case, idx: &CodeIndex) -> Vec<(String, bool)> {
    let level = case.level();
    let st = stem(&case.id);
    let mut out: BTreeMap<String, bool> = BTreeMap::new();
    let mut add = |loc: String, ignored: bool| {
        let e = out.entry(loc).or_insert(true);
        *e = *e && ignored;
    };
    for f in &idx.fns {
        let numbered = f.is_test && fn_case_id(&f.name).as_deref() == Some(case.id.as_str());
        let fuzz = level == "fuzz" && is_fuzz_file(&f.file) && named_for(&f.name, &st);
        if numbered || fuzz {
            add(format!("{}::{}", f.file, f.name), f.ignored && !fuzz);
        }
    }
    if level == "dst" {
        for s in idx.scenarios.iter().filter(|s| s.value == case.id) {
            add(
                format!("{}::{}", s.file, s.enclosing.as_deref().unwrap_or("?")),
                false,
            );
        }
    }
    if level == "bench"
        && let Some(name) = case.bench_name()
    {
        for s in idx.bench_literals.iter().filter(|s| s.value == name) {
            add(
                format!("{}::{}", s.file, s.enclosing.as_deref().unwrap_or("?")),
                false,
            );
        }
    }
    out.into_iter().collect()
}

/// Run the trace over `root` (no output).
pub fn trace(root: &Path, cfg: &Config) -> Result<Report, EnvError> {
    let (spec_files, cases) = load_cases(root)?;
    let idx = scan_repo(root)?;
    let mut r = Report {
        spec_files,
        cases: cases.len(),
        test_fns: idx.fns.iter().filter(|f| f.is_test).count(),
        ..Report::default()
    };
    let ids: BTreeSet<&str> = cases.iter().map(|c| c.id.as_str()).collect();
    let mut first_at: BTreeMap<&str, String> = BTreeMap::new();
    for case in &cases {
        let at = format!("{}:{}", case.file, case.line);
        if let Some(prev) = first_at.get(case.id.as_str()) {
            r.errors
                .push(format!("{at} {}: case id defined twice (also {prev})", case.id));
        } else {
            first_at.insert(&case.id, at);
        }
    }

    for case in &cases {
        let Some(ms) = traced(case, cfg, &mut r.errors) else {
            continue;
        };
        let found = evidence(case, &idx);
        let live: Vec<&String> = found
            .iter()
            .filter(|(_, ign)| !*ign || case.is_quarantined())
            .map(|(l, _)| l)
            .collect();
        let count = r.by_milestone.entry(ms).or_default();
        count.cases += 1;
        if live.is_empty() {
            count.unimplemented += 1;
        }
        let at = format!("{}:{}", case.file, case.line);
        if let Impl::Other(raw) = case.implementation() {
            r.errors.push(format!(
                "{at} {}: 实现 '{raw}' is neither 待实现 nor `<path>::<fn>`",
                case.id
            ));
            continue;
        }
        if ms > cfg.current_milestone {
            continue;
        }
        r.required += 1;
        if live.is_empty() {
            let why = if found.is_empty() {
                "no test found".to_string()
            } else {
                "test is #[ignore]d".to_string()
            };
            r.unimplemented.push((
                case.id.clone(),
                format!("{at} {} {}: {why}", case.field("里程碑"), case.level()),
            ));
            continue;
        }
        match case.implementation() {
            Impl::Pending => r.unimplemented.push((
                case.id.clone(),
                format!(
                    "{at}: 实现 = 待实现 but found {}",
                    live.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                ),
            )),
            Impl::At { path, func } => {
                let want = format!("{path}::{func}");
                if live.len() != 1 || *live[0] != want {
                    r.inconsistent.push(Inconsistent {
                        id: case.id.clone(),
                        spec: want,
                        found: live.iter().map(|s| s.to_string()).collect(),
                    });
                }
            }
            Impl::Other(_) => unreachable!("reported as an error above"),
        }
    }

    let mut seen = BTreeSet::new();
    for f in idx.fns.iter().filter(|f| f.is_test) {
        if let Some(id) = fn_case_id(&f.name)
            && !ids.contains(id.as_str())
            && seen.insert(f.name.clone())
        {
            r.extra
                .push((f.name.clone(), format!("{}:{} (no case {id})", f.file, f.line)));
        }
    }
    r.unimplemented.sort();
    r.unimplemented.dedup_by(|a, b| a.0 == b.0);
    r.extra.sort();
    r.inconsistent.sort_by(|a, b| a.id.cmp(&b.id));
    r.errors.sort();
    Ok(r)
}

/// Human summary for stderr.
pub fn summary(r: &Report, cfg: &Config) -> String {
    let mut s = String::new();
    let ext = cfg.enabled_extensions.join(", ");
    s += &format!(
        "test-trace: current_milestone = {}, enabled_extensions = [{ext}]\n",
        cfg.current_milestone_text
    );
    s += &format!(
        "test-trace: {} spec files, {} cases, {} required; {} test functions\n",
        r.spec_files, r.cases, r.required, r.test_fns
    );
    for n in &r.notes {
        s += &format!("note: {n}\n");
    }
    s += &format!("unimplemented ({}):\n", r.unimplemented.len());
    for (id, why) in &r.unimplemented {
        s += &format!("  {id}  {why}\n");
    }
    s += &format!("extra ({}):\n", r.extra.len());
    for (name, at) in &r.extra {
        s += &format!("  {name}  {at}\n");
    }
    s += &format!("inconsistent ({}):\n", r.inconsistent.len());
    for x in &r.inconsistent {
        s += &format!("  {}  spec {}  found [{}]\n", x.id, x.spec, x.found.join(", "));
    }
    s += "by milestone (automatic levels, report-only):\n";
    for (m, c) in &r.by_milestone {
        s += &format!(
            "  {:<5} cases={:<5} implemented={:<5} unimplemented={}\n",
            milestone_text(*m),
            c.cases,
            c.cases - c.unimplemented,
            c.unimplemented
        );
    }
    s += &format!("errors ({}):\n", r.errors.len());
    for e in &r.errors {
        s += &format!("  {e}\n");
    }
    s += &format!(
        "test-trace: unimplemented={} extra={} inconsistent={} (report-only) errors={} -> {}\n",
        r.unimplemented.len(),
        r.extra.len(),
        r.inconsistent.len(),
        r.errors.len(),
        if r.passed() { "PASS" } else { "FAIL" }
    );
    s
}

/// Entry point: returns the exit code.
pub fn run(root: &Path) -> i32 {
    let result = Config::load(root).and_then(|cfg| {
        let r = trace(root, &cfg)?;
        write_json(root, "test-trace", &r.to_json())?;
        Ok((r, cfg))
    });
    match result {
        Ok((r, cfg)) => {
            eprint!("{}", summary(&r, &cfg));
            if r.passed() { EXIT_OK } else { EXIT_FAIL }
        }
        Err(e) => {
            eprintln!("test-trace: error: {e}");
            EXIT_ENV
        }
    }
}
