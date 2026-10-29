//! `ResearchDriver` (README "驱动"): logged Inputs in journal order, the masked accounts' logged
//! reports replaced by the simulated venue's, engine timers fired from the timer heap, all merged
//! by event time on one thread without a wall clock.

use std::collections::VecDeque;

use hft_event::{BlobHdr, BlobKind, BlobRef, ClockStatus, EventKind, HostProfile, RecordFlags, Role, Timer};
use hft_journal::{AppendErr, RollAt, RollErr, RollReason};
use hft_rt::CalibPair;
use hft_sequencer::driver::{Driver, DriverMode, DriverView, ParseCx, RollInfo, replay::Log};
use hft_sequencer::journal::{BootSink, JournalSink, RollMeta, role_of};
use hft_sequencer::parsed::{ParsedConfig, parse_blob};
use hft_sequencer::{ConfigErr, EmitResult, InputSlot, MemRecord, OutRecord, SegClock};
use hft_strategy_sdk::StrategySet;
use hft_types::{GroupId, KillBits, Seq, Tsc};
use sha2::{Digest, Sha256};
use zerocopy::IntoBytes;

use crate::source::{Rec, Source, SourceErr};
use crate::venue::{ResearchVenue, SimInput, masked_kind, report_account};

/// Output kinds the sink keeps for the run's tables.
fn captured_kind(k: EventKind) -> bool {
    matches!(k, EventKind::ORDER_DECISION | EventKind::CMD_RESULT)
}

/// One Output of the replay, stamped with the engine clock of its event.
#[derive(Clone, Debug)]
pub struct Captured {
    pub seq: u64,
    pub kind: EventKind,
    pub flags: RecordFlags,
    pub tsc: u64,
    pub body: Vec<u8>,
    pub utc_ns: i64,
    pub engine_ns: i64,
}

/// The replay's journal: assigns fresh, increasing seqs to every record (logged Inputs keep their
/// order, inserted Inputs get the next number) and keeps the Outputs of `captured_kind`.
#[derive(Debug, Default)]
pub struct ResearchSink {
    seq: u64,
    /// Logged seq of the Input being handed (0 = inserted by the replay).
    pub handed_log_seq: u64,
    /// `(replay seq, logged seq)` of every logged Input.
    pub seq_map: Vec<(u64, u64)>,
    pub captured: Vec<Captured>,
    /// Index of the first capture not yet stamped with the engine clock.
    stamped: usize,
}

impl ResearchSink {
    fn next(&mut self) -> Seq {
        self.seq += 1;
        Seq(self.seq)
    }
    /// Logged seq of replay seq `s` (an Input of the journal), 0 otherwise.
    pub fn log_seq(&self, s: u64) -> u64 {
        self.seq_map
            .binary_search_by_key(&s, |p| p.0)
            .ok()
            .and_then(|i| self.seq_map.get(i))
            .map_or(0, |p| p.1)
    }
}

/// Boot block of a roll: every record takes the next seq.
#[derive(Debug)]
pub struct SinkBoot<'a> {
    s: &'a mut ResearchSink,
}

impl BootSink for SinkBoot<'_> {
    fn append_var(&mut self, _kind: EventKind, _tsc: Tsc, _pieces: &[&[u8]]) -> Result<Seq, AppendErr> {
        Ok(self.s.next())
    }
    fn finish(self) -> Result<Seq, AppendErr> {
        Ok(self.s.next())
    }
}

