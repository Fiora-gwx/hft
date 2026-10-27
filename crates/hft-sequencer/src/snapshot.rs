//! Snapshots (README "快照"): section write order, this crate's sections (5, 8, 9, 10, 15, 16, 19,
//! 20, 24, 25, 26, 27), state/out hashes and header back-fill, keyframes written in one go, segment
//! rolls with their boot blocks, L2 keyframes.

use hft_event::{
    BlobHdr, ClockStatus, CmdScope, EventKind, HostProfile, L2SnapHdr, Level, RecordFlags, SectionHdr,
    SnapshotHdr,
};
use hft_journal::{RollErr, RollReason};
use hft_strategy_sdk::{SnapshotReader, SnapshotWriter, StrategySet};
use hft_types::{AccountId, GroupId, InstId, StrategyId, Tsc, VenueId};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

use crate::Driver;
use crate::clock::{EngineClockRow, SeqDayRow, UtcLineRow};
use crate::commands::{
    ACTIVE_CONFIG_VERSION, ActiveConfigRow, CMD_DEDUP_VERSION, CmdDedupHdr, FILL_SEQ_VERSION, FillSeqRow,
    RECON_PENDING_VERSION, RECON_VIEW_DIFFS, RECON_VIEW_REPORTS, RECON_VIEW_VERSION, ReconDiff, ReconDiffRow,
    ReconPendingHdr, ReconView, ReconViewRow, SECTION_ACTIVE_CONFIG, SECTION_CMD_DEDUP, SECTION_FILL_SEQ,
    SECTION_RECON_PENDING, SECTION_RECON_VIEW, SECTION_SLOT_BINDINGS, SECTION_VENUE_NOTICES,
    SLOT_BINDINGS_VERSION, VENUE_NOTICES_VERSION,
};
use crate::engine::Engine;
use crate::journal::{BootSink, JournalSink};
use crate::market::{
    CONN_LIVE_VERSION, ConnLiveRow, MARKET_LAST_VERSION, SECTION_CONN_LIVE, SECTION_MARKET_LAST,
};
use crate::strategies::Instances;
use crate::timers::{SECTION_TIMERS, TIMERS_VERSION};
use crate::types::FatalReason;

pub const PURPOSE_BOOT: u8 = 1;
pub const PURPOSE_KEYFRAME: u8 = 2;
pub const PURPOSE_ON_DEMAND: u8 = 3;
pub const PURPOSE_RECOVERY: u8 = 4;
pub const PURPOSE_DRAIN: u8 = 5;

pub const SECTION_STRATEGY: u16 = 5;
pub const STRATEGY_VERSION: u16 = 2;
pub const SECTION_CLOCK_CAL: u16 = 16;
pub const CLOCK_CAL_VERSION: u16 = 1;
pub const SECTION_SEQ_DAY: u16 = 20;
pub const SEQ_DAY_VERSION: u16 = 1;
pub const SECTION_ENGINE_CLOCK: u16 = 25;
pub const ENGINE_CLOCK_VERSION: u16 = 1;
pub const SECTION_UTC_LINE: u16 = 32;
pub const UTC_LINE_VERSION: u16 = 1;
pub const SECTION_STRATEGY_TIMERS: u16 = 27;
pub const STRATEGY_TIMERS_VERSION: u16 = 1;

/// Section 27 instance header (16 B), followed by `n_timers` registration rows.
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, FromBytes, IntoBytes, Immutable, KnownLayout)]
pub struct StrategyTimersHdr {
    pub strategy: StrategyId,
    pub _pad: [u8; 3],
    pub n_timers: u32,
    /// Next SDK identity, including when no registrations remain. `u64::MAX` means exhausted.
    pub next_id: u64,
}
const _: () = assert!(core::mem::size_of::<StrategyTimersHdr>() == 16);
const _: () = assert!(core::mem::offset_of!(StrategyTimersHdr, next_id) == 8);

/// Section 27 registration (24 B). Its deadline is the same registered TSC as section 8;
/// restore cross-checks the full multiset before shifting both copies at recovery end.
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, FromBytes, IntoBytes, Immutable, KnownLayout)]
pub struct StrategyTimerRow {
    pub id: u64,
    pub deadline_tsc: u64,
    pub tag: u32,
    pub group: GroupId,
}
const _: () = assert!(core::mem::size_of::<StrategyTimerRow>() == 24);
const _: () = assert!(core::mem::offset_of!(StrategyTimerRow, deadline_tsc) == 8);
const _: () = assert!(core::mem::offset_of!(StrategyTimerRow, tag) == 16);
const _: () = assert!(core::mem::offset_of!(StrategyTimerRow, group) == 20);

/// Section 5 row header (24 B).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, FromBytes, IntoBytes, Immutable, KnownLayout)]
pub struct StrategyRowHdr {
    pub strategy: StrategyId,
    pub account: AccountId,
    pub kind_id: u16,
    pub schema_ver: u16,
    pub desc_version: u16,
    pub params_version: u64,
    pub len: u32,
    pub n_groups: u16,
    pub _pad: u16,
}
const _: () = assert!(core::mem::size_of::<StrategyRowHdr>() == 24);
const _: () = assert!(core::mem::offset_of!(StrategyRowHdr, kind_id) == 2);
const _: () = assert!(core::mem::offset_of!(StrategyRowHdr, desc_version) == 6);
const _: () = assert!(core::mem::offset_of!(StrategyRowHdr, params_version) == 8);
const _: () = assert!(core::mem::offset_of!(StrategyRowHdr, len) == 16);
const _: () = assert!(core::mem::offset_of!(StrategyRowHdr, n_groups) == 20);

