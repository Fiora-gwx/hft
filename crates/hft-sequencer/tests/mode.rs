//! Modes and caps (TESTS.md "模式与 cap 的驱动": SEQ-197–SEQ-213).

use hft_event::ControlCmd;
use hft_risk::mode::Effective;
use hft_sequencer::driver::live::RingId;
use hft_types::{AccountId, GroupId, KillLevel, RunMode, Side, StrategyId};

use crate::common::*;

fn steady() -> H {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    h
}

fn kill(a: AccountId, hard: bool) -> ControlCmd<'static> {
    ControlCmd::Kill {
        scope: scope(StrategyId(0), a, GroupId(0)),
        level: KillLevel::Account,
        flatten: false,
        cancel_resting: true,
        hard,
    }
}

fn trace_args(h: &H, what: &str) -> Vec<(u64, u64)> {
    h.s.engine().trace.ev.iter().filter(|e| e.what == what).map(|e| (e.a, e.b)).collect()
}

#[test]
fn seq_198_cells_changes() {
    let mut h = steady();
    clear_log();
    h.trace_on();
    h.cmd(AC1, &ControlCmd::SetMode { scope: scope(S1, AC1, G1), mode: RunMode::Shadow });
    h.run();
    let m: Vec<Got> = log().into_iter().filter(|g| g.kind == "Mode" && g.s == S1 && !g.model).collect();
    assert!(m.iter().any(|g| g.text.contains("old: Live") && g.text.contains("new: Shadow")), "{m:?}");
    assert_eq!(trace_args(&h, "ledger.set_cell_shadow"), vec![(1, 1)]);
    clear_log();
    h.trace_on();
    h.cmd_confirmed(AC1, &ControlCmd::SetMode { scope: scope(S1, AC1, G1), mode: RunMode::Live });
    h.run();
    assert_eq!(trace_args(&h, "ledger.set_cell_shadow"), vec![(1, 0)]);
    clear_log();
    h.trace_on();
    h.cmd(AC1, &kill(AC1, false));
    h.run();
    let m: Vec<Got> = log().into_iter().filter(|g| g.kind == "Mode" && g.s == S1 && !g.model).collect();
    assert!(m.iter().any(|g| g.text.contains("new: CloseOnly")), "{m:?}");
    assert!(trace_args(&h, "ledger.set_cell_shadow").is_empty());
}

#[test]
fn seq_199_shadow_deferred_notes() {
    let mut h = steady();
    h.open_position(AC1, A1, Side::Buy, "0.010");
    let m = h.mark();
    let id = h.cmd(AC1, &ControlCmd::SetMode { scope: scope(S1, AC1, G1), mode: RunMode::Shadow });
    h.run();
    let r = h.result_of(id).unwrap();
    assert_eq!((r.status, r.detail), (0, 1));
    assert_eq!(h.mode(S1, AC1, G1), Effective::CloseOnly { deferred_shadow: true });
    let n = h.notes_since(m).into_iter().find(|n| n.code == hft_sequencer::codes::seq_note(8)).expect("ShadowDeferred");
    assert_eq!(n.v[0], 1 | (1 << 8));
    assert_eq!(n.v[1], u64::from(G1.0));
    assert_eq!(n.v[2], 0);
}

#[test]
fn seq_200_cancels() {
    // .1 SetMode CloseOnly → cancel_open of the cell.
    let mut h = steady();
    h.trace_on();
    h.cmd(AC1, &ControlCmd::SetMode { scope: scope(S1, AC1, G1), mode: RunMode::CloseOnly });
    h.run();
    assert_eq!(trace_args(&h, "oms.cancel_open"), vec![(1, u64::from(G1.0))]);
    // .2 Kill (not HARD) → cancel_all open_only = true; .3 HARD → false.
    let mut h = steady();
    h.trace_on();
    h.cmd(AC1, &kill(AC1, false));
    h.run();
    assert!(trace_args(&h, "oms.cancel_all").iter().all(|x| x.0 == 1) && !trace_args(&h, "oms.cancel_all").is_empty());
    let mut h = steady();
    h.trace_on();
    h.cmd_confirmed(AC1, &kill(AC1, true));
    h.run();
    assert!(trace_args(&h, "oms.cancel_all").iter().any(|x| x.0 == 0));
}

#[test]
fn seq_201_kill_words() {
    let mut h = steady();
    let k0 = h.io_ref().kills.len();
    h.cmd(AC1, &kill(AC1, false));
    h.run();
    let kills = h.io_ref().kills.clone();
    assert!(kills.len() > k0);
    assert_eq!(kills.last().unwrap().as_slice(), h.s.engine().risk.mode.kill_bits());
    // Sim account: words unchanged.
    let k1 = h.io_ref().kills.clone();
    h.cmd(AC9, &kill(AC9, false));
    h.run();
    assert_eq!(h.io_ref().kills.last(), k1.last());
    // Verify writes no words and replays byte for byte.
    let m = h.verify();
    assert!(m.is_empty(), "{m:?}");
}

#[test]
fn seq_208_risk_events() {
    let mut h = steady();
    clear_log();
    h.trace_on();
    h.cmd(AC2, &kill(AC2, false));
    h.run();
    let r: Vec<Got> = log().into_iter().filter(|g| g.kind == "Risk" && !g.model).collect();
    assert!(r.iter().any(|g| g.s == S2));
    assert!(r.iter().all(|g| g.s != S1));
    assert!(h.s.engine().trace.count("exec.on_risk") >= 1);
    assert!(h.s.engine().trace.count("exec.on_mode") >= 1);
}

#[test]
fn seq_211_venue_notice_delivery() {
    let mut h = steady();
    clear_log();
    h.cmd_confirmed(AC1, &ControlCmd::VenueNotice {
        venue: B,
        kind: 1,
        _pad: 0,
        inst: B1,
        a: UTC0 + 3_600_000_000_000,
        b: 0,
    });
    h.run();
    let caps = h.s.engine().risk.mode.caps();
    assert!(caps.iter().any(|c| c.source == hft_types::CapSource::DELISTING));
    let notes: Vec<Got> = log().into_iter().filter(|g| g.kind == "Notice" && !g.model).collect();
    assert!(notes.iter().any(|g| g.s == S1));
    let kf = h.keyframe();
    let (_, secs) = snap(&kf);
    assert!(secs.notices.iter().any(|r| r.venue == B && r.kind == 1 && r.inst == B1));
}

#[test]
fn seq_213_cancel_obligation() {
    // .1 Kill (not HARD, cancel_resting): reissue open_only on ConnState Up.
    let mut h = steady();
    h.cmd(AC1, &kill(AC1, false));
    h.run();
    h.trace_on();
    let t = h.now();
    h.push(RingId::Rep(0), &conn(B, 0, hft_event::ConnStateKind::Up, hft_event::ConnRole::Order, hft_types::InstId(0), AC1, t));
    h.run();
    assert_eq!(trace_args(&h, "oms.reissue_cancels"), vec![(1, 1)]);
    // .5 no cap: nothing.
    let mut h = steady();
    h.trace_on();
    let t = h.now();
    h.push(RingId::Rep(0), &conn(B, 0, hft_event::ConnStateKind::Up, hft_event::ConnRole::Order, hft_types::InstId(0), AC1, t));
    h.run();
    assert!(trace_args(&h, "oms.reissue_cancels").is_empty());
}
