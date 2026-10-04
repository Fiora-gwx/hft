//! `unsafe` only in `src/layout.rs`, each with a `// SAFETY:` comment (JNL-103).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use std::path::{Path, PathBuf};

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            rs_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// Source with comments and string/char literals blanked (line structure kept).
fn strip(src: &str) -> String {
    let b = src.as_bytes();
    let mut o = String::with_capacity(src.len());
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                if b[i] == b'\n' {
                    o.push('\n');
                }
                i += 1;
            }
            i += 2;
        } else if c == b'"' {
            i += 1;
            while i < b.len() && b[i] != b'"' {
                if b[i] == b'\\' {
                    i += 1;
                }
                if b.get(i) == Some(&b'\n') {
                    o.push('\n');
                }
                i += 1;
            }
            i += 1;
            o.push_str("\"\"");
        } else if c == b'\'' && b.get(i + 2) == Some(&b'\'') {
            i += 3;
            o.push_str("' '");
        } else {
            o.push(c as char);
            i += 1;
        }
    }
    o
}

#[test]
fn jnl_103_unsafe_confined() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rs_files(&src, &mut files);
    let mut found = 0;
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap();
        let orig: Vec<&str> = text.lines().collect();
        let stripped = strip(&text);
        for (n, line) in stripped.lines().enumerate() {
            let has = line
                .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .any(|w| w == "unsafe");
            if !has {
                continue;
            }
            found += 1;
            assert!(
                f.ends_with("layout.rs"),
                "unsafe outside layout.rs: {}:{}",
                f.display(),
                n + 1
            );
            let lo = n.saturating_sub(3);
            let ok = orig[lo..=n].iter().any(|l| l.contains("// SAFETY:"));
            assert!(ok, "no SAFETY comment within 3 lines: {}:{}", f.display(), n + 1);
        }
    }
    assert!(found > 0);
}
