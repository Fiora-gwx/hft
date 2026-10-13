//! Snapshots and boot blocks (TESTS.md "快照": SEQ-242–SEQ-255).

use core::mem::{offset_of, size_of};

use hft_event::{ControlCmd, EventKind, Role, SectionHdr};
use hft_journal::{RollAt, RollReason};
use hft_sequencer::market::{ConnLiveRow, MarketLastRow};
use hft_sequencer::snapshot::{
    ClockCalRow, GroupSeqRow, StrategyRowHdr, StrategyTimerRow, StrategyTimersHdr,
};
use hft_sequencer::timers::TimerRow;
use hft_sequencer::{
    ActiveConfigRow, CmdDedupHdr, CmdDedupRow, ReconPendingHdr, VenueNoticeRow, ViewerPreviewRow,
};
use proptest::prelude::*;
use std::collections::BTreeSet;
use zerocopy::{FromBytes, IntoBytes};

use crate::common::*;

fn steady() -> H {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    h
}

/// `(section, version)` in body order.
fn sections(body: &[u8]) -> Vec<(u16, u16)> {
    let mut v = Vec::new();
    let mut off = 0usize;
    while let Some(b) = body.get(off..off + 16) {
        let Ok(sh) = SectionHdr::read_from_bytes(b) else {
            break;
        };
        v.push((sh.section, sh.version));
        off += 16 + sh.len as usize;
    }
    v
}

fn on_demand(h: &mut H) -> hft_sequencer::MemRecord {
    h.cmd(AC1, &ControlCmd::SnapshotNow);
    h.run();
    h.run();
    h.snapshots().pop().unwrap()
}

#[test]
fn seq_242_bootblock_order() {
    let mut h = steady();
    let n_out = h.io_ref().out.len();
    let n_feed = h.io_ref().feed.len();
    let segs0 = h.j().segments.len();
    h.sys(2);
    let segs = &h.j().segments;
    assert_eq!(segs.len(), segs0 + 1);
    let old = &segs[segs.len() - 2];
    let end = old.records.last().unwrap();
    assert_eq!(end.kind, EventKind::SEGMENT_ROLL);
    assert_eq!(end.role(), Role::Meta);
    let new = &segs[segs.len() - 1];
    let kinds: Vec<EventKind> = new.records.iter().map(|r| r.kind).collect();
    assert_eq!(kinds[0], EventKind::SEGMENT_ROLL);
    assert_eq!(kinds[1], EventKind::HOST_PROFILE);
    assert_eq!(kinds[2], EventKind::CLOCK_STATUS);
    let blobs: Vec<hft_event::BlobKind> = new
        .records
        .iter()
        .filter(|r| r.kind == EventKind::CONFIG_BLOB)
        .filter_map(|r| hft_sequencer::types::read_body::<hft_event::BlobHdr>(&r.body).map(|b| b.blob.kind))
        .collect();
    use hft_event::BlobKind as K;
    assert_eq!(
        blobs,
        vec![
            K::Refdata,
            K::Engine,
            K::StrategyParams,
            K::StrategyParams,
            K::StrategyParams,
            K::RiskParams
        ]
    );
    let si = kinds
        .iter()
        .position(|k| *k == EventKind::STATE_SNAPSHOT)
        .unwrap();
    assert!(kinds[3..si].iter().all(|k| *k == EventKind::CONFIG_BLOB));
    assert_eq!(*kinds.last().unwrap(), EventKind::SEGMENT_ROLL);
    let (hd, _) = snap(&new.records[si]);
    assert_eq!(hd.purpose, 1);
    assert_eq!(h.io_ref().out.len(), n_out);
    assert_eq!(h.io_ref().feed.len(), n_feed);
}