impl JournalSink for ResearchSink {
    type Boot<'a> = SinkBoot<'a>;
    fn append_var(
        &mut self,
        kind: EventKind,
        flags: RecordFlags,
        tsc: Tsc,
        pieces: &[&[u8]],
    ) -> Result<Seq, AppendErr> {
        let s = self.next();
        let body: Vec<u8>;
        let role = if captured_kind(kind) || pieces.len() != 1 {
            body = pieces.concat();
            role_of(kind, &body)
        } else {
            body = Vec::new();
            role_of(kind, pieces[0])
        };
        match role {
            Role::Input => {
                if self.handed_log_seq != 0 {
                    self.seq_map.push((s.0, self.handed_log_seq));
                    self.handed_log_seq = 0;
                }
            }
            Role::Output if captured_kind(kind) => self.captured.push(Captured {
                seq: s.0,
                kind,
                flags,
                tsc: tsc.0,
                body,
                utc_ns: 0,
                engine_ns: 0,
            }),
            _ => {}
        }
        Ok(s)
    }
    fn end_event(&mut self) {}
    fn publish(&mut self) {}
    fn roll(&mut self, _reason: RollReason, _info: &RollMeta<'_>) -> Result<SinkBoot<'_>, RollErr> {
        Ok(SinkBoot { s: self })
    }
    fn needs_roll(&self, _now: Tsc) -> Option<(RollReason, RollAt)> {
        None
    }
}

/// Where a handed Input came from.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Origin {
    /// A journal Input; its logged seq.
    Log(u64),
    /// A report of the simulated venue.
    Sim,
    /// A timer or continuation fired from the engine's own state.
    Engine,
    /// The variant's `ConfigBlob` / `SetParams`.
    Variant,
}

#[derive(Clone, Debug)]
struct Pending {
    kind: EventKind,
    flags: RecordFlags,
    tsc: u64,
    body: Vec<u8>,
    origin: Origin,
}

/// Counters of one run (README "运行报告").
#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub struct DriverStats {
    pub log_inputs: u64,
    pub log_meta: u64,
    pub log_outputs_skipped: u64,
    pub masked_reports_dropped: u64,
    pub log_timers_dropped: u64,
    pub engine_timers: u64,
    pub sim_inputs: u64,
    pub variant_inputs: u64,
}

/// What the driver does with the journal's records.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct DriverCfg {
    /// Fire heap timers and OMS / notification continuations from the engine's state and drop the
    /// journal's heap-timer Inputs (owner ≠ Sys): the replayed order flow differs from the logged.
    pub engine_timers: bool,
    /// Inputs per round.
    pub per_round: u16,
}

impl Default for DriverCfg {
    fn default() -> Self {
        DriverCfg {
            engine_timers: true,
            per_round: 64,
        }
    }
}

/// The research driver.
pub struct ResearchDriver {
    /// Records from the journal start through the boot snapshot's event (boot reads them).
    prefix: Log,
    /// Next prefix record to hand (set by `start_at`).
    pre_cursor: usize,
    src: Source,
    /// Segment index (into `src.segs`) of the last handed journal record.
    cur_seg: usize,
    next_seg: Option<(SegClock, Vec<MemRecord>)>,
    pub venue: Option<ResearchVenue>,
    queue: VecDeque<Pending>,
    pub sink: ResearchSink,
    cfg: DriverCfg,
    taken: u16,
    cur_tsc: u64,
    last_tsc: u64,
    cont_checked: bool,
    /// `ConfigBlob` records seen (prefix and stream), newest last; plus the variants' blobs.
    blobs: Vec<(BlobRef, Vec<u8>)>,
    pub stats: DriverStats,
    pub err: Option<SourceErr>,
    eof: bool,
    /// The simulated venue's order events and deliveries, stamped like `sink.captured`.
    pub obs: Vec<Captured>,
    /// The journal's own `OrderDecision` Outputs after the start snapshot (`parity`).
    pub logged: Vec<Captured>,
    obs_stamped: usize,
    /// `(account, venue, asset)` whose first logged `BalanceReport` has been handed: it
    /// initialises the ledger's cash (hft-ledger `UNINIT`); later ones are account truth of the
    /// live run and are dropped.
    balance_seen: std::collections::BTreeSet<(u8, u8, u16)>,
}

