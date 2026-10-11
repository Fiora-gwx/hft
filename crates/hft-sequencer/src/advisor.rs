//! The advisor (README "advisor", "advisor 与 Model", "模型检查点"): a mirror sequencer over the
//! committed seq journal (`ReplayDriver` in mirror mode, Outputs neither compared nor emitted),
//! the strategies' Models fed from the mirror's deliveries, the Advice outbox and ring, chunked
//! model checkpoints and `AdvisorLag`.
//!
//! The Model deliveries themselves happen inside the mirror's processing (the same code path as
//! the fast part, `Instances::mirror`): [`deliver`] (market, own events, Notice, Manual),
//! [`warmup`] (applied `InjectWarmup`), [`model_tick`] (`Timer{Sys, AdvisorTick}`) and
//! [`on_params`] (`SetParams`).

use hft_event::{AdviceHdr, EventKind, Note, NoteLevel, NoteSource, Role};
use hft_rt::Heartbeat;
use hft_strategy_sdk::{
    AdviceBufs, EventRef, GIX_NONE, MODEL_TICK, ModelParamEffect, OwnEvMask, RefView, ResetCause,
    SnapshotProgress, SnapshotReader, SnapshotWriter, StrategyInstance, StrategySet,
};
use hft_types::{AccountId, GROUP_NONE, GroupId, StrategyId, Tsc};
use zerocopy::IntoBytes;

use crate::codes::advisor_note;
use crate::driver::replay::{Log, ReplayDriver, ReplayMode};
use crate::engine::{Engine, EngineSetup};
use crate::journal::MemRecord;
use crate::notify::{Notif, Queued};
use crate::strategies::{Direct, Instances, MarketEv};
use crate::types::{BootError, RecoverPlan, RecoveryFlags, SnapshotSel};
use crate::{BootSource, Sequencer, StepOutcome};

pub mod checkpoint;

// ------------------------------------------------------------------------------- mirror hooks

/// One Advice produced by a Model (before the outbox).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdviceItem {
    pub strategy: StrategyId,
    pub account: AccountId,
    pub group: GroupId,
    pub local_kind: u16,
    pub basis_seq: u64,
    pub basis_tsc: Tsc,
    pub payload: Vec<u8>,
}

/// Per-instance Model state of the mirror.
pub struct ModelSlot<S: StrategySet> {
    pub model: Option<S::ModelInst>,
    pub bufs: Box<AdviceBufs>,
    /// This advisor runs the instance's Model (sharding).
    pub enabled: bool,
    /// Catching up to a checkpoint's `last_seq`: nothing is delivered.
    pub hold_until: u64,
    /// Events delivered (tests).
    pub delivered: u64,
}

impl<S: StrategySet> core::fmt::Debug for ModelSlot<S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ModelSlot")
            .field("model", &self.model.is_some())
            .field("enabled", &self.enabled)
            .field("hold_until", &self.hold_until)
            .finish_non_exhaustive()
    }
}

impl<S: StrategySet> Default for ModelSlot<S> {
    fn default() -> Self {
        ModelSlot {
            model: None,
            bufs: Box::new(AdviceBufs::new()),
            enabled: false,
            hold_until: 0,
            delivered: 0,
        }
    }
}

/// The mirror-side Model table (lives in `Instances` so the shared processing reaches it).
pub struct MirrorModels<S: StrategySet> {
    pub on: bool,
    pub slots: Vec<ModelSlot<S>>,
    pub advice: Vec<AdviceItem>,
    /// Meta Notes of the advisor (ModelGroupReset, …), in order.
    pub notes: Vec<Note>,
    /// UTC of the previous `Timer{Sys, AdvisorTick}` (model tick crossing).
    pub prev_tick_utc: Option<i64>,
}

impl<S: StrategySet> core::fmt::Debug for MirrorModels<S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MirrorModels")
            .field("on", &self.on)
            .finish_non_exhaustive()
    }
}

impl<S: StrategySet> Default for MirrorModels<S> {
    fn default() -> Self {
        MirrorModels {
            on: false,
            slots: Vec::new(),
            advice: Vec::new(),
            notes: Vec::new(),
            prev_tick_utc: None,
        }
    }
}

fn advisor_note(code: u32, level: NoteLevel, s: StrategyId, g: GroupId, v: [u64; 8]) -> Note {
    Note {
        code: hft_event::note_code(NoteSource::Advisor, code),
        level,
        source: NoteSource::Advisor,
        strategy: s,
        _pad: 0,
        group: g,
        inst: hft_types::InstId(0),
        v,
    }
}

fn model_active<S: StrategySet>(e: &Engine, inst: &Instances<S>, ix: usize) -> bool {
    inst.models
        .slots
        .get(ix)
        .is_some_and(|m| m.enabled && m.model.is_some() && e.cur_seq > m.hold_until)
}

