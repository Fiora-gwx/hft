//! CLI assembly only: input location, validation, sequencer invocation and report formatting.
#![forbid(unsafe_code)]

mod checkpoints;
mod latency;
mod options;
mod report;
mod research;

use std::io::Write;
use std::path::{Path, PathBuf};

use hft_journal::{FileProblem, InstanceName, JournalKind, JournalReader};
use hft_rt::Heartbeat;
use hft_sequencer::driver::recompute::{RecomputeError, recompute};
use hft_sequencer::driver::replay::Log;
use hft_sequencer::driver::replay_io::LogReadError;
use hft_sequencer::driver::verify::{VerifyError, verify_with_clock};
use hft_strategy_sdk::StrategySet;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The release binary supplies only the registry. Integration fixtures can exercise precisely
/// this entry point with a test StrategySet without adding test kinds to the release registry.
pub fn run<S: StrategySet>(args: &[String], stdout: &mut impl Write, stderr: &mut impl Write) -> i32 {
    match execute::<S>(args, stdout) {
        Ok(code) => code,
        Err((code, message)) => {
            let _ = writeln!(stderr, "replayer: {message}");
            code
        }
    }
}

type Failure = (i32, String);

fn monotonic_ns() -> u64 {
    static EPOCH: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let elapsed = EPOCH.get_or_init(std::time::Instant::now).elapsed().as_nanos();
    u64::try_from(elapsed).unwrap_or(u64::MAX)
}

fn decide_hist(samples: &[u64]) -> serde_json::Value {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let quantile = |permille: usize| {
        let index = (sorted.len() * permille).div_ceil(1000).saturating_sub(1);
        sorted.get(index).copied().unwrap_or(0)
    };
    serde_json::json!({"unit": "ns", "count": sorted.len(), "p50": quantile(500),
        "p90": quantile(900), "p99": quantile(990), "p99.9": quantile(999), "max": quantile(1000)})
}
fn model_error(e: RecomputeError) -> Failure {
    let code = match e {
        RecomputeError::CheckpointKey { .. } | RecomputeError::Verify(VerifyError::Window) => 2,
        RecomputeError::Verify(VerifyError::UnknownStrategy { .. }) => 4,
        _ => 3,
    };
    (code, format!("Model verification failed: {e:?}"))
}

fn protect_report(report: Option<&Path>, files: &[PathBuf]) -> Result<(), Failure> {
    let Some(p) = report else {
        return Ok(());
    };
    for target in [p.to_owned(), report::divergence_path(p)] {
        let parent = target
            .parent()
            .filter(|x| !x.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let abs = std::fs::canonicalize(parent)
            .map_err(|e| (2, e.to_string()))?
            .join(target.file_name().ok_or((2, "report filename missing".into()))?);
        for source in files {
            let input = std::fs::canonicalize(source).map_err(|e| (3, e.to_string()))?;
            if input == abs || input.with_extension("seal") == abs {
                return Err((2, "report path overlaps an input".into()));
            }
        }
    }
    Ok(())
}
fn discover(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), Failure> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| (3, format!("{}: {e}", path.display())))?;
    if meta.file_type().is_symlink() {
        return Err((2, format!("symlink input is not supported: {}", path.display())));
    }
    if meta.is_file() {
        files.push(path.to_owned());
        return Ok(());
    }
    if !meta.is_dir() {
        return Err((3, format!("not a journal file or directory: {}", path.display())));
    }
    for entry in std::fs::read_dir(path).map_err(|e| (3, e.to_string()))? {
        let e = entry.map_err(|e| (3, e.to_string()))?;
        let ty = e.file_type().map_err(|e| (3, e.to_string()))?;
        let p = e.path();
        if ty.is_dir() || p.extension().is_some_and(|s| s == "jrn") {
            discover(&p, files)?;
        }
    }
    Ok(())
}

