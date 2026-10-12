//! Assembly and the first rounds (TESTS.md SEQ-006–SEQ-014).

use hft_event::{ControlCmd, EventKind, Role};
use hft_sequencer::driver::live::RingId;
use hft_sequencer::{FatalReason, ParsedConfig, StepOutcome};
use hft_types::{InstId, KillLevel};

use crate::common::*;

fn capacity_params(version: u64) -> Spb {
    let mut p = Spb::s1(version);
    p.groups = vec![
        gspec(G1),
        gspec_eth(hft_types::GroupId(G1.0 + 1)),
        group(
            hft_types::GroupId(G1.0 + 2),
            FILL,
            &[leg(InstId(1), A), leg(InstId(2), B)],
        ),
    ];
    p
}

fn capacity_ready(h: &mut H) {
    use hft_event::{ConnRole, ConnStateKind, LeverageItem};
    use hft_types::{AccountId, CapSource, GroupId, StrategyId};
    use zerocopy::IntoBytes;

    h.bring_up(false);
    // B0's helper only covers its original instruments. Bring up the extra group's two legs
    // with ordinary journaled venue reports as well.
    for (i, v) in [(InstId(1), A), (InstId(2), B)] {
        h.push(
            RingId::Rep(0),
            &conn(v, 0, ConnStateKind::Up, ConnRole::Md, i, AccountId(0), h.now()),
        );
        h.quote(i, "10", "10.01");
    }
    for a in [AC1, AC2] {
        let mut body = cfg_hdr(a, B, 3).as_bytes().to_vec();
        for inst in [InstId(2), B1, B2] {
            body.extend_from_slice(
                LeverageItem {
                    inst,
                    _pad: 0,
                    leverage: r("3.00"),
                }
                .as_bytes(),
            );
        }
        h.push_bytes(RingId::Rep(0), EventKind::ACCOUNT_CONFIG_REPORT, body);
    }
    h.run();
    for a in [AC1, AC2, AC9] {
        let sc = scope(StrategyId(0), a, GroupId(0));
        if h.s.engine().risk.mode.cap(CapSource::RECOVERY, &sc).is_some() {
            let id = h.cmd_confirmed(
                a,
                &ControlCmd::Resume {
                    scope: sc,
                    source: CapSource::RECOVERY,
                },
            );
            h.run();
            assert_eq!(h.res(id).0, hft_sequencer::codes::status::APPLIED);
        }
    }
    let cells: Vec<_> = h.s.instances().cells().collect();
    for cell in cells {
        let expected = if h.s.engine().risk.mode.desired(&cell) == Some(hft_types::RunMode::Live) {
            hft_sequencer::codes::status::NO_OP
        } else {
            hft_sequencer::codes::status::APPLIED
        };
        let id = h.cmd_confirmed(
            cell.account,
            &ControlCmd::SetMode {
                scope: scope(cell.strategy, cell.account, cell.group),
                mode: hft_types::RunMode::Live,
            },
        );
        h.run();
        assert_eq!(h.res(id), (expected, 0), "SetMode Live for {cell:?}");
        assert_eq!(
            h.s.engine().risk.mode.effective(cell),
            hft_risk::mode::Effective::Live
        );
    }
}

