//! `cargo xtask fixtures {fetch, check, scrub}` (tools/README.md "其他 xtask 子命令";
//! rules: fixtures/README.md). `extract`, `corpus` and `bbo-diff` arrive with their milestones (exit 2).
//!
//! `check` scans every file under `fixtures/` (Markdown and `fixtures/scrub-rules.toml` excluded) for:
//! login/signature field names (`"signature":`, `"apiKey":`, `"passphrase":`, `"listenKey":` and the same
//! names as query parameters), 64-hex-digit secrets (except on lines naming a hash: `sha256`, `hash`,
//! `digest`), `-----BEGIN`, base64 Ed25519 keys (a quoted 44-character base64 string of 32 bytes, or the
//! PKCS#8 Ed25519 prefix `MC4CAQAwBQYDK2Vw`) and the real account ids registered as SHA-256 in
//! `scrub-rules.toml` (`[forbid] sha256 = [...]`). `scrub` removes the login/signature fields and
//! applies the `[[replace]]` rules (`pattern` regex → `with`) to UTF-8 files in place.

pub(crate) mod bundle;
pub mod fetch;
pub(crate) mod pointer;

use std::path::Path;
use std::sync::LazyLock;

use regex::{Captures, Regex};
use serde_json::json;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::{EXIT_ENV, EXIT_FAIL, EXIT_OK, EnvError, rel_path, write_json};

pub const RULES: &str = "fixtures/scrub-rules.toml";
const FIELDS: &str = "signature|apiKey|passphrase|listenKey";

static JSON_FIELD: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!(r#""(?:{FIELDS})"\s*:"#)).unwrap());
static QUERY_FIELD: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!(r"[?&](?:{FIELDS})=")).unwrap());
static HEX64: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[0-9A-Fa-f]{64,}").unwrap());
static HASH_LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)sha256|hash|digest").unwrap());
static B64_KEY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""([A-Za-z0-9+/]{43}=)""#).unwrap());
static TOKEN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[A-Za-z0-9_.@:-]{3,}").unwrap());
static DROP_JSON: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r#"\s*"(?:{FIELDS})"\s*:\s*(?:"(?:[^"\\]|\\.)*"|[^,}}\]\s]+)\s*(,?)"#
    ))
    .unwrap()
});
static DANGLING_COMMA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r",(\s*[}\]])").unwrap());
static DROP_QUERY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r#"([?&])(?:{FIELDS})=[^&\s"]*(&?)"#)).unwrap());

/// One finding of `check`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub file: String,
    pub offset: usize,
    pub what: String,
}

#[derive(Debug, Default)]
struct Rules {
    forbid_sha: Vec<String>,
    replace: Vec<(Regex, String)>,
}

fn load_rules(root: &Path) -> Result<Rules, EnvError> {
    let p = root.join(RULES);
    if !p.exists() {
        return Ok(Rules::default());
    }
    let text = std::fs::read_to_string(&p).map_err(|e| EnvError(format!("read {RULES}: {e}")))?;
    let t: toml::Table = text
        .parse()
        .map_err(|e| EnvError(format!("parse {RULES}: {e}")))?;
    let mut r = Rules::default();
    if let Some(a) = t
        .get("forbid")
        .and_then(|f| f.get("sha256"))
        .and_then(|x| x.as_array())
    {
        r.forbid_sha = a
            .iter()
            .filter_map(|x| x.as_str().map(|s| s.to_ascii_lowercase()))
            .collect();
    }
    for e in t
        .get("replace")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default()
    {
        let pat = e.get("pattern").and_then(|x| x.as_str()).unwrap_or("");
        let with = e.get("with").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let re = Regex::new(pat).map_err(|x| EnvError(format!("{RULES}: bad pattern {pat:?}: {x}")))?;
        r.replace.push((re, with));
    }
    Ok(r)
}

fn files(root: &Path) -> Vec<String> {
    let base = root.join("fixtures");
    let mut out = Vec::new();
    for e in WalkDir::new(&base).sort_by_file_name().into_iter().flatten() {
        if !e.file_type().is_file() {
            continue;
        }
        let rel = rel_path(root, e.path());
        if rel == RULES || rel.ends_with(".md") {
            continue;
        }
        out.push(rel);
    }
    out
}

