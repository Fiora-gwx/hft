use crate::common::*;
use hft_event::{BlobHdr, BlobKind, EventKind};
use hft_refdata::{AccountKind, EcfBuilder, EngineConfig, EngineSection};
use hft_sequencer::driver::live::LiveDriver;
use hft_sequencer::driver::replay::Log;
use hft_sequencer::{BootSource, EngineSetup, Sequencer};
use hft_types::{AccountId, HostClass, VenueId};

fn full_setup(rd: &hft_refdata::RefData) -> EngineSetup {
    let mut s = b0_setup(rd);
    s.keys_required.clear();
    for id in 1..=u8::MAX {
        let a = rd.account(AccountId(id));
        if a.id.0 == 0 || matches!(a.kind, AccountKind::Sim(_)) {
            continue;
        }
        for v in 1..64u8 {
            if a.venues & (1u64 << v) != 0 {
                s.keys_required.push((a.id, VenueId(v), 3));
            }
        }
    }
    s
}

#[test]
fn seq_376_complete_config_roundtrip() {
    let rd = b0_rd().load();
    let mut s = full_setup(&rd);
    s.gws.venues.reverse(); // gw indexes are kill-word identities, not a set to re-sort.
    s.oms.hedge_deadline_ms = 12_345;
    s.oms.slots.set(A, AC1, 47);
    s.ledger.funding_tol_abs.0 += 41;
    s.exec.accounts[0].params = Some(hft_exec::ExecParams {
        broken_auto_fix_ms: 456_789,
        ..s.exec.default
    });
    s.advisor_max_lag_ms = 6789;
    s.drain_timeout_ms = 3456;
    let b = s.encode_blob(7, HostClass::Dev2, UTC0);
    let q = EngineSetup::decode_blob(&b, &rd, s.clock, s.oms.engine_epoch, s.base_engine_ns).unwrap();
    assert_eq!(q.caps, s.caps);
    assert_eq!(q.seq, s.seq);
    assert_eq!(q.oms, s.oms);
    assert_eq!(q.ledger, s.ledger);
    assert_eq!(q.exec, s.exec);
    assert_eq!(q.gws.venues, s.gws.venues);
    assert_eq!(q.feeds, s.feeds);
    assert_eq!(q.keys_required, s.keys_required);
    assert_eq!(
        (q.role_prod, q.advisor_max_lag_ms, q.drain_timeout_ms),
        (s.role_prod, s.advisor_max_lag_ms, s.drain_timeout_ms)
    );
    assert_eq!(b, q.encode_blob(7, HostClass::Dev2, UTC0));
}

#[test]
fn seq_377_missing_config_does_not_use_defaults() {
    let rd = b0_rd().load();
    let s = full_setup(&rd);
    let ec = EngineConfig::load(&s.encode_blob(1, HostClass::Dev2, UTC0), None).unwrap();
    for missing in [
        EngineSection::Oms,
        EngineSection::Ledger,
        EngineSection::Capacities,
        EngineSection::Sequencer,
        EngineSection::Exec,
        EngineSection::Assembly,
    ] {
        let mut b = EcfBuilder::new(1, HostClass::Dev2);
        for tag in EngineSection::ALL {
            if tag == missing {
                continue;
            }
            if let Some(v) = ec.section(tag) {
                b.section(tag, v.row_ver, v.row_size, v.count, v.bytes.to_vec());
            }
        }
        assert!(
            EngineSetup::decode_blob(&b.finish(), &rd, s.clock, 1, 0).is_err(),
            "missing {missing:?}"
        );
    }
    for mutation in 0..5 {
        let mut s = s.clone();
        match mutation {
            0 => s.keys_required.clear(),
            1 => s.gws.venues.push(s.gws.venues[0]),
            2 => s.keys_required[0].2 = 4,
            3 => {
                s.exec.accounts.remove(0);
            }
            _ => s.feeds.0 |= 1u64 << 63,
        };
        assert!(
            EngineSetup::decode_blob(&s.encode_blob(1, HostClass::Dev2, UTC0), &rd, s.clock, 1, 0).is_err(),
            "mutation {mutation}"
        );
    }
}

