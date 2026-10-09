//! The command path (TESTS.md "命令路径": SEQ-278–SEQ-304, SEQ-348, SEQ-354–SEQ-358).

use hft_event::{CmdHdr, ControlCmd};
use hft_sequencer::codes::{detail, status};
use hft_sequencer::{SnapshotSel, StepOutcome};
use hft_types::{AccountId, CapSource, GroupId, KillLevel, RunMode, StrategyId, Tsc};

use crate::common::*;

fn steady() -> H {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    h
}

fn kill1() -> ControlCmd<'static> {
    ControlCmd::Kill { scope: scope(StrategyId(0), AC1, GroupId(0)), level: KillLevel::Account, flatten: false, cancel_resting: true, hard: false }
}

fn set_id(h: &mut CmdHdr, id: u64) {
    h.cmd_id = [0; 16];
    h.cmd_id[..8].copy_from_slice(&id.to_le_bytes());
    h.cmd_id[15] = 0xEE;
}

#[test]
fn seq_278_check_order() {
    let mut h = steady();
    // A command to duplicate.
    let id0 = h.cmd_with("cli:test", ADMIN, AC1, &ControlCmd::SnapshotNow, |x| set_id(x, 900));
    h.run();
    assert_eq!(h.res(id0).0, status::APPLIED);
    // .1 expired + duplicate → Expired.
    let now = h.now();
    let id = h.cmd_with("cli:test", ADMIN, AC1, &ControlCmd::SnapshotNow, |x| {
        set_id(x, 900);
        x.expires_tsc = Tsc(now - 1);
    });
    let m = h.mark();
    h.run();
    let r: Vec<_> = h.since(m).iter().filter(|r| r.kind == hft_event::EventKind::CMD_RESULT).filter_map(|r| r.read::<hft_event::CmdResult>()).filter(|r| r.cmd_id == id).collect();
    assert_eq!(r[0].status, status::EXPIRED);
    // .2 duplicate + role insufficient → Duplicate.
    let id = h.cmd_with("cli:test", VIEWER, AC1, &ControlCmd::SnapshotNow, |x| set_id(x, 900));
    let m = h.mark();
    h.run();
    let r: Vec<_> = h.since(m).iter().filter(|r| r.kind == hft_event::EventKind::CMD_RESULT).filter_map(|r| r.read::<hft_event::CmdResult>()).filter(|r| r.cmd_id == id).collect();
    assert_eq!(r[0].status, status::DUPLICATE);
    // .5 needs confirm + precondition → NeedsConfirm.
    let id = h.cmd(AC1, &ControlCmd::Resume { scope: scope(StrategyId(0), AC1, GroupId(0)), source: CapSource::KILL });
    h.run();
    assert_eq!(h.res(id).0, status::NEEDS_CONFIRM);
}

#[test]
fn seq_279_cmd_dedup_window() {
    for (n, dup) in [(63u64, true), (64, false)] {
        let mut h = steady();
        let c = h.cmd_with("cli:test", ADMIN, AC1, &kill1(), |x| set_id(x, 5_000));
        h.run();
        assert_eq!(h.res(c).0, status::APPLIED);
        for k in 0..n {
            h.cmd_with("cli:test", ADMIN, AC1, &ControlCmd::SetMode { scope: scope(S1, AC1, G1), mode: RunMode::CloseOnly }, |x| set_id(x, 6_000 + k));
            h.run();
        }
        let m = h.mark();
        h.cmd_with("cli:test", ADMIN, AC1, &kill1(), |x| set_id(x, 5_000));
        h.run();
        let r: Vec<_> = h.since(m).iter().filter(|r| r.kind == hft_event::EventKind::CMD_RESULT).filter_map(|r| r.read::<hft_event::CmdResult>()).collect();
        if dup {
            assert_eq!(r[0].status, status::DUPLICATE, "n {n}");
        } else {
            assert_ne!(r[0].status, status::DUPLICATE, "n {n}");
        }
    }
    // .3 across a restart (section 15).
    let mut h = steady();
    let c = h.cmd_with("cli:test", ADMIN, AC1, &kill1(), |x| set_id(x, 5_000));
    h.run();
    assert_eq!(h.res(c).0, status::APPLIED);
    h.keyframe();
    let mut h2 = H::crash_restart(h, plan(SnapshotSel::Latest, true, 1, 0), Rerun::default()).unwrap();
    h2.step();
    let m = h2.mark();
    h2.cmd_with("cli:test", ADMIN, AC1, &kill1(), |x| set_id(x, 5_000));
    h2.run();
    let r: Vec<_> = h2.since(m).iter().filter(|r| r.kind == hft_event::EventKind::CMD_RESULT).filter_map(|r| r.read::<hft_event::CmdResult>()).collect();
    assert_eq!(r[0].status, status::DUPLICATE);
}

