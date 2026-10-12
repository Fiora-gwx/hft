//! Starting from a logged snapshot (README "冷启动恢复", "从快照起步", "恢复时决策不一致",
//! "毒丸输入隔离", "版本变化与回滚"): the same restore for the recover, verify and mirror
//! drivers; the recover path additionally replays the tail with the replay driver, ends the
//! recovery (`recovery_end`, timer shift, mismatch handling, Recovery caps, new epoch) and writes
//! the Recovery boot block through the live driver.

use hft_event::{BlobHdr, BlobKind, BlobRef, EventKind, NoteLevel, RecordFlags, Role, SnapshotHdr};
use hft_journal::RollReason;
use hft_ledger::Ledger;
use hft_oms::Oms;
use hft_refdata::RefData;
use hft_risk::ModeCfg;
use hft_risk::Risk;
use hft_risk::mode::{ModeInput, RecoveryFlags};
use hft_rt::Heartbeat;
use hft_strategy_sdk::{RefView, SnapshotReader, StrategyInstance, StrategySet};
use hft_types::{AccountId, CapSource, GroupId, InstId, Tsc, VenueId};
use zerocopy::{FromBytes, IntoBytes};

use crate::driver::replay::{Log, Mismatch, ReplayDriver, ReplayMode};
use crate::driver::{Driver, DriverMode};
use crate::engine::{Engine, EngineSetup, account_count, exec_bounds};
use crate::journal::{JournalSink, MemRecord};
use crate::parsed::parse_params;
use crate::snapshot::{ClockCalRow, PURPOSE_DRAIN, PURPOSE_RECOVERY, SeqSections, read_seq_sections};
use crate::strategies::Instances;
use crate::types::{BootError, CapKind, ConfigErr, InputSlot, JournalErr, RecoverPlan, SnapshotSel};
use crate::{Sequencer, codes};

/// Recovery bookkeeping kept in the engine.
#[derive(Clone, Debug)]
pub struct RecoverState {
    pub clock_cal: ClockCalRow,
    /// `now` of `recovery_end` (Recovery snapshot `snap_tsc`).
    pub recovery_now: Tsc,
    /// `InputHash` of the truncated half event (reported to hft-core).
    pub poison: Option<u64>,
    /// Output mismatches of the tail replay.
    pub mismatches: Vec<Mismatch>,
    /// Seq of the start snapshot record and of the last replayed record.
    pub snapshot_seq: u64,
    pub tail_end_seq: u64,
    /// Physical tail range deliberately skipped by FROM_SNAPSHOT, or (0, 0) when empty.
    /// Boot-only evidence; not persisted in the state snapshot.
    pub skipped_range: Option<(u64, u64)>,
    /// Inputs handed during the tail replay.
    pub replayed: u64,
    /// `(venue, account)` pairs for which `force_unknown` ran, with its return value.
    pub forced: Vec<(VenueId, AccountId, u32)>,
    /// `RecoveryFlags` given per account at the end of the recovery.
    pub flags: Vec<(AccountId, u8)>,
    /// `(venue, account, cursor)` of the `MirrorRestore` batches still to send.
    pub mirror: Vec<(VenueId, AccountId, u32)>,
    /// The start snapshot's `engine_epoch` (section 3): `prev_epoch` of `SetEpoch`.
    pub snap_epoch: u32,
    /// The start snapshot is the journal's Drain tail.
    pub drain_tail: bool,
    /// RecoverMismatch Notes written after the Recovery boot block.
    pub pending_notes: Vec<hft_event::Note>,
}

impl Default for RecoverState {
    fn default() -> RecoverState {
        RecoverState {
            clock_cal: ClockCalRow::default(),
            recovery_now: Tsc(0),
            poison: None,
            mismatches: Vec::new(),
            snapshot_seq: 0,
            tail_end_seq: 0,
            skipped_range: None,
            replayed: 0,
            forced: Vec::new(),
            flags: Vec::new(),
            mirror: Vec::new(),
            snap_epoch: 0,
            drain_tail: false,
            pending_notes: Vec::new(),
        }
    }
}

/// `xxh3_64(kind ‖ body)` of a logged record.
pub fn record_input_hash(r: &MemRecord) -> u64 {
    let mut s = InputSlot::with_capacity(r.body.len());
    s.set_bytes(r.kind, RecordFlags(0), Tsc(r.tsc), &r.body);
    crate::driver::live::input_hash(&s)
}

/// Index just past the last record carrying `EVENT_END`.
pub fn event_end(log: &Log) -> usize {
    log.recs
        .iter()
        .rposition(|r| r.flags.0 & RecordFlags::EVENT_END.0 != 0)
        .map_or(0, |i| i + 1)
}

/// Indices of the `StateSnapshot` records of a log (ascending).
pub fn snapshot_indices(log: &Log) -> Vec<usize> {
    log.recs
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == EventKind::STATE_SNAPSHOT)
        .map(|(i, _)| i)
        .collect()
}

fn snap_hdr(r: &MemRecord) -> Option<SnapshotHdr> {
    SnapshotHdr::read_from_bytes(r.body.get(..core::mem::size_of::<SnapshotHdr>())?).ok()
}

/// Whether the log ends with a Drain snapshot followed by no Input.
pub fn ends_with_drain(log: &Log) -> bool {
    let Some(&ix) = snapshot_indices(log).last() else {
        return false;
    };
    let drain = log
        .recs
        .get(ix)
        .and_then(snap_hdr)
        .is_some_and(|h| h.purpose == PURPOSE_DRAIN);
    drain && !log.recs.iter().skip(ix + 1).any(|r| r.role() == Role::Input)
}

