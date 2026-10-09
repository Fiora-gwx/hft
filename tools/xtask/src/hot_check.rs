//! `cargo xtask hot-check` (tools/README.md "cargo xtask hot-check"; rules: docs/hot-path-rules.md).
//!
//! Scope (tools/README.md "hot-check 口径"): library crates are the packages under `crates/` and
//! `strategies/` (directories starting with `_` excluded); their tier comes from `tools/xtask/deps.toml`.
//! Source rules read `<crate>/src/**/*.rs` and skip `#[cfg(test)]` code; the `#[allow]` rule reads every
//! `.rs` file of a hot/pure crate (tests, benches, examples and build scripts too). A strategy crate's
//! fast part is `src/` without `src/model/`. Violations go to stderr and `target/xtask/hot-check.json`.

pub mod scan;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::json;
use syn::Type;
use walkdir::WalkDir;

use crate::deps;
use crate::{EXIT_ENV, EXIT_FAIL, EXIT_OK, EnvError, rel_path, write_json};
use scan::{FileScan, StructDef};

pub const ALLOW_LIST: &str = "tools/xtask/allow-list.toml";
/// Crates allowed to contain `unsafe` (B5).
pub const UNSAFE_CRATES: &[&str] = &[
    "crates/hft-rt",
    "crates/hft-journal",
    "crates/hft-net",
    "crates/hft-book",
];
/// Crates whose types must not contain `f32`/`f64` (N3); strategy fast parts are added separately.
pub const N3_CRATES: &[&str] = &[
    "crates/hft-types",
    "crates/hft-strategy-sdk",
    "crates/hft-oms",
    "crates/hft-risk",
    "crates/hft-exec",
    "crates/hft-strat-lib",
    "crates/venue-*",
    "crates/hft-ledger",
];
/// Modules exempt from N3 inside N3 crates (only `Model` code uses them).
pub const N3_EXEMPT: &[&str] = &[
    "crates/hft-strategy-sdk/src/model_math",
    "crates/hft-strat-lib/src/model",
];
/// Crates that may define `Atomic*` state (T3).
pub const ATOMIC_CRATES: &[&str] = &["crates/hft-rt", "crates/hft-journal"];
/// The only file allowed to call the rustls record encryption entry (T6).
pub const TLS_FILE: &str = "crates/hft-net/src/tls.rs";
/// `#[allow]` lints that must be registered in the allow-list.
pub const REGISTERED_LINTS: &[&str] = &[
    "clippy::disallowed_methods",
    "clippy::disallowed_types",
    "clippy::disallowed_macros",
    "clippy::unwrap_used",
    "clippy::expect_used",
    "clippy::panic",
    "clippy::unreachable",
    "clippy::todo",
    "clippy::unimplemented",
    "clippy::indexing_slicing",
];
/// Profiles compared with docs/development.md "构建剖面" (B1).
pub const PROFILES: &[&str] = &["release", "release-hot", "ci", "bench"];

/// One violation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Violation {
    /// Rule id of docs/hot-path-rules.md (`N6`, `B5`, …; `检查机制` for the lint configuration).
    pub rule: String,
    pub file: String,
    pub line: usize,
    pub detail: String,
}

fn v(rule: &str, file: &str, line: usize, detail: impl Into<String>) -> Violation {
    Violation {
        rule: rule.into(),
        file: file.into(),
        line,
        detail: detail.into(),
    }
}

fn read(root: &Path, rel: &str) -> Result<String, EnvError> {
    std::fs::read_to_string(root.join(rel)).map_err(|e| EnvError(format!("read {rel}: {e}")))
}

/// `pattern` with `*` (one path segment) and `**` (any number of segments) against `path`.
pub fn glob_match(pattern: &str, path: &str) -> bool {
    let mut re = String::from("^");
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' if chars.peek() == Some(&'*') => {
                chars.next();
                if chars.peek() == Some(&'/') {
                    chars.next();
                    re += "(?:.*/)?";
                } else {
                    re += ".*";
                }
            }
            '*' => re += "[^/]*",
            c => re += &regex::escape(&c.to_string()),
        }
    }
    re += "$";
    Regex::new(&re).is_ok_and(|r| r.is_match(path))
}

/// The registered exceptions (`tools/xtask/allow-list.toml`).
#[derive(Debug, Default)]
pub struct AllowList {
    /// `(file glob, lints)`.
    pub allow: Vec<(String, Vec<String>)>,
    /// `(row type, field)` TSC rows (D6).
    pub tsc: Vec<(String, String)>,
}

