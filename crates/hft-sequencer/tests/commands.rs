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
    ControlCmd::Kill {
        scope: scope(StrategyId(0), AC1, GroupId(0)),
        level: KillLevel::Account,
        flatten: false,
        cancel_resting: true,
        hard: false,
    }
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
    let id0 = h.cmd_with("cli:test", ADMIN, AC1, &ControlCmd::SnapshotNow, |x| {
        set_id(x, 900)
    });
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
    let r: Vec<_> = h
        .since(m)
        .iter()
        .filter(|r| r.kind == hft_event::EventKind::CMD_RESULT)
        .filter_map(|r| r.read::<hft_event::CmdResult>())
        .filter(|r| r.cmd_id == id)
        .collect();
    assert_eq!(r[0].status, status::EXPIRED);
    // .2 duplicate + role insufficient → Duplicate.
    let id = h.cmd_with("cli:test", VIEWER, AC1, &ControlCmd::SnapshotNow, |x| {
        set_id(x, 900)
    });
    let m = h.mark();
    h.run();
    let r: Vec<_> = h
        .since(m)
        .iter()
        .filter(|r| r.kind == hft_event::EventKind::CMD_RESULT)
        .filter_map(|r| r.read::<hft_event::CmdResult>())
        .filter(|r| r.cmd_id == id)
        .collect();
    assert_eq!(r[0].status, status::DUPLICATE);
    // .5 needs confirm + precondition → NeedsConfirm.
    let id = h.cmd(
        AC1,
        &ControlCmd::Resume {
            scope: scope(StrategyId(0), AC1, GroupId(0)),
            source: CapSource::KILL,
        },
    );
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
            h.cmd_with(
                "cli:test",
                ADMIN,
                AC1,
                &ControlCmd::SetMode {
                    scope: scope(S1, AC1, G1),
                    mode: RunMode::CloseOnly,
                },
                |x| set_id(x, 6_000 + k),
            );
            h.run();
        }
        let m = h.mark();
        h.cmd_with("cli:test", ADMIN, AC1, &kill1(), |x| set_id(x, 5_000));
        h.run();
        let r: Vec<_> = h
            .since(m)
            .iter()
            .filter(|r| r.kind == hft_event::EventKind::CMD_RESULT)
            .filter_map(|r| r.read::<hft_event::CmdResult>())
            .collect();
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
    let r: Vec<_> = h2
        .since(m)
        .iter()
        .filter(|r| r.kind == hft_event::EventKind::CMD_RESULT)
        .filter_map(|r| r.read::<hft_event::CmdResult>())
        .collect();
    assert_eq!(r[0].status, status::DUPLICATE);
}

#[test]
fn seq_280_account_semantics() {
    let mut h = steady();
    let id = h.cmd(
        AC1,
        &ControlCmd::SetMode {
            scope: scope(S2, AC2, G2),
            mode: RunMode::CloseOnly,
        },
    );
    h.run();
    assert_eq!(
        h.res(id),
        (status::INVALID_ARGS, u32::from(detail::ACCOUNT_MISMATCH))
    );
    let id = h.cmd(
        AC2,
        &ControlCmd::SetMode {
            scope: scope(S2, AC2, G2),
            mode: RunMode::CloseOnly,
        },
    );
    h.run();
    assert_eq!(h.res(id).0, status::APPLIED);
}

