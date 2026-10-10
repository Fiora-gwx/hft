//! `SeqRig<S>` (feature `seq`; README "10. SeqRig"): the real `hft_sequencer::Sequencer` on the
//! live driver over in-memory rings, a manual TSC and the sequencer's `MemJournal`, for tests and
//! fixture generators outside the engine crates that need a hot process's journal and its state
//! at each event boundary (hft-py's ledger fixtures). Inputs are pushed onto rings as the gws,
//! the housekeeping thread and ctl-gateway would; out-ring frames are only collected (no venue
//! answers on its own). Single-threaded, no clock, no random source.

use std::collections::{BTreeMap, VecDeque};

use hft_event::{
    BlobRef, Body, ClockStatus, CmdHdr, CmdResult, ControlCmd, EventKind, FeedCmd, GwCmd, HostProfile,
    RecordFlags,
};
use hft_exec::{ExecAccount, ExecConfig, ExecParams};
use hft_ledger::{Capacities, Ledger, LedgerConfig};
use hft_oms::OmsConfig;
use hft_refdata::{EcfBuilder, EngineSection, RefData};
use hft_risk::{GwMap, RiskParams};
use hft_rt::{CalibPair, Heartbeat};
use hft_sequencer::driver::RollInfo;
use hft_sequencer::driver::live::{LiveCfg, LiveDriver, LiveIo};
use hft_sequencer::driver::replay::Log;
use hft_sequencer::types::InputSlot;
use hft_sequencer::{
    CapacitiesRow, ConfigSet, EngineSetup, MemJournal, ParsedConfig, RingLimit, SegClock, SeqParamsRow,
    Sequencer,
};
use hft_strategy_sdk::{SnapshotWriter, StrategySet, VenueSet};
use hft_types::{AccountId, HostClass, InstId, KillBits, Tsc, VenueId};
use zerocopy::IntoBytes;

pub use hft_sequencer::driver::live::RingId;
pub use hft_sequencer::types::OutRing;
pub use hft_sequencer::{MemRecord, MemSegment, StepOutcome};

/// Rounds one [`SeqRig::run`] may take (a runaway guard).
const MAX_ROUNDS: usize = 100_000;

/// What an empty start of the rig needs. The engine blob (ECF1 with the capacities, sequencer and
/// ledger sections) and the `EngineSetup` are derived from these values.
#[derive(Clone, Debug)]
pub struct RigConfig {
    pub refdata_blob: Vec<u8>,
    /// SPB1 blobs of the initial strategy parameters.
    pub params: Vec<Vec<u8>>,
    pub risk: RiskParams,
    /// The bytes the boot block records for `risk` (any encoding; the rig never decodes it).
    pub risk_blob: Vec<u8>,
    pub caps: Capacities,
    pub ledger: LedgerConfig,
    /// Venues with a gw and a feed in this instance (ring index = position).
    pub venues: Vec<VenueId>,
    /// `(account, venue)` pairs whose trading keys the account-configuration gate expects.
    pub keys: Vec<(AccountId, VenueId)>,
    /// Engine epoch of the run (`OmsConfig::engine_epoch`).
    pub epoch: u32,
    /// TSC frequency (mHz) and the calibration pair of the first segment.
    pub tsc_hz_milli: u64,
    pub calib_tsc: u64,
    pub calib_utc_ns: i64,
    /// TSC at boot (boot-block records).
    pub boot_tsc: u64,
    /// `GroupTable` capacity used when parsing `params`.
    pub max_groups: u16,
}

/// The sequencer parameters of a rig run (generous ring limits; keyframes every 5 min).
pub fn seq_params() -> SeqParamsRow {
    let mut rings = [RingLimit::default(); 8];
    for (i, r) in rings.iter_mut().enumerate().skip(1) {
        *r = RingLimit {
            per_ring: 64,
            total: if i == 4 { 0 } else { 256 },
        };
    }
    SeqParamsRow {
        rings,
        md_round_ns: 50_000,
        ctl_bytes_per_round: 65_536,
        keyframe_period_ms: 300_000,
        keyframe_max_events: 1_500_000,
        advisor_mem_budget_mib: 256,
        advisor_ckpt_max_mib: 1_024,
        sim_max_per_round: 64,
        conflate_backlog: 64,
        notify_per_continue: 64,
        replay_per_round: 1_024,
        auto_resume_after_clean_drain: 1,
        allow_missing_standby: 0,
        _pad: [0; 6],
    }
}

