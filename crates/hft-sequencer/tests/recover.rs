//! Cold-start recovery (TESTS.md "冷启动恢复": SEQ-256–SEQ-266, SEQ-342, SEQ-366).

use hft_event::{ControlCmd, EventKind, RecordFlags, Role};
use hft_journal::RollReason;
use hft_sequencer::driver::live::RingId;
use hft_sequencer::driver::replay::{Log, ReplayDriver, ReplayMode};
use hft_sequencer::{BootError, BootSource, MemJournal, RecoveryFlags, Sequencer, SnapshotSel, StepOutcome};
use hft_types::{AccountId, CapSource, Side};
use proptest::prelude::*;

use crate::common::*;

fn steady() -> H {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    h
}

fn hash_of<D: hft_sequencer::Driver<TestSet>>(s: &Sequencer<D, TestSet>) -> [u8; 16] {
    let mut w = hft_strategy_sdk::SnapshotWriter::new(64 << 20);
    s.write_sections(&mut w);
    hft_sequencer::snapshot::state_hash(w.as_bytes())
}

/// Verify replay of `log` from its first snapshot to the end; the final state hash.
fn replay_hash(log: Log, setup: &hft_sequencer::EngineSetup) -> [u8; 16] {
    let first = log
        .recs
        .iter()
        .find(|r| r.kind == EventKind::STATE_SNAPSHOT)
        .unwrap()
        .seq;
    let drv = ReplayDriver::new(ReplayMode::Verify, log, 0, 64);
    let p = plan(SnapshotSel::AtSeq(first), true, 0, 0);
    let mut s: Sequencer<ReplayDriver, TestSet> =
        Sequencer::boot(BootSource::Journal(p), setup, drv, hb()).unwrap();
    while s.step() == StepOutcome::Busy {}
    hash_of(&s)
}

