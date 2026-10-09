//! Keyframes (TESTS.md SEQ-085–SEQ-089, SEQ-245–SEQ-247).

use hft_event::{EventKind, Role, Timer};
use hft_sequencer::StepOutcome;
use hft_sequencer::driver::live::RingId;
use hft_types::InstId;

use crate::common::*;

fn timers_since(h: &H, m: usize) -> Vec<(Timer, u64)> {
    h.since(m)
        .iter()
        .filter(|r| r.kind == EventKind::TIMER && r.role() == Role::Input)
        .filter_map(|r| r.read::<Timer>().map(|t| (t, r.tsc)))
        .collect()
}

fn sys_ids(h: &H, m: usize) -> Vec<u32> {
    timers_since(h, m)
        .iter()
        .filter(|t| t.0.owner == 1)
        .map(|t| t.0.timer_id)
        .collect()
}

#[test]
fn seq_085_keyframe_round_conditions() {
    reset_script();
    let mut b = b0();
    b.seq.keyframe_period_ms = 1_000;
    let mut h = b.boot();
    h.steady();
    h.run();
    // .6 not due: Idle.
    assert_eq!(h.step(), StepOutcome::Idle);
    // .2 due, previous round idle but bbo-A non-empty: not written, bbo taken.
    h.adv(2_000_000_000);
    h.quote(A1, "50000", "50003");
    let m = h.mark();
    h.step();
    assert!(!sys_ids(&h, m).contains(&1));
    assert!(h.since(m).iter().any(|r| r.kind == EventKind::BBO));
    // .3 previous round Busy, rings empty: not written.
    let m = h.mark();
    h.step();
    assert!(!sys_ids(&h, m).contains(&1));
    // .1 previous round idle, rings empty: only the Snapshot Timer.
    let m = h.mark();
    h.step();
    let inp: Vec<EventKind> = h
        .since(m)
        .iter()
        .filter(|r| r.role() == Role::Input)
        .map(|r| r.kind)
        .collect();
    assert_eq!(inp, vec![EventKind::TIMER]);
    assert_eq!(sys_ids(&h, m), vec![1]);
}

#[test]
fn seq_086_time_trigger_boundary() {
    for (dt, write) in [
        (299_999_000_000u64, false),
        (300_000_000_000, true),
        (300_001_000_000, true),
    ] {
        let mut h = {
            reset_script();
            let mut h = b0().boot();
            h.steady();
            h
        };
        let kf = h.keyframe();
        let t0 = timers_since(&h, 0)
            .into_iter()
            .filter(|t| t.0.owner == 1 && t.0.timer_id == 1)
            .last()
            .unwrap()
            .1;
        let _ = kf;
        h.at(t0 + dt);
        // The idle round, then the keyframe round (ConflateReport / AdvisorTick may come first).
        let m = h.mark();
        for _ in 0..4 {
            h.step();
        }
        assert_eq!(sys_ids(&h, m).contains(&1), write, "dt {dt}");
    }
}

#[test]
fn seq_087_event_count_trigger_boundary() {
    for (n, write) in [(999usize, false), (1_000, true), (1_001, true)] {
        reset_script();
        let mut b = b0();
        b.seq.keyframe_max_events = 1_000;
        let mut h = b.boot();
        h.steady();
        h.keyframe();
        let since = h.s.view().inputs_since_keyframe();
        assert_eq!(since, 0);
        let mut k = 0;
        while (h.s.view().inputs_since_keyframe() as usize) < n {
            k += 1;
            h.push(
                RingId::Rep(0),
                &conn(
                    A,
                    1,
                    hft_event::ConnStateKind::Alive,
                    hft_event::ConnRole::Md,
                    InstId(0),
                    hft_types::AccountId(0),
                    k,
                ),
            );
            h.step();
        }
        let m = h.mark();
        h.step();
        h.step();
        assert_eq!(sys_ids(&h, m).contains(&1), write, "n {n}");
    }
}

#[test]
fn seq_088_max_deferral() {
    reset_script();
    let mut b = b0();
    b.seq.keyframe_period_ms = 1_000;
    let mut h = b.boot();
    h.steady();
    h.keyframe();
    let t = h.now() + 1_000_000_000;
    // Never idle: a Bbo every round.
    let mut written_at = None;
    let mut k = 0u64;
    while written_at.is_none() && k < 400 {
        k += 1;
        h.at(t + k * 10_000_000);
        h.quote(A1, "50000", if k % 2 == 0 { "50003" } else { "50004" });
        let m = h.mark();
        h.step();
        if sys_ids(&h, m).contains(&1) {
            written_at = Some(h.now());
            let inp: Vec<EventKind> = h
                .since(m)
                .iter()
                .filter(|r| r.role() == Role::Input)
                .map(|r| r.kind)
                .collect();
            assert_eq!(inp, vec![EventKind::TIMER]);
        }
    }
    let at = written_at.expect("forced keyframe");
    assert!(at >= t + 2_000_000_000 && at < t + 2_020_000_000, "{}", at - t);
    let m = h.verify();
    assert!(m.is_empty(), "{m:?}");
}

#[test]
fn seq_245_keyframe_one_shot() {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    h.quote(A1, "50000", "50004");
    h.run();
    let p0 = h.j().publishes;
    let m = h.mark();
    h.sys(1);
    let recs = h.since(m);
    let inputs: Vec<_> = recs.iter().filter(|r| r.role() == Role::Input).collect();
    assert_eq!(inputs.len(), 1);
    let s = recs.iter().find(|r| r.kind == EventKind::STATE_SNAPSHOT).unwrap();
    let (hd, _) = snap(s);
    assert_eq!(hd.purpose, 2);
    assert_eq!(hd.state_hash, [0; 16]);
    assert_ne!(hd.out_hash, [0; 16]);
    assert!(h.j().publishes > p0);
}

#[test]
fn seq_246_keyframe_sections_equal_on_demand() {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    let k = h.keyframe();
    h.cmd(AC1, &hft_event::ControlCmd::SnapshotNow);
    h.run();
    h.run();
    let o = h.snapshots().pop().unwrap();
    let (kh, ks) = snap(&k);
    let (oh, os) = snap(&o);
    assert_eq!(kh.purpose, 2);
    assert_eq!(oh.purpose, 3);
    // Sections other than the ones the intermediate Inputs (SnapshotNow command, its Timer)
    // change are identical; compare this crate's stable sections.
    assert_eq!(ks.strategies, os.strategies);
    assert_eq!(ks.market, os.market);
    assert_eq!(ks.engine_clock, os.engine_clock);
    assert_eq!(ks.conns, os.conns);
    assert_ne!(oh.state_hash, [0; 16]);
}

#[test]
fn seq_247_reader_side_keyframe_hash() {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    // Two snapshots at the same state: a keyframe and an on-demand one written by the same
    // Input position class; the reader's hash of the keyframe sections equals the computed
    // hash of an on-demand snapshot of the same state.
    let k = h.keyframe();
    let (_, _) = snap(&k);
    let hk = hft_sequencer::snapshot::state_hash(&k.body[96..]);
    // Recompute from the engine state at the same point (no Input in between).
    let mut w = hft_strategy_sdk::SnapshotWriter::new(64 << 20);
    h.s.write_sections(&mut w);
    let ho = hft_sequencer::snapshot::state_hash(w.as_bytes());
    assert_eq!(hk, ho);
    let m = h.verify_from(Some(k.seq));
    assert!(m.is_empty(), "{m:?}");
    let _ = (StepOutcome::Idle, RingId::Ctl, InstId(0));
}