/// Collects the Advice of the last Model call of slot `ix`.
fn collect<S: StrategySet>(e: &Engine, inst: &mut Instances<S>, ix: usize) {
    let Some(sl) = inst.slots.get(ix) else { return };
    let (strategy, account) = (sl.id, sl.account);
    let Some(ms) = inst.models.slots.get(ix) else {
        return;
    };
    let items: Vec<AdviceItem> = ms
        .bufs
        .iter()
        .map(|(g, k, p)| AdviceItem {
            strategy,
            account,
            group: g,
            local_kind: k,
            basis_seq: e.cur_seq,
            basis_tsc: e.now.tsc,
            payload: p.to_vec(),
        })
        .collect();
    inst.models.advice.extend(items);
}

fn call<S: StrategySet>(e: &Engine, inst: &mut Instances<S>, ix: usize, ev: &EventRef<'_>) {
    {
        let Some(ms) = inst.models.slots.get_mut(ix) else {
            return;
        };
        let Some(m) = ms.model.as_mut() else { return };
        ms.bufs.begin();
        let mut v = crate::strategies::full_views(e);
        // Model side: `target()`/`order()` return `None` (SDK `ModelViews`).
        v.model = true;
        S::model_on_event(m, ev, &v, &mut ms.bufs.out());
        ms.delivered += 1;
    }
    collect(e, inst, ix);
}

/// Model deliveries of one round (README "advisor 与 Model" 2; SDK delivery table).
pub(crate) fn deliver<S: StrategySet>(
    e: &Engine,
    inst: &mut Instances<S>,
    market: Option<MarketEv>,
    queued: &[Queued],
    manual: Option<(usize, u16, &[u8], bool)>,
) {
    if !inst.models.on {
        return;
    }
    let order: Vec<u16> = inst.dispatch_order.clone();
    for &ix16 in order.iter() {
        let ix = usize::from(ix16);
        if !model_active(e, inst, ix) {
            continue;
        }
        if let Some(m) = market {
            let entries = inst.subs.model_entries(m.inst(), ix16);
            for (gix, mask) in entries {
                if !mask.contains(m.mask()) {
                    continue;
                }
                let ev = crate::strategies::market_ev_pub(&m, gix, &e.market);
                call(e, inst, ix, &ev);
            }
        }
        let own = inst.slots.get(ix).map_or(OwnEvMask(0), |s| s.own);
        for q in queued.iter().filter(|q| usize::from(q.inst) == ix) {
            let ok = match q.n {
                Notif::Order { .. } => own.0 & OwnEvMask::ORDER.0 != 0,
                Notif::Exec { .. } => own.0 & OwnEvMask::EXEC.0 != 0,
                Notif::Notice { .. } => true,
                _ => false,
            };
            if !ok {
                continue;
            }
            let ev = crate::strategies::notif_ev_pub(&q.n);
            call(e, inst, ix, &ev);
        }
        if let Some((mix, gix, payload, reduce_only)) = manual
            && mix == ix
            && own.0 & OwnEvMask::MANUAL.0 != 0
        {
            let ev = EventRef::Manual {
                gix,
                payload,
                reduce_only,
            };
            call(e, inst, ix, &ev);
        }
    }
}

/// An Applied `InjectWarmup` (always to the Model when it exists).
pub(crate) fn warmup<S: StrategySet>(e: &Engine, inst: &mut Instances<S>, ix: usize, d: &Direct<'_>) {
    if !inst.models.on || !model_active(e, inst, ix) {
        return;
    }
    if let Direct::Warmup {
        local_kind,
        chunk_no,
        last,
        payload,
    } = *d
    {
        let ev = EventRef::Warmup {
            local_kind,
            chunk_no,
            last,
            payload,
        };
        call(e, inst, ix, &ev);
    }
}

/// `Timer{Sys, AdvisorTick}`: the model tick of every Model whose `every_ms` interval was crossed
/// since the previous AdvisorTick (UTC aligned; the first AdvisorTick seen delivers nothing).
pub(crate) fn model_tick<S: StrategySet>(e: &Engine, inst: &mut Instances<S>) {
    if !inst.models.on {
        return;
    }
    let utc = e.now.utc_ns;
    let prev = inst.models.prev_tick_utc.replace(utc);
    let Some(prev) = prev else { return };
    let order: Vec<u16> = inst.dispatch_order.clone();
    for &ix16 in order.iter() {
        let ix = usize::from(ix16);
        let Some(ms) = inst.slots.get(ix).and_then(|s| s.model_tick) else {
            continue;
        };
        let every = i64::from(ms.max(1)) * 1_000_000;
        if prev.div_euclid(every) == utc.div_euclid(every) {
            continue;
        }
        if !model_active(e, inst, ix) {
            continue;
        }
        let ev = EventRef::Timer {
            tag: MODEL_TICK,
            group: GROUP_NONE,
            gix: GIX_NONE,
        };
        call(e, inst, ix, &ev);
    }
}

