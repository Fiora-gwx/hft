//! Environment decisions written as Inputs (TESTS.md "环境决定写成 Input": SEQ-079–SEQ-098).

use hft_event::{ControlCmd, EventKind, Role, Timer};
use hft_journal::{RollAt, RollReason};
use hft_sequencer::StepOutcome;
use hft_sequencer::driver::live::RingId;
use hft_strategy_sdk::EventRef;
use hft_types::{GroupId, InstId, Tsc};

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
fn seq_097_env_positions_differ_verify_passes() {
    let mut positions = Vec::new();
    for idle_ms in [50, 1000] {
        reset_script();
        let mut b = b0();
        b.seq.keyframe_period_ms = 200;
        let mut h = b.boot();
        h.steady();
        let start = tsc_of(UTC0 + 4 * H + 30 * MIN - SECS / 2);
        h.at(start);
        h.keyframe();
        h.run();
        let mark = h.mark();
        for ms in 1..=1000 {
            h.at(start + ms * MS);
            h.push(RingId::Md(0), &trade(A1, ms));
            h.step();
            if ms % idle_ms == 0 {
                for _ in 0..6 {
                    h.step();
                }
            }
        }
        h.run();
        let recs = h.since(mark);
        let seqs: Vec<_> = [2, 1]
            .into_iter()
            .map(|id| {
                recs.iter()
                    .filter(|r| r.kind == EventKind::TIMER)
                    .find(|r| {
                        r.read::<Timer>()
                            .is_some_and(|t| t.owner == 1 && t.timer_id == id)
                    })
                    .expect("environment timer")
                    .seq
            })
            .collect();
        assert!(h.verify().is_empty(), "idle {idle_ms}");
        positions.push(seqs);
    }
    assert_ne!(positions[0][0], positions[1][0]);
    assert_ne!(positions[0][1], positions[1][1]);
}

/// B0 with keyframes out of the way.
fn quiet() -> H {
    reset_script();
    let mut b = b0();
    b.seq.keyframe_period_ms = 4_000_000_000;
    let mut h = b.boot();
    h.steady();
    h
}

/// UTC → TSC of the B0 run (1 GHz from UTC0 at T0).
fn tsc_of(utc: i64) -> u64 {
    T0 + (utc - UTC0) as u64
}

const H: i64 = 3_600_000_000_000;
const MIN: i64 = 60_000_000_000;
const SECS: i64 = 1_000_000_000;

/// An idle round, then the round after it; returns the Sys timer ids of that second round.
fn idle_then_round(h: &mut H) -> Vec<u32> {
    h.step();
    h.step();
    let m = h.mark();
    h.step();
    sys_ids(h, m)
}

#[test]
fn seq_079_heap_timer_to_input() {
    let mut h = quiet();
    set_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            let _ = ctx.timer_at(Tsc(9_000_000), 9, G1);
        }
    });
    h.quote(A1, "50000", "50003");
    h.run();
    clear_hook();
    clear_log();
    h.at(9_000_500);
    let m = h.mark();
    h.step();
    let t = timers_since(&h, m)
        .into_iter()
        .find(|t| t.0.owner == 17)
        .expect("timer");
    assert_eq!(t.0.timer_id, 9);
    assert_eq!(t.0.group, G1);
    assert_eq!(t.0.deadline_tsc, 9_000_000);
    assert_eq!(t.1, 9_000_500);
    let got = log()
        .into_iter()
        .find(|g| g.kind == "Timer" && g.s == S1)
        .expect("delivered");
    assert!(got.text.contains("tag: 9"));
    assert!(!h.s.engine().heap.sorted().iter().any(|e| e.owner == 17));
}

#[test]
fn seq_080_timer_owner_dispatch() {
    let mut h = quiet();
    // .1 Sweep: re-registered +1 000 ms.
    let sweep =
        h.s.engine()
            .heap
            .sorted()
            .into_iter()
            .find(|e| e.owner == 3 && e.timer_id == 2)
            .expect("sweep");
    h.at(sweep.deadline);
    h.trace_on();
    h.step();
    assert!(h.trace().contains(&"timer"));
    let next =
        h.s.engine()
            .heap
            .sorted()
            .into_iter()
            .find(|e| e.owner == 3 && e.timer_id == 2)
            .expect("sweep again");
    assert_eq!(next.deadline, h.now() + 1_000_000_000);
    // .6 a strategy timer only reaches its instance.
    set_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            let _ = ctx.timer_at(Tsc(hx.v.now().tsc.0 + 10), 9, G1);
        }
    });
    h.quote(A1, "50000", "50003");
    h.run();
    clear_hook();
    clear_log();
    h.adv(100);
    h.run();
    let got: Vec<Got> = log().into_iter().filter(|g| g.kind == "Timer").collect();
    assert!(got.iter().all(|g| g.s == S1) && !got.is_empty());
}

