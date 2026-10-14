//! Read-only decoder fixtures. Expectations are independently reviewed files, never decoder output.
use serde_json::Value;
use std::{collections::BTreeSet, fmt, fs, path::Path};

#[derive(Clone, Debug)]
pub struct GoldenCase {
    pub id: String,
    pub venue: String,
    pub channel: String,
    pub meta: toml::Table,
    pub input: Vec<Vec<u8>>,
    /// One vector per input frame, including frames which intentionally emit nothing.
    pub expected: Vec<Vec<Value>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GoldenDiff {
    pub frame: usize,
    pub event: usize,
    pub field: String,
    pub expected: String,
    pub actual: String,
}
impl fmt::Display for GoldenDiff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "frame {} event {} {}: expected {}, actual {}",
            self.frame, self.event, self.field, self.expected, self.actual
        )
    }
}
impl std::error::Error for GoldenDiff {}

/// Loads `<root>/<venue>/<channel>`, sorted by path. Supports both per-case sidecars and
/// case directories with `meta.toml`. Missing/ambiguous expectations are errors, not empty suites.
pub fn golden_cases(root: &Path, venue: &str, channel: &str) -> Result<Vec<GoldenCase>, String> {
    fn name(s: &str) -> bool {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
            && s != "."
            && s != ".."
    }
    if !name(venue) || !name(channel) {
        return Err("invalid fixture namespace".into());
    }
    let base = root.join(venue).join(channel);
    let mut paths = Vec::new();
    fn visit(dir: &Path, paths: &mut Vec<std::path::PathBuf>) -> Result<(), String> {
        for entry in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
            let entry = entry.map_err(|e| e.to_string())?;
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_symlink() {
                return Err("fixture symlinks are not supported".into());
            }
            if kind.is_dir() {
                visit(&entry.path(), paths)?;
            } else if entry.path().extension().is_some_and(|x| x == "in") {
                paths.push(entry.path());
            }
        }
        Ok(())
    }
    visit(&base, &mut paths)?;
    paths.sort();
    if paths.is_empty() {
        return Err(format!("no fixtures in {}", base.display()));
    }
    paths
        .into_iter()
        .map(|path| {
            let sidecar = path.with_extension("meta.toml");
            let meta_path = if sidecar.exists() {
                sidecar
            } else {
                path.with_file_name("meta.toml")
            };
            let meta: toml::Table = fs::read_to_string(&meta_path)
                .map_err(|e| e.to_string())?
                .parse()
                .map_err(|e: toml::de::Error| e.to_string())?;
            for (key, want) in [("venue", venue), ("channel", channel)] {
                if meta.get(key).is_some_and(|v| v.as_str() != Some(want)) {
                    return Err(format!("{}: mismatched {key}", path.display()));
                }
            }
            let bytes = fs::read(&path).map_err(|e| e.to_string())?;
            let mut input: Vec<Vec<u8>> = bytes
                .split(|b| *b == b'\n')
                .map(|s| s.strip_suffix(b"\r").unwrap_or(s).to_vec())
                .collect();
            if input.last().is_some_and(Vec::is_empty) {
                input.pop();
            }
            if input.is_empty() || input.iter().any(Vec::is_empty) {
                return Err(format!("{}: empty input frame", path.display()));
            }
            let expected_path = path.with_extension("expected.jsonl");
            let text = fs::read_to_string(&expected_path).map_err(|e| e.to_string())?;
            let rows: Vec<Value> = text
                .lines()
                .map(serde_json::from_str)
                .collect::<Result<_, _>>()
                .map_err(|e| e.to_string())?;
            let counts = if let Some(counts) = meta.get("events_per_frame") {
                counts
                    .as_array()
                    .ok_or("events_per_frame must be an array")?
                    .iter()
                    .map(|v| {
                        v.as_integer()
                            .and_then(|n| usize::try_from(n).ok())
                            .ok_or("invalid frame event count")
                    })
                    .collect::<Result<Vec<_>, _>>()?
            } else if input.len() == 1 {
                vec![rows.len()]
            } else if rows.len() == input.len() {
                vec![1; input.len()]
            } else {
                return Err(format!("{}: specify events_per_frame", meta_path.display()));
            };
            if counts.len() != input.len()
                || counts.iter().try_fold(0usize, |a, b| a.checked_add(*b)) != Some(rows.len())
            {
                return Err("events_per_frame does not match input/expectations".into());
            }
            let mut offset = 0;
            let expected = counts
                .into_iter()
                .map(|n| {
                    let r = rows[offset..offset + n].to_vec();
                    offset += n;
                    r
                })
                .collect();
            Ok(GoldenCase {
                id: path
                    .strip_prefix(&base)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .replace('\\', "/"),
                venue: venue.into(),
                channel: channel.into(),
                meta,
                input,
                expected,
            })
        })
        .collect()
}