#[test]
fn seq_421_recovery_clears_old_drain_progress_before_the_new_run() {
    use hft_strategy_sdk::EventRef;
    use hft_types::Tif;

    for pending in [false, true] {
        for cancel_resting in [false, true] {
            reset_script();
            // A deterministic recorded BBO leaves an IOC unacknowledged in the pending row.
            // Keep the same pure hook installed for the independent full replay.
            set_hook(|hx, ev, ctx| {
                if let EventRef::Bbo { inst, .. } = ev
                    && *inst == A1
                    && hx.s.strategy == S1
                    && hx.v.bbo(A1).bid_px == px("50020")
                {
                    let _ = ctx.place(intent(
                        hx,
                        A1,
                        Side::Buy,
                        Tif::Ioc,
                        hft_strategy_sdk::IntentClass::Open,
                        hx.v.bbo(A1).ask_px,
                        q("0.001"),
                    ));
                }
            });
            let mut h = b0().boot_self_contained();
            h.steady();
            if pending {
                let mark = h.mark();
                h.quote(A1, "50020", "50021");
                h.run();
                let ds = decisions(&h.since(mark));
                assert_eq!(ds.len(), 1);
                assert_eq!(ds[0].1.reject, 0);
            }
            let window = h.keyframe().seq;
            h.cmd(AccountId(0), &ControlCmd::Drain { cancel_resting });
            let result = h.run();
            assert_eq!(matches!(result, StepOutcome::Done { .. }), !pending);
            assert!(h.s.view().draining());
            assert!(h.verify_from(Some(window)).is_empty());

            // Restore before the Drain so the old tail must recreate its runtime progress.
            let mut h = h
                .crash_restart(plan(SnapshotSel::AtSeq(window), true, 2, 0), Rerun::default())
                .unwrap();
            assert!(h.s.engine().recovery_replayed() > 0);
            assert!(h.s.engine().recovery_mismatches().is_empty());
            assert!(!h.s.view().draining());
            assert_eq!(h.s.engine().drain, Default::default());
            assert!(
                h.s.engine()
                    .risk
                    .mode
                    .cap(CapSource::DRAINING, &hft_risk::scope_any())
                    .is_none()
            );
            assert!(
                h.s.engine()
                    .risk
                    .mode
                    .caps()
                    .iter()
                    .any(|c| c.source == CapSource::RECOVERY)
            );
            assert!(!matches!(
                h.run(),
                StepOutcome::Done { .. } | StepOutcome::Fatal(_)
            ));

            let id = h.cmd(
                AccountId(0),
                &ControlCmd::Drain {
                    cancel_resting: !cancel_resting,
                },
            );
            h.run();
            assert_eq!(
                h.result_of(id).unwrap().status,
                hft_sequencer::codes::status::APPLIED
            );
            assert!(h.s.view().draining());
            assert_eq!(h.s.engine().drain.cancel_resting, !cancel_resting);
            assert_eq!(
                h.s.engine().drain.cause,
                Some(hft_sequencer::types::DoneCause::Drain)
            );
            assert_eq!(h.s.engine().drain.started_ens, h.s.engine().now.engine_ns);

            #[cfg(feature = "replay-io")]
            for from_seq in [None, Some(window)] {
                use hft_sequencer::driver::verify::{VerifyOptions, verify};
                let log = Log::from_segments(&h.j().segments);
                let got = verify::<TestSet>(
                    log,
                    VerifyOptions {
                        from_seq,
                        ..Default::default()
                    },
                    hb(),
                )
                .unwrap();
                assert_eq!(got.runs.len(), 2);
                assert!(got.inputs > 0 && got.sink.compared > 0);
                assert!(got.sink.mismatches.is_empty(), "{:?}", got.sink.detail);
                assert!(
                    got.sink.recovery_diffs.is_empty(),
                    "{:?}",
                    got.sink.recovery_diffs
                );
            }
            clear_hook();
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(12))]
    #[test]
    fn seq_256_truncate_to_event_end(cut in 0usize..1_000) {
        let mut h = steady();
        // One Input per step; the hash after each event, keyed by the journal length.
        let mut at: Vec<(usize, [u8; 16])> = vec![(h.j().len(), hash_of(&h.s))];
        for k in 0..40u64 {
            h.push(RingId::Rep(0), &conn(A, 1, hft_event::ConnStateKind::Alive, hft_event::ConnRole::Md, hft_types::InstId(0), AccountId(0), 10 + k));
            h.run();
            at.push((h.j().len(), hash_of(&h.s)));
            if k % 3 == 0 {
                h.quote(A1, "50000", if k % 2 == 0 { "50003" } else { "50004" });
                h.run();
                at.push((h.j().len(), hash_of(&h.s)));
            }
        }
        let mut log = Log::from_segments(&h.j().segments);
        let n = log.recs.len();
        let base = at[0].0;
        let cut_at = base + cut % (n - base + 1);
        // The truncated half event (records after the last EVENT_END before the cut).
        log.recs.truncate(cut_at);
        let end = hft_sequencer::recover_event_end(&log);
        log.recs.truncate(end);
        let want = at.iter().rev().find(|x| x.0 <= end).map(|x| x.1).unwrap();
        let got = replay_hash(log, &h.setup);
        prop_assert_eq!(got, want);
    }
}

#[test]
fn seq_257_snapshot_selection() {
    let mut h = steady();
    let s1 = h.keyframe();
    h.quote(A1, "50000", "50004");
    h.run();
    h.cmd(AC1, &ControlCmd::SnapshotNow);
    h.run();
    h.run();
    let s2 = h.snapshots().pop().unwrap();
    assert!(s2.seq > s1.seq);
    let j = h.j().clone();
    let setup = h.setup.clone();
    // .1 Latest → S2 (no tail).
    let h1 = H::<TestSet>::restart_over(
        j.clone(),
        setup.clone(),
        plan(SnapshotSel::Latest, true, 1, 0),
        Rerun::default(),
    )
    .unwrap();
    assert_eq!(h1.s.engine().recovery_replayed(), 0);
    // .3 AtSeq(S2 − 1) → S1 (the tail after S1 is replayed).
    let h3 = H::<TestSet>::restart_over(
        j.clone(),
        setup.clone(),
        plan(SnapshotSel::AtSeq(s2.seq - 1), true, 1, 0),
        Rerun::default(),
    )
    .unwrap();
    assert!(h3.s.engine().recovery_replayed() > 0);
    // .5 Latest with a desc_version two behind the binary → SnapshotUndecodable{5}.
    let r = H::<TestSetV5>::restart_over(j, setup, plan(SnapshotSel::Latest, true, 1, 0), Rerun::default());
    assert_eq!(r.err(), Some(BootError::SnapshotUndecodable { section: 5 }));
}