/// Real Drain tail: ten subscriptions, six cells, three instances, forty heap entries.
/// All decision configuration is encoded, so the post-shrink run can be verified in isolation.
fn capacity_drain() -> H {
    use hft_sequencer::driver::live::LiveDriver;
    use hft_sequencer::{BootSource, Sequencer};
    use hft_strategy_sdk::{EvMask, EventRef, Part};
    use hft_types::{AccountId, GroupId, HostClass, Tsc};

    reset_script();
    let mut b = b0();
    b.caps.max_timers = 512;
    for id in [2, 4] {
        b.rd.insts[id as usize - 1] = inst_spec(
            InstId(id),
            B,
            &format!("F{id}_USDT"),
            true,
            FILL,
            ("0.01", 1),
            ("0.001", 100_000),
        );
    }
    let mut p2 = Spb::s2(1);
    p2.groups.push(gspec_eth(GroupId(G2.0 + 1)));
    b.params = vec![capacity_params(1), p2, Spb::s9(1)];
    let mut subs = vec![(A1, 0, EvMask::BBO, Part::Fast)];
    for id in 3..=6 {
        subs.push((InstId(id), 0, EvMask::BBO, Part::Fast));
    }
    set_subs(S1, subs);
    let rd = b.rd.load();
    let mut setup = b.setup(&rd);
    setup.risk_decode = Some(hft_sequencer::config_blob::decode_risk_blob);
    let mut configs = b.configs(&rd);
    configs.engine_blob = setup.encode_blob(1, HostClass::Dev2, UTC0);
    configs.risk_blob = configs.risk.encode_blob().unwrap();
    configs.risk_sha256 = sha(&configs.risk_blob);
    let heartbeat = hb();
    let s = Sequencer::boot(
        BootSource::Empty { configs },
        &setup,
        LiveDriver::new(TestIo::new(setup.clock), b.live),
        heartbeat,
    )
    .unwrap();
    let mut h = H {
        s,
        rd,
        setup,
        cmd_no: 0,
        hb: heartbeat,
        upd: 1,
    };
    capacity_ready(&mut h);
    assert_eq!(h.s.instances().cells().count(), 6);
    assert_eq!(h.s.instances().slots.len(), 3);
    assert_eq!(h.s.instances().subs.insts.len(), 10);
    // Every timer comes through Ctx and a recorded BBO. No snapshot or heap is fabricated.
    add_hook(|hx, ev, ctx| {
        if hx.s.strategy == S1
            && let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
            && b.ask_px == px("50009")
        {
            let now = hx.v.now();
            ctx.timer_at(Tsc(now.tsc.0 + 1_000 * SEC), now.seq as u32, G1)
                .unwrap();
        }
    });
    let count = 40usize.checked_sub(h.s.engine().heap.len()).unwrap();
    assert!(count > 0);
    for _ in 0..count {
        h.quote(A1, "50000", "50009");
        h.run();
    }
    h.cmd_confirmed(
        AccountId(0),
        &ControlCmd::Drain {
            cancel_resting: false,
        },
    );
    assert!(matches!(h.run(), StepOutcome::Done { .. }));
    assert_eq!(h.s.engine().heap.len(), 40);
    assert_eq!(h.j().committed, h.j().len());
    let records = h.recs();
    let last = records
        .iter()
        .rposition(|r| r.kind == EventKind::STATE_SNAPSHOT)
        .unwrap();
    assert_eq!(
        records[last].read::<hft_event::SnapshotHdr>().unwrap().purpose,
        hft_sequencer::snapshot::PURPOSE_DRAIN
    );
    assert!(records[last + 1..].iter().all(|r| r.role() != Role::Input));
    h
}

#[test]
fn seq_006_capacity_shrink() {
    use hft_sequencer::{BootError, CapKind, SnapshotSel};

    let h = capacity_drain();
    // A restored strategy timer still consumes its shared quota. Spare engine capacity cannot
    // be borrowed just because those engine timers are not all currently registered.
    let timers = h.s.engine().engine_timer_demand + h.s.engine().strat_timers;
    assert!(timers > 40 && timers < h.setup.caps.max_timers);
    let base = h.setup.caps;
    let mut rows = Vec::new();
    for (value, want) in [(9, Some(CapKind::Insts)), (10, None)] {
        let mut c = base;
        c.max_insts = value;
        rows.push((c, want));
    }
    for (value, want) in [(5, Some(CapKind::Cells)), (6, None)] {
        let mut c = base;
        c.max_cells_total = value;
        rows.push((c, want));
    }
    let mut c = base;
    c.max_instances = 2;
    rows.push((c, Some(CapKind::Instances)));
    let mut c = base;
    c.max_groups_per_instance = 2;
    rows.push((c, Some(CapKind::GroupsPerInstance)));
    for (value, want) in [(timers - 1, Some(CapKind::Timers)), (timers, None)] {
        let mut c = base;
        c.max_timers = value;
        rows.push((c, want));
    }
    let mut c = base;
    c.max_insts = 9;
    c.max_timers = 39;
    rows.push((c, Some(CapKind::Insts)));
    for what in [
        CapKind::Cells,
        CapKind::Instances,
        CapKind::GroupsPerInstance,
        CapKind::Timers,
    ] {
        let mut c = base;
        c.max_timers = 39;
        if what != CapKind::Timers {
            c.max_groups_per_instance = 2;
        }
        if matches!(what, CapKind::Cells | CapKind::Instances) {
            c.max_instances = 2;
        }
        if what == CapKind::Cells {
            c.max_cells_total = 5;
        }
        rows.push((c, Some(what)));
    }
    for (i, (caps, want)) in rows.into_iter().enumerate() {
        let mut setup = h.setup.clone();
        setup.caps = caps;
        let got = H::<TestSet>::restart_over(
            h.j().clone(),
            setup,
            plan(SnapshotSel::DrainTail, false, 1, 0),
            Rerun::default(),
        );
        match (got, want) {
            (Err(got), Some(what)) => assert_eq!(got, BootError::CapacityShrink { what }, "row {}", i + 1),
            (Ok(got), None) => {
                assert_eq!(got.s.engine().caps, caps);
                assert_eq!(got.s.engine().setup.caps, caps);
                assert_eq!(got.s.instances().cells().count(), 6);
                assert_eq!(got.s.instances().subs.insts.len(), 10);
                assert_eq!(got.s.engine().heap.len(), 40);
                assert!(got.s.engine().strat_timers <= caps.max_timers - got.s.engine().engine_timer_demand);
            }
            (got, want) => panic!("row {}: got {:?}, expected {want:?}", i + 1, got.err()),
        }
    }
}