/// After the mirror applied parameters to instance `ix`: `create_model` (first parameters) or
/// `model_on_params` with its group-reset Notes.
pub(crate) fn on_params<S: StrategySet>(e: &Engine, inst: &mut Instances<S>, ix: usize) {
    if !inst.models.on {
        return;
    }
    let Some(sl) = inst.slots.get(ix) else { return };
    let Some(p) = sl.cur.as_ref() else { return };
    let (strategy, account, kind_id) = (sl.id, sl.account, sl.kind_id);
    let version = p.version;
    let Some(ms) = inst.models.slots.get_mut(ix) else {
        return;
    };
    if !ms.enabled {
        return;
    }
    let rv = RefView::new(&e.rd, account);
    let mut notes: Vec<Note> = Vec::new();
    match ms.model.as_mut() {
        None => {
            let si = StrategyInstance {
                strategy,
                account,
                kind_id,
            };
            ms.model = Some(S::create_model(si, &p.params, &p.groups, &rv));
        }
        Some(m) => {
            if e.cur_seq <= ms.hold_until {
                return;
            }
            let eff = S::model_on_params(m, &p.params, &p.groups, &rv);
            match eff {
                ModelParamEffect::None => {}
                ModelParamEffect::ResetAll => {
                    for (_, g) in p.groups.iter() {
                        notes.push(advisor_note(
                            advisor_note::MODEL_GROUP_RESET,
                            NoteLevel::P2,
                            strategy,
                            g.id,
                            [ResetCause::RESET_ALL_CODE, version, 0, 0, 0, 0, 0, 0],
                        ));
                    }
                }
                ModelParamEffect::ResetGroups(r) => {
                    for (gix, c) in r.iter() {
                        if gix >= p.groups.len() {
                            continue;
                        }
                        let g = p.groups.get(gix).id;
                        notes.push(advisor_note(
                            advisor_note::MODEL_GROUP_RESET,
                            NoteLevel::P2,
                            strategy,
                            g,
                            [u64::from(c.code()), version, 0, 0, 0, 0, 0, 0],
                        ));
                    }
                }
            }
        }
    }
    inst.models.notes.extend(notes);
}

// ------------------------------------------------------------------------------- checkpoints

/// Checkpoint header (the key of SDK "Model 检查点" plus what the store needs). The file format
/// around it belongs to apps/hft-core; `checksum` is filled by the store at `commit` with
/// [`ckpt_checksum`] of the body.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct CkptHdr {
    pub strategy: StrategyId,
    pub account: AccountId,
    pub kind_id: u16,
    pub schema_id: u32,
    /// The strategy-declared `Model::LAYOUT_VERSION`, never a descriptor digest.
    pub layout_version: u32,
    pub path_version: u32,
    pub last_seq: u64,
    /// engine_ns of `last_seq` (AdvisorLag after a restart).
    pub last_ens: i64,
    pub len: u64,
    pub checksum: u64,
}

/// Checkpoint I/O failure (`commit` over the disk limit included).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct CkptIoErr;

/// The checkpoint store (apps/hft-core implements files; tests keep memory).
pub trait CheckpointStore {
    /// The newest committed checkpoint of `s` (body into `out`).
    fn load(&mut self, s: StrategyId, out: &mut Vec<u8>) -> Option<CkptHdr>;
    fn begin(&mut self, s: StrategyId, hdr: &CkptHdr) -> Result<(), CkptIoErr>;
    fn write_chunk(&mut self, bytes: &[u8]) -> Result<(), CkptIoErr>;
    fn commit(&mut self) -> Result<(), CkptIoErr>;
}

/// `xxh3_64` of a checkpoint body.
pub fn ckpt_checksum(body: &[u8]) -> u64 {
    xxhash_rust::xxh3::xxh3_64(body)
}

/// The advice ring as the advisor sees it (variable-length records).
pub trait AdviceRing {
    /// Free bytes.
    fn free(&self) -> usize;
    /// Pushes one record (`AdviceHdr` + payload, or a Meta `Note`); the caller checked `free`.
    fn push(&mut self, kind: EventKind, bytes: &[u8]) -> bool;
}

#[derive(Clone, Debug)]
struct CkptJob {
    ix: usize,
    hdr: CkptHdr,
    /// `None` = the path block is next.
    next_gix: Option<u16>,
    written: u64,
    limit: u64,
}

/// Advisor configuration (apps/hft-core `[advisor]`).
#[derive(Clone, Debug)]
pub struct AdvisorCfg {
    /// Mirror Inputs per `poll_once`.
    pub batch: u16,
    /// `advisor.model_checkpoint_ms` (0 = never).
    pub checkpoint_ms: u64,
    /// `advisor.checkpoint_chunk_kib` × 1024.
    pub chunk_bytes: u32,
    /// `advisor.max_lag_ms`.
    pub max_lag_ms: u32,
    /// The instances this advisor runs (`None` = every instance with parameters).
    pub shard: Option<Vec<StrategyId>>,
}

