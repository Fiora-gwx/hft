//! `replayer research` over generated journals (TESTS.md RPL-3xx).
use hft_replay::{DriverCfg, RunCfg, SimParams, Variant, run_one};
use hft_testkit::dst::{RerunPolicy, run_record};
use hft_testkit::scenarios::{SeqEngine, replay_config, suites};
use hft_testkit::strategies::TestStrategies;
use std::path::PathBuf;

/// A DST run (suite 0, `seed`, faults off) exported as physical seq segment files.
fn dst_journal(seed: u64, steps: u64) -> (hft_testkit::tmp::TmpDir, Vec<PathBuf>) {
    let mut cfg = replay_config(&suites()[0], seed);
    cfg.steps = steps;
    cfg.faults = hft_testkit::dst::FaultRates::off();
    cfg.rerun = RerunPolicy::Never;
    let rec = run_record::<SeqEngine>(&cfg);
    assert!(rec.outcome.is_pass(), "DST seed {seed}");
    let log = rec.physical_journal.as_ref().expect("physical journal");
    let tmp = hft_testkit::tmp::TmpDir::in_temp("rpl-research").unwrap();
    let dir = crate::fixture::dst::export(log, &tmp.path().join("journal"));
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "jrn"))
        .collect();
    files.sort();
    (tmp, files)
}

fn cfg(files: &[PathBuf]) -> RunCfg {
    RunCfg {
        files: files.to_vec(),
        accounts: None,
        variant: Variant {
            name: "base".into(),
            set: Default::default(),
        },
        sim: SimParams::default(),
        driver: DriverCfg::default(),
        reassembly: 128 << 20,
    }
}

#[test]
fn rpl_300_simulated_run_is_deterministic() {
    let (_t, files) = dst_journal(0, 2_000);
    let c = cfg(&files);
    let a = run_one::<TestStrategies>(&c).unwrap();
    let b = run_one::<TestStrategies>(&c).unwrap();
    // .1 the replayed accounts' logged reports are replaced: some dropped, simulated ones handed,
    // the simulated orders filled.
    assert_eq!(a.mask.len(), 2, "RPL-300.1");
    assert!(a.stats.masked_reports_dropped > 0, "RPL-300.1 {:?}", a.stats);
    assert!(a.stats.sim_inputs > 0, "RPL-300.1 {:?}", a.stats);
    assert!(
        a.stats.log_timers_dropped > 0 && a.stats.engine_timers > 0,
        "RPL-300.1 {:?}",
        a.stats
    );
    assert!(a.decisions.rows() > 0 && a.fills.rows() > 0, "RPL-300.1");
    // .2 two runs: the same tables, counters and simulated-venue state hash.
    assert_eq!(a.decisions, b.decisions, "RPL-300.2");
    assert_eq!(
        (&a.fills, &a.orders, &a.windows),
        (&b.fills, &b.orders, &b.windows),
        "RPL-300.2"
    );
    assert_eq!(a.sim_state_hash, b.sim_state_hash, "RPL-300.2");
    assert_eq!(format!("{:?}", a.stats), format!("{:?}", b.stats), "RPL-300.2");
}

/// Logged `OrderDecision` bodies (measure field masked) after the first `StateSnapshot`.
fn logged_decisions(files: &[PathBuf]) -> Vec<Vec<u8>> {
    let mut src = hft_replay::source::Source::open(files, 128 << 20).unwrap();
    let mut out = Vec::new();
    let mut started = false;
    while let Some(r) = src.next_rec().unwrap() {
        started |= r.r.kind == hft_event::EventKind::STATE_SNAPSHOT;
        if started && r.r.kind == hft_event::EventKind::ORDER_DECISION {
            let mut b = r.r.body.clone();
            hft_sequencer::journal::mask_measure(r.r.kind, &mut b);
            out.push(b);
        }
    }
    out
}