impl AllowList {
    pub fn load(root: &Path) -> Result<AllowList, EnvError> {
        let p = root.join(ALLOW_LIST);
        if !p.exists() {
            return Ok(AllowList::default());
        }
        let text = read(root, ALLOW_LIST)?;
        let t: toml::Table = text
            .parse()
            .map_err(|e| EnvError(format!("parse {ALLOW_LIST}: {e}")))?;
        let arr = |k: &str| t.get(k).and_then(|x| x.as_array()).cloned().unwrap_or_default();
        let s = |e: &toml::Value, k: &str| e.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
        let mut out = AllowList::default();
        for e in arr("allow") {
            let lints = e
                .get("lints")
                .and_then(|x| x.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            if s(&e, "reason").trim().is_empty() {
                return Err(EnvError(format!(
                    "{ALLOW_LIST}: entry for {} has no reason",
                    s(&e, "file")
                )));
            }
            out.allow.push((s(&e, "file"), lints));
        }
        for e in arr("tsc") {
            let class = e.get("class").and_then(|x| x.as_integer()).unwrap_or(0);
            if !(1..=3).contains(&class) || s(&e, "reason").trim().is_empty() {
                return Err(EnvError(format!(
                    "{ALLOW_LIST}: [[tsc]] {}.{} needs class 1–3 (D6) and a reason",
                    s(&e, "row"),
                    s(&e, "field")
                )));
            }
            out.tsc.push((s(&e, "row"), s(&e, "field")));
        }
        Ok(out)
    }

    fn allows(&self, file: &str, lint: &str) -> bool {
        self.allow
            .iter()
            .any(|(g, ls)| glob_match(g, file) && ls.iter().any(|l| l == lint))
    }
}

/// A library crate under `crates/` or `strategies/`.
#[derive(Debug, Clone)]
struct Crate {
    dir: String,
    tier: Option<String>,
}

fn lib_crates(root: &Path, cfg: &deps::Config) -> Vec<Crate> {
    let mut out = Vec::new();
    for base in ["crates", "strategies"] {
        let Ok(rd) = std::fs::read_dir(root.join(base)) else {
            continue;
        };
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if n.starts_with('_') || !e.path().join("Cargo.toml").is_file() {
                continue;
            }
            let dir = format!("{base}/{n}");
            let tier = cfg.rule_for(&dir).map(|r| r.tier.clone());
            out.push(Crate { dir, tier });
        }
    }
    out.sort_by(|a, b| a.dir.cmp(&b.dir));
    out
}

/// `.rs` files below `dir` (relative paths), not descending into nested packages (`fuzz/`) or `target/`.
fn rs_files(root: &Path, dir: &str) -> Vec<String> {
    let base = root.join(dir);
    if !base.is_dir() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut it = WalkDir::new(&base).sort_by_file_name().into_iter();
    while let Some(Ok(e)) = it.next() {
        let p = e.path();
        if e.file_type().is_dir() {
            let nested = p != base && p.join("Cargo.toml").is_file();
            let name = e.file_name().to_string_lossy();
            if nested || name == "target" || (p != base && name.starts_with('.')) {
                it.skip_current_dir();
            }
            continue;
        }
        if p.extension().is_some_and(|x| x == "rs") {
            out.push(rel_path(root, p));
        }
    }
    out
}

/// Parsed files of the scanned trees; `test_files` = files reached only through `#[cfg(test)] mod x;`.
struct Tree {
    files: BTreeMap<String, (String, FileScan)>,
    test_files: BTreeSet<String>,
}