impl Default for AdvisorCfg {
    fn default() -> Self {
        AdvisorCfg {
            batch: 256,
            checkpoint_ms: 600_000,
            chunk_bytes: 1 << 20,
            max_lag_ms: 5_000,
            shard: None,
        }
    }
}

/// One outbox entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutboxItem {
    pub key: (StrategyId, AccountId, GroupId, u16),
    pub rec: Vec<u8>,
}

/// The advisor.
pub struct Advisor<S: StrategySet, C: CheckpointStore, A: AdviceRing> {
    pub mirror: Sequencer<ReplayDriver, S>,
    pub ckpt: C,
    pub ring: A,
    pub cfg: AdvisorCfg,
    pub outbox: Vec<OutboxItem>,
    /// Meta Notes waiting for ring space (ahead of Advice).
    pub notes: Vec<Note>,
    pub advice_coalesced: u64,
    job: Option<CkptJob>,
    last_ckpt_ens: Option<i64>,
    /// Per instance: engine_ns of the last AdvisorLag Note.
    lag_note: Vec<(StrategyId, i64)>,
    /// Committed records seen.
    seen: usize,
    /// Checkpoints committed (tests).
    pub commits: u64,
}

impl<S: StrategySet, C: CheckpointStore, A: AdviceRing> core::fmt::Debug for Advisor<S, C, A> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Advisor")
            .field("outbox", &self.outbox.len())
            .finish_non_exhaustive()
    }
}

/// engine_ns of record `ix` of a log (EngineClock of its run; continuous across restarts).
pub fn ens_of_record(log: &Log, ix: usize) -> Option<i64> {
    let r = log.recs.get(ix)?;
    let ec_ix = (0..=ix).rev().find(|k| {
        log.recs
            .get(*k)
            .is_some_and(|x| x.kind == EventKind::ENGINE_CLOCK)
    })?;
    let ec = log.recs.get(ec_ix)?.read::<hft_event::EngineClock>()?;
    let row = crate::clock::EngineClockRow {
        base_engine_ns: ec.base_engine_ns,
        base_tsc: log.recs.get(ec_ix)?.tsc,
        tsc_hz_milli: ec.tsc_hz_milli,
        clock_seq: 0,
    };
    let c = crate::clock::EngineClock::new(row).ok()?;
    c.ens_of(Tsc(r.tsc)).ok()
}