/// Section 5 group row (16 B).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, FromBytes, IntoBytes, Immutable, KnownLayout)]
pub struct GroupSeqRow {
    pub group: GroupId,
    pub _pad: u32,
    pub spec_seq: u64,
}
const _: () = assert!(core::mem::size_of::<GroupSeqRow>() == 16);
const _: () = assert!(core::mem::offset_of!(GroupSeqRow, spec_seq) == 8);

/// Section 16 row (40 B; not in `state_hash`).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, FromBytes, IntoBytes, Immutable, KnownLayout)]
pub struct ClockCalRow {
    pub tsc: Tsc,
    pub realtime_ns: i64,
    pub phc_ns: i64,
    pub tsc_hz_milli: u64,
    pub from_seq: u64,
}
const _: () = assert!(core::mem::size_of::<ClockCalRow>() == 40);
const _: () = assert!(core::mem::offset_of!(ClockCalRow, tsc_hz_milli) == 24);
const _: () = assert!(core::mem::offset_of!(ClockCalRow, from_seq) == 32);

impl ClockCalRow {
    pub(crate) fn recorded(c: ClockStatus, from_seq: u64) -> Self {
        Self {
            tsc: c.tsc,
            realtime_ns: c.realtime_ns,
            phc_ns: c.phc_ns,
            tsc_hz_milli: c.tsc_hz_milli,
            from_seq,
        }
    }
}

/// Size limit of one strategy's fast-part snapshot.
pub const STRATEGY_SNAPSHOT_MAX: usize = 256 * 1024;