/// The latest `HostProfile` of a log (version comparison).
fn last_build(log: &Log) -> Option<[u8; 20]> {
    log.recs
        .iter()
        .rev()
        .find(|r| r.kind == EventKind::HOST_PROFILE)
        .and_then(|r| r.read::<hft_event::HostProfile>())
        .map(|h| h.build_git)
}

/// Every logged blob content up to record `upto` (exclusive).
fn blobs_upto(log: &Log, upto: usize) -> Vec<(BlobRef, Vec<u8>)> {
    let mut v: Vec<(BlobRef, Vec<u8>)> = Vec::new();
    for r in log.recs.iter().take(upto) {
        if r.kind != EventKind::CONFIG_BLOB {
            continue;
        }
        let Some(h) = crate::types::read_body::<BlobHdr>(&r.body) else {
            continue;
        };
        let c = r.body.get(72..72 + h.blob.len as usize).unwrap_or(&[]);
        if !v.iter().any(|x| x.0.sha256 == h.blob.sha256) {
            v.push((h.blob, c.to_vec()));
        }
    }
    v
}

/// Restores every component from the snapshot record `ix` of `log` (README "从快照起步").
/// `epoch` = the OMS engine epoch to run with (recover: the new one; verify/mirror: the
/// snapshot's).
pub(crate) fn restore_at<S: StrategySet>(
    log: &Log,
    ix: usize,
    setup: &EngineSetup,
    epoch: Option<u32>,
) -> Result<(Engine, Instances<S>), BootError> {
    let rec = log
        .recs
        .get(ix)
        .ok_or(BootError::Journal(JournalErr::NoSnapshot))?;
    let hdr = snap_hdr(rec).ok_or(BootError::SnapshotUndecodable { section: 0 })?;
    let body = rec.body.get(core::mem::size_of::<SnapshotHdr>()..).unwrap_or(&[]);
    let secs: SeqSections =
        read_seq_sections(body).map_err(|s| BootError::SnapshotUndecodable { section: s })?;
    let blobs = blobs_upto(log, ix);
    let content = |r: &BlobRef| {
        blobs
            .iter()
            .find(|x| x.0.sha256 == r.sha256)
            .map(|x| x.1.as_slice())
    };
    // Active configuration (section 9): refdata, engine, strategy params, risk.
    let mut refdata: Option<BlobRef> = None;
    let mut engine_ref: Option<BlobRef> = None;
    let mut risk_ref: Option<BlobRef> = None;
    let mut params: Vec<(BlobRef, hft_event::CmdScope)> = Vec::new();
    for (b, sc) in secs.active.iter() {
        match b.kind {
            BlobKind::Refdata => refdata = Some(*b),
            BlobKind::Engine => engine_ref = Some(*b),
            BlobKind::RiskParams => risk_ref = Some(*b),
            BlobKind::StrategyParams => params.push((*b, *sc)),
            _ => {}
        }
    }
    let rd_ref = refdata.ok_or(BootError::SnapshotUndecodable { section: 9 })?;
    let rd_bytes = content(&rd_ref).ok_or(BootError::Config(ConfigErr::Missing))?;
    let rd = RefData::load(rd_bytes, None).map_err(|r| {
        BootError::Config(ConfigErr::Blob {
            kind: BlobKind::Refdata as u16,
            code: r.code() as u16,
        })
    })?;
    let risk_ref = risk_ref.ok_or(BootError::SnapshotUndecodable { section: 9 })?;
    let risk_bytes = content(&risk_ref).ok_or(BootError::Config(ConfigErr::Missing))?;
    let decode = setup
        .risk_decode
        .ok_or(BootError::Config(ConfigErr::Component("risk_decode")))?;
    let risk_params = decode(risk_bytes, AccountId(0), &rd)
        .map_err(|d| BootError::Config(ConfigErr::Params(u32::from(d))))?;
    // The OMS epoch: section 3.
    let r = SnapshotReader::new(body);
    let snap_epoch = r
        .find_section(hft_oms::SECTION_OMS_COUNTERS)
        .ok()
        .flatten()
        .and_then(|s| {
            s.body
                .get(..4)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        })
        .unwrap_or(setup.oms.engine_epoch);
    let mut setup2 = setup.clone();
    setup2.oms.engine_epoch = epoch.unwrap_or(snap_epoch);
    // A small new cell/timer limit must not turn an otherwise readable snapshot into a
    // component decode error. Allocate enough to inspect the saved occupancy, then reject in
    // CapKind order below. A successful restore uses exactly the requested bounds.
    let cells_saved: usize = secs.strategies.iter().map(|(_, gs, _)| gs.len()).sum();
    let mut allocation = setup2.clone();
    allocation.caps.max_cells_total = allocation
        .caps
        .max_cells_total
        .max(u32::try_from(cells_saved).map_err(|_| BootError::SnapshotUndecodable { section: 5 })?);
    let demand = crate::engine::timer_demand(&rd, &allocation);
    allocation.caps.max_timers =
        allocation.caps.max_timers.max(demand + 1).max(
            u32::try_from(secs.timers.len()).map_err(|_| BootError::SnapshotUndecodable { section: 8 })?,
        );
    let mut e = Engine::new(rd, &allocation, &risk_params)?;
    e.caps = setup2.caps;
    e.setup = setup2.clone();
    let mut relaxed = e.caps;
    relaxed.max_instances = u16::MAX;

    let mut inst = Instances::<S>::new(&e.rd, &relaxed)?;
    e.rec.snap_epoch = snap_epoch;
    for (b, _) in [(rd_ref, 0u8), (risk_ref, 0)] {
        if let Some(c) = content(&b) {
            e.cmd.blobs.put(b, c);
        }
    }
    e.cmd.active.refdata = rd_ref;
    e.cmd.active.risk = risk_ref;
    if let Some(er) = engine_ref {
        if let Some(c) = content(&er) {
            e.cmd.blobs.put(er, c);
        }
        e.cmd.active.engine = er;
    }
    // Components (fixed write order of their sections does not matter for reading).
    let mut r = SnapshotReader::new(body);
    e.ledger = Ledger::restore(&mut r, &e.rd, &setup.ledger, &allocation.caps.to_ledger())
        .map_err(|_| BootError::SnapshotUndecodable { section: 1 })?;
    let mut r = SnapshotReader::new(body);
    let mut ocfg = setup2.oms.clone();
    ocfg.tsc_hz_milli = setup.clock.tsc_hz_milli;
    e.oms = Oms::restore(&mut r, &ocfg, &e.rd).map_err(|_| BootError::SnapshotUndecodable { section: 2 })?;
    e.epoch = e.oms.engine_epoch();
    e.prev_epoch = snap_epoch;
    let bounds = exec_bounds(&setup.exec, &e.rd);
    let mc = ModeCfg {
        auto_resume_after_clean_drain: setup.seq.auto_resume_after_clean_drain == 1,
        allow_missing_standby: setup.seq.allow_missing_standby == 1,
    };
    let rcaps = hft_risk::Capacities {
        max_insts: e.caps.max_insts,
        max_cells_total: allocation.caps.max_cells_total,
        max_caps: hft_risk::Capacities::MAX_CAPS,
    };
    let mut r = SnapshotReader::new(body);
    e.risk = Risk::restore(&mut r, &risk_params, &e.rd, &setup.gws, &rcaps, &bounds, mc)
        .map_err(|_| BootError::SnapshotUndecodable { section: 4 })?;
    let mut r = SnapshotReader::new(body);
    e.books
        .restore(&mut r)
        .map_err(|_| BootError::SnapshotUndecodable { section: 6 })?;
    // Strategy instances (section 5) with their parameters.
    let mut cells_total = 0usize;
    let mut groups_over = false;
    for (h, gs, bytes) in secs.strategies.iter() {
        let Some(sl_ix) = inst.ix(h.strategy) else {
            return Err(BootError::SnapshotUndecodable { section: 5 });
        };
        let Some((b, _)) = params.iter().find(|(_, sc)| sc.strategy == h.strategy) else {
            return Err(BootError::SnapshotUndecodable { section: 9 });
        };
        let pb = content(b).ok_or(BootError::Config(ConfigErr::Missing))?;
        let p = parse_params::<S>(pb, b.sha256, &e.rd, u16::MAX, None)
            .map_err(|d| BootError::Config(ConfigErr::Params(d)))?;
        if p.groups.len() > e.caps.max_groups_per_instance {
            groups_over = true;
        }
        cells_total += usize::from(p.groups.len());
        let Some(slot) = inst.slots.get_mut(sl_ix) else {
            continue;
        };
        let si = StrategyInstance {
            strategy: slot.id,
            account: slot.account,
            kind_id: slot.kind_id,
        };
        let rv = RefView::new(&e.rd, slot.account);
        let mut s = S::create(si, &p.params, &p.groups, &rv)
            .map_err(|_| BootError::SnapshotUndecodable { section: 5 })?;
        let mut rr = SnapshotReader::with_desc_version(bytes, h.desc_version);
        S::restore(&mut s, &mut rr).map_err(|_| BootError::SnapshotUndecodable { section: 5 })?;
        slot.inst = Some(s);
        slot.blob = *b;
        slot.model_mem = p.model_mem;
        slot.model_ckpt = p.model_ckpt;
        slot.spec_seq.clear();
        for g in gs.iter() {
            slot.spec_seq.push((g.group, g.spec_seq));
        }
        slot.spec_seq.sort_by_key(|x| x.0);
        e.cmd.blobs.put(*b, pb);
        slot.cur = Some(Box::new(p));
    }
    // Section 8 timers (deadlines in the snapshot's run TSC).
    let mut timers_over = false;
    if secs.timers.len() > e.caps.max_timers as usize {
        timers_over = true;
    }
    e.heap
        .set_capacity(secs.timers.len().max(e.caps.max_timers as usize));
    for t in secs.timers.iter() {
        let _ = e.heap.push(crate::timers::TimerEntry {
            deadline: t.deadline_tsc,
            owner: t.owner,
            timer_id: t.timer_id,
            group: t.group.0,
        });
        if t.owner >= hft_event::TIMER_OWNER_STRATEGY_BASE {
            e.strat_timers += 1;
            let sid = hft_types::StrategyId((t.owner - hft_event::TIMER_OWNER_STRATEGY_BASE) as u8);
            if let Some(slot) = inst.get_mut(sid) {
                let id = hft_strategy_sdk::TimerId(slot.bufs.next_timer_id());
                slot.bufs.set_next_timer_id(id.0 + 1);
                slot.timers.push((id, t.timer_id, t.group));
            }
        }
    }
    e.heap.set_capacity(e.caps.max_timers as usize);
    // Section 10 market rows.
    for row in secs.market.iter() {
        if let Some(d) = e.market.row_mut(row.inst) {
            *d = *row;
        }
    }
    // Section 15 dedup + previews.
    for d in secs.dedup.iter() {
        e.cmd.dedup.push_back(*d);
    }
    e.cmd.previews.extend_from_slice(&secs.previews);
    e.rec.clock_cal = secs.clock_cal;
    // Section 19 recon groups.
    for (h, v) in secs.recon.iter() {
        e.cmd.recon.push(crate::commands::ReconGroupState {
            report_id: h.report_id,
            account: h.account,
            venue: h.venue,
            count: h.count,
            received: h.received,
            stored: v.clone(),
        });
    }
    e.last_day_rolled = secs.seq_day.last_day_rolled;
    e.cmd.notices.extend_from_slice(&secs.notices);
    if secs.engine_clock.tsc_hz_milli != 0 {
        e.clock = crate::clock::EngineClock::new(secs.engine_clock)
            .map_err(|_| BootError::SnapshotUndecodable { section: 25 })?;
        e.boot_seq = secs.engine_clock.clock_seq + 1;
    }
    for c in secs.conns.iter() {
        if let Some(s) = e.market.conn_mut(c.venue, c.slot) {
            s.up = c.flags & 1 != 0;
            s.resync = c.flags & 2 != 0;
            s.live_tsc = c.live_tsc.0;
            s.used = true;
        }
    }
    // The segment of the snapshot record.
    let si = log.seg_of(ix);
    if let Some(sc) = log.segs.get(si) {
        e.seg = *sc;
    }
    e.cur_seq = hdr.at_seq;
    e.last_ens = hdr.engine_ns;
    e.last_tsc = rec.tsc;
    e.last_keyframe_tsc = rec.tsc;
    e.rec.snapshot_seq = rec.seq;
    e.clock_ok = hdr.purpose != 0
        && e.risk
            .mode
            .cap(CapSource::CLOCK_BAD, &hft_risk::scope_any())
            .is_none();
    // Derived caches (README "从快照起步").
    e.market.rx_from_seq = hdr.at_seq;
    for i in 0..e.market.rows.len() {
        e.market.refresh_funding(InstId(i as u32));
    }
    let all = crate::strategies::all_groups(&inst, e.caps.max_cells_total.max(cells_total as u32));
    let mut r = SnapshotReader::new(body);
    let rv0 = RefView::new(&e.rd, AccountId(0));
    e.exec = hft_exec::Exec::restore(
        &mut r,
        &setup.exec,
        &hft_exec::Capacities {
            max_cells_total: e.caps.max_cells_total.max(cells_total as u32),
            max_subs_per_inst: e.caps.max_subs_per_inst,
        },
        &all,
        &rv0,
    )
    .map_err(|_| BootError::SnapshotUndecodable { section: 21 })?;
    for s in inst.slots.iter() {
        let Some(p) = s.cur.as_ref() else { continue };
        e.oms.on_groups_changed(&p.groups);
        e.ledger.on_groups_changed(s.id, &p.groups);
        e.risk.on_groups_changed(s.account, &p.groups);
    }
    // risk.on_legs cache (outputs not executed).
    let keys: Vec<(AccountId, GroupId, hft_event::GroupSpec)> = inst
        .slots
        .iter()
        .filter_map(|s| s.cur.as_ref().map(|p| (s.account, p)))
        .flat_map(|(a, p)| p.groups.specs().iter().map(move |g| (a, g.id, *g)))
        .collect();
    for (a, g, sp) in keys.iter() {
        let ls = crate::views::leg_summary(&e.oms, &e.market, &e.rd, *a, *g, Some(sp));
        let _ = e.risk.on_legs(*a, *g, &ls);
    }
    let marks: Vec<(InstId, hft_types::Px, i64)> = secs
        .market
        .iter()
        .filter(|r| r.mark_ens != 0 || r.mark_px.0 != 0)
        .map(|r| (r.inst, r.mark_px, r.mark_ens))
        .collect();
    e.ledger.restore_marks(&marks);
    for row in secs.market.iter() {
        if row.interval_s != 0 {
            e.ledger.on_funding_interval(row.inst, row.interval_s);
        }
    }
    let cells: Vec<(AccountId, GroupId)> = inst.cells().map(|c| (c.account, c.group)).collect();
    e.gpos.set_cells(cells.into_iter());
    // Gate input caches from the restored gate state.
    derive_gate_caches(&mut e);
    // Subscription index; the feed commands of a start are sent at EngineClock.
    crate::subs::rebuild_all(&mut e, &mut inst);
    let insts_over = inst.subs.overflow > 0;
    e.out.clear();
    for s in e.sent_union.iter_mut() {
        *s = 0;
    }
    // Apply the boot block's L2 keyframes that follow the snapshot (levels only).
    let mut k = ix + 1;
    while let Some(r) = log.recs.get(k) {
        if r.kind == EventKind::L2_SNAPSHOT {
            crate::process::apply_l2_keyframe(&mut e, &r.body);
        } else if r.kind != EventKind::SEGMENT_ROLL || r.role() != Role::Meta {
            break;
        }
        k += 1;
    }
    // Replay resumes at the next Input. Outputs after the selected snapshot (including the
    // boot block's L2 keyframes) already belong to the next Output interval.
    for r in log
        .recs
        .iter()
        .skip(ix + 1)
        .take_while(|r| r.role() != Role::Input)
    {
        if r.role() == Role::Output {
            e.out_hash.add(r.kind, r.flags, &[&r.body]);
        }
    }
    // Capacity checks in CapKind order.
    let n_instances = inst.slots.iter().filter(|s| s.cur.is_some()).count();
    let shrink = if insts_over {
        Some(CapKind::Insts)
    } else if cells_total > e.caps.max_cells_total as usize {
        Some(CapKind::Cells)
    } else if n_instances > usize::from(e.caps.max_instances)
        || inst.slots.len() > usize::from(e.caps.max_instances)
    {
        Some(CapKind::Instances)
    } else if groups_over {
        Some(CapKind::GroupsPerInstance)
    } else if timers_over || e.strat_timers > e.caps.max_timers.saturating_sub(demand) {
        Some(CapKind::Timers)
    } else {
        None
    };
    if let Some(what) = shrink {
        return Err(BootError::CapacityShrink { what });
    }
    if e.caps.max_timers <= demand {
        return Err(BootError::TimerCapacity { demand });
    }
    Ok((e, inst))
}