#[test]
fn seq_281_preview_no_state_change() {
    let mut h = steady();
    h.cmd(
        AC1,
        &ControlCmd::SetMode {
            scope: scope(S1, AC1, G1),
            mode: RunMode::Shadow,
        },
    );
    h.run();
    let v0 = h.s.engine().risk.mode.ctl_state_version();
    let mode0 = h.mode(S1, AC1, G1);
    let id = h.cmd_with(
        "cli:test",
        TRADER,
        AC1,
        &ControlCmd::SetMode {
            scope: scope(S1, AC1, G1),
            mode: RunMode::Live,
        },
        |x| x.hflags = CmdHdr::PREVIEW,
    );
    h.run();
    let r = h.result_of(id).unwrap();
    assert!(r.rflags & hft_event::CmdResult::PREVIEW != 0);
    assert_eq!(h.s.engine().risk.mode.ctl_state_version(), v0);
    assert_eq!(h.mode(S1, AC1, G1), mode0);
    // .3 Viewer Kill preview: one preview row, nothing else.
    let caps0 = h.s.engine().risk.mode.caps().len();
    let id = h.cmd_with("cli:viewer", VIEWER, AC1, &kill1(), |x| {
        x.hflags = CmdHdr::PREVIEW
    });
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
    let resume = ControlCmd::Resume {
        scope: scope(StrategyId(0), AC1, GroupId(0)),
        source: CapSource::KILL,
    };
    let id = h.cmd_with("cli:test", TRADER, AC1, &resume, |_| {});
    h.run();
    assert_eq!(h.res(id).0, status::NEEDS_CONFIRM);
    let id = h.cmd_with("cli:test", TRADER, AC1, &resume, |x| {
        x.confirmed = 1;
        x.expect_state_version = v - 1;
    });
    h.run();
    assert_eq!(
        h.res(id),
        (status::CONFIRM_INVALID, u32::from(detail::STATE_CHANGED))
    );
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
            let mut it = intent(
                hx,
                A1,
                hft_types::Side::Buy,
                hft_types::Tif::Gtc,
                hft_strategy_sdk::IntentClass::Open,
                px("49000"),
                q("0.001"),
            );
            it.group = G1;
            let _ = ctx.place(it);
        }
    });
    h.quote(A1, "50000", "50049");
    h.run();
    h.cmd(
        AC1,
        &ControlCmd::SetMode {
            scope: scope(S1, AC1, G1),
            mode: RunMode::CloseOnly,
        },
    );
    h.run();
    h.cmd_confirmed(
        AccountId(0),
        &ControlCmd::Drain {
            cancel_resting: false,
        },
    );
    h.run();
    assert!(h.s.view().draining());
    let id = h.cmd_confirmed(
        AC1,
        &ControlCmd::SetMode {
            scope: scope(S1, AC1, G1),
            mode: RunMode::Live,
        },
    );
    h.run();
    assert_eq!(
        h.res(id),
        (status::PRECONDITION_FAILED, u32::from(detail::DRAINING))
    );
    let fk = ControlCmd::Kill {
        scope: scope(StrategyId(0), AC1, GroupId(0)),
        level: KillLevel::Account,
        flatten: true,
        cancel_resting: true,
        hard: false,
    };
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
    assert_eq!(
        h.res(id),
        (status::INVALID_ARGS, u32::from(detail::KIND_MISMATCH))
    );
    // .6 both legs on the same exchange.
    let mut p = Spb::s1(2);
    p.groups = vec![group(G1, BTC, &[leg(A1, A), leg(A3, A)])];
    let id = h.set_params_as(&p, "pipeline", ADMIN, true, |_| {});
    h.run();
    assert_eq!(
        h.res(id),
        (status::INVALID_ARGS, u32::from(detail::SAME_EXCHANGE_LEGS))
    );
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
        assert_eq!(
            h.res(id),
            (status::INVALID_ARGS, u32::from(detail::BAD_GROUP)),
            "{g:?}"
        );
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
    assert_eq!(
        h.res(id),
        (status::PRECONDITION_FAILED, u32::from(detail::GROUP_HAS_POSITION))
    );
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