#[test]
fn seq_258_tail_replay_no_emit() {
    let mut h = steady();
    h.keyframe();
    h.open_position(AC1, A1, Side::Buy, "0.001");
    for k in 0..5u64 {
        h.quote(B1, "50010", if k % 2 == 0 { "50012" } else { "50013" });
        h.run();
    }
    let kf = h
        .snapshots()
        .into_iter()
        .find(|r| snap(r).0.purpose == 2)
        .unwrap();
    let v = h.verify_from(Some(kf.seq));
    assert!(v.is_empty(), "verify from the keyframe: {v:?}");
    let h2 = H::crash_restart(h, plan(SnapshotSel::Latest, true, 1, 0), Rerun::default()).unwrap();
    assert!(
        h2.s.engine().recovery_mismatches().is_empty(),
        "{:?}",
        h2.s.engine().recovery_mismatches()
    );
    assert!(h2.s.engine().recovery_replayed() > 0);
    assert!(h2.io_ref().out.is_empty());
    assert!(h2.io_ref().feed.is_empty());
    assert_eq!(h2.s.engine().ledger.pos(AC1, A1).qty, q("0.001"));
}

#[test]
fn seq_259_no_tail_tail_lost() {
    let mut h = steady();
    h.keyframe();
    h.open_position(AC1, A1, Side::Buy, "0.001");
    let h2 = H::crash_restart(
        h,
        plan(SnapshotSel::Latest, false, 1, RecoveryFlags::TAIL_LOST),
        Rerun::default(),
    )
    .unwrap();
    assert_eq!(h2.s.engine().recovery_replayed(), 0);
    for a in [AC1, AC2, AC9] {
        let f =
            h2.s.engine()
                .recovery_flags()
                .iter()
                .find(|x| x.0 == a)
                .map(|x| x.1)
                .unwrap();
        assert!(f & RecoveryFlags::TAIL_LOST != 0);
    }
    assert_eq!(h2.s.engine().ledger.pos(AC1, A1).qty.0, 0);
}

#[test]
fn seq_260_recovery_end_now() {
    let mut h = steady();
    h.keyframe();
    let rr = Rerun {
        tsc: 500_000,
        hz: 3 * HZ,
        ..Rerun::default()
    };
    let h2 = H::crash_restart(h, plan(SnapshotSel::Latest, true, 1, 0), rr).unwrap();
    let rs = h2.snapshots().pop().unwrap();
    let (hd, _) = snap(&rs);
    assert_eq!(hd.purpose, 4);
    assert_eq!(hd.snap_tsc, 500_000);
    // Timers were shifted into this run's TSC.
    let sw =
        h2.s.engine()
            .heap
            .sorted()
            .into_iter()
            .find(|e| e.owner == 3 && e.timer_id == 2)
            .unwrap();
    assert!(sw.deadline < 500_000 + 3_000_000_000_000, "{}", sw.deadline);
}

#[test]
fn seq_262_recovery_cap_no_cells() {
    let mut h = steady();
    h.cmd(
        AC2,
        &ControlCmd::SetMode {
            scope: scope(S2, AC2, G2),
            mode: hft_types::RunMode::Shadow,
        },
    );
    h.run();
    h.keyframe();
    let d1 = h.s.engine().risk.mode.desired(hft_risk::mode::CellKey {
        strategy: S1,
        account: AC1,
        group: G1,
    });
    let d2 = h.s.engine().risk.mode.desired(hft_risk::mode::CellKey {
        strategy: S2,
        account: AC2,
        group: G2,
    });
    let h2 = H::crash_restart(h, plan(SnapshotSel::Latest, true, 1, 0), Rerun::default()).unwrap();
    let e = h2.s.engine();
    for a in [AC1, AC2, AC9] {
        assert!(
            e.risk
                .mode
                .caps()
                .iter()
                .any(|c| c.source == CapSource::RECOVERY && c.scope.account == a),
            "{a:?}"
        );
    }
    assert_eq!(
        e.risk.mode.desired(hft_risk::mode::CellKey {
            strategy: S1,
            account: AC1,
            group: G1
        }),
        d1
    );
    assert_eq!(
        e.risk.mode.desired(hft_risk::mode::CellKey {
            strategy: S2,
            account: AC2,
            group: G2
        }),
        d2
    );
}

