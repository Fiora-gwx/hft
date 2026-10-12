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
use zerocopy::FromBytes;

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
            1, 13, 22, 2, 3, 12, 4, 14, 17, 18, 23, 5, 8, 9, 10, 15, 16, 19, 20, 24, 25, 26, 27, 6, 7, 21
        ]
    );
    assert_eq!(snap(&o).0.n_sections, 26);
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
