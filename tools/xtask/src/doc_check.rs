//! `cargo xtask doc-check`: documentation gates (tools/README.md "cargo xtask doc-check").
//!
//! Native checks (one per row of the tools/README.md doc-check table; [`native`], [`layout`], [`budget`]) run on
//! the tree at the repo root, driven by `tools/xtask/doc-check.toml` ([`config`]); then the external scripts that
//! config lists (`tools/doccheck/check_hft.py`, `check_tests.py`) run, each printing `problems=<n>` as its last
//! summary line. Any native problem or non-zero script count fails the gate.
//! The pure helpers [`slug`], [`slugs`] and [`doc_length`] are the anchor and length rules shared with
//! `check_hft.py` (same definitions); [`check_rings`] is the "环名" row.

pub mod budget;
pub mod config;
pub mod layout;
pub mod md;
pub mod native;
pub mod perms;

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::json;

use crate::{EXIT_ENV, EXIT_FAIL, EXIT_OK, write_json};

static LINK_TEXT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[([^\]]*)\]\([^)]*\)").unwrap());
static NOT_SLUG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^\w\- ]").unwrap());
static LINK_URL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\]\([^)\s]*\)").unwrap());
static CJK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\x{3400}-\x{4DBF}\x{4E00}-\x{9FFF}\x{F900}-\x{FAFF}]").unwrap());
static WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[A-Za-z0-9_.\-]+").unwrap());
static PROBLEMS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^problems=(\d+)").unwrap());

/// GitHub anchor of a heading text: link text kept, lowercase, punctuation (incl. full-width) dropped,
/// spaces → `-`, CJK kept.
pub fn slug(heading: &str) -> String {
    let h = LINK_TEXT.replace_all(heading.trim(), "$1").to_lowercase();
    NOT_SLUG.replace_all(&h, "").replace(' ', "-")
}

/// Anchors of a sequence of headings in one file: repeated slugs get `-1`, `-2`, … (GitHub rule).
pub fn slugs<'a>(headings: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    headings
        .into_iter()
        .map(|h| {
            let b = slug(h);
            let n = seen.entry(b.clone()).or_insert(0);
            let out = if *n == 0 { b } else { format!("{b}-{n}") };
            *n += 1;
            out
        })
        .collect()
}

/// Length measure of docs/AGENTS.md "篇幅": drop fenced code blocks and Markdown link URLs, then
/// count CJK ideographs plus `[A-Za-z0-9_.-]+` tokens that contain a letter or digit.
pub fn doc_length(text: &str) -> usize {
    let mut kept = Vec::new();
    let mut in_code = false;
    for line in text.lines() {
        let st = line.trim_start();
        if st.starts_with("```") || st.starts_with("~~~") {
            in_code = !in_code;
            continue;
        }
        if !in_code {
            kept.push(line);
        }
    }
    let t = kept.join("\n");
    let t = LINK_URL.replace_all(&t, "]");
    CJK.find_iter(&t).count()
        + WORD
            .find_iter(&t)
            .filter(|m| m.as_str().bytes().any(|b| b.is_ascii_alphanumeric()))
            .count()
}

/// First Python 3 interpreter among `python`, `python3`.
pub fn find_python() -> Option<String> {
    ["python", "python3"].into_iter().find_map(|p| {
        let out = Command::new(p).arg("--version").output().ok()?;
        let v = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
        (out.status.success() && v.trim_start().starts_with("Python 3")).then(|| p.to_string())
    })
}

/// One ring row of a Markdown table (`| 环 | … |`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RingRow {
    /// Ring name (`rep-<venue>`).
    pub ring: String,
    /// Element type (aliases resolved: `OutMsg` → `GwCmd`; "同 `<ring>`" → that ring's type).
    pub elem: String,
    /// Companion byte rings named in the element cell (`rep-<venue>-bulk`).
    pub companions: Vec<String>,
    /// 1-based line in its file.
    pub line: usize,
}

/// hft-event README (owner of the 环拓扑 table).
pub const RING_EVENT_MD: &str = "crates/hft-event/README.md";
/// hft-sequencer README (owner of the ring table).
pub const RING_SEQ_MD: &str = "crates/hft-sequencer/README.md";
/// Optional rings that take no part in the comparison.
pub const RING_OPTIONAL: &[&str] = &["chain-in", "chain-out"];