/// Index of the first Input after record `ix`.
fn next_input(log: &Log, ix: usize) -> usize {
    let mut k = ix + 1;
    while let Some(r) = log.recs.get(k) {
        if r.role() == Role::Input {
            return k;
        }
        k += 1;
    }
    log.recs.len()
}

/// Chooses the start snapshot (record index) and restores it.
fn choose_and_restore<S: StrategySet, F: Fn(u32) -> Option<u32>>(
    log: &Log,
    plan: &RecoverPlan,
    setup: &EngineSetup,
    epoch_of: &F,
) -> Result<(usize, Engine, Instances<S>), BootError> {
    let snaps = snapshot_indices(log);
    let last = *snaps.last().ok_or(BootError::Journal(JournalErr::NoSnapshot))?;
    let try_one = |ix: usize| -> Result<(usize, Engine, Instances<S>), BootError> {
        let rec = log
            .recs
            .get(ix)
            .ok_or(BootError::Journal(JournalErr::NoSnapshot))?;
        let body = rec.body.get(core::mem::size_of::<SnapshotHdr>()..).unwrap_or(&[]);
        let r = SnapshotReader::new(body);
        let snap_epoch = r
            .find_section(hft_oms::SECTION_OMS_COUNTERS)
            .ok()
            .flatten()
            .and_then(|s| {
                s.body
                    .get(..4)
                    .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            })
            .unwrap_or(0);
        let (e, i) = restore_at::<S>(log, ix, setup, epoch_of(snap_epoch))?;
        Ok((ix, e, i))
    };
    match plan.start {
        SnapshotSel::Latest => try_one(last),
        SnapshotSel::DrainTail => {
            if !ends_with_drain(log) {
                return Err(BootError::VersionNeedsDrain);
            }
            try_one(last)
        }
        SnapshotSel::AtSeq(s) => {
            // The newest snapshot at or before `s`.
            let ix = snaps
                .iter()
                .rev()
                .copied()
                .find(|i| log.recs.get(*i).is_some_and(|r| r.seq <= s))
                .ok_or(BootError::Journal(JournalErr::NoSnapshot))?;
            try_one(ix)
        }
        SnapshotSel::NewestReadable => {
            let mut last_err = BootError::Journal(JournalErr::NoSnapshot);
            for ix in snaps.iter().rev().copied() {
                match try_one(ix) {
                    Ok(x) => return Ok(x),
                    Err(e @ BootError::SnapshotUndecodable { .. }) => last_err = e,
                    Err(e) => return Err(e),
                }
            }
            Err(last_err)
        }
    }
}

