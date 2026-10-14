//! `cargo xtask new-strategy <name> --kind-id <n> [--with-model] [--with-pipeline]` (tools/README.md
//! "cargo xtask new-strategy"): renders `strategies/_template/` into `strategies/<name>/`, appends the
//! kind_id ledger row, the `strategy_set!` line and the registry dependency, adds the crate to the
//! workspace, then runs the crate's unit tests and `deps`.
//!
//! Template layout (`strategies/_template/`): `base/` is always rendered, `model/` (with
//! `--with-model`) and `pipeline/` (with `--with-pipeline`) are overlays on the same crate root (an
//! overlay file replaces the base file with the same path). Paths and UTF-8 contents substitute
//! `{{name}}` (kebab, = crate and directory name), `{{crate}}` (snake), `{{Type}}` (CamelCase) and
//! `{{kind_id}}` (decimal); a `.tmpl` suffix is dropped (template sources carry it so that no repository
//! scanner — cargo, doc-check, test-trace, Python linters — reads them as live files). The hot clippy
//! configuration `tools/lint/clippy-hot.toml` is copied in as `clippy.toml` (hot-check compares the
//! two byte for byte) unless the template has one.

use std::path::{Path, PathBuf};

use crate::exec::{Cmd, Exec};
use crate::{EXIT_ENV, EXIT_FAIL, EXIT_OK, EnvError};

pub const TEMPLATE: &str = "strategies/_template";
pub const REGISTRY: &str = "strategies/registry";
/// Suffix of template source files, dropped when rendered.
pub const TMPL: &str = ".tmpl";
/// The clippy configuration of `hot` crates (strategies are hot).
pub const CLIPPY_HOT: &str = "tools/lint/clippy-hot.toml";
/// kind_id test range (strategies/registry README): never allocated by `new-strategy`.
pub const TEST_RANGE_START: u32 = 0xFF00;

/// What to render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opts {
    pub name: String,
    pub kind_id: u16,
    pub with_model: bool,
    pub with_pipeline: bool,
}

impl Opts {
    /// `{{crate}}`: the Rust identifier of the crate.
    pub fn krate(&self) -> String {
        self.name.replace('-', "_")
    }

    /// `{{Type}}`: CamelCase of the name.
    pub fn ty(&self) -> String {
        self.name
            .split('-')
            .map(|w| {
                let mut c = w.chars();
                c.next()
                    .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                    .unwrap_or_default()
            })
            .collect()
    }

    fn subst(&self, s: &str) -> String {
        s.replace("{{name}}", &self.name)
            .replace("{{crate}}", &self.krate())
            .replace("{{Type}}", &self.ty())
            .replace("{{kind_id}}", &self.kind_id.to_string())
    }

    /// The equivalent command-line arguments.
    pub fn args(&self) -> Vec<String> {
        let mut a = vec![
            self.name.clone(),
            "--kind-id".into(),
            format!("{:#06x}", self.kind_id),
        ];
        if self.with_model {
            a.push("--with-model".into());
        }
        if self.with_pipeline {
            a.push("--with-pipeline".into());
        }
        a
    }
}

/// `5` or `0x0005`.
pub fn parse_kind_id(s: &str) -> Option<u16> {
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(h) => u16::from_str_radix(h, 16).ok(),
        None => s.parse().ok(),
    }
}

/// `[a-z][a-z0-9-]*`, not `registry`, no trailing or doubled `-`.
pub fn valid_name(n: &str) -> bool {
    let mut c = n.chars();
    c.next().is_some_and(|f| f.is_ascii_lowercase())
        && n.chars()
            .all(|x| x.is_ascii_lowercase() || x.is_ascii_digit() || x == '-')
        && !n.ends_with('-')
        && !n.contains("--")
        && n != "registry"
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .collect();
    out.sort();
    out
}

