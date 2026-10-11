//! Closed fixture manifests shared by acquisition and replay. Paths are portable cache-relative names.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::EnvError;

use super::fetch::{error, regular};

#[derive(Debug)]
pub(crate) struct Object {
    pub uri: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug)]
pub(crate) struct Pointer {
    pub name: String,
    pub content: Content,
}

#[derive(Debug)]
pub(crate) enum Content {
    Object(Object),
    Files(Bundle),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StrategySet {
    Registry,
    TestkitReplayV1,
}

#[derive(Debug)]
pub(crate) struct Bundle {
    pub strategy_set: StrategySet,
    pub journal: String,
    pub files: BTreeMap<String, Object>,
    pub dirs: BTreeSet<String>,
    pub checks: Vec<Check>,
}

#[derive(Debug)]
pub(crate) struct Check {
    pub name: String,
    pub advice: String,
    pub checkpoint: Option<String>,
    pub from_seq: Option<u64>,
    pub to_seq: Option<u64>,
}

fn string<'a>(p: &Path, t: &'a toml::Table, key: &str) -> Result<&'a str, EnvError> {
    t.get(key)
        .and_then(toml::Value::as_str)
        .ok_or_else(|| error(p, format!("missing string {key}")))
}

fn only(p: &Path, t: &toml::Table, keys: &[&str]) -> Result<(), EnvError> {
    for key in t.keys() {
        if !keys.contains(&key.as_str()) {
            return Err(error(p, format!("unknown manifest field {key}")));
        }
    }
    Ok(())
}

fn object(p: &Path, t: &toml::Table) -> Result<Object, EnvError> {
    let uri = string(p, t, "uri")?;
    let (bucket, key) = uri
        .strip_prefix("s3://")
        .and_then(|u| u.split_once('/'))
        .ok_or_else(|| error(p, "uri must be s3://<bucket>/hft-fixtures/<key>.zst"))?;
    if bucket.is_empty()
        || !bucket
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-'))
        || !key.starts_with("hft-fixtures/")
        || !key.ends_with(".zst")
        || key.split('/').any(|s| s.is_empty() || matches!(s, "." | ".."))
        || !key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.'))
    {
        return Err(error(p, "invalid fixture S3 URI"));
    }
    let sha256 = string(p, t, "sha256")?;
    if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(error(p, "sha256 must contain exactly 64 hexadecimal digits"));
    }
    let bytes = t
        .get("bytes")
        .and_then(toml::Value::as_integer)
        .and_then(|n| u64::try_from(n).ok())
        .filter(|n| *n > 0)
        .ok_or_else(|| error(p, "bytes must be a positive integer"))?;
    Ok(Object {
        uri: uri.into(),
        sha256: sha256.to_ascii_lowercase(),
        bytes,
    })
}

fn component(s: &str, dots: bool) -> bool {
    let stem = s.split('.').next().unwrap_or("").to_ascii_uppercase();
    !s.is_empty()
        && !s.starts_with('.')
        && !s.ends_with('.')
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_') || (dots && b == b'.'))
        && !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        && !(stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}

fn relative(p: &Path, s: &str) -> Result<String, EnvError> {
    if !s.split('/').all(|s| component(s, true)) {
        return Err(error(p, format!("invalid portable relative path {s:?}")));
    }
    Ok(s.into())
}

fn rows<'a>(p: &Path, t: &'a toml::Table, key: &str) -> Result<&'a Vec<toml::Value>, EnvError> {
    t.get(key)
        .and_then(toml::Value::as_array)
        .filter(|r| !r.is_empty())
        .ok_or_else(|| error(p, format!("{key} must be a nonempty array of tables")))
}

fn table<'a>(p: &Path, value: &'a toml::Value) -> Result<&'a toml::Table, EnvError> {
    value.as_table().ok_or_else(|| error(p, "expected a table"))
}

fn bound(p: &Path, t: &toml::Table, key: &str) -> Result<Option<u64>, EnvError> {
    t.get(key)
        .map(|v| {
            v.as_integer()
                .and_then(|n| u64::try_from(n).ok())
                .filter(|n| *n > 0)
                .ok_or_else(|| error(p, format!("{key} must be positive")))
        })
        .transpose()
}