fn load_tree(root: &Path, dirs: &[String], out: &mut Vec<Violation>) -> Tree {
    let mut files = BTreeMap::new();
    for d in dirs {
        for f in rs_files(root, d) {
            let Ok(src) = std::fs::read_to_string(root.join(&f)) else {
                continue;
            };
            match scan::scan(&src) {
                Ok(s) => {
                    files.insert(f, (src, s));
                }
                Err(e) => out.push(v("syntax", &f, 0, format!("does not parse: {e}"))),
            }
        }
    }
    let mut test_files = BTreeSet::new();
    for (f, (_, s)) in &files {
        let p = Path::new(f);
        let stem = p.file_stem().and_then(|x| x.to_str()).unwrap_or("");
        let parent = p
            .parent()
            .map(|x| x.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let mod_dir = if matches!(stem, "lib" | "main" | "mod") {
            parent
        } else {
            format!("{parent}/{stem}")
        };
        for m in &s.test_mods {
            test_files.insert(format!("{mod_dir}/{m}.rs"));
            test_files.insert(format!("{mod_dir}/{m}/mod.rs"));
        }
    }
    Tree { files, test_files }
}

fn under(file: &str, dir: &str) -> bool {
    file.starts_with(&format!("{dir}/"))
}

fn crate_matches(pats: &[&str], dir: &str) -> bool {
    pats.iter().any(|p| glob_match(p, dir))
}

// ------------------------------------------------------------------------------------------- layout

/// Size and alignment table for `#[repr(C)]` padding detection.
#[derive(Default)]
struct Types {
    structs: BTreeMap<String, StructDef>,
    enums: BTreeMap<String, u64>,
    consts: BTreeMap<String, u64>,
    aliases: BTreeMap<String, Type>,
}

fn prim(t: &str) -> Option<(u64, u64)> {
    Some(match t {
        "u8" | "i8" | "bool" | "AtomicU8" | "AtomicI8" | "AtomicBool" | "NonZeroU8" => (1, 1),
        "u16" | "i16" | "AtomicU16" | "AtomicI16" | "NonZeroU16" => (2, 2),
        "u32" | "i32" | "f32" | "char" | "AtomicU32" | "AtomicI32" | "NonZeroU32" => (4, 4),
        "u64" | "i64" | "f64" | "AtomicU64" | "AtomicI64" | "NonZeroU64" | "usize" | "isize"
        | "AtomicUsize" | "AtomicIsize" | "AtomicPtr" => (8, 8),
        "u128" | "i128" => (16, 16),
        "PhantomData" | "PhantomPinned" => (0, 1),
        _ => return None,
    })
}

fn align_up(x: u64, a: u64) -> u64 {
    if a <= 1 { x } else { x.div_ceil(a) * a }
}

impl Types {
    fn size(&self, t: &Type, depth: u32) -> Option<(u64, u64)> {
        if depth > 16 {
            return None;
        }
        match t {
            Type::Array(a) => {
                let (s, al) = self.size(&a.elem, depth + 1)?;
                let n = match &a.len {
                    syn::Expr::Lit(_) => scan_int(&a.len)?,
                    syn::Expr::Path(p) => *self.consts.get(&p.path.segments.last()?.ident.to_string())?,
                    _ => return None,
                };
                Some((s * n, al))
            }
            Type::Paren(p) => self.size(&p.elem, depth + 1),
            Type::Group(g) => self.size(&g.elem, depth + 1),
            Type::Ptr(_) | Type::Reference(_) => Some((8, 8)),
            Type::Path(p) => {
                let last = p.path.segments.last()?;
                let n = last.ident.to_string();
                if let Some(x) = prim(&n) {
                    return Some(x);
                }
                if let Some(b) = self.enums.get(&n) {
                    return Some((*b, *b));
                }
                if let Some(a) = self.aliases.get(&n) {
                    return self.size(a, depth + 1);
                }
                let s = self.structs.get(&n)?;
                if s.generic {
                    return None;
                }
                self.layout(s, depth + 1).map(|l| (l.size, l.align))
            }
            _ => None,
        }
    }

    fn layout(&self, s: &StructDef, depth: u32) -> Option<Layout> {
        let transparent = s.repr.iter().any(|r| r == "transparent");
        let c = s.repr.iter().any(|r| r == "C");
        if s.repr.iter().any(|r| r.starts_with("packed")) {
            return None;
        }
        if transparent {
            let mut out = None;
            for (_, t) in &s.fields {
                let (sz, al) = self.size(t, depth + 1)?;
                if sz > 0 {
                    out = Some(Layout {
                        size: sz,
                        align: al,
                        interior: 0,
                        trailing: 0,
                    });
                }
            }
            return out.or(Some(Layout {
                size: 0,
                align: 1,
                interior: 0,
                trailing: 0,
            }));
        }
        if !c {
            return None;
        }
        let mut off = 0u64;
        let mut align = 1u64;
        let mut interior = 0u64;
        for (_, t) in &s.fields {
            let (sz, al) = self.size(t, depth + 1)?;
            let o = align_up(off, al);
            interior += o - off;
            off = o + sz;
            align = align.max(al);
        }
        for r in &s.repr {
            if let Some(n) = r.strip_prefix("align(").and_then(|x| x.strip_suffix(')')) {
                align = align.max(n.trim().parse().ok()?);
            }
        }
        let size = align_up(off, align);
        Some(Layout {
            size,
            align,
            interior,
            trailing: size - off,
        })
    }
}

fn scan_int(e: &syn::Expr) -> Option<u64> {
    match e {
        syn::Expr::Lit(l) => match &l.lit {
            syn::Lit::Int(i) => i.base10_parse().ok(),
            _ => None,
        },
        _ => None,
    }
}

struct Layout {
    size: u64,
    align: u64,
    interior: u64,
    trailing: u64,
}

// ------------------------------------------------------------------------------------ config checks

static PROFILE_KV: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`([a-z][a-z-]*)\s*=\s*([^`]+)`").unwrap());
static PURGE_DELAY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)purge_?delay[\s\S]{0,80}?-\s*1\b").unwrap());
static CODE_CELL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`([A-Za-z0-9_.-]+)`").unwrap());

/// Rows (cells) of the tables under the heading whose text is `heading`.
fn md_table_rows(text: &str, heading: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut inside = false;
    for l in text.lines() {
        if l.starts_with('#') {
            inside = l.trim_start_matches('#').trim() == heading;
            continue;
        }
        if inside && l.trim_start().starts_with('|') {
            let cells: Vec<String> = l
                .trim()
                .trim_matches('|')
                .split('|')
                .map(|c| c.trim().to_string())
                .collect();
            if cells
                .iter()
                .all(|c| c.chars().all(|ch| ch == '-' || ch == ':' || ch == ' '))
            {
                continue;
            }
            rows.push(cells);
        }
    }
    rows
}

fn toml_value(s: &str) -> Option<toml::Value> {
    format!("v = {s}")
        .parse::<toml::Table>()
        .ok()
        .and_then(|t| t.get("v").cloned())
}

/// Effective settings of `name` following `inherits` (`bench` → `release`, `test` → `dev` implicitly).
fn effective(
    table: &BTreeMap<String, BTreeMap<String, toml::Value>>,
    name: &str,
    implicit: bool,
) -> BTreeMap<String, toml::Value> {
    let mut chain = Vec::new();
    let mut cur = Some(name.to_string());
    while let Some(n) = cur.take() {
        if chain.contains(&n) || chain.len() > 8 {
            break;
        }
        chain.push(n.clone());
        let parent = table
            .get(&n)
            .and_then(|m| m.get("inherits"))
            .and_then(|v| v.as_str())
            .map(str::to_string);
        cur = parent.or_else(|| {
            if !implicit {
                return None;
            }
            match n.as_str() {
                "bench" => Some("release".into()),
                "test" => Some("dev".into()),
                _ => None,
            }
        });
    }
    let mut out = BTreeMap::new();
    for n in chain.iter().rev() {
        if let Some(m) = table.get(n) {
            for (k, v) in m {
                if k != "inherits" {
                    out.insert(k.clone(), v.clone());
                }
            }
        }
    }
    out
}

fn check_profiles(root: &Path, out: &mut Vec<Violation>) -> Result<(), EnvError> {
    const DEV: &str = "docs/development.md";
    let doc = read(root, DEV)?;
    let mut spec: BTreeMap<String, BTreeMap<String, toml::Value>> = BTreeMap::new();
    for r in md_table_rows(&doc, "构建剖面") {
        let (Some(name), Some(set)) = (r.first(), r.get(2)) else {
            continue;
        };
        let name = name.trim_matches('`').to_string();
        let mut m = BTreeMap::new();
        for c in PROFILE_KV.captures_iter(set) {
            if let Some(val) = toml_value(c[2].trim()) {
                m.insert(c[1].to_string(), val);
            }
        }
        spec.insert(name, m);
    }
    let cargo: toml::Table = read(root, "Cargo.toml")?
        .parse()
        .map_err(|e| EnvError(format!("parse Cargo.toml: {e}")))?;
    let mut have: BTreeMap<String, BTreeMap<String, toml::Value>> = BTreeMap::new();
    if let Some(p) = cargo.get("profile").and_then(|p| p.as_table()) {
        for (k, t) in p {
            if let Some(t) = t.as_table() {
                have.insert(k.clone(), t.iter().map(|(a, b)| (a.clone(), b.clone())).collect());
            }
        }
    }
    for name in PROFILES {
        if !spec.contains_key(*name) {
            return Err(EnvError(format!("{DEV} 构建剖面: no row for `{name}`")));
        }
        if !have.contains_key(*name) {
            out.push(v("B1", "Cargo.toml", 0, format!("[profile.{name}] is missing")));
            continue;
        }
        let want = effective(&spec, name, true);
        let got = effective(&have, name, true);
        for (k, val) in &want {
            if got.get(k) != Some(val) {
                out.push(v(
                    "B1",
                    "Cargo.toml",
                    0,
                    format!(
                        "[profile.{name}] {k} = {} differs from {DEV} 构建剖面 ({k} = {val})",
                        got.get(k).map_or("<unset>".to_string(), |x| x.to_string())
                    ),
                ));
            }
        }
    }
    // B2: release target-cpu.
    for r in md_table_rows(&doc, "目标 CPU") {
        let (Some(triple), Some(cpu)) = (r.get(1), r.get(2)) else {
            continue;
        };
        let (Some(t), Some(c)) = (CODE_CELL.captures(triple), CODE_CELL.captures(cpu)) else {
            continue;
        };
        let (triple, cpu) = (t[1].to_string(), c[1].to_string());
        let cfg_text = std::fs::read_to_string(root.join(".cargo/config.toml")).unwrap_or_default();
        let cfg: toml::Table = cfg_text.parse().unwrap_or_default();
        let flags: Vec<String> = cfg
            .get("target")
            .and_then(|x| x.get(&triple))
            .and_then(|x| x.get("rustflags"))
            .and_then(|x| x.as_array())
            .map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        let joined = flags.join(" ").replace("-C ", "-C");
        if !joined.contains(&format!("-Ctarget-cpu={cpu}")) {
            out.push(v(
                "B2",
                ".cargo/config.toml",
                0,
                format!(
                    "[target.{triple}] rustflags lack `-C target-cpu={cpu}` (docs/development.md 目标 CPU)"
                ),
            ));
        }
        break; // first (core) row only
    }
    Ok(())
}

