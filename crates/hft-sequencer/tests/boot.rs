//! Assembly and the first rounds (TESTS.md SEQ-006–SEQ-014).

use hft_event::{ControlCmd, EventKind, Role};
use hft_sequencer::driver::live::RingId;
use hft_sequencer::{FatalReason, ParsedConfig, StepOutcome};
use hft_types::{InstId, KillLevel};

use crate::common::*;

fn after_boot(h: &H) -> usize {
    // Index just past `SegmentRoll{BootDone}`.
    let recs = h.recs();
    let p = recs
        .iter()
        .position(|r| r.kind == EventKind::SEGMENT_ROLL && r.body.get(32).copied() == Some(3))
        .expect("boot block");
    p + 1
}

#[test]
fn seq_008_first_round_engine_clock_only() {
    let mut h = b0().boot();
    let t = h.now();
    let c = h.cmd_elem("cli:test", 4, AC1, &ControlCmd::SnapshotNow, |_| {}).0;
    h.push_e(RingId::Ctl, c);
    h.push(RingId::Rep(0), &conn(A, 0, hft_event::ConnStateKind::Up, hft_event::ConnRole::Md, InstId(0), hft_types::AccountId(0), t));
    h.quote(A1, "50000", "50001");
    assert_eq!(h.step(), StepOutcome::Busy);
    let m = after_boot(&h);
    let recs = h.since(m);
    assert_eq!(recs.len(), 2, "{recs:?}");
    assert_eq!(recs[0].kind, EventKind::ENGINE_CLOCK);
    assert_eq!(recs[0].role(), Role::Input);
    assert_eq!(recs[1].kind, EventKind::NOTE);
    assert!(recs[1].flags.0 & hft_event::RecordFlags::EVENT_END.0 != 0);
    assert_eq!(h.io().ring(RingId::Ctl).len(), 1);
    assert_eq!(h.io().ring(RingId::Rep(0)).len(), 1);
    assert_eq!(h.io().ring(RingId::Bbo(0)).len(), 1);
    let m2 = h.mark();
    h.step();
    let inputs: Vec<EventKind> = h.since(m2).iter().filter(|r| r.role() == Role::Input).map(|r| r.kind).collect();
    let dbg: Vec<_> = h.since(m2).iter().filter(|r| r.kind == EventKind::TIMER).map(|r| r.read::<hft_event::Timer>()).collect();
    assert_eq!(&inputs[..3], &[EventKind::CONTROL_CMD, EventKind::CONN_STATE, EventKind::BBO], "{dbg:?}");
}

#[test]
fn seq_009_stop_marker_kill_second() {
    let mut b = b0();
    b.live.stop_kill = Some((KillLevel::Global, scope(hft_types::StrategyId(0), hft_types::AccountId(0), hft_types::GroupId(0))));
    let mut h = b.boot();
    h.quote(A1, "50000", "50001");
    h.step();
    h.step();
    let m = after_boot(&h);
    let recs = h.since(m);
    let inputs: Vec<_> = recs.iter().filter(|r| r.role() == Role::Input).collect();
    assert_eq!(inputs[0].kind, EventKind::ENGINE_CLOCK);
    assert_eq!(inputs[1].kind, EventKind::CONTROL_CMD);
    let hdr = inputs[1].read::<hft_event::CmdHdr>().unwrap();
    assert_eq!(&hdr.issuer[..11], b"cli:systemd");
    match ControlCmd::decode(&inputs[1].body).unwrap().1 {
        ControlCmd::Kill { level, hard, flatten, .. } => {
            assert_eq!(level, KillLevel::Global);
            assert!(!hard);
            assert!(!flatten);
        }
        c => panic!("{c:?}"),
    }
    assert_eq!(inputs[2].kind, EventKind::BBO);
    let res = h.result_of(hdr.cmd_id).expect("CmdResult");
    assert_eq!(res.status, 0);
    let caps = h.notes_since(m);
    assert!(
        caps.iter().any(|n| n.code == hft_event::note_code(hft_event::NoteSource::Risk, 2)
            && hft_risk::cap_of_note(n).source == hft_types::CapSource::KILL),
        "{caps:?}"
    );
}

#[test]
fn seq_010_idle_round() {
    let mut h = b0().boot();
    h.steady();
    let before = h.j().committed_len;
    let hb = h.hb.read();
    assert_eq!(h.step(), StepOutcome::Idle);
    assert_eq!(h.j().committed_len, before);
    assert_eq!(h.hb.read(), hb + 1);
}

#[test]
fn seq_011_initial_view() {
    let h = b0().boot();
    let v = h.s.view();
    assert!(!v.wants_continue());
    assert_eq!(v.notify_headroom(), 640);
    assert!(!v.snapshot_requested());
    assert!(!v.draining());
    assert!(!v.refdata_recheck_due());
    assert_eq!(v.stop_engine_ns(), 0);
    assert_eq!(v.next_funding_settle(), None);
}

#[test]
fn seq_013_journal_full_fatal() {
    let mut h = b0().boot();
    h.steady();
    set_hook(|hx, ev, ctx| {
        if let hft_strategy_sdk::EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            let _ = ctx.place(open_buy(hx, A1));
        }
    });
    h.run();
    let before = h.j().committed_len;
    let seen = h.j().faults.outputs_seen;
    h.io().j.faults.full_at_output = Some(seen);
    h.io().out.clear();
    h.quote(A1, "50000", "50002");
    assert_eq!(h.step(), StepOutcome::Fatal(FatalReason::JournalFull));
    assert!(h.io().out.is_empty());
    assert!(h.j().committed_len <= before);
    clear_hook();
}

#[test]
fn seq_014_retire_old_params() {
    let mut h = b0().boot();
    h.steady();
    let h1 = Spb::s1(1).blob_ref().sha256;
    let mut p = Spb::s1(2);
    p.values[F_THR] = 6;
    h.io().retired.clear();
    let id = h.set_params(&p);
    h.run();
    assert_eq!(h.result_of(id).unwrap().status, 0);
    let r = &h.io().retired;
    assert_eq!(r.len(), 1);
    match &r[0] {
        ParsedConfig::StrategyParams(p) => assert_eq!(p.sha256, h1),
        x => panic!("{x:?}"),
    }
}