#[test]
fn seq_244_roll_next_not_ready() {
    let mut h = steady();
    h.io().j.faults.needs_roll = Some((RollReason::Size, RollAt::EventBoundary));
    h.io().j.faults.roll_not_ready = 1;
    let segs0 = h.j().segments.len();
    let m = h.mark();
    h.quote(A1, "50000", "50004");
    h.step();
    assert_eq!(h.j().segments.len(), segs0);
    let r1: Vec<_> = h.since(m);
    let t = r1
        .iter()
        .position(|r| {
            r.kind == EventKind::TIMER
                && r.read::<hft_event::Timer>()
                    .is_some_and(|t| t.owner == 1 && t.timer_id == 2)
        })
        .expect("roll timer");
    assert!(r1.get(t + 1).is_none_or(|r| r.role() == Role::Input));
    h.quote(A1, "50000", "50005");
    h.step();
    h.io().j.faults.needs_roll = None;
    assert_eq!(h.j().segments.len(), segs0 + 1);
    let v = h.verify();
    assert!(v.is_empty(), "{v:?}");
}

#[test]
fn seq_248_section_write_order() {
    let mut h = steady();
    let o = on_demand(&mut h);
    let s: Vec<u16> = sections(&o.body[96..]).iter().map(|x| x.0).collect();
    assert_eq!(
        s,
        vec![
            1, 13, 22, 2, 3, 12, 4, 14, 17, 18, 23, 5, 8, 9, 10, 15, 16, 19, 20, 24, 25, 26, 27, 29, 6, 7, 21
        ]
    );
    assert_eq!(snap(&o).0.n_sections, 27);
}

#[cfg(feature = "testkit")]
#[test]
fn seq_250_start_from_snapshot_rebuild() {
    use hft_sequencer::audit::RestoreRow;
    use hft_sequencer::driver::replay::{Log, ReplayDriver, ReplayMode};
    use hft_sequencer::{BootSource, Sequencer, SnapshotSel};
    reset_script();
    let mut b = b0();
    b.params.retain(|p| [S1, S2].contains(&p.strategy));
    for p in &mut b.params {
        let (mem, ckpt) = if p.strategy == S1 {
            (1024, 128)
        } else {
            (2048, 256)
        };
        p.values[F_MEM] = mem;
        p.values[F_CKPT] = ckpt;
    }
    let mut h = b.boot_self_contained();
    h.steady();
    h.at(T0 + 7_000_000);
    h.mark_md(B1, "50000");
    h.funding_md(B1, "0.0001", UTC0 + 3_600_000_000_000, 3600);
    let snapshot = h.keyframe();
    let (hdr, rows) = snap(&snapshot);
    let b1 = rows.market.iter().find(|r| r.inst == B1).unwrap();
    assert_eq!(
        (b1.mark_px, b1.mark_ens, b1.interval_s),
        (px("50000"), 7_000_000, 3600)
    );
    assert_eq!(rows.market.iter().find(|r| r.inst == A1).unwrap().interval_s, 0);
    h.quote(A1, "50000", "50008");
    h.run();
    h.keyframe();
    let log = Log::from_segments(&h.j().segments);
    let original = log.recs.clone();
    let drv = ReplayDriver::new(ReplayMode::Verify, log, 0, 1);
    let mut replay = Sequencer::<ReplayDriver, TestSet>::boot(
        BootSource::Journal(plan(SnapshotSel::AtSeq(snapshot.seq), true, 0, 0)),
        &h.setup,
        drv,
        hb(),
    )
    .unwrap();
    assert_eq!(replay.driver().handed, 0);
    assert_eq!(replay.driver().sink.compared, 0);
    assert_eq!(replay.driver().sink.log.recs, original);
    assert_eq!(replay.engine().market.rx_from_seq, hdr.at_seq);
    let expected = [
        RestoreRow::ModelBudget {
            strategy: S1,
            mem: 1024,
            ckpt: 128,
        },
        RestoreRow::ModelBudget {
            strategy: S2,
            mem: 2048,
            ckpt: 256,
        },
        RestoreRow::LedgerGroups {
            strategy: S1,
            groups: 1,
        },
        RestoreRow::RiskGroups {
            account: AC1,
            groups: 1,
        },
        RestoreRow::LedgerGroups {
            strategy: S2,
            groups: 1,
        },
        RestoreRow::RiskGroups {
            account: AC2,
            groups: 1,
        },
        RestoreRow::RestoreMarks { count: 1 },
        RestoreRow::Mark {
            inst: B1,
            px: px("50000"),
            engine_ns: 7_000_000,
        },
        RestoreRow::FundingInterval {
            inst: B1,
            seconds: 3600,
        },
    ];
    assert_eq!(replay.engine().restore_observations().unwrap(), expected);
    for (id, mem, ckpt) in [(S1, 1024, 128), (S2, 2048, 256)] {
        let s = replay.instances().get(id).unwrap();
        assert_eq!((s.model_mem, s.model_ckpt), (mem, ckpt));
    }
    assert_eq!(replay.engine().ledger.funding_grid(B, B1).0, 3600);
    for _ in 0..1_000 {
        if replay.driver().finished() {
            break;
        }
        assert!(!matches!(replay.step(), hft_sequencer::StepOutcome::Fatal(_)));
    }
    assert!(replay.driver().finished());
    replay.driver_mut().finish_verification();
    assert!(replay.driver().mismatches().is_empty());
    assert_eq!(replay.engine().restore_observations().unwrap(), expected);
    verify_snapshot_fixture(&h, None);
    verify_snapshot_fixture(&h, Some(snapshot.seq));
}

