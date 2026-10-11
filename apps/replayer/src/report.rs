use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use hft_event::{EventKind, Layout, Role, SegmentRoll, kind_info};
use hft_sequencer::driver::recompute::RecomputeResult;
use hft_sequencer::driver::snapshot_diff::snapshot_difference;
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

fn snapshot_detail(expected: &[u8], actual: &[u8], fields: &mut Vec<Value>) -> Value {
    let kind = EventKind::STATE_SNAPSHOT.0.to_le_bytes();
    if expected.get(..2) != Some(kind.as_slice()) || actual.get(..2) != Some(kind.as_slice()) {
        return Value::Null;
    }
    match snapshot_difference(&expected[2..], &actual[2..]) {
        Err(e) => {
            json!({"error": {"side": format!("{:?}", e.side), "offset": e.offset, "reason": format!("{:?}", e.reason)}})
        }
        Ok(d) => {
            let first = d.first.map(|f| {
                json!({
                    "component": f.component, "section": f.section, "row": f.row,
                    "field": f.field, "layout": f.layout,
                    "expected_offset": f.logged_offset, "actual_offset": f.regenerated_offset,
                    "expected_hex": hex(f.logged), "actual_hex": hex(f.regenerated)
                })
            });
            if let Some(f) = &first {
                fields.push(f.clone());
            }
            json!({
                "expected_state_hash": hex(&d.logged_state_hash),
                "actual_state_hash": hex(&d.regenerated_state_hash),
                "components": d.components.iter().map(|h| json!({
                    "component": h.component, "expected": format!("{:016x}", h.logged),
                    "actual": format!("{:016x}", h.regenerated)
                })).collect::<Vec<_>>(),
                "first": first
            })
        }
    }
}

