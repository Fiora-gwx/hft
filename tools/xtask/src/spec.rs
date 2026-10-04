//! Test-spec parser for `TESTS.md` and `docs/tests/*.md` (format: docs/tests/README.md §4).
//!
//! Mirrors the case parser of `tools/doccheck/check_tests.py` (`parse_cases`) so both tools read the same
//! cases and fields from the same text.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use walkdir::WalkDir;

use crate::{EnvError, rel_path};

static CASE_HEAD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^###\s+(\S+)\s+(.*)$").unwrap());
static COMP_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^([A-Z]{2,4})-(\d{3})$").unwrap());
static SCEN_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(A\d+|B\d+|C\d+|[A-Z]{1,4}-\d+[a-z]?|INV-\d+|DRL-\d+)$").unwrap());
static ANY_HEAD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^#{1,3}\s").unwrap());
static FIELD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^- ([^\s:：]{1,6})[:：]\s*(.*)$").unwrap());
static IMPL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^`?([^`\s]+)::([A-Za-z0-9_]+)`?$").unwrap());
static MILESTONE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^M(\d+)(?:\.(\d+))?$").unwrap());
static STATUS_QUARANTINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^隔离[:：]").unwrap());
static STATUS_EXTENSION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^可选扩展[:：]\s*(.+)$").unwrap());

/// Levels that run in CI and therefore need code (docs/tests §6 rule 2).
pub const AUTO_LEVELS: &[&str] = &[
    "unit", "table", "prop", "golden", "fuzz", "bench", "dst", "integ", "proc", "replay",
];

/// One test case: a `### <id> <title>` heading plus its field list.
#[derive(Debug, Clone)]
pub struct Case {
    pub id: String,
    pub title: String,
    /// Spec file, relative to the repo root.
    pub file: String,
    /// 1-based line of the heading.
    pub line: usize,
    /// Field name → value (continuation lines joined with `\n`).
    pub fields: BTreeMap<String, String>,
    pub body: Vec<String>,
}

/// A milestone such as `M1` or `M1.5`, ordered numerically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Milestone(pub u32, pub u32);

impl Milestone {
    pub fn parse(s: &str) -> Option<Milestone> {
        let c = MILESTONE.captures(s.trim())?;
        let major = c[1].parse().ok()?;
        let minor = c.get(2).map_or(Some(0), |m| m.as_str().parse().ok())?;
        Some(Milestone(major, minor))
    }
}

/// Parsed `实现` field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Impl {
    /// `待实现`, empty or missing.
    Pending,
    /// `<path>::<fn>`.
    At { path: String, func: String },
    /// Anything else (raw text).
    Other(String),
}

impl Case {
    pub fn field(&self, name: &str) -> &str {
        self.fields.get(name).map_or("", |s| s.trim())
    }

    /// `级别` without backticks.
    pub fn level(&self) -> String {
        self.field("级别").trim_matches('`').trim().to_string()
    }

    pub fn milestone(&self) -> Option<Milestone> {
        Milestone::parse(self.field("里程碑"))
    }

    /// A deleted case keeps its heading; body says `已删除` and has at most one field.
    pub fn is_deleted(&self) -> bool {
        self.fields.len() <= 1 && self.body.iter().any(|l| l.contains("已删除"))
    }

    pub fn implementation(&self) -> Impl {
        let raw = self.field("实现");
        if raw.is_empty() || raw.trim_matches('`') == "待实现" {
            return Impl::Pending;
        }
        match IMPL.captures(raw) {
            Some(c) => Impl::At {
                path: c[1].to_string(),
                func: c[2].to_string(),
            },
            None => Impl::Other(raw.to_string()),
        }
    }

    /// `状态:隔离:<Agent Note 链接>`.
    pub fn is_quarantined(&self) -> bool {
        STATUS_QUARANTINE.is_match(self.field("状态"))
    }

    /// `状态:可选扩展:<名>` → the extension name.
    pub fn extension(&self) -> Option<String> {
        let st = self.field("状态");
        let first = st.lines().next().unwrap_or("");
        STATUS_EXTENSION.captures(first).map(|c| c[1].trim().to_string())
    }