impl<S: StrategySet, C: CheckpointStore, A: AdviceRing> Advisor<S, C, A> {
    /// Starts over the committed `log`: loads checkpoints, chooses the mirror's start snapshot
    /// and restores (README "模型检查点" 2–4).
    pub fn start(
        log: &Log,
        setup: &EngineSetup,
        cfg: AdvisorCfg,
        mut ckpt: C,
        ring: A,
        hb: &'static Heartbeat,
    ) -> Result<Advisor<S, C, A>, BootError> {
        // Instances of the latest snapshot decide which Models exist; checkpoints are loaded
        // per bound refdata instance.
        let snaps = crate::recover::snapshot_indices(log);
        let latest = *snaps
            .last()
            .ok_or(BootError::Journal(crate::types::JournalErr::NoSnapshot))?;
        let mut loaded: Vec<(StrategyId, CkptHdr, Vec<u8>, u32)> = Vec::new();
        let mut notes: Vec<Note> = Vec::new();
        // The probe sequencer gives the kinds of the instances.
        let probe = {
            let (e, i) = crate::recover::restore_at::<S>(log, latest, setup, None)?;
            let v: Vec<(StrategyId, AccountId, u16)> =
                i.slots.iter().map(|s| (s.id, s.account, s.kind_id)).collect();
            drop(e);
            v
        };
        let first_seq = log.recs.first().map_or(0, |r| r.seq);
        for (s, a, kind) in probe.iter().copied() {
            if cfg.shard.as_ref().is_some_and(|v| !v.contains(&s)) {
                continue;
            }
            if S::kinds().iter().find(|d| d.kind_id == kind).is_none() {
                continue;
            }
            let mut buf = Vec::new();
            let reason = match ckpt.load(s, &mut buf) {
                None => Some(crate::codes::discard::MISSING),
                Some(h) => {
                    let schema = S::kinds()
                        .iter()
                        .find(|d| d.kind_id == kind)
                        .map_or(0, |d| d.schema.id);
                    if h.strategy != s
                        || h.account != a
                        || h.kind_id != kind
                        || h.schema_id != schema
                        || h.layout_version != S::model_layout_version(kind)
                    {
                        Some(crate::codes::discard::KEY_MISMATCH)
                    } else if ckpt_checksum(&buf) != h.checksum
                        || h.len != buf.len() as u64
                        || !path_section_fits(&buf)
                    {
                        Some(crate::codes::discard::CORRUPT)
                    } else if h.last_seq < first_seq || !log.recs.iter().any(|r| r.seq == h.last_seq) {
                        Some(crate::codes::discard::SEQ_NOT_IN_JOURNAL)
                    } else {
                        loaded.push((s, h, buf, 0));
                        None
                    }
                }
            };
            if let Some(r) = reason {
                notes.push(advisor_note(
                    advisor_note::MODEL_CHECKPOINT_DISCARDED,
                    NoteLevel::P1,
                    s,
                    GROUP_NONE,
                    [u64::from(s.0), r, 0, 0, 0, 0, 0, 0],
                ));
            }
        }
        // Mirror start: the newest snapshot not after every loaded checkpoint's last_seq.
        let min_last = loaded.iter().map(|x| x.1.last_seq).min();
        let start_ix = match min_last {
            Some(m) => snaps
                .iter()
                .rev()
                .copied()
                .find(|i| log.recs.get(*i).is_some_and(|r| r.seq <= m))
                .unwrap_or(latest),
            None => latest,
        };
        let start_seq = log.recs.get(start_ix).map_or(0, |r| r.seq);
        let drv = ReplayDriver::new(ReplayMode::Mirror, log.clone(), 0, cfg.batch.max(1));
        let p = RecoverPlan {
            start: SnapshotSel::AtSeq(start_seq),
            replay_tail: true,
            epoch: 0,
            flags: RecoveryFlags(0),
            quarantine: Vec::new(),
        };
        let mut mirror = Sequencer::<ReplayDriver, S>::boot(BootSource::Journal(p), setup, drv, hb)?;
        // Models.
        {
            let Sequencer { e, inst, .. } = &mut mirror;
            inst.models.on = true;
            inst.models.slots = (0..inst.slots.len()).map(|_| ModelSlot::default()).collect();
            for ix in 0..inst.slots.len() {
                let Some(sl) = inst.slots.get(ix) else { continue };
                let s = sl.id;
                // Enabled by shard membership: an instance whose first parameters arrive later
                // gets its Model when the mirror applies them (`on_params` → `create_model`).
                let enabled = cfg.shard.as_ref().is_none_or(|v| v.contains(&s));
                let has_params = sl.cur.is_some();
                if let Some(ms) = inst.models.slots.get_mut(ix) {
                    ms.enabled = enabled;
                }
                if !enabled || !has_params {
                    continue;
                }
                on_params(e, inst, ix);
                if let Some(pos) = loaded.iter().position(|x| x.0 == s) {
                    let (_, h, buf, _) = loaded.remove(pos);
                    let kind = sl_kind(inst, ix);
                    match restore_model::<S>(inst, ix, &h, &buf, kind) {
                        Ok(path_reset) => {
                            if let Some(ms) = inst.models.slots.get_mut(ix) {
                                ms.hold_until = h.last_seq;
                            }
                            if let Some((why, ver)) = path_reset {
                                let settle = inst
                                    .slots
                                    .get(ix)
                                    .and_then(|s| s.cur.as_ref())
                                    .map_or(0, |p| S::model_path_settle_ms(&p.params));
                                notes.push(advisor_note(
                                    advisor_note::MODEL_PATH_RESET,
                                    NoteLevel::P2,
                                    s,
                                    GROUP_NONE,
                                    [
                                        why,
                                        h.last_seq,
                                        settle,
                                        u64::from(ver),
                                        u64::from(S::model_path_version(kind)),
                                        0,
                                        0,
                                        0,
                                    ],
                                ));
                            }
                        }
                        Err(()) => {
                            // Restore failed: empty state.
                            if let Some(ms) = inst.models.slots.get_mut(ix) {
                                ms.model = None;
                            }
                            on_params(e, inst, ix);
                            notes.push(advisor_note(
                                advisor_note::MODEL_CHECKPOINT_DISCARDED,
                                NoteLevel::P1,
                                s,
                                GROUP_NONE,
                                [
                                    u64::from(s.0),
                                    crate::codes::discard::RESTORE_ERR,
                                    0,
                                    0,
                                    0,
                                    0,
                                    0,
                                    0,
                                ],
                            ));
                        }
                    }
                }
            }
            inst.models.notes.clear();
            inst.models.advice.clear();
            // The model tick's "previous" AdvisorTick: the last one at or before the start.
            inst.models.prev_tick_utc = prev_tick_utc(log, start_ix);
        }
        // Models restored from a checkpoint take the parameters at their last_seq once, after
        // the mirror reached it (`pending_params`).
        let mut a = Advisor {
            mirror,
            ckpt,
            ring,
            cfg,
            outbox: Vec::new(),
            notes,
            advice_coalesced: 0,
            job: None,
            last_ckpt_ens: None,
            lag_note: Vec::new(),
            seen: log.recs.len(),
            commits: 0,
        };
        a.mirror.e.trace.rec("advisor.start", start_seq, 0);
        Ok(a)
    }