/// Required lint levels of `tools/lint/attrs-<name>.rs`: lint → level.
fn template_levels(root: &Path, name: &str) -> Result<BTreeMap<String, String>, EnvError> {
    let rel = format!("tools/lint/attrs-{name}.rs");
    let src = read(root, &rel)?;
    let ls = scan::inner_lint_levels(&src).map_err(|e| EnvError(format!("{rel}: {e}")))?;
    Ok(ls.into_iter().map(|(lvl, l)| (l, lvl)).collect())
}

fn rank(level: &str) -> u8 {
    match level {
        "forbid" => 3,
        "deny" => 2,
        "warn" => 1,
        _ => 0,
    }
}

// ----------------------------------------------------------------------------------------- the check

/// Runs every check on the tree at `root`.
pub fn check(root: &Path) -> Result<Vec<Violation>, EnvError> {
    let cfg = deps::Config::load(root)?;
    let allow = AllowList::load(root)?;
    let mut out = Vec::new();
    let crates = lib_crates(root, &cfg);
    let hot_pure: Vec<&Crate> = crates
        .iter()
        .filter(|c| matches!(c.tier.as_deref(), Some("hot" | "pure")))
        .collect();

    // Lint configuration: clippy.toml copies and lib.rs attribute blocks.
    let tpl = |t: &str| read(root, &format!("tools/lint/clippy-{t}.toml"));
    let (clippy_hot, clippy_pure) = (tpl("hot")?, tpl("pure")?);
    let (attrs_hot, attrs_pure, attrs_unsafe) = (
        template_levels(root, "hot")?,
        template_levels(root, "pure")?,
        template_levels(root, "unsafe")?,
    );
    for c in &hot_pure {
        let tier = c.tier.as_deref().unwrap_or("");
        let want = if tier == "hot" { &clippy_hot } else { &clippy_pure };
        let rel = format!("{}/clippy.toml", c.dir);
        match std::fs::read(root.join(&rel)) {
            Ok(b) if b == want.as_bytes() => {}
            Ok(_) => out.push(v(
                "检查机制",
                &rel,
                0,
                format!("differs from tools/lint/clippy-{tier}.toml"),
            )),
            Err(_) => out.push(v(
                "检查机制",
                &rel,
                0,
                format!("missing (copy tools/lint/clippy-{tier}.toml)"),
            )),
        }
        let lib = format!("{}/src/lib.rs", c.dir);
        let Ok(src) = std::fs::read_to_string(root.join(&lib)) else {
            continue;
        };
        let have = match scan::inner_lint_levels(&src) {
            Ok(h) => h,
            Err(e) => {
                out.push(v("syntax", &lib, 0, e));
                continue;
            }
        };
        let level_of = |lint: &str| -> u8 {
            // The last attribute naming the lint decides (a later allow/warn weakens it).
            have.iter()
                .rev()
                .find(|(_, l)| l == lint)
                .map_or(0, |(lvl, _)| rank(lvl))
        };
        let mut req: BTreeMap<String, String> = if tier == "hot" {
            attrs_hot.clone()
        } else {
            attrs_pure.clone()
        };
        if crate_matches(UNSAFE_CRATES, &c.dir) && level_of("unsafe_code") < rank("forbid") {
            req.remove("unsafe_code");
            req.extend(attrs_unsafe.clone());
        }
        if crate_matches(N3_CRATES, &c.dir) || c.dir.starts_with("strategies/") {
            req.insert("clippy::float_arithmetic".into(), "deny".into());
        }
        for (lint, lvl) in &req {
            let got = level_of(lint);
            if got < rank(lvl) {
                let rule = if lint == "unsafe_code" || lint == "clippy::undocumented_unsafe_blocks" {
                    "B5"
                } else if lint == "clippy::float_arithmetic" {
                    "N3"
                } else {
                    "C1"
                };
                let how = if got == 0 && !have.iter().any(|(_, l)| l == lint) {
                    "missing"
                } else {
                    "weakened"
                };
                out.push(v(
                    rule,
                    &lib,
                    0,
                    format!("lint block: `{lint}` {how} (required: {lvl}; tools/lint/attrs-*.rs)"),
                ));
            }
        }
    }

    // Source scan.
    let mut dirs: Vec<String> = crates.iter().map(|c| c.dir.clone()).collect();
    if let Ok(rd) = std::fs::read_dir(root.join("apps")) {
        for e in rd.flatten() {
            if e.path().join("Cargo.toml").is_file() {
                dirs.push(format!("apps/{}", e.file_name().to_string_lossy()));
            }
        }
    }
    let tree = load_tree(root, &dirs, &mut out);
    let tier_of = |f: &str| -> Option<&Crate> { crates.iter().find(|c| under(f, &c.dir)) };
    let in_src = |f: &str, c: &str| under(f, &format!("{c}/src"));
    let is_test_file = |f: &str| tree.test_files.contains(f);

    // Type table for layouts (all hot/pure crate sources).
    let mut types = Types::default();
    for (f, (_, s)) in &tree.files {
        if tier_of(f).is_some_and(|c| matches!(c.tier.as_deref(), Some("hot" | "pure"))) {
            for st in &s.structs {
                types
                    .structs
                    .entry(st.what.name.clone())
                    .or_insert_with(|| st.what.clone());
            }
            types.enums.extend(s.enums.iter().cloned());
            types.consts.extend(s.consts.iter().cloned());
            types.aliases.extend(s.aliases.iter().cloned());
        }
    }
    // size_of assertions per crate.
    let mut crate_text: BTreeMap<String, String> = BTreeMap::new();
    for (f, (src, _)) in &tree.files {
        if let Some(c) = tier_of(f)
            && in_src(f, &c.dir)
        {
            crate_text.entry(c.dir.clone()).or_default().push_str(src);
        }
    }

    for (f, (src, s)) in &tree.files {
        let krate = tier_of(f);
        let app = f.starts_with("apps/");
        let hp = krate.is_some_and(|c| matches!(c.tier.as_deref(), Some("hot" | "pure")));
        let src_file =
            krate.is_some_and(|c| in_src(f, &c.dir)) || (app && f.split('/').nth(2) == Some("src"));
        let file_test = is_test_file(f);
        let strategy = krate.is_some_and(|c| c.dir.starts_with("strategies/"));
        let model = strategy && krate.is_some_and(|c| under(f, &format!("{}/src/model", c.dir)));
        let lines: Vec<&str> = src.lines().collect();

        // #[allow] registration (every file of a hot/pure crate).
        if hp {
            for a in &s.allows {
                for l in &a.what {
                    if REGISTERED_LINTS.contains(&l.as_str()) && !allow.allows(f, l) {
                        let rule = if strategy { "S1" } else { "检查机制" };
                        out.push(v(
                            rule,
                            f,
                            a.line,
                            format!("#[allow({l})] is not registered in {ALLOW_LIST}"),
                        ));
                    }
                }
            }
        }
        if !src_file {
            continue;
        }
        let live = |t: bool| !t && !file_test;

        if let Some(c) = krate {
            // N6 / S1: trait objects.
            if hp && !model {
                for d in s.dyn_types.iter().filter(|d| live(d.test)) {
                    let rule = if strategy { "S1" } else { "N6" };
                    out.push(v(rule, f, d.line, "trait object (`dyn`) in a type position"));
                }
            }
            // N3 / S1: floats.
            let n3 = (crate_matches(N3_CRATES, &c.dir) && !N3_EXEMPT.iter().any(|x| f.starts_with(x)))
                || (strategy && !model);
            if n3 {
                for x in s.floats.iter().filter(|d| live(d.test)) {
                    let rule = if strategy { "S1" } else { "N3" };
                    out.push(v(rule, f, x.line, format!("`{}` in a type position", x.what)));
                }
            }
            // B5 / S1: unsafe.
            if hp {
                let allowed = crate_matches(UNSAFE_CRATES, &c.dir);
                for u in &s.unsafes {
                    if !allowed {
                        let rule = if strategy { "S1" } else { "B5" };
                        out.push(v(
                            rule,
                            f,
                            u.line,
                            format!("`unsafe` {} outside {UNSAFE_CRATES:?}", u.what),
                        ));
                    } else if matches!(u.what, "block" | "impl") && !has_safety(&lines, u.line) {
                        out.push(v(
                            "B5",
                            f,
                            u.line,
                            format!("`unsafe` {} without a `// SAFETY:` comment", u.what),
                        ));
                    }
                }
            }
            // T1: thread creation.
            for x in s.spawns.iter().filter(|d| live(d.test)) {
                out.push(v("T1", f, x.line, format!("`{}` outside apps/*", x.what)));
            }
            // T3: Atomic* outside hft-rt/hft-journal.
            if hp && !crate_matches(ATOMIC_CRATES, &c.dir) {
                for x in s.atomics.iter().filter(|d| live(d.test)) {
                    out.push(v(
                        "T3",
                        f,
                        x.line,
                        format!("`{}` outside hft-rt/hft-journal", x.what),
                    ));
                }
            }
            // Pure tier (crates/AGENTS.md "tier"; T4 for the clock): no `std::time`, `std::fs`,
            // `std::net` outside test code.
            if krate.is_some_and(|c| c.tier.as_deref() == Some("pure")) {
                for x in s.std_io.iter().filter(|d| live(d.test)) {
                    let rule = if x.what.starts_with("std::time") {
                        "T4"
                    } else {
                        "pure"
                    };
                    out.push(v(rule, f, x.line, format!("`{}` in a pure crate", x.what)));
                }
            }
            // N3: model_math / strat-lib model only from a strategy's src/model/.
            let own = c.dir == "crates/hft-strategy-sdk" || c.dir == "crates/hft-strat-lib";
            if !own && !model {
                for x in s.model_refs.iter().filter(|d| live(d.test)) {
                    out.push(v(
                        "N3",
                        f,
                        x.line,
                        format!("`{}` referenced outside a strategy's src/model/", x.what),
                    ));
                }
            }
            // M4: repr(C) assertions and padding; D6: TSC fields in snapshot rows.
            if hp {
                for st in s.structs.iter().filter(|d| live(d.test)) {
                    let d = &st.what;
                    if d.repr.iter().any(|r| r == "C") {
                        let re = Regex::new(&format!(
                            r"size_of::<\s*(?:[\w:]+::)?{}\s*(?:<[^<>]*>)?\s*>\s*\(\s*\)",
                            regex::escape(&d.name)
                        ))
                        .map_err(|e| EnvError(e.to_string()))?;
                        if !crate_text.get(&c.dir).is_some_and(|t| re.is_match(t)) {
                            out.push(v(
                                "M4",
                                f,
                                st.line,
                                format!(
                                    "#[repr(C)] struct {} has no `size_of::<{}>()` compile-time assertion",
                                    d.name, d.name
                                ),
                            ));
                        }
                        if !d.into_bytes
                            && !d.generic
                            && let Some(l) = types.layout(d, 0)
                        {
                            let aligned = d.repr.iter().any(|r| r.starts_with("align("));
                            if l.interior > 0 || (l.trailing > 0 && !aligned) {
                                out.push(v(
                                    "M6",
                                    f,
                                    st.line,
                                    format!(
                                        "#[repr(C)] struct {} has implicit padding ({} B between fields, {} B at the end of {} B); add explicit `_pad`",
                                        d.name, l.interior, l.trailing, l.size
                                    ),
                                ));
                            }
                        }
                    }
                    if d.name.ends_with("Row") {
                        for (fname, ty) in &d.fields {
                            let tsc_ty = matches!(ty, Type::Path(p) if p.path.segments.last().is_some_and(|s| s.ident == "Tsc"));
                            let tsc_name = fname == "tsc" || fname.ends_with("_tsc");
                            if (tsc_ty || tsc_name)
                                && !allow.tsc.iter().any(|(r, x)| r == &d.name && x == fname)
                            {
                                out.push(v(
                                    "D6",
                                    f,
                                    st.line,
                                    format!("snapshot row {}.{fname} holds a TSC value and is not a [[tsc]] entry of {ALLOW_LIST}", d.name),
                                ));
                            }
                        }
                    }
                }
            }
        }
        // M3: MmapMut only in hft-journal.
        if !under(f, "crates/hft-journal") {
            for x in s.mmap_mut.iter().filter(|d| live(d.test)) {
                out.push(v("M3", f, x.line, "`memmap2::MmapMut` outside hft-journal"));
            }
        }
        // M1: mimalloc purge_delay.
        if s.global_allocator && src.contains("MiMalloc") {
            let pkg = f.splitn(3, '/').take(2).collect::<Vec<_>>().join("/");
            let ok = tree
                .files
                .iter()
                .any(|(g, (t, _))| under(g, &pkg) && PURGE_DELAY.is_match(t));
            if !ok {
                out.push(v("M1", f, 0, "global mimalloc without `purge_delay = -1`"));
            }
        }
    }
    out.extend(tls_violations(&tree));
    check_profiles(root, &mut out)?;
    out.sort();
    out.dedup();
    Ok(out)
}