fn verify_snapshot_fixture(h: &H, from: Option<u64>) {
    assert!(h.verify_from(from).is_empty());
    #[cfg(feature = "replay-io")]
    {
        use hft_sequencer::driver::replay::Log;
        use hft_sequencer::driver::verify::{VerifyOptions, verify};
        let checked = verify::<TestSet>(
            Log::from_segments(&h.j().segments),
            VerifyOptions {
                from_seq: from,
                ..VerifyOptions::default()
            },
            hb(),
        )
        .unwrap();
        assert!(checked.inputs > 0);
        assert!(
            checked.sink.mismatches.is_empty(),
            "{:?}",
            checked.sink.mismatches
        );
    }
}

#[test]
fn seq_251_row_layouts() {
    assert_eq!(size_of::<StrategyRowHdr>(), 24);
    assert_eq!(offset_of!(StrategyRowHdr, kind_id), 2);
    assert_eq!(offset_of!(StrategyRowHdr, desc_version), 6);
    assert_eq!(offset_of!(StrategyRowHdr, params_version), 8);
    assert_eq!(offset_of!(StrategyRowHdr, len), 16);
    assert_eq!(offset_of!(StrategyRowHdr, n_groups), 20);
    assert_eq!(size_of::<GroupSeqRow>(), 16);
    assert_eq!(offset_of!(GroupSeqRow, spec_seq), 8);
    assert_eq!(size_of::<TimerRow>(), 24);
    assert_eq!(offset_of!(TimerRow, timer_id), 8);
    assert_eq!(offset_of!(TimerRow, owner), 12);
    assert_eq!(offset_of!(TimerRow, group), 16);
    assert_eq!(size_of::<StrategyTimersHdr>(), 16);
    assert_eq!(offset_of!(StrategyTimersHdr, next_id), 8);
    assert_eq!(size_of::<StrategyTimerRow>(), 24);
    assert_eq!(offset_of!(StrategyTimerRow, deadline_tsc), 8);
    assert_eq!(offset_of!(StrategyTimerRow, tag), 16);
    assert_eq!(offset_of!(StrategyTimerRow, group), 20);
    assert_eq!(size_of::<ActiveConfigRow>(), 64);
    assert_eq!(size_of::<MarketLastRow>(), 112);
    assert_eq!(offset_of!(MarketLastRow, funding_rate), 32);
    assert_eq!(offset_of!(MarketLastRow, mark_px), 48);
    assert_eq!(offset_of!(MarketLastRow, settled_cut_ns), 72);
    assert_eq!(offset_of!(MarketLastRow, bbo_member_mask), 80);
    assert_eq!(offset_of!(MarketLastRow, bbo_flags), 81);
    assert_eq!(offset_of!(MarketLastRow, bbo_hw_rx_ns), 88);
    assert_eq!(offset_of!(MarketLastRow, mark_ens), 96);
    assert_eq!(offset_of!(MarketLastRow, prev_interval_s), 104);
    assert_eq!(size_of::<ReconPendingHdr>(), 16);
    assert_eq!(offset_of!(ReconPendingHdr, n_stored), 10);
    assert_eq!(offset_of!(ReconPendingHdr, count), 12);
    assert_eq!(offset_of!(ReconPendingHdr, received), 14);
    assert_eq!(size_of::<CmdDedupHdr>(), 8);
    assert_eq!(size_of::<CmdDedupRow>(), 32);
    assert_eq!(offset_of!(CmdDedupRow, cmd_seq), 16);
    assert_eq!(offset_of!(CmdDedupRow, status), 24);
    assert_eq!(size_of::<ViewerPreviewRow>(), 64);
    assert_eq!(offset_of!(ViewerPreviewRow, issuer), 32);
    assert_eq!(offset_of!(ViewerPreviewRow, preview_engine_ns), 56);
    assert_eq!(size_of::<ClockCalRow>(), 40);
    assert_eq!(offset_of!(ClockCalRow, tsc_hz_milli), 24);
    assert_eq!(offset_of!(ClockCalRow, from_seq), 32);
    assert_eq!(size_of::<VenueNoticeRow>(), 32);
    assert_eq!(offset_of!(VenueNoticeRow, inst), 4);
    assert_eq!(offset_of!(VenueNoticeRow, a), 8);
    assert_eq!(offset_of!(VenueNoticeRow, cmd_seq), 24);
    assert_eq!(size_of::<ConnLiveRow>(), 16);
    assert_eq!(offset_of!(ConnLiveRow, flags), 2);
    assert_eq!(offset_of!(ConnLiveRow, live_tsc), 8);
}