fn w_seq_sections<S: StrategySet>(
    e: &Engine,
    inst: &Instances<S>,
    w: &mut SnapshotWriter,
    sw: &mut SnapshotWriter,
) {
    // 5 StrategyState.
    w.begin_section(SECTION_STRATEGY, STRATEGY_VERSION);
    let mut order: Vec<usize> = (0..inst.slots.len()).collect();
    order.sort_by_key(|i| inst.slots.get(*i).map_or(0, |s| s.id.0));
    for &ix in &order {
        let Some(s) = inst.slots.get(ix) else { continue };
        let (Some(st), Some(p)) = (s.inst.as_ref(), s.cur.as_ref()) else {
            continue;
        };
        sw.clear();
        S::snapshot(st, sw);
        let bytes = sw.as_bytes();
        let desc = S::kinds().iter().find(|d| d.kind_id == s.kind_id);
        let hdr = StrategyRowHdr {
            strategy: s.id,
            account: s.account,
            kind_id: s.kind_id,
            schema_ver: desc.map_or(0, |d| d.schema.version),
            desc_version: desc.map_or(0, |d| d.version),
            params_version: p.version,
            len: bytes.len() as u32,
            n_groups: s.spec_seq.len() as u16,
            _pad: 0,
        };
        w.put_row(&hdr);
        for (g, ss) in s.spec_seq.iter() {
            w.put_bytes(
                GroupSeqRow {
                    group: *g,
                    _pad: 0,
                    spec_seq: *ss,
                }
                .as_bytes(),
            );
        }
        w.put_bytes(bytes);
        let pad = bytes.len().next_multiple_of(8) - bytes.len();
        w.put_bytes([0u8; 8].get(..pad).unwrap_or(&[]));
    }
    w.end_section();
    // 8 Timers.
    w.begin_section(SECTION_TIMERS, TIMERS_VERSION);
    for t in e.heap.sorted() {
        w.put_row(&t.row());
    }
    w.end_section();
    // 9 ActiveConfig.
    w.begin_section(SECTION_ACTIVE_CONFIG, ACTIVE_CONFIG_VERSION);
    for r in active_rows(e, inst) {
        w.put_row(&r);
    }
    w.end_section();
    // 10 MarketLast.
    w.begin_section(SECTION_MARKET_LAST, MARKET_LAST_VERSION);
    for (ix, r) in e.market.rows.iter().enumerate() {
        if ix == 0 || r.is_empty() {
            continue;
        }
        let mut row = *r;
        row.inst = InstId(ix as u32);
        w.put_row(&row);
    }
    w.end_section();
    // 15 CmdDedup.
    w.begin_section(SECTION_CMD_DEDUP, CMD_DEDUP_VERSION);
    w.put_bytes(
        CmdDedupHdr {
            n_dedup: e.cmd.dedup.len() as u32,
            n_preview: e.cmd.previews.len() as u32,
        }
        .as_bytes(),
    );
    for r in e.cmd.dedup.iter() {
        w.put_row(r);
    }
    for r in e.cmd.previews.iter() {
        w.put_row(r);
    }
    w.end_section();
    // 16 ClockCal.
    w.begin_section(SECTION_CLOCK_CAL, CLOCK_CAL_VERSION);
    w.put_row(&e.rec.clock_cal);
    w.end_section();
    // 19 ReconPending.
    w.begin_section(SECTION_RECON_PENDING, RECON_PENDING_VERSION);
    for g in e.cmd.recon.iter() {
        w.put_row(&ReconPendingHdr {
            report_id: g.report_id,
            account: g.account,
            venue: g.venue,
            n_stored: g.stored.len() as u16,
            count: g.count,
            received: g.received,
        });
        for r in g.stored.iter() {
            w.put_bytes(r.as_bytes());
        }
    }
    w.end_section();
    // 20 SeqDay.
    w.begin_section(SECTION_SEQ_DAY, SEQ_DAY_VERSION);
    w.put_row(&SeqDayRow {
        last_day_rolled: e.last_day_rolled,
        _pad: 0,
    });
    w.end_section();
    // 24 VenueNotices (expired ones are dropped at write time).
    w.begin_section(SECTION_VENUE_NOTICES, VENUE_NOTICES_VERSION);
    for r in e.cmd.notices.iter() {
        if r.kind == 3 && e.now.utc_ns > r.b {
            continue;
        }
        w.put_row(r);
    }
    w.end_section();
    // 25 EngineClock.
    w.begin_section(SECTION_ENGINE_CLOCK, ENGINE_CLOCK_VERSION);
    w.put_row(&e.clock.row);
    w.end_section();
    // 32 UtcLine: only once a UtcAnchor was applied in this run (absent = segment pair).
    if e.clock.utc().is_set() {
        w.begin_section(SECTION_UTC_LINE, UTC_LINE_VERSION);
        w.put_row(e.clock.utc());
        w.end_section();
    }
    // 26 ConnLive.
    w.begin_section(SECTION_CONN_LIVE, CONN_LIVE_VERSION);
    for (v, slots) in e.market.conns.iter().enumerate() {
        for (k, c) in slots.iter().enumerate() {
            if !c.used {
                continue;
            }
            let flags = u8::from(c.up) | (u8::from(c.resync) << 1);
            w.put_row(&ConnLiveRow {
                venue: VenueId(v as u8),
                slot: k as u8,
                flags,
                _pad: [0; 5],
                live_tsc: Tsc(c.live_tsc),
            });
        }
    }
    w.end_section();
    // 27 StrategyTimers. Every bound instance has a counter, even before its first params
    // or after its final timer fires. Keep SDK identities independent of heap ordering.
    w.begin_section(SECTION_STRATEGY_TIMERS, STRATEGY_TIMERS_VERSION);
    for ix in order {
        let Some(s) = inst.slots.get(ix) else { continue };
        w.put_row(&StrategyTimersHdr {
            strategy: s.id,
            _pad: [0; 3],
            n_timers: s.timers.len() as u32,
            next_id: s.bufs.next_timer_id(),
        });
        for t in &s.timers {
            w.put_bytes(
                StrategyTimerRow {
                    id: t.id.0,
                    deadline_tsc: t.at.0,
                    tag: t.tag,
                    group: t.group,
                }
                .as_bytes(),
            );
        }
    }
    w.end_section();
    // 28 FillSeq: only when some (account, inst) has a fill, so fill-free snapshots keep
    // their bytes; an absent section restores as an empty table.
    let mut rows = e.cmd.fill_seq.rows().peekable();
    if rows.peek().is_some() {
        w.begin_section(SECTION_FILL_SEQ, FILL_SEQ_VERSION);
        for r in rows {
            w.put_row(&r);
        }
        w.end_section();
    }
    // 29 ReconView: only when some report view is retained (absent = empty table).
    if !e.cmd.recon_views.is_empty() {
        w.begin_section(SECTION_RECON_VIEW, RECON_VIEW_VERSION);
        for v in e.cmd.recon_views.views() {
            let diffs: Vec<&ReconDiff> = e.cmd.recon_views.diffs_of(v.report_id).collect();
            w.put_row(&ReconViewRow {
                report_id: v.report_id,
                view_seq: v.view_seq,
                n_diffs: diffs.len() as u32,
                flags: u32::from(v.overflow),
            });
            for d in diffs {
                w.put_bytes(
                    ReconDiffRow {
                        account: d.account,
                        _pad: [0; 3],
                        inst: d.inst,
                        diff: d.diff,
                    }
                    .as_bytes(),
                );
            }
        }
        w.end_section();
    }
    if e.cmd.conns.physical {
        w.begin_section(crate::commands::SECTION_ORDER_CONNS, 1);
        for row in e.cmd.conns.snapshot_rows() {
            w.put_row(&row);
        }
        w.end_section();
    }
    // 31 SlotBindings (written only when there are any, so snapshots of venues without run-time
    // binding are unchanged).
    if !e.cmd.bindings.is_empty() {
        w.begin_section(SECTION_SLOT_BINDINGS, SLOT_BINDINGS_VERSION);
        for r in e.cmd.bindings.iter() {
            w.put_row(r);
        }
        w.end_section();
    }
}

fn active_rows<S: StrategySet>(e: &Engine, inst: &Instances<S>) -> Vec<ActiveConfigRow> {
    let none = CmdScope {
        strategy: StrategyId(0),
        venue: VenueId(0),
        account: AccountId(0),
        _pad: 0,
        group: GroupId(0),
        inst: InstId(0),
        _pad2: 0,
    };
    let mut v = vec![
        ActiveConfigRow {
            blob: e.cmd.active.pending_refdata.unwrap_or(e.cmd.active.refdata),
            scope: none,
        },
        ActiveConfigRow {
            blob: e.cmd.active.engine,
            scope: none,
        },
    ];
    for s in inst.slots.iter() {
        if s.cur.is_some() {
            let mut sc = none;
            sc.strategy = s.id;
            sc.account = s.account;
            v.push(ActiveConfigRow {
                blob: s.blob,
                scope: sc,
            });
        }
    }
    v.push(ActiveConfigRow {
        blob: e.cmd.active.risk,
        scope: none,
    });
    v
}