impl core::fmt::Debug for ResearchDriver {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ResearchDriver")
            .field("stats", &self.stats)
            .finish_non_exhaustive()
    }
}

fn blob_of(r: &MemRecord) -> Option<(BlobRef, Vec<u8>)> {
    if r.kind != EventKind::CONFIG_BLOB {
        return None;
    }
    let h = r.read::<BlobHdr>()?;
    let content = r.body.get(72..72 + h.blob.len as usize)?.to_vec();
    Some((h.blob, content))
}

/// Reads the journal from its start through the end of the first `StateSnapshot`'s event.
pub fn read_prefix(src: &mut Source) -> Result<Log, SourceErr> {
    let mut log = Log::default();
    let mut snap = false;
    let mut seg = usize::MAX;
    while let Some(Rec { r, seg: s }) = src.next_rec()? {
        if s != seg {
            seg = s;
            log.seg_start.push(log.recs.len());
            log.segs.push(src.segs[s]);
        }
        snap |= r.kind == EventKind::STATE_SNAPSHOT;
        let end = r.flags.0 & RecordFlags::EVENT_END.0 != 0;
        log.recs.push(r);
        if snap && end {
            return Ok(log);
        }
    }
    if log.recs.is_empty() {
        Err(SourceErr::Empty)
    } else {
        Err(SourceErr::Body {
            seq: 0,
            what: "no complete StateSnapshot".into(),
        })
    }
}

impl ResearchDriver {
    pub fn new(prefix: Log, src: Source, cfg: DriverCfg) -> ResearchDriver {
        let blobs = prefix.recs.iter().filter_map(blob_of).collect();
        let seq = prefix.recs.last().map_or(0, |r| r.seq);
        let cur_seg = src.segs.len().saturating_sub(1);
        ResearchDriver {
            prefix,
            pre_cursor: usize::MAX,
            src,
            cur_seg,
            next_seg: None,
            venue: None,
            queue: VecDeque::with_capacity(256),
            sink: ResearchSink {
                seq,
                ..ResearchSink::default()
            },
            cfg,
            taken: 0,
            cur_tsc: 0,
            last_tsc: 0,
            cont_checked: true,
            blobs,
            stats: DriverStats::default(),
            err: None,
            eof: false,
            obs: Vec::new(),
            logged: Vec::new(),
            obs_stamped: 0,
            balance_seen: Default::default(),
        }
    }

    /// The journal is consumed and nothing is queued.
    pub fn finished(&self) -> bool {
        self.eof && self.queue.is_empty()
    }

    /// Content of a blob reference: the variants' and the journal's `ConfigBlob`s, SHA-256 checked.
    pub fn blob(&self, r: &BlobRef) -> Option<&[u8]> {
        self.blobs.iter().rev().find_map(|(b, c)| {
            (b == r && <[u8; 32]>::from(Sha256::digest(c)) == r.sha256).then_some(c.as_slice())
        })
    }

    /// Queues the variant's `ConfigBlob` and `SetParams` as the first Inputs after boot.
    pub fn push_variant(
        &mut self,
        blob: BlobRef,
        content: Vec<u8>,
        blob_input: Vec<u8>,
        cmd: Vec<u8>,
        tsc: u64,
    ) {
        self.blobs.push((blob, content));
        for (kind, body) in [
            (EventKind::CONFIG_BLOB, blob_input),
            (EventKind::CONTROL_CMD, cmd),
        ] {
            self.queue.push_back(Pending {
                kind,
                flags: RecordFlags(0),
                tsc,
                body,
                origin: Origin::Variant,
            });
        }
    }

    /// Record-header tsc of the boot snapshot's event (the replay's starting time).
    pub fn start_tsc(&self) -> u64 {
        self.prefix.recs.last().map_or(0, |r| r.tsc)
    }

