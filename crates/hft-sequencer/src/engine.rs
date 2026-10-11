//! The non-strategy business state (`Engine`), the assembly inputs (`EngineSetup`) and the empty
//! start.

use hft_book::BookSet;
use hft_event::{
    BlobKind, BlobRef, CmdScope, EventKind, FeedCmd, GwCmd, Note, NoteLevel, NoteSource, RecordFlags,
};
use hft_exec::{Exec, ExecConfig, ExecOut, OutCaps};
use hft_ledger::{Ledger, LedgerConfig};
use hft_oms::{OmsConfig, OmsOut, TrigStamps};
use hft_refdata::{AccountKind, DeadManCap, InstKind, RefData};
use hft_risk::{ExecBounds, GwMap, MarginOut, ModeCfg, ModeOutcome, Risk, RiskParams};
use hft_rt::Heartbeat;
use hft_strategy_sdk::{EngineClockAnchor, Now, StrategySet, VenueSet};
use hft_types::{AccountId, GroupId, InstId, Notional, Tsc, VenueId};

use crate::clock::EngineClock;
use crate::commands::CmdState;
use crate::driver::Driver;
use crate::journal::{OutHash, SegClock};
use crate::market::Market;
use crate::notify::Notify;
use crate::params::{CapacitiesRow, SeqParamsRow};
use crate::strategies::Instances;
use crate::timers::TimerHeap;
use crate::trace::Trace;
use crate::types::{BootError, ConfigErr, OutRing, StepOutcome};
use crate::{ConfigSet, Sequencer};

/// Assembly inputs that are not in the journal's config blobs (apps/hft-core builds them from its
/// configuration and the ECF1 sections it parses).
#[derive(Clone, Debug)]
pub struct EngineSetup {
    pub caps: CapacitiesRow,
    pub seq: SeqParamsRow,
    pub oms: OmsConfig,
    pub ledger: LedgerConfig,
    pub exec: ExecConfig,
    /// Venues with a gw (index = kill word index).
    pub gws: GwMap,
    /// Venues with a feed in this hft-core instance.
    pub feeds: VenueSet,
    /// Trading key set per (account, venue) (`Gate(AccountCfg{keys_required})`).
    pub keys_required: Vec<(AccountId, VenueId, u8)>,
    /// Instance role is `prod` (SimFault is forbidden there).
    pub role_prod: bool,
    /// hft-core `advisor.max_lag_ms`.
    pub advisor_max_lag_ms: u32,
    /// hft-core `shutdown.drain_timeout_ms` (0 = no timeout).
    pub drain_timeout_ms: u32,
    /// `SetRisk` decoding (injected at assembly).
    pub risk_decode: Option<crate::parsed::RiskDecode>,
    /// TSC frequency of this run (mHz) and the UTC calibration pair of the first segment.
    pub clock: SegClock,
    /// `EngineClock.base_engine_ns` of an empty start (normally 0).
    pub base_engine_ns: i64,
}

/// One buffered output record of the current Input.
#[derive(Copy, Clone, Debug)]
pub(crate) struct OutRec {
    pub kind: EventKind,
    pub flags: RecordFlags,
    pub off: u32,
    pub len: u32,
}

/// One buffered ring message.
#[derive(Copy, Clone, Debug)]
pub(crate) enum Emit {
    /// `rec` = index of the `OrderDecision` it projects (decision_seq/decided_tsc filled at flush).
    Gw {
        ring: OutRing,
        cmd: GwCmd,
        rec: Option<u32>,
    },
    Feed {
        venue: VenueId,
        cmd: FeedCmd,
    },
}

/// Per-Input output buffer: records in production order, then ring messages.
#[derive(Debug)]
pub(crate) struct OutBuf {
    pub recs: Vec<OutRec>,
    pub bytes: Vec<u8>,
    pub emits: Vec<Emit>,
    /// Seqs of the appended records (index = rec index).
    pub seqs: Vec<u64>,
    /// Orders (slots) placed/cancelled/amended in this Input (decided_tsc stamping).
    pub stamped: Vec<hft_strategy_sdk::OrderRef>,
}