#[test]
fn seq_252_row_versions() {
    let mut h = steady();
    let o = on_demand(&mut h);
    let s = sections(&o.body[96..]);
    for (sec, ver) in [
        (5u16, 2u16),
        (8, 2),
        (9, 1),
        (10, 6),
        (15, 2),
        (16, 1),
        (19, 2),
        (24, 1),
        (26, 2),
        (27, 1),
    ] {
        assert_eq!(
            s.iter().find(|x| x.0 == sec).map(|x| x.1),
            Some(ver),
            "section {sec}"
        );
    }
}

fn compatibility_snapshot(r: &mut hft_sequencer::MemRecord, case: u8) {
    let mut body = Vec::new();
    let mut reader = hft_strategy_sdk::SnapshotReader::new(&r.body[96..]);
    while let Some(section) = reader.next_section().unwrap() {
        let mut head = SectionHdr {
            section: section.section,
            version: section.version,
            count: section.count,
            len: section.body.len() as u32,
            _rsv: 0,
        };
        let mut rows = section.body.to_vec();
        if section.section == 10 && [1, 2].contains(&case) {
            assert_eq!(rows.len(), section.count as usize * 112);
            rows.clear();
            for row in section.body.chunks_exact(112) {
                if case == 1 {
                    rows.extend_from_slice(&row[..104]);
                } else {
                    rows.extend_from_slice(row);
                    rows.extend_from_slice(&[0xA5; 8]);
                }
            }
            head.version = if case == 1 { 5 } else { 7 };
            head.len = rows.len() as u32;
        } else if section.section == 8 && case == 4 {
            head.count += 1;
        }
        body.extend_from_slice(head.as_bytes());
        body.extend_from_slice(&rows);
    }
    let mut hdr = r.read::<hft_event::SnapshotHdr>().unwrap();
    if case == 3 {
        body.extend_from_slice(
            SectionHdr {
                section: 99,
                version: 1,
                count: 5,
                len: 40,
                _rsv: 0,
            }
            .as_bytes(),
        );
        body.extend_from_slice(&[0xB7; 40]);
        hdr.n_sections += 1;
    }
    hdr.state_hash = hft_sequencer::snapshot::state_hash(&body);
    r.body = hdr.as_bytes().to_vec();
    r.body.extend_from_slice(&body);
}