fn config_journal(complete: bool) -> hft_sequencer::journal::MemJournal {
    reset_script();
    let b = b0();
    let rd = b.rd.load();
    let setup = full_setup(&rd);
    let mut configs = b.configs(&rd);
    if complete {
        configs.engine_blob = setup.encode_blob(1, HostClass::Dev2, UTC0);
        configs.risk_blob = configs.risk.encode_blob().unwrap();
        configs.risk_sha256 = sha(&configs.risk_blob);
    }
    let heartbeat = hb();
    let drv = LiveDriver::new(TestIo::<TestSet>::new(setup.clock), b.live);
    let s = Sequencer::boot(BootSource::Empty { configs }, &setup, drv, heartbeat).unwrap();
    let mut h = H {
        s,
        rd,
        setup,
        cmd_no: 0,
        hb: heartbeat,
        upd: 1,
    };
    h.steady();
    h.open_position(AC1, A1, hft_types::Side::Buy, "0.001");
    h.open_position(AC1, B1, hft_types::Side::Sell, "0.001");
    h.keyframe();
    for k in 0..12 {
        h.quote(A1, "50000", if k % 2 == 0 { "50003" } else { "50004" });
        h.run();
    }
    h.keyframe();
    h.j().clone()
}

#[test]
fn seq_378_replay_config_only_from_journal() {
    // Drop every assembler configuration value. Decode fresh from the journal's original boot
    // block, then use the same VerifyDriver as replayer, including all output snapshots.
    let j = config_journal(true);
    let log = Log::from_segments(&j.segments);
    let at = log
        .recs
        .iter()
        .find(|r| r.kind == EventKind::STATE_SNAPSHOT)
        .unwrap()
        .seq;
    let setup = EngineSetup::from_snapshot(&log, at).unwrap();
    assert!(verify_journal::<TestSet>(&j, &setup, None).is_empty());
    // A physically complete configuration carries all three component sections, not a marker.
    assert!(
        log.recs
            .iter()
            .filter(|r| r.kind == EventKind::CONFIG_BLOB)
            .any(|r| r.body.windows(8).any(|w| w == hft_risk::RISK_BLOB_MAGIC))
    );
}

#[test]
fn seq_379_snapshot_config_requires_preceding_verified_blob() {
    let log = Log::from_segments(&config_journal(true).segments);
    let at = log
        .recs
        .iter()
        .find(|r| r.kind == EventKind::STATE_SNAPSHOT)
        .unwrap()
        .seq;
    for kind in [
        BlobKind::Engine,
        BlobKind::Refdata,
        BlobKind::RiskParams,
        BlobKind::StrategyParams,
    ] {
        let ix = log
            .recs
            .iter()
            .position(|r| r.kind == EventKind::CONFIG_BLOB && r.read::<BlobHdr>().unwrap().blob.kind == kind)
            .unwrap();
        for mutation in 0..4 {
            let mut bad = log.clone();
            match mutation {
                0 => {
                    bad.recs.remove(ix);
                }
                1 => {
                    let r = bad.recs.remove(ix);
                    bad.recs.push(r);
                }
                2 => bad.recs[ix].body[80] ^= 1,
                _ => {
                    bad.recs[ix].body.truncate(80);
                }
            }
            assert!(
                EngineSetup::from_snapshot(&bad, at).is_err(),
                "{kind:?} mutation {mutation}"
            );
        }
    }
    assert!(EngineSetup::from_snapshot(&log, u64::MAX).is_err());
    let mut bad = log.clone();
    bad.segs[0].tsc_hz_milli += 1;
    assert!(EngineSetup::from_snapshot(&bad, at).is_err());
}

#[test]
fn seq_380_complete_config_preserves_business_outputs() {
    let old = config_journal(false);
    let new = config_journal(true);
    let project = |j: &hft_sequencer::journal::MemJournal| {
        j.records()
            .filter(|r| !matches!(r.kind, EventKind::CONFIG_BLOB | EventKind::STATE_SNAPSHOT))
            .cloned()
            .collect::<Vec<_>>()
    };
    assert!(new.records().any(|r| r.kind == EventKind::ORDER_DECISION));
    assert_eq!(project(&old), project(&new));
    let snapshots = |j: &hft_sequencer::journal::MemJournal| {
        j.records()
            .filter(|r| r.kind == EventKind::STATE_SNAPSHOT)
            .cloned()
            .collect::<Vec<_>>()
    };
    let before = snapshots(&old);
    let after = snapshots(&new);
    assert_eq!(before.len(), after.len());
    for (a, b) in before.iter().zip(&after) {
        let mut x = hft_strategy_sdk::SnapshotReader::new(&a.body[96..]);
        let mut y = hft_strategy_sdk::SnapshotReader::new(&b.body[96..]);
        while let Some(s) = x.next_section().unwrap() {
            let t = y.next_section().unwrap().unwrap();
            assert_eq!((s.section, s.version, s.count), (t.section, t.version, t.count));
            if s.section == 9 {
                assert_ne!(
                    s.body, t.body,
                    "complete config must change active blob identities"
                );
            } else {
                assert_eq!(s.body, t.body, "snapshot {} section {}", a.seq, s.section);
            }
        }
        assert!(y.next_section().unwrap().is_none());
        assert_ne!(
            hft_sequencer::snapshot::state_hash(&a.body[96..]),
            hft_sequencer::snapshot::state_hash(&b.body[96..])
        );
    }
}