#[test]
fn seq_290_model_budget() {
    let mib = 1u64 << 20;
    // s2 and s9 take 200 MiB of memory and 300 MiB of checkpoint together.
    reset_script();
    let mut b = b0();
    let mut p2 = Spb::s2(1);
    p2.values[F_MEM] = (150 * mib) as i64;
    p2.values[F_CKPT] = (200 * mib) as i64;
    let mut p9 = Spb::s9(1);
    p9.values[F_MEM] = (50 * mib) as i64;
    p9.values[F_CKPT] = (100 * mib) as i64;
    b.params = vec![Spb::s1(1), p2, p9];
    let mut h = b.boot();
    h.steady();
    for (k, (mem, ckpt, ok)) in [
        (56 * mib, 100 * mib, true),
        (56 * mib + 1, 100 * mib, false),
        (10 * mib, 212 * mib, true),
        (10 * mib, 212 * mib + 1, false),
    ]
    .into_iter()
    .enumerate()
    {
        let mut p = Spb::s1(2 + k as u64);
        p.values[F_THR] = 10 + k as i64;
        p.values[F_MEM] = mem as i64;
        p.values[F_CKPT] = ckpt as i64;
        let id = h.set_params_as(&p, "pipeline", ADMIN, true, |_| {});
        h.run();
        let (st, d) = h.res(id);
        if ok {
            assert_eq!(st, status::APPLIED, "row {}", k + 1);
        } else {
            assert_eq!(st, status::INVALID_ARGS, "row {}", k + 1);
            assert_eq!(d & 0xFFFF, u32::from(detail::PARAM_INVALID));
            assert_eq!((d >> 16) & 0xF, u32::from(detail::PARAM_KIND_MODEL_MEM_BUDGET));
        }
    }
}

#[test]
fn seq_294_kill_combinations() {
    let cases: [(KillLevel, AccountId, bool, bool, u8); 4] = [
        (KillLevel::Global, AccountId(0), true, false, status::APPLIED),
        (KillLevel::Account, AC1, true, false, status::APPLIED),
        (KillLevel::Account, AC1, true, true, status::INVALID_ARGS),
        (KillLevel::Global, AccountId(0), false, true, status::INVALID_ARGS),
    ];
    for (k, (level, a, hard, flatten, want)) in cases.into_iter().enumerate() {
        let mut h = steady();
        let c = ControlCmd::Kill {
            scope: scope(StrategyId(0), a, GroupId(0)),
            level,
            flatten,
            cancel_resting: true,
            hard,
        };
        let id = h.cmd_confirmed(a, &c);
        h.run();
        assert_eq!(h.res(id).0, want, "row {}", k + 1);
        if want == status::INVALID_ARGS {
            assert_eq!(h.res(id).1, u32::from(detail::BAD_COMBINATION));
        }
    }
}

#[test]
fn seq_297_baseline_transfer_notice() {
    let mut h = steady();
    let id = h.cmd_confirmed(
        AC1,
        &ControlCmd::VenueNotice {
            venue: B,
            kind: 3,
            _pad: 0,
            inst: hft_types::InstId(0),
            a: UTC0 + 3_600_000_000_000,
            b: UTC0 + 5_400_000_000_000,
        },
    );
    h.run();
    assert_eq!(h.res(id).0, status::APPLIED);
    let id = h.cmd_confirmed(
        AC1,
        &ControlCmd::VenueNotice {
            venue: B,
            kind: 2,
            _pad: 0,
            inst: B1,
            a: 3_600,
            b: 0,
        },
    );
    h.run();
    assert_eq!(h.res(id).0, status::APPLIED);
    let kf = h.keyframe();
    let n = snap(&kf).1.notices;
    assert!(n.iter().any(|r| r.venue == B && r.kind == 3 && r.inst.0 == 0));
    assert!(n.iter().any(|r| r.venue == B && r.kind == 2 && r.inst == B1));
}

