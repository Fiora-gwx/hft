//! Temporary strict-cash matrix using the unmodified standard suites and all invariant checks.
use hft_testkit::dst::driver::encode_records;
use hft_testkit::dst::report::write_artifacts;
use hft_testkit::dst::world::VP;
use hft_testkit::dst::{DstOutcome, run_record};
use hft_testkit::scenarios::{DstEngine, suites};
use std::path::Path;

fn main() {
    let all = suites();
    let mut planned = Vec::new();
    // Exactly CI 191's commit-derived range; the audit branch's own SHA does not reseed it.
    for seed in 0xd05e_fd09_f013_185cu64..=0xd05e_fd09_f013_185c + 199 {
        let ix = hft_testkit::seed::suite_index(seed, all.len());
        planned.push((seed, ix));
    }
    let mut files: Vec<_> = std::fs::read_dir("fixtures/dst-seeds")
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "txt"))
        .collect();
    files.sort();
    for path in files {
        let name = path.file_stem().unwrap().to_str().unwrap();
        let ix = all.iter().position(|suite| suite.name == name).unwrap();
        for line in std::fs::read_to_string(path).unwrap().lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let token = line
                .split(|c: char| c.is_whitespace() || c == ',')
                .next()
                .unwrap();
            let seed = match token.strip_prefix("0x") {
                Some(hex) => u64::from_str_radix(hex, 16).unwrap(),
                None => token.parse().unwrap(),
            };
            planned.push((seed, ix));
        }
    }
    assert_eq!(planned.len(), 220);
    let mut failed = 0;
    for (seed, ix) in planned {
        let mut cfg = all[ix].config(seed);
        cfg.steps = 10_000;
        cfg.world
            .venues
            .iter_mut()
            .find(|venue| venue.id == VP)
            .unwrap()
            .perp_cash = true;
        let rec = run_record::<DstEngine>(&cfg);
        match &rec.outcome {
            DstOutcome::Pass { .. } => {
                assert_eq!(rec.steps_run, 10_000);
                assert!(rec.snapshot_checks > 0 && !rec.trail.points.is_empty());
                println!(
                    "{seed:#018x}\t{}\tPASS\t{}\t{}",
                    cfg.suite, rec.steps_run, rec.snapshot_checks
                );
            }
            DstOutcome::Fail(failure) => {
                failed += 1;
                let dir = Path::new("audit/strict-failures").join(cfg.suite);
                let artifacts = write_artifacts(&dir, failure).unwrap();
                std::fs::write(
                    artifacts.dir.join("full-journal.bin"),
                    encode_records(&rec.journal),
                )
                .unwrap();
                let deliveries = &rec.deliveries[rec.deliveries.len().saturating_sub(24)..];
                std::fs::write(artifacts.dir.join("detail.txt"), format!(
                    "Explicit VP.perp_cash=true; original 10000-step fault plan.\nReproduce with this audit source at its recorded commit.\n{}\nRecent deliveries\n{deliveries:#?}\nRaw matched tail\n{:#?}\nTruth\n{:#?}\nObservations\n{:#?}\n", failure.message, rec.matched_tail, rec.truth, rec.obs
                )).unwrap();
                println!(
                    "{seed:#018x}\t{}\tFAIL\t{}\t{}\t{}",
                    cfg.suite,
                    failure.step,
                    failure.invariant.label(),
                    failure.message.replace(['\n', '\r', '\t'], " ")
                );
            }
        }
    }
    eprintln!("strict cash matrix: {failed}/220 failures");
    assert_eq!(
        failed, 0,
        "strict cash failures remain; evidence is under audit/strict-failures"
    );
}