#[test]
fn seq_381_config_clock_follows_boot_and_recovery_order() {
    let j = config_journal(true);
    let log = Log::from_segments(&j.segments);
    let snapshots: Vec<_> = log
        .recs
        .iter()
        .filter(|r| r.kind == EventKind::STATE_SNAPSHOT)
        .collect();
    let initial = hft_sequencer::snapshot::read_seq_sections(&snapshots[0].body[96..]).unwrap();
    assert_eq!(
        initial.engine_clock,
        hft_sequencer::clock::EngineClockRow::default()
    );
    for r in &snapshots {
        assert_eq!(
            EngineSetup::from_snapshot(&log, r.seq)
                .unwrap()
                .clock
                .tsc_hz_milli,
            HZ
        );
    }
    let mut bad = log.clone();
    bad.recs.retain(|r| r.kind != EventKind::CLOCK_STATUS);
    assert!(EngineSetup::from_snapshot(&bad, snapshots[0].seq).is_err());
    let setup = EngineSetup::from_snapshot(&log, snapshots.last().unwrap().seq).unwrap();
    let mut h = H::<TestSet>::restart_over(
        j,
        setup,
        plan(hft_sequencer::SnapshotSel::Latest, true, 2, 0),
        Rerun {
            hz: 2 * HZ,
            ..Rerun::default()
        },
    )
    .unwrap();
    let log = Log::from_segments(&h.j().segments);
    let rec = log
        .recs
        .iter()
        .rfind(|r| r.kind == EventKind::STATE_SNAPSHOT)
        .unwrap();
    assert_eq!(
        rec.read::<hft_event::SnapshotHdr>().unwrap().purpose,
        hft_sequencer::snapshot::PURPOSE_RECOVERY
    );
    let sections = hft_sequencer::snapshot::read_seq_sections(&rec.body[96..]).unwrap();
    assert_eq!(sections.engine_clock.tsc_hz_milli, HZ); // Previous run until the next EngineClock Input.
    let setup = EngineSetup::from_snapshot(&log, rec.seq).unwrap();
    assert_eq!(setup.clock.tsc_hz_milli, 2 * HZ);
    assert_eq!(setup.oms.tsc_hz_milli, 2 * HZ);
    h.run();
    assert!(verify_journal::<TestSet>(h.j(), &setup, Some(rec.seq)).is_empty());
}

#[test]
fn seq_395_unstarted_clock_is_local_to_process_boot() {
    use hft_event::{SegmentRoll, SnapshotHdr};
    use hft_journal::{RollMark, RollReason};
    use hft_sequencer::{BootError, RecoveryFlags, SnapshotSel};
    use zerocopy::IntoBytes;

    let j = config_journal(true);
    let old = Log::from_segments(&j.segments);
    let initial = old
        .recs
        .iter()
        .find(|r| r.kind == EventKind::STATE_SNAPSHOT)
        .unwrap()
        .seq;
    let clock = old
        .recs
        .iter()
        .find(|r| r.kind == EventKind::ENGINE_CLOCK)
        .unwrap()
        .clone();
    let setup = EngineSetup::from_snapshot(&old, initial).unwrap();
    let mut h = H::<TestSet>::restart_over(
        j,
        setup,
        plan(
            SnapshotSel::AtSeq(initial),
            false,
            2,
            RecoveryFlags::FROM_SNAPSHOT | RecoveryFlags::TAIL_LOST,
        ),
        Rerun {
            hz: 2 * HZ,
            ..Rerun::default()
        },
    )
    .unwrap();
    let log = Log::from_segments(&h.j().segments);
    let start = *log.seg_start.last().unwrap();
    let ix = log
        .recs
        .iter()
        .rposition(|r| r.kind == EventKind::STATE_SNAPSHOT)
        .unwrap();
    let at = log.recs[ix].seq;
    let secs = hft_sequencer::snapshot::read_seq_sections(&log.recs[ix].body[96..]).unwrap();
    assert_eq!(secs.engine_clock, hft_sequencer::clock::EngineClockRow::default());
    assert!(
        log.recs[..start]
            .iter()
            .any(|r| r.kind == EventKind::ENGINE_CLOCK)
    );
    let setup = EngineSetup::from_snapshot(&log, at).unwrap();
    assert_eq!(setup.clock.tsc_hz_milli, 2 * HZ);
    let done = log.recs[start..]
        .iter()
        .find(|r| {
            r.kind == EventKind::SEGMENT_ROLL
                && r.read::<SegmentRoll>()
                    .is_some_and(|s| s.mark == RollMark::BootDone as u8)
        })
        .unwrap()
        .clone();
    for case in 0..4 {
        let mut bad = log.clone();
        match case {
            0 => {
                let mut begin = bad.recs[start].read::<SegmentRoll>().unwrap();
                begin.reason = RollReason::Hourly as u8;
                bad.recs[start].body = begin.as_bytes().to_vec();
                let mut snap = bad.recs[ix].read::<SnapshotHdr>().unwrap();
                snap.purpose = hft_sequencer::snapshot::PURPOSE_BOOT;
                bad.recs[ix].body[..size_of::<SnapshotHdr>()].copy_from_slice(snap.as_bytes());
            }
            1 => {
                let mut snap = bad.recs[ix].read::<SnapshotHdr>().unwrap();
                snap.purpose = hft_sequencer::snapshot::PURPOSE_KEYFRAME;
                bad.recs[ix].body[..size_of::<SnapshotHdr>()].copy_from_slice(snap.as_bytes());
            }
            2 => bad.recs.insert(ix, clock.clone()),
            3 => bad.recs.insert(ix, done.clone()),
            _ => unreachable!(),
        }
        assert!(
            matches!(
                EngineSetup::from_snapshot(&bad, at),
                Err(BootError::SnapshotUndecodable { section: 25 })
            ),
            "invalid empty clock accepted: {case}"
        );
    }
    h.run();
    assert!(verify_journal::<TestSet>(h.j(), &setup, Some(at)).is_empty());
}