#[test]
fn rpl_301_nothing_simulated_reproduces_the_logged_decisions() {
    let (_t, files) = dst_journal(0, 2_000);
    let mut c = cfg(&files);
    c.accounts = Some(Vec::new());
    c.driver.engine_timers = false;
    let r = run_one::<TestStrategies>(&c).unwrap();
    let logged = logged_decisions(&files);
    eprintln!(
        "logged {} regenerated {} stats {:?}",
        logged.len(),
        r.decisions.rows(),
        r.stats
    );
    assert!(!logged.is_empty());
    assert_eq!(r.stats.sim_inputs, 0);
    assert_eq!(r.stats.masked_reports_dropped, 0);
    let regen = rows(
        &r.decisions,
        &[
            "trigger_log_seq",
            "inst",
            "slot",
            "gen",
            "action",
            "side",
            "px",
            "qty",
            "reject",
            "reason",
            "intent_no",
        ],
    );
    let want: Vec<Vec<i128>> = logged
        .iter()
        .map(|b| {
            let d = hft_sequencer::read_body::<hft_event::OrderDecision>(b).unwrap();
            vec![
                i128::from(d.trigger_seq),
                i128::from(d.inst.0),
                i128::from(d.slot.0),
                i128::from(d.r#gen),
                i128::from(d.action as u8),
                i128::from(d.side as u8),
                i128::from(d.px.0),
                i128::from(d.qty.0),
                i128::from(d.reject),
                i128::from(d.reason),
                i128::from(d.intent_no),
            ]
        })
        .collect();
    assert_eq!(regen, want, "RPL-301.1");
}

/// Selected columns of a table as rows of integers.
fn rows(t: &hft_replay::table::Table, names: &[&str]) -> Vec<Vec<i128>> {
    use hft_replay::table::Col;
    let cols: Vec<Vec<i128>> = names
        .iter()
        .map(|n| {
            match &t
                .cols
                .iter()
                .find(|c| c.0 == *n)
                .unwrap_or_else(|| panic!("column {n}"))
                .1
            {
                Col::I64(v) => v.iter().map(|x| i128::from(*x)).collect(),
                Col::U64(v) => v.iter().map(|x| i128::from(*x)).collect(),
                Col::U8(v) => v.iter().map(|x| i128::from(*x)).collect(),
                _ => panic!("column {n} is not an integer"),
            }
        })
        .collect();
    (0..t.rows())
        .map(|i| cols.iter().map(|c| c[i]).collect())
        .collect()
}

#[test]
fn rpl_302_variant_set_params_applies_or_refuses() {
    let (_t, files) = dst_journal(0, 2_000);
    let base = run_one::<TestStrategies>(&cfg(&files)).unwrap();
    // .1 a schema field within range: SetParams applied, the active blob changes.
    let mut c = cfg(&files);
    c.variant = Variant {
        name: "thr5".into(),
        set: [("thr".to_string(), 5)].into_iter().collect(),
    };
    let v = run_one::<TestStrategies>(&c).unwrap();
    // Only the instance whose schema has `thr` gets a SetParams (ConfigBlob + ControlCmd).
    assert_eq!(v.stats.variant_inputs, 2, "RPL-302.1");
    assert_eq!(v.params_sha.len(), base.params_sha.len(), "RPL-302.1");
    let changed = v
        .params_sha
        .iter()
        .zip(&base.params_sha)
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(changed, 1, "RPL-302.1");
    // .2 an unknown field or a value outside the schema range: the run is refused.
    for (k, val) in [("no_such_field", 1), ("thr", -1)] {
        c.variant.set = [(k.to_string(), val)].into_iter().collect();
        let e = run_one::<TestStrategies>(&c).unwrap_err();
        assert!(e.contains(k), "RPL-302.2 {e}");
    }
}

#[test]
fn rpl_303_research_command_writes_tables_and_run_json() {
    let (t, files) = dst_journal(0, 1_000);
    let dir = files[0].parent().unwrap().to_path_buf();
    let out = t.path().join("out");
    let exe = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples")
        .join(format!("replay_probe{}", std::env::consts::EXE_SUFFIX));
    assert!(exe.is_file(), "CI prerequisite missing: {}", exe.display());
    let o = std::process::Command::new(&exe)
        .args([
            "--fixture-dst",
            "research",
            "--journal",
            dir.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
            "--set",
            "thr=5",
            "--seeds",
            "1,2",
            "--jobs",
            "2",
        ])
        .output()
        .unwrap();
    assert_eq!(
        o.status.code(),
        Some(0),
        "RPL-303.1 {}",
        String::from_utf8_lossy(&o.stderr)
    );
    for f in [
        "decisions.parquet",
        "fills.parquet",
        "orders.parquet",
        "windows.parquet",
        "run.json",
    ] {
        assert!(out.join(f).is_file(), "RPL-303.1 {f}");
    }
    let meta: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("run.json")).unwrap()).unwrap();
    let runs = meta["runs"].as_array().unwrap();
    // .2 two variants (base, set1) × two seeds, each with its parameter hashes.
    assert_eq!(runs.len(), 4, "RPL-303.2");
    let names: Vec<_> = runs
        .iter()
        .map(|r| (r["variant"].as_str().unwrap(), r["seed"].as_u64().unwrap()))
        .collect();
    assert_eq!(
        names,
        vec![("base", 1), ("base", 2), ("set1", 1), ("set1", 2)],
        "RPL-303.2"
    );
    assert!(
        meta["calib_sha"].as_str().is_some_and(|s| s.len() == 64),
        "RPL-303.2"
    );
    // .3 an unknown option: exit 2.
    let o = std::process::Command::new(&exe)
        .args([
            "--fixture-dst",
            "research",
            "--journal",
            dir.to_str().unwrap(),
            "--bogus",
            "1",
        ])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(2), "RPL-303.3");
}