/// `// SAFETY:` on the `unsafe` line, in the comment lines directly above it, or above the statement
/// the `unsafe` belongs to (continuation lines without `;`/`{`/`}` at their end).
fn has_safety(lines: &[&str], line: usize) -> bool {
    let idx = line.saturating_sub(1);
    if lines.get(idx).is_some_and(|l| l.contains("SAFETY:")) {
        return true;
    }
    let mut i = idx;
    let mut hops = 0;
    while i > 0 && hops < 12 {
        i -= 1;
        hops += 1;
        let l = lines.get(i).map_or("", |l| l.trim());
        if l.starts_with("//") || l.starts_with("#[") || l.starts_with("/*") || l.starts_with('*') {
            if l.contains("SAFETY:") {
                return true;
            }
            continue;
        }
        if l.is_empty() || l.ends_with(';') || l.ends_with('{') || l.ends_with('}') {
            return false;
        }
    }
    false
}

fn tls_violations(tree: &Tree) -> Vec<Violation> {
    let mut out = Vec::new();
    for (f, (src, s)) in &tree.files {
        if f == TLS_FILE || tree.test_files.contains(f) {
            continue;
        }
        let tls_file = src.contains("rustls") || src.contains("WriteTraffic");
        for c in s.encrypt_calls.iter().filter(|c| !c.test) {
            if tls_file || c.what.contains("WriteTraffic") {
                out.push(v(
                    "T6",
                    f,
                    c.line,
                    format!("rustls record encryption ({}) outside {TLS_FILE}", c.what),
                ));
            }
        }
    }
    out
}

