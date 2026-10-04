//! Shared helpers: the error/exit-code type, TOML input loading and key checking, venue and
//! enum name parsing, decimal → fixed point (via `hft_types::parse_exact`, never f64).

use std::collections::BTreeSet;
use std::fmt;
use std::path::Path;

use hft_types::{FxErr, VenueId, parse_exact, write_fixed};
use toml::{Table, Value};

/// Exit codes (tools/README.md "hft-cfg"): 0 success, 1 validation or diff rejected,
/// 2 input / environment error.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Exit {
    Rejected = 1,
    Input = 2,
}

/// A failed command: exit code plus the message printed on stderr (it names the rule or code).
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct CfgError {
    pub exit: Exit,
    pub msg: String,
}

impl fmt::Display for CfgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.msg)
    }
}

pub type Res<T> = Result<T, CfgError>;

/// Validation failure (exit 1).
pub fn rej<T>(msg: impl Into<String>) -> Res<T> {
    Err(CfgError {
        exit: Exit::Rejected,
        msg: msg.into(),
    })
}

/// Input or environment failure (exit 2).
pub fn input<T>(msg: impl Into<String>) -> Res<T> {
    Err(CfgError {
        exit: Exit::Input,
        msg: msg.into(),
    })
}

/// Reads a file; missing or unreadable → exit 2.
pub fn read_bytes(path: &Path) -> Res<Vec<u8>> {
    std::fs::read(path).or_else(|e| input(format!("{}: {e}", path.display())))
}

/// Reads a UTF-8 text file; missing, unreadable or not UTF-8 → exit 2.
pub fn read_text(path: &Path) -> Res<String> {
    let b = read_bytes(path)?;
    String::from_utf8(b).or_else(|_| input(format!("{}: not UTF-8", path.display())))
}

/// Parses TOML text; a syntax error → exit 2.
pub fn parse_toml(text: &str, path: &Path) -> Res<Table> {
    text.parse::<Table>()
        .or_else(|e| input(format!("{}: invalid TOML: {e}", path.display())))
}

/// Reads one TOML file, or every `*.toml` of a directory (file-name order) merged into one
/// table: tables merge key by key, top-level arrays concatenate, any other repeated key → exit 1.
pub fn load_toml_input(path: &Path) -> Res<Table> {
    if path.is_dir() {
        let mut files: Vec<_> = std::fs::read_dir(path)
            .or_else(|e| input(format!("{}: {e}", path.display())))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "toml"))
            .collect();
        files.sort();
        let mut root = Table::new();
        for f in files {
            let t = parse_toml(&read_text(&f)?, &f)?;
            merge(&mut root, t, "", true)?;
        }
        Ok(root)
    } else {
        parse_toml(&read_text(path)?, path)
    }
}

fn merge(into: &mut Table, from: Table, prefix: &str, top: bool) -> Res<()> {
    for (k, v) in from {
        let path = if prefix.is_empty() {
            k.clone()
        } else {
            format!("{prefix}.{k}")
        };
        match (into.get_mut(&k), v) {
            (None, v) => {
                into.insert(k, v);
            }
            (Some(Value::Table(a)), Value::Table(b)) => merge(a, b, &path, false)?,
            (Some(Value::Array(a)), Value::Array(b)) if top => a.extend(b),
            _ => return rej(format!("{path}: defined in more than one file")),
        }
    }
    Ok(())
}

/// Rejects keys of `t` outside `allowed` (exit 1, "unknown key").
pub fn check_keys(t: &Table, allowed: &[&str], ctx: &str) -> Res<()> {
    for k in t.keys() {
        if !allowed.contains(&k.as_str()) {
            return rej(format!("{ctx}: unknown key `{k}`"));
        }
    }
    Ok(())
}