/// Renders the template at `template` into `out` (created); returns the written paths relative
/// to `out`.
pub fn render(template: &Path, out: &Path, o: &Opts) -> Result<Vec<String>, EnvError> {
    let base = template.join("base");
    if !base.is_dir() {
        return Err(EnvError(format!(
            "{}: template base/ not found",
            template.display()
        )));
    }
    let mut layers = vec![base];
    if o.with_model {
        layers.push(template.join("model"));
    }
    if o.with_pipeline {
        layers.push(template.join("pipeline"));
    }
    let mut files: std::collections::BTreeMap<String, PathBuf> = std::collections::BTreeMap::new();
    // A missing overlay renders nothing (`new-strategy` checks the layers it was asked for first;
    // `scaffold-gate` reports the missing shape).
    for l in &layers {
        for f in walk(l) {
            let rel = crate::rel_path(l, &f);
            let rel = rel.strip_suffix(TMPL).unwrap_or(&rel).to_string();
            files.insert(rel, f);
        }
    }
    let mut written = Vec::new();
    for (rel, src) in files {
        let rel = o.subst(&rel);
        let to = out.join(&rel);
        if let Some(d) = to.parent() {
            std::fs::create_dir_all(d).map_err(|e| EnvError(format!("create {}: {e}", d.display())))?;
        }
        let bytes = std::fs::read(&src).map_err(|e| EnvError(format!("read {}: {e}", src.display())))?;
        let data = match String::from_utf8(bytes) {
            Ok(t) => o.subst(&t).into_bytes(),
            Err(e) => e.into_bytes(),
        };
        std::fs::write(&to, data).map_err(|e| EnvError(format!("write {}: {e}", to.display())))?;
        written.push(rel);
    }
    Ok(written)
}

/// Copies `tools/lint/clippy-hot.toml` of `root` to `<out>/clippy.toml` unless the rendered crate has
/// one; `Ok(false)` when `root` has no such file (fixture repositories).
pub fn copy_clippy(root: &Path, out: &Path) -> Result<bool, EnvError> {
    let src = root.join(CLIPPY_HOT);
    let dst = out.join("clippy.toml");
    if !src.is_file() || dst.exists() {
        return Ok(false);
    }
    std::fs::copy(&src, &dst).map_err(|e| EnvError(format!("copy {CLIPPY_HOT}: {e}")))?;
    Ok(true)
}

fn parse_int(s: &str) -> Option<u32> {
    let s = s.trim();
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(h) => u32::from_str_radix(h, 16).ok(),
        None => s.parse().ok(),
    }
}

/// Line index of the kind_id ledger table header and the kind_ids of its single-id rows
/// (`(line, kind_id)`; range rows such as `0xFF00–0xFFFF` are skipped).
pub fn ledger_rows(readme: &str) -> Option<(usize, Vec<(usize, u32)>)> {
    let lines: Vec<&str> = readme.lines().collect();
    let head = lines.iter().position(|l| {
        let cells: Vec<&str> = l.split('|').map(str::trim).collect();
        cells.get(1) == Some(&"kind_id") && cells.get(2) == Some(&"crate")
    })?;
    let mut rows = Vec::new();
    for (i, l) in lines.iter().enumerate().skip(head + 2) {
        if !l.trim_start().starts_with('|') {
            break;
        }
        if let Some(k) = l.split('|').nth(1).and_then(parse_int) {
            rows.push((i, k));
        }
    }
    Some((head, rows))
}

/// The next kind_id: (largest real row, retired rows included) + 1.
pub fn next_kind_id(readme: &str) -> Option<u32> {
    let (_, rows) = ledger_rows(readme)?;
    Some(
        rows.iter()
            .map(|(_, k)| *k)
            .filter(|k| *k < TEST_RANGE_START)
            .max()
            .unwrap_or(0)
            + 1,
    )
}

/// The ledger with the row of `o` after the last real row.
pub fn add_ledger_row(readme: &str, o: &Opts) -> Option<String> {
    let (head, rows) = ledger_rows(readme)?;
    let after = rows
        .iter()
        .filter(|(_, k)| *k < TEST_RANGE_START)
        .map(|(i, _)| *i)
        .max()
        .unwrap_or(head + 1);
    let row = format!(
        "| {} | [{}](../{}/README.md) | core | 默认 |",
        o.kind_id, o.name, o.name
    );
    let mut lines: Vec<String> = readme.lines().map(str::to_string).collect();
    lines.insert(after + 1, row);
    let mut s = lines.join("\n");
    if readme.ends_with('\n') {
        s.push('\n');
    }
    Some(s)
}