/// T6 scan alone (shared with component tests such as hft-net's): every rustls record encryption call
/// site under `crates/`, `apps/` and `strategies/` outside `crates/hft-net/src/tls.rs`.
pub fn scan_tls_encrypt(root: &Path) -> Vec<Violation> {
    let mut dirs = Vec::new();
    for base in ["crates", "apps", "strategies"] {
        if let Ok(rd) = std::fs::read_dir(root.join(base)) {
            for e in rd.flatten() {
                if e.path().join("Cargo.toml").is_file() {
                    dirs.push(format!("{base}/{}", e.file_name().to_string_lossy()));
                }
            }
        }
    }
    let mut ignore = Vec::new();
    let tree = load_tree(root, &dirs, &mut ignore);
    tls_violations(&tree)
}

/// Entry point.
pub fn run(root: &Path) -> i32 {
    let vs = match check(root) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("hot-check: error: {e}");
            return EXIT_ENV;
        }
    };
    for x in &vs {
        eprintln!("[{}] {}:{}: {}", x.rule, x.file, x.line, x.detail);
    }
    let j = json!({
        "violations": vs.iter().map(|x| json!({"rule": x.rule, "file": x.file, "line": x.line, "detail": x.detail})).collect::<Vec<_>>(),
    });
    if let Err(e) = write_json(root, "hot-check", &j) {
        eprintln!("hot-check: error: {e}");
        return EXIT_ENV;
    }
    eprintln!(
        "hot-check: violations={} -> {}",
        vs.len(),
        if vs.is_empty() { "PASS" } else { "FAIL" }
    );
    if vs.is_empty() { EXIT_OK } else { EXIT_FAIL }
}
