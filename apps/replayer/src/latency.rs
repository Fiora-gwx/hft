//! Read-only journal/telemetry assembly. No strategy registry or sequencer is consulted.
use crate::Failure;
use hdrhistogram::{
    Histogram,
    serialization::{Serializer, V2Serializer},
};
use hft_event::{BlobHdr, BlobKind, EventKind, HostProfile};
use hft_journal::{JournalKind, JournalReader, Next};
use hft_refdata::RefData;
use hft_telemetry::{EventWeightedCfg, EwKey, Stage, TsSrc, event_weighted_with, stratified_probe};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::PathBuf,
};
use zerocopy::{FromBytes, TryFromBytes};

pub(crate) fn execute(args: &[String], stdout: &mut impl Write) -> Result<i32, Failure> {
    let mut opts = BTreeMap::new();
    if args.len() % 2 != 1 {
        return Err((2, "latency options require values".into()));
    }
    for pair in args[1..].chunks_exact(2) {
        if !["--journal", "--out", "--cal-max-err-ns", "--burst-gap-ns"].contains(&pair[0].as_str())
            || opts.insert(pair[0].as_str(), pair[1].as_str()).is_some()
        {
            return Err((2, "unsupported or duplicate latency option".into()));
        }
    }
    let required = |key| {
        opts.get(key)
            .copied()
            .filter(|v| !v.is_empty())
            .ok_or((2, format!("{key} required")))
    };
    let journal = PathBuf::from(required("--journal")?);
    let out = PathBuf::from(required("--out")?);
    let number = |key| {
        required(key)?
            .parse::<u32>()
            .ok()
            .filter(|n| *n > 0)
            .ok_or((2, format!("{key} requires a positive u32")))
    };
    let cfg = EventWeightedCfg {
        from_seq: 0,
        to_seq: u64::MAX,
        cal_max_err_ns: number("--cal-max-err-ns")?,
        burst_gap_ns: number("--burst-gap-ns")?,
        max_series: 4096,
    };
    let mut files = Vec::new();
    crate::discover(&journal, &mut files)?;
    let open = || JournalReader::open_files(&files, 128 << 20).map_err(|e| (3, format!("input: {e:?}")));
    let mut reader = open()?;
    if reader
        .file_segments()
        .is_none_or(|ss| ss.iter().any(|s| s.header.journal_kind != JournalKind::Seq as u8))
    {
        return Err((3, "latency requires a seq journal".into()));
    }
    let mut rdata = None;
    let mut profiles: Vec<(u64, [u8; 32], [u8; 16])> = Vec::new();
    loop {
        let (seq, kind, body) = match reader.poll() {
            Next::Record(r) => (r.hdr.seq, EventKind(r.hdr.kind), r.body),
            Next::Event(e) => (e.seq, e.kind, e.body),
            Next::End => break,
            Next::Pending => return Err((3, "input is not complete".into())),
            Next::Gap(g) => return Err((3, format!("journal gap: {g:?}"))),
            Next::Err(e) => return Err((3, format!("journal read: {e:?}"))),
        };
        if kind == EventKind::CONFIG_BLOB {
            let (hdr, content) =
                BlobHdr::try_read_from_prefix(body).map_err(|_| (3, "invalid ConfigBlob".into()))?;
            if hdr.blob.kind == BlobKind::Refdata {
                let rd = RefData::load(content, None).map_err(|e| (3, format!("refdata: {e:?}")))?;
                // Refdata hot replacement is outside this command's current scope. Do not
                // silently use one exponent table for a journal containing different tables.
                if rdata.as_ref().is_some_and(|(_, hash)| *hash != hdr.blob.sha256) {
                    return Err((3, "refdata changes inside latency input".into()));
                }
                rdata = Some((rd, hdr.blob.sha256));
            }
        } else if kind == EventKind::HOST_PROFILE {
            let (p, _) =
                HostProfile::read_from_prefix(body).map_err(|_| (3, "invalid HostProfile".into()))?;
            if profiles
                .last()
                .is_none_or(|(_, hash, _)| *hash != p.profile_sha256)
            {
                profiles.push((seq, p.profile_sha256, p.release));
            }
        }
    }
    let (rdata, _) = rdata.ok_or((3, "no journal refdata".into()))?;
    let mut histograms: BTreeMap<(usize, EwKey, bool), Histogram<u64>> = BTreeMap::new();
    let mut hist_error = false;
    let reports = event_weighted_with(&mut open()?, &rdata, &cfg, |sample| {
        let dataset = profiles
            .partition_point(|(seq, _, _)| *seq <= sample.seq)
            .saturating_sub(1);
        let h = histograms
            .entry((dataset, sample.key, sample.hi))
            .or_insert_with(|| {
                Histogram::new_with_bounds(1, 10_000_000_000, 3).expect("constant histogram bounds")
            });
        if sample.value < 0 || h.record(sample.value as u64).is_err() {
            hist_error = true;
        }
    })
    .map_err(|e| (3, e.to_string()))?;
    if hist_error {
        return Err((3, "sample exceeds histogram bounds".into()));
    }
    if reports.is_empty() {
        return Err((2, "empty latency window".into()));
    }
    let mut datasets = Vec::new();
    let mut binaries = Vec::new();
    for (index, report) in reports.iter().enumerate() {
        let mut bytes = Vec::new();
        let mut series = Vec::new();
        for ((dataset, key, hi), hist) in &histograms {
            if *dataset != index {
                continue;
            }
            let offset = bytes.len();
            V2Serializer::new()
                .serialize(hist, &mut bytes)
                .map_err(|e| (2, e.to_string()))?;
            series.push(json!({"key":key,"upper_bound":hi,"offset":offset,"bytes":bytes.len()-offset}));
        }
        let sw: u64 = report
            .hist
            .iter()
            .filter(|(k, _)| k.stage == Stage::Ingest && k.ts_src == TsSrc::Sw)
            .map(|(_, q)| q.count)
            .sum();
        let mut dataset = serde_json::to_value(report).map_err(|e| (2, e.to_string()))?;
        dataset["groups"] = json!(
            report
                .hist
                .iter()
                .map(|(key, q)| json!({"key":key,"q":q}))
                .collect::<Vec<_>>()
        );
        dataset
            .as_object_mut()
            .ok_or((2, "report format".into()))?
            .remove("hist");
        dataset["sw_anchor_share"] = json!(if report.n == 0 {
            0.0
        } else {
            sw as f64 / report.n as f64
        });
        dataset["histograms"] = json!(series);
        // The telemetry implementation owns stratification and weighting.
        let probe_cfg = EventWeightedCfg {
            from_seq: profiles.get(index).map_or(0, |p| p.0),
            to_seq: profiles
                .get(index + 1)
                .map_or(u64::MAX, |p| p.0.saturating_sub(1)),
            ..cfg
        };
        dataset["probe_strata"] = serde_json::to_value(
            stratified_probe(&mut open()?, &rdata, &probe_cfg, report).map_err(|e| (3, e.to_string()))?,
        )
        .map_err(|e| (2, e.to_string()))?;
        datasets.push(dataset);
        binaries.push(bytes);
    }
    // Validate all input before creating output. Reject symlinks and input aliases.
    std::fs::create_dir_all(&out).map_err(|e| (2, e.to_string()))?;
    let out = std::fs::canonicalize(out).map_err(|e| (2, e.to_string()))?;
    let inputs: BTreeSet<_> = files
        .iter()
        .map(std::fs::canonicalize)
        .collect::<Result<_, _>>()
        .map_err(|e| (3, e.to_string()))?;
    let names: Vec<_> = (0..binaries.len())
        .map(|i| format!("latency-{i}.hdr"))
        .chain(["latency.json".into()])
        .collect();
    for name in &names {
        let dest = out.join(name);
        if std::fs::symlink_metadata(&dest).is_ok_and(|m| !m.is_file() || m.file_type().is_symlink())
            || inputs.contains(&dest)
        {
            return Err((2, "unsafe latency output path".into()));
        }
    }
    for (i, bytes) in binaries.iter().enumerate() {
        std::fs::write(out.join(&names[i]), bytes).map_err(|e| (2, e.to_string()))?;
    }
    let result = json!({"cfg":{"from_seq":cfg.from_seq,"to_seq":cfg.to_seq,"cal_max_err_ns":cfg.cal_max_err_ns,"burst_gap_ns":cfg.burst_gap_ns},"datasets":datasets});
    std::fs::write(
        out.join("latency.json"),
        serde_json::to_vec_pretty(&result).map_err(|e| (2, e.to_string()))?,
    )
    .map_err(|e| (2, e.to_string()))?;
    writeln!(stdout, "{} datasets", reports.len()).map_err(|e| (2, e.to_string()))?;
    Ok(0)
}