/// Byte offsets of the `{`…`}` body of the struct in the first `strategy_set!` invocation.
fn set_body(src: &str) -> Option<(usize, usize)> {
    let m = src.find("strategy_set!")?;
    let st = m + src[m..].find("struct")?;
    let open = st + src[st..].find('{')?;
    let mut depth = 0usize;
    for (i, ch) in src[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((open, open + i));
                }
            }
            _ => {}
        }
    }
    None
}

/// `src` with `item` (`krate::Type`) appended as the last entry of the `strategy_set!` list.
pub fn add_set_line(src: &str, item: &str) -> Option<String> {
    let (open, close) = set_body(src)?;
    let body = &src[open + 1..close];
    let line_start = src[..close].rfind('\n').map_or(0, |i| i + 1);
    if src[line_start..close].trim().is_empty() {
        // Line-based: the closing brace stands alone.
        let last = body
            .lines()
            .rev()
            .find(|l| {
                let c = l.split("//").next().unwrap_or("").trim();
                !c.is_empty()
            })
            .unwrap_or("");
        let indent: String = last.chars().take_while(|c| c.is_whitespace()).collect();
        let indent = if last.trim().is_empty() {
            "        ".to_string()
        } else {
            indent
        };
        let code = last.split("//").next().unwrap_or("").trim_end();
        let mut out = String::new();
        if !code.is_empty() && !code.ends_with(',') && !code.ends_with('{') {
            // Add the missing comma after the last entry (before any trailing comment).
            let pos = src[..line_start].rfind(code).map(|p| p + code.len());
            match pos {
                Some(p) => {
                    out.push_str(&src[..p]);
                    out.push(',');
                    out.push_str(&src[p..line_start]);
                }
                None => out.push_str(&src[..line_start]),
            }
        } else {
            out.push_str(&src[..line_start]);
        }
        out.push_str(&format!("{indent}{item},\n"));
        out.push_str(&src[line_start..]);
        Some(out)
    } else {
        let trimmed = body.trim_end();
        let sep = if trimmed.ends_with(',') || trimmed.ends_with('{') || trimmed.is_empty() {
            " "
        } else {
            ", "
        };
        Some(format!(
            "{}{sep}{item} {}",
            &src[..open + 1 + trimmed.len()],
            &src[close..]
        ))
    }
}

/// `toml` with `line` appended at the end of its `[dependencies]` section (created when absent).
pub fn add_dependency(toml: &str, line: &str) -> String {
    let lines: Vec<&str> = toml.lines().collect();
    let Some(h) = lines.iter().position(|l| l.trim() == "[dependencies]") else {
        let mut s = toml.trim_end().to_string();
        s.push_str(&format!("\n\n[dependencies]\n{line}\n"));
        return s;
    };
    let end = lines
        .iter()
        .enumerate()
        .skip(h + 1)
        .find(|(_, l)| l.trim_start().starts_with('['))
        .map_or(lines.len(), |(i, _)| i);
    // Insert after the last non-blank line of the section.
    let mut at = end;
    while at > h + 1 && lines[at - 1].trim().is_empty() {
        at -= 1;
    }
    let mut out: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    out.insert(at, line.to_string());
    let mut s = out.join("\n");
    if toml.ends_with('\n') {
        s.push('\n');
    }
    s
}

