use std::path::PathBuf;

use hft_sequencer::driver::recompute::RecomputeLimits;
use hft_sequencer::driver::replay_io::LogReadLimits;
use hft_sequencer::driver::verify::VerifyOptions;

#[derive(Debug)]
pub(crate) struct Options {
    pub journal: PathBuf,
    pub instance: Option<String>,
    pub verify: VerifyOptions,
    pub report: Option<PathBuf>,
    pub check: bool,
    pub record_decide_hist: bool,
    pub reassembly: usize,
    pub limits: LogReadLimits,
    pub recompute: bool,
    pub checkpoint: Option<PathBuf>,
    pub model_limits: RecomputeLimits,
}

pub(crate) fn parse(args: &[String]) -> Result<Options, String> {
    if args.first().map(String::as_str) != Some("verify") {
        return Err("usage: replayer verify --journal <file|dir> [--from-seq N] [--to-seq N] [--instance prod|sim] [--advice journal|recompute] [--model-checkpoint file|dir] [--keep-going] [--record-decide-hist] [--report file.json] [--check]; other subcommands are not implemented".into());
    }
    let mut journal = None;
    let mut o = Options {
        journal: PathBuf::new(),
        instance: None,
        verify: VerifyOptions::default(),
        report: None,
        check: false,
        record_decide_hist: false,
        recompute: false,
        checkpoint: None,
        model_limits: RecomputeLimits {
            records: 1_000_000,
            bytes: 512 << 20,
        },
        reassembly: 128 << 20,
        limits: LogReadLimits {
            records: 1_000_000,
            body_bytes: 512 << 20,
        },
    };
    let mut seen = std::collections::BTreeSet::new();
    let mut i = 1;
    while i < args.len() {
        let key = args[i].as_str();
        if !seen.insert(key) {
            return Err(format!("duplicate option {key}"));
        }
        i += 1;
        match key {
            "--record-decide-hist" => {
                o.record_decide_hist = true;
                continue;
            }
            "--keep-going" => {
                o.verify.keep_going = true;
                continue;
            }
            "--check" => {
                o.check = true;
                continue;
            }
            _ => {}
        }
        let value = args
            .get(i)
            .filter(|s| !s.starts_with("--"))
            .ok_or_else(|| format!("{key} needs a value"))?;
        i += 1;
        let number = || {
            value
                .parse::<u64>()
                .ok()
                .filter(|v| *v > 0)
                .ok_or_else(|| format!("{key} needs a positive integer"))
        };
        let bound = || usize::try_from(number()?).map_err(|_| format!("{key} is too large"));
        match key {
            "--journal" => journal = Some(value.clone()),
            "--instance" if matches!(value.as_str(), "prod" | "sim") => o.instance = Some(value.clone()),
            "--advice" if value == "journal" => {}
            "--advice" if value == "recompute" => o.recompute = true,
            "--model-checkpoint" if !value.is_empty() && !value.starts_with("s3://") => {
                o.checkpoint = Some(PathBuf::from(value))
            }
            "--report" => o.report = Some(PathBuf::from(value)),
            "--from-seq" => o.verify.from_seq = Some(number()?),
            "--to-seq" => o.verify.to_seq = Some(number()?),
            "--max-records" => o.limits.records = bound()?,
            "--max-body-bytes" => o.limits.body_bytes = bound()?,
            "--reassembly-bytes" => o.reassembly = bound()?,
            "--max-model-records" => o.model_limits.records = bound()?,
            "--max-model-bytes" => o.model_limits.bytes = bound()?,
            _ => {
                return Err(format!(
                    "unsupported option or value: {key} {value}; daily reports and remote input are not implemented"
                ));
            }
        }
    }
    let j = journal.ok_or("--journal is required")?;
    if j.is_empty() || j.starts_with("s3://") {
        return Err("--journal requires a local file or directory".into());
    }
    if o.verify.from_seq.unwrap_or(0) > o.verify.to_seq.unwrap_or(u64::MAX) {
        return Err("--from-seq exceeds --to-seq".into());
    }
    if o.check && o.report.is_some() {
        return Err("--check does not write reports".into());
    }
    if o.checkpoint.is_some() && !o.recompute {
        return Err("--model-checkpoint requires --advice recompute".into());
    }
    if o.report
        .as_ref()
        .is_some_and(|p| p.extension().is_none_or(|s| s != "json"))
    {
        return Err("--report requires a .json file".into());
    }
    o.journal = PathBuf::from(j);
    Ok(o)
}