#[test]
fn seq_263_epoch() {
    for (persisted, want) in [(9u32, 10u32), (0, 2)] {
        let mut h = steady();
        h.keyframe();
        let snap_epoch = h.s.view().engine_epoch();
        assert_eq!(snap_epoch, 1);
        let mut h2 =
            H::crash_restart(h, plan(SnapshotSel::Latest, true, persisted, 0), Rerun::default()).unwrap();
        assert_eq!(h2.s.view().engine_epoch(), want);
        let n = h2.io_ref().out.len();
        let m = h2.mark();
        h2.step();
        let ep = h2
            .notes_since(m)
            .into_iter()
            .find(|n| n.code == hft_sequencer::codes::seq_note(1))
            .unwrap();
        assert_eq!(ep.v[0], u64::from(want));
        let se: Vec<_> = h2.io_ref().out[n..]
            .iter()
            .filter(|o| o.1.op == hft_event::GwOp::SetEpoch)
            .collect();
        assert!(!se.is_empty());
        assert!(se.iter().all(|o| o.1.epoch() == want as u16
            && o.1.prev_epoch() == snap_epoch as u16
            && o.1.accept_prev()));
    }
}

#[test]
fn seq_264_recovery_segment() {
    let mut h = steady();
    h.keyframe();
    h.quote(A1, "50000", "50004");
    h.run();
    let mut h2 = H::crash_restart(h, plan(SnapshotSel::Latest, true, 1, 0), Rerun::default()).unwrap();
    let stop = h2.s.view().stop_engine_ns();
    h2.step();
    let seg = h2.j().segments.last().unwrap().clone();
    assert_eq!(seg.reason, RollReason::Recovery as u8);
    let done = seg
        .records
        .iter()
        .position(|r| r.kind == EventKind::SEGMENT_ROLL && r.body.get(32) == Some(&3))
        .unwrap();
    let ec = &seg.records[done + 1];
    assert_eq!(ec.kind, EventKind::ENGINE_CLOCK);
    assert_eq!(ec.read::<hft_event::EngineClock>().unwrap().base_engine_ns, stop);
    let _ = (RecordFlags(0), Role::Input, MemJournal::default());
}

#[test]
fn seq_265_mirror_restore_batches() {
    let mut h = steady();
    // A resting GTC order of account 1 on A.
    add_hook(|hx, ev, ctx| {
        if let hft_strategy_sdk::EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
            && b.ask_px == px("50039")
        {
            let mut it = intent(
                hx,
                A1,
                Side::Buy,
                hft_types::Tif::Gtc,
                hft_strategy_sdk::IntentClass::Open,
                px("49000"),
                q("0.001"),
            );
            it.group = G1;
            let _ = ctx.place(it);
        }
    });
    let m = h.mark();
    h.quote(A1, "50000", "50039");
    h.run();
    let d = decisions(&h.since(m))[0].1;
    h.ack(&d);
    h.keyframe();
    let mut h2 = H::crash_restart(h, plan(SnapshotSel::Latest, true, 1, 0), Rerun::default()).unwrap();
    h2.step();
    let mr: Vec<_> = h2
        .io_ref()
        .out
        .iter()
        .filter(|o| o.1.op == hft_event::GwOp::MirrorRestore)
        .collect();
    assert_eq!(mr.len(), 1);
    assert_eq!(mr[0].0, hft_sequencer::OutRing::Venue(A));
    assert_eq!(mr[0].1.slot, d.slot);
}