pub fn opt_int(t: &Table, key: &str, ctx: &str) -> Res<Option<i64>> {
    match t.get(key) {
        None => Ok(None),
        Some(Value::Integer(x)) => Ok(Some(*x)),
        Some(_) => rej(format!("{ctx}.{key}: expected an integer")),
    }
}

pub fn req_int(t: &Table, key: &str, ctx: &str) -> Res<i64> {
    opt_int(t, key, ctx)?.map_or_else(|| rej(format!("{ctx}: missing key `{key}`")), Ok)
}

/// Integer converted to a narrower type; out of range → exit 1.
pub fn int_as<T: TryFrom<i64>>(x: i64, key: &str, ctx: &str) -> Res<T> {
    T::try_from(x).or_else(|_| rej(format!("{ctx}.{key}: {x} out of range")))
}

pub fn req<T: TryFrom<i64>>(t: &Table, key: &str, ctx: &str) -> Res<T> {
    int_as(req_int(t, key, ctx)?, key, ctx)
}

pub fn opt<T: TryFrom<i64> + Default>(t: &Table, key: &str, ctx: &str) -> Res<T> {
    match opt_int(t, key, ctx)? {
        None => Ok(T::default()),
        Some(x) => int_as(x, key, ctx),
    }
}

pub fn opt_str<'a>(t: &'a Table, key: &str, ctx: &str) -> Res<Option<&'a str>> {
    match t.get(key) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => rej(format!("{ctx}.{key}: expected a string")),
    }
}

pub fn req_str<'a>(t: &'a Table, key: &str, ctx: &str) -> Res<&'a str> {
    opt_str(t, key, ctx)?.map_or_else(|| rej(format!("{ctx}: missing key `{key}`")), Ok)
}

pub fn opt_bool(t: &Table, key: &str, ctx: &str) -> Res<Option<bool>> {
    match t.get(key) {
        None => Ok(None),
        Some(Value::Boolean(b)) => Ok(Some(*b)),
        Some(_) => rej(format!("{ctx}.{key}: expected a boolean")),
    }
}

pub fn req_bool(t: &Table, key: &str, ctx: &str) -> Res<bool> {
    opt_bool(t, key, ctx)?.map_or_else(|| rej(format!("{ctx}: missing key `{key}`")), Ok)
}

pub fn opt_table<'a>(t: &'a Table, key: &str, ctx: &str) -> Res<Option<&'a Table>> {
    match t.get(key) {
        None => Ok(None),
        Some(Value::Table(x)) => Ok(Some(x)),
        Some(_) => rej(format!("{ctx}.{key}: expected a table")),
    }
}

pub fn opt_array<'a>(t: &'a Table, key: &str, ctx: &str) -> Res<&'a [Value]> {
    match t.get(key) {
        None => Ok(&[]),
        Some(Value::Array(x)) => Ok(x),
        Some(_) => rej(format!("{ctx}.{key}: expected an array")),
    }
}

pub fn as_table<'a>(v: &'a Value, ctx: &str) -> Res<&'a Table> {
    v.as_table()
        .map_or_else(|| rej(format!("{ctx}: expected a table")), Ok)
}