#[test]
fn seq_082_hourly_roll_funding_window() {
    // .1 05:00:00.500 (the run started at 00:30): an idle round writes Roll.
    let mut h = quiet();
    h.at(tsc_of(UTC0 + 4 * H + 30 * MIN + SECS / 2));
    let m = h.mark();
    let mut outs = Vec::new();
    for _ in 0..6 {
        outs.push(h.step());
    }
    let ids = sys_ids(&h, m);
    assert!(ids.contains(&2), "{ids:?} {outs:?}");
}

fn needs_roll(h: &mut H, at: Option<RollAt>) {
    h.io().j.faults.needs_roll = at.map(|a| (RollReason::Size, a));
}

#[test]
fn seq_083_size_roll_two_levels() {
    // .1 IdleRound while never idle: no Roll.
    let mut h = quiet();
    needs_roll(&mut h, Some(RollAt::IdleRound));
    let m = h.mark();
    for k in 0..5 {
        h.quote(A1, "50000", if k % 2 == 0 { "50003" } else { "50004" });
        h.step();
    }
    assert!(!sys_ids(&h, m).contains(&2));
    // .2 an idle round: the next round writes Roll.
    let m = h.mark();
    for _ in 0..4 {
        h.step();
    }
    needs_roll(&mut h, None);
    let ids = sys_ids(&h, m);
    assert!(ids.contains(&2), "{ids:?}");
    // .4 EventBoundary: without waiting for an idle round.
    let mut h = quiet();
    needs_roll(&mut h, Some(RollAt::EventBoundary));
    let m = h.mark();
    h.quote(A1, "50000", "50003");
    h.step();
    h.quote(A1, "50000", "50004");
    h.step();
    needs_roll(&mut h, None);
    assert!(sys_ids(&h, m).contains(&2));
    // .6 None.
    let mut h = quiet();
    needs_roll(&mut h, None);
    let m = h.mark();
    idle_then_round(&mut h);
    assert!(!sys_ids(&h, m).contains(&2));
}

#[test]
fn seq_090_snapshot_now() {
    let mut h = quiet();
    let id = h.cmd(AC1, &ControlCmd::SnapshotNow);
    h.step();
    assert_eq!(h.result_of(id).map(|r| r.status), Some(0));
    assert!(h.s.view().snapshot_requested());
    let m = h.mark();
    for k in 0..3 {
        h.quote(A1, "50000", if k % 2 == 0 { "50003" } else { "50004" });
        h.step();
    }
    assert!(!sys_ids(&h, m).contains(&7));
    h.step();
    let m2 = h.mark();
    h.step();
    assert_eq!(sys_ids(&h, m2), vec![7]);
    let snap_rec = h
        .since(m2)
        .into_iter()
        .find(|r| r.kind == EventKind::STATE_SNAPSHOT)
        .expect("snapshot");
    assert_eq!(snap(&snap_rec).0.purpose, 3);
    assert!(!h.s.view().snapshot_requested());
}

#[test]
fn seq_091_day_roll() {
    let mut h = quiet();
    assert_eq!(h.s.view().last_day_rolled(), 20_727);
    // .1 2026-10-02T00:00:00.001Z.
    h.at(tsc_of(UTC0 + 23 * H + 30 * MIN + 1_000_000));
    h.trace_on();
    let m = h.mark();
    h.step();
    h.step();
    let ids = sys_ids(&h, m);
    assert!(ids.contains(&(3 | (20_728 << 8))), "{ids:?}");
    assert_eq!(ids.iter().filter(|i| **i & 0xFF == 3).count(), 1);
    let tr = h.trace();
    let r = tr.iter().position(|x| *x == "risk.on_day_roll").unwrap();
    let l = tr.iter().position(|x| *x == "ledger.on_day_roll").unwrap();
    assert!(r < l);
    assert_eq!(h.s.view().last_day_rolled(), 20_728);
    // .3/.4 repeated and older days do nothing.
    for d in [20_728u32, 20_726] {
        h.trace_on();
        h.sys(3 | (d << 8));
        assert!(!h.trace().contains(&"risk.on_day_roll"));
        assert_eq!(h.s.view().last_day_rolled(), 20_728);
    }
    // .2 clock bad: not written.
    let mut h = quiet();
    h.sys(9);
    h.at(tsc_of(UTC0 + 23 * H + 30 * MIN + 1_000_000));
    let m = h.mark();
    for _ in 0..3 {
        h.step();
    }
    assert!(!sys_ids(&h, m).iter().any(|i| i & 0xFF == 3));
}

#[test]
fn seq_093_storage_timers() {
    for (id, kind, level) in [
        (268u32, "PersistLag", 1u8),
        (524, "PersistLag", 2),
        (269, "DiskLow", 1),
        (14, "Both", 0),
    ] {
        let mut h = quiet();
        h.trace_on();
        h.sys(id);
        let tr = h.s.engine().trace.ev.clone();
        let st = tr
            .iter()
            .find(|e| e.what == "apply.Storage")
            .unwrap_or_else(|| panic!("{id}: {:?}", h.trace()));
        assert_eq!(st.b, u64::from(level), "{kind}");
    }
}