#[test]
fn seq_253_row_compat() {
    use hft_sequencer::{BootError, SnapshotSel};
    reset_script();
    let mut source = b0().boot_self_contained();
    source.steady();
    source.at(T0 + 7_000_000);
    source.mark_md(B1, "50000");
    source.funding_md(B1, "0.0001", UTC0 + 7_200_000_000_000, 7200);
    source.funding_md(B1, "0.0002", UTC0 + 3_600_000_000_000, 3600);
    let original = source.keyframe();
    let original_rows = snap(&original).1.market;
    assert_eq!(
        original_rows
            .iter()
            .find(|r| r.inst == B1)
            .unwrap()
            .prev_interval_s,
        7200
    );
    verify_snapshot_fixture(&source, None);
    for case in 1..=4 {
        let mut journal = source.j().clone();
        let r = journal
            .segments
            .iter_mut()
            .flat_map(|s| &mut s.records)
            .find(|r| r.seq == original.seq)
            .unwrap();
        compatibility_snapshot(r, case);
        if case != 4 {
            // Compatibility means successful restore followed by current-format output.
            // It is not byte-identical regeneration of an old or unknown serialized row.
            #[cfg(feature = "replay-io")]
            {
                use hft_sequencer::driver::replay::Log;
                use hft_sequencer::driver::replay_snapshot::{SnapshotCheckError, check_snapshot};
                assert!(matches!(
                    check_snapshot::<TestSet>(&Log::from_segments(&journal.segments), original.seq),
                    Err(SnapshotCheckError::Size | SnapshotCheckError::RoundTrip { .. })
                ));
            }
        }
        let recovered = H::<TestSet>::restart_over(
            journal,
            source.setup.clone(),
            plan(SnapshotSel::Latest, true, 1, 0),
            Rerun::default(),
        );
        if case == 4 {
            assert!(matches!(
                recovered,
                Err(BootError::SnapshotUndecodable { section: 8 })
            ));
            continue;
        }
        let mut recovered = recovered.unwrap();
        let new_snapshot = recovered.snapshots().pop().unwrap();
        let new_rows = snap(&new_snapshot).1.market;
        assert_eq!(new_rows.len(), original_rows.len());
        for (actual, old) in new_rows.iter().zip(&original_rows) {
            assert_eq!(&actual.as_bytes()[..104], &old.as_bytes()[..104], "case {case}");
            assert_eq!(
                actual.prev_interval_s,
                if case == 1 {
                    old.interval_s
                } else {
                    old.prev_interval_s
                }
            );
            assert_eq!(actual._pad2, 0);
        }
        assert!(recovered.s.engine().recovery_mismatches().is_empty());
        recovered.run();
        recovered.quote(A1, "50001", "50009");
        recovered.run();
        recovered.keyframe();
        // Independent replay starts at the actual new Recovery boundary, after migration.
        verify_snapshot_fixture(&recovered, Some(new_snapshot.seq));
    }
}