/// The adapter owns the protocol contexts and renders canonical events/errors/hints. It is called
/// exactly once per frame; state is retained between frames. This function performs no writes.
pub fn run_decoder_case(
    case: &GoldenCase,
    mut decode: impl FnMut(&[u8]) -> Vec<Value>,
) -> Result<(), GoldenDiff> {
    if case.input.len() != case.expected.len() {
        return Err(GoldenDiff {
            frame: 0,
            event: 0,
            field: "frames".into(),
            expected: case.expected.len().to_string(),
            actual: case.input.len().to_string(),
        });
    }
    for (frame, (input, expected)) in case.input.iter().zip(&case.expected).enumerate() {
        let actual = decode(input);
        for event in 0..expected.len().max(actual.len()) {
            let diff = match (expected.get(event), actual.get(event)) {
                (Some(e), Some(a)) => first_diff(e, a, ""),
                (e, a) => Some((
                    "event".into(),
                    e.map_or("<missing>".into(), Value::to_string),
                    a.map_or("<missing>".into(), Value::to_string),
                )),
            };
            if let Some((field, expected, actual)) = diff {
                let render = |s: String| {
                    let exponent = case
                        .meta
                        .get("exponents")
                        .and_then(|v| v.get(&field))
                        .and_then(toml::Value::as_integer);
                    match (s.trim_matches('"').parse::<i128>(), exponent) {
                        (Ok(raw), Some(exp @ -18..=0)) => {
                            let scale = 10i128.pow((-exp) as u32);
                            let mag = raw.unsigned_abs();
                            format!(
                                "{s} ({}{}.{:0width$})",
                                if raw < 0 { "-" } else { "" },
                                mag / scale as u128,
                                mag % scale as u128,
                                width = (-exp) as usize
                            )
                        }
                        _ => s,
                    }
                };
                return Err(GoldenDiff {
                    frame,
                    event,
                    field: field.clone(),
                    expected: render(expected),
                    actual: render(actual),
                });
            }
        }
    }
    Ok(())
}

fn first_diff(e: &Value, a: &Value, path: &str) -> Option<(String, String, String)> {
    match (e, a) {
        (Value::Object(e), Value::Object(a)) => {
            let keys: BTreeSet<_> = e.keys().chain(a.keys()).collect();
            for key in keys {
                if ["rx_tsc", "hw_rx_ns", "sw_rx_ns", "rx_empty_gap_ns", "conn"].contains(&key.as_str()) {
                    continue;
                }
                let field = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                match (e.get(key), a.get(key)) {
                    (Some(e), Some(a)) => {
                        if let Some(d) = first_diff(e, a, &field) {
                            return Some(d);
                        }
                    }
                    (e, a) => {
                        return Some((
                            field,
                            e.map_or("<missing>".into(), Value::to_string),
                            a.map_or("<missing>".into(), Value::to_string),
                        ));
                    }
                }
            }
            None
        }
        (Value::Array(e), Value::Array(a)) if e.len() == a.len() => e
            .iter()
            .zip(a)
            .enumerate()
            .find_map(|(i, (e, a))| first_diff(e, a, &format!("{path}[{i}]"))),
        _ if e == a => None,
        _ => Some((path.into(), e.to_string(), a.to_string())),
    }
}
