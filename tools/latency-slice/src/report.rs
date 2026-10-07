//! `--report` (README "产出"): hft-telemetry over the seq journal — the per-sample stages
//! (`Telemetry`), the event-weighted statistics with strata and the `kernel_rx` upper bound
//! (`event_weighted`), and the probe samples by stratum (`stratified_probe`).

use std::collections::BTreeMap;
use std::path::Path;

use hft_event::{EventKind, OrderAction, OrderDecision, OrderEvent, OrderEventKind, RecordHeader};
use hft_journal::{JournalReader, Next};
use hft_refdata::RefData;
use hft_telemetry::{
    Dataset, EventWeightedCfg, Quantiles, StrategyCatalog, Telemetry, TelemetryConfig, event_weighted,
    stratified_probe,
};
use serde_json::{Value, json};
use zerocopy::TryFromBytes;

use crate::journal;

/// Report thresholds (README "配置文件" `report.*`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ReportCfg {
    pub burst_gap_ns: u32,
    pub cal_max_err_ns: u32,
}

/// Record counts of the seq journal.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct Counts {
    pub records: u64,
    pub bbo: u64,
    pub decisions_new: u64,
    pub decisions_cancel: u64,
    pub acks: u64,
    pub rejects: u64,
    pub cancel_acks: u64,
    pub cancel_rejects: u64,
    pub outcome_unknown: u64,
    pub latency_samples: u64,
    pub clock_status: u64,
    pub send_reports: u64,
}

const BLOB_HDR: usize = 72;

/// The refdata restated in the seq journal's boot block.
fn refdata(rd: &mut JournalReader) -> Result<RefData, String> {
    loop {
        let (kind, body) = match rd.poll() {
            Next::Record(v) => (EventKind(v.hdr.kind), v.body),
            Next::Event(e) => (e.kind, e.body),
            Next::Pending | Next::End => return Err("no refdata ConfigBlob in the seq journal".into()),
            Next::Gap(g) => return Err(format!("journal gap: {g:?}")),
            Next::Err(e) => return Err(format!("journal read error: {e:?}")),
        };
        if kind == EventKind::CONFIG_BLOB && body.len() > BLOB_HDR {
            let blob = body.get(BLOB_HDR..).unwrap_or(&[]);
            return RefData::load(blob, None).map_err(|e| format!("refdata in journal rejected: {e:?}"));
        }
    }
}

fn count(c: &mut Counts, hdr: &RecordHeader, body: &[u8]) {
    c.records += 1;
    match EventKind(hdr.kind) {
        EventKind::BBO => c.bbo += 1,
        EventKind::ORDER_DECISION => {
            if let Ok((d, _)) = OrderDecision::try_read_from_prefix(body) {
                match d.action {
                    OrderAction::New => c.decisions_new += 1,
                    OrderAction::Cancel => c.decisions_cancel += 1,
                    _ => {}
                }
            }
        }
        EventKind::ORDER_EVENT => {
            if let Ok((e, _)) = OrderEvent::try_read_from_prefix(body) {
                match e.what {
                    OrderEventKind::Ack => c.acks += 1,
                    OrderEventKind::Reject => c.rejects += 1,
                    OrderEventKind::CancelAck | OrderEventKind::Expired => c.cancel_acks += 1,
                    OrderEventKind::CancelReject => c.cancel_rejects += 1,
                    OrderEventKind::OutcomeUnknown => c.outcome_unknown += 1,
                    _ => {}
                }
            }
        }
        EventKind::LATENCY_SAMPLE => c.latency_samples += 1,
        EventKind::CLOCK_STATUS => c.clock_status += 1,
        EventKind::SEND_REPORT => c.send_reports += 1,
        _ => {}
    }
}

