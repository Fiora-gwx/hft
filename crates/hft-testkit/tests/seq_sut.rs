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
fn tk_105_real_engine_late_fee() {
    use hft_event::{OrderEvent, OrderEventKind};
    use hft_testkit::dst::faults::Fault;
    use hft_testkit::dst::world::{USDT, VS};
    for delay_ms in [50, 6_000] {
        let mut c = cfg(1, 10_000, &[], 0, &[]);
        c.scripted.push((0, Fault::LateFeeOnly { venue: VS, delay_ms }));
        let r = run_record::<E>(&c);
        assert!(r.outcome.is_pass(), "delay={delay_ms}: {}", show(&r));
        assert_eq!(r.steps_run, c.steps);
        let fees: Vec<_> = r
            .journal
            .iter()
            .filter_map(|j| j.try_read::<OrderEvent>())
            .filter(|e| e.what == OrderEventKind::FeeOnly)
            .collect();
        assert_eq!(fees.len(), 1, "the scripted fault must actually reach the engine");
        assert!(fees[0].fee.0 > 0);
        assert_eq!(fees[0].fee_asset, USDT);
    }
}

#[test]
fn tk_106_real_base_fee_and_retired_links() {
    use hft_event::{OrderAction, OrderDecision, OrderEvent, OrderEventKind};
    use hft_refdata::FeeAssetRule;
    use hft_testkit::dst::faults::Fault;
    use hft_testkit::dst::invariants::{InvId, InvState, StepView, checker};
    use hft_testkit::dst::obs::{Obs, StepInfo};
    use hft_testkit::dst::seq::build;
    use hft_testkit::dst::world::{BTC, VP, VS};
    let check = |o: &Obs| {
        let info = StepInfo::default();
        (checker(InvId::I8).unwrap().step)(
            &mut InvState::default(),
            &StepView {
                step: 0,
                now_ns: 0,
                records: &[],
                delivered: None,
                frames: &[],
                info: &info,
                before: o,
                after: o,
                truth: &[],
                rebuild: None,
            },
        )
    };
    for delay_ms in [50, 6_000] {
        let mut c = cfg(1, 10_000, &[], 0, &[]);
        // Keep the opening opportunity and cycle the position fast enough to exercise reuse
        // within the fixed 10,000-step budget; the ordinary suites keep their market settings.
        c.market = hft_testkit::dst::MarketSource::Generated(hft_testkit::dst::MarketGen {
            tick_ms: 50,
            walk_centi_bp: 0,
        });
        for s in &mut c.world.strategies {
            use hft_strategy_sdk::ParamSet;
            use hft_testkit::strategies::probe_taker::TakerParams;
            s.params = hft_strategy_sdk::params::encode_body(
                &TakerParams::SCHEMA,
                1,
                &hft_strategy_sdk::params::RawParams {
                    defaults: vec![build::TAKER_SPREAD_TH, 1, 7, 100],
                    overrides: Vec::new(),
                },
            )
            .unwrap();
        }
        c.world
            .venues
            .iter_mut()
            .find(|v| v.id == VS)
            .unwrap()
            .fee_in_base = true;
        for all_sim in [false, true] {
            let rd = build::load_refdata(&build::refdata_blob(&c.world, all_sim).unwrap()).unwrap();
            assert_eq!(rd.caps(VS).fee_asset_rule, FeeAssetRule::Configured(BTC));
        }
        c.scripted.push((0, Fault::LateFeeOnly { venue: VS, delay_ms }));
        let r = run_record::<E>(&c);
        assert!(r.outcome.is_pass(), "delay={delay_ms}: {}", show(&r));
        assert_eq!(r.steps_run, c.steps);
        let fees: Vec<_> = r
            .journal
            .iter()
            .filter_map(|j| j.try_read::<OrderEvent>())
            .filter(|e| e.what == OrderEventKind::FeeOnly)
            .collect();
        assert_eq!(fees.len(), 1, "delay={delay_ms}: fee fault must be delivered");
        assert!(fees[0].fee.0 > 0);
        assert_eq!(fees[0].fee_asset, BTC);
        let orders: Vec<_> = r
            .journal
            .iter()
            .filter_map(|j| j.try_read::<OrderDecision>())
            .filter(|d| d.action == OrderAction::New && d.reject == 0 && d.venue == VS)
            .collect();
        for side in [hft_types::Side::Buy, hft_types::Side::Sell] {
            assert!(
                orders.iter().any(|d| d.side == side),
                "base fee must cover {side:?}"
            );
        }
        assert!(
            orders.iter().any(|d| d.r#gen > 1),
            "delay={delay_ms}: no recycled slot used; orders={}, end_ns={}, history={:?}",
            orders.len(),
            r.end_ns,
            r.obs.hedge_history
        );
        assert!(
            r.obs.hedge_history.iter().any(|h| h.links > 0),
            "no retired link observed"
        );
        let ix = r
            .obs
            .legs
            .iter()
            .position(|l| l.fee_adjust != 0)
            .expect("no independently observed fee correction");
        assert!(check(&r.obs).is_ok());
        let mut wrong = r.obs.clone();
        wrong.legs[ix].fee_adjust = 0;
        assert!(check(&wrong).is_err(), "missing correction accepted");
        let mut wrong = r.obs.clone();
        wrong.legs[ix].residual += 1;
        assert!(check(&wrong).is_err(), "wrong residual accepted");
        let mut wrong = r.obs.clone();
        let h = wrong.hedge_history.iter_mut().find(|h| h.first_net != 0).unwrap();
        h.first_net += 1;
        assert!(check(&wrong).is_err(), "lost historical fill accepted");
        let mut unsupported = c.world.clone();
        unsupported
            .venues
            .iter_mut()
            .find(|v| v.id == VP)
            .unwrap()
            .fee_in_base = true;
        assert!(build::refdata_blob(&unsupported, false).is_err());
        let mut unsupported = c.world.clone();
        let mut other = unsupported.insts.iter().find(|i| i.venue == VS).unwrap().clone();
        other.base = hft_types::AssetId(99);
        unsupported.insts.push(other);
        assert!(build::refdata_blob(&unsupported, false).is_err());
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

#[test]
fn tk_102_observes_recycled_slot_tombs() {
    let suite = hft_testkit::scenarios::suites()
        .into_iter()
        .find(|s| s.name == "core")
        .expect("core suite");
    let mut c = suite.config(0xbe41_3439_dca2_4fad);
    c.steps = 10_000;
    c.rerun = RerunPolicy::Never;
    let r = run_record::<E>(&c);
    assert!(r.outcome.is_pass(), "{}", show(&r));
    assert_eq!(r.steps_run, c.steps);
    let mut latest = std::collections::BTreeMap::new();
    for d in r
        .journal
        .iter()
        .filter_map(|j| j.try_read::<hft_event::OrderDecision>())
    {
        if d.action == hft_event::OrderAction::New && d.reject == 0 {
            latest.insert((d.venue.0, d.account.0, d.slot.0), d.r#gen);
        }
    }
    assert!(
        r.obs.tombs.iter().any(|t| {
            latest.get(&(t.venue.0, t.account.0, t.slot)) == Some(&t.r#gen)
                && !r
                    .obs
                    .slots
                    .iter()
                    .any(|s| s.venue == t.venue && s.account == t.account && s.slot == t.slot)
        }),
        "no tomb of a free slot's latest allocated generation was observed"
    );
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