/// Throughput probe (not a case): `HFT_RESEARCH_BENCH_STEPS` DST steps, events per second.
#[test]
#[ignore]
fn research_throughput_probe() {
    let steps: u64 = std::env::var("HFT_RESEARCH_BENCH_STEPS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(50_000);
    let (_t, files) = dst_journal(0, steps);
    let c = cfg(&files);
    let t0 = std::time::Instant::now();
    let r = run_one::<TestStrategies>(&c).unwrap();
    let dt = t0.elapsed().as_secs_f64();
    let n = r.stats.log_inputs
        + r.stats.log_meta
        + r.stats.log_outputs_skipped
        + r.stats.masked_reports_dropped
        + r.stats.log_timers_dropped;
    eprintln!(
        "records {n} handed {} in {dt:.3}s = {:.0} rec/s ({:.2} us/rec); {:?}",
        r.stats.log_inputs + r.stats.sim_inputs + r.stats.engine_timers,
        n as f64 / dt,
        dt * 1e6 / n as f64,
        r.stats
    );
}

#[test]
fn rpl_304_parity_matches_the_logged_decisions_and_reports_a_variant() {
    // .1 the journal's own parameters: every decision on both sides.
    let (_t, files) = dst_journal(0, 2_000);
    let mut c = cfg(&files);
    c.accounts = Some(Vec::new());
    c.driver.engine_timers = false;
    let r = run_one::<TestStrategies>(&c).unwrap();
    let (_, s) = r.parity.unwrap();
    assert!(s.logged > 0, "RPL-304.1 {s:?}");
    assert_eq!(
        (s.both, s.logged_only, s.regen_only, s.first_diff_trigger),
        (s.logged, 0, 0, 0),
        "RPL-304.1 {s:?}"
    );
    assert_eq!(s.rate(), 1.0, "RPL-304.1");
    // .2 a variant that changes decisions is classified, not refused.
    c.variant = Variant {
        name: "thr".into(),
        set: [("thr".to_string(), 1_000_000)].into_iter().collect(),
    };
    let v = run_one::<TestStrategies>(&c).unwrap();
    let (t, s2) = v.parity.unwrap();
    assert_eq!(s2.logged, s.logged, "RPL-304.2");
    assert_eq!(s2.both + s2.logged_only, s2.logged, "RPL-304.2 {s2:?}");
    assert_eq!(s2.both + s2.regen_only, s2.regenerated, "RPL-304.2 {s2:?}");
    assert_eq!(
        t.rows() as u64,
        s2.both + s2.logged_only + s2.regen_only,
        "RPL-304.2"
    );
}