/// `boot(Journal(plan))`.
pub(crate) fn boot_journal<D: Driver<S>, S: StrategySet>(
    plan: RecoverPlan,
    setup: &EngineSetup,
    mut drv: D,
    hb: &'static Heartbeat,
) -> Result<Sequencer<D, S>, BootError> {
    let mut log = drv
        .start_log()
        .ok_or(BootError::Journal(JournalErr::NoSnapshot))?;
    let mode = drv.mode();
    // 1. Truncate to the last EVENT_END; report the half event's Input.
    let end = event_end(&log);
    let poison = log
        .recs
        .get(end..)
        .unwrap_or(&[])
        .iter()
        .find(|r| r.role() == Role::Input)
        .map(record_input_hash);
    log.recs.truncate(end);
    // Version change (live recovery only).
    if mode == DriverMode::Live {
        let cur = drv.host_profile().build_git;
        let changed = last_build(&log).is_some_and(|b| b != cur);
        if changed
            && matches!(plan.start, SnapshotSel::Latest | SnapshotSel::AtSeq(_))
            && !ends_with_drain(&log)
        {
            return Err(BootError::VersionNeedsDrain);
        }
    }
    let live = mode == DriverMode::Live;
    // The tail is replayed with the snapshot's epoch (the logged reports carry it); the new
    // epoch takes effect at the end of the recovery.
    let epoch_of = |_snap: u32| -> Option<u32> { None };
    let (ix, mut e, mut inst) = choose_and_restore::<S, _>(&log, &plan, setup, &epoch_of)?;
    e.rec.poison = poison;
    e.rec.drain_tail = ends_with_drain(&log) && snapshot_indices(&log).last() == Some(&ix);
    if !live {
        // Verify / mirror: continue from the snapshot with the driver itself.
        drv.start_at(next_input(&log, ix));
        return Ok(Sequencer {
            e,
            inst,
            drv,
            hb,
            slot: InputSlot::with_capacity(1 << 20),
        });
    }
    // 3. Tail replay.
    if plan.flags.0 & RecoveryFlags::FROM_SNAPSHOT != 0 {
        if plan.replay_tail {
            return Err(BootError::Config(ConfigErr::Component(
                "FROM_SNAPSHOT replays tail",
            )));
        }
        let last = log
            .recs
            .len()
            .checked_sub(1)
            .and_then(|i| log.record_end_seq(i))
            .ok_or(BootError::Journal(JournalErr::NoSnapshot))?;
        let snap_end = log
            .record_end_seq(ix)
            .ok_or(BootError::Journal(JournalErr::NoSnapshot))?;
        e.rec.skipped_range = Some(if snap_end < last {
            (snap_end + 1, last)
        } else {
            (0, 0)
        });
    }
    let snap_rec_tsc = log.recs.get(ix).map_or(0, |r| r.tsc);
    let snap_ens = e.last_ens;
    let mut stop_tsc = snap_rec_tsc;
    let mut stop_ens = snap_ens;
    let hz_prev = if e.clock.row.tsc_hz_milli != 0 {
        e.clock.row.tsc_hz_milli
    } else {
        e.seg.tsc_hz_milli
    };
    if plan.replay_tail {
        let start = next_input(&log, ix);
        let per_round = setup.seq.replay_per_round;
        let rd = ReplayDriver::new(ReplayMode::Recover, log.clone(), start, per_round);
        let mut rs: Sequencer<ReplayDriver, S> = Sequencer {
            e,
            inst,
            drv: rd,
            hb,
            slot: InputSlot::with_capacity(1 << 20),
        };
        rs.drv.set_run_tsc(drv.run_tsc());
        loop {
            match rs.step() {
                crate::StepOutcome::Busy => {}
                crate::StepOutcome::Idle => break,
                crate::StepOutcome::Done { .. } => break,
                crate::StepOutcome::Fatal(f) => {
                    let _ = f;
                    return Err(BootError::Journal(JournalErr::Corrupt { seq: rs.e.cur_seq }));
                }
            }
            if rs.drv.finished() {
                // One more step flushes nothing; stop here.
                break;
            }
        }
        let Sequencer {
            e: e2,
            inst: i2,
            drv: rdrv,
            ..
        } = rs;
        e = e2;
        inst = i2;
        e.rec.mismatches = rdrv.mismatches().to_vec();
        e.rec.replayed = rdrv.handed;
        if e.last_tsc != 0 {
            stop_tsc = e.last_tsc;
            stop_ens = e.last_ens;
        }
        e.rec.tail_end_seq = log.recs.last().map_or(0, |r| r.seq);
    }
    e.finished = None;
    e.last_ens = stop_ens;
    // 4. recovery_end with this run's TSC.
    let now = drv.run_tsc().unwrap_or(Tsc(stop_tsc));
    let hz_cur = drv.roll_info().tsc_hz_milli;
    let hz_cur = if hz_cur == 0 {
        setup.clock.tsc_hz_milli
    } else {
        hz_cur
    };
    recovery_end(
        &mut e,
        &mut inst,
        &mut drv,
        &plan,
        now,
        Tsc(stop_tsc),
        hz_prev,
        hz_cur,
    )?;
    // New segment: Recovery boot block (the live driver then writes EngineClock).
    e.seg = crate::journal::SegClock {
        tsc_hz_milli: hz_cur,
        calib_tsc: drv.roll_info().calib.tsc.0,
        calib_realtime_ns: drv.roll_info().calib.realtime_ns,
    };
    record_capacity_config(&mut e, drv.roll_info().utc_ns)?;
    let mut seq = Sequencer {
        e,
        inst,
        drv,
        hb,
        slot: InputSlot::with_capacity(1 << 20),
    };
    seq.e.set_epoch_due = true;
    let reason = if plan.flags.0 & RecoveryFlags::FROM_SNAPSHOT != 0 {
        RollReason::RecoverFromSnapshot
    } else {
        RollReason::Recovery
    };
    crate::snapshot::write_boot_block(&mut seq, reason, PURPOSE_RECOVERY)
        .map_err(|_| BootError::Journal(JournalErr::Chain))?;
    let notes = core::mem::take(&mut seq.e.rec.pending_notes);
    if !notes.is_empty() {
        let t = Tsc(seq.e.rec.recovery_now.0);
        let j = seq.drv.journal();
        for n in notes.iter() {
            j.append(RecordFlags(0), t, n)
                .map_err(|_| BootError::Journal(JournalErr::Chain))?;
            seq.e
                .out_hash
                .add(EventKind::NOTE, RecordFlags(0), &[n.as_bytes()]);
        }
        j.end_event();
        j.publish();
    }
    Ok(seq)
}