#[test]
fn seq_280_account_semantics() {
    let mut h = steady();
    let id = h.cmd(AC1, &ControlCmd::SetMode { scope: scope(S2, AC2, G2), mode: RunMode::CloseOnly });
    h.run();
    assert_eq!(h.res(id), (status::INVALID_ARGS, u32::from(detail::ACCOUNT_MISMATCH)));
    let id = h.cmd(AC2, &ControlCmd::SetMode { scope: scope(S2, AC2, G2), mode: RunMode::CloseOnly });
    h.run();
    assert_eq!(h.res(id).0, status::APPLIED);
}

#[test]
fn seq_281_preview_no_state_change() {
    let mut h = steady();
    h.cmd(AC1, &ControlCmd::SetMode { scope: scope(S1, AC1, G1), mode: RunMode::Shadow });
    h.run();
    let v0 = h.s.engine().risk.mode.ctl_state_version();
    let mode0 = h.mode(S1, AC1, G1);
    let id = h.cmd_with("cli:test", TRADER, AC1, &ControlCmd::SetMode { scope: scope(S1, AC1, G1), mode: RunMode::Live }, |x| x.hflags = CmdHdr::PREVIEW);
    h.run();
    let r = h.result_of(id).unwrap();
    assert!(r.rflags & hft_event::CmdResult::PREVIEW != 0);
    assert_eq!(h.s.engine().risk.mode.ctl_state_version(), v0);
    assert_eq!(h.mode(S1, AC1, G1), mode0);
    // .3 Viewer Kill preview: one preview row, nothing else.
    let caps0 = h.s.engine().risk.mode.caps().len();
    let id = h.cmd_with("cli:viewer", VIEWER, AC1, &kill1(), |x| x.hflags = CmdHdr::PREVIEW);
    h.run();
    assert_eq!(h.res(id).0, status::APPLIED);
    assert_eq!(h.s.engine().risk.mode.caps().len(), caps0);
    let kf = h.keyframe();
    assert_eq!(snap(&kf).1.previews.len(), 1);
}

#[test]
fn seq_282_confirm_and_version() {
    let mut h = steady();
    h.cmd(AC1, &kill1());
    h.run();
    let v = h.s.engine().risk.mode.ctl_state_version();
    let resume = ControlCmd::Resume { scope: scope(StrategyId(0), AC1, GroupId(0)), source: CapSource::KILL };
    let id = h.cmd_with("cli:test", TRADER, AC1, &resume, |_| {});
    h.run();
    assert_eq!(h.res(id).0, status::NEEDS_CONFIRM);
    let id = h.cmd_with("cli:test", TRADER, AC1, &resume, |x| {
        x.confirmed = 1;
        x.expect_state_version = v - 1;
    });
    h.run();
    assert_eq!(h.res(id), (status::CONFIRM_INVALID, u32::from(detail::STATE_CHANGED)));
    let r = h.result_of(id).unwrap();
    assert_eq!(r.ctl_state_version, h.s.engine().risk.mode.ctl_state_version());
    let v = h.s.engine().risk.mode.ctl_state_version();
    let id = h.cmd_with("cli:test", TRADER, AC1, &resume, |x| {
        x.confirmed = 1;
        x.expect_state_version = v;
    });
    h.run();
    assert_eq!(h.res(id).0, status::APPLIED, "{:?}", h.res(id));
    // .4 a downgrade needs no confirmation.
    let id = h.cmd_with("cli:test", TRADER, AC1, &kill1(), |_| {});
    h.run();
    assert_eq!(h.res(id).0, status::APPLIED);
}