#[test]
fn seq_298_sim_reset_transfer() {
    let mut h = steady();
    let id = h.cmd_confirmed(AC1, &ControlCmd::SimReset { account: AC1 });
    h.run();
    assert_eq!(h.res(id), (status::INVALID_ARGS, u32::from(detail::NOT_SIM)));
    // g9 is Live: not idle.
    let id = h.cmd_confirmed(AC9, &ControlCmd::SimReset { account: AC9 });
    h.run();
    assert_eq!(
        h.res(id),
        (status::PRECONDITION_FAILED, u32::from(detail::SIM_NOT_IDLE))
    );
    let id = h.cmd_confirmed(
        AC1,
        &ControlCmd::SimTransfer {
            account: AC1,
            from: A,
            to: B,
            asset: USDT,
            amount: q("1"),
        },
    );
    h.run();
    assert_eq!(h.res(id), (status::INVALID_ARGS, u32::from(detail::NOT_SIM)));
}

#[test]
fn seq_299_write_off_residual() {
    let mut h = steady();
    let c = ControlCmd::WriteOffResidual {
        account: AC1,
        _pad: [0; 3],
        group: G1,
    };
    let id = h.cmd(AC1, &c);
    h.run();
    assert_eq!(h.res(id).0, status::NEEDS_CONFIRM);
    let c = ControlCmd::WriteOffResidual {
        account: AC1,
        _pad: [0; 3],
        group: G2,
    };
    let id = h.cmd_confirmed(AC1, &c);
    h.run();
    assert_eq!(
        h.res(id),
        (status::INVALID_ARGS, u32::from(detail::UNKNOWN_GROUP))
    );
}

#[test]
fn seq_300_smoke_order() {
    let smoke = |a: AccountId| ControlCmd::SmokeOrder {
        account: a,
        venue: B,
        _pad: 0,
        inst: B1,
        side: hft_types::Side::Buy,
        _pad2: [0; 3],
        ttl_ms: 30_000,
    };
    // .1 Shadow cell: still sent, flagged SMOKE, no strategy delivery.
    let mut h = steady();
    h.cmd(
        AC1,
        &ControlCmd::SetMode {
            scope: scope(S1, AC1, G1),
            mode: RunMode::Shadow,
        },
    );
    h.run();
    clear_log();
    let n = h.io_ref().out.len();
    let m = h.mark();
    let id = h.cmd_confirmed(AC1, &smoke(AC1));
    h.run();
    let r = h.result_of(id).unwrap();
    assert_eq!(r.status, status::APPLIED, "{r:?}");
    let ds = decisions(&h.since(m));
    assert_eq!(ds[0].1.trigger_seq, r.cmd_seq);
    let out: Vec<_> = h.io_ref().out[n..].to_vec();
    assert_eq!(out.len(), 1);
    assert!(out[0].1.oflags & hft_event::OrderFlags::SMOKE.0 != 0);
    assert!(!log().iter().any(|g| g.kind == "Order"));
    // .4 a second smoke order on (1, B).
    let id = h.cmd_confirmed(AC1, &smoke(AC1));
    h.run();
    assert_eq!(
        h.res(id),
        (status::PRECONDITION_FAILED, u32::from(detail::SMOKE_ACTIVE))
    );
    // .6 Trader: role insufficient.
    let id = h.cmd_with("cli:test", TRADER, AC1, &smoke(AC1), |x| x.confirmed = 1);
    h.run();
    assert_eq!(h.res(id).0, status::ROLE_INSUFFICIENT);
    // .7 the Sim account.
    let id = h.cmd_confirmed(AC9, &smoke(AC9));
    h.run();
    assert_eq!(h.res(id), (status::INVALID_ARGS, u32::from(detail::NOT_LIVE)));
}