/// Writes every component's sections in the fixed order into `w`.
pub(crate) fn write_sections<S: StrategySet>(e: &Engine, inst: &Instances<S>, w: &mut SnapshotWriter) {
    let mut sw = SnapshotWriter::new(STRATEGY_SNAPSHOT_MAX);
    write_sections_with_scratch(e, inst, w, &mut sw);
}

fn write_sections_with_scratch<S: StrategySet>(
    e: &Engine,
    inst: &Instances<S>,
    w: &mut SnapshotWriter,
    sw: &mut SnapshotWriter,
) {
    e.ledger.snapshot(w);
    e.oms.snapshot(w);
    e.risk.snapshot(w);
    w_seq_sections(e, inst, w, sw);
    e.books.snapshot(w);
    e.exec.snapshot(w);
    w.end_section();
}

/// `state_hash`: xxh3-128 of every section (header + body) in write order, sections 16, 28 and
/// 29 excluded. Sections 28 (latest fill seqs) and 29 (retained reconciliation views) are derived
/// guard indexes: replay still compares their bytes and the restore self-check round-trips them,
/// but they do not move the conformance digest.
pub fn state_hash(sections: &[u8]) -> [u8; 16] {
    let mut h = xxhash_rust::xxh3::Xxh3::new();
    let mut off = 0usize;
    while let Some(hb) = sections.get(off..off + 16) {
        let Some(sh) = SectionHdr::read_from_bytes(hb).ok() else {
            break;
        };
        let end = off + 16 + sh.len as usize;
        let Some(all) = sections.get(off..end) else { break };
        if !matches!(
            sh.section,
            SECTION_CLOCK_CAL | SECTION_FILL_SEQ | SECTION_RECON_VIEW
        ) {
            h.update(all);
        }
        off = end;
    }
    h.digest128().to_le_bytes()
}

/// Number of sections in a section blob.
pub fn count_sections(sections: &[u8]) -> u32 {
    let mut n = 0;
    let mut off = 0usize;
    while let Some(hb) = sections.get(off..off + 16) {
        let Some(sh) = SectionHdr::read_from_bytes(hb).ok() else {
            break;
        };
        off += 16 + sh.len as usize;
        n += 1;
    }
    n
}

/// Builds the snapshot body (header + sections) for `purpose` at the current Input.
pub(crate) fn build_snapshot<S: StrategySet>(
    e: &mut Engine,
    inst: &Instances<S>,
    purpose: u8,
    snap_tsc: Tsc,
) -> Vec<u8> {
    // A fresh 64 MiB allocation here faults and locks pages under MCL_FUTURE. Keep the
    // same atomic snapshot contract, but pay that allocation cost only during startup.
    let mut w = std::mem::replace(&mut e.snapshot_writer, SnapshotWriter::new(0));
    let mut sw = std::mem::replace(&mut e.strategy_snapshot_writer, SnapshotWriter::new(0));
    w.clear();
    write_sections_with_scratch(e, inst, &mut w, &mut sw);
    // The gate-input caches restart from the snapshotted state, as in a replay from here.
    crate::recover::derive_gate_caches(e);
    let sections = w.as_bytes();
    let n_books = if purpose == PURPOSE_BOOT || purpose == PURPOSE_RECOVERY {
        live_books(e).count() as u32
    } else {
        0
    };
    let hdr = SnapshotHdr {
        purpose,
        _pad: [0; 3],
        n_sections: count_sections(sections),
        at_seq: e.cur_seq,
        state_hash: if purpose == PURPOSE_KEYFRAME {
            [0; 16]
        } else {
            state_hash(sections)
        },
        n_books,
        _pad2: 0,
        out_hash: e.out_hash.digest(),
        snap_tsc: snap_tsc.0,
        boot_id: [0; 16],
        snap_utc_ns: e.now.utc_ns,
        // Recovery has no new Input yet; without a replayed tail `now` is still empty.
        // The committed stop is also the next EngineClock's base, including a second
        // crash after this snapshot but before that EngineClock is consumed.
        engine_ns: if purpose == PURPOSE_RECOVERY {
            e.last_ens
        } else {
            e.now.engine_ns
        },
    };
    let mut body = Vec::with_capacity(96 + sections.len());
    body.extend_from_slice(hdr.as_bytes());
    body.extend_from_slice(sections);
    e.snapshot_writer = w;
    e.strategy_snapshot_writer = sw;
    body
}

/// Insts with a Live L2 book (ascending).
pub(crate) fn live_books(e: &Engine) -> impl Iterator<Item = InstId> + Clone + '_ {
    e.l2_insts.iter().copied().filter(|i| {
        e.books
            .l2(*i)
            .is_some_and(|b| b.state() == hft_book::BookState::Live)
    })
}

/// A snapshot written within the current event (keyframe, on demand, Drain).
pub(crate) fn write_snapshot<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    purpose: u8,
) -> Result<(), FatalReason> {
    let t = drv.measure();
    let body = build_snapshot(e, inst, purpose, t);
    e.trace.rec("snapshot", u64::from(purpose), body.len() as u64);
    e.out.push_bytes(EventKind::STATE_SNAPSHOT, RecordFlags(0), &body);
    after_snapshot(e, purpose);
    Ok(())
}

pub(crate) fn after_snapshot(e: &mut Engine, purpose: u8) {
    e.market.set_rx_from(e.cur_seq);
    if purpose == PURPOSE_KEYFRAME {
        e.inputs_since_keyframe = 0;
        e.last_keyframe_tsc = e.now.tsc.0;
    }
}