#[test]
fn seq_283_set_params_confirmation() {
    let mut h = steady();
    // .6 cli, threshold only: Applied without confirmation.
    let mut p = Spb::s1(2);
    p.values[F_THR] = 7;
    let id = h.set_params_as(&p, "cli:alice", ADMIN, false, |_| {});
    h.run();
    assert_eq!(h.res(id).0, status::APPLIED);
    // .4 cli, a new open-allowed group: NeedsConfirm.
    let mut p = Spb::s1(3);
    p.groups = vec![gspec(G1), gspec_eth(GroupId(0x0100_0002))];
    let id = h.set_params_as(&p, "cli:alice", ADMIN, false, |_| {});
    h.run();
    assert_eq!(h.res(id).0, status::NEEDS_CONFIRM);
    // .1 pipeline, the same change: Applied.
    let id = h.set_params_as(&p, "pipeline", ADMIN, false, |_| {});
    h.run();
    assert_eq!(h.res(id).0, status::APPLIED);
    // .7 pipeline, structural change on a Live instance: NeedsConfirm.
    let mut p = Spb::s1(4);
    p.groups = vec![gspec(G1), gspec_eth(GroupId(0x0100_0002))];
    p.values[F_WIN] = 99;
    let id = h.set_params_as(&p, "pipeline", ADMIN, false, |_| {});
    h.run();
    assert_eq!(h.res(id).0, status::NEEDS_CONFIRM);
    // .10 pipeline, structural change on the Sim instance: Applied.
    let mut p = Spb::s9(2);
    p.values[F_WIN] = 99;
    let id = h.set_params_as(&p, "pipeline", ADMIN, false, |_| {});
    h.run();
    assert_eq!(h.res(id).0, status::APPLIED);
}

#[test]
fn seq_284_draining_and_gate() {
    let mut h = steady();
    // An unacknowledged order keeps the Drain from finishing.
    add_hook(|hx, ev, ctx| {
        if let hft_strategy_sdk::EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
            && b.ask_px == px("50049")
        {
            let mut it = intent(hx, A1, hft_types::Side::Buy, hft_types::Tif::Gtc, hft_strategy_sdk::IntentClass::Open, px("49000"), q("0.001"));
            it.group = G1;
            let _ = ctx.place(it);
        }
    });
    h.quote(A1, "50000", "50049");
    h.run();
    h.cmd(AC1, &ControlCmd::SetMode { scope: scope(S1, AC1, G1), mode: RunMode::CloseOnly });
    h.run();
    h.cmd_confirmed(AccountId(0), &ControlCmd::Drain { cancel_resting: false });
    h.run();
    assert!(h.s.view().draining());
    let id = h.cmd_confirmed(AC1, &ControlCmd::SetMode { scope: scope(S1, AC1, G1), mode: RunMode::Live });
    h.run();
    assert_eq!(h.res(id), (status::PRECONDITION_FAILED, u32::from(detail::DRAINING)));
    let fk = ControlCmd::Kill { scope: scope(StrategyId(0), AC1, GroupId(0)), level: KillLevel::Account, flatten: true, cancel_resting: true, hard: false };
    let id = h.cmd(AC1, &fk);
    h.run();
    assert_eq!(h.res(id).0, status::NEEDS_CONFIRM);
    h.trace_on();
    let id = h.cmd_confirmed(AC1, &fk);
    h.run();
    assert_eq!(h.res(id).0, status::APPLIED);
    assert!(h.trace().contains(&"exec.engine_flatten"));
}

