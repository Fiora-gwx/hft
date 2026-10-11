use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use hft_event::{EventKind, Layout, Role, SegmentRoll, kind_info};
use hft_sequencer::driver::verify::VerifyResult;
use serde_json::{Value, json};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// Gregorian UTC formatting for report metadata only. No wall-clock value enters replay.
fn utc(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let sec = seconds.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = year + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        sec / 3600,
        (sec % 3600) / 60,
        sec % 60
    )
}

fn field_diff(expected: &[u8], actual: &[u8]) -> Vec<Value> {
    let mut out = Vec::new();
    if expected.get(..2) != actual.get(..2) {
        out.push(json!({"field": "kind", "expected_hex": hex(expected.get(..2).unwrap_or(&[])), "actual_hex": hex(actual.get(..2).unwrap_or(&[]))}));
    }
    let a = expected.get(2..).unwrap_or(&[]);
    let b = actual.get(2..).unwrap_or(&[]);
    let kind = expected
        .get(..2)
        .or_else(|| actual.get(..2))
        .map(|k| EventKind(u16::from_le_bytes([k[0], k[1]])));
    if let Some(info) = kind.and_then(kind_info) {
        for f in info.fields {
            let start = usize::from(f.offset);
            let end = start + usize::from(f.ty.width());
            let slice = |x: &[u8]| x.get(start..end.min(x.len())).unwrap_or(&[]).to_vec();
            let (x, y) = (slice(a), slice(b));
            if x != y {
                out.push(
                    json!({"field": f.name, "offset": start, "expected_hex": hex(&x), "actual_hex": hex(&y)}),
                );
            }
        }
        let prefix = usize::from(match info.layout {
            Layout::Fixed { size } => size,
            Layout::Var { prefix, .. } | Layout::Multi { prefix } => prefix,
        });
        let (x, y) = (a.get(prefix..).unwrap_or(&[]), b.get(prefix..).unwrap_or(&[]));
        if x != y {
            out.push(
                json!({"field": "payload", "offset": prefix, "expected_hex": hex(x), "actual_hex": hex(y)}),
            );
        }
    }
    if a.len() != b.len() {
        out.push(json!({"field": "body_len", "expected": a.len(), "actual": b.len()}));
    }
    out
}

pub(crate) fn format(instance: &str, result: &VerifyResult) -> (Value, Value) {
    let sink = &result.sink;
    let details: Vec<Value> = sink.mismatches.iter().enumerate().map(|(i, m)| {
        let (expected, actual) = sink.detail.get(i).filter(|d| d.0 == m.seq).map_or((&[][..], &[][..]), |d| (d.1.as_slice(), d.2.as_slice()));
        let (expected_flags, actual_flags) = sink.detail_flags.get(i).copied().unwrap_or_default();
        let mut diff = field_diff(expected, actual);
        if expected_flags != actual_flags { diff.push(json!({"field": "flags", "expected": expected_flags, "actual": actual_flags})); }
        let prior: Vec<_> = sink.log.recs.iter().filter(|r| r.seq < m.seq && r.seq > result.start_snapshot && r.role() == Role::Input).rev().take(1000).map(|r| json!({"seq": r.seq, "kind": r.kind.0})).collect();
        let prior: Vec<_> = prior.into_iter().rev().collect();
        let snapshot = sink.log.recs.iter().rev().find(|r| r.seq < m.seq && r.kind == EventKind::STATE_SNAPSHOT).map(|r| r.seq);
        json!({"seq": m.seq, "kind": m.kind.0, "kind_name": kind_info(m.kind).map(|k| k.name), "expected_hex": hex(expected), "actual_hex": hex(actual), "expected_flags": expected_flags, "actual_flags": actual_flags, "field_diff": diff, "previous_inputs": prior, "nearest_snapshot_seq": snapshot})
    }).collect();
    let utc_ns = sink
        .log
        .recs
        .first()
        .and_then(|r| r.read::<SegmentRoll>())
        .map_or(0, |r| r.utc_ns);
    let date = utc(utc_ns.div_euclid(1_000_000_000));
    let generated = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs().min(i64::MAX as u64) as i64);
    let summary = json!({"instance": instance, "date": &date[..10], "release": crate::VERSION, "verify": if details.is_empty() { "pass" } else { "fail" }, "first_divergent_seq": sink.mismatches.first().map(|m| m.seq), "recompute": "skipped", "skipped_reason": "advice_journal", "generated_utc": utc(generated), "start_snapshot_seq": result.start_snapshot, "inputs_replayed": result.inputs, "outputs_compared": sink.compared, "snapshots_checked": result.snapshots, "mismatches": details.len()});
    (summary, json!(details))
}

pub(crate) fn divergence_path(path: &Path) -> PathBuf {
    path.with_extension("divergence.json")
}
pub(crate) fn write(path: &Path, value: &Value) -> std::io::Result<()> {
    use std::io::Write;
    // create_new refuses to follow an existing symlink or overwrite an unrelated temp file.
    let temp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    let outcome = (|| {
        f.write_all(&serde_json::to_vec_pretty(value)?)?;
        f.write_all(b"\n")?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&temp, path)
    })();
    if outcome.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    outcome
}