/// A recovery with new capacities is a new configuration boundary. Keep the other ECF1
/// sections byte-for-byte and publish the new reference in the Recovery boot block, so an
/// independent reader cannot silently reconstruct the previous run's limits.
fn record_capacity_config(e: &mut Engine, utc_ns: i64) -> Result<(), BootError> {
    use hft_refdata::{EcfBuilder, EngineConfig, EngineSection};

    let bad = || BootError::Config(ConfigErr::Component("recovery capacities"));
    let bytes = e.cmd.blobs.get(&e.cmd.active.engine.sha256).ok_or_else(bad)?;
    let old = EngineConfig::load(bytes, None).map_err(|_| bad())?;
    let caps = crate::params::capacities(&old).map_err(|_| bad())?;
    if caps == e.caps {
        return Ok(());
    }
    let version = old.version().checked_add(1).ok_or_else(bad)?;
    let mut b = EcfBuilder::new(version, old.host_class());
    b.created_utc_ns(utc_ns);
    for id in EngineSection::ALL {
        if id == EngineSection::Capacities {
            b.section(
                id,
                crate::params::CAPACITIES_ROW_VERSION,
                24,
                1,
                e.caps.as_bytes().to_vec(),
            );
        } else if let Some(s) = old.section(id) {
            b.section(id, s.row_ver, s.row_size, s.count, s.bytes.to_vec());
        }
    }
    let bytes = b.finish();
    let ec = EngineConfig::load(&bytes, None).map_err(|_| bad())?;
    let reference = Engine::blob_ref(BlobKind::Engine, version, &bytes, *ec.sha256());
    e.cmd.blobs.put(reference, &bytes);
    e.cmd.active.engine = reference;
    Ok(())
}