pub(crate) fn format(
    instance: &str,
    result: &VerifyResult,
    model: Option<&RecomputeResult>,
) -> (Value, Value) {
    let sink = &result.sink;
    let describe = |m: &hft_sequencer::driver::Mismatch,
                    expected: &[u8],
                    actual: &[u8],
                    (expected_flags, actual_flags): (Option<u16>, Option<u16>)| {
        let mut diff = field_diff(expected, actual);
        let snapshot = snapshot_detail(expected, actual, &mut diff);
        if expected_flags != actual_flags {
            diff.push(json!({"field": "flags", "expected": expected_flags, "actual": actual_flags}));
        }
        let prior: Vec<_> = sink
            .log
            .recs
            .iter()
            .filter(|r| r.seq < m.seq && r.seq > result.start_snapshot && r.role() == Role::Input)
            .rev()
            .take(1000)
            .map(|r| json!({"seq": r.seq, "kind": r.kind.0}))
            .collect();
        let prior: Vec<_> = prior.into_iter().rev().collect();
        let nearest_snapshot = sink
            .log
            .recs
            .iter()
            .rev()
            .find(|r| r.seq < m.seq && r.kind == EventKind::STATE_SNAPSHOT)
            .map(|r| r.seq);
        let mut detail = json!({"seq": m.seq, "kind": m.kind.0, "kind_name": kind_info(m.kind).map(|k| k.name), "expected_hex": hex(expected), "actual_hex": hex(actual), "expected_flags": expected_flags, "actual_flags": actual_flags, "field_diff": diff, "previous_inputs": prior, "nearest_snapshot_seq": nearest_snapshot});
        if !snapshot.is_null() {
            detail["component"] = snapshot["first"]["component"].clone();
            detail["snapshot"] = snapshot;
        }
        detail
    };
    let details: Vec<_> = sink
        .mismatches
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let (expected, actual) = sink
                .detail
                .get(i)
                .filter(|d| d.0 == m.seq)
                .map_or((&[][..], &[][..]), |d| (d.1.as_slice(), d.2.as_slice()));
            describe(
                m,
                expected,
                actual,
                sink.detail_flags.get(i).copied().unwrap_or_default(),
            )
        })
        .collect();
    let recovery: Vec<_> = sink
        .recovery_diffs
        .iter()
        .map(|d| describe(&d.mismatch, &d.logged, &d.regenerated, d.flags))
        .collect();
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
    let runs: Vec<_> = result
        .runs
        .iter()
        .map(|r| {
            json!({
                "start_snapshot_seq": r.start_snapshot, "stop_before_seq": r.stop_before,
                "inputs_replayed": r.inputs, "outputs_compared": r.outputs,
                "snapshots_checked": r.snapshots, "mismatches": r.mismatches,
                "recovery_diffs": r.recovery_diffs
            })
        })
        .collect();
    let mut summary = json!({"instance": instance, "date": &date[..10], "release": crate::VERSION, "verify": if details.is_empty() { "pass" } else { "fail" }, "first_divergent_seq": sink.mismatches.first().map(|m| m.seq), "recompute": "skipped", "skipped_reason": "advice_journal", "generated_utc": utc(generated), "start_snapshot_seq": result.start_snapshot, "inputs_replayed": result.inputs, "outputs_compared": sink.compared, "snapshots_checked": result.snapshots, "mismatches": details.len(), "runs": runs});
    // Component belongs to the first ordinary divergence, never a later snapshot or an
    // accepted recovery difference. Non-snapshot and malformed/header-only cases have none.
    summary["component"] = details.first().map_or(Value::Null, |d| d["component"].clone());
    let mut detail = json!({"divergences": details, "recovery_diffs": recovery});
    summary["input_continuity"] = json!(if sink.log.recovery_gaps.is_empty() {
        "unbroken"
    } else {
        "recovery_gap"
    });
    summary["recovery_gaps"] = json!(
        sink.log
            .recovery_gaps
            .iter()
            .map(|g| json!({
                "segment_no": g.segment_no, "first_seq": g.first_seq,
                "prev_last_seq": g.prev_last_seq, "previous_segment_included": g.first > 0,
                "unavailable_seq_count": g.first_seq - g.prev_last_seq - 1,
                "tail_loss_unknown": true
            }))
            .collect::<Vec<_>>()
    );
    if let Some(m) = model {
        let model_first = m.differences.first().map(|d| d.seq);
        let output_first = sink.mismatches.first().map(|d| d.seq);
        // Model divergence affects the process exit code, not the independent Output verdict.
        summary["first_output_divergent_seq"] = json!(output_first);
        summary["first_model_divergent_seq"] = json!(model_first);
        summary["recompute"] = json!(if model_first.is_some() {
            "fail"
        } else if !m.model_no_start.is_empty() {
            "skipped"
        } else {
            "pass"
        });
        summary["skipped_reason"] = json!(if model_first.is_none() && !m.model_no_start.is_empty() {
            Some("model_no_start")
        } else {
            None
        });
        summary["model_advice_compared"] = json!(m.compared);
        if !m.started_instances.is_empty() || m.model_no_start.is_empty() {
            summary["coalesced"] = json!(m.coalesced);
        }
        summary["checkpoint_key"] = json!(m.checkpoint_keys.iter().map(|(name, k)| json!({
            "strategy_instance": name, "kind_id": k.kind_id, "layout_version": k.layout_version,
            "schema_id": k.schema_id, "strategy": k.strategy.0, "account": k.account.0, "last_seq": k.last_seq
        })).collect::<Vec<_>>());
        summary["recompute_from_seq"] = json!(m.recompute_from_seq);
        detail["model_no_start"] = json!(m.model_no_start);
        detail["model_divergences"] = json!(
            m.differences
                .iter()
                .map(|d| {
                    let mut logged = EventKind::ADVICE.0.to_le_bytes().to_vec();
                    logged.extend_from_slice(&d.logged);
                    let mut actual = Vec::new();
                    if let Some(body) = &d.regenerated {
                        actual.extend_from_slice(&EventKind::ADVICE.0.to_le_bytes());
                        actual.extend_from_slice(body);
                    }
                    json!({"seq": d.seq, "strategy": d.strategy.0, "account": d.account.0,
                "group": d.group.0, "local_kind": d.local_kind, "basis_seq": d.basis_seq,
                "expected_hex": hex(&d.logged), "actual_hex": d.regenerated.as_deref().map(hex),
                "field_diff": field_diff(&logged, &actual)})
                })
                .collect::<Vec<_>>()
        );
    }
    (summary, detail)
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