/// `CamelCase` → `snake_case` (the spelling of enum values in hft-cfg TOML and the dump).
pub fn snake(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// Parses an enum value written in lower case (`one_way` or `one-way`); any upper-case letter,
/// or a name not in `all` → exit 1 listing the accepted values.
pub fn parse_enum<T: Copy>(all: &[T], name: fn(T) -> &'static str, s: &str, ctx: &str) -> Res<T> {
    let want = s.replace('-', "_");
    if s.chars().all(|c| !c.is_ascii_uppercase())
        && let Some(x) = all.iter().copied().find(|x| snake(name(*x)) == want)
    {
        return Ok(x);
    }
    let names: Vec<String> = all.iter().map(|x| format!("\"{}\"", snake(name(*x)))).collect();
    rej(format!("{ctx}: \"{s}\" is not one of {}", names.join(", ")))
}

/// Venue name: the hft-types constant name in either spelling (`binance-usdm`, `BINANCE_USDM`).
pub fn parse_venue(s: &str) -> Option<VenueId> {
    let want = s.to_ascii_uppercase().replace('-', "_");
    (1..=63u8)
        .map(VenueId)
        .find(|v| v.name().is_some_and(|n| n == want))
}

pub fn venue_or_rej(s: &str, ctx: &str) -> Res<VenueId> {
    parse_venue(s).map_or_else(|| rej(format!("{ctx}: UnknownVenue `{s}`")), Ok)
}

/// Display name of a venue (`binance-usdm`).
pub fn venue_name(v: VenueId) -> String {
    v.name().map_or_else(
        || format!("venue-{}", v.0),
        |n| n.to_ascii_lowercase().replace('_', "-"),
    )
}

fn fx_name(e: FxErr) -> &'static str {
    match e {
        FxErr::SYNTAX => "SYNTAX",
        FxErr::EMPTY => "EMPTY",
        FxErr::PRECISION => "PRECISION",
        FxErr::OVERFLOW => "OVERFLOW",
        FxErr::NEGATIVE => "NEGATIVE",
        FxErr::EXP_RANGE => "EXP_RANGE",
        _ => "error",
    }
}

/// Exact decimal → raw at `exp` (hft-types `parse_exact`; no float). Error → exit 1.
pub fn decimal(text: &str, exp: i8, ctx: &str) -> Res<i64> {
    parse_exact(text.as_bytes(), exp).or_else(|e| {
        rej(format!(
            "{ctx}: \"{text}\" at exponent {exp}: parse_exact {}",
            fx_name(e)
        ))
    })
}

/// Raw at `exp` → shortest exact decimal text.
pub fn fmt_fixed(raw: i64, exp: i8) -> String {
    let mut buf = [0u8; 40];
    let n = write_fixed(raw, exp, &mut buf);
    String::from_utf8_lossy(buf.get(..n).unwrap_or(&[])).into_owned()
}

/// Value given either as `<key>_raw = <int>` or as `<key> = "<decimal>"` at `exp`; both or
/// neither → exit 1 (neither: `None` when `required` is false).
pub fn raw_or_decimal(t: &Table, key: &str, exp: i8, ctx: &str, required: bool) -> Res<Option<i64>> {
    let raw_key = format!("{key}_raw");
    match (opt_int(t, &raw_key, ctx)?, t.get(key)) {
        (Some(_), Some(_)) => rej(format!("{ctx}: both `{key}` and `{raw_key}`")),
        (Some(r), None) => Ok(Some(r)),
        (None, Some(Value::String(s))) => decimal(s, exp, &format!("{ctx}.{key}")).map(Some),
        (None, Some(Value::Integer(i))) => decimal(&i.to_string(), exp, &format!("{ctx}.{key}")).map(Some),
        (None, Some(_)) => rej(format!("{ctx}.{key}: expected a decimal string")),
        (None, None) if required => rej(format!("{ctx}: missing key `{key}` (or `{raw_key}`)")),
        (None, None) => Ok(None),
    }
}

/// Keys of a table as a set (used to report unconsumed keys).
pub fn keyset(t: &Table) -> BTreeSet<&str> {
    t.keys().map(String::as_str).collect()
}

/// Copy of `b` in 8-byte aligned storage (SPB1 views need alignment).
pub fn aligned(b: &[u8]) -> Vec<u64> {
    let mut v = vec![0u64; b.len().div_ceil(8)];
    if let Some(d) = zerocopy::IntoBytes::as_mut_bytes(v.as_mut_slice()).get_mut(..b.len()) {
        d.copy_from_slice(b);
    }
    v
}

/// The first `len` bytes of aligned storage.
pub fn aligned_bytes(v: &[u64], len: usize) -> &[u8] {
    zerocopy::IntoBytes::as_bytes(v).get(..len).unwrap_or(&[])
}
