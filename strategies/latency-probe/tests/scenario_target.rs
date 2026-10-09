//! target probes (TESTS.md LPR-041–LPR-048).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::type_complexity
)] // allow-list: test code

mod common;

use common::*;
use hft_strategy_sdk::{
    BrokenReason, ExecSpec, ExecUpdateKind, FirstLeg, IntentOutcome, PxMode, ResidualPolicy, StuckReason,
    TargetIntent, TargetOrigin, TargetReason, TargetReject, TargetState, TargetView, Urgency,
};
use hft_types::{Notional, Qty, Rate, RejectBits, Tsc};
use latency_probe::Phase;

fn due(h: &mut H) {
    h.bbo(I2);
    h.timer(GTS, 1);
    assert_eq!(h.phase(GTS), Phase::Due);
}

/// LPR-041 step 1: the opening target.
fn open(h: &mut H) -> TargetIntent {
    due(h);
    let rec = h.bbo(I1);
    assert_eq!((rec.targets.len(), rec.places.len()), (1, 0));
    assert_eq!(h.phase(GTS), Phase::Opening);
    rec.targets[0]
}

/// LPR-042.1: the opening target accepted (read at +10 ms).
fn accepted(h: &mut H) -> TargetIntent {
    let t = open(h);
    h.at(10);
    let rec = h.bbo(I2);
    assert!(rec.notes.is_empty());
    assert_eq!(h.phase(GTS), Phase::Opening);
    t
}

/// LPR-043 step 2: holding, then the 0/0 exit target (Flattening(1)).
fn exiting(h: &mut H) -> TargetIntent {
    accepted(h);
    h.at(1_000);
    h.net(GTS, 0, 100_000).net(GTS, 1, -100_000);
    h.exec(
        GTS,
        ExecUpdateKind::Done {
            cleared_by_engine: false,
        },
    );
    h.at(6_000);
    let rec = h.timer(GTS, 2);
    assert_eq!(rec.targets.len(), 1);
    assert_eq!((h.phase(GTS), h.group(GTS).attempt), (Phase::Flattening, 1));
    rec.targets[0]
}

#[test]
fn lpr_041_open_target_fields() {
    let mut h = H::new(&[], vec![gts()]);
    let t = open(&mut h);
    assert_eq!((t.account, t.group, t.n_legs), (T1, GTS, 2));
    assert_eq!(
        (t.legs[0].leg_ix, t.legs[0].qty, t.legs[1].leg_ix, t.legs[1].qty),
        (0, Qty(100_000), 1, Qty(-100_000))
    );
    assert_eq!(
        (t.reason, t.urgency, t.band, t.tag),
        (TargetReason::Rebalance, Urgency::Normal, Rate(500_000_000), 0)
    );
    let ExecSpec::PairOnFill(p) = t.exec else {
        panic!("exec {:?}", t.exec)
    };
    let s = p.slice;
    assert_eq!(
        (s.depth_frac, s.depth_levels, s.max_cross),
        (Rate(1_000_000_000), 1, Rate(5_000_000))
    );
    assert_eq!(
        (s.max_chunk_notional, s.min_chunk_notional, s.cooldown_ms),
        (Notional(120_000_020), Notional(0), 0)
    );
    assert_eq!(
        (p.first_leg, p.first_leg_ix, p.hedge_px_mode),
        (FirstLeg::Fixed, 0, PxMode::Cross)
    );
    assert_eq!(
        (p.hedge_cross, p.hedge_max_slip),
        (Rate(5_000_000), Rate(5_000_000))
    );
    assert_eq!(
        (p.ratio_num, p.ratio_den, p.fix_tolerance, p.residual),
        (1, 1, Rate(0), ResidualPolicy::TrimExcess)
    );
    assert_eq!(h.group(GTS).next_pair_ens, E0 + 60_000_000_000);
}