/// Root `Cargo.toml` with `dir` added to `[workspace] members` (unchanged when a member pattern
/// already covers it).
pub fn add_member(toml: &str, dir: &str) -> Option<String> {
    let t: toml::Table = toml.parse().ok()?;
    let members = t.get("workspace")?.get("members")?.as_array()?;
    let parent = dir.rsplit_once('/').map_or("", |(p, _)| p);
    if members
        .iter()
        .filter_map(|v| v.as_str())
        .any(|m| m == dir || m == format!("{parent}/*"))
    {
        return Some(toml.to_string());
    }
    let m = toml.find("members")?;
    let open = m + toml[m..].find('[')?;
    let close = open + toml[open..].find(']')?;
    let line_start = toml[..close].rfind('\n').map_or(0, |i| i + 1);
    if toml[line_start..close].trim().is_empty() {
        let prev = toml[open + 1..line_start]
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty());
        let indent: String = prev
            .map(|l| l.chars().take_while(|c| c.is_whitespace()).collect())
            .unwrap_or_else(|| "    ".to_string());
        let needs_comma = prev.is_some_and(|l| !l.trim_end().ends_with(','));
        let mut out = String::new();
        if needs_comma {
            let p = toml[..line_start].trim_end().len();
            out.push_str(&toml[..p]);
            out.push(',');
            out.push_str(&toml[p..line_start]);
        } else {
            out.push_str(&toml[..line_start]);
        }
        out.push_str(&format!("{indent}\"{dir}\",\n"));
        out.push_str(&toml[line_start..]);
        Some(out)
    } else {
        let inner = toml[open + 1..close].trim_end();
        let sep = if inner.trim().is_empty() || inner.ends_with(',') {
            ""
        } else {
            ", "
        };
        Some(format!(
            "{}{sep}\"{dir}\"{}",
            &toml[..open + 1 + inner.len()],
            &toml[close..]
        ))
    }
}

fn parse(args: &[String]) -> Result<Opts, EnvError> {
    let mut name = None;
    let mut kind = None;
    let (mut with_model, mut with_pipeline) = (false, false);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--kind-id" => {
                let v = it
                    .next()
                    .ok_or_else(|| EnvError("--kind-id needs a value".into()))?;
                kind = Some(parse_kind_id(v).ok_or_else(|| EnvError(format!("--kind-id {v}: not a u16")))?);
            }
            "--with-model" => with_model = true,
            "--with-pipeline" => with_pipeline = true,
            x if x.starts_with('-') => return Err(EnvError(format!("unknown argument {x}"))),
            x if name.is_none() => name = Some(x.to_string()),
            x => return Err(EnvError(format!("unexpected argument {x}"))),
        }
    }
    let name = name.ok_or_else(|| EnvError("missing <name>".into()))?;
    if !valid_name(&name) {
        return Err(EnvError(format!(
            "{name}: a strategy name is [a-z][a-z0-9-]* (not `registry`)"
        )));
    }
    let kind_id = kind.ok_or_else(|| EnvError("missing --kind-id <n>".into()))?;
    Ok(Opts {
        name,
        kind_id,
        with_model,
        with_pipeline,
    })
}

fn read(root: &Path, rel: &str) -> Result<String, EnvError> {
    std::fs::read_to_string(root.join(rel)).map_err(|e| EnvError(format!("read {rel}: {e}")))
}

fn write(root: &Path, rel: &str, text: &str) -> Result<(), EnvError> {
    std::fs::write(root.join(rel), text).map_err(|e| EnvError(format!("write {rel}: {e}")))
}

const LIB: &str = "strategies/registry/src/lib.rs";
const REG_TOML: &str = "strategies/registry/Cargo.toml";
const REG_README: &str = "strategies/registry/README.md";