#[test]
fn seq_301_strategy_cmd() {
    let mut h = steady();
    let sc = |inst: StrategyId, g: GroupId, ro: u8| ControlCmd::StrategyCmd {
        instance: inst,
        reduce_only: ro,
        len: 32,
        group: g,
        payload: [0x11; 256],
    };
    clear_log();
    let id = h.cmd_with("cli:test", TRADER, AC1, &sc(S1, G1, 0), |x| {
        x.confirmed = 1;
    });
    h.run();
    let v = h.s.engine().risk.mode.ctl_state_version();
    let _ = v;
    let st = h.res(id).0;
    assert!(st == status::APPLIED || st == status::CONFIRM_INVALID, "{st}");
    let id = h.cmd_confirmed(AC1, &sc(S1, G1, 0));
    h.run();
    assert_eq!(h.res(id).0, status::APPLIED);
    let got: Vec<Got> = log()
        .into_iter()
        .filter(|g| g.kind == "Manual" && !g.model)
        .collect();
    assert!(!got.is_empty() && got.iter().all(|g| g.s == S1 && g.gix == 0));
    let id = h.cmd_confirmed(AC1, &sc(S1, G2, 0));
    h.run();
    assert_eq!(
        h.res(id),
        (status::INVALID_ARGS, u32::from(detail::UNKNOWN_GROUP))
    );
    let id = h.cmd_confirmed(AccountId(0), &sc(StrategyId(7), G1, 0));
    h.run();
    assert_eq!(h.res(id).0, status::UNKNOWN_TARGET);
    let id = h.cmd(AC1, &sc(S1, G1, 0));
    h.run();
    assert_eq!(h.res(id).0, status::NEEDS_CONFIRM);
    clear_log();
    let id = h.cmd_confirmed(AC1, &sc(S1, hft_types::GROUP_NONE, 1));
    h.run();
    assert_eq!(h.res(id).0, status::APPLIED);
    let got: Vec<Got> = log()
        .into_iter()
        .filter(|g| g.kind == "Manual" && !g.model)
        .collect();
    assert!(got.iter().any(|g| g.gix == hft_strategy_sdk::GIX_NONE));
}

#[test]
fn seq_302_inject_warmup() {
    reset_script();
    set_warmup_both(S2);
    let mut h = b0().boot();
    h.steady();
    clear_log();
    let w = |inst: StrategyId| ControlCmd::InjectWarmup {
        instance: inst,
        last: 1,
        local_kind: 3,
        chunk_no: 0,
        len: 4_096,
        _pad: [0; 6],
        payload: &[7u8; 4_096],
    };
    let id = h.cmd(AccountId(0), &w(S2));
    h.run();
    assert_eq!(h.res(id).0, status::APPLIED);
    assert!(log().iter().any(|g| g.s == S2 && g.kind == "Warmup" && !g.model));
    clear_log();
    let id = h.cmd(AccountId(0), &w(S1));
    h.run();
    assert_eq!(h.res(id).0, status::APPLIED);
    assert!(!log().iter().any(|g| g.s == S1 && g.kind == "Warmup" && !g.model));
    let id = h.cmd(AccountId(0), &w(StrategyId(7)));
    h.run();
    assert_eq!(h.res(id).0, status::UNKNOWN_TARGET);
    // .5 20 warmup records of 4 096 B each, then a Kill: at most 2 chunks per round, the Kill in
    // the round after the 10 rounds of chunks.
    let probe = h
        .cmd_elem("cli:test", 4, AccountId(0), &w(S2), |_| {})
        .0
        .body
        .len();
    let pay = vec![7u8; 4_096 - (probe - 4_096)];
    let w4k = ControlCmd::InjectWarmup {
        instance: S2,
        last: 1,
        local_kind: 3,
        chunk_no: 0,
        len: pay.len() as u16,
        _pad: [0; 6],
        payload: &pay,
    };
    for _ in 0..20 {
        let (e, _) = h.cmd_elem("cli:test", 4, AccountId(0), &w4k, |_| {});
        assert_eq!(e.body.len(), 4_096);
        h.push_e(hft_sequencer::driver::live::RingId::Ctl, e);
    }
    let k = h.cmd(AC1, &kill1());
    let mut rounds = 0;
    while h.result_of(k).is_none() && rounds < 20 {
        let before = h
            .io_ref()
            .rings
            .get(&hft_sequencer::driver::live::RingId::Ctl)
            .map_or(0, |r| r.len());
        h.step();
        let after = h
            .io_ref()
            .rings
            .get(&hft_sequencer::driver::live::RingId::Ctl)
            .map_or(0, |r| r.len());
        assert!(before - after <= 2, "{before} -> {after}");
        rounds += 1;
    }
    assert!(rounds <= 11, "{rounds}");
}