static BACKTICK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`([^`]+)`").unwrap());
static RING_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z]+(?:-[a-z]+)*(?:-<venue>(?:-[a-z]+)*)?$").unwrap());

fn md_cells(line: &str) -> Vec<String> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    t.split('|').map(|c| c.trim().to_string()).collect()
}

/// Element type of an element cell: its first backticked token, `Type::Variant` / `Type{…}` cut to
/// `Type`, `OutMsg` → `GwCmd`.
fn first_type(cell: &str) -> String {
    let t = BACKTICK
        .captures(cell)
        .map(|c| c[1].to_string())
        .unwrap_or_default();
    let t = t.split("::").next().unwrap_or("");
    let t = t.split('{').next().unwrap_or("").trim();
    if t == "OutMsg" {
        "GwCmd".to_string()
    } else {
        t.to_string()
    }
}

/// Rows of the first table whose header starts with `环` and has an element column (`元素…`),
/// searched from the first line containing `after` (or from the top); code blocks are skipped.
fn ring_table(text: &str, after: Option<&str>) -> Option<Vec<RingRow>> {
    let lines: Vec<&str> = text.lines().collect();
    let start = match after {
        Some(a) => lines.iter().position(|l| l.contains(a))?,
        None => 0,
    };
    let mut in_code = false;
    for i in start..lines.len() {
        let l = lines[i].trim_start();
        if l.starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code || !l.starts_with('|') {
            continue;
        }
        let hdr = md_cells(l);
        let Some(ec) = hdr.iter().position(|c| c.starts_with("元素")) else {
            continue;
        };
        if hdr.first().is_none_or(|c| c != "环") {
            continue;
        }
        let mut rows: Vec<RingRow> = Vec::new();
        for (j, row) in lines.iter().enumerate().skip(i + 2) {
            if !row.trim_start().starts_with('|') {
                break;
            }
            let cells = md_cells(row);
            let cell = cells.get(ec).cloned().unwrap_or_default();
            let elem = first_type(&cell);
            let companions: Vec<String> = BACKTICK
                .captures_iter(&cell)
                .map(|c| c[1].to_string())
                .filter(|t| t.ends_with("-bulk") && RING_NAME.is_match(t))
                .collect();
            for c in BACKTICK.captures_iter(cells.first().map_or("", |c| c.as_str())) {
                let ring = c[1].replace("<v>", "<venue>");
                if RING_NAME.is_match(&ring) {
                    rows.push(RingRow {
                        ring,
                        elem: elem.clone(),
                        companions: companions.clone(),
                        line: j + 1,
                    });
                }
            }
        }
        // "同 `<ring>`": the element type of that ring.
        let named = rows.clone();
        for r in rows.iter_mut() {
            if let Some(o) = named.iter().find(|o| o.ring == r.elem) {
                r.elem = o.elem.clone();
            }
        }
        return Some(rows);
    }
    None
}

/// `(hft-event 环拓扑 rows, hft-sequencer ring-table rows)` of the tree at `root`.
pub fn ring_tables(root: &Path) -> Result<(Vec<RingRow>, Vec<RingRow>), String> {
    let read = |p: &str| std::fs::read_to_string(root.join(p)).map_err(|e| format!("{p}: {e}"));
    let ev = read(RING_EVENT_MD)?;
    let sq = read(RING_SEQ_MD)?;
    let ev_rows = ring_table(&ev, Some("**环拓扑**"))
        .ok_or_else(|| format!("{RING_EVENT_MD}: 环拓扑 table (| 环 | … | 元素类型 |) not found"))?;
    let sq_rows = ring_table(&sq, None)
        .ok_or_else(|| format!("{RING_SEQ_MD}: ring table (| 环 | … | 元素 | …) not found"))?;
    Ok((ev_rows, sq_rows))
}

/// "环名" check (tools/README.md doc-check table): every non-optional ring of either table appears in
/// the other with the same element type. Returns the problems.
pub fn check_rings(root: &Path) -> Vec<String> {
    let (ev, sq) = match ring_tables(root) {
        Ok(x) => x,
        Err(e) => return vec![e],
    };
    let mut out = Vec::new();
    let keep = |r: &&RingRow| !RING_OPTIONAL.contains(&r.ring.as_str());
    for e in ev.iter().filter(keep) {
        match sq.iter().find(|s| s.ring == e.ring) {
            None => out.push(format!(
                "{RING_EVENT_MD}:{}: ring `{}` is in the hft-event 环拓扑 table but not in the hft-sequencer ring table",
                e.line, e.ring
            )),
            Some(s) if s.elem != e.elem => out.push(format!(
                "{RING_SEQ_MD}:{}: ring `{}` element type `{}` differs from `{}` in the hft-event 环拓扑 table ({RING_EVENT_MD}:{})",
                s.line, e.ring, s.elem, e.elem, e.line
            )),
            Some(_) => {}
        }
    }
    for s in sq.iter().filter(keep) {
        if !ev.iter().any(|e| e.ring == s.ring) {
            out.push(format!(
                "{RING_SEQ_MD}:{}: ring `{}` is in the hft-sequencer ring table but not in the hft-event 环拓扑 table",
                s.line, s.ring
            ));
        }
    }
    out
}

/// All native checks: `(check name, problems)`.
pub fn native_checks(cx: &native::Ctx) -> Vec<(&'static str, Vec<native::Problem>)> {
    let rings = check_rings(&cx.root)
        .into_iter()
        .map(|p| ("RINGNAME", p))
        .collect();
    let perm_table = perms::check(&cx.root)
        .into_iter()
        .map(|p| ("PERMTABLE", p))
        .collect();
    vec![
        ("links", native::check_links(cx)),
        ("readme-structure", native::check_structure(cx)),
        ("budget", budget::check_budgets(cx)),
        ("segment-names", native::check_segments(cx)),
        ("event-producers", native::check_producers(cx)),
        ("event-roles", native::check_kind_roles(cx)),
        ("note-timer-names", native::check_note_names(cx)),
        ("codes-and-layout", layout::check_layout(cx)),
        ("dep-graph", native::check_dep_graph(cx)),
        ("rings", rings),
        ("perm-table", perm_table),
        ("venue-readme", budget::check_venues(cx)),
        ("runbooks", native::check_runbooks(cx)),
        ("scope-table", native::check_scope_table(cx)),
        ("name-rules", native::check_rules(cx)),
        ("agent-notes", native::check_notes(cx)),
        ("length", native::check_length(cx)),
    ]
}

/// Entry point: returns the exit code.
pub fn run(root: &Path) -> i32 {
    let cfg = match config::Config::load(root) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("doc-check: error: {e}");
            return EXIT_ENV;
        }
    };
    let scripts = cfg.scripts.clone();
    let cx = native::Ctx::load(root, cfg);
    let mut code = EXIT_OK;
    let mut native_json = Vec::new();
    let mut total = 0;
    for (name, probs) in native_checks(&cx) {
        for (cat, msg) in &probs {
            eprintln!("[{cat}] {msg}");
        }
        total += probs.len();
        native_json.push(json!({ "check": name, "problems": probs.iter().map(|(c, m)| format!("[{c}] {m}")).collect::<Vec<_>>() }));
    }
    eprintln!(
        "doc-check: native checks: files={} problems={total}",
        cx.docs.len()
    );
    if total > 0 {
        code = EXIT_FAIL;
    }
    let mut checks = Vec::new();
    if !scripts.is_empty() {
        let Some(py) = find_python() else {
            eprintln!("doc-check: error: no Python 3 interpreter (python / python3) on PATH");
            return EXIT_ENV;
        };
        for script in &scripts {
            let path = root.join(script);
            if !path.is_file() {
                eprintln!("doc-check: error: {script} not found");
                return EXIT_ENV;
            }
            let out = match Command::new(&py)
                .arg(&path)
                .arg("--base")
                .arg(root)
                .env("PYTHONIOENCODING", "utf-8")
                .current_dir(root)
                .output()
            {
                Ok(o) => o,
                Err(e) => {
                    eprintln!("doc-check: error: run {script}: {e}");
                    return EXIT_ENV;
                }
            };
            let stdout = String::from_utf8_lossy(&out.stdout);
            eprint!("{stdout}");
            eprint!("{}", String::from_utf8_lossy(&out.stderr));
            let problems = PROBLEMS
                .captures_iter(&stdout)
                .last()
                .and_then(|c| c[1].parse::<u64>().ok());
            let status = out.status.code().unwrap_or(-1);
            eprintln!(
                "doc-check: {script}: exit {status}, problems={}",
                problems.map_or("?".to_string(), |n| n.to_string())
            );
            checks.push(json!({ "script": script, "exit": status, "problems": problems }));
            match problems {
                None => code = EXIT_ENV,
                Some(n) if n > 0 || status != 0 => code = code.max(EXIT_FAIL),
                Some(_) => {}
            }
        }
    }
    if let Err(e) = write_json(
        root,
        "doc-check",
        &json!({ "checks": checks, "native": native_json }),
    ) {
        eprintln!("doc-check: error: {e}");
        return EXIT_ENV;
    }
    eprintln!("doc-check: {}", if code == EXIT_OK { "PASS" } else { "FAIL" });
    code
}