/// The sequencer's gate-input caches (`feeds_last`, `unknown_last`, order-connection rows) as a
/// function of the snapshotted state. Called on every restore and right after every snapshot is
/// built, so a run and a replay started from that snapshot hold the same caches (INV-6).
pub(crate) fn derive_gate_caches(e: &mut Engine) {
    for a in 1..=account_count(&e.rd) {
        let acc = AccountId(a as u8);
        let bits = e.risk.mode.gate_bits(acc);
        if let Some(x) = e.cmd.feeds_last.get_mut(a) {
            *x = bits & hft_risk::mode::gate_bits::FEEDS != 0;
        }
        let mut n = 0u32;
        for v in 0..64u8 {
            if e.rd.account(acc).venues & (1u64 << v) != 0 {
                n += e.oms.table_view(VenueId(v), acc).map_or(0, |t| t.unknown_count);
            }
        }
        // NO_UNKNOWN set = 0 was the last count applied; otherwise the current count if any,
        // else "never reported" (u32::MAX), so the next count of 0 is applied.
        let last = if bits & hft_risk::mode::gate_bits::NO_UNKNOWN != 0 {
            0
        } else if n != 0 {
            n
        } else {
            u32::MAX
        };
        if let Some(x) = e.cmd.unknown_last.get_mut(a) {
            *x = last;
        }
        let vb = e.risk.mode.venue_bitmaps(acc);
        for v in 0..64u8 {
            if e.rd.account(acc).venues & (1u64 << v) == 0 {
                continue;
            }
            let primary = vb[3] & (1u64 << v) != 0;
            let standby = vb[4] & (1u64 << v) != 0;
            e.cmd.conns.set(
                acc,
                VenueId(v),
                crate::commands::ConnRow {
                    primary,
                    standby,
                    us_seen: primary,
                    us_up: primary,
                },
            );
        }
    }
}