#[test]
fn seq_304_replace_refdata_recheck() {
    let mut h = steady();
    // .5 a valid new refdata: Applied, pending, Draining{cancel_resting = false}.
    let mut rd2 = b0_rd();
    rd2.version = 2;
    let blob = rd2.blob();
    let r = hft_event::BlobRef {
        kind: hft_event::BlobKind::Refdata,
        encoding: 1,
        len: blob.len() as u32,
        version: 2,
        sha256: sha(&blob),
    };
    let parsed = hft_refdata::RefData::load(&blob, Some(&h.s.engine().rd));
    h.io().mailbox.push(hft_sequencer::ParsedConfig::Refdata {
        sha256: r.sha256,
        result: Box::new(parsed),
    });
    h.blob_input(r, scope(StrategyId(0), AccountId(0), GroupId(0)), &blob);
    let id = h.cmd_confirmed(AccountId(0), &ControlCmd::ReplaceRefdata { blob: r });
    h.run();
    assert_eq!(h.res(id).0, status::APPLIED, "{:?}", h.res(id));
    assert!(h.s.view().draining());
    let draining =
        h.s.engine()
            .risk
            .mode
            .caps()
            .iter()
            .find(|c| c.source == CapSource::DRAINING)
            .copied()
            .expect("Draining");
    assert_eq!(draining.params[0], 0);
}

// ------------------------------------------------------------------------------- SEQ-289

/// ETH spot on A (inst 25) and ETH perp on B (inst 26), with fillers 23 / 24.
const X25: hft_types::InstId = hft_types::InstId(25);
const X26: hft_types::InstId = hft_types::InstId(26);

fn lot_of(s: &str) -> (&str, i64) {
    (s, (s.parse::<f64>().unwrap() * 1e8).round() as i64)
}

/// B0 plus the pair (25, 26) with lots `la` / `lb` and the first leg's `min_notional`; account
/// 1's `force_slice.max_chunk_notional` 1 000 USDT, `dust_notional` 10 USDT. `with_group` boots
/// s1 with that pair already in a group.
fn lot_rig(la: &str, lb: &str, min_notional: &str, with_group: bool) -> H {
    reset_script();
    let mut b = b0();
    for id in 23..=26u32 {
        let i = hft_types::InstId(id);
        let s = match id {
            25 => {
                let mut s = inst_spec(i, A, "ETHFDUSD", false, ETH, ("0.01", 1), lot_of(la));
                s.min_notional = n(min_notional);
                s
            }
            26 => inst_spec(i, B, "ETH2_USDT", true, ETH, ("0.01", 1), lot_of(lb)),
            _ => inst_spec(
                i,
                A,
                &format!("F{id}USDT"),
                false,
                FILL,
                ("0.01", 1),
                ("0.001", 100_000),
            ),
        };
        b.rd.insts.push(s);
    }
    b.setup_edit.push(|s| {
        s.exec.default.dust_notional = n("10");
        s.exec.default.force_slice.max_chunk_notional = n("1000");
    });
    if with_group {
        b.params[0]
            .groups
            .push(group(GroupId(0x0100_0007), ETH, &[leg(X26, B), leg(X25, A)]));
    }
    let mut h = b.boot();
    // g7 at boot has no feeds for its legs: leave the cells in their boot mode.
    h.bring_up(!with_group);
    h
}