fn bundle(p: &Path, t: &toml::Table) -> Result<Bundle, EnvError> {
    only(
        p,
        t,
        &[
            "format",
            "strategy_set",
            "journal",
            "files",
            "checks",
            "purpose",
            "source",
        ],
    )?;
    if t.get("purpose").is_some_and(|v| !v.is_str()) || t.get("source").is_some_and(|v| !v.is_table()) {
        return Err(error(
            p,
            "purpose must be a string and source must be a metadata table",
        ));
    }
    let strategy_set = match string(p, t, "strategy_set")? {
        "registry" => StrategySet::Registry,
        "testkit-replay-v1" => StrategySet::TestkitReplayV1,
        _ => return Err(error(p, "unknown fixture strategy_set")),
    };
    let journal = relative(p, string(p, t, "journal")?)?;
    let mut files = BTreeMap::new();
    let mut dirs = BTreeSet::new();
    let mut names = BTreeMap::new();
    for row in rows(p, t, "files")? {
        let row = table(p, row)?;
        only(p, row, &["path", "uri", "sha256", "bytes"])?;
        let path = relative(p, string(p, row, "path")?)?;
        let mut prefix = String::new();
        let parts: Vec<_> = path.split('/').collect();
        for (i, part) in parts.iter().enumerate() {
            if i != 0 {
                prefix.push('/');
            }
            prefix.push_str(part);
            let dir = i + 1 != parts.len();
            if let Some((old, old_dir)) = names.insert(prefix.to_ascii_lowercase(), (prefix.clone(), dir))
                && (old != prefix || old_dir != dir || !dir)
            {
                return Err(error(
                    p,
                    "duplicate, case alias or overlapping file/directory path",
                ));
            }
            if dir {
                dirs.insert(prefix.clone());
            }
        }
        files.insert(path, object(p, row)?);
    }
    if !dirs.contains(&journal) {
        return Err(error(p, "journal must name a declared directory"));
    }
    let journal_prefix = format!("{journal}/");
    let mut segments = 0;
    for path in files.keys().filter(|path| path.starts_with(&journal_prefix)) {
        let (stem, ext) = path
            .rsplit_once('.')
            .ok_or_else(|| error(p, "journal member needs .jrn or .seal"))?;
        let number = &stem[journal_prefix.len()..];
        if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) || !matches!(ext, "jrn" | "seal")
        {
            return Err(error(p, "journal must contain numbered .jrn/.seal pairs"));
        }
        let other = if ext == "jrn" {
            segments += 1;
            "seal"
        } else {
            "jrn"
        };
        if !files.contains_key(&format!("{stem}.{other}")) {
            return Err(error(p, "journal segment or seal missing from manifest"));
        }
    }
    if segments == 0 {
        return Err(error(p, "manifest contains no journal segments"));
    }
    let mut checks = Vec::new();
    let mut check_names = BTreeSet::new();
    for row in rows(p, t, "checks")? {
        let row = table(p, row)?;
        only(p, row, &["name", "advice", "checkpoint", "from_seq", "to_seq"])?;
        let name = string(p, row, "name")?;
        if !component(name, false) || !check_names.insert(name.to_ascii_lowercase()) {
            return Err(error(p, "invalid or duplicate check name"));
        }
        let advice = string(p, row, "advice")?;
        if !matches!(advice, "journal" | "recompute") {
            return Err(error(p, "unknown advice mode"));
        }
        let checkpoint = if row.contains_key("checkpoint") {
            let path = relative(p, string(p, row, "checkpoint")?)?;
            if advice != "recompute" || !(files.contains_key(&path) || dirs.contains(&path)) {
                return Err(error(p, "checkpoint must be a declared recompute input"));
            }
            Some(path)
        } else {
            None
        };
        let from_seq = bound(p, row, "from_seq")?;
        let to_seq = bound(p, row, "to_seq")?;
        if let (Some(a), Some(b)) = (from_seq, to_seq)
            && a > b
        {
            return Err(error(p, "from_seq exceeds to_seq"));
        }
        checks.push(Check {
            name: name.into(),
            advice: advice.into(),
            checkpoint,
            from_seq,
            to_seq,
        });
    }
    Ok(Bundle {
        strategy_set,
        journal,
        files,
        dirs,
        checks,
    })
}

pub(crate) fn read(path: &Path) -> Result<Pointer, EnvError> {
    regular(path)?.ok_or_else(|| error(path, "pointer missing"))?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_suffix(".pointer.toml"))
        .filter(|n| component(n, false))
        .ok_or_else(|| error(path, "invalid portable pointer name"))?;
    let text = fs::read_to_string(path).map_err(|e| error(path, e))?;
    let t: toml::Table = text.parse().map_err(|e| error(path, e))?;
    let content = match t.get("format") {
        None => {
            if ["files", "checks", "strategy_set", "journal"]
                .iter()
                .any(|k| t.contains_key(*k))
            {
                return Err(error(path, "bundle fields require format = files-v1"));
            }
            Content::Object(object(path, &t)?)
        }
        Some(v) if v.as_str() == Some("files-v1") => Content::Files(bundle(path, &t)?),
        _ => return Err(error(path, "unsupported pointer format")),
    };
    Ok(Pointer {
        name: name.into(),
        content,
    })
}