/// `(file, new text)` of every edit, or why the request is refused (exit 1).
type Edits = Result<Vec<(&'static str, String)>, String>;

/// All edits of the four registry/workspace files, computed before anything is written.
fn plan_edits(root: &Path, o: &Opts) -> Result<Edits, EnvError> {
    let readme = read(root, REG_README)?;
    let Some(next) = next_kind_id(&readme) else {
        return Err(EnvError(format!("{REG_README}: kind_id ledger table not found")));
    };
    if u32::from(o.kind_id) != next {
        return Ok(Err(format!(
            "--kind-id {}: the next kind_id in the {REG_README} ledger is {next} (occupied and retired rows count; the test range {TEST_RANGE_START:#06x}–0xffff is not allocated here)",
            o.kind_id
        )));
    }
    if root.join("strategies").join(&o.name).exists() {
        return Ok(Err(format!("strategies/{}/ already exists", o.name)));
    }
    let lib = read(root, LIB)?;
    let lib = add_set_line(&lib, &format!("{}::{}", o.krate(), o.ty()))
        .ok_or_else(|| EnvError(format!("{LIB}: no strategy_set! struct body found")))?;
    let reg_toml = add_dependency(
        &read(root, REG_TOML)?,
        &format!("{} = {{ path = \"../{}\" }}", o.name, o.name),
    );
    let readme = add_ledger_row(&readme, o).ok_or_else(|| EnvError(format!("{REG_README}: ledger table")))?;
    let ws = add_member(&read(root, "Cargo.toml")?, &format!("strategies/{}", o.name))
        .ok_or_else(|| EnvError("Cargo.toml: no [workspace] members list".into()))?;
    Ok(Ok(vec![
        (LIB, lib),
        (REG_TOML, reg_toml),
        (REG_README, readme),
        ("Cargo.toml", ws),
    ]))
}

/// Runs `new-strategy` with `exec` (unit tests and `deps`); returns the exit code.
pub fn run_with(root: &Path, args: &[String], exec: &mut dyn Exec) -> i32 {
    let o = match parse(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!(
                "new-strategy: error: {e}\nusage: cargo xtask new-strategy <name> --kind-id <n> [--with-model] [--with-pipeline]"
            );
            return EXIT_ENV;
        }
    };
    for (layer, wanted) in [
        ("base", true),
        ("model", o.with_model),
        ("pipeline", o.with_pipeline),
    ] {
        if wanted && !root.join(TEMPLATE).join(layer).is_dir() {
            eprintln!("new-strategy: error: {TEMPLATE}/{layer}/ not found");
            return EXIT_ENV;
        }
    }
    let edits = match plan_edits(root, &o) {
        Ok(Ok(e)) => e,
        Ok(Err(why)) => {
            eprintln!("new-strategy: {why}; nothing written");
            return EXIT_FAIL;
        }
        Err(e) => {
            eprintln!("new-strategy: error: {e}");
            return EXIT_ENV;
        }
    };
    let out = root.join("strategies").join(&o.name);
    let written =
        match render(&root.join(TEMPLATE), &out, &o).and_then(|w| copy_clippy(root, &out).map(|_| w)) {
            Ok(w) => w,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&out);
                eprintln!("new-strategy: error: {e}");
                return EXIT_ENV;
            }
        };
    for (rel, text) in &edits {
        if let Err(e) = write(root, rel, text) {
            eprintln!("new-strategy: error: {e}");
            return EXIT_ENV;
        }
    }
    eprintln!(
        "new-strategy: strategies/{}/ ({} files), kind_id {} in {REG_README}, `{}::{}` in {LIB}, dependency in {REG_TOML}, workspace member",
        o.name,
        written.len(),
        o.kind_id,
        o.krate(),
        o.ty()
    );
    let mut code = EXIT_OK;
    for c in [
        Cmd::new("cargo", &["test", "-p", &o.name]),
        Cmd::new("xtask", &["deps"]),
    ] {
        match exec.run(&c) {
            Ok(r) if r.ok() => eprintln!("new-strategy: `{}` passed", c.line()),
            Ok(r) => {
                eprintln!("new-strategy: `{}` exited {}", c.line(), r.code);
                code = EXIT_FAIL;
            }
            Err(e) => {
                eprintln!("new-strategy: error: {e}");
                return EXIT_ENV;
            }
        }
    }
    code
}

/// Entry point.
pub fn run(root: &Path, args: &[String]) -> i32 {
    match crate::exec::from_env(root) {
        Ok(mut e) => run_with(root, args, e.as_mut()),
        Err(e) => {
            eprintln!("new-strategy: error: {e}");
            EXIT_ENV
        }
    }
}