/// The Output interval ends when the snapshot is appended, after any preceding buffered
/// Outputs. SnapshotHdr's layout fixes out_hash at bytes 40..56.
pub(crate) fn set_out_hash(body: &mut [u8], hash: [u8; 16]) {
    if let Some(dst) = body.get_mut(40..56) {
        dst.copy_from_slice(&hash);
    }
}

/// `L2Snapshot{origin = Keyframe}` of one Live book.
pub(crate) fn l2_keyframe(e: &mut Engine, i: InstId) {
    let Some(body) = l2_keyframe_body(e, i) else {
        return;
    };
    e.out.push_bytes(EventKind::L2_SNAPSHOT, RecordFlags(0), &body);
}

fn l2_keyframe_body(e: &Engine, i: InstId) -> Option<Vec<u8>> {
    let b = e.books.l2(i)?;
    if b.state() != hft_book::BookState::Live {
        return None;
    }
    let depth = usize::from(e.rd.inst(i).l2_depth.max(1)).saturating_mul(2).max(2);
    let mut levels = vec![
        Level {
            px: hft_types::Px(0),
            qty: hft_types::Qty(0)
        };
        depth.max(64)
    ];
    let (nb, na, _) = b.export(&mut levels);
    let mut h = <L2SnapHdr as zerocopy::FromZeros>::new_zeroed();
    h.h.venue = e.rd.inst(i).venue;
    h.h.inst = i;
    h.snap_id = b.last_id();
    h.origin = L2SnapHdr::KEYFRAME;
    h.n_bids = nb;
    h.n_asks = na;
    let n = usize::from(nb) + usize::from(na);
    let mut body = Vec::with_capacity(72 + 16 * n);
    body.extend_from_slice(h.as_bytes());
    for l in levels.iter().take(n) {
        body.extend_from_slice(l.as_bytes());
    }
    Some(body)
}

/// `Timer{Sys, Roll}`: seal, open, boot block (README "快照" 段滚动). `NextNotReady` → nothing.
pub(crate) fn roll<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    _tsc: Tsc,
) -> Result<(), FatalReason> {
    drv.journal().end_event();
    match boot_block(e, inst, drv, RollReason::Hourly, PURPOSE_BOOT) {
        Ok(()) => Ok(()),
        Err(BootBlockErr::NotReady) => Ok(()),
        Err(BootBlockErr::Append) => Err(FatalReason::JournalFull),
    }
}

/// Boot-block failures.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum BootBlockErr {
    NotReady,
    Append,
}

/// Writes a complete boot block into a freshly rolled segment.
pub(crate) fn boot_block<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    reason: RollReason,
    purpose: u8,
) -> Result<(), BootBlockErr> {
    let info = drv.roll_info();
    let rows = active_rows(e, inst);
    let refs: Vec<hft_event::BlobRef> = rows.iter().map(|r| r.blob).collect();
    let meta = crate::journal::RollMeta {
        utc_ns: info.utc_ns,
        calib: info.calib,
        tsc_hz_milli: info.tsc_hz_milli,
        configs: &refs,
    };
    let measured = drv.measure();
    let snapshot_tsc = if purpose == PURPOSE_RECOVERY {
        e.rec.recovery_now
    } else {
        measured
    };
    let host = drv.host_profile();
    let clk = drv.clock_status();
    let tsc = Tsc(info.calib.tsc.0);
    // Config blobs.
    let mut cfg: Vec<Vec<u8>> = Vec::with_capacity(rows.len());
    for r in rows.iter() {
        let content = e.cmd.blobs.get(&r.blob.sha256).unwrap_or(&[]);
        let h = BlobHdr {
            blob: r.blob,
            scope: r.scope,
            boot: 1,
            _pad: [0; 7],
        };
        let mut b = Vec::with_capacity(72 + content.len());
        b.extend_from_slice(h.as_bytes());
        b.extend_from_slice(content);
        cfg.push(b);
    }
    let books: Vec<Vec<u8>> = live_books(e).filter_map(|i| l2_keyframe_body(e, i)).collect();
    {
        let j = drv.journal();
        let mut b = match j.roll(reason, &meta) {
            Ok(b) => b,
            Err(RollErr::NextNotReady) => return Err(BootBlockErr::NotReady),
            Err(_) => return Err(BootBlockErr::Append),
        };
        b.append_var(EventKind::HOST_PROFILE, tsc, &[host.as_bytes()])
            .map_err(|_| BootBlockErr::Append)?;
        let clock_seq = b
            .append_var(EventKind::CLOCK_STATUS, tsc, &[clk.as_bytes()])
            .map_err(|_| BootBlockErr::Append)?;
        e.rec.clock_cal = ClockCalRow::recorded(clk, clock_seq.0);
        let mut snap = build_snapshot(e, inst, purpose, snapshot_tsc);
        if reason == RollReason::RecoverFromSnapshot {
            let (from, to) = e.rec.skipped_range.ok_or(BootBlockErr::Append)?;
            let n = hft_event::Note {
                code: hft_event::note_code(
                    hft_event::NoteSource::Boot,
                    crate::codes::boot_note::RECOVER_FROM_SNAPSHOT,
                ),
                level: hft_event::NoteLevel::P1,
                source: hft_event::NoteSource::Boot,
                strategy: StrategyId(0),
                _pad: 0,
                group: hft_types::GROUP_NONE,
                inst: InstId(0),
                // Operator (TailSkipped reason 3); hft-core's recovery launcher identity.
                v: [
                    e.rec.snapshot_seq,
                    from,
                    to,
                    3,
                    u64::from_le_bytes(*b"cli:syst"),
                    0,
                    0,
                    0,
                ],
            };
            b.append_var(EventKind::NOTE, tsc, &[n.as_bytes()])
                .map_err(|_| BootBlockErr::Append)?;
        }
        for c in cfg.iter() {
            b.append_var(EventKind::CONFIG_BLOB, tsc, &[c])
                .map_err(|_| BootBlockErr::Append)?;
            e.out_hash.add(EventKind::CONFIG_BLOB, RecordFlags(0), &[c]);
        }
        set_out_hash(&mut snap, e.out_hash.digest());
        b.append_var(EventKind::STATE_SNAPSHOT, tsc, &[&snap])
            .map_err(|_| BootBlockErr::Append)?;
        e.out_hash
            .add(EventKind::STATE_SNAPSHOT, RecordFlags(0), &[&snap]);
        for k in books.iter() {
            b.append_var(EventKind::L2_SNAPSHOT, tsc, &[k])
                .map_err(|_| BootBlockErr::Append)?;
            e.out_hash.add(EventKind::L2_SNAPSHOT, RecordFlags(0), &[k]);
        }
        b.finish().map_err(|_| BootBlockErr::Append)?;
    }
    e.seg = crate::journal::SegClock {
        tsc_hz_milli: info.tsc_hz_milli,
        calib_tsc: info.calib.tsc.0,
        calib_realtime_ns: info.calib.realtime_ns,
    };
    after_snapshot(e, purpose);
    e.trace
        .rec("boot_block", u64::from(reason as u8), u64::from(purpose));
    Ok(())
}