fn caps_row(c: &Capacities) -> CapacitiesRow {
    CapacitiesRow {
        max_insts: c.max_insts,
        max_cells_total: c.max_cells_total,
        max_timers: c.max_timers,
        recent_cmd_ids: c.recent_cmd_ids,
        max_subs_per_inst: c.max_subs_per_inst,
        max_groups_per_instance: c.max_groups_per_instance,
        max_instances: c.max_instances,
        _pad: 0,
    }
}

/// The ECF1 blob of a rig run: capacities (3), sequencer (5) and ledger sections.
pub fn engine_blob(c: &RigConfig) -> Vec<u8> {
    let caps = caps_row(&c.caps);
    let seq = seq_params();
    let mut b = EcfBuilder::new(1, HostClass::Dev2);
    b.created_utc_ns(c.calib_utc_ns);
    b.section(EngineSection::Capacities, 1, 24, 1, caps.as_bytes().to_vec());
    b.section(EngineSection::Sequencer, 1, 72, 1, seq.as_bytes().to_vec());
    let row = c.ledger.to_row();
    b.section(
        EngineSection::Ledger,
        hft_ledger::LEDGER_ROW_VERSION,
        512,
        1,
        row.as_bytes().to_vec(),
    );
    b.finish()
}

fn sha256(b: &[u8]) -> [u8; 32] {
    use sha2::Digest;
    sha2::Sha256::digest(b).into()
}

/// The `EngineSetup` of a rig run.
pub fn engine_setup(c: &RigConfig, rd: &RefData) -> EngineSetup {
    let mut oms = OmsConfig::new(c.epoch, c.tsc_hz_milli);
    oms.max_out_per_event = 16;
    oms.links_per_account = 16;
    oms.slots = hft_oms::PerTable::new(64);
    let accounts = (1..=254u8)
        .filter(|a| rd.account(AccountId(*a)).id == AccountId(*a))
        .map(|a| ExecAccount {
            account: AccountId(a),
            sim: matches!(rd.account(AccountId(a)).kind, hft_refdata::AccountKind::Sim(_)),
            params: None,
        })
        .collect();
    let feeds = c.venues.iter().fold(0u64, |m, v| m | 1u64 << v.0);
    EngineSetup {
        caps: caps_row(&c.caps),
        seq: seq_params(),
        oms,
        ledger: c.ledger.clone(),
        exec: ExecConfig {
            default: ExecParams::default(),
            accounts,
        },
        gws: GwMap {
            venues: c.venues.clone(),
        },
        feeds: VenueSet(feeds),
        keys_required: c.keys.iter().map(|(a, v)| (*a, *v, 3u8)).collect(),
        role_prod: false,
        advisor_max_lag_ms: 5_000,
        drain_timeout_ms: 0,
        risk_decode: None,
        clock: SegClock {
            tsc_hz_milli: c.tsc_hz_milli,
            calib_tsc: c.calib_tsc,
            calib_realtime_ns: c.calib_utc_ns,
        },
        base_engine_ns: 0,
    }
}

/// One queued ring element: kind and body.
pub type Elem = (EventKind, Vec<u8>);

/// The live environment of the rig: in-memory rings, a manual TSC, collected out-ring frames.
pub struct RigIo<S: StrategySet> {
    pub j: MemJournal,
    pub tsc: u64,
    pub rings: BTreeMap<(u8, u8), VecDeque<Elem>>,
    pub n_venues: u8,
    pub out: Vec<(OutRing, GwCmd)>,
    pub mailbox: Vec<ParsedConfig<S>>,
    pub kills: Vec<KillBits>,
    pub calib: CalibPair,
    pub hz: u64,
}

fn key(r: RingId) -> (u8, u8) {
    match r {
        RingId::Ctl => (0, 0),
        RingId::Hk => (1, 0),
        RingId::Rep(i) => (2, i),
        RingId::Chain => (3, 0),
        RingId::Bbo(i) => (4, i),
        RingId::Advice => (5, 0),
        RingId::Md(i) => (6, i),
        RingId::RepSim => (7, 0),
    }
}

impl<S: StrategySet> core::fmt::Debug for RigIo<S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RigIo")
            .field("tsc", &self.tsc)
            .finish_non_exhaustive()
    }
}

impl<S: StrategySet> RigIo<S> {
    fn queued(&self) -> usize {
        self.rings.values().map(VecDeque::len).sum()
    }
}