    /// Feeds newly committed records (`log` = the committed journal so far).
    pub fn feed(&mut self, log: &Log) {
        if log.recs.len() > self.seen {
            let new = log.recs.get(self.seen..).unwrap_or(&[]).to_vec();
            self.mirror.drv.extend(&new, &log.segs, &log.seg_start);
            if let Some(first) = new.first() {
                let from = log.part_ends.partition_point(|p| p.0 < first.seq);
                self.mirror
                    .drv
                    .sink
                    .log
                    .part_ends
                    .extend_from_slice(log.part_ends.get(from..).unwrap_or(&[]));
            }
            self.seen = log.recs.len();
        }
    }

    /// The seq of the last record the mirror applied.
    pub fn applied_seq(&self) -> u64 {
        self.mirror.e.cur_seq
    }

    /// One bounded unit of work (README "advisor" `poll_once`): flush the outbox, then one
    /// checkpoint chunk or one batch of mirrored Inputs. Never blocks.
    pub fn poll_once(&mut self, log: &Log) -> bool {
        self.flush();
        if self.job.is_some() {
            self.ckpt_step();
            self.flush();
            return true;
        }
        self.feed(log);
        // A checkpoint may be exactly at the mirror's starting snapshot. Its parameters
        // must be applied before the first following event, not at the end of that batch.
        let released = self.release_holds(self.mirror.e.cur_seq);
        let boundary = self
            .mirror
            .inst
            .models
            .slots
            .iter()
            .filter_map(|m| (m.hold_until != 0).then_some(m.hold_until))
            .min();
        let end = self.mirror.drv.end;
        if let Some(at) = boundary {
            // Restrict Input delivery only; the last event still finishes all of its Outputs.
            self.mirror.drv.end = end.min(self.mirror.drv.sink.log.recs.partition_point(|r| r.seq <= at));
        }
        let before = self.mirror.drv.handed;
        let out = self.mirror.step();
        let reached = boundary.filter(|_| self.mirror.drv.finished());
        self.mirror.drv.end = end;
        let progressed = self.mirror.drv.handed != before;
        if matches!(out, StepOutcome::Fatal(_)) {
            return false;
        }
        // Meta/Output records after the last Input do not advance e.cur_seq, but they also
        // cannot change the parameters. Exhausting this prefix reaches their boundary too.
        let released = self.release_holds(reached.unwrap_or(self.mirror.e.cur_seq)) || released;
        self.drain_mirror();
        self.lag(log);
        self.maybe_checkpoint();
        self.flush();
        progressed || released
    }

    fn release_holds(&mut self, through: u64) -> bool {
        let Sequencer { e, inst, .. } = &mut self.mirror;
        let mut released = false;
        for ix in 0..inst.models.slots.len() {
            let held = inst
                .models
                .slots
                .get(ix)
                .is_some_and(|m| m.hold_until != 0 && through >= m.hold_until);
            if !held {
                continue;
            }
            if let Some(m) = inst.models.slots.get_mut(ix) {
                m.hold_until = 0;
                released = true;
            }
            // `model_on_params` once with the parameters in effect at last_seq.
            let Some(sl) = inst.slots.get(ix) else { continue };
            let Some(p) = sl.cur.as_ref() else { continue };
            let rv = RefView::new(&e.rd, sl.account);
            if let Some(ms) = inst.models.slots.get_mut(ix)
                && let Some(m) = ms.model.as_mut()
            {
                let _ = S::model_on_params(m, &p.params, &p.groups, &rv);
            }
        }
        released
    }

    fn drain_mirror(&mut self) {
        let items = core::mem::take(&mut self.mirror.inst.models.advice);
        for it in items {
            let mut h = <AdviceHdr as zerocopy::FromZeros>::new_zeroed();
            h.strategy = it.strategy;
            h.account = it.account;
            h.local_kind = it.local_kind;
            h.group = it.group;
            h.len = it.payload.len() as u16;
            h.basis_seq = it.basis_seq;
            h.basis_tsc = it.basis_tsc;
            let mut rec = h.as_bytes().to_vec();
            rec.extend_from_slice(&it.payload);
            let key = (it.strategy, it.account, it.group, it.local_kind);
            match self.outbox.iter_mut().find(|o| o.key == key) {
                Some(o) => {
                    o.rec = rec;
                    self.advice_coalesced += 1;
                }
                None => self.outbox.push(OutboxItem { key, rec }),
            }
        }
        let notes = core::mem::take(&mut self.mirror.inst.models.notes);
        self.notes.extend(notes);
    }

    /// Pushes Notes then outbox entries while the ring has room (order kept, nothing dropped).
    pub fn flush(&mut self) {
        while let Some(n) = self.notes.first() {
            let b = n.as_bytes();
            if self.ring.free() < b.len() || !self.ring.push(EventKind::NOTE, b) {
                return;
            }
            self.notes.remove(0);
        }
        while let Some(o) = self.outbox.first() {
            if self.ring.free() < o.rec.len() {
                return;
            }
            if !self.ring.push(EventKind::ADVICE, &o.rec) {
                return;
            }
            self.outbox.remove(0);
        }
    }