#[test]
fn lpr_042_target_outcomes() {
    let rows = [
        ("LPR-042.1", IntentOutcome::TargetAccepted, Phase::Opening, None),
        ("LPR-042.2", IntentOutcome::TargetShadowed, Phase::Opening, None),
        (
            "LPR-042.3",
            IntentOutcome::TargetRejected(TargetReject::BadLegs),
            Phase::Idle,
            Some(2),
        ),
        (
            "LPR-042.4",
            IntentOutcome::TargetRejected(TargetReject::LotMismatch),
            Phase::Blocked,
            Some(10),
        ),
    ];
    for (row, o, phase, note) in rows {
        let mut h = H::new(&[], vec![gts()]);
        due(&mut h);
        h.next_outcomes = Some(vec![o]);
        h.bbo(I1);
        h.at(10);
        let rec = h.bbo(I2);
        assert_eq!(h.phase(GTS), phase, "{row}");
        let n = rec.notes_of(9);
        match note {
            Some(v0) => assert_eq!((n.len(), n[0].v[0]), (1, v0), "{row}"),
            None => assert!(rec.notes.is_empty(), "{row}"),
        }
        if phase == Phase::Idle {
            assert_eq!(rec.timers.iter().filter(|t| t.tag == 1).count(), 1, "{row}");
        }
    }
}

#[test]
fn lpr_043_target_cycle() {
    let mut h = H::new(&[], vec![gts()]);
    let t0 = accepted(&mut h);
    h.at(1_000);
    h.net(GTS, 0, 100_000).net(GTS, 1, -100_000);
    let rec = h.exec(
        GTS,
        ExecUpdateKind::Done {
            cleared_by_engine: false,
        },
    );
    assert_eq!(h.phase(GTS), Phase::Holding);
    assert_eq!(
        (rec.timers[0].at, rec.timers[0].tag, rec.timers[0].group),
        (h.tsc_of(E0 + 6_000_000_000), 2, GTS)
    );
    h.at(6_000);
    let rec = h.timer(GTS, 2);
    let t = rec.targets[0];
    assert_eq!(
        (t.reason, t.legs[0].qty, t.legs[1].qty, t.tag),
        (TargetReason::Exit, Qty(0), Qty(0), 0)
    );
    assert_eq!(t.exec, t0.exec);
    assert_eq!((h.phase(GTS), h.group(GTS).attempt), (Phase::Flattening, 1));
    h.at(7_000);
    h.flat(GTS);
    let rec = h.exec(
        GTS,
        ExecUpdateKind::Done {
            cleared_by_engine: false,
        },
    );
    assert_eq!(rec.clears, vec![(T1, GTS)]);
    assert_eq!(h.phase(GTS), Phase::Idle);
    let tick: Vec<_> = rec.timers.iter().filter(|t| t.tag == 1).collect();
    assert_eq!((tick.len(), tick[0].at), (1, h.tsc_of(E0 + 60_000_000_000)));
}

#[test]
fn lpr_044_stuck_opening() {
    let mut h = H::new(&[], vec![gts()]);
    accepted(&mut h);
    let rec = h.exec(GTS, ExecUpdateKind::Stuck(StuckReason::NoLiquidity));
    let n = rec.notes_of(8);
    assert_eq!((n.len(), n[0].v[0], n[0].v[1]), (1, 3, 1));
    assert_eq!(rec.targets.len(), 1);
    let t = rec.targets[0];
    assert_eq!(
        (t.reason, t.legs[0].qty, t.legs[1].qty, t.tag),
        (TargetReason::Exit, Qty(0), Qty(0), 0)
    );
    assert_eq!((h.phase(GTS), h.group(GTS).attempt), (Phase::Flattening, 1));
}

#[test]
fn lpr_045_stuck_closing_retry() {
    let mut h = H::new(&[], vec![gts()]);
    let first = exiting(&mut h);
    h.at(7_000);
    let rec = h.exec(GTS, ExecUpdateKind::Stuck(StuckReason::SliceFails));
    let n = rec.notes_of(8);
    assert_eq!((n.len(), n[0].v[0], n[0].v[1]), (1, 5, 2));
    assert!(rec.targets.is_empty());
    assert_eq!(
        (rec.timers[0].at, rec.timers[0].tag),
        (h.tsc_of(ms(7_000) + 5_000_000_000), 2)
    );
    h.at(12_000);
    let rec = h.timer(GTS, 2);
    assert_eq!(rec.targets.len(), 1);
    let t = rec.targets[0];
    assert_eq!(t.tag, 1);
    assert_eq!(TargetIntent { tag: 0, ..t }, first);
    assert_eq!((h.phase(GTS), h.group(GTS).attempt), (Phase::Flattening, 2));
    h.at(13_000);
    let rec = h.exec(GTS, ExecUpdateKind::Stuck(StuckReason::SliceFails));
    assert_eq!(rec.clears, vec![(T1, GTS)]);
    let n = rec.notes_of(6);
    assert_eq!((n.len(), n[0].v[0], n[0].v[1]), (1, 120_000_050, 1));
    assert_eq!(h.phase(GTS), Phase::Blocked);
}