impl OutBuf {
    pub fn new() -> OutBuf {
        OutBuf {
            recs: Vec::with_capacity(512),
            bytes: Vec::with_capacity(1 << 16),
            emits: Vec::with_capacity(256),
            seqs: Vec::with_capacity(512),
            stamped: Vec::with_capacity(64),
        }
    }
    pub fn clear(&mut self) {
        self.recs.clear();
        self.bytes.clear();
        self.emits.clear();
        self.seqs.clear();
        self.stamped.clear();
    }
    /// Buffers one record; returns its index.
    pub fn push_bytes(&mut self, kind: EventKind, flags: RecordFlags, body: &[u8]) -> u32 {
        let off = self.bytes.len() as u32;
        self.bytes.extend_from_slice(body);
        while !self.bytes.len().is_multiple_of(8) {
            self.bytes.push(0);
        }
        self.recs.push(OutRec {
            kind,
            flags,
            off,
            len: body.len() as u32,
        });
        (self.recs.len() - 1) as u32
    }
    pub fn push<B: hft_event::Body>(&mut self, flags: RecordFlags, b: &B) -> u32 {
        self.push_bytes(B::KIND, flags, b.as_bytes())
    }
    pub fn body(&self, r: &OutRec) -> &[u8] {
        self.bytes
            .get(r.off as usize..(r.off + r.len) as usize)
            .unwrap_or(&[])
    }
}

/// Drain progress (README "Drain").
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct DrainState {
    pub active: bool,
    pub cancel_resting: bool,
    /// Why the Drain started.
    pub cause: Option<crate::types::DoneCause>,
    /// engine_ns of the start (timeout).
    pub started_ens: i64,
    /// `ReplaceRefdata` Drain: recheck due / done.
    pub recheck_due: bool,
    pub recheck_done: bool,
    pub recheck_ok: bool,
    pub timed_out: bool,
}

/// Every non-strategy state of the sequencer.
pub struct Engine {
    #[cfg(feature = "testkit")]
    pub(crate) audit: crate::audit::Audit,
    pub rd: RefData,
    pub setup: EngineSetup,
    pub caps: CapacitiesRow,
    pub sp: SeqParamsRow,
    pub books: BookSet,
    pub oms: hft_oms::Oms,
    pub risk: Risk,
    pub ledger: Ledger,
    pub exec: Exec,
    pub market: Market,
    pub heap: TimerHeap,
    /// Upper bound of engine-registered heap entries (README "定时器").
    pub engine_timer_demand: u32,
    pub notify: Notify,
    pub clock: EngineClock,
    pub seg: SegClock,
    /// Seq of this run's `Note{EpochStart}` (0 before `EngineClock`).
    pub boot_seq: u64,
    pub clock_ok: bool,
    pub last_day_rolled: u32,
    pub now: Now,
    /// Seq and kind of the Input being processed.
    pub cur_seq: u64,
    pub cur_kind: EventKind,
    pub trig: TrigStamps,
    /// engine_ns of the last fully applied record (`stop_engine_ns`).
    pub last_ens: i64,
    pub last_tsc: u64,
    pub epoch: u32,
    pub prev_epoch: u32,
    /// Dust line per account index (ExecConfig).
    pub dust: Vec<Notional>,
    pub(crate) out: OutBuf,
    /// Scratch for calls whose output is discarded (boot, group tables).
    pub(crate) oms_pool: Vec<OmsOut>,
    pub(crate) exec_pool: Vec<ExecOut>,
    pub(crate) mode_pool: Vec<ModeOutcome>,
    pub(crate) margin_pool: Vec<MarginOut>,
    mode_dims: (usize, usize, usize),
    pub(crate) cmd: CmdState,
    pub drain: DrainState,
    /// `SnapshotNow` applied, snapshot not yet written.
    pub snapshot_requested: bool,
    /// Bbo dropped by conflation since the last report (fed by the live driver through Inputs).
    pub conflated: u64,
    /// `resolve_broken` scan cursor: (account, cursor) while not finished.
    pub naked_scan: Option<(AccountId, u32)>,
    /// Pending `EpochStart` follow-ups (SetEpoch after recovery).
    pub(crate) set_epoch_due: bool,
    /// Output hash since the last snapshot.
    pub out_hash: OutHash,
    /// Inputs since the last keyframe.
    pub inputs_since_keyframe: u64,
    pub last_keyframe_tsc: u64,
    /// The step result once the Drain (or a fatal) ended the run.
    pub finished: Option<StepOutcome>,
    /// Group position views (`Views::group_pos` returns references).
    pub(crate) gpos: crate::views::GposCache,
    /// Recovery bookkeeping.
    pub(crate) rec: crate::recover::RecoverState,
    /// Call trace (feature `testing`).
    pub trace: Trace,
    /// Count of strategy timers in the heap.
    pub strat_timers: u32,
    /// `CtxFull` count of `timer_at` over the strategy quota.
    pub ctx_full: u64,
    /// The input hash of the record being processed (quarantine).
    /// Union of streams last sent per inst (this run only).
    pub(crate) sent_union: Vec<u8>,
    /// Insts whose position changed between zero and non-zero in this Input.
    pub(crate) pos_touched: Vec<InstId>,
}