impl<S: StrategySet> LiveIo<S> for RigIo<S> {
    type J = MemJournal;
    fn journal(&mut self) -> &mut MemJournal {
        &mut self.j
    }
    fn tsc(&mut self) -> Tsc {
        Tsc(self.tsc)
    }
    fn n_rep(&self) -> u8 {
        self.n_venues.max(1)
    }
    fn n_bbo(&self) -> u8 {
        self.n_venues.max(1)
    }
    fn n_md(&self) -> u8 {
        self.n_venues.max(1)
    }
    fn len(&mut self, r: RingId) -> usize {
        self.rings.get(&key(r)).map_or(0, VecDeque::len)
    }
    fn peek_len(&mut self, r: RingId) -> Option<usize> {
        self.rings.get(&key(r)).and_then(|q| q.front()).map(|e| e.1.len())
    }
    fn peek_inst(&mut self, r: RingId) -> Option<InstId> {
        let e = self.rings.get(&key(r))?.front()?;
        (e.0 == EventKind::BBO)
            .then(|| hft_sequencer::types::read_body::<hft_event::Bbo>(&e.1).map_or(InstId(0), |b| b.h.inst))
    }
    fn pop(&mut self, r: RingId, out: &mut InputSlot) -> bool {
        let Some((k, b)) = self.rings.get_mut(&key(r)).and_then(VecDeque::pop_front) else {
            return false;
        };
        out.set_bytes(k, RecordFlags(0), Tsc(self.tsc), &b);
        true
    }
    fn push_out(&mut self, ring: OutRing, c: &GwCmd) -> bool {
        self.out.push((ring, *c));
        true
    }
    fn push_feed(&mut self, _v: VenueId, _c: &FeedCmd) -> bool {
        true
    }
    fn take_parsed(&mut self, r: &BlobRef) -> Option<ParsedConfig<S>> {
        let ix = self.mailbox.iter().position(|p| p.sha256() == &r.sha256)?;
        Some(self.mailbox.remove(ix))
    }
    fn retire(&mut self, _p: ParsedConfig<S>) {}
    fn write_kill(&mut self, bits: &[KillBits]) {
        self.kills = bits.to_vec();
    }
    fn roll_info(&mut self) -> RollInfo {
        RollInfo {
            utc_ns: self.utc_of(Tsc(self.tsc)),
            calib: self.calib,
            tsc_hz_milli: self.hz,
        }
    }
    fn host_profile(&self) -> HostProfile {
        hft_sequencer::snapshot::host_profile(self.hz)
    }
    fn clock_status(&self) -> ClockStatus {
        hft_sequencer::snapshot::clock_status(self.calib, self.hz)
    }
    fn utc_of(&self, t: Tsc) -> i64 {
        let d = i128::from(t.0) - i128::from(self.calib.tsc.0);
        self.calib.realtime_ns + (d * 1_000_000_000_000 / i128::from(self.hz.max(1))) as i64
    }
    fn recovery_log(&mut self) -> Option<Log> {
        None
    }
}

/// The driver of a rig run.
pub type RigDrv<S> = LiveDriver<S, RigIo<S>>;

/// A booted sequencer over [`RigIo`].
pub struct SeqRig<S: StrategySet> {
    s: Sequencer<RigDrv<S>, S>,
    cmd_no: u64,
}

impl<S: StrategySet> core::fmt::Debug for SeqRig<S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SeqRig")
            .field("cmd_no", &self.cmd_no)
            .finish_non_exhaustive()
    }
}

impl<S: StrategySet> SeqRig<S> {
    /// Empty start from `c` (boot block written at `c.boot_tsc`).
    pub fn boot(c: &RigConfig) -> Result<SeqRig<S>, String> {
        let rd = RefData::load(&c.refdata_blob, None).map_err(|e| format!("refdata rejected: {e:?}"))?;
        let setup = engine_setup(c, &rd);
        let mut params = Vec::new();
        for b in &c.params {
            let p = hft_sequencer::parsed::parse_params::<S>(b, sha256(b), &rd, c.max_groups, None)
                .map_err(|d| format!("params rejected: {d:#x}"))?;
            params.push((b.clone(), p));
        }
        let configs = ConfigSet {
            refdata_blob: c.refdata_blob.clone(),
            engine_blob: engine_blob(c),
            risk_sha256: sha256(&c.risk_blob),
            risk_blob: c.risk_blob.clone(),
            risk: c.risk.clone(),
            params,
        };
        let io = RigIo {
            j: MemJournal::new(setup.clock),
            tsc: c.boot_tsc,
            rings: BTreeMap::new(),
            n_venues: c.venues.len() as u8,
            out: Vec::new(),
            mailbox: Vec::new(),
            kills: Vec::new(),
            calib: CalibPair {
                tsc: Tsc(c.calib_tsc),
                realtime_ns: c.calib_utc_ns,
                phc_ns: 0,
                err_ns: 0,
            },
            hz: c.tsc_hz_milli,
        };
        let drv = LiveDriver::new(
            io,
            LiveCfg {
                feed_stage_cap: 1024,
                ..LiveCfg::default()
            },
        );
        let hb: &'static Heartbeat = Box::leak(Box::new(Heartbeat::new()));
        let s = Sequencer::boot(hft_sequencer::BootSource::Empty { configs }, &setup, drv, hb)
            .map_err(|e| format!("boot failed: {e:?}"))?;
        Ok(SeqRig { s, cmd_no: 0 })
    }