/// The boot block of `boot` (empty start: Startup; recovery: Recovery).
pub(crate) fn write_boot_block<D: Driver<S>, S: StrategySet>(
    seq: &mut crate::Sequencer<D, S>,
    reason: RollReason,
    purpose: u8,
) -> Result<(), BootBlockErr> {
    let crate::Sequencer { e, inst, drv, .. } = seq;
    // Outputs produced while assembling are not journaled (the boot snapshot carries the state).
    e.out.clear();
    e.notify.begin();
    let p = if purpose == 1 && reason == RollReason::Recovery {
        PURPOSE_RECOVERY
    } else {
        purpose
    };
    boot_block(e, inst, drv, reason, p)
}

/// The `HostProfile` / `ClockStatus` placeholders of drivers without a host (tests, replay).
pub fn host_profile(hz: u64) -> HostProfile {
    let mut h = <HostProfile as zerocopy::FromZeros>::new_zeroed();
    h.tsc_hz_milli = hz;
    h
}

pub fn clock_status(calib: hft_rt::CalibPair, hz: u64) -> ClockStatus {
    let mut c = <ClockStatus as zerocopy::FromZeros>::new_zeroed();
    c.tsc = calib.tsc;
    c.realtime_ns = calib.realtime_ns;
    c.phc_ns = calib.phc_ns;
    c.tsc_hz_milli = hz;
    c
}

// ------------------------------------------------------------------------------- restore

/// This crate's sections read back from a snapshot body (sections only, header stripped).
#[derive(Clone, Debug, Default)]
pub struct SeqSections {
    pub strategies: Vec<(StrategyRowHdr, Vec<GroupSeqRow>, Vec<u8>)>,
    pub timers: Vec<crate::timers::TimerRow>,
    pub active: Vec<(hft_event::BlobRef, CmdScope)>,
    pub market: Vec<crate::market::MarketLastRow>,
    pub dedup: Vec<crate::commands::CmdDedupRow>,
    pub previews: Vec<crate::commands::ViewerPreviewRow>,
    pub clock_cal: ClockCalRow,
    pub recon: Vec<(ReconPendingHdr, Vec<hft_event::ReconcileReport>)>,
    pub seq_day: SeqDayRow,
    pub notices: Vec<crate::commands::VenueNoticeRow>,
    /// Section 31 (absent = no binding).
    pub bindings: Vec<crate::commands::SlotBindingRow>,
    pub engine_clock: EngineClockRow,
    /// Section 32 (absent = no UtcAnchor applied in the snapshot's run).
    pub utc_line: UtcLineRow,
    pub conns: Vec<ConnLiveRow>,
    pub order_conns: Option<Vec<crate::commands::OrderConnsRow>>,
    /// `None` distinguishes an old snapshot with no identity history from an empty table.
    pub strategy_timers: Option<Vec<(StrategyTimersHdr, Vec<StrategyTimerRow>)>>,
    /// Section 28 (absent = no fill recorded, including snapshots written before it existed).
    pub fill_seq: Vec<FillSeqRow>,
    /// Section 29 (absent = no retained reconciliation view), oldest report first.
    pub recon_views: Vec<(ReconView, Vec<ReconDiff>)>,
}