impl core::fmt::Debug for Engine {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Engine")
            .field("boot_seq", &self.boot_seq)
            .field("cur_seq", &self.cur_seq)
            .finish_non_exhaustive()
    }
}

/// Region for the L2 books (`BookSet::new_in` needs a 64 B aligned `'static` slice).
fn book_region(n: usize) -> &'static mut [u8] {
    let v: &'static mut [u8] = Box::leak(vec![0u8; n + 64].into_boxed_slice());
    let off = v.as_ptr().align_offset(64);
    let (_, rest) = v.split_at_mut(off.min(64));
    let (head, _) = rest.split_at_mut(n);
    head
}

/// Insts that get an L2 book: refdata `l2_depth > 0` on a venue with a feed.
pub(crate) fn l2_insts(rd: &RefData, feeds: VenueSet) -> Vec<InstId> {
    (1..=rd.inst_count())
        .map(|i| InstId(i as u32))
        .filter(|i| {
            let h = rd.inst(*i);
            h.l2_depth > 0 && feeds.contains(h.venue)
        })
        .collect()
}

/// Exec bounds per `AccountId` (risk needs them indexed by account).
pub(crate) fn exec_bounds(cfg: &ExecConfig, rd: &RefData) -> Vec<ExecBounds> {
    let n = account_count(rd);
    let mut v = vec![ExecBounds::default(); n + 1];
    for a in 1..=n {
        let p = cfg
            .accounts
            .iter()
            .find(|x| usize::from(x.account.0) == a)
            .and_then(|x| x.params)
            .unwrap_or(cfg.default);
        if let Some(b) = v.get_mut(a) {
            *b = ExecBounds {
                force_max_cross: p.force_slice.max_cross,
                force_max_chunk: p.force_slice.max_chunk_notional,
                dust_notional: p.dust_notional,
            };
        }
    }
    v
}

/// Number of accounts in refdata (ids are dense from 1).
pub(crate) fn account_count(rd: &RefData) -> usize {
    (1..=255u8)
        .take_while(|a| rd.account(AccountId(*a)).id.0 == *a)
        .count()
}

/// Number of (account, venue) pairs in refdata.
pub(crate) fn account_venue_pairs(rd: &RefData) -> usize {
    let mut n = 0;
    for a in 1..=account_count(rd) {
        n += rd.account(AccountId(a as u8)).venues.count_ones() as usize;
    }
    n
}

