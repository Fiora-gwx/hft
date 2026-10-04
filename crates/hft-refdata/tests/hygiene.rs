//! REF-088: dependency closure and purity of `src/`.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use std::path::{Path, PathBuf};
use std::process::Command;

const ALLOWED: [&str; 4] = ["hft-types", "zerocopy", "sha2", "rustc-hash"];
const BANNED_DEPS: [&str; 4] = ["venue-", "hft-strategy-sdk", "serde_json", "tokio"];
const BANNED_TEXT: [&str; 6] = [
    "std::fs",
    "std::net",
    "std::thread",
    "std::time",
    "HashMap",
    "AtomicU",
];

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("src dir").flatten() {
        let p = entry.path();
        if p.is_dir() {
            rs_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// Library text of a source file: everything before a `#[cfg(test)]` test module.
fn library_text(src: &str) -> &str {
    src.find("#[cfg(test)]\nmod tests").map_or(src, |i| &src[..i])
}

fn has_word(text: &str, word: &str) -> bool {
    let b = text.as_bytes();
    let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    text.match_indices(word).any(|(i, _)| {
        let before = i.checked_sub(1).and_then(|j| b.get(j)).is_some_and(|c| ident(*c));
        let after = b.get(i + word.len()).is_some_and(|c| ident(*c));
        !before && !after
    })
}

#[test]
fn ref_088_pure_dependencies() {
    let out = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--no-deps", "--offline"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo metadata");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let pkg = meta["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| p["name"] == "hft-refdata")
        .expect("package");
    let normal: Vec<&str> = pkg["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| d["kind"].is_null() && d["optional"] != true)
        .filter_map(|d| d["name"].as_str())
        .collect();
    assert!(!normal.is_empty());
    for d in &normal {
        assert!(ALLOWED.contains(d), "unexpected dependency {d}");
        assert!(
            !BANNED_DEPS.iter().any(|b| d.starts_with(b)),
            "banned dependency {d}"
        );
    }

    let mut files = Vec::new();
    rs_files(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
    assert!(!files.is_empty());
    for f in files {
        let src = std::fs::read_to_string(&f).expect("read").replace("\r\n", "\n");
        let text = library_text(&src);
        for w in BANNED_TEXT {
            assert!(!text.contains(w), "{} contains {w}", f.display());
        }
        assert!(!has_word(text, "rand"), "{} uses rand", f.display());
    }
}