/// SEQ-261.4: the new segment's verify replay starts from the `purpose = Recovery` snapshot and
/// reproduces the restarted run, including the gate inputs after the restart (README "恢复结束").
#[test]
fn seq_261_remaining_durations_preserved() {
    let mut h = steady();
    h.keyframe();
    h.open_position(AC1, A1, Side::Buy, "0.001");
    let mut h2 = H::crash_restart(h, plan(SnapshotSel::Latest, true, 1, 0), Rerun::default()).unwrap();
    h2.run();
    let t = h2.now();
    for v in [A, B] {
        h2.push(
            hft_sequencer::driver::live::RingId::Rep(0),
            &conn(
                v,
                0,
                hft_event::ConnStateKind::Up,
                hft_event::ConnRole::Md,
                hft_types::InstId(0),
                hft_types::AccountId(0),
                t,
            ),
        );
        for a in [AC1, AC2] {
            for (k, role) in [
                (0u8, hft_event::ConnRole::Order),
                (1, hft_event::ConnRole::Order),
                (0, hft_event::ConnRole::UserStream),
            ] {
                h2.push(
                    hft_sequencer::driver::live::RingId::Rep(0),
                    &conn(
                        v,
                        k,
                        hft_event::ConnStateKind::Up,
                        role,
                        hft_types::InstId(0),
                        a,
                        t,
                    ),
                );
            }
        }
    }
    h2.run();
    h2.sys(8);
    for a in [AC1, AC2] {
        h2.sys(16 | (u32::from(a.0) << 16));
    }
    h2.quote(A1, "50000", "50001");
    h2.run();
    // An order after the restart is tracked on its group leg (the OMS group legs survive the
    // epoch reload of the recovery end).
    let ext = |h: &H| {
        let e = h.s.engine();
        (1..=16u32)
            .filter_map(|s| e.oms.raw_slot(B, AC1, s))
            .filter(|(sl, _, _)| sl.state != hft_oms::OrderState::Free)
            .map(|(_, x, _)| x.leg_ix)
            .collect::<Vec<u8>>()
    };
    let rs = h2
        .snapshots()
        .into_iter()
        .rfind(|r| snap(r).0.purpose == 4)
        .expect("recovery snapshot");
    let v = h2.verify_from(Some(rs.seq));
    assert!(v.is_empty(), "verify from the Recovery snapshot: {v:?}");
    let _ = ext(&h2);
    assert!(
        h2.s.engine().oms.legs(AC1, G1).view.n_legs == 2,
        "group legs registered after the restart"
    );
}

/// SEQ-261.4: the Recovery snapshot restores exactly the state the restarted run holds.
#[test]
fn recovery_snapshot_restores_the_run_state() {
    let mut h = steady();
    h.keyframe();
    h.open_position(AC1, A1, Side::Buy, "0.001");
    let h2 = H::crash_restart(h, plan(SnapshotSel::Latest, true, 1, 0), Rerun::default()).unwrap();
    let rs = h2
        .snapshots()
        .into_iter()
        .rfind(|r| snap(r).0.purpose == 4)
        .expect("recovery snapshot");
    let mut wa = hft_strategy_sdk::SnapshotWriter::new(64 << 20);
    h2.s.write_sections(&mut wa);
    let log = hft_sequencer::driver::replay::Log::from_segments(&h2.j().segments);
    let drv = hft_sequencer::driver::replay::ReplayDriver::new(
        hft_sequencer::driver::replay::ReplayMode::Verify,
        log,
        0,
        1024,
    );
    let s: hft_sequencer::Sequencer<hft_sequencer::driver::replay::ReplayDriver, TestSet> =
        hft_sequencer::Sequencer::boot(
            hft_sequencer::BootSource::Journal(plan(SnapshotSel::AtSeq(rs.seq), false, 0, 0)),
            &h2.setup,
            drv,
            hb(),
        )
        .unwrap();
    let mut wb = hft_strategy_sdk::SnapshotWriter::new(64 << 20);
    s.write_sections(&mut wb);
    let (a, b) = (wa.as_bytes(), wb.as_bytes());
    let secs = |x: &[u8]| {
        let mut v = Vec::new();
        let mut off = 0usize;
        while off + 16 <= x.len() {
            let n = u16::from_le_bytes([x[off], x[off + 1]]);
            let len = u32::from_le_bytes([x[off + 8], x[off + 9], x[off + 10], x[off + 11]]) as usize;
            v.push((n, x[off..off + 16 + len].to_vec()));
            off += 16 + len;
        }
        v
    };
    let (sa, sb) = (secs(a), secs(b));
    for (n, x) in &sa {
        let y = sb
            .iter()
            .find(|(m, _)| m == n)
            .map(|(_, y)| y.clone())
            .unwrap_or_default();
        if *x != y {
            let k = x.iter().zip(y.iter()).position(|(p, q)| p != q).unwrap_or(0);
            eprintln!(
                "section {n} differs at {k}: live {:?} replay {:?}",
                &x[k.saturating_sub(4)..(k + 24).min(x.len())],
                &y[k.saturating_sub(4)..(k + 24).min(y.len())]
            );
        }
    }
    assert_eq!(a, b);
}