/// Whether the account is a Sim account.
pub(crate) fn is_sim(rd: &RefData, a: AccountId) -> bool {
    matches!(rd.account(a).kind, AccountKind::Sim(_))
}

/// The engine-registered timer demand (README "定时器" 容量).
pub(crate) fn timer_demand(rd: &RefData, setup: &EngineSetup) -> u32 {
    let cells = setup.caps.max_cells_total;
    let exec = 3 * cells + 1;
    let n_acc = account_count(rd) as u32;
    let mut slots = 0u32;
    let mut risk = 0u32;
    let mut with_tables = 0u32;
    for a in 1..=n_acc {
        let am = rd.account(AccountId(a as u8));
        if am.venues != 0 {
            with_tables += 1;
        }
        for v in 0..64u8 {
            if am.venues & (1u64 << v) == 0 {
                continue;
            }
            slots += setup.oms.slots.get(VenueId(v), AccountId(a as u8));
            if rd.venue(VenueId(v)).product == InstKind::Perp {
                risk += 3;
            }
        }
    }
    let oms = with_tables * setup.oms.links_per_account + slots + 1;
    exec + oms + risk + sys_timer_count(rd)
}

/// Periodic system timers registered at every start: OmsSweep, DeadMan per (venue, live account)
/// with a dead-man interface, ConnRotate per venue with a live account.
pub(crate) fn sys_timer_count(rd: &RefData) -> u32 {
    let mut n = 1;
    for (v, accounts) in live_gw_venues(rd) {
        if rd.caps(v).dms != DeadManCap::None {
            n += accounts.len() as u32;
        }
        n += 1;
    }
    n
}

/// Venues traded by Live accounts, with those accounts (ascending).
pub(crate) fn live_gw_venues(rd: &RefData) -> Vec<(VenueId, Vec<AccountId>)> {
    let mut out: Vec<(VenueId, Vec<AccountId>)> = Vec::new();
    for v in 0..64u8 {
        let mut accs = Vec::new();
        for a in 1..=account_count(rd) {
            let am = rd.account(AccountId(a as u8));
            if am.venues & (1u64 << v) != 0 && matches!(am.kind, AccountKind::Live) {
                accs.push(AccountId(a as u8));
            }
        }
        if !accs.is_empty() {
            out.push((VenueId(v), accs));
        }
    }
    out
}

impl Engine {
    /// Builds every component (allocations happen here, not in `step`).
    pub(crate) fn new(
        rd: RefData,
        setup: &EngineSetup,
        risk_params: &RiskParams,
    ) -> Result<Engine, BootError> {
        let caps = setup.caps;
        let sp = setup.seq;
        let l2 = l2_insts(&rd, setup.feeds);
        let mem = book_region(BookSet::bytes_needed(&rd, &l2));
        let books =
            BookSet::new_in(&rd, &l2, mem).map_err(|_| BootError::Config(ConfigErr::Component("book")))?;
        let mut ocfg = setup.oms.clone();
        ocfg.tsc_hz_milli = setup.clock.tsc_hz_milli;
        let oms =
            hft_oms::Oms::new(&ocfg, &rd).map_err(|_| BootError::Config(ConfigErr::Component("oms")))?;
        let bounds = exec_bounds(&setup.exec, &rd);
        let mc = ModeCfg {
            auto_resume_after_clean_drain: sp.auto_resume_after_clean_drain == 1,
            allow_missing_standby: sp.allow_missing_standby == 1,
        };
        let rcaps = hft_risk::Capacities {
            max_insts: caps.max_insts,
            max_cells_total: caps.max_cells_total,
            max_caps: hft_risk::Capacities::MAX_CAPS,
        };
        let risk = Risk::new(risk_params, &rd, &setup.gws, &rcaps, &bounds, mc)
            .map_err(|_| BootError::Config(ConfigErr::Component("risk")))?;
        let ledger = Ledger::new(&rd, &setup.ledger, &caps.to_ledger())
            .map_err(|_| BootError::Config(ConfigErr::Component("ledger")))?;
        let exec = Exec::new(
            &setup.exec,
            &hft_exec::Capacities {
                max_cells_total: caps.max_cells_total,
                max_subs_per_inst: caps.max_subs_per_inst,
            },
        )
        .map_err(|_| BootError::Config(ConfigErr::Component("exec")))?;
        Engine::assemble(rd, setup, books, oms, risk, ledger, exec)
    }

