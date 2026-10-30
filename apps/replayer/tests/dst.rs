//! RPL-012: actual DST -> physical files -> independent replayer process, no checkpoints.
use hft_event::{EventKind, OrderDecision, OrderEvent, OrderEventKind, SegmentRoll};
use hft_testkit::dst::{DstConfig, DstOutcome, RerunPolicy, run_record};
use hft_testkit::scenarios::{SeqEngine, replay_config, suites};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}
fn executable() -> PathBuf {
    let exe = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples")
        .join(format!("replay_probe{}", std::env::consts::EXE_SUFFIX));
    assert!(exe.is_file(), "CI prerequisite missing: {}", exe.display());
    exe
}

fn verify_case(cfg: &DstConfig, dir: &Path) -> Value {
    verify_case_with(cfg, dir, true)
}

/// `verify_case`; `model` = the world has a Model whose Advice must be produced and recomputed.
fn verify_case_with(cfg: &DstConfig, dir: &Path, model: bool) -> Value {
    std::fs::create_dir_all(dir).unwrap();
    let plan = cfg.fault_plan();
    std::fs::write(
        dir.join("faults.txt"),
        hft_testkit::dst::report::fault_table(&plan),
    )
    .unwrap();
    let repro = format!(
        "HFT_REPLAY_DST_SEED={:#018x} HFT_REPLAY_DST_COUNT=1 HFT_REPLAY_DST_SUITE={} cargo test --locked --profile ci -p replayer --test cli dst::rpl_012_dst_journals_verify -- --exact --nocapture\n",
        cfg.seed, cfg.suite
    );
    std::fs::write(dir.join("repro.txt"), repro).unwrap();
    let rec = run_record::<SeqEngine>(cfg);
    if let DstOutcome::Fail(f) = &rec.outcome {
        std::fs::write(
            dir.join("dst-failure.txt"),
            format!("{:?}\n{}", f.invariant, f.message),
        )
        .unwrap();
    }
    let log = rec.physical_journal.as_ref().expect("real DST physical source");
    if rec.outcome.is_pass() {
        assert_eq!(
            log.recs.len(),
            rec.journal.len(),
            "export must retain the whole published run"
        );
    }
    for (ix, (a, b)) in log.recs.iter().zip(&rec.journal).enumerate() {
        assert_eq!((a.seq, a.kind, a.tsc, a.flags), (b.seq, b.kind, b.tsc, b.flags));
        assert_eq!(a.body, b.body, "RunRecord source at {}", a.seq);
        assert_eq!(log.record_end_seq(ix), Some(b.end_seq));
    }
    let journal = crate::fixture::dst::export(log, &dir.join("journal"));
    let output = Command::new(executable())
        .args([
            "--fixture-dst",
            "verify",
            "--journal",
            journal.to_str().unwrap(),
            "--advice",
            "recompute",
            "--report",
            dir.join("verify.json").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    std::fs::write(dir.join("stdout.json"), &output.stdout).unwrap();
    std::fs::write(dir.join("stderr.txt"), &output.stderr).unwrap();
    if let DstOutcome::Fail(f) = &rec.outcome {
        panic!("DST seed {:#018x}: {:?}: {}", cfg.seed, f.invariant, f.message);
    }
    let report: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "seed {:#018x}: report {e}; exit {}; stderr {}",
            cfg.seed,
            output.status,
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(1_024)
                .collect::<String>()
        )
    });
    let summary = &report["summary"];
    assert!(
        output.status.success(),
        "seed {:#018x}: exit {}; verify={} recompute={} first_output={} first_model={}; full report in {}",
        cfg.seed,
        output.status,
        summary["verify"],
        summary["recompute"],
        summary["first_output_divergent_seq"],
        summary["first_model_divergent_seq"],
        dir.display(),
    );
    assert_eq!(summary["verify"], "pass", "{report}");
    assert_eq!(summary["recompute"], "pass", "{report}");
    assert!(report["model_divergences"].as_array().unwrap().is_empty());
    assert!(report["model_no_start"].as_array().unwrap().is_empty());
    assert!(summary["checkpoint_key"].as_array().unwrap().is_empty());
    let advice = log.recs.iter().filter(|r| r.kind == EventKind::ADVICE).count();
    if model {
        assert!(advice > 0, "real Model channel was never exercised");
    }
    assert_eq!(summary["model_advice_compared"].as_u64(), Some(advice as u64));
    let recovery_seqs: Vec<_> = log
        .recs
        .iter()
        .filter(|r| r.kind == EventKind::SEGMENT_ROLL)
        .filter(|r| {
            r.read::<SegmentRoll>().is_some_and(|s| {
                s.mark == hft_journal::RollMark::Begin as u8
                    && s.reason == hft_journal::RollReason::Recovery as u8
            })
        })
        .map(|r| r.seq)
        .collect();
    let recovery = recovery_seqs.len();
    // Accepted order decisions per account (RPL-309: both the Live and the Sim account trade).
    let mut decisions = std::collections::BTreeMap::<u8, u64>::new();
    for d in log
        .recs
        .iter()
        .filter(|r| r.kind == EventKind::ORDER_DECISION)
        .filter_map(|r| r.read::<OrderDecision>())
        .filter(|d| d.reject == 0)
    {
        *decisions.entry(d.account.0).or_default() += 1;
    }
    let partial_fills = log
        .recs
        .iter()
        .filter(|r| r.kind == EventKind::ORDER_EVENT)
        .filter_map(|r| r.read::<OrderEvent>())
        .filter(|e| e.what == OrderEventKind::PartialFill && e.qty.0 > 0)
        .count();
    let stale_gen: Vec<_> = rec.stale_gen.iter().map(|v| {
        let executions: Vec<_> = v.executions.iter().map(|x| {
            let fill = rec.external_receipts.iter().find(|r|
                r.fill.venue == v.venue && r.fill.account == x.account && r.fill.exch_order_id == x.order_id)
                .expect("stale execution must retain its original receipt").fill;
            let delivered_seq = rec.journal.iter().find_map(|j| j.try_read::<OrderEvent>()
                .filter(|e| e.venue == v.venue && e.account == x.account && e.exch_order_id == x.order_id
                    && (e.epoch, e.slot, e.r#gen) == (x.epoch, x.slot, x.ack_gen ^ (1 << 31)))
                .map(|_| j.seq));
            json!({"requested_ns": x.requested_ns, "matched_ns": x.matched_ns,
                "delivered_ns": x.delivered_ns, "ack_ns": x.ack_ns, "account": x.account.0,
                "inst": x.inst.0, "epoch": x.epoch, "slot": x.slot.0, "ack_gen": x.ack_gen,
                "ack_order_id": x.ack_order_id, "order_id": x.order_id, "delivered_seq": delivered_seq,
                "side": fill.side as u8, "qty": fill.qty.0, "px": fill.px.0, "trade_id": fill.trade_id,
                "fill_seq": fill.fill_seq, "cash_qty": fill.cash_qty.0, "cash": fill.cash_amount.0,
                "fee": fill.fee.0, "fee_asset": fill.fee_asset.0})
        }).collect();
        json!({"venue": v.venue.0, "requested": v.requested, "pending": v.pending, "executions": executions})
    }).collect();
    let stale_gen_fills: usize = rec.stale_gen.iter().map(|v| v.executions.len()).sum();
    let cancel_filled: Vec<_> = rec
        .cancel_filled
        .iter()
        .map(|v| {
            use zerocopy::IntoBytes;
            let bytes = |e: &[u8]| e.iter().map(|b| format!("{b:02x}")).collect::<String>();
            let seq = |e: Option<OrderEvent>| {
                e.and_then(|e| {
                    rec.journal
                        .iter()
                        .find(|j| j.kind == EventKind::ORDER_EVENT && j.body == e.as_bytes())
                        .map(|j| j.seq)
                })
            };
            let cases: Vec<_> = v.cases.iter().map(|x| json!({
            "requested_ns": x.requested_ns, "held_ns": x.held_ns, "deadline_ns": x.deadline_ns,
            "original_due_ns": x.original_due_ns, "original": bytes(x.original.as_bytes()),
            "command_ns": x.command_ns, "command": x.command.map(|c| bytes(c.as_bytes())),
            "command_seq": x.command.map(|c| c.decision_seq),
            "rejection": x.rejection.map(|e| bytes(e.as_bytes())), "rejection_seq": seq(x.rejection),
            "rejection_due_ns": x.rejection_due_ns, "release_ns": x.release_ns,
            "report": x.report.map(|e| bytes(e.as_bytes())), "delivered_ns": x.delivered_ns,
            "delivered_seq": seq(x.report),
        })).collect();
            json!({"venue": v.venue.0, "requested": v.requested, "awaiting_fill": v.awaiting_fill,
            "awaiting_release": v.awaiting_release, "cases": cases})
        })
        .collect();
    let late_recycle: Vec<_> = rec
        .late_recycle
        .iter()
        .map(|v| {
            let copies: Vec<_> = v
                .copies
                .iter()
                .map(|x| {
                    use zerocopy::IntoBytes;
                    let late_seq = rec
                        .journal
                        .iter()
                        .find(|j| {
                            j.seq > x.recycle_seq
                                && j.kind == EventKind::ORDER_EVENT
                                && j.body == x.report.as_bytes()
                        })
                        .map(|j| j.seq);
                    let hex = |e: &OrderEvent| {
                        e.as_bytes()
                            .iter()
                            .map(|b| format!("{b:02x}"))
                            .collect::<String>()
                    };
                    json!({"requested_ns": x.requested_ns, "original_seq": x.original_seq,
                "recycle_seq": x.recycle_seq, "recycle_ns": x.recycle_ns,
                "delivered_ns": x.delivered_ns, "delivered_seq": late_seq,
                "original": hex(&x.original), "report": hex(&x.report),
                "tomb": {"venue": x.tomb.venue.0, "account": x.tomb.account.0,
                    "epoch": x.tomb.epoch, "slot": x.tomb.slot, "gen": x.tomb.r#gen,
                    "inst": x.tomb.inst.0, "filled": x.tomb.filled.to_string()},
                "current": x.current.map(|s| json!({"epoch": s.epoch, "gen": s.r#gen}))})
                })
                .collect();
            json!({"venue": v.venue.0, "requested": v.requested, "awaiting_fill": v.awaiting_fill,
            "awaiting_recycle": v.awaiting_recycle, "copies": copies})
        })
        .collect();
    let files: Vec<_> = (1..=log.segs.len())
        .map(|si| {
            let file = journal.join(format!("{si:010}.jrn"));
            use sha2::{Digest, Sha256};
            let bytes = std::fs::read(&file).unwrap();
            json!({"name": file.file_name().unwrap().to_str().unwrap(), "bytes": bytes.len(),
            "sha256": format!("{:x}", Sha256::digest(&bytes))})
        })
        .collect();
    let result = json!({"seed": format!("{:#018x}", cfg.seed), "suite": cfg.suite,
        "steps_limit": cfg.steps, "steps_run": rec.steps_run, "end_ns": rec.end_ns,
        "records": log.recs.len(), "segments": log.segs.len(), "recovery_runs": recovery,
        "recovery_seqs": recovery_seqs,
        "physical_last_seq": log.record_end_seq(log.recs.len() - 1),
        "advices": advice, "decisions_by_account": decisions, "partial_fills": partial_fills, "stale_gen_fills": stale_gen_fills,
        "stale_gen": stale_gen, "late_recycle": late_recycle, "cancel_filled": cancel_filled,
        "faults_planned": plan.items.len(),
        "faults_scheduled_before_end": plan.items.iter().filter(|(t, _)| *t <= rec.end_ns).count(),
        "files": files, "summary": summary});
    std::fs::write(dir.join("case.json"), serde_json::to_vec_pretty(&result).unwrap()).unwrap();
    result
}

#[test]
fn rpl_283_dst_files_preserve_records_and_model_across_recovery() {
    use hft_testkit::dst::faults::{CrashFault, Fault};
    let mut cfg = replay_config(&suites()[0], 1);
    cfg.steps = 1_000;
    cfg.faults = hft_testkit::dst::FaultRates::off();
    cfg.rerun = RerunPolicy::Never;
    cfg.scripted = vec![(
        0,
        Fault::Crash(CrashFault {
            at_step: 250,
            repeat: 2,
            point: hft_testkit::dst::CrashPoint::SeqInput,
        }),
    )];
    let dir = root().join(format!(
        "target/dst/0000000000000001/rpl-283-{}",
        std::process::id()
    ));
    let _evidence = hft_testkit::tmp::TmpDir::evidence(&dir).unwrap();
    let result = verify_case(&cfg, &dir);
    assert!(result["recovery_runs"].as_u64().unwrap() >= 2, "{result}");
}

/// RPL-309: a prod instance's journal (`role_prod`) with a Live account and a Sim account (Shadow
/// variant) verifies: the simulated replies are Inputs, verify does not re-run the simulation.
#[test]
fn rpl_309_prod_role_journal_with_sim_account_verifies() {
    // The suite world (two takers) with account b Sim, as `tk_058`, recorded as a prod instance.
    let mut cfg = suites()[0].config(1);
    cfg.steps = 2_000;
    cfg.rerun = RerunPolicy::Never;
    cfg.world.physical_journal = true;
    cfg.world.model_advice = true;
    cfg.world.accounts[1].kind = hft_testkit::dst::world::AccountKind::Sim;
    cfg.world.role_prod = true;
    let dir = root().join(format!(
        "target/dst/0000000000000001/rpl-309-{}",
        std::process::id()
    ));
    let _evidence = hft_testkit::tmp::TmpDir::evidence(&dir).unwrap();
    let result = verify_case_with(&cfg, &dir, false);
    let by_account = &result["decisions_by_account"];
    let live = cfg.world.accounts[0].id.0.to_string();
    let sim = cfg.world.accounts[1].id.0.to_string();
    assert!(
        by_account[&live].as_u64().unwrap_or(0) > 0,
        "RPL-309 Live account traded: {result}"
    );
    assert!(
        by_account[&sim].as_u64().unwrap_or(0) > 0,
        "RPL-309 Sim account traded: {result}"
    );
    assert_eq!(result["summary"]["instance"], "prod", "RPL-309 {result}");
}

#[test]
fn rpl_284_partial_fills_replay_real_files_and_model_across_recovery() {
    use hft_testkit::dst::faults::{CrashFault, Fault};
    let mut cfg = replay_config(&suites()[0], 1);
    cfg.steps = 10_000;
    cfg.faults = hft_testkit::dst::FaultRates::off();
    cfg.rerun = RerunPolicy::Never;
    cfg.scripted = vec![
        (
            0,
            Fault::PartialFills {
                venue: hft_testkit::dst::world::VS,
                dur_ms: 5_000,
            },
        ),
        (
            0,
            Fault::Crash(CrashFault {
                at_step: 250,
                repeat: 2,
                point: hft_testkit::dst::CrashPoint::SeqInput,
            }),
        ),
    ];
    let dir = root().join(format!(
        "target/dst/0000000000000001/rpl-284-{}",
        std::process::id()
    ));
    let _evidence = hft_testkit::tmp::TmpDir::evidence(&dir).unwrap();
    let result = verify_case(&cfg, &dir);
    assert_eq!(result["steps_run"], 10_000);
    assert!(result["partial_fills"].as_u64().unwrap() > 0, "{result}");
    assert!(result["recovery_runs"].as_u64().unwrap() >= 2, "{result}");
}

#[test]
fn rpl_286_default_stale_fills_replay_original_files_and_model_across_recovery() {
    use hft_testkit::dst::faults::{CrashFault, Fault};
    use hft_testkit::dst::world::{VP, VS};
    for seed in [0, 2] {
        let mut cfg = replay_config(&suites()[0], seed);
        cfg.steps = 10_000;
        cfg.faults = hft_testkit::dst::FaultRates::off();
        cfg.rerun = RerunPolicy::Never;
        cfg.scripted = vec![
            (0, Fault::StaleGenFill { venue: VS, count: 1 }),
            (0, Fault::StaleGenFill { venue: VP, count: 1 }),
            (5_000_000_000, Fault::StaleGenFill { venue: VS, count: 1 }),
            (5_000_000_000, Fault::StaleGenFill { venue: VP, count: 1 }),
            (
                0,
                Fault::Crash(CrashFault {
                    at_step: 250,
                    repeat: 2,
                    point: hft_testkit::dst::CrashPoint::SeqInput,
                }),
            ),
        ];
        let dir = root().join(format!("target/dst/{seed:016x}/rpl-286-{}", std::process::id()));
        let _evidence = hft_testkit::tmp::TmpDir::evidence(&dir).unwrap();
        let result = verify_case(&cfg, &dir);
        assert_eq!(result["steps_run"], 10_000);
        assert_eq!(result["stale_gen_fills"], 4, "{result}");
        for venue in result["stale_gen"].as_array().unwrap() {
            assert_eq!(venue["requested"], 2);
            assert_eq!(venue["pending"], 0);
            assert!(
                venue["executions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|x| x["delivered_seq"].as_u64().is_some())
            );
        }
        assert!(result["recovery_runs"].as_u64().unwrap() >= 2, "{result}");
    }
}

#[test]
fn rpl_287_recycled_copies_replay_real_files_and_model_after_recovery() {
    use hft_testkit::dst::world::{VP, VS};
    use hft_testkit::dst::{CrashFault, Fault, FaultRates};
    for seed in [0, 2] {
        let mut cfg = replay_config(suites().iter().find(|s| s.name == "core").unwrap(), seed);
        cfg.steps = 10_000;
        cfg.faults = FaultRates::off();
        cfg.rerun = RerunPolicy::Never;
        cfg.scripted = vec![
            (0, Fault::LateFillAfterRecycle { venue: VS, count: 1 }),
            (0, Fault::LateFillAfterRecycle { venue: VP, count: 1 }),
            (
                0,
                Fault::Crash(CrashFault {
                    at_step: 250,
                    repeat: 2,
                    point: hft_testkit::dst::CrashPoint::SeqInput,
                }),
            ),
        ];
        let dir = root().join(format!("target/dst/{seed:016x}/rpl-287-{}", std::process::id()));
        let _evidence = hft_testkit::tmp::TmpDir::evidence(&dir).unwrap();
        let result = verify_case(&cfg, &dir);
        assert_eq!(result["steps_run"], 10_000);
        assert!(result["recovery_runs"].as_u64().unwrap() >= 2);
        let recoveries = result["recovery_seqs"].as_array().unwrap();
        let first_recovery = recoveries.first().unwrap().as_u64().unwrap();
        let last_recovery = recoveries.last().unwrap().as_u64().unwrap();
        let mut total = 0;
        for v in result["late_recycle"].as_array().unwrap() {
            assert_eq!(v["requested"], 1);
            assert_eq!(v["awaiting_fill"], 0);
            assert_eq!(v["awaiting_recycle"], 0);
            let copies = v["copies"].as_array().unwrap();
            assert_eq!(copies.len(), 1);
            for x in copies {
                assert!(x["original_seq"].as_u64().unwrap() < first_recovery);
                assert!(x["delivered_seq"].as_u64().unwrap() > last_recovery);
                assert!(x["original_seq"].as_u64().unwrap() < x["recycle_seq"].as_u64().unwrap());
                assert!(x["delivered_seq"].as_u64().unwrap() > x["recycle_seq"].as_u64().unwrap());
            }
            total += copies.len();
        }
        assert_eq!(total, 2);
    }
}

#[test]
fn rpl_288_cancel_reject_filled_replays_actual_files_and_model() {
    for seed in [0, 2] {
        let mut cfg = hft_testkit::scenarios::cancel_filled::config(seed, true);
        cfg.world.physical_journal = true;
        cfg.world.model_advice = true;
        let dir = root().join(format!("target/dst/{seed:016x}/rpl-288-{}", std::process::id()));
        let _evidence = hft_testkit::tmp::TmpDir::evidence(&dir).unwrap();
        let result = verify_case(&cfg, &dir);
        assert_eq!(result["steps_run"], 10_000);
        assert!(result["recovery_runs"].as_u64().unwrap() >= 2);
        let mut races = 0;
        for v in result["cancel_filled"].as_array().unwrap() {
            assert_eq!(v["requested"], 1);
            assert_eq!(v["awaiting_fill"], 0);
            assert_eq!(v["awaiting_release"], 0);
            for x in v["cases"].as_array().unwrap() {
                let cmd = x["command_seq"].as_u64().unwrap();
                let reject = x["rejection_seq"].as_u64().unwrap();
                let fill = x["delivered_seq"].as_u64().unwrap();
                assert!(cmd < reject && reject < fill);
                races += 1;
            }
        }
        assert_eq!(races, 2);
    }
}

#[test]
fn rpl_012_dst_journals_verify() {
    let root = root();
    let start = std::env::var("HFT_REPLAY_DST_SEED")
        .ok()
        .map(|s| xtask::dst::parse_u64(&s).expect("HFT_REPLAY_DST_SEED must be a u64"))
        .unwrap_or_else(|| xtask::dst::seed_from_head(&root).unwrap());
    let count = std::env::var("HFT_REPLAY_DST_COUNT")
        .ok()
        .map(|s| s.parse::<u64>().unwrap())
        .unwrap_or(1);
    assert!(count > 0);
    let override_suite = std::env::var("HFT_REPLAY_DST_SUITE").ok();
    let suites = suites();
    let mut cases: Vec<_> = (0..count)
        .map(|ix| {
            let seed = start.wrapping_add(ix);
            let suite = override_suite
                .as_ref()
                .map(|name| suites.iter().find(|s| s.name == name).unwrap())
                .unwrap_or_else(|| hft_testkit::dst::scenario::suite_for_seed(&suites, seed).unwrap());
            (seed, suite.name.to_owned(), "derived")
        })
        .collect();
    // Explicit single-suite reproduction stays focused. The CI call supplies no override and
    // always includes every regression in its registered suite, without deduplicating rows.
    if override_suite.is_none() {
        let regressions = xtask::dst::regressions(&root).unwrap();
        assert!(!regressions.is_empty(), "regression inventory is unavailable");
        cases.extend(regressions.into_iter().map(|(s, suite)| (s, suite, "regression")));
    }
    let mut rows = Vec::new();
    let mut failures = Vec::new();
    for (index, (seed, suite, source)) in cases.iter().enumerate() {
        let suite = suites.iter().find(|s| s.name == suite).unwrap();
        let cfg = replay_config(suite, *seed);
        let dir = root.join(format!(
            "target/dst/{seed:016x}/rpl-012/{}-{index:03}-{}",
            std::process::id(),
            suite.name
        ));
        let evidence = hft_testkit::tmp::TmpDir::evidence(&dir).unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| verify_case(&cfg, &dir)));
        match result {
            Ok(mut row) => {
                drop(evidence);
                row["source"] = json!(source);
                rows.push(row);
                eprintln!(
                    "RPL-012 {}/{} seed={seed:#018x} suite={} pass",
                    index + 1,
                    cases.len(),
                    suite.name
                );
            }
            Err(e) => {
                // A caught failure keeps its journals, fault table and replay reports.
                evidence.keep();
                let why = e
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| e.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                    .unwrap_or_default();
                let row = json!({"seed": format!("{seed:#018x}"), "suite": suite.name, "source": source, "error": why});
                std::fs::write(dir.join("failure.json"), serde_json::to_vec_pretty(&row).unwrap()).unwrap();
                failures.push(row);
            }
        }
        let report = json!({"case": "RPL-012", "start": format!("{start:#018x}"), "count": count,
            "expected": cases.len(), "passed": rows, "failed": failures});
        std::fs::create_dir_all(root.join("target/xtask")).unwrap();
        std::fs::write(
            root.join("target/xtask/replay-dst.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
    }
    assert!(
        failures.is_empty(),
        "RPL-012 failed {}/{} seeds; see target/dst and target/xtask/replay-dst.json",
        failures.len(),
        cases.len()
    );
    assert_eq!(rows.len(), cases.len());
}