    /// The sequencer (read-only).
    pub fn sequencer(&self) -> &Sequencer<RigDrv<S>, S> {
        &self.s
    }
    fn io(&mut self) -> &mut RigIo<S> {
        &mut self.s.driver_mut().io
    }
    /// The current TSC (stamped on every Input handed over).
    pub fn tsc(&self) -> u64 {
        self.s.driver().io.tsc
    }
    pub fn set_tsc(&mut self, t: u64) {
        self.io().tsc = t;
    }
    /// Queues one ring element.
    pub fn push(&mut self, r: RingId, kind: EventKind, body: Vec<u8>) {
        self.io().rings.entry(key(r)).or_default().push_back((kind, body));
    }
    pub fn push_body<B: Body>(&mut self, r: RingId, b: &B) {
        self.push(r, B::KIND, b.as_bytes().to_vec());
    }
    /// Queues a confirmed Admin command (`cli:rig`) on the ctl ring; returns its `cmd_id`.
    pub fn cmd(&mut self, account: AccountId, c: &ControlCmd<'_>) -> Result<[u8; 16], String> {
        self.cmd_no += 1;
        let v = self.s.engine().risk.mode.ctl_state_version();
        let mut h = CmdHdr::zeroed();
        h.cmd_id[..8].copy_from_slice(&self.cmd_no.to_le_bytes());
        h.cmd_id[15] = 0x5E;
        let issuer = b"cli:rig";
        h.issuer[..issuer.len()].copy_from_slice(issuer);
        h.role = CmdHdr::ROLE_ADMIN;
        h.account = account;
        h.cmd_ver = 1;
        h.confirmed = 1;
        h.expect_state_version = v;
        let mut buf = vec![0u8; 8_192];
        let n = c.encode(&h, &mut buf).map_err(|e| format!("encode: {e:?}"))?;
        buf.truncate(n);
        let id = h.cmd_id;
        self.push(RingId::Ctl, EventKind::CONTROL_CMD, buf);
        Ok(id)
    }
    /// Steps until a round is idle and nothing is queued; a fatal outcome is an error.
    pub fn run(&mut self) -> Result<(), String> {
        for _ in 0..MAX_ROUNDS {
            match self.s.step() {
                StepOutcome::Busy => {}
                StepOutcome::Idle => {
                    if self.s.driver().io.queued() == 0 {
                        return Ok(());
                    }
                }
                o => return Err(format!("sequencer stopped: {o:?}")),
            }
        }
        Err("sequencer did not go idle".into())
    }
    /// The journal (every record, published or not).
    pub fn journal(&self) -> &MemJournal {
        &self.s.driver().io.j
    }
    /// Out-ring frames pushed so far.
    pub fn out(&self) -> &[(OutRing, GwCmd)] {
        &self.s.driver().io.out
    }
    /// The hot ledger.
    pub fn ledger(&self) -> &Ledger {
        &self.s.engine().ledger
    }
    /// The hot ledger's snapshot sections (1, 13, 22) as `Ledger::snapshot` writes them.
    pub fn ledger_sections(&self) -> Vec<u8> {
        let mut w = SnapshotWriter::new(64 << 20);
        self.s.engine().ledger.snapshot(&mut w);
        w.as_bytes().to_vec()
    }
    pub fn epoch(&self) -> u32 {
        self.s.view().engine_epoch()
    }
    /// The `CmdResult` of a command in the journal.
    pub fn result_of(&self, id: [u8; 16]) -> Option<CmdResult> {
        self.journal()
            .records()
            .filter(|r| r.kind == EventKind::CMD_RESULT)
            .filter_map(|r| r.read::<CmdResult>())
            .find(|c| c.cmd_id == id)
    }
}