/// README "冷启动恢复" 4: shifts, mismatches, Recovery caps and flags (no cell is written).
#[allow(clippy::too_many_arguments)]
fn recovery_end<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    plan: &RecoverPlan,
    now: Tsc,
    stop: Tsc,
    hz_prev: u64,
    hz_cur: u64,
) -> Result<(), BootError> {
    let fatal = |_| BootError::Journal(JournalErr::Chain);
    e.rec.recovery_now = now;
    e.out.clear();
    // New epoch (apps/hft-core formula): max(snapshot, persisted) + 1, written into the OMS
    // counters (section 3) by reloading its sections with the new configuration.
    let new_epoch = e.rec.snap_epoch.max(plan.epoch) + 1;
    {
        let mut w = hft_strategy_sdk::SnapshotWriter::new(64 << 20);
        e.oms.snapshot(&mut w);
        let mut cfg = e.setup.oms.clone();
        cfg.engine_epoch = new_epoch;
        cfg.tsc_hz_milli = e.setup.clock.tsc_hz_milli;
        let mut r = SnapshotReader::new(w.as_bytes());
        e.oms =
            Oms::restore(&mut r, &cfg, &e.rd).map_err(|_| BootError::SnapshotUndecodable { section: 3 })?;
        e.prev_epoch = e.epoch;
        e.epoch = new_epoch;
        // The group legs are not in the OMS sections: register them again, as `restore_at` does.
        for s in inst.slots.iter() {
            if let Some(p) = s.cur.as_ref() {
                e.oms.on_groups_changed(&p.groups);
            }
        }
    }
    let cause = e.cur_seq;
    let mut o = e.take_oms();
    e.trace.rec("oms.recovery_end", now.0, stop.0);
    e.oms.recovery_end(now, stop, hz_prev, hz_cur, cause, &mut o);
    e.put_oms(o);
    e.trace.rec("exec.recovery_end", now.0, stop.0);
    e.exec.recovery_end(now, stop, hz_prev, hz_cur);
    // Heap shift (same formula as the components).
    e.heap
        .map_deadlines(|d| crate::timers::shift_tsc(d, now, stop, hz_prev, hz_cur));
    // Executor recovery timers merge with same-key entries (earlier kept).
    let reqs: Vec<hft_exec::ExecTimerReq> = e.exec.recovery_timers().collect();
    for t in reqs {
        if t.cancel {
            continue;
        }
        // Merge with a same-key heap entry: keep the earlier deadline.
        let existing = e
            .heap
            .sorted()
            .iter()
            .find(|x| x.owner == 5 && x.timer_id == t.timer_id && x.group == t.group.0)
            .map(|x| x.deadline);
        let at = existing.map_or(t.at.0, |d| d.min(t.at.0));
        e.heap.remove_key(5, t.timer_id, t.group.0);
        e.heap
            .push(crate::timers::TimerEntry {
                deadline: at,
                owner: 5,
                timer_id: t.timer_id,
                group: t.group.0,
            })
            .map_err(|_| BootError::Journal(JournalErr::Chain))?;
    }
    // Mismatches → (venue, account) scope.
    let n_acc = account_count(&e.rd);
    let mut pairs: Vec<(VenueId, AccountId, u64)> = Vec::new();
    for m in e.rec.mismatches.clone() {
        let mut add = |v: VenueId, a: AccountId| {
            if !pairs.iter().any(|p| p.0 == v && p.1 == a) {
                pairs.push((v, a, m.seq));
            }
        };
        let scoped = matches!(
            m.kind,
            EventKind::ORDER_DECISION | EventKind::EXEC_DECISION | EventKind::FUNDING_SETTLE
        ) && m.account.0 != 0;
        if scoped {
            if m.venue.0 != 0 {
                add(m.venue, m.account);
            } else {
                for v in 0..64u8 {
                    if e.rd.account(m.account).venues & (1u64 << v) != 0 {
                        add(VenueId(v), m.account);
                    }
                }
            }
        } else {
            for a in 1..=n_acc {
                let acc = AccountId(a as u8);
                for v in 0..64u8 {
                    if e.rd.account(acc).venues & (1u64 << v) != 0 {
                        add(VenueId(v), acc);
                    }
                }
            }
        }
    }
    for (v, a, seq) in pairs.iter() {
        let mut o = e.take_oms();
        let n = e.oms.force_unknown(*v, *a, now, *seq, &mut o);
        e.put_oms(o);
        e.trace.rec("oms.force_unknown", u64::from(v.0), u64::from(a.0));
        e.rec.forced.push((*v, *a, n));
        e.seq_note(
            codes::note::RECOVER_MISMATCH,
            NoteLevel::P0,
            GroupId(0),
            InstId(0),
            [u64::from(v.0) | (u64::from(a.0) << 8), *seq, 0, 0, 0, 0, 0, 0],
        );
    }
    // CLEAN_DRAIN: hft-risk conditions ①–③ over the whole instance.
    let clean = clean_restart(e, plan);
    for a in 1..=n_acc {
        let acc = AccountId(a as u8);
        let mut sc = hft_risk::scope_any();
        sc.account = acc;
        let rec = crate::mode_glue::auto_rec(CapSource::RECOVERY, sc, e.cur_seq, 0);
        e.trace.rec("apply.Recovery", a as u64, 0);
        crate::mode_glue::apply_mode(e, inst, drv, ModeInput::Auto(rec)).map_err(fatal)?;
        let mut f = plan.flags.0;
        if pairs.iter().any(|p| p.1 == acc) {
            f |= RecoveryFlags::RECOVER_MISMATCH;
        }
        if clean {
            f |= RecoveryFlags::CLEAN_DRAIN;
        }
        e.rec.flags.push((acc, f));
        e.trace.rec("apply.RecoveryFlags", a as u64, u64::from(f));
        crate::mode_glue::apply_mode(
            e,
            inst,
            drv,
            ModeInput::RecoveryFlags {
                account: acc,
                flags: RecoveryFlags(f),
            },
        )
        .map_err(fatal)?;
    }
    // Mirror restore batches are sent after EngineClock (see `mirror_restore_all`).
    e.rec.mirror.clear();
    for (v, accounts) in crate::engine::live_gw_venues(&e.rd) {
        for a in accounts {
            e.rec.mirror.push((v, a, 0));
        }
    }
    // Only the RecoverMismatch Notes are journaled (right after the Recovery boot block); the
    // other outputs of the recovery end are reflected by the Recovery snapshot.
    let mut notes = Vec::new();
    for r in e.out.recs.iter() {
        if r.kind == EventKind::NOTE
            && let Some(n) = crate::types::read_body::<hft_event::Note>(e.out.body(r))
            && n.code == codes::seq_note(codes::note::RECOVER_MISMATCH)
        {
            notes.push(n);
        }
    }
    e.rec.pending_notes = notes;
    e.out.clear();
    Ok(())
}