fn execute<S: StrategySet>(args: &[String], stdout: &mut impl Write) -> Result<i32, Failure> {
    if args.first().is_some_and(|s| s == "latency") {
        return latency::execute(args, stdout);
    }
    if args.first().is_some_and(|s| s == "research") {
        return research::execute::<S>(args, stdout);
    }
    if args.first().is_some_and(|s| s == "calib") {
        return research::execute_calib(args, stdout);
    }
    if args.first().is_some_and(|s| s == "parity") {
        return research::execute_parity::<S>(args, stdout);
    }
    if args == ["--version"] {
        writeln!(stdout, "{VERSION}").map_err(|e| (2, e.to_string()))?;
        for kind in S::kinds() {
            writeln!(stdout, "{} {}", kind.kind_id, kind.name).map_err(|e| (2, e.to_string()))?;
        }
        return Ok(0);
    }
    let opts = options::parse(args).map_err(|e| (2, e))?;
    let mut files = Vec::new();
    discover(&opts.journal, &mut files)?;
    let mut reader = JournalReader::open_files_recoverable(&files, opts.reassembly).map_err(|e| {
        let code = if matches!(e.problem, FileProblem::Format(_)) {
            4
        } else {
            3
        };
        (code, format!("{}: {:?}", e.path.display(), e.problem))
    })?;
    let segments = reader
        .file_segments()
        .ok_or((3, "missing file manifest".into()))?;
    let first = segments.first().ok_or((3, "empty input".into()))?;
    if first.header.journal_kind != JournalKind::Seq as u8 {
        return Err((2, "verify requires a seq journal".into()));
    }
    let instance = if first.header.instance == InstanceName::Prod.to_header() {
        "prod"
    } else if first.header.instance == InstanceName::Sim.to_header() {
        "sim"
    } else {
        return Err((2, "unknown instance".into()));
    };
    if opts.instance.as_deref().is_some_and(|v| v != instance) {
        return Err((2, "--instance differs from the segment header".into()));
    }
    // A report may never overwrite an input or its seal, even after a rename.
    protect_report(opts.report.as_deref(), &files)?;
    let instance = instance.to_owned();
    let log = Log::from_reader(&mut reader, opts.limits).map_err(|e| {
        (
            if matches!(e, LogReadError::Header { .. }) {
                4
            } else {
                3
            },
            format!("journal read: {e:?}"),
        )
    })?;
    drop(reader);
    if !log.recs.iter().any(|r| {
        r.kind == hft_event::EventKind::CONFIG_BLOB
            && r.read::<hft_event::BlobHdr>()
                .is_some_and(|b| b.blob.kind == hft_event::BlobKind::Refdata)
    }) {
        let message = "journal 不自足: 缺少 ConfigBlob Refdata";
        writeln!(
            stdout,
            "{}",
            serde_json::json!({"error": message, "missing_blob": "Refdata"})
        )
        .map_err(|e| (2, e.to_string()))?;
        return Err((3, message.into()));
    }
    let (checkpoints, paths) =
        checkpoints::load::<S>(opts.checkpoint.as_deref(), &log, opts.model_limits.bytes)?;
    protect_report(opts.report.as_deref(), &paths)?;
    if opts.check {
        writeln!(
            stdout,
            "input checked: {} records; no replay performed",
            log.recs.len()
        )
        .map_err(|e| (2, e.to_string()))?;
        return Ok(0);
    }
    static HB: Heartbeat = Heartbeat::new();
    let model = if opts.recompute {
        Some(recompute::<S>(&log, opts.verify, &checkpoints, opts.model_limits, &HB).map_err(model_error)?)
    } else {
        None
    };
    let mut verify_opts = opts.verify;
    if let Some(m) = &model {
        verify_opts.from_seq = m.verify_from_seq;
    }
    let result = verify_with_clock::<S>(
        log,
        verify_opts,
        &HB,
        opts.record_decide_hist.then_some(monotonic_ns as fn() -> u64),
    )
    .map_err(|e| {
        let code = match e {
            VerifyError::UnknownStrategy { .. } => 4,
            VerifyError::Window => 2,
            _ => 3,
        };
        (code, format!("verification failed: {e:?}"))
    })?;
    let fail =
        !result.sink.mismatches.is_empty() || model.as_ref().is_some_and(|m| !m.differences.is_empty());
    let (mut summary, detail) = report::format(&instance, &result, model.as_ref());
    if let Some(timing) = &result.sink.decide_timing {
        summary["decide_hist"] = decide_hist(&timing.samples_ns);
    }
    let mut output = detail.clone();
    output["summary"] = summary.clone();
    if let Some(first) = result.sink.mismatches.first() {
        let fragment = report::fragment(&opts.journal, &result.sink.log, first.seq)
            .map_err(|e| (2, format!("diagnostic fragment: {e}")))?;
        output["journal_fragment"] = fragment.display().to_string().into();
    }
    if let Some(hist) = summary.get("decide_hist") {
        output["decide_hist"] = hist.clone();
    }
    writeln!(stdout, "{}", output).map_err(|e| (2, e.to_string()))?;
    if let Some(path) = opts.report {
        report::write(&path, &summary).map_err(|e| (2, e.to_string()))?;
        // Always replace both lists in the sidecar; no stale divergence survives a success.
        report::write(&report::divergence_path(&path), &detail).map_err(|e| (2, e.to_string()))?;
    }
    Ok(i32::from(fail))
}