#[test]
fn seq_007_shrunk_capacity_applies() {
    use hft_sequencer::SnapshotSel;
    use hft_sequencer::codes::status;
    use hft_types::{AccountId, CapSource, GroupId, StrategyId};

    let original = capacity_drain();
    let mut setup = original.setup.clone();
    setup.caps.max_cells_total = 6;
    let mut h = H::<TestSet>::restart_over(
        original.j().clone(),
        setup,
        plan(SnapshotSel::DrainTail, false, 1, 0),
        Rerun::default(),
    )
    .unwrap();
    let engine_blob = |h: &H| {
        let r = h
            .recs()
            .into_iter()
            .rev()
            .find(|r| {
                r.kind == EventKind::CONFIG_BLOB
                    && r.read::<hft_event::BlobHdr>()
                        .is_some_and(|h| h.blob.kind == hft_event::BlobKind::Engine)
            })
            .unwrap();
        let header = r.read::<hft_event::BlobHdr>().unwrap();
        hft_refdata::EngineConfig::load(&r.body[72..72 + header.blob.len as usize], None).unwrap()
    };
    let old = engine_blob(&original);
    let new = engine_blob(&h);
    assert_eq!(new.version(), old.version() + 1);
    assert_eq!(hft_sequencer::params::capacities(&new).unwrap(), h.setup.caps);
    for section in hft_refdata::EngineSection::ALL {
        if section != hft_refdata::EngineSection::Capacities {
            assert_eq!(old.section(section), new.section(section), "{section:?}");
        }
    }
    assert!(
        h.s.engine()
            .risk
            .mode
            .cap(hft_types::CapSource::DRAINING, &hft_risk::scope_any())
            .is_none()
    );
    capacity_ready(&mut h);
    assert!(!h.s.view().draining());
    let mut p = capacity_params(2);
    p.groups.push(group(
        GroupId(G1.0 + 3),
        FILL,
        &[leg(InstId(3), A), leg(InstId(4), B)],
    ));
    let at = h.mark();
    let id = h.set_params_as(&p, "pipeline", 0, false, |_| {});
    h.run();
    assert_eq!(h.res(id).0, status::CAPACITY_EXCEEDED);
    assert_eq!(h.s.instances().get(S1).unwrap().groups().unwrap().len(), 3);
    assert_eq!(h.s.instances().get(S1).unwrap().params_version(), 1);
    assert_eq!(h.s.instances().cells().count(), 6);
    assert!(!h.since(at).iter().any(|r| r.kind == EventKind::ORDER_DECISION));
    let cap_set = hft_risk::NOTE_CODES
        .iter()
        .find(|x| x.2 == "CapSet")
        .map(|x| ((x.0 as u32) << 24) | x.1)
        .unwrap();
    assert!(!h.notes_since(at).iter().any(|n| n.code == cap_set));

    #[cfg(feature = "replay-io")]
    let recovery = h
        .recs()
        .iter()
        .rev()
        .find(|r| r.kind == EventKind::STATE_SNAPSHOT)
        .unwrap()
        .seq;
    // A second real Drain keeps the same capacities and a deliberate account Kill. Retiring
    // Draining must neither drop that Kill nor create another capacity configuration version.
    let sc = scope(StrategyId(0), AC2, GroupId(0));
    let id = h.cmd_confirmed(
        AC2,
        &ControlCmd::Kill {
            scope: sc,
            level: KillLevel::Account,
            flatten: false,
            cancel_resting: true,
            hard: false,
        },
    );
    h.run();
    assert_eq!(h.res(id).0, status::APPLIED);
    let kill = h.s.engine().risk.mode.cap(CapSource::KILL, &sc).copied().unwrap();
    h.cmd_confirmed(
        AccountId(0),
        &ControlCmd::Drain {
            cancel_resting: false,
        },
    );
    assert!(matches!(h.run(), StepOutcome::Done { .. }));
    let mut h = H::<TestSet>::restart_over(
        h.j().clone(),
        h.setup.clone(),
        plan(SnapshotSel::DrainTail, false, 2, 0),
        Rerun::default(),
    )
    .unwrap();
    assert_eq!(engine_blob(&h).sha256(), new.sha256());
    assert_eq!(h.s.engine().risk.mode.cap(CapSource::KILL, &sc), Some(&kill));
    assert!(
        h.s.engine()
            .risk
            .mode
            .cap(CapSource::DRAINING, &hft_risk::scope_any())
            .is_none()
    );
    assert!(h.s.engine().risk.mode.cap(CapSource::RECOVERY, &sc).is_some());
    h.run(); // Record the third run's EngineClock and ordinary startup outputs.

    #[cfg(feature = "replay-io")]
    {
        use hft_sequencer::driver::replay::Log;
        use hft_sequencer::driver::verify::{VerifyOptions, verify};
        let log = Log::from_segments(&h.j().segments);
        let rebuilt = hft_sequencer::EngineSetup::from_snapshot(&log, recovery).unwrap();
        assert_eq!(rebuilt.caps, h.setup.caps, "journal retained the old limits");
        let got = verify::<TestSet>(log.clone(), VerifyOptions::default(), hb()).unwrap();
        assert_eq!(got.runs.len(), 3);
        assert!(got.inputs > 0 && got.sink.compared > 0);
        assert!(got.sink.mismatches.is_empty(), "{:?}", got.sink.detail);
        assert!(
            got.sink.recovery_diffs.is_empty(),
            "{:?}",
            got.sink.recovery_diffs
        );
        let got = verify::<TestSet>(
            log,
            VerifyOptions {
                from_seq: Some(recovery),
                ..Default::default()
            },
            hb(),
        )
        .unwrap();
        assert!(got.sink.mismatches.is_empty(), "{:?}", got.sink.detail);
        assert!(
            got.sink.recovery_diffs.is_empty(),
            "{:?}",
            got.sink.recovery_diffs
        );
    }
}

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
    h.push(
        RingId::Rep(0),
        &conn(
            A,
            0,
            hft_event::ConnStateKind::Up,
            hft_event::ConnRole::Md,
            InstId(0),
            hft_types::AccountId(0),
            t,
        ),
    );
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
    let inputs: Vec<EventKind> = h
        .since(m2)
        .iter()
        .filter(|r| r.role() == Role::Input)
        .map(|r| r.kind)
        .collect();
    let dbg: Vec<_> = h
        .since(m2)
        .iter()
        .filter(|r| r.kind == EventKind::TIMER)
        .map(|r| r.read::<hft_event::Timer>())
        .collect();
    assert_eq!(
        &inputs[..3],
        &[EventKind::CONTROL_CMD, EventKind::CONN_STATE, EventKind::BBO],
        "{dbg:?}"
    );
}

#[test]
fn seq_009_stop_marker_kill_second() {
    let mut b = b0();
    b.live.stop_kill = Some((
        KillLevel::Global,
        scope(
            hft_types::StrategyId(0),
            hft_types::AccountId(0),
            hft_types::GroupId(0),
        ),
    ));
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
        ControlCmd::Kill {
            level, hard, flatten, ..
        } => {
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
        caps.iter()
            .any(|n| n.code == hft_event::note_code(hft_event::NoteSource::Risk, 2)
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