/// hft-risk "干净 Drain 重启" ①–③ for the whole instance.
fn clean_restart(e: &Engine, plan: &RecoverPlan) -> bool {
    // ① same binary/schema: a version change starts from DrainTail.
    if plan.start == SnapshotSel::DrainTail {
        return false;
    }
    // ② the start snapshot is the journal's Drain tail (no Input replayed after it).
    if !e.rec.drain_tail || e.rec.replayed != 0 {
        return false;
    }
    // ③ every account: no Unknown slot, no Broken/HedgeFailure cap, no open link, no pending
    // engine reduce target.
    if e.exec.engine_reduce_pending() {
        return false;
    }
    for a in 1..=account_count(&e.rd) {
        let acc = AccountId(a as u8);
        for v in 0..64u8 {
            if e.rd.account(acc).venues & (1u64 << v) != 0
                && e.oms
                    .table_view(VenueId(v), acc)
                    .is_some_and(|t| t.unknown_count != 0)
            {
                return false;
            }
        }
    }
    let bad = e
        .risk
        .mode
        .caps()
        .iter()
        .any(|c| c.source == CapSource::BROKEN || c.source == CapSource::HEDGE_FAILURE);
    if bad {
        return false;
    }
    let n_links = account_count(&e.rd) as u32 * e.setup.oms.links_per_account.max(1);
    !(1..=n_links).any(|id| e.oms.link(id).is_some_and(|l| l.active()))
}

/// After the first `EngineClock` of a recovered run: `GwCmd{MirrorRestore}` batches per
/// (venue, account).
pub(crate) fn mirror_restore_all(e: &mut Engine) {
    let list = core::mem::take(&mut e.rec.mirror);
    for (v, a, mut cursor) in list {
        loop {
            let mut batch = hft_oms::FixedVec::<hft_oms::MirrorRestore, 256>::new();
            let more = e.oms.mirror_restore(v, a, &mut batch, &mut cursor);
            for m in batch.as_slice() {
                let c = hft_event::GwCmd::mirror_restore(
                    a,
                    m.inst,
                    hft_types::SlotId(m.slot),
                    m.r#gen,
                    m.epoch,
                    m.exch_order_id,
                );
                e.out.emits.push(crate::engine::Emit::Gw {
                    ring: crate::types::OutRing::Venue(v),
                    cmd: c,
                    rec: None,
                });
            }
            if !more || batch.is_empty() {
                break;
            }
        }
    }
}