    /// First backticked token of `基准名` (or the whole value).
    pub fn bench_name(&self) -> Option<String> {
        let v = self.field("基准名");
        if v.is_empty() {
            return None;
        }
        if let Some(a) = v.find('`')
            && let Some(len) = v[a + 1..].find('`')
        {
            return Some(v[a + 1..a + 1 + len].trim().to_string());
        }
        Some(v.to_string())
    }
}

/// True for a case id (`OMS-014`, `B-1`, `INV-3`, …) as accepted in a case heading.
pub fn is_case_id(s: &str) -> bool {
    COMP_ID.is_match(s) || SCEN_ID.is_match(s)
}

/// Parse the cases of one spec file (same rules as `check_tests.py::parse_cases`).
pub fn parse_cases(file: &str, text: &str) -> Vec<Case> {
    let mut cases: Vec<Case> = Vec::new();
    let mut cur: Option<usize> = None;
    let mut in_code = false;
    let mut field: Option<String> = None;
    for (i, line) in text.lines().enumerate() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
        }
        if !in_code {
            if let Some(c) = CASE_HEAD.captures(line)
                && is_case_id(&c[1])
            {
                cases.push(Case {
                    id: c[1].to_string(),
                    title: c[2].to_string(),
                    file: file.to_string(),
                    line: i + 1,
                    fields: BTreeMap::new(),
                    body: Vec::new(),
                });
                cur = Some(cases.len() - 1);
                field = None;
                continue;
            }
            if ANY_HEAD.is_match(line) {
                cur = None;
                field = None;
                continue;
            }
        }
        let Some(ci) = cur else { continue };
        let case = &mut cases[ci];
        case.body.push(line.to_string());
        match FIELD.captures(line) {
            Some(c) if !in_code => {
                let name = c[1].to_string();
                case.fields.insert(name.clone(), c[2].to_string());
                field = Some(name);
            }
            _ => {
                if let Some(f) = &field
                    && (line.starts_with("  ") || line.trim().is_empty())
                    && !line.starts_with("- ")
                    && let Some(v) = case.fields.get_mut(f)
                {
                    v.push('\n');
                    v.push_str(line);
                }
            }
        }
    }
    cases
}

/// Directories never scanned (build output, VCS, virtualenvs, hidden directories).
pub fn skip_dir(name: &str) -> bool {
    name.starts_with('.')
        || matches!(
            name,
            "target" | "node_modules" | "__pycache__" | "venv" | "site-packages"
        )
}

/// Fixture repos of the xtask tests: fake specs and code, never part of the real trace.
pub const FIXTURES_DIR: &str = "tools/xtask/tests/fixtures";

/// Files under `root`, sorted, skipping [`skip_dir`] directories and [`FIXTURES_DIR`].
pub fn walk_repo(root: &Path) -> impl Iterator<Item = walkdir::Result<walkdir::DirEntry>> {
    let fixtures = root.join(FIXTURES_DIR);
    WalkDir::new(root)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(move |e| {
            e.depth() == 0
                || !(e.file_type().is_dir()
                    && (skip_dir(&e.file_name().to_string_lossy()) || e.path() == fixtures))
        })
}

/// All spec files: every `TESTS.md` in the repo plus `docs/tests/*.md` except its `README.md`
/// (the format page, whose examples are not cases).
pub fn spec_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in walk_repo(root).flatten() {
        if e.file_type().is_file() && e.file_name() == "TESTS.md" {
            out.push(e.into_path());
        }
    }
    let dt = root.join("docs").join("tests");
    if let Ok(rd) = std::fs::read_dir(&dt) {
        let mut extra: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.extension().is_some_and(|x| x == "md")
                    && p.file_name().is_some_and(|n| n != "README.md" && n != "TESTS.md")
            })
            .collect();
        extra.sort();
        out.extend(extra);
    }
    out
}

/// Parse every spec file under `root`.
pub fn load_cases(root: &Path) -> Result<(usize, Vec<Case>), EnvError> {
    let files = spec_files(root);
    let mut cases = Vec::new();
    for p in &files {
        let text = std::fs::read_to_string(p).map_err(|e| EnvError(format!("read {}: {e}", p.display())))?;
        cases.extend(parse_cases(&rel_path(root, p), &text));
    }
    Ok((files.len(), cases))
}
