//! Snapshots (README "快照"): section write order, this crate's sections (5, 8, 9, 10, 15, 16, 19,
//! 20, 24, 25, 26), state/out hashes and header back-fill, keyframes written in one go, segment
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
use crate::clock::{EngineClockRow, SeqDayRow};
use crate::commands::{
    ACTIVE_CONFIG_VERSION, CMD_DEDUP_VERSION, ActiveConfigRow, CmdDedupHdr, RECON_PENDING_VERSION, ReconPendingHdr,
    SECTION_ACTIVE_CONFIG, SECTION_CMD_DEDUP, SECTION_RECON_PENDING, SECTION_VENUE_NOTICES, VENUE_NOTICES_VERSION,
};
use crate::engine::Engine;
use crate::journal::{BootSink, JournalSink, role_of};
use crate::market::{CONN_LIVE_VERSION, ConnLiveRow, MARKET_LAST_VERSION, SECTION_CONN_LIVE, SECTION_MARKET_LAST};
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

/// Size limit of one strategy's fast-part snapshot.
pub const STRATEGY_SNAPSHOT_MAX: usize = 256 * 1024;

fn w_seq_sections<S: StrategySet>(e: &Engine, inst: &Instances<S>, w: &mut SnapshotWriter) {
    // 5 StrategyState.
    w.begin_section(SECTION_STRATEGY, STRATEGY_VERSION);
    let mut sw = SnapshotWriter::new(STRATEGY_SNAPSHOT_MAX);
    let mut order: Vec<usize> = (0..inst.slots.len()).collect();
    order.sort_by_key(|i| inst.slots.get(*i).map_or(0, |s| s.id.0));
    for ix in order {
        let Some(s) = inst.slots.get(ix) else { continue };
        let (Some(st), Some(p)) = (s.inst.as_ref(), s.cur.as_ref()) else {
            continue;
        };
        sw.clear();
        S::snapshot(st, &mut sw);
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
            v.push(ActiveConfigRow { blob: s.blob, scope: sc });
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
    e.ledger.snapshot(w);
    e.oms.snapshot(w);
    e.risk.snapshot(w);
    w_seq_sections(e, inst, w);
    e.books.snapshot(w);
    e.exec.snapshot(w);
    w.end_section();
}

/// `state_hash`: xxh3-128 of every section (header + body) in write order, section 16 excluded.
pub fn state_hash(sections: &[u8]) -> [u8; 16] {
    let mut h = xxhash_rust::xxh3::Xxh3::new();
    let mut off = 0usize;
    while let Some(hb) = sections.get(off..off + 16) {
        let Some(sh) = SectionHdr::read_from_bytes(hb).ok() else { break };
        let end = off + 16 + sh.len as usize;
        let Some(all) = sections.get(off..end) else { break };
        if sh.section != SECTION_CLOCK_CAL {
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
        let Some(sh) = SectionHdr::read_from_bytes(hb).ok() else { break };
        off += 16 + sh.len as usize;
        n += 1;
    }
    n
}

/// Builds the snapshot body (header + sections) for `purpose` at the current Input.
pub(crate) fn build_snapshot<S: StrategySet>(e: &mut Engine, inst: &Instances<S>, purpose: u8, snap_tsc: Tsc) -> Vec<u8> {
    let mut w = SnapshotWriter::new(64 << 20);
    write_sections(e, inst, &mut w);
    // The gate-input caches restart from the snapshotted state, as in a replay from here.
    crate::recover::derive_gate_caches(e);
    let sections = w.as_bytes();
    let n_books = if purpose == PURPOSE_BOOT || purpose == PURPOSE_RECOVERY {
        live_books(e).len() as u32
    } else {
        0
    };
    let hdr = SnapshotHdr {
        purpose,
        _pad: [0; 3],
        n_sections: count_sections(sections),
        at_seq: e.cur_seq,
        state_hash: if purpose == PURPOSE_KEYFRAME { [0; 16] } else { state_hash(sections) },
        n_books,
        _pad2: 0,
        out_hash: e.out_hash.digest(),
        snap_tsc: snap_tsc.0,
        boot_id: [0; 16],
        snap_utc_ns: e.now.utc_ns,
        engine_ns: e.now.engine_ns,
    };
    let mut body = Vec::with_capacity(96 + sections.len());
    body.extend_from_slice(hdr.as_bytes());
    body.extend_from_slice(sections);
    body
}

/// Insts with a Live L2 book (ascending).
pub(crate) fn live_books(e: &Engine) -> Vec<InstId> {
    (1..=e.rd.inst_count())
        .map(|i| InstId(i as u32))
        .filter(|i| e.books.l2(*i).is_some_and(|b| b.state() == hft_book::BookState::Live))
        .collect()
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
    e.out_hash.reset();
    if purpose == PURPOSE_KEYFRAME {
        e.inputs_since_keyframe = 0;
        e.last_keyframe_tsc = e.now.tsc.0;
    }
}

/// `L2Snapshot{origin = Keyframe}` of one Live book.
pub(crate) fn l2_keyframe(e: &mut Engine, i: InstId) {
    let Some(body) = l2_keyframe_body(e, i) else { return };
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
    let snap = {
        let t = drv.measure();
        build_snapshot(e, inst, purpose, if purpose == PURPOSE_RECOVERY { e.rec.recovery_now } else { t })
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
    let books: Vec<Vec<u8>> = live_books(e).iter().filter_map(|i| l2_keyframe_body(e, *i)).collect();
    let mut hashes: Vec<(EventKind, Vec<u8>)> = Vec::new();
    {
        let j = drv.journal();
        let mut b = match j.roll(reason, &meta) {
            Ok(b) => b,
            Err(RollErr::NextNotReady) => return Err(BootBlockErr::NotReady),
            Err(_) => return Err(BootBlockErr::Append),
        };
        b.append_var(EventKind::HOST_PROFILE, tsc, &[host.as_bytes()]).map_err(|_| BootBlockErr::Append)?;
        b.append_var(EventKind::CLOCK_STATUS, tsc, &[clk.as_bytes()]).map_err(|_| BootBlockErr::Append)?;
        for c in cfg.iter() {
            b.append_var(EventKind::CONFIG_BLOB, tsc, &[c]).map_err(|_| BootBlockErr::Append)?;
            hashes.push((EventKind::CONFIG_BLOB, c.clone()));
        }
        b.append_var(EventKind::STATE_SNAPSHOT, tsc, &[&snap]).map_err(|_| BootBlockErr::Append)?;
        for k in books.iter() {
            b.append_var(EventKind::L2_SNAPSHOT, tsc, &[k]).map_err(|_| BootBlockErr::Append)?;
        }
        b.finish().map_err(|_| BootBlockErr::Append)?;
    }
    for (k, body) in hashes {
        if role_of(k, &body) == hft_event::Role::Output {
            e.out_hash.add(k, RecordFlags(0), &[&body]);
        }
    }
    e.seg = crate::journal::SegClock {
        tsc_hz_milli: info.tsc_hz_milli,
        calib_tsc: info.calib.tsc.0,
        calib_realtime_ns: info.calib.realtime_ns,
    };
    after_snapshot(e, purpose);
    e.trace.rec("boot_block", u64::from(reason as u8), u64::from(purpose));
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
    let p = if purpose == 1 && reason == RollReason::Recovery { PURPOSE_RECOVERY } else { purpose };
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
    pub engine_clock: EngineClockRow,
    pub conns: Vec<ConnLiveRow>,
}

/// Parses sections 5–26 of this crate; `Err(section)` = undecodable.
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
            let bytes = rr.get_bytes(h.len as usize).map_err(|_| SECTION_STRATEGY)?.to_vec();
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
            let b = s.body.get(k * row_size..(k + 1) * row_size).ok_or(SECTION_MARKET_LAST)?;
            let mut buf = [0u8; 112];
            let n = b.len().min(112);
            buf.get_mut(..n).ok_or(SECTION_MARKET_LAST)?.copy_from_slice(b.get(..n).ok_or(SECTION_MARKET_LAST)?);
            let mut row = crate::market::MarketLastRow::read_from_bytes(&buf).map_err(|_| SECTION_MARKET_LAST)?;
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
    if let Some(s) = sec(SECTION_ENGINE_CLOCK)? {
        out.engine_clock = s.reader().get_row().map_err(|_| SECTION_ENGINE_CLOCK)?;
    }
    if let Some(s) = sec(SECTION_CONN_LIVE)? {
        let mut rr = s.reader();
        for _ in 0..s.count {
            out.conns.push(rr.get_row().map_err(|_| SECTION_CONN_LIVE)?);
        }
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
