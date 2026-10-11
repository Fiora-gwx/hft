//! CLI assembly only: input location, validation, sequencer invocation and report formatting.
#![forbid(unsafe_code)]

mod options;
mod report;

use std::io::Write;
use std::path::{Path, PathBuf};

use hft_journal::{FileProblem, InstanceName, JournalKind, JournalReader};
use hft_rt::Heartbeat;
use hft_sequencer::driver::replay::Log;
use hft_sequencer::driver::replay_io::LogReadError;
use hft_sequencer::driver::verify::{VerifyError, verify};
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
    let mut reader = JournalReader::open_files(&files, opts.reassembly).map_err(|e| {
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
    // A report may never overwrite an input or its seal, even when the source has been renamed.
    if let Some(p) = &opts.report {
        for target in [p.clone(), report::divergence_path(p)] {
            let parent = target
                .parent()
                .filter(|x| !x.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            let abs = std::fs::canonicalize(parent)
                .map_err(|e| (2, e.to_string()))?
                .join(target.file_name().ok_or((2, "report filename missing".into()))?);
            for source in &files {
                let input = std::fs::canonicalize(source).map_err(|e| (3, e.to_string()))?;
                if input == abs || input.with_extension("seal") == abs {
                    return Err((2, "report path overlaps journal input".into()));
                }
            }
        }
    }
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
    let result = verify::<S>(log, opts.verify, &HB).map_err(|e| {
        let code = match e {
            VerifyError::UnknownStrategy { .. } => 4,
            VerifyError::Window | VerifyError::UnsupportedRun { .. } => 2,
            _ => 3,
        };
        (code, format!("verification failed: {e:?}"))
    })?;
    let fail = !result.sink.mismatches.is_empty();
    let (summary, detail) = report::format(&instance, &result);
    writeln!(
        stdout,
        "{}",
        serde_json::json!({"summary": summary, "divergences": detail})
    )
    .map_err(|e| (2, e.to_string()))?;
    if let Some(path) = opts.report {
        report::write(&path, &summary).map_err(|e| (2, e.to_string()))?;
        // Always replace the sidecar, including [] on success; no stale divergence survives.
        report::write(&report::divergence_path(&path), &detail).map_err(|e| (2, e.to_string()))?;
    }
    Ok(i32::from(fail))
}