#[test]
fn seq_398_snapshot_recovery_audit_uses_committed_tail() {
    use hft_event::{Note, NoteSource, RecordFlags};
    use hft_sequencer::journal::JournalSink;
    use hft_sequencer::{BootError, RecoveryFlags, SnapshotSel};
    use hft_types::Tsc;

    for latest in [false, true] {
        let mut j = config_journal(true);
        let old = Log::from_segments(&j.segments);
        let snaps: Vec<_> = old
            .recs
            .iter()
            .filter(|r| r.kind == EventKind::STATE_SNAPSHOT)
            .collect();
        let selected = if latest {
            snaps.last().unwrap().seq
        } else {
            snaps[0].seq
        };
        let committed_end = old.recs[hft_sequencer::recover::event_end(&old) - 1].seq;
        if latest {
            assert_eq!(selected, committed_end);
        }
        // An actual half event is present in storage but must stay outside the audit range.
        let bbo = old.recs.iter().find(|r| r.kind == EventKind::BBO).unwrap();
        let half = j
            .append_var(EventKind::BBO, RecordFlags(0), Tsc(bbo.tsc), &[&bbo.body])
            .unwrap();
        assert!(half.0 > committed_end);
        let setup = EngineSetup::from_snapshot(&old, selected).unwrap();
        let mut h = H::<TestSet>::restart_over(
            j.clone(),
            setup.clone(),
            plan(
                SnapshotSel::AtSeq(selected),
                false,
                2,
                RecoveryFlags::FROM_SNAPSHOT | RecoveryFlags::TAIL_LOST,
            ),
            Rerun::default(),
        )
        .unwrap();
        let all = Log::from_segments(&h.j().segments);
        let start = *all.seg_start.last().unwrap();
        let recovery_snapshot = all.recs[start..]
            .iter()
            .find(|r| r.kind == EventKind::STATE_SNAPSHOT)
            .unwrap()
            .seq;
        let notes: Vec<_> = all.recs[start..]
            .iter()
            .filter_map(|r| (r.kind == EventKind::NOTE).then(|| r.read::<Note>()).flatten())
            .filter(|n| {
                n.code
                    == hft_event::note_code(
                        NoteSource::Boot,
                        hft_sequencer::codes::boot_note::RECOVER_FROM_SNAPSHOT,
                    )
            })
            .collect();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].v[0], selected);
        assert_eq!(
            [notes[0].v[1], notes[0].v[2]],
            if latest {
                [0, 0]
            } else {
                [selected + 1, committed_end]
            }
        );
        h.run();
        h.keyframe();
        let setup =
            EngineSetup::from_snapshot(&Log::from_segments(&h.j().segments), recovery_snapshot).unwrap();
        assert!(verify_journal::<TestSet>(h.j(), &setup, Some(recovery_snapshot)).is_empty());
        assert!(matches!(
            H::<TestSet>::restart_over(
                j,
                setup,
                plan(
                    SnapshotSel::AtSeq(selected),
                    true,
                    2,
                    RecoveryFlags::FROM_SNAPSHOT | RecoveryFlags::TAIL_LOST
                ),
                Rerun::default(),
            ),
            Err(BootError::Config(_))
        ));
    }
}