    /// Wraps already-built components.
    pub(crate) fn assemble(
        rd: RefData,
        setup: &EngineSetup,
        books: BookSet,
        oms: hft_oms::Oms,
        risk: Risk,
        ledger: Ledger,
        exec: Exec,
    ) -> Result<Engine, BootError> {
        let caps = setup.caps;
        let demand = timer_demand(&rd, setup);
        if caps.max_timers <= demand {
            return Err(BootError::TimerCapacity { demand });
        }
        let n_acc = account_count(&rd);
        let mut dust = vec![Notional(0); n_acc + 1];
        for (a, d) in dust.iter_mut().enumerate() {
            let p = setup
                .exec
                .accounts
                .iter()
                .find(|x| usize::from(x.account.0) == a)
                .and_then(|x| x.params)
                .unwrap_or(setup.exec.default);
            *d = p.dust_notional;
        }
        let u = caps.notify_unit() as usize;
        let mode_dims = (
            caps.max_cells_total as usize,
            n_acc + 1,
            account_venue_pairs(&rd) + 1,
        );
        let mode_pool = (0..4)
            .map(|_| ModeOutcome::with_capacity(mode_dims.0, mode_dims.1, mode_dims.2))
            .collect();
        let market = Market::new(&rd);
        let n_inst = rd.inst_count();
        let anchor = EngineClockAnchor {
            base_engine_ns: 0,
            base_tsc: Tsc(0),
            tsc_hz_milli: setup.clock.tsc_hz_milli,
            utc_tsc: Tsc(setup.clock.calib_tsc),
            utc_ns: setup.clock.calib_realtime_ns,
        };
        Ok(Engine {
            gpos: crate::views::GposCache::new(caps.max_cells_total as usize),
            rd,
            setup: setup.clone(),
            caps,
            sp: setup.seq,
            books,
            oms,
            risk,
            ledger,
            exec,
            market,
            heap: TimerHeap::new(caps.max_timers as usize),
            engine_timer_demand: demand,
            notify: Notify::new(u),
            clock: EngineClock::default(),
            seg: setup.clock,
            boot_seq: 0,
            clock_ok: false,
            last_day_rolled: 0,
            now: Now::new(Tsc(0), 0, 0, true, anchor),
            cur_seq: 0,
            cur_kind: EventKind(0),
            trig: TrigStamps::default(),
            last_ens: 0,
            last_tsc: 0,
            epoch: setup.oms.engine_epoch,
            prev_epoch: setup.oms.engine_epoch,
            dust,
            out: OutBuf::new(),
            #[cfg(feature = "testkit")]
            audit: crate::audit::Audit::new(),
            oms_pool: (0..6).map(|_| OmsOut::new()).collect(),
            exec_pool: (0..6).map(|_| ExecOut::with_caps(OutCaps::README)).collect(),
            mode_pool,
            margin_pool: (0..4).map(|_| MarginOut::default()).collect(),
            mode_dims,
            cmd: CmdState::new(caps.recent_cmd_ids as usize),
            drain: DrainState::default(),
            snapshot_requested: false,
            conflated: 0,
            naked_scan: None,
            set_epoch_due: false,
            out_hash: OutHash::new(),
            inputs_since_keyframe: 0,
            last_keyframe_tsc: 0,
            finished: None,
            rec: crate::recover::RecoverState::default(),
            trace: Trace::default(),
            strat_timers: 0,
            ctx_full: 0,
            sent_union: vec![0; n_inst + 1],
            pos_touched: Vec::with_capacity(64),
        })
    }

