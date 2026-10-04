//! Source scanner for `test-trace`: test functions, DST scenario registrations and Criterion ids.
//!
//! Rust files are lexed just enough to drop comments and string contents (so commented-out code and
//! code inside string literals never count), then function items, their outer attributes, `proptest!`
//! blocks, `Scenario::new("<id>", …)` calls and (in bench files) string literals are collected.
//! Python test files (`test_*.py`, `*_test.py`) contribute `def test_*` functions.

use std::path::Path;
use std::sync::LazyLock;

use crate::spec::walk_repo;
use crate::{EnvError, rel_path};
use regex::Regex;

static RUST_FN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)").unwrap());
static PROPTEST: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bproptest\s*!\s*[\{\(\[]").unwrap());
static SCENARIO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"\bScenario\s*::\s*new\s*\(\s*""#).unwrap());
static PY_DEF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?:async\s+)?def\s+([A-Za-z_][A-Za-z0-9_]*)").unwrap());

/// A function item found in source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnDef {
    /// File relative to the repo root.
    pub file: String,
    pub name: String,
    pub line: usize,
    /// Rust: has a test attribute (`#[test]`, `#[rstest]`, `#[tokio::test]`, …) or sits in `proptest!`;
    /// Python: `def test_*` in a pytest file.
    pub is_test: bool,
    /// Rust `#[ignore]`; Python `@pytest.mark.skip*` / `@unittest.skip*`.
    pub ignored: bool,
}

/// A string literal of interest (scenario id or Criterion id) and the function that contains it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Literal {
    pub file: String,
    pub value: String,
    pub line: usize,
    pub enclosing: Option<String>,
}

/// Everything `test-trace` needs from the code.
#[derive(Debug, Default)]
pub struct CodeIndex {
    pub fns: Vec<FnDef>,
    /// `Scenario::new("<id>", …)` registrations.
    pub scenarios: Vec<Literal>,
    /// String literals in bench files (candidate Criterion ids).
    pub bench_literals: Vec<Literal>,
}

/// Bench sources: any path with a `benches` component, or under `benchmarks/`.
pub fn is_bench_file(rel: &str) -> bool {
    rel.starts_with("benchmarks/") || rel.split('/').any(|c| c == "benches")
}

/// Fuzz-target sources: any path with a `fuzz` component.
pub fn is_fuzz_file(rel: &str) -> bool {
    rel.split('/').any(|c| c == "fuzz")
}

fn is_python_test_file(name: &str) -> bool {
    name.ends_with(".py") && (name.starts_with("test_") || name.ends_with("_test.py"))
}

/// Scan all `.rs` files and pytest files under `root`.
pub fn scan_repo(root: &Path) -> Result<CodeIndex, EnvError> {
    let mut idx = CodeIndex::default();
    for e in walk_repo(root) {
        let e = e.map_err(|e| EnvError(format!("walk {}: {e}", root.display())))?;
        if !e.file_type().is_file() {
            continue;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        let is_rs = name.ends_with(".rs");
        if !is_rs && !is_python_test_file(&name) {
            continue;
        }
        let Ok(bytes) = std::fs::read(e.path()) else {
            continue;
        };
        let src = String::from_utf8_lossy(&bytes);
        let rel = rel_path(root, e.path());
        if is_rs {
            scan_rust(&rel, &src, &mut idx);
        } else {
            scan_python(&rel, &src, &mut idx);
        }
    }
    Ok(idx)
}

fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c >= 0x80
}

fn utf8_len(first: u8) -> usize {
    match first {
        0xF0..=0xFF => 4,
        0xE0..=0xEF => 3,
        0xC0..=0xDF => 2,
        _ => 1,
    }
}

