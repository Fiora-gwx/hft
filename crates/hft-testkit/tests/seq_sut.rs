//! The engine adapter `SeqSut` (README "6. DST 框架"): the real sequencer boots on the W1 world,
//! steps under the DST loop and keeps its invariants; the same seed gives the same trail.

use hft_testkit::dst::faults::FaultCategory;
use hft_testkit::dst::{DstOutcome, RerunPolicy, SeqSut, run_record};
use hft_testkit::strategies::TestStrategies;

use crate::support::cfg;

type E = SeqSut<TestStrategies>;

fn show(r: &hft_testkit::dst::RunRecord) -> String {
    match &r.outcome {
        DstOutcome::Pass { .. } => format!("pass after {} steps", r.steps_run),
        DstOutcome::Fail(f) => {
            let mut s = format!("{:?} at step {}: {}\n", f.invariant, f.step, f.message);
            for t in f.trace.iter().rev().take(12).rev() {
                s.push_str(&format!("  {t:?}\n"));
            }
            s
        }
    }
}

#[test]
fn seq_sut_runs_without_faults() {
    let c = cfg(1, 2_000, &[], 0, &[]);
    let r = run_record::<E>(&c);
    eprintln!("SHOW {}", show(&r));
    assert!(r.outcome.is_pass(), "{}", show(&r));
    assert!(r.steps_run > 100, "{} steps", r.steps_run);
    // The takers trade: decisions reach the venues and fills come back.
    assert!(!r.frames.is_empty(), "no frame left the engine");
    assert!(
        r.journal.iter().any(|j| j
            .try_read::<hft_event::OrderEvent>()
            .is_some_and(|e| e.what == hft_event::OrderEventKind::Fill)),
        "no fill"
    );
}

#[test]
fn seq_sut_core_faults() {
    use FaultCategory::*;
    for seed in 1..=3u64 {
        let c = cfg(
            seed,
            2_000,
            &[Conn, Report, Reject, TwoLeg, Market, Account, UserStream, Control],
            30,
            &[],
        );
        let r = run_record::<E>(&c);
        assert!(r.outcome.is_pass(), "seed {seed}: {}", show(&r));
    }
}

#[test]
fn seq_sut_same_seed_same_trail() {
    let mut c = cfg(7, 1_000, &[], 0, &[]);
    c.rerun = RerunPolicy::Always;
    let r = run_record::<E>(&c);
    assert!(r.outcome.is_pass(), "{}", show(&r));
}

#[test]
fn tk_101_observes_last_oms_slot() {
    let suite = hft_testkit::scenarios::suites()
        .into_iter()
        .find(|s| s.name == "core")
        .expect("core suite");
    let mut c = suite.config(0xbe41_3439_dca2_4fad);
    c.steps = 780;
    c.rerun = RerunPolicy::Never;
    let r = run_record::<E>(&c);
    assert!(r.outcome.is_pass(), "{}", show(&r));
    assert_eq!(r.steps_run, c.steps);
    // This prefix reaches the highest valid slot. A check on an empty table would miss the
    // observer's old 0..capacity range, which silently omitted that slot.
    assert!(r.obs.tables.iter().any(|t| {
        r.obs
            .slots
            .iter()
            .any(|s| s.venue == t.venue && s.account == t.account && s.slot == t.capacity)
    }));
    for t in &r.obs.tables {
        let occupied: Vec<_> = r
            .obs
            .slots
            .iter()
            .filter(|s| s.venue == t.venue && s.account == t.account)
            .collect();
        assert_eq!(occupied.len() + t.free as usize, t.capacity as usize);
        assert!(occupied.iter().all(|s| (1..=t.capacity).contains(&s.slot)));
    }
}

/// Seed sweep in `hft-dst run` order (`DST_SEED`, `DST_N`; manual: `cargo test -p hft-testkit
/// --test testkit -- --ignored seq_sut_sweep --nocapture`).
#[test]
#[ignore]
fn seq_sut_sweep() {
    let from: u64 = std::env::var("DST_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let n: u64 = std::env::var("DST_N")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    let all = hft_testkit::scenarios::suites();
    let mut bad = Vec::new();
    for seed in from..from + n {
        let s = hft_testkit::dst::scenario::suite_for_seed(&all, seed).unwrap();
        let cfg = s.config(seed);
        let r = run_record::<E>(&cfg);
        if !r.outcome.is_pass() {
            eprintln!("seed {seed} suite {}: {}", s.name, show(&r));
            bad.push(seed);
        }
    }
    eprintln!("sweep {from}..{}: {} failures {bad:?}", from + n, bad.len());
}