#[test]
fn lpr_046_stuck_shadow_gated() {
    let rows = [
        ("LPR-046.1", StuckReason::Shadow, 2),
        ("LPR-046.2", StuckReason::Gated(RejectBits::CLOSE_ONLY), 1),
    ];
    for (row, r, code) in rows {
        let mut h = H::new(&[], vec![gts()]);
        exiting(&mut h);
        let rec = h.exec(GTS, ExecUpdateKind::Stuck(r));
        let n = rec.notes_of(8);
        assert_eq!((n.len(), n[0].v[0], n[0].v[1]), (1, code, 2), "{row}");
        assert!(
            rec.targets.is_empty() && rec.timers.iter().all(|t| t.tag != 2),
            "{row}"
        );
        assert_eq!(
            (h.phase(GTS), h.group(GTS).attempt),
            (Phase::Flattening, 1),
            "{row}"
        );
    }
}

#[test]
fn lpr_047_active_target_blocks_due() {
    let mut h = H::new(&[], vec![gts()]);
    let t = open(&mut H::new(&[], vec![gts()]));
    let tv = TargetView {
        intent: t,
        origin: TargetOrigin::Strategy,
        state: TargetState::Working,
        set_seq: 1,
        set_tsc: Tsc(1),
        last_slice_tsc: Tsc(0),
        n_slices: 0,
        filled: [Qty(0); 4],
    };
    h.bbo(I2);
    hft_strategy_sdk::testing::MockViews::target(&mut h.v.m, T1, GTS, Some(tv));
    let mut timers = Vec::new();
    let rec = h.timer(GTS, 1);
    timers.extend(rec.timers);
    assert!(rec.targets.is_empty());
    h.at(1);
    let rec = h.bbo(I1);
    timers.extend(rec.timers);
    assert!(rec.targets.is_empty());
    assert_eq!(h.phase(GTS), Phase::Idle);
    let ticks: Vec<_> = timers.iter().filter(|t| t.tag == 1).collect();
    assert_eq!((ticks.len(), ticks[0].at), (1, h.tsc_of(E0 + 60_000_000_000)));
}

#[test]
fn lpr_048_broken_then_cleared() {
    let rows = [("LPR-048.1", 0), ("LPR-048.2", 1), ("LPR-048.3", 2)];
    for (row, k) in rows {
        let mut h = H::new(&[], vec![gts()]);
        accepted(&mut h);
        h.exec(GTS, ExecUpdateKind::Broken(BrokenReason::ReverseFailed));
        assert_eq!(h.phase(GTS), Phase::Blocked, "{row}");
        match k {
            0 => {
                h.net(GTS, 0, 100_000);
                let rec = h.timer(GTS, 1);
                assert_eq!((rec.intents(), h.phase(GTS)), (0, Phase::Blocked), "{row}");
            }
            1 => {
                h.net(GTS, 0, 100_000);
                let rec = h.exec(
                    GTS,
                    ExecUpdateKind::Done {
                        cleared_by_engine: true,
                    },
                );
                assert_eq!(rec.targets.len(), 1, "{row}");
                let t = rec.targets[0];
                assert_eq!(
                    (t.reason, t.legs[0].qty, t.legs[1].qty),
                    (TargetReason::Exit, Qty(0), Qty(0)),
                    "{row}"
                );
                assert_eq!(
                    (h.phase(GTS), h.group(GTS).attempt),
                    (Phase::Flattening, 1),
                    "{row}"
                );
            }
            _ => {
                let rec = h.exec(
                    GTS,
                    ExecUpdateKind::Done {
                        cleared_by_engine: true,
                    },
                );
                assert_eq!((rec.intents(), h.phase(GTS)), (0, Phase::Idle), "{row}");
            }
        }
    }
}