/// Parses sections 5–27 of this crate; `Err(section)` = undecodable.
pub fn read_seq_sections(body: &[u8]) -> Result<SeqSections, u16> {
    let r = SnapshotReader::new(body);
    let mut out = SeqSections::default();
    let sec = |n: u16| r.find_section(n).map_err(|_| n);
    if let Some(s) = sec(SECTION_STRATEGY)? {
        let mut rr = s.reader();
        for _ in 0..s.count {
            let h: StrategyRowHdr = rr.get_row().map_err(|_| SECTION_STRATEGY)?;
            let mut g = Vec::with_capacity(usize::from(h.n_groups));
            for _ in 0..h.n_groups {
                g.push(rr.get_row::<GroupSeqRow>().map_err(|_| SECTION_STRATEGY)?);
            }
            let bytes = rr
                .get_bytes(h.len as usize)
                .map_err(|_| SECTION_STRATEGY)?
                .to_vec();
            let pad = (h.len as usize).next_multiple_of(8) - h.len as usize;
            rr.get_bytes(pad).map_err(|_| SECTION_STRATEGY)?;
            out.strategies.push((h, g, bytes));
        }
    }
    if let Some(s) = sec(SECTION_TIMERS)? {
        if s.body.len() < s.count as usize * 24 || (s.body.len() - s.count as usize * 24) >= 8 {
            return Err(SECTION_TIMERS);
        }
        let mut rr = s.reader();
        for _ in 0..s.count {
            out.timers.push(rr.get_row().map_err(|_| SECTION_TIMERS)?);
        }
    }
    if let Some(s) = sec(SECTION_ACTIVE_CONFIG)? {
        let mut rr = s.reader();
        for _ in 0..s.count {
            let r: ActiveConfigRow = rr.try_row().map_err(|_| SECTION_ACTIVE_CONFIG)?;
            out.active.push((r.blob, r.scope));
        }
    }
    if let Some(s) = sec(SECTION_MARKET_LAST)? {
        let rows = s.count.max(1) as usize;
        let row_size = if s.count == 0 { 112 } else { s.body.len() / rows };
        for k in 0..s.count as usize {
            let b = s
                .body
                .get(k * row_size..(k + 1) * row_size)
                .ok_or(SECTION_MARKET_LAST)?;
            let mut buf = [0u8; 112];
            let n = b.len().min(112);
            buf.get_mut(..n)
                .ok_or(SECTION_MARKET_LAST)?
                .copy_from_slice(b.get(..n).ok_or(SECTION_MARKET_LAST)?);
            let mut row =
                crate::market::MarketLastRow::read_from_bytes(&buf).map_err(|_| SECTION_MARKET_LAST)?;
            if row_size < 112 {
                row.prev_interval_s = row.interval_s;
            }
            out.market.push(row);
        }
    }
    if let Some(s) = sec(SECTION_CMD_DEDUP)? {
        let mut rr = s.reader();
        let h: CmdDedupHdr = rr.get_row().map_err(|_| SECTION_CMD_DEDUP)?;
        for _ in 0..h.n_dedup {
            out.dedup.push(rr.get_row().map_err(|_| SECTION_CMD_DEDUP)?);
        }
        for _ in 0..h.n_preview {
            out.previews.push(rr.get_row().map_err(|_| SECTION_CMD_DEDUP)?);
        }
    }
    if let Some(s) = sec(SECTION_CLOCK_CAL)? {
        out.clock_cal = s.reader().get_row().unwrap_or_default();
    }
    if let Some(s) = sec(SECTION_RECON_PENDING)? {
        let mut rr = s.reader();
        for _ in 0..s.count {
            let h: ReconPendingHdr = rr.get_row().map_err(|_| SECTION_RECON_PENDING)?;
            let mut v = Vec::new();
            for _ in 0..h.n_stored {
                v.push(rr.get_row().map_err(|_| SECTION_RECON_PENDING)?);
            }
            out.recon.push((h, v));
        }
    }
    if let Some(s) = sec(SECTION_SEQ_DAY)? {
        out.seq_day = s.reader().get_row().map_err(|_| SECTION_SEQ_DAY)?;
    }
    if let Some(s) = sec(SECTION_VENUE_NOTICES)? {
        let mut rr = s.reader();
        for _ in 0..s.count {
            out.notices.push(rr.get_row().map_err(|_| SECTION_VENUE_NOTICES)?);
        }
    }
    if let Some(s) = sec(SECTION_SLOT_BINDINGS)? {
        let mut rr = s.reader();
        for _ in 0..s.count {
            out.bindings
                .push(rr.get_row().map_err(|_| SECTION_SLOT_BINDINGS)?);
        }
    }
    if let Some(s) = sec(SECTION_ENGINE_CLOCK)? {
        out.engine_clock = s.reader().get_row().map_err(|_| SECTION_ENGINE_CLOCK)?;
    }
    if let Some(s) = sec(SECTION_UTC_LINE)? {
        if s.version != UTC_LINE_VERSION || s.count != 1 || s.body.len() != size_of::<UtcLineRow>() {
            return Err(SECTION_UTC_LINE);
        }
        let row: UtcLineRow = s.reader().get_row().map_err(|_| SECTION_UTC_LINE)?;
        if !row.is_set() || out.engine_clock.tsc_hz_milli == 0 {
            return Err(SECTION_UTC_LINE);
        }
        out.utc_line = row;
    }
    if let Some(s) = sec(SECTION_CONN_LIVE)? {
        let mut rr = s.reader();
        for _ in 0..s.count {
            out.conns.push(rr.get_row().map_err(|_| SECTION_CONN_LIVE)?);
        }
    }
    let mut scan = SnapshotReader::new(body);
    let mut timer_state = None;
    while let Some(s) = scan.next_section().map_err(|_| SECTION_STRATEGY_TIMERS)? {
        if s.section == SECTION_STRATEGY_TIMERS && timer_state.replace(s).is_some() {
            return Err(SECTION_STRATEGY_TIMERS);
        }
    }
    if let Some(s) = timer_state {
        let err = SECTION_STRATEGY_TIMERS;
        if s.version != STRATEGY_TIMERS_VERSION || s.count as usize > s.body.len() / 16 {
            return Err(err);
        }
        let mut rr = s.reader();
        let mut states = Vec::new();
        let mut prev_strategy = 0;
        for _ in 0..s.count {
            let h: StrategyTimersHdr = rr.get_row().map_err(|_| err)?;
            if h.strategy.0 <= prev_strategy
                || h.strategy.0 == u8::MAX
                || h._pad != [0; 3]
                || h.next_id == 0
                || h.n_timers as usize > rr.remaining() / 24
            {
                return Err(err);
            }
            prev_strategy = h.strategy.0;
            let mut timers = Vec::new();
            let mut prev_id = 0;
            for _ in 0..h.n_timers {
                let t: StrategyTimerRow = rr.get_row().map_err(|_| err)?;
                if t.id <= prev_id || t.id >= h.next_id {
                    return Err(err);
                }
                prev_id = t.id;
                timers.push(t);
            }
            states.push((h, timers));
        }
        if !rr.is_empty() {
            return Err(err);
        }
        out.strategy_timers = Some(states);
    }
    if let Some(s) = sec(SECTION_FILL_SEQ)? {
        let err = SECTION_FILL_SEQ;
        if s.version != FILL_SEQ_VERSION || s.body.len() != s.count as usize * 16 {
            return Err(err);
        }
        let mut rr = s.reader();
        let mut prev = (0u8, 0u32);
        for k in 0..s.count {
            let r: FillSeqRow = rr.get_row().map_err(|_| err)?;
            let key = (r.account.0, r.inst.0);
            if r._pad != [0; 3] || r.seq == 0 || r.inst.0 == 0 || (k > 0 && key <= prev) {
                return Err(err);
            }
            prev = key;
            out.fill_seq.push(r);
        }
    }
    if let Some(s) = sec(SECTION_RECON_VIEW)? {
        let err = SECTION_RECON_VIEW;
        if s.version != RECON_VIEW_VERSION || s.count as usize > RECON_VIEW_REPORTS {
            return Err(err);
        }
        let mut rr = s.reader();
        let mut total = 0usize;
        for _ in 0..s.count {
            let h: ReconViewRow = rr.get_row().map_err(|_| err)?;
            total = total.saturating_add(h.n_diffs as usize);
            if h.flags > 1
                || (h.flags == 1 && h.n_diffs != 0)
                || total > RECON_VIEW_DIFFS
                || out.recon_views.iter().any(|(v, _)| v.report_id == h.report_id)
            {
                return Err(err);
            }
            let mut diffs = Vec::with_capacity(h.n_diffs as usize);
            for _ in 0..h.n_diffs {
                let b = rr.get_bytes(16).map_err(|_| err)?;
                let d = ReconDiffRow::read_from_bytes(b).map_err(|_| err)?;
                if d._pad != [0; 3]
                    || diffs
                        .iter()
                        .any(|x: &ReconDiff| x.account == d.account && x.inst == d.inst)
                {
                    return Err(err);
                }
                diffs.push(ReconDiff {
                    report_id: h.report_id,
                    account: d.account,
                    inst: d.inst,
                    diff: d.diff,
                });
            }
            let v = ReconView {
                report_id: h.report_id,
                view_seq: h.view_seq,
                overflow: h.flags == 1,
            };
            out.recon_views.push((v, diffs));
        }
        if !rr.is_empty() {
            return Err(err);
        }
    }
    if let Some(s) = sec(crate::commands::SECTION_ORDER_CONNS)? {
        let err = crate::commands::SECTION_ORDER_CONNS;
        if s.version != 1 {
            return Err(err);
        }
        let mut rr = s.reader();
        let mut rows = Vec::new();
        let mut last = None;
        for _ in 0..s.count {
            let row: crate::commands::OrderConnsRow = rr.get_row().map_err(|_| err)?;
            let key = (row.account.0, row.venue.0);
            let orders: u32 = row.orders.iter().map(|x| x.count_ones()).sum();
            let users = row.users.iter().any(|x| *x != 0);
            if row._pad != [0; 5]
                || row.flags & !15 != 0
                || row.account.0 == 0
                || row.venue.0 >= 64
                || last.is_some_and(|v| v >= key)
                || (row.flags & 1 != 0) != (orders > 0)
                || (row.flags & 2 != 0) != (orders > 1)
                || (row.flags & 8 != 0) != users
                || (users && row.flags & 4 == 0)
            {
                return Err(err);
            }
            last = Some(key);
            rows.push(row);
        }
        if !rr.is_empty() {
            return Err(err);
        }
        out.order_conns = Some(rows);
    }
    Ok(out)
}

impl Default for StrategyRowHdr {
    fn default() -> StrategyRowHdr {
        <StrategyRowHdr as zerocopy::FromZeros>::new_zeroed()
    }
}

impl Default for GroupSeqRow {
    fn default() -> GroupSeqRow {
        <GroupSeqRow as zerocopy::FromZeros>::new_zeroed()
    }
}

impl Default for ClockCalRow {
    fn default() -> ClockCalRow {
        <ClockCalRow as zerocopy::FromZeros>::new_zeroed()
    }
}