    fn timer(owner: u16, id: u32, group: GroupId, deadline: u64, now: u64) -> Pending {
        let t = Timer {
            timer_id: id,
            owner,
            _pad: 0,
            group,
            _pad2: 0,
            deadline_tsc: deadline,
        };
        Pending {
            kind: EventKind::TIMER,
            flags: RecordFlags(0),
            tsc: now,
            body: t.as_bytes().to_vec(),
            origin: Origin::Engine,
        }
    }

    fn sim(s: SimInput, last: u64) -> Pending {
        Pending {
            kind: s.kind,
            flags: RecordFlags(0),
            tsc: s.due.0.max(last),
            body: s.body,
            origin: Origin::Sim,
        }
    }

    /// The next journal record after the boot snapshot (prefix tail first) without consuming it.
    fn peek_log_tsc(&mut self) -> Option<u64> {
        if let Some(r) = self.prefix.recs.get(self.pre_cursor) {
            return Some(r.tsc);
        }
        match self.src.peek() {
            Ok(r) => r.map(|r| r.r.tsc),
            Err(e) => {
                self.err = Some(e);
                None
            }
        }
    }

    fn next_log(&mut self) -> Option<MemRecord> {
        if let Some(r) = self.prefix.recs.get(self.pre_cursor) {
            self.pre_cursor += 1;
            return Some(r.clone());
        }
        match self.src.next_rec() {
            Ok(Some(Rec { r, seg })) => {
                if seg != self.cur_seg {
                    self.cur_seg = seg;
                    self.next_seg = None;
                }
                Some(r)
            }
            Ok(None) => None,
            Err(e) => {
                self.err = Some(e);
                None
            }
        }
    }

    /// What happens to one journal record: `None` = not handed.
    fn admit(&mut self, r: MemRecord) -> Option<Pending> {
        let role = r.role();
        if role == Role::Output {
            self.stats.log_outputs_skipped += 1;
            if r.kind == EventKind::ORDER_DECISION {
                self.logged.push(Captured {
                    seq: r.seq,
                    kind: r.kind,
                    flags: r.flags,
                    tsc: r.tsc,
                    body: r.body,
                    utc_ns: 0,
                    engine_ns: 0,
                });
            }
            return None;
        }
        if let Some(venue) = &mut self.venue {
            if masked_kind(r.kind) && report_account(r.kind, &r.body).is_some_and(|a| venue.masked(a)) {
                let first_balance = r.kind == EventKind::BALANCE_REPORT
                    && r.read::<hft_event::BalanceReport>()
                        .is_some_and(|b| self.balance_seen.insert((b.account.0, b.venue.0, b.asset.0)));
                if !first_balance {
                    self.stats.masked_reports_dropped += 1;
                    return None;
                }
            }
            if self.cfg.engine_timers
                && r.kind == EventKind::TIMER
                && r.read::<Timer>().is_some_and(|t| t.owner != 1)
            {
                self.stats.log_timers_dropped += 1;
                return None;
            }
            venue.on_record(&r);
        }
        if let Some(b) = blob_of(&r) {
            self.blobs.push(b);
        }
        if role == Role::Meta {
            self.stats.log_meta += 1;
        } else {
            self.stats.log_inputs += 1;
        }
        Some(Pending {
            kind: r.kind,
            flags: RecordFlags(
                r.flags.0 & !(RecordFlags::EVENT_END.0 | RecordFlags::CRC.0 | RecordFlags::PART.0),
            ),
            tsc: r.tsc,
            body: r.body,
            origin: Origin::Log(r.seq),
        })
    }

    fn stamp(&mut self, v: &DriverView<'_>) {
        let now = v.engine().now;
        for c in self.sink.captured.iter_mut().skip(self.sink.stamped) {
            c.utc_ns = now.utc_ns;
            c.engine_ns = now.engine_ns;
        }
        self.sink.stamped = self.sink.captured.len();
        for c in self.obs.iter_mut().skip(self.obs_stamped) {
            let a = e_anchor(v, c.tsc);
            c.utc_ns = a.utc_of(Tsc(c.tsc));
            c.engine_ns = a.ens_of(Tsc(c.tsc));
        }
        self.obs_stamped = self.obs.len();
    }