#[test]
fn seq_094_kill_self_test_ok() {
    reset_script();
    let mut h = b0().boot();
    h.step();
    let g0 = h.gate_bits(AC1);
    let n = hft_event::Note {
        code: hft_event::note_code(hft_event::NoteSource::Boot, 2),
        level: hft_event::NoteLevel::Info,
        source: hft_event::NoteSource::Boot,
        strategy: hft_types::StrategyId(0),
        _pad: 0,
        group: GroupId(0),
        inst: InstId(0),
        v: [1, 0, 0, 0, 0, 0, 0, 0],
    };
    let m = h.mark();
    h.push(RingId::Hk, &n);
    h.run();
    assert_eq!(h.gate_bits(AC1), g0);
    assert!(
        h.notes_since(m)
            .iter()
            .all(|n| n.code != hft_sequencer::codes::seq_note(7))
    );
    h.trace_on();
    let m = h.mark();
    h.sys(16 | (1 << 16));
    assert_eq!(h.s.engine().trace.count("apply.KillSelfTest"), 1);
    if h.gate_bits(AC1) != g0 {
        let sg: Vec<_> = h
            .notes_since(m)
            .into_iter()
            .filter(|n| n.code == hft_sequencer::codes::seq_note(7))
            .collect();
        assert_eq!(sg.len(), 1);
        assert_eq!(sg[0].v[0], 1);
    }
}

#[test]
fn seq_095_conflate_report_and_advisor_tick() {
    let mut h = quiet();
    let start = h.now();
    let m = h.mark();
    // 3.5 s of rounds, 10 ms apart.
    for k in 1..=350u64 {
        h.at(start + k * 10_000_000);
        h.step();
    }
    let ids = sys_ids(&h, m);
    assert_eq!(ids.iter().filter(|i| **i == 10).count(), 3);
    assert_eq!(ids.iter().filter(|i| **i == 6).count(), 3);
    // AdvisorTick events have no Output.
    let recs = h.since(m);
    for (ix, r) in recs.iter().enumerate() {
        if r.kind == EventKind::TIMER && r.read::<Timer>().is_some_and(|t| t.owner == 1 && t.timer_id == 6) {
            assert!(recs.get(ix + 1).is_none_or(|n| n.role() != Role::Output));
        }
    }
}

#[test]
fn seq_096_replay_drivers_no_env_inputs() {
    let mut h = quiet();
    h.keyframe();
    for k in 0..20u64 {
        h.adv(400_000_000);
        h.quote(A1, "50000", if k % 2 == 0 { "50003" } else { "50004" });
        h.run();
    }
    let logged: Vec<Timer> = h
        .recs()
        .iter()
        .filter(|r| r.kind == EventKind::TIMER && r.role() == Role::Input)
        .filter_map(|r| r.read::<Timer>())
        .collect();
    let mut setup = h.setup.clone();
    setup.seq.replay_per_round = 7;
    let log = hft_sequencer::driver::replay::Log::from_segments(&h.j().segments);
    let first = log
        .recs
        .iter()
        .find(|r| r.kind == EventKind::STATE_SNAPSHOT)
        .unwrap()
        .seq;
    for mode in [
        hft_sequencer::driver::replay::ReplayMode::Verify,
        hft_sequencer::driver::replay::ReplayMode::Mirror,
    ] {
        let drv = hft_sequencer::driver::replay::ReplayDriver::new(mode, log.clone(), 0, 7);
        let p = plan(hft_sequencer::SnapshotSel::AtSeq(first), true, 0, 0);
        let mut s: hft_sequencer::Sequencer<hft_sequencer::driver::replay::ReplayDriver, TestSet> =
            hft_sequencer::Sequencer::boot(hft_sequencer::BootSource::Journal(p), &setup, drv, hb()).unwrap();
        s.engine_mut().trace.on = true;
        while s.step() == StepOutcome::Busy {}
        let n_timers = s.engine().trace.count("timer");
        assert_eq!(n_timers, logged.len(), "{mode:?}");
        assert!(
            s.driver().mismatches().is_empty() || mode == hft_sequencer::driver::replay::ReplayMode::Mirror
        );
    }
}

#[test]
fn seq_098_ms_tsc_from_segment_header() {
    reset_script();
    let mut b = b0();
    b.setup_edit.push(|s| s.clock.tsc_hz_milli = 2_000_000_000_000);
    let mut h = b.boot();
    h.io().hz = 2_000_000_000_000;
    h.steady();
    let c: Cap<u64> = cap();
    let c2 = c.clone();
    set_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            let now = hx.v.now();
            let at = now.tsc_of(now.engine_ns + 500_000_000);
            c2.borrow_mut().push(now.tsc.0);
            let _ = ctx.timer_at(at, 5, G1);
        }
    });
    h.quote(A1, "50000", "50003");
    h.run();
    clear_hook();
    let t = *c.borrow().last().unwrap();
    let e =
        h.s.engine()
            .heap
            .sorted()
            .into_iter()
            .find(|e| e.owner == 17 && e.timer_id == 5)
            .unwrap();
    assert_eq!(e.deadline, t + 1_000_000_000);
    let m = h.verify();
    assert!(m.is_empty(), "{m:?}");
}