    /// `InputHash` of the half event a cold start truncated (reported to hft-core).
    pub fn truncated_input_hash(&self) -> Option<u64> {
        self.rec.poison
    }
    /// `(venue, account, count)` of every `force_unknown` of the last recovery.
    pub fn recovery_forced(&self) -> &[(VenueId, AccountId, u32)] {
        &self.rec.forced
    }
    /// `RecoveryFlags` bits given per account at the end of the last recovery.
    pub fn recovery_flags(&self) -> &[(AccountId, u8)] {
        &self.rec.flags
    }
    /// Inputs replayed from the tail by the last recovery.
    pub fn recovery_replayed(&self) -> u64 {
        self.rec.replayed
    }
    /// Output mismatches of the last recovery's tail replay.
    pub fn recovery_mismatches(&self) -> &[crate::driver::Mismatch] {
        &self.rec.mismatches
    }
    /// `(n_stored, count, received)` of an incomplete reconciliation group (section 19).
    pub fn cmd_recon_pending(&self, report_id: u64) -> Option<(u16, u16, u16)> {
        self.cmd
            .recon
            .iter()
            .find(|g| g.report_id == report_id)
            .map(|g| (g.stored.len() as u16, g.count, g.received))
    }
    /// Whether a command id is in the de-duplication table (tests, tools).
    pub fn cmd_dedup_has(&self, id: [u8; 16]) -> bool {
        self.cmd.dedup.iter().any(|r| r.cmd_id == id)
    }
    /// The read-only views of the current state (tests, tools).
    pub fn seq_views(&self) -> crate::views::SeqViews<'_> {
        crate::strategies::full_views(self)
    }
    /// The `RiskView` adapter of the current state (tests, tools).
    pub fn risk_adapter(&self) -> crate::views::RiskAdapter<'_> {
        crate::views::RiskAdapter { v: self.seq_views() }
    }

    /// The Now of a record of this run.
    pub(crate) fn now_of(&self, tsc: Tsc, seq: u64) -> Now {
        let a = self
            .clock
            .anchor(Tsc(self.seg.calib_tsc), self.seg.calib_realtime_ns);
        Now::new(tsc, seq, self.boot_seq, self.clock_ok, a)
    }

    /// Buffers a Sequencer Note.
    pub(crate) fn seq_note(&mut self, n: u32, level: NoteLevel, group: GroupId, inst: InstId, v: [u64; 8]) {
        let note = Note {
            code: crate::codes::seq_note(n),
            level,
            source: NoteSource::Sequencer,
            strategy: hft_types::StrategyId(0),
            _pad: 0,
            group,
            inst,
            v,
        };
        self.out.push(RecordFlags(0), &note);
    }

    /// Viewer previews held for confirmation (control-plane `VIEWER_CONFIRM_TTL_MS`; at most 16).
    pub fn viewer_preview_count(&self) -> usize {
        self.cmd.previews.len()
    }

    /// An empty `OmsOut` from the pool (nested drains each take their own).
    pub(crate) fn take_oms(&mut self) -> OmsOut {
        self.oms_pool.pop().unwrap_or_default()
    }
    pub(crate) fn put_oms(&mut self, mut o: OmsOut) {
        o.clear();
        self.oms_pool.push(o);
    }
    pub(crate) fn take_exec(&mut self) -> ExecOut {
        self.exec_pool
            .pop()
            .unwrap_or_else(|| ExecOut::with_caps(OutCaps::README))
    }
    pub(crate) fn put_exec(&mut self, mut o: ExecOut) {
        o.clear();
        self.exec_pool.push(o);
    }
    pub(crate) fn take_mode(&mut self) -> ModeOutcome {
        let d = self.mode_dims;
        self.mode_pool
            .pop()
            .unwrap_or_else(|| ModeOutcome::with_capacity(d.0, d.1, d.2))
    }
    pub(crate) fn put_mode(&mut self, mut o: ModeOutcome) {
        o.clear();
        self.mode_pool.push(o);
    }
    pub(crate) fn take_margin(&mut self) -> MarginOut {
        self.margin_pool.pop().unwrap_or_default()
    }
    pub(crate) fn put_margin(&mut self, mut o: MarginOut) {
        o.clear();
        self.margin_pool.push(o);
    }

    /// Account kind helpers.
    pub fn is_sim(&self, a: AccountId) -> bool {
        is_sim(&self.rd, a)
    }

    /// The `BlobRef` of a config blob.
    pub(crate) fn blob_ref(kind: BlobKind, version: u64, bytes: &[u8], sha256: [u8; 32]) -> BlobRef {
        BlobRef {
            kind,
            encoding: 1,
            len: bytes.len() as u32,
            version,
            sha256,
        }
    }
}

