//! Temporary, read-only cross-commit DST evidence exporter; removed after the audit.
use std::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::path::PathBuf::from(std::env::args().nth(1).ok_or("output directory required")?);
    std::fs::create_dir_all(&out)?;
    let suite = hft_testkit::scenarios::suites()
        .into_iter()
        .find(|s| s.name == "core")
        .ok_or("core suite missing")?;
    for seed in [7, 28, 34] {
        let mut cfg = suite.config(seed);
        cfg.steps = 2_000;
        cfg.rerun = hft_testkit::dst::RerunPolicy::Never;
        let record = hft_testkit::dst::run_record::<hft_testkit::scenarios::DstEngine>(&cfg);
        assert!(record.outcome.is_pass(), "seed {seed}: {:?}", record.outcome);
        assert_eq!(record.steps_run, cfg.steps);
        let trail = record.trail.to_bytes();
        std::fs::write(out.join(format!("seed-{seed}.trail")), &trail)?;
        let digest = xxhash_rust::xxh3::xxh3_128(&trail);
        let mut journal = std::fs::File::create(out.join(format!("seed-{seed}.jrec")))?;
        journal.write_all(b"HJRA0001")?;
        journal.write_all(&(record.journal.len() as u64).to_le_bytes())?;
        for r in &record.journal {
            journal.write_all(&r.seq.to_le_bytes())?;
            journal.write_all(&r.tsc.to_le_bytes())?;
            journal.write_all(&r.kind.0.to_le_bytes())?;
            journal.write_all(&r.flags.0.to_le_bytes())?;
            journal.write_all(&(r.body.len() as u32).to_le_bytes())?;
            journal.write_all(&r.body)?;
        }
        let summary = format!(
            "seed={seed} steps={} records={} snapshot_checks={} trail={digest:032x}\n",
            record.steps_run,
            record.journal.len(),
            record.snapshot_checks
        );
        std::fs::write(out.join(format!("seed-{seed}.txt")), &summary)?;
        print!("{summary}");
    }
    Ok(())
}