#[test]
fn seq_285_set_params_binding() {
    let mut h = steady();
    // .1 strategy 1 with account 2.
    let mut p = Spb::s1(2);
    p.account = AC2;
    let id = h.set_params_as(&p, "pipeline", ADMIN, true, |_| {});
    h.run();
    assert_eq!(h.res(id).0, status::INVALID_ARGS);
    // .2 kind mismatch.
    let mut p = Spb::s1(2);
    p.kind_id = TARGET;
    p.digest = digest(TARGET);
    let id = h.set_params_as(&p, "pipeline", ADMIN, true, |_| {});
    h.run();
    assert_eq!(h.res(id), (status::INVALID_ARGS, u32::from(detail::KIND_MISMATCH)));
    // .6 both legs on the same exchange.
    let mut p = Spb::s1(2);
    p.groups = vec![group(G1, BTC, &[leg(A1, A), leg(A3, A)])];
    let id = h.set_params_as(&p, "pipeline", ADMIN, true, |_| {});
    h.run();
    assert_eq!(h.res(id), (status::INVALID_ARGS, u32::from(detail::SAME_EXCHANGE_LEGS)));
    assert_eq!(h.s.instances().get(S1).unwrap().params_version(), 1);
}

#[test]
fn seq_286_set_params_version_group_id() {
    let mut h = steady();
    let mut p = Spb::s1(5);
    p.values[F_THR] = 6;
    h.set_params(&p);
    h.run();
    for v in [5u64, 4] {
        let mut p = Spb::s1(v);
        p.values[F_THR] = 7;
        let id = h.set_params_as(&p, "pipeline", ADMIN, true, |_| {});
        h.run();
        assert_eq!(h.res(id).0, status::STALE_VERSION, "v {v}");
    }
    let mut p = Spb::s1(6);
    p.values[F_THR] = 7;
    let id = h.set_params_as(&p, "pipeline", ADMIN, true, |x| x.base_version = 4);
    h.run();
    assert_eq!(h.res(id).0, status::VERSION_CONFLICT);
    for g in [hft_types::GROUP_NONE, hft_types::GROUP_ALL, GroupId(0x0200_0005)] {
        let mut p = Spb::s1(6);
        p.groups = vec![gspec(g)];
        let id = h.set_params_as(&p, "pipeline", ADMIN, true, |_| {});
        h.run();
        assert_eq!(h.res(id), (status::INVALID_ARGS, u32::from(detail::BAD_GROUP)), "{g:?}");
    }
    let mut p = Spb::s1(6);
    p.groups = vec![gspec(GroupId(0x0100_0003)), gspec_eth(GroupId(0x0100_0003))];
    let id = h.set_params_as(&p, "pipeline", ADMIN, true, |_| {});
    h.run();
    assert_eq!(h.res(id), (status::INVALID_ARGS, u32::from(detail::GROUP_ID_DUP)));
}

#[test]
fn seq_287_inst_owned_group_has_position() {
    let mut h = steady();
    // .1 a second Live group on (1, a1).
    let mut p = Spb::s1(2);
    p.groups = vec![gspec(G1), gspec(GroupId(0x0100_0002))];
    let id = h.set_params_as(&p, "pipeline", ADMIN, true, |_| {});
    h.run();
    assert_eq!(h.res(id), (status::INVALID_ARGS, u32::from(detail::INST_OWNED)));
    // .3 removing g1 with a1 inventory.
    h.open_position(AC1, A1, hft_types::Side::Buy, "0.001");
    let mut p = Spb::s1(2);
    p.groups = vec![gspec_eth(GroupId(0x0100_0002))];
    let id = h.set_params_as(&p, "pipeline", ADMIN, true, |_| {});
    h.run();
    assert_eq!(h.res(id), (status::PRECONDITION_FAILED, u32::from(detail::GROUP_HAS_POSITION)));
}

#[test]
fn seq_288_set_params_capacity() {
    let mut h = steady();
    // 32 cells in total: 3 now; 30 more groups exceed it.
    let mut p = Spb::s1(2);
    p.groups = (1..=31u32).map(|k| gspec_eth(GroupId(0x0100_0000 + k))).collect();
    let id = h.set_params_as(&p, "pipeline", ADMIN, true, |_| {});
    h.run();
    let r = h.res(id).0;
    assert!(r == status::CAPACITY_EXCEEDED || r == status::INVALID_ARGS, "{r}");
    assert_eq!(h.s.instances().get(S1).unwrap().params_version(), 1);
    let _ = StepOutcome::Idle;
}