/// Empty start: build the components, apply the initial parameters, write the boot block.
pub(crate) fn boot_empty<D: Driver<S>, S: StrategySet>(
    configs: ConfigSet<S>,
    setup: &EngineSetup,
    mut drv: D,
    hb: &'static Heartbeat,
) -> Result<Sequencer<D, S>, BootError> {
    let rd = RefData::load(&configs.refdata_blob, None).map_err(|r| {
        BootError::Config(ConfigErr::Blob {
            kind: BlobKind::Refdata as u16,
            code: r.code() as u16,
        })
    })?;
    let mut e = Engine::new(rd, setup, &configs.risk)?;
    let mut inst = Instances::<S>::new(&e.rd, &e.caps)?;
    let ec = hft_refdata::EngineConfig::load(&configs.engine_blob, None).map_err(|r| {
        BootError::Config(ConfigErr::Blob {
            kind: BlobKind::Engine as u16,
            code: r.code() as u16,
        })
    })?;
    e.cmd.active.refdata = Engine::blob_ref(
        BlobKind::Refdata,
        e.rd.version(),
        &configs.refdata_blob,
        *e.rd.sha256(),
    );
    e.cmd.active.engine =
        Engine::blob_ref(BlobKind::Engine, ec.version(), &configs.engine_blob, *ec.sha256());
    e.cmd.active.risk = Engine::blob_ref(
        BlobKind::RiskParams,
        configs.risk.version,
        &configs.risk_blob,
        configs.risk_sha256,
    );
    e.cmd.blobs.put(e.cmd.active.refdata, &configs.refdata_blob);
    e.cmd.blobs.put(e.cmd.active.engine, &configs.engine_blob);
    e.cmd.blobs.put(e.cmd.active.risk, &configs.risk_blob);
    for (blob, p) in configs.params {
        let r = BlobRef {
            kind: BlobKind::StrategyParams,
            encoding: 1,
            len: blob.len() as u32,
            version: p.version,
            sha256: p.sha256,
        };
        e.cmd.blobs.put(r, &blob);
        let scope = CmdScope {
            strategy: p.strategy,
            venue: VenueId(0),
            account: p.account,
            _pad: 0,
            group: GroupId(0),
            inst: InstId(0),
            _pad2: 0,
        };
        crate::strategies::install_initial(&mut e, &mut inst, &mut drv, Box::new(p), r, scope)?;
    }
    let mut seq = Sequencer {
        e,
        inst,
        drv,
        hb,
        slot: crate::types::InputSlot::with_capacity(1 << 20),
    };
    crate::subs::rebuild_all(&mut seq.e, &mut seq.inst);
    crate::snapshot::write_boot_block(&mut seq, hft_journal::RollReason::Startup, 1)
        .map_err(|_| BootError::Journal(crate::types::JournalErr::Chain))?;
    Ok(seq)
}