    /// `AdvisorLag` (hft-event definition): per Model instance, committed end vs consumed end.
    fn lag(&mut self, log: &Log) {
        let Some(end_ix) = log.recs.len().checked_sub(1) else {
            return;
        };
        let Some(end_ens) = ens_of_record(log, end_ix) else {
            return;
        };
        let end_seq = log.recs.get(end_ix).map_or(0, |r| r.seq);
        let consumed_seq = self.mirror.e.cur_seq;
        let consumed_ens = self.mirror.e.last_ens;
        let ids: Vec<StrategyId> = self
            .mirror
            .inst
            .slots
            .iter()
            .enumerate()
            .filter(|(i, _)| {
                self.mirror
                    .inst
                    .models
                    .slots
                    .get(*i)
                    .is_some_and(|m| m.enabled && m.model.is_some())
            })
            .map(|(_, s)| s.id)
            .collect();
        for s in ids {
            let lag_ns = end_ens - consumed_ens;
            if let Some(n) = lag_note(
                s,
                lag_ns,
                end_ens,
                end_seq,
                consumed_seq,
                self.cfg.max_lag_ms,
                &mut self.lag_note,
            ) {
                self.notes.push(n);
            }
        }
    }

    fn maybe_checkpoint(&mut self) {
        if self.cfg.checkpoint_ms == 0 || self.job.is_some() {
            return;
        }
        let ens = self.mirror.e.last_ens;
        let last = *self.last_ckpt_ens.get_or_insert(ens);
        if ens - last < self.cfg.checkpoint_ms as i64 * 1_000_000 {
            return;
        }
        self.last_ckpt_ens = Some(ens);
        self.begin_checkpoints();
    }

    /// Starts the checkpoint of the first enabled Model (the others follow in order).
    pub fn begin_checkpoints(&mut self) {
        self.next_job(0);
    }

    fn next_job(&mut self, from: usize) {
        let inst = &self.mirror.inst;
        let e = &self.mirror.e;
        for ix in from..inst.slots.len() {
            let Some(ms) = inst.models.slots.get(ix) else {
                continue;
            };
            if !ms.enabled || ms.model.is_none() {
                continue;
            }
            let Some(sl) = inst.slots.get(ix) else { continue };
            let limit = sl
                .cur
                .as_ref()
                .map_or(0, |p| S::model_snapshot_bytes(&p.params, p.groups.len()));
            let hdr = CkptHdr {
                strategy: sl.id,
                account: sl.account,
                kind_id: sl.kind_id,
                schema_id: S::kinds()
                    .iter()
                    .find(|d| d.kind_id == sl.kind_id)
                    .map_or(0, |d| d.schema.id),
                layout_version: S::model_layout_version(sl.kind_id),
                path_version: S::model_path_version(sl.kind_id),
                last_seq: e.cur_seq,
                last_ens: e.last_ens,
                len: 0,
                checksum: 0,
            };
            if self.ckpt.begin(sl.id, &hdr).is_err() {
                continue;
            }
            self.job = Some(CkptJob {
                ix,
                hdr,
                next_gix: None,
                written: 0,
                limit,
            });
            return;
        }
    }

    /// One chunk of the active checkpoint (the whole advisor holds at `last_seq` meanwhile).
    fn ckpt_step(&mut self) {
        let Some(mut job) = self.job.take() else { return };
        let inst = &self.mirror.inst;
        let Some(m) = inst.models.slots.get(job.ix).and_then(|m| m.model.as_ref()) else {
            return;
        };
        let max = self.cfg.chunk_bytes.max(64) as usize;
        let mut w = SnapshotWriter::new(max.saturating_mul(2).max(1 << 16));
        let mut done = false;
        let mut bytes: Vec<u8> = Vec::new();
        match job.next_gix {
            None => {
                S::model_snapshot_path(m, &mut w);
                let p = w.as_bytes();
                bytes.extend_from_slice(&job.hdr.path_version.to_le_bytes());
                bytes.extend_from_slice(&(p.len() as u32).to_le_bytes());
                bytes.extend_from_slice(p);
                job.next_gix = Some(0);
            }
            Some(g) => {
                let pr = S::model_snapshot_chunk(m, g, &mut w, max as u32);
                bytes.extend_from_slice(w.as_bytes());
                match pr {
                    SnapshotProgress::More(n) => job.next_gix = Some(n),
                    SnapshotProgress::Done => done = true,
                }
            }
        }
        job.written += bytes.len() as u64;
        // Both sections (the 8 B path-section prefix excluded) over the declared
        // `snapshot_bytes`: this generation is void, the previous one stays (SDK "Model 检查点").
        if job.limit != 0 && job.written.saturating_sub(8) > job.limit {
            self.skipped(&job);
            self.next_job(job.ix + 1);
            return;
        }
        if self.ckpt.write_chunk(&bytes).is_err() {
            self.skipped(&job);
            self.next_job(job.ix + 1);
            return;
        }
        if done {
            if self.ckpt.commit().is_ok() {
                self.commits += 1;
            } else {
                self.skipped(&job);
            }
            self.next_job(job.ix + 1);
            return;
        }
        self.job = Some(job);
    }