// Read the specified row bytes independently of read_seq_sections and writer sorting.
fn assert_snapshot_rows(r: &hft_sequencer::MemRecord, used: &BTreeSet<(u8, u8)>) {
    assert_eq!(&r.body[1..4], &[0; 3]);
    assert_eq!(&r.body[36..40], &[0; 4]);
    let mut reader = hft_strategy_sdk::SnapshotReader::new(&r.body[96..]);
    let mut seen = BTreeSet::new();
    while let Some(s) = reader.next_section().unwrap() {
        assert!(seen.insert(s.section));
        let end = 96 + reader.pos();
        assert_eq!(&r.body[end - s.body.len() - 4..end - s.body.len()], &[0; 4]);
        match s.section {
            5 => {
                let mut off = 0;
                let mut last = None;
                for _ in 0..s.count {
                    let hdr = StrategyRowHdr::read_from_bytes(&s.body[off..off + 24]).unwrap();
                    assert!(last.is_none_or(|id| id < hdr.strategy));
                    last = Some(hdr.strategy);
                    assert_eq!(hdr._pad, 0);
                    off += 24;
                    let mut group = None;
                    for _ in 0..hdr.n_groups {
                        let g = GroupSeqRow::read_from_bytes(&s.body[off..off + 16]).unwrap();
                        assert!(group.is_none_or(|id| id < g.group));
                        group = Some(g.group);
                        assert_eq!(g._pad, 0);
                        off += 16;
                    }
                    off += hdr.len as usize;
                    let pad = (hdr.len as usize).next_multiple_of(8) - hdr.len as usize;
                    assert!(s.body[off..off + pad].iter().all(|b| *b == 0));
                    off += pad;
                }
                assert_eq!(off, s.body.len());
            }
            10 => {
                assert_eq!(s.body.len(), s.count as usize * 112);
                let mut last = None;
                for bytes in s.body.chunks_exact(112) {
                    let row = MarketLastRow::read_from_bytes(bytes).unwrap();
                    assert!(last.is_none_or(|id| id < row.inst));
                    last = Some(row.inst);
                    assert_eq!(row._rsv, [0; 6]);
                    assert_eq!(row._pad2, 0);
                }
            }
            24 => {
                assert_eq!(s.body.len(), s.count as usize * 32);
                let mut last = None;
                for bytes in s.body.chunks_exact(32) {
                    let row = VenueNoticeRow::read_from_bytes(bytes).unwrap();
                    let key = (row.venue, row.inst, row.kind);
                    assert!(last.is_none_or(|k| k < key));
                    last = Some(key);
                    assert_eq!(row._pad, 0);
                }
            }
            26 => {
                assert_eq!(s.body.len(), s.count as usize * 16);
                let mut actual = BTreeSet::new();
                let mut last = None;
                for bytes in s.body.chunks_exact(16) {
                    let row = ConnLiveRow::read_from_bytes(bytes).unwrap();
                    let key = (row.venue.0, row.slot);
                    assert!(last.is_none_or(|k| k < key));
                    last = Some(key);
                    assert_eq!(row._pad, [0; 5]);
                    actual.insert(key);
                }
                assert_eq!(&actual, used);
            }
            _ => {}
        }
    }
    for id in [5, 10, 24, 26] {
        assert!(seen.contains(&id));
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(ProptestConfig::default().cases.max(300)))]
    #[test]
    fn seq_254_rows_sorted_and_zero_padded(words in proptest::collection::vec(any::<u64>(), 100..=4980)) {
        use hft_event::{ConnRole, ConnStateKind};
        use hft_sequencer::{SnapshotSel, driver::live::RingId};
        use hft_types::{AccountId, GroupId, InstId};
        reset_script();
        let mut b = b0();
        b.seq.keyframe_max_events = u32::MAX;
        let mut params = Spb::s2(1);
        params.groups.push(gspec_eth(GroupId(0x0200_0002)));
        *b.params.iter_mut().find(|p| p.strategy == S2).unwrap() = params.clone();
        let mut h = b.boot_self_contained();
        h.steady();
        let first = h.mark();
        // Startup BBOs and Md Up facts in B0 used slot 0 on both venues.
        let mut used = BTreeSet::from([(A.0, 0), (B.0, 0)]);
        let mut i = 0;
        let mut restarted = false;
        while i < words.len() {
            let word = words[i];
            // Every example contains Up, Down, Notice and SetParams, before random mixing.
            let action = if i < 4 { i } else { (word % 7) as usize };
            match action {
                0 | 1 => {
                    let v = if word & 8 == 0 { A } else { B };
                    let k = ((word >> 8) % u64::from(hft_types::MAX_BBO_CONN_SLOTS)) as u8;
                    let state = if action == 0 { ConnStateKind::Up } else { ConnStateKind::Down };
                    h.push(RingId::Rep(0), &conn(v, k, state, ConnRole::Md, InstId(0), AccountId(0), h.now()));
                    h.run();
                    used.insert((v.0, k));
                }
                2 => {
                    let (venue, inst, kind) = match (word >> 8) % 5 {
                        0 => (A, A1, 1), 1 => (B, B1, 1), 2 => (B, B2, 2),
                        3 => (A, InstId(0), 3), _ => (B, InstId(0), 3),
                    };
                    let a = if kind == 2 { 3600 } else { UTC0 + 3_600_000_000_000 };
                    let id = h.cmd_confirmed(AC1, &ControlCmd::VenueNotice {
                        venue, kind, _pad: 0, inst, a, b: UTC0 + 7_200_000_000_000,
                    });
                    h.run();
                    prop_assert_eq!(h.result_of(id).unwrap().status, 0);
                }
                3 if i + 1 < words.len() => {
                    params.version += 1;
                    params.values[F_THR] = (word >> 8) as i64 & 0xFFFF;
                    let id = h.set_params(&params);
                    h.run();
                    prop_assert_eq!(h.result_of(id).unwrap().status, 0);
                    // ConfigBlob and SetParams are two actual Inputs.
                    prop_assert_eq!(h.s.instances().get(S2).unwrap().cur.as_ref().unwrap().version, params.version);
                    i += 1;
                }
                4 | 5 => {
                    let inst = if action == 4 { A1 } else { B1 };
                    h.quote(inst, "50000", "50002");
                    h.run();
                    used.insert((inst_venue(inst).0, 0));
                }
                _ => h.mark_md(if word & 8 == 0 { A1 } else { B1 }, "50000"),
            }
            i += 1;
            if !restarted && i >= words.len() / 2 {
                assert_snapshot_rows(&h.keyframe(), &used);
                h = h.crash_restart(plan(SnapshotSel::Latest, true, 1, 0), Rerun::default()).unwrap();
                h.run(); // Actual new EngineClock clears the previous run's used slots.
                used.clear();
                assert_snapshot_rows(&h.keyframe(), &used);
                restarted = true;
            }
        }
        assert_snapshot_rows(&h.keyframe(), &used);
        let inputs = h.j().records().skip(first).filter(|r| r.role() == Role::Input).count();
        prop_assert!((100..=5000).contains(&inputs), "actual inputs={inputs}");
    }
}

#[test]
fn seq_255_venue_notice_expiry() {
    let mut h = steady();
    let t1000 = UTC0 + 9 * 3_600_000_000_000 + 30 * 60_000_000_000;
    let t1030 = t1000 + 30 * 60_000_000_000;
    h.cmd_confirmed(
        AC1,
        &ControlCmd::VenueNotice {
            venue: B,
            kind: 3,
            _pad: 0,
            inst: hft_types::InstId(0),
            a: t1000,
            b: t1030,
        },
    );
    h.run();
    let o1 = on_demand(&mut h);
    assert!(snap(&o1).1.notices.iter().any(|r| r.kind == 3 && r.venue == B));
    h.at(T0 + (t1030 - UTC0) as u64 + 60_000_000_000);
    let o2 = on_demand(&mut h);
    assert!(!snap(&o2).1.notices.iter().any(|r| r.kind == 3 && r.venue == B));
}
