//! Failure output (README "6. DST 框架", testing §3.4): exactly one copyable repro line, and the
//! artifacts of a failed seed — the journal fragment of the last 200 events (records in the journal
//! record format, framed by `hft_event::decode_record`) and the fault table — under
//! `<dir>/<seed>/`.

use std::fmt::Write as _;
use std::io;
use std::path::{Path, PathBuf};

use crate::dst::driver::encode_records;
use crate::dst::{DstFailure, FaultPlan};

/// `DST FAILED seed=0x… step=N invariant=I3(exposure) -> cargo xtask dst --seeds 1 --seed 0x… --suite core --steps N+1 --trace`.
pub fn repro_line(f: &DstFailure) -> String {
    repro_line_case(f, None)
}

/// The selected case remains in the repro even when a generic invariant fails first.
pub fn repro_line_case(f: &DstFailure, case: Option<&str>) -> String {
    format!(
        "DST FAILED seed={:#018x} step={} invariant={} -> {}",
        f.seed,
        f.step,
        f.invariant.label(),
        repro_command_case(f, case)
    )
}

/// The repro command alone (tools README "cargo xtask dst").
pub fn repro_command(f: &DstFailure) -> String {
    repro_command_case(f, None)
}

pub fn repro_command_case(f: &DstFailure, case: Option<&str>) -> String {
    let case = case.map_or(String::new(), |id| format!(" --case {id}"));
    format!(
        "cargo xtask dst --seeds 1 --seed {:#018x} --suite {}{case} --steps {} --trace",
        f.seed,
        f.suite,
        f.step + 1
    )
}

/// Human-readable fault table, one item per line: `<virtual_ns> <category> <fault>`.
pub fn fault_table(p: &FaultPlan) -> String {
    let mut s = String::new();
    for (t, f) in &p.items {
        let _ = writeln!(s, "{t} {} {f:?}", f.category().name());
    }
    s
}

/// Paths written by [`write_artifacts`].
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct Artifacts {
    pub dir: PathBuf,
    pub journal: PathBuf,
    pub faults: PathBuf,
    pub repro: PathBuf,
}

/// Writes `<dir>/<seed as 16 hex digits>/{journal.bin, faults.txt, repro.txt}`.
pub fn write_artifacts(dir: &Path, f: &DstFailure) -> io::Result<Artifacts> {
    write_artifacts_case(dir, f, None)
}

/// `case` comes from the registered case table, never from an unvalidated path argument.
pub fn write_artifacts_case(dir: &Path, f: &DstFailure, case: Option<&str>) -> io::Result<Artifacts> {
    let d = dir.join(format!("{:016x}", f.seed));
    std::fs::create_dir_all(&d)?;
    let a = Artifacts {
        journal: d.join("journal.bin"),
        faults: d.join("faults.txt"),
        repro: d.join("repro.txt"),
        dir: d,
    };
    std::fs::write(&a.journal, encode_records(&f.journal_tail))?;
    std::fs::write(&a.faults, fault_table(&f.fault_schedule))?;
    std::fs::write(&a.repro, format!("{}\n{}\n", repro_line_case(f, case), f.message))?;
    Ok(a)
}