/// Scans one file's bytes.
pub fn scan_bytes(file: &str, bytes: &[u8], forbid_sha: &[String]) -> Vec<Finding> {
    let text = String::from_utf8_lossy(bytes);
    let mut out = Vec::new();
    let mut add = |offset: usize, what: &str| {
        out.push(Finding {
            file: file.to_string(),
            offset,
            what: what.to_string(),
        })
    };
    for m in JSON_FIELD.find_iter(&text) {
        add(m.start(), "login/signature field");
    }
    for m in QUERY_FIELD.find_iter(&text) {
        add(m.start() + 1, "login/signature query parameter");
    }
    for m in HEX64.find_iter(&text) {
        if m.as_str().len() != 64 {
            continue;
        }
        let ls = text[..m.start()].rfind('\n').map_or(0, |i| i + 1);
        let le = text[m.end()..].find('\n').map_or(text.len(), |i| m.end() + i);
        if !HASH_LINE.is_match(&text[ls..le]) {
            add(m.start(), "64-hex-digit secret");
        }
    }
    for (i, _) in text.match_indices("-----BEGIN") {
        add(i, "PEM block");
    }
    for c in B64_KEY.captures_iter(&text) {
        if let Some(g) = c.get(1) {
            add(g.start(), "base64 Ed25519 key");
        }
    }
    for (i, _) in text.match_indices("MC4CAQAwBQYDK2Vw") {
        add(i, "PKCS#8 Ed25519 key");
    }
    if !forbid_sha.is_empty() {
        for m in TOKEN.find_iter(&text) {
            let h = hex(&Sha256::digest(m.as_str().as_bytes()));
            if forbid_sha.contains(&h) {
                add(m.start(), "registered account id");
            }
        }
    }
    out.sort_by_key(|f| f.offset);
    out
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// `fixtures check`.
pub fn check(root: &Path) -> Result<Vec<Finding>, EnvError> {
    let rules = load_rules(root)?;
    let mut out = Vec::new();
    for f in files(root) {
        let b = std::fs::read(root.join(&f)).map_err(|e| EnvError(format!("read {f}: {e}")))?;
        out.extend(scan_bytes(&f, &b, &rules.forbid_sha));
    }
    Ok(out)
}

/// Scrubbed text (login/signature fields removed, `[[replace]]` rules applied).
fn scrub_text(s: &str, rules: &Rules) -> String {
    let mut t = DROP_JSON.replace_all(s, "").into_owned();
    t = DANGLING_COMMA.replace_all(&t, "$1").into_owned();
    t = t.replace("{,", "{");
    t = DROP_QUERY
        .replace_all(&t, |c: &Captures| {
            if &c[2] == "&" {
                c[1].to_string()
            } else {
                String::new()
            }
        })
        .into_owned();
    for (re, with) in &rules.replace {
        t = re.replace_all(&t, with.as_str()).into_owned();
    }
    t
}

/// `fixtures scrub [<path>…]`: returns the files changed.
pub fn scrub(root: &Path, paths: &[String]) -> Result<Vec<String>, EnvError> {
    let rules = load_rules(root)?;
    let targets: Vec<String> = if paths.is_empty() {
        files(root)
    } else {
        paths.to_vec()
    };
    let mut changed = Vec::new();
    for f in targets {
        let p = root.join(&f);
        let Ok(text) = std::fs::read_to_string(&p) else {
            continue;
        };
        let t = scrub_text(&text, &rules);
        if t != text {
            std::fs::write(&p, t).map_err(|e| EnvError(format!("write {f}: {e}")))?;
            changed.push(f);
        }
    }
    Ok(changed)
}

/// Entry point.
pub fn run(root: &Path, args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        Some("fetch") if args.len() == 1 => match crate::exec::from_env(root) {
            Ok(mut exec) => fetch::run_with(root, exec.as_mut()),
            Err(e) => {
                eprintln!("fixtures fetch: {e}");
                EXIT_ENV
            }
        },
        Some("check") if args.len() == 1 => {
            let fs = match check(root) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("fixtures check: error: {e}");
                    return EXIT_ENV;
                }
            };
            for f in &fs {
                eprintln!("{} @ {}: {}", f.file, f.offset, f.what);
            }
            let j = json!({
                "findings": fs.iter().map(|f| json!({"file": f.file, "offset": f.offset, "what": f.what})).collect::<Vec<_>>(),
            });
            if let Err(e) = write_json(root, "fixtures", &j) {
                eprintln!("fixtures check: error: {e}");
                return EXIT_ENV;
            }
            eprintln!(
                "fixtures check: findings={} -> {}",
                fs.len(),
                if fs.is_empty() { "PASS" } else { "FAIL" }
            );
            if fs.is_empty() { EXIT_OK } else { EXIT_FAIL }
        }
        Some("scrub") => match scrub(root, &args[1..]) {
            Ok(ch) => {
                for f in &ch {
                    eprintln!("fixtures scrub: rewrote {f}");
                }
                eprintln!("fixtures scrub: {} file(s) changed", ch.len());
                EXIT_OK
            }
            Err(e) => {
                eprintln!("fixtures scrub: error: {e}");
                EXIT_ENV
            }
        },
        Some(s @ ("extract" | "corpus" | "bbo-diff")) => {
            eprintln!(
                "fixtures {s}: not implemented yet (arrives with its milestone, tools/README.md 已知限制)"
            );
            EXIT_ENV
        }
        _ => {
            eprintln!("usage: cargo xtask fixtures fetch | check | scrub [<path>…]");
            EXIT_ENV
        }
    }
}
