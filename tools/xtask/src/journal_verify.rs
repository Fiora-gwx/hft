//! `cargo xtask journal-verify <dir> [--from-segment N] [--json]` (tools/README.md "其他 xtask 子命令";
//! checks: docs/subsystems/journal-format.md "校验与缺段检测", implemented by `hft_journal::verify`).
//!
//! A sealed segment shorter than its `.seal` length is reported at the offset where the file ends
//! (truncation point) instead of offset 0. `RECOVERY_GAP` segments are reported as "possible lost tail"
//! of the previous segment and do not fail. `--from-segment N` keeps segments ≥ N; `--json` also prints
//! the JSON report to stdout.

use std::collections::BTreeMap;
use std::path::Path;

use hft_journal::{VerifyOpts, verify};
use serde_json::{Value, json};
use walkdir::WalkDir;

use crate::{EXIT_ENV, EXIT_FAIL, EXIT_OK, EnvError, write_json};

/// Item names of journal-format "校验与缺段检测".
fn item_name(i: u8) -> &'static str {
    match i {
        1 => "segment header",
        2 => "segment chain",
        3 => "record",
        4 => "boot block",
        5 => "seal",
        _ => "other",
    }
}

#[derive(Debug, Default)]
struct Args {
    dir: String,
    from: u64,
    json: bool,
}

fn parse(args: &[String]) -> Result<Args, EnvError> {
    let mut a = Args::default();
    let mut it = args.iter();
    while let Some(x) = it.next() {
        match x.as_str() {
            "--json" => a.json = true,
            "--from-segment" => {
                let v = it
                    .next()
                    .ok_or_else(|| EnvError("--from-segment needs a value".into()))?;
                a.from = v
                    .parse()
                    .map_err(|_| EnvError(format!("bad --from-segment {v}")))?;
            }
            d if a.dir.is_empty() && !d.starts_with("--") => a.dir = d.to_string(),
            d => return Err(EnvError(format!("unexpected argument {d}"))),
        }
    }
    if a.dir.is_empty() {
        return Err(EnvError(
            "usage: journal-verify <dir> [--from-segment N] [--json]".into(),
        ));
    }
    Ok(a)
}

/// Segment files below `dir`: `segment_no → (jrn length, sealed length from .seal)`.
fn segment_files(dir: &Path) -> BTreeMap<u64, (u64, Option<u64>)> {
    let mut m = BTreeMap::new();
    let mut seals = BTreeMap::new();
    for e in WalkDir::new(dir).max_depth(2).into_iter().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some((stem, ext)) = name.rsplit_once('.') else {
            continue;
        };
        let Ok(no) = stem.split('-').next().unwrap_or("").parse::<u64>() else {
            continue;
        };
        match ext {
            "jrn" => {
                let len = e.metadata().map_or(0, |x| x.len());
                m.insert(no, (len, None));
            }
            "seal" => {
                if let Ok(b) = std::fs::read(e.path())
                    && let Some(x) = b.get(32..40)
                {
                    let mut a = [0u8; 8];
                    a.copy_from_slice(x);
                    seals.insert(no, u64::from_le_bytes(a));
                }
            }
            _ => {}
        }
    }
    for (no, len) in seals {
        if let Some(e) = m.get_mut(&no) {
            e.1 = Some(len);
        }
    }
    m
}

/// Verifies `dir`; returns `(exit code, report)`.
pub fn evaluate(dir: &Path, from: u64) -> (i32, Value) {
    let rep = verify(dir, &VerifyOpts::default());
    let files = segment_files(dir);
    let mut segs: BTreeMap<u64, Vec<Value>> = BTreeMap::new();
    for no in files.keys().filter(|n| **n >= from) {
        segs.insert(*no, Vec::new());
    }
    let mut bad = false;
    for i in &rep.issues {
        if i.segment_no < from {
            continue;
        }
        let mut offset = i.offset;
        let mut what = item_name(i.item).to_string();
        if i.item == 5
            && let Some((len, Some(sealed))) = files.get(&i.segment_no)
            && len < sealed
        {
            offset = *len;
            what = format!("seal: file truncated (sealed length {sealed} B)");
        }
        segs.entry(i.segment_no).or_default().push(json!({
            "offset": offset, "item": i.item, "what": what,
        }));
        bad = true;
    }
    let missing: Vec<(u64, u64)> = rep
        .missing
        .iter()
        .filter(|(_, b)| *b >= from)
        .map(|(a, b)| ((*a).max(from), *b))
        .collect();
    bad |= !missing.is_empty();
    let gaps: Vec<u64> = rep.recovery_gaps.iter().copied().filter(|n| *n >= from).collect();
    let unsealed: Vec<u64> = rep.unsealed.iter().copied().filter(|n| *n >= from).collect();
    let seg_json: Vec<Value> = segs
        .iter()
        .map(|(no, is)| {
            json!({
                "segment_no": no,
                "ok": is.is_empty(),
                "issues": is,
                "recovery_gap": gaps.contains(no),
                "sealed": !unsealed.contains(no),
            })
        })
        .collect();
    let code = if bad { EXIT_FAIL } else { EXIT_OK };
    let j = json!({
        "dir": dir.display().to_string(),
        "from_segment": from,
        "segments": seg_json,
        "missing": missing.iter().map(|(a, b)| json!([a, b])).collect::<Vec<_>>(),
        "recovery_gaps": gaps,
        "unsealed": unsealed,
        "result": if bad { "fail" } else { "pass" },
    });
    (code, j)
}

/// Entry point.
pub fn run(root: &Path, args: &[String]) -> i32 {
    let a = match parse(args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("journal-verify: error: {e}");
            return EXIT_ENV;
        }
    };
    let dir = Path::new(&a.dir);
    if !dir.is_dir() {
        eprintln!("journal-verify: error: {} is not a directory", a.dir);
        return EXIT_ENV;
    }
    let (code, j) = evaluate(dir, a.from);
    for s in j["segments"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
        for i in s["issues"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
            eprintln!(
                "segment {} offset {}: {}",
                s["segment_no"],
                i["offset"],
                i["what"].as_str().unwrap_or("")
            );
        }
    }
    for m in j["missing"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
        eprintln!("missing segments {}..={}", m[0], m[1]);
    }
    for g in j["recovery_gaps"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
        eprintln!("segment {g}: RECOVERY_GAP — possible lost tail of the previous segment (可能丢失的尾部)");
    }
    if let Err(e) = write_json(root, "journal-verify", &j) {
        eprintln!("journal-verify: error: {e}");
        return EXIT_ENV;
    }
    if a.json {
        println!("{}", serde_json::to_string_pretty(&j).unwrap_or_default());
    }
    eprintln!(
        "journal-verify: -> {}",
        if code == EXIT_OK { "PASS" } else { "FAIL" }
    );
    code
}