    /// A void generation (over `snapshot_bytes`, or the store failed): `Note{ModelCkptSkipped}`
    /// (P1; `v[0]` = strategy, `v[1]` = bytes written so far, `v[2]` = declared `snapshot_bytes`).
    fn skipped(&mut self, job: &CkptJob) {
        let s = job.hdr.strategy;
        self.notes.push(advisor_note(
            advisor_note::MODEL_CKPT_SKIPPED,
            NoteLevel::P1,
            s,
            GROUP_NONE,
            [u64::from(s.0), job.written, job.limit, 0, 0, 0, 0, 0],
        ));
    }

    /// A checkpoint is being written.
    pub fn checkpointing(&self) -> bool {
        self.job.is_some()
    }
}

/// The path section's length prefix stays within the body (else the file is corrupt).
fn path_section_fits(body: &[u8]) -> bool {
    checkpoint::path_sections(body).is_ok()
}

fn sl_kind<S: StrategySet>(inst: &Instances<S>, ix: usize) -> u16 {
    inst.slots.get(ix).map_or(0, |s| s.kind_id)
}

/// Restores a Model from a checkpoint body; `Ok(Some((reason, version)))` = the path section
/// was dropped (`ModelPathReset`).
fn restore_model<S: StrategySet>(
    inst: &mut Instances<S>,
    ix: usize,
    h: &CkptHdr,
    body: &[u8],
    kind: u16,
) -> Result<Option<(u64, u32)>, ()> {
    let (pver, path, rest) = checkpoint::path_sections(body).map_err(|_| ())?;
    let Some(ms) = inst.models.slots.get_mut(ix) else {
        return Err(());
    };
    let Some(m) = ms.model.as_mut() else {
        return Err(());
    };
    let mut r = SnapshotReader::new(rest);
    S::model_restore(m, &mut r).map_err(|_| ())?;
    let _ = h;
    if pver != S::model_path_version(kind) {
        return Ok(Some((1, pver)));
    }
    let mut pr = SnapshotReader::new(path);
    if S::model_restore_path(m, &mut pr).is_err() {
        return Ok(Some((2, pver)));
    }
    Ok(None)
}

/// UTC of the last `Timer{Sys, AdvisorTick}` at or before record `ix`.
fn prev_tick_utc(log: &Log, ix: usize) -> Option<i64> {
    let k = (0..=ix).rev().find(|k| {
        log.recs.get(*k).is_some_and(|r| {
            r.kind == EventKind::TIMER
                && r.role() == Role::Input
                && r.read::<hft_event::Timer>()
                    .is_some_and(|t| t.owner == 1 && t.timer_id & 0xFF == 6)
        })
    })?;
    let r = log.recs.get(k)?;
    let c = log.segs.get(log.seg_of(k))?;
    let d = i128::from(r.tsc) - i128::from(c.calib_tsc);
    Some(c.calib_realtime_ns + (d * 1_000_000_000_000 / i128::from(c.tsc_hz_milli.max(1))) as i64)
}

/// One `AdvisorLag` evaluation of instance `s` (hft-event definition): a Note when
/// `lag_ns > max_lag_ms × 10⁶` and no Note of `s` within the last 10 s of `end_ens`.
pub fn lag_note(
    s: StrategyId,
    lag_ns: i64,
    end_ens: i64,
    end_seq: u64,
    consumed_seq: u64,
    max_lag_ms: u32,
    last: &mut Vec<(StrategyId, i64)>,
) -> Option<Note> {
    if lag_ns <= i64::from(max_lag_ms) * 1_000_000 {
        return None;
    }
    let prev = last.iter().find(|x| x.0 == s).map(|x| x.1);
    if prev.is_some_and(|p| end_ens - p < 10_000_000_000) {
        return None;
    }
    match last.iter_mut().find(|x| x.0 == s) {
        Some(x) => x.1 = end_ens,
        None => last.push((s, end_ens)),
    }
    Some(advisor_note(
        advisor_note::ADVISOR_LAG,
        NoteLevel::P1,
        s,
        GROUP_NONE,
        [
            u64::from(s.0),
            (lag_ns / 1_000_000) as u64,
            end_seq.saturating_sub(consumed_seq),
            consumed_seq,
            0,
            0,
            0,
            0,
        ],
    ))
}

/// Records of a log as owned `MemRecord`s (helper for feeding).
pub fn records(log: &Log) -> &[MemRecord] {
    &log.recs
}