fn mark_on(h: &mut H, i: hft_types::InstId, v: hft_types::VenueId, p: &str) {
    let mut hd = <hft_event::MarketHdr as zerocopy::FromZeros>::new_zeroed();
    hd.venue = v;
    hd.inst = i;
    hd.rx_tsc = h.now();
    h.push(
        hft_sequencer::driver::live::RingId::Md(1),
        &hft_event::MarkPrice { h: hd, px: px(p) },
    );
    h.run();
}

fn book_on(h: &mut H, i: hft_types::InstId, v: hft_types::VenueId, bid: &str, ask: &str) {
    let t = h.now();
    h.push(
        hft_sequencer::driver::live::RingId::Rep(0),
        &conn(
            v,
            0,
            hft_event::ConnStateKind::Up,
            hft_event::ConnRole::Md,
            i,
            AccountId(0),
            t,
        ),
    );
    h.upd += 1;
    let u = h.upd;
    let ring = if v == A {
        hft_sequencer::driver::live::RingId::Bbo(0)
    } else {
        hft_sequencer::driver::live::RingId::Bbo(1)
    };
    h.push(ring, &bbo(i, v, 0, bid, ask, u, t));
    h.run();
}

/// SetParams of s1 adding (or keeping) g7 = (26, 25).
fn add_g7(h: &mut H, version: u64, thr: i64) -> (u8, u32) {
    let mut p = Spb::s1(version);
    p.groups = vec![
        gspec(G1),
        group(GroupId(0x0100_0007), ETH, &[leg(X26, B), leg(X25, A)]),
    ];
    p.values[F_THR] = thr;
    let id = h.set_params_as(&p, "pipeline", ADMIN, true, |_| {});
    h.run();
    h.res(id)
}

#[test]
fn seq_289_lot_and_dust() {
    let lm = (status::INVALID_ARGS, u32::from(detail::LOT_MISMATCH));
    let applied = (status::APPLIED, 0);
    // .1 lots 0.001 / 0.01 divide.
    let mut h = lot_rig("0.001", "0.01", "5", false);
    assert_eq!(add_g7(&mut h, 2, 5), applied, ".1");
    // .2 / .3 lots 0.004 / 0.01 (pair_lot 0.02) at mark 50 000 (1 000) and 50 001 (1 000.02).
    for (mark, want) in [("50000", applied), ("50001", lm)] {
        let mut h = lot_rig("0.004", "0.01", "5", false);
        mark_on(&mut h, X26, B, mark);
        assert_eq!(add_g7(&mut h, 2, 5), want, "mark {mark}");
    }
    // .4 no mark, a book mid of 49 000.
    let mut h = lot_rig("0.004", "0.01", "5", false);
    book_on(&mut h, X26, B, "48999.99", "49000.01");
    assert_eq!(add_g7(&mut h, 2, 5), applied, ".4");
    // .5 no mark, no mid.
    let mut h = lot_rig("0.004", "0.01", "5", false);
    assert_eq!(add_g7(&mut h, 2, 5), lm, ".5");
    assert_eq!(
        h.s.instances().get(S1).unwrap().params_version(),
        1,
        ".5 old params"
    );
    // .6 an existing g7 at 0.004 / 0.01 with the mark at 60 000; only a threshold changes.
    let mut h = lot_rig("0.004", "0.01", "5", true);
    mark_on(&mut h, X26, B, "60000");
    assert_eq!(add_g7(&mut h, 2, 9), applied, ".6");
    // .7 / .8 the new group's trading leg min_notional 10 (= dust) and 10.01.
    let mut h = lot_rig("0.001", "0.01", "10", false);
    assert_eq!(add_g7(&mut h, 2, 5), applied, ".7");
    let mut h = lot_rig("0.001", "0.01", "10.01", false);
    assert_eq!(
        add_g7(&mut h, 2, 5),
        (status::INVALID_ARGS, u32::from(detail::DUST_BELOW_MIN_NOTIONAL)),
        ".8"
    );
    assert_eq!(
        h.s.instances().get(S1).unwrap().params_version(),
        1,
        ".8 old params"
    );
}

