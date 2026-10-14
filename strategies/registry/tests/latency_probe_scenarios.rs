//! Core M3 engine scenarios. Long-duration acceptance remains listed in TESTS.md.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)]
#[path = "probe/common.rs"]
mod common;
use common::*;
use hft_event::{
    EventKind, ExecDecision, GwOp, LatencySample, Note, OrderDecision, OrderEvent, OrderEventKind,
};
use hft_testkit::dst::{
    sut::Sut,
    world::{ACCT_A, G1, P1, S1},
};
use hft_types::RejectBits;

#[test]
fn lpr_065_quote_core_ack_cancel_and_replay() {
    let w = world(0, false, 20_000_000);
    let mut r = Rig::new(&w);
    r.run(10);
    let new = r.sent.iter().filter(|f| f.cmd.op == GwOp::Place).count();
    let cancel = r.sent.iter().filter(|f| f.cmd.op == GwOp::Cancel).count();
    assert!(new >= 18, "no periodic probes: {:?}", r.sent);
    assert_eq!(cancel, new);
    assert_eq!(r.positions(), (0, 0));
    let journal = r.sut.journal();
    assert_eq!(
        journal
            .iter()
            .filter_map(|j| j.try_read::<OrderEvent>())
            .filter(|e| e.what == OrderEventKind::CancelAck)
            .count(),
        new
    );
    let samples: Vec<_> = journal
        .iter()
        .filter_map(|j| j.try_read::<LatencySample>())
        .collect();
    assert_eq!(samples.iter().filter(|s| s.action == 0).count(), new);
    assert_eq!(samples.iter().filter(|s| s.action == 1).count(), new);
    for s in samples {
        let input = journal.iter().find(|j| j.seq == s.trigger_seq).unwrap();
        assert_eq!(input.kind.0, s.trigger_kind);
        if s.action == 0 {
            assert_eq!(input.kind, EventKind::BBO);
        } else {
            let e = input.try_read::<OrderEvent>().unwrap();
            assert_eq!(e.what, OrderEventKind::Ack);
            assert_eq!((e.slot, e.r#gen, e.venue), (s.slot, s.r#gen, s.venue));
        }
    }
    r.replay();
}

#[test]
fn lpr_067_pair_cycle() {
    let mut r = Rig::new(&world(1, false, 20_000_000));
    r.run(50);
    assert_eq!(r.positions(), (0, 0));
    let places = r.sent.iter().filter(|f| f.cmd.op == GwOp::Place).count();
    assert!(places >= 16, "pair did not cycle: {places}");
    assert!(
        r.sut
            .journal()
            .iter()
            .filter_map(|j| j.try_read::<LatencySample>())
            .any(|s| s.link_role == 2)
    );
    assert!(
        r.sut
            .sequencer()
            .engine()
            .exec
            .cell(ACCT_A, G1)
            .is_none_or(|c| c.broken_since.0 == 0)
    );
    r.replay();
}

#[test]
fn lpr_066_xgw_samples() {
    let mut r = Rig::new(&world(0, true, 20_000_000));
    r.run(600);
    let journal = r.sut.journal();
    let first = r
        .sent
        .iter()
        .filter(|f| f.cmd.op == GwOp::Place && f.cmd.inst == S1)
        .count();
    let cross: Vec<_> = journal
        .iter()
        .filter_map(|j| j.try_read::<LatencySample>())
        .filter(|s| s.action == 1 && s.venue == r.sut.world().inst(S1).unwrap().venue)
        .collect();
    assert!(first >= 599);
    assert_eq!(cross.len(), first);
    for sample in cross {
        let e = journal
            .iter()
            .find(|j| j.seq == sample.trigger_seq)
            .unwrap()
            .try_read::<OrderEvent>()
            .unwrap();
        assert_eq!(e.what, OrderEventKind::Ack);
        assert_eq!(e.inst, P1);
        let cancels: Vec<_> = r
            .sent
            .iter()
            .filter(|f| f.cmd.op == GwOp::Cancel && f.trigger_seq == sample.trigger_seq)
            .collect();
        assert_eq!(cancels.len(), 2);
        assert_eq!((cancels[0].cmd.inst, cancels[1].cmd.inst), (P1, S1));
    }
    assert_eq!(r.positions(), (0, 0));
    r.replay();
}

#[test]
fn lpr_070_px_band_gated() {
    // Keep the engine fixture's 30m band and put far_bp above it on both legs.
    let mut r = Rig::new(&world(0, false, 40_000_000));
    r.run(600);
    r.feed(S1);
    r.feed(P1);
    assert!(!r.sent.iter().any(|f| f.cmd.op == GwOp::Place));
    let journal = r.sut.journal();
    let decisions: Vec<_> = journal
        .iter()
        .filter_map(|j| j.try_read::<OrderDecision>())
        .collect();
    assert_eq!(decisions.len(), 1_200);
    assert!(decisions.iter().all(|d| d.reject & RejectBits::PX_BAND.0 != 0));
    assert!(!journal.iter().any(|j| j.try_read::<LatencySample>().is_some()));
    let notes: Vec<_> = journal
        .iter()
        .filter_map(|j| j.try_read::<Note>())
        .filter(|n| n.code & 0xFF_FFFF == latency_probe::desc::PROBE_GATED)
        .collect();
    // Warm boot enables the strategy before the first scripted quote. The note flush at
    // 600 s contains 599 completed probes per leg; this boundary's two gates flush later.
    assert_eq!(notes.iter().map(|n| n.v[0]).sum::<u64>(), 1_198);
    assert!(notes.iter().all(|n| n.v[1] == u64::from(RejectBits::PX_BAND.0)));
    r.replay();
}

#[test]
fn lpr_068_target_cycle() {
    let mut r = Rig::new(&world(2, false, 20_000_000));
    r.run(180);
    assert_eq!(r.positions(), (0, 0));
    assert!(r.sent.iter().filter(|f| f.cmd.op == GwOp::Place).count() >= 12);
    assert!(
        r.sut
            .journal()
            .iter()
            .any(|j| j.try_read::<ExecDecision>().is_some())
    );
    assert!(
        r.sut
            .sequencer()
            .engine()
            .exec
            .cell(ACCT_A, G1)
            .is_none_or(|c| !c.has_target)
    );
    r.replay();
}
