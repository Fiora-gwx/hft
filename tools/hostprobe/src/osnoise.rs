//! `hostprobe osnoise --cpus <list> --duration <d>`: wraps `rtla osnoise hist` and reports, per
//! isolated core, the maximum and the p99.9 noise sample (tools/README.md "hostprobe").

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::host::Host;

/// Per-CPU result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuNoise {
    pub samples: u64,
    pub max_ns: u64,
    pub p999_ns: u64,
}

/// Parses `rtla osnoise hist` output (1 µs buckets): `Index CPU-002 …` header, one row per bucket,
/// then `over:`, `count:`, `min:`, `avg:`, `max:` rows.
pub fn parse_hist(text: &str) -> Result<BTreeMap<u32, CpuNoise>, String> {
    let mut cpus: Vec<u32> = Vec::new();
    let mut buckets: Vec<(u64, Vec<u64>)> = Vec::new();
    let mut over: Vec<u64> = Vec::new();
    let mut max: Vec<u64> = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        let mut f = l.split_whitespace();
        let head = f.next().unwrap_or("");
        let nums = |f: std::str::SplitWhitespace| -> Vec<u64> { f.filter_map(|x| x.parse().ok()).collect() };
        match head {
            "Index" => {
                cpus = f
                    .filter_map(|c| c.strip_prefix("CPU-").and_then(|n| n.parse().ok()))
                    .collect();
            }
            "over:" => over = nums(f),
            "max:" => max = nums(f),
            "count:" | "min:" | "avg:" => {}
            idx => {
                if let Ok(i) = idx.parse::<u64>() {
                    buckets.push((i, nums(f)));
                }
            }
        }
    }
    if cpus.is_empty() {
        return Err("rtla output has no `Index CPU-…` header".into());
    }
    let mut out = BTreeMap::new();
    for (k, cpu) in cpus.iter().enumerate() {
        let col: Vec<(u64, u64)> = buckets
            .iter()
            .map(|(i, v)| (*i, v.get(k).copied().unwrap_or(0)))
            .collect();
        let ov = over.get(k).copied().unwrap_or(0);
        let total: u64 = col.iter().map(|(_, c)| c).sum::<u64>() + ov;
        let mx = max.get(k).copied().unwrap_or(0);
        // Smallest bucket whose cumulative count reaches 99.9 % of the samples.
        let need = (total * 999).div_ceil(1000);
        let mut cum = 0;
        let mut p999 = mx;
        for (i, c) in &col {
            cum += c;
            if cum >= need {
                p999 = *i;
                break;
            }
        }
        out.insert(
            *cpu,
            CpuNoise {
                samples: total,
                max_ns: mx * 1000,
                p999_ns: p999 * 1000,
            },
        );
    }
    Ok(out)
}

/// Runs `rtla osnoise hist -c <cpus> -d <duration>` and returns the report.
pub fn run(h: &mut dyn Host, cpus: &str, duration: &str) -> Result<Value, String> {
    let text = h.exec("rtla", &["osnoise", "hist", "-c", cpus, "-d", duration])?;
    let res = parse_hist(&text)?;
    let per: serde_json::Map<String, Value> = res
        .into_iter()
        .map(|(c, n)| {
            (
                c.to_string(),
                json!({"samples": n.samples, "max_ns": n.max_ns, "p999_ns": n.p999_ns}),
            )
        })
        .collect();
    Ok(json!({"tool": "rtla osnoise hist", "cpus": cpus, "duration": duration, "per_cpu": per}))
}