    /// Engine UTC of a TSC of this run (the engine's own clock line).
    pub fn utc_at(v: &DriverView<'_>, t: u64) -> i64 {
        e_anchor(v, t).utc_of(Tsc(t))
    }

    fn observe(&mut self, s: &SimInput) {
        if matches!(s.kind, EventKind::ORDER_EVENT | EventKind::EXTERNAL_FILL) {
            self.obs.push(Captured {
                seq: 0,
                kind: s.kind,
                flags: RecordFlags(0),
                tsc: s.due.0,
                body: s.body.clone(),
                utc_ns: 0,
                engine_ns: 0,
            });
        }
    }

    fn hand(&mut self, out: &mut InputSlot, p: Pending) {
        out.set_bytes(p.kind, p.flags, Tsc(p.tsc), &p.body);
        match p.origin {
            Origin::Log(s) => self.sink.handed_log_seq = s,
            Origin::Sim => {
                self.sink.handed_log_seq = 0;
                self.stats.sim_inputs += 1;
            }
            Origin::Engine => {
                self.sink.handed_log_seq = 0;
                self.stats.engine_timers += 1;
            }
            Origin::Variant => {
                self.sink.handed_log_seq = 0;
                self.stats.variant_inputs += 1;
            }
        }
        self.cur_tsc = p.tsc;
        self.last_tsc = self.last_tsc.max(p.tsc);
        self.taken += 1;
        self.cont_checked = false;
    }
}

