//! `cargo xtask doc-check`: documentation gates (tools/README.md "cargo xtask doc-check").
//!
//! For now a wrapper around the Python checkers `tools/doccheck/check_hft.py` (links, README structure,
//! names, residue, layout, length, …) and `tools/doccheck/check_tests.py` (test-spec format and coverage).
//! Each prints `problems=<n>` as its last summary line; any non-zero count fails the gate.
//! The pure helpers [`slug`], [`slugs`] and [`doc_length`] are the native implementations of the anchor
//! and length rules shared with `check_hft.py` (same definitions).

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::json;

use crate::{EXIT_ENV, EXIT_FAIL, EXIT_OK, write_json};

/// The Python checkers run by `doc-check`, relative to the repo root.
pub const SCRIPTS: &[&str] = &["tools/doccheck/check_hft.py", "tools/doccheck/check_tests.py"];

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

/// Entry point: returns the exit code.
pub fn run(root: &Path) -> i32 {
    let Some(py) = find_python() else {
        eprintln!("doc-check: error: no Python 3 interpreter (python / python3) on PATH");
        return EXIT_ENV;
    };
    let mut checks = Vec::new();
    let mut code = EXIT_OK;
    for script in SCRIPTS {
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
    if let Err(e) = write_json(root, "doc-check", &json!({ "checks": checks })) {
        eprintln!("doc-check: error: {e}");
        return EXIT_ENV;
    }
    eprintln!("doc-check: {}", if code == EXIT_OK { "PASS" } else { "FAIL" });
    code
}
