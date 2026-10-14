//! Deterministic engine faults for real probe strategies (core subset of LPR-071).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)]
#[path = "probe/common.rs"]
mod common;
use common::world;
use hft_event::{ExecDecision, GwOp, OrderDecision, OrderEvent, OrderEventKind};
use hft_strategy_registry::Strategies;
use hft_testkit::dst::{
    DstConfig, Fault, MarketGen, MarketSource, MockVenueCfg, RerunPolicy, SeqSut, run_record,
};
use hft_types::VenueId;

#[test]
fn lpr_071_dst_core_faults() {
    for seed in [7, 19, 41, 83, 137, 199] {
        let mut cfg = DstConfig::new(seed, "latency-probe");
        cfg.world = world(0, false, 20_000_000);
        cfg.venues = cfg.world.venues.iter().map(|v| MockVenueCfg::w1(v.id)).collect();
        cfg.steps = 600;
        cfg.horizon_ns = 15_000_000_000;
        cfg.market = MarketSource::Generated(MarketGen {
            tick_ms: 100,
            walk_centi_bp: 0,
        });
        cfg.actors.ctl = false;
        cfg.actors.advisor = false;
        cfg.rerun = RerunPolicy::Always;
        cfg.rebuild_every = 200;
        cfg.scripted = vec![
            (
                2_000_000_000,
                Fault::ReportDup {
                    venue: VenueId(1),
                    dur_ms: 2_000,
                },
            ),
            (
                4_000_000_000,
                Fault::RateLimit {
                    venue: VenueId(10),
                    dur_ms: 2_000,
                },
            ),
            (
                7_000_000_000,
                Fault::ReportDelay {
                    venue: VenueId(1),
                    max_ms: 200,
                    dur_ms: 2_000,
                },
            ),
        ];
        let r = run_record::<SeqSut<Strategies>>(&cfg);
        assert!(r.outcome.is_pass(), "seed {seed}: {:?}", r.outcome.failure());
        assert!(r.journal_verification.is_some());
        assert!(r.compared[6] > 0 && r.compared[9] > 0);
        assert!(r.frames.iter().any(|(_, f)| f.cmd.op == GwOp::Place));
        assert!(
            r.journal
                .iter()
                .filter_map(|j| j.try_read::<OrderEvent>())
                .any(|e| e.what == OrderEventKind::Reject),
            "fault must reject a real probe"
        );
        let mut last = [None; 256];
        for (time, f) in &r.frames {
            if f.cmd.op == GwOp::Place {
                let ix = f.cmd.inst.0 as usize;
                if let Some(prev) = last[ix] {
                    assert!(time - prev >= 1_000_000_000, "rejection cannot accelerate probes");
                }
                last[ix] = Some(*time);
            }
        }
    }
}

#[test]
fn lpr_069_second_leg_rejected() {
    let mut cfg = DstConfig::new(7, "latency-probe");
    cfg.world = world(1, false, 20_000_000);
    cfg.world.risk.broken_auto_fix_ms = 2_000;
    cfg.venues = cfg.world.venues.iter().map(|v| MockVenueCfg::w1(v.id)).collect();
    cfg.steps = 2_500;
    cfg.horizon_ns = 31_000_000_000;
    cfg.market = MarketSource::Generated(MarketGen {
        tick_ms: 100,
        walk_centi_bp: 0,
    });
    cfg.actors.ctl = false;
    cfg.actors.advisor = false;
    cfg.rerun = RerunPolicy::Always;
    cfg.rebuild_every = 500;
    cfg.scripted = vec![(
        1,
        Fault::PersistentSecondReject {
            venue: VenueId(10),
            dur_ms: 31_000,
            autofix_rejected: false,
            hedge_venue_down: false,
        },
    )];
    let r = run_record::<SeqSut<Strategies>>(&cfg);
    assert!(r.outcome.is_pass(), "{:?}", r.outcome.failure());
    let broken = r
        .journal
        .iter()
        .find(|j| j.try_read::<ExecDecision>().is_some_and(|e| e.state_to == 4))
        .expect("engine must enter Broken")
        .seq;
    assert!(
        r.journal
            .iter()
            .any(|j| j.try_read::<ExecDecision>().is_some_and(|e| e.action == 14)),
        "bounded engine AutoFix must run"
    );
    assert!(
        !r.journal
            .iter()
            .filter(|j| j.seq > broken)
            .filter_map(|j| j.try_read::<OrderDecision>())
            .any(|d| d.strategy.0 == 1 && d.link_role == 1),
        "blocked strategy must not start an opening/closing link"
    );
    assert!(r.journal_verification.is_some());
    assert!(r.compared[6] > 0 && r.compared[9] > 0);
}