impl<S: StrategySet> Driver<S> for ResearchDriver {
    type J = ResearchSink;
    fn begin_round(&mut self, _v: &DriverView<'_>) {
        self.taken = 0;
    }
    fn take(&mut self, v: &DriverView<'_>, out: &mut InputSlot) -> bool {
        self.stamp(v);
        if self.taken >= self.cfg.per_round || self.err.is_some() {
            return false;
        }
        loop {
            if let Some(p) = self.queue.pop_front() {
                self.hand(out, p);
                return true;
            }
            let last = self.last_tsc;
            if self.cfg.engine_timers && !self.cont_checked {
                self.cont_checked = true;
                if v.oms_wants_continue() {
                    self.queue.push_back(Self::timer(3, 1, GroupId(0), last, last));
                    continue;
                }
                if v.notify_wants_continue() {
                    self.queue.push_back(Self::timer(1, 17, GroupId(0), last, last));
                    continue;
                }
            }
            let Some(t_log) = self.peek_log_tsc() else {
                self.eof = true;
                return false;
            };
            let heap = if self.cfg.engine_timers && self.venue.is_some() {
                v.heap_top().filter(|h| h.deadline <= t_log)
            } else {
                None
            };
            let t = heap.map_or(t_log, |h| h.deadline.max(last));
            if let Some(venue) = &mut self.venue {
                venue.advance(Tsc(t));
                let mut due = Vec::new();
                venue.due(Tsc(t), (v.engine_epoch() & 0x7fff) as u16, &mut due);
                if !due.is_empty() {
                    for s in due {
                        self.observe(&s);
                        self.queue.push_back(Self::sim(s, last));
                    }
                    continue;
                }
            }
            if let Some(h) = heap {
                self.queue
                    .push_back(Self::timer(h.owner, h.timer_id, GroupId(h.group), h.deadline, t));
                continue;
            }
            let Some(r) = self.next_log() else {
                self.eof = true;
                return false;
            };
            if let Some(p) = self.admit(r) {
                self.queue.push_back(p);
            }
        }
    }
    fn run_tsc(&mut self) -> Option<Tsc> {
        None
    }
    fn measure(&mut self) -> Tsc {
        Tsc(self.cur_tsc)
    }
    fn emit(&mut self, o: &OutRecord<'_>) -> EmitResult {
        let Some(venue) = &mut self.venue else {
            return EmitResult::Skipped;
        };
        match o {
            OutRecord::Out(_, c) => venue.on_command(c, c.decided_tsc),
            OutRecord::Pair(p, _) => {
                for (_, c) in p.iter() {
                    venue.on_command(c, c.decided_tsc);
                }
            }
            OutRecord::FeedCtl(..) => return EmitResult::Skipped,
        }
        let last = self.last_tsc;
        for s in venue.take_immediate() {
            self.observe(&s);
            self.queue.push_back(Self::sim(s, last));
        }
        EmitResult::Pushed
    }
    fn take_parsed(&mut self, r: &BlobRef, cx: &ParseCx<'_, S>) -> Result<ParsedConfig<S>, ConfigErr> {
        let content = self.blob(r).ok_or(ConfigErr::Missing)?;
        parse_blob::<S>(
            r,
            content,
            cx.rd,
            cx.max_groups,
            cx.base,
            cx.risk_account,
            cx.risk_decode,
        )
        .map_err(ConfigErr::Params)
    }
    fn retire(&mut self, _old: ParsedConfig<S>) {}
    fn journal(&mut self) -> &mut ResearchSink {
        &mut self.sink
    }
    fn mode(&self) -> DriverMode {
        DriverMode::Verify
    }
    fn write_kill(&mut self, _bits: &[KillBits]) {}
    fn roll_info(&mut self) -> RollInfo {
        if self.next_seg.is_none() {
            match self.src.next_segment(self.cur_seg) {
                Ok(n) => self.next_seg = n,
                Err(e) => self.err = Some(e),
            }
        }
        let c = self
            .next_seg
            .as_ref()
            .map(|n| n.0)
            .or_else(|| self.src.segs.get(self.cur_seg).copied())
            .unwrap_or_default();
        RollInfo {
            utc_ns: c.calib_realtime_ns,
            calib: CalibPair {
                tsc: Tsc(c.calib_tsc),
                realtime_ns: c.calib_realtime_ns,
                phc_ns: 0,
                err_ns: 0,
            },
            tsc_hz_milli: c.tsc_hz_milli,
        }
    }
    fn host_profile(&self) -> HostProfile {
        hft_sequencer::snapshot::host_profile(0)
    }
    fn clock_status(&self) -> ClockStatus {
        self.next_seg
            .as_ref()
            .and_then(|n| {
                n.1.iter()
                    .find(|r| r.kind == EventKind::CLOCK_STATUS)
                    .and_then(|r| r.read::<ClockStatus>())
            })
            .unwrap_or_else(|| hft_sequencer::snapshot::clock_status(CalibPair::default(), 0))
    }
    fn start_log(&mut self) -> Option<Log> {
        Some(self.prefix.clone())
    }
    fn start_at(&mut self, ix: usize) {
        self.pre_cursor = ix;
    }
}

/// The engine clock anchor at TSC `t` (its UTC line, or the segment's calibration pair).
fn e_anchor(v: &DriverView<'_>, t: u64) -> hft_strategy_sdk::EngineClockAnchor {
    let e = v.engine();
    e.clock
        .anchor_at(Tsc(t), Tsc(e.seg.calib_tsc), e.seg.calib_realtime_ns)
}

/// The `BlobHdr` + content of a variant `ConfigBlob` Input.
pub fn blob_input(r: BlobRef, scope: hft_event::CmdScope, content: &[u8]) -> Vec<u8> {
    let h = BlobHdr {
        blob: r,
        scope,
        boot: 0,
        _pad: [0; 7],
    };
    let mut b = h.as_bytes().to_vec();
    b.extend_from_slice(content);
    b
}

/// Kind of a blob reference (for diagnostics).
pub fn is_params(r: &BlobRef) -> bool {
    r.kind == BlobKind::StrategyParams
}