#[test]
fn seq_432_bind_slot() {
    use hft_event::FeedCmd;
    let mut h = steady();
    let bind = |inst, start_s: i64, end_s: i64, fill: u8| ControlCmd::BindSlot {
        venue: A,
        _pad: [0; 3],
        inst,
        start_s,
        end_s,
        wire_id: [fill; 32],
    };
    // .1 a subscribed inst: recorded and sent to the feed as four parts, in order.
    let n = h.io_ref().feed.len();
    let id = h.cmd_confirmed(AC1, &bind(A1, 1_000, 1_300, 0x5A));
    h.run();
    assert_eq!(h.res(id).0, status::APPLIED);
    let parts: Vec<_> = h.io_ref().feed[n..]
        .iter()
        .filter_map(|(v, c)| match c {
            FeedCmd::BindWire { inst, part, chunk } if *v == A && *inst == A1 => Some((*part, *chunk)),
            _ => None,
        })
        .collect();
    assert_eq!(
        parts,
        (0u8..4).map(|p| (p, [0x5A; 8])).collect::<Vec<_>>(),
        "SEQ-432.1"
    );
    // .2 the same binding again: NoOp, nothing sent.
    let n = h.io_ref().feed.len();
    let id = h.cmd_confirmed(AC1, &bind(A1, 1_000, 1_300, 0x5A));
    h.run();
    assert_eq!(h.res(id).0, status::NO_OP, "SEQ-432.2");
    assert_eq!(h.io_ref().feed.len(), n, "SEQ-432.2");
    // .3 an inst of another venue, an empty window: InvalidArgs.
    let id = h.cmd_confirmed(
        AC1,
        &ControlCmd::BindSlot {
            venue: B,
            _pad: [0; 3],
            inst: A1,
            start_s: 1_000,
            end_s: 1_300,
            wire_id: [1; 32],
        },
    );
    h.run();
    assert_eq!(
        h.res(id),
        (status::INVALID_ARGS, u32::from(detail::BAD_VALUE)),
        "SEQ-432.3"
    );
    let id = h.cmd_confirmed(AC1, &bind(A1, 1_300, 1_300, 1));
    h.run();
    assert_eq!(
        h.res(id),
        (status::INVALID_ARGS, u32::from(detail::BAD_VALUE)),
        "SEQ-432.3"
    );
    // .4 the binding is in the keyframe (section 31).
    let kf = h.keyframe();
    let b = snap(&kf).1.bindings;
    assert!(
        b.iter()
            .any(|r| r.inst == A1 && r.wire_id == [0x5A; 32] && r.end_s == 1_300),
        "SEQ-432.4"
    );
    // .5 a slot holding a position cannot be rebound.
    h.open_position(AC1, A1, hft_types::Side::Buy, "0.001");
    let id = h.cmd_confirmed(AC1, &bind(A1, 1_300, 1_600, 0x6B));
    h.run();
    assert_eq!(
        h.res(id),
        (status::PRECONDITION_FAILED, u32::from(detail::GROUP_HAS_POSITION)),
        "SEQ-432.5"
    );
    // .6 the discovery process: issuer `pipeline` without a role, account `*` (0).
    let mut h = steady();
    let id = h.cmd_with("pipeline", 0, AccountId(0), &bind(A1, 1_000, 1_300, 0x5A), |_| {});
    h.run();
    assert_eq!(h.res(id).0, status::APPLIED, "SEQ-432.6");
    let id = h.cmd_with("ops_bot", 0, AccountId(0), &bind(A1, 1_300, 1_600, 0x6B), |_| {});
    h.run();
    assert_eq!(h.res(id).0, status::ROLE_INSUFFICIENT, "SEQ-432.6");
}