/// Blank comments and string/char literal contents (keeping byte offsets and newlines).
/// Returns the blanked text and the byte ranges of string-literal contents in the original.
pub fn blank_rust(src: &str) -> (String, Vec<(usize, usize)>) {
    let b = src.as_bytes();
    let n = b.len();
    let mut out = b.to_vec();
    let mut lits = Vec::new();
    let blank = |out: &mut Vec<u8>, a: usize, z: usize| {
        for c in out.iter_mut().take(z.min(n)).skip(a) {
            if *c != b'\n' {
                *c = b' ';
            }
        }
    };
    let at = |i: usize| if i < n { b[i] } else { 0 };
    let mut i = 0;
    while i < n {
        let c = b[i];
        if c == b'/' && at(i + 1) == b'/' {
            let j = b[i..].iter().position(|&x| x == b'\n').map_or(n, |p| i + p);
            blank(&mut out, i, j);
            i = j;
        } else if c == b'/' && at(i + 1) == b'*' {
            let (mut depth, mut j) = (1, i + 2);
            while j < n && depth > 0 {
                if b[j] == b'/' && at(j + 1) == b'*' {
                    depth += 1;
                    j += 2;
                } else if b[j] == b'*' && at(j + 1) == b'/' {
                    depth -= 1;
                    j += 2;
                } else {
                    j += 1;
                }
            }
            blank(&mut out, i, j);
            i = j;
        } else if c == b'"' {
            let mut j = i + 1;
            while j < n && b[j] != b'"' {
                j += if b[j] == b'\\' { 2 } else { 1 };
            }
            let j = j.min(n);
            lits.push((i + 1, j));
            blank(&mut out, i + 1, j);
            i = j + 1;
        } else if c == b'\'' {
            if at(i + 1) == b'\\' {
                let mut j = i + 3;
                while j < n && b[j] != b'\'' && b[j] != b'\n' {
                    j += 1;
                }
                blank(&mut out, i + 1, j);
                i = j + 1;
            } else if i + 1 < n {
                let len = utf8_len(b[i + 1]);
                if at(i + 1 + len) == b'\'' && b[i + 1] != b'\'' {
                    blank(&mut out, i + 1, i + 1 + len);
                    i += 2 + len;
                } else {
                    i += 1; // lifetime or label
                }
            } else {
                i += 1;
            }
        } else if (c.is_ascii_alphabetic() || c == b'_') && (i == 0 || !is_ident(b[i - 1])) {
            let mut j = i;
            while j < n && is_ident(b[j]) {
                j += 1;
            }
            let ident = &b[i..j];
            if matches!(ident, b"r" | b"br" | b"cr") && (at(j) == b'"' || at(j) == b'#') {
                let hashes = b[j..].iter().take_while(|&&x| x == b'#').count();
                if at(j + hashes) == b'"' {
                    let start = j + hashes + 1;
                    let mut k = start;
                    let end = loop {
                        if k >= n {
                            break n;
                        }
                        if b[k] == b'"'
                            && b[k + 1..].iter().take(hashes).filter(|&&x| x == b'#').count() == hashes
                        {
                            break k;
                        }
                        k += 1;
                    };
                    lits.push((start, end));
                    blank(&mut out, start, end);
                    i = (end + 1 + hashes).min(n);
                    continue;
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }
    (String::from_utf8_lossy(&out).into_owned(), lits)
}

fn line_of(starts: &[usize], pos: usize) -> usize {
    match starts.binary_search(&pos) {
        Ok(i) => i + 1,
        Err(i) => i,
    }
}

fn line_starts(s: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(s.match_indices('\n').map(|(i, _)| i + 1))
        .collect()
}

/// Index of the delimiter matching the opener at `open` (forward), if any.
fn match_forward(b: &[u8], open: usize) -> Option<usize> {
    let (o, c) = match b[open] {
        b'{' => (b'{', b'}'),
        b'(' => (b'(', b')'),
        _ => (b'[', b']'),
    };
    let mut depth = 0usize;
    for (k, &x) in b.iter().enumerate().skip(open) {
        if x == o {
            depth += 1;
        } else if x == c {
            depth -= 1;
            if depth == 0 {
                return Some(k);
            }
        }
    }
    None
}

/// Index of the opener matching the closer at `close` (backward), if any.
fn match_backward(b: &[u8], close: usize, o: u8, c: u8) -> Option<usize> {
    let mut depth = 0usize;
    for k in (0..=close).rev() {
        if b[k] == c {
            depth += 1;
        } else if b[k] == o {
            depth -= 1;
            if depth == 0 {
                return Some(k);
            }
        }
    }
    None
}

/// Outer attributes (text between `#[` and `]`) written before the item starting at `pos`.
fn attrs_before(b: &[u8], pos: usize) -> Vec<String> {
    const KW: &[&[u8]] = &[b"pub", b"async", b"unsafe", b"const", b"extern", b"default"];
    let mut attrs = Vec::new();
    let mut k = pos;
    'outer: loop {
        while k > 0 && b[k - 1].is_ascii_whitespace() {
            k -= 1;
        }
        if k == 0 {
            break;
        }
        for kw in KW {
            if k >= kw.len()
                && &b[k - kw.len()..k] == *kw
                && (k == kw.len() || !is_ident(b[k - kw.len() - 1]))
            {
                k -= kw.len();
                continue 'outer;
            }
        }
        match b[k - 1] {
            b')' => {
                // `pub(crate)` / `pub(in path)`
                let Some(o) = match_backward(b, k - 1, b'(', b')') else {
                    break;
                };
                let mut q = o;
                while q > 0 && b[q - 1].is_ascii_whitespace() {
                    q -= 1;
                }
                if q >= 3 && &b[q - 3..q] == b"pub" {
                    k = q - 3;
                } else {
                    break;
                }
            }
            b'"' => {
                // ABI string of `extern "C"` (contents already blanked)
                let Some(o) = b[..k - 1].iter().rposition(|&x| x == b'"') else {
                    break;
                };
                k = o;
            }
            b']' => {
                let Some(o) = match_backward(b, k - 1, b'[', b']') else {
                    break;
                };
                if o > 0 && b[o - 1] == b'#' {
                    attrs.push(String::from_utf8_lossy(&b[o + 1..k - 1]).trim().to_string());
                    k = o - 1;
                } else {
                    break;
                }
            }
            _ => break,
        }
    }
    attrs
}

fn attr_path(attr: &str) -> &str {
    let end = attr
        .find(|c: char| c == '(' || c == '=' || c == ']' || c.is_whitespace())
        .unwrap_or(attr.len());
    &attr[..end]
}

fn is_test_attr(attr: &str) -> bool {
    let p = attr_path(attr);
    matches!(p, "test" | "rstest" | "test_case" | "wasm_bindgen_test") || p.ends_with("::test")
}

/// Scan one Rust source file.
pub fn scan_rust(file: &str, src: &str, idx: &mut CodeIndex) {
    let (blanked, lits) = blank_rust(src);
    let b = blanked.as_bytes();
    let starts = line_starts(src);
    let prop_ranges: Vec<(usize, usize)> = PROPTEST
        .find_iter(&blanked)
        .filter_map(|m| match_forward(b, m.end() - 1).map(|e| (m.start(), e)))
        .collect();
    let mut fn_pos: Vec<(usize, String)> = Vec::new();
    for c in RUST_FN.captures_iter(&blanked) {
        let m = c.get(0).unwrap();
        let name = c[1].to_string();
        let attrs = attrs_before(b, m.start());
        let in_prop = prop_ranges.iter().any(|&(a, z)| m.start() > a && m.start() < z);
        idx.fns.push(FnDef {
            file: file.to_string(),
            name: name.clone(),
            line: line_of(&starts, m.start()),
            is_test: in_prop || attrs.iter().any(|a| is_test_attr(a)),
            ignored: attrs.iter().any(|a| attr_path(a) == "ignore"),
        });
        fn_pos.push((m.start(), name));
    }
    let enclosing = |pos: usize| {
        fn_pos
            .iter()
            .rev()
            .find(|(p, _)| *p < pos)
            .map(|(_, n)| n.clone())
    };
    let lit_value = |start: usize| -> Option<String> {
        lits.iter()
            .find(|(a, _)| *a == start)
            .map(|&(a, z)| src[a..z].to_string())
    };
    for m in SCENARIO.find_iter(&blanked) {
        if let Some(v) = lit_value(m.end()) {
            idx.scenarios.push(Literal {
                file: file.to_string(),
                value: v,
                line: line_of(&starts, m.start()),
                enclosing: enclosing(m.start()),
            });
        }
    }
    if is_bench_file(file) {
        for &(a, z) in &lits {
            idx.bench_literals.push(Literal {
                file: file.to_string(),
                value: src[a..z].to_string(),
                line: line_of(&starts, a),
                enclosing: enclosing(a),
            });
        }
    }
}

/// Scan one pytest file.
pub fn scan_python(file: &str, src: &str, idx: &mut CodeIndex) {
    let lines: Vec<&str> = src.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let Some(c) = PY_DEF.captures(line) else {
            continue;
        };
        let name = c[1].to_string();
        let mut ignored = false;
        for prev in lines[..i].iter().rev() {
            let t = prev.trim();
            if !t.starts_with('@') {
                break;
            }
            if t.contains(".skip") {
                ignored = true;
            }
        }
        idx.fns.push(FnDef {
            file: file.to_string(),
            is_test: name.starts_with("test_"),
            name,
            line: i + 1,
            ignored,
        });
    }
}