/// The report of the seq journal under `hot`/`cold` (hot may be absent: cold copies only).
pub fn build(hot: &Path, cold: &Path, rc: &ReportCfg, run: Option<Value>) -> Result<Value, String> {
    let mut r0 = journal::reader(hot, cold, "seq")?;
    let rd = refdata(&mut r0)?;
    let tc = TelemetryConfig {
        max_series: 4096,
        cal_max_err_ns: rc.cal_max_err_ns,
        dataset: Dataset::Live,
        ..TelemetryConfig::default()
    };
    let mut tel = Telemetry::new(&tc, &rd, &StrategyCatalog::empty());
    let mut counts = Counts::default();
    let mut r1 = journal::reader(hot, cold, "seq")?;
    loop {
        match r1.poll() {
            Next::Record(v) => {
                count(&mut counts, &v.hdr, v.body);
                tel.ingest(&v.hdr, v.body);
            }
            Next::Event(_) => counts.records += 1,
            Next::Pending | Next::End => break,
            Next::Gap(g) => return Err(format!("journal gap: {g:?}")),
            Next::Err(e) => return Err(format!("journal read error: {e:?}")),
        }
    }
    tel.finish();
    let day = tel.day_close(0);
    let ec = EventWeightedCfg {
        from_seq: 0,
        to_seq: u64::MAX,
        cal_max_err_ns: rc.cal_max_err_ns,
        burst_gap_ns: rc.burst_gap_ns,
        max_series: 4096,
    };
    let mut r2 = journal::reader(hot, cold, "seq")?;
    let ew = event_weighted(&mut r2, &rd, &ec).map_err(|e| e.to_string())?;
    let strat = match ew.first() {
        Some(first) => {
            let mut r3 = journal::reader(hot, cold, "seq")?;
            Some(stratified_probe(&mut r3, &rd, &ec, first).map_err(|e| e.to_string())?)
        }
        None => None,
    };
    Ok(json!({
        "run": run.unwrap_or(Value::Null),
        "journal": counts,
        "samples": {
            "hist": day.hist.iter().map(|(k, q)| json!({"key": k, "q": q})).collect::<Vec<_>>(),
            "cal_err": day.cal_err.iter().map(|(k, q)| json!({"key": k, "q": q})).collect::<Vec<_>>(),
            "counters": day.counters.iter().map(|(k, n)| json!({"key": k, "n": n})).collect::<Vec<_>>(),
        },
        "event_weighted": ew,
        "stratified": strat,
        "thresholds": {"burst_gap_ns": rc.burst_gap_ns, "cal_max_err_ns": rc.cal_max_err_ns},
    }))
}

fn us(v: i64) -> String {
    format!("{:.1}", v as f64 / 1_000.0)
}

fn opt_us(v: Option<i64>) -> String {
    v.map_or_else(|| "n/a".to_string(), us)
}

fn qline(q: &Quantiles) -> String {
    format!(
        "n={:<8} p50={:>9} p90={:>9} p99={:>9} p99.9={:>9} max={:>9} µs",
        q.count,
        us(q.p50),
        us(q.p90),
        opt_us(q.p99),
        opt_us(q.p999),
        us(q.max)
    )
}

/// The human summary of a report value.
pub fn text(v: &Value) -> String {
    let mut out = String::new();
    out.push_str("# latency-slice report\n\n");
    if !v["run"].is_null() {
        out.push_str(&format!("run: {}\n\n", v["run"]));
    }
    out.push_str(&format!("journal: {}\n\n", v["journal"]));
    out.push_str("## samples (LatencySample, per stage / ts_src / action / trigger)\n");
    let mut rows: BTreeMap<String, String> = BTreeMap::new();
    if let Some(h) = v["samples"]["hist"].as_array() {
        for e in h {
            let key = &e["key"];
            let name = format!(
                "{:<14} {:<8} action={} trigger={}",
                key["stage"].as_str().unwrap_or("?"),
                key["ts_src"].as_str().unwrap_or("?"),
                key["action"],
                key["trigger_kind"]
            );
            if let Ok(q) = serde_json::from_value::<QuantilesJson>(e["q"].clone()) {
                rows.insert(name.clone(), qline(&q.into()));
            }
        }
    }
    for (k, l) in rows {
        out.push_str(&format!("{k}  {l}\n"));
    }
    out.push_str("\n## event-weighted market records (per stage / ts_src / stratum)\n");
    if let Some(ds) = v["event_weighted"].as_array() {
        for d in ds {
            for (sec, label) in [("hist", ""), ("kernel_rx_hi", " (upper bound)")] {
                if let Some(h) = d[sec].as_array() {
                    for e in h {
                        let key = &e[0];
                        if let Ok(q) = serde_json::from_value::<QuantilesJson>(e[1].clone()) {
                            out.push_str(&format!(
                                "{:<14}{label} {:<8} {:<6} {}\n",
                                key["stage"].as_str().unwrap_or("?"),
                                key["ts_src"].as_str().unwrap_or("?"),
                                key["stratum"].as_str().unwrap_or("?"),
                                qline(&q.into())
                            ));
                        }
                    }
                }
            }
            out.push_str(&format!(
                "n={} missing={} gap_missing={} unsynced_cal={}\n",
                d["n"], d["missing"], d["gap_missing"], d["unsynced_cal"]
            ));
        }
    }
    out
}

#[derive(serde::Deserialize)]
struct QuantilesJson {
    count: u64,
    p50: i64,
    p90: i64,
    p99: Option<i64>,
    p999: Option<i64>,
    max: i64,
}

impl From<QuantilesJson> for Quantiles {
    fn from(q: QuantilesJson) -> Quantiles {
        Quantiles {
            count: q.count,
            p50: q.p50,
            p90: q.p90,
            p99: q.p99,
            p999: q.p999,
            max: q.max,
        }
    }
}

/// Writes `report.json` and `report.txt` into `out`.
pub fn write(out: &Path, v: &Value) -> Result<(), String> {
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let j = serde_json::to_string_pretty(v).map_err(|e| e.to_string())?;
    std::fs::write(out.join("report.json"), j).map_err(|e| e.to_string())?;
    std::fs::write(out.join("report.txt"), text(v)).map_err(|e| e.to_string())?;
    Ok(())
}
