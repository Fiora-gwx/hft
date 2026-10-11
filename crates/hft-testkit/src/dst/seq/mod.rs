//! `SeqSut<S>`: the engine adapter of the DST (README "6. DST 框架", "DST 的世界模型" 2). The real
//! `hft_sequencer::Sequencer` runs single-threaded on the live driver over [`io::DstIo`], whose
//! clock is the world's virtual time; the world's venues match on `hft_sim::SimExchange`
//! ([`matcher::SimMatcher`]); `observe` copies the components' state into [`Obs`] and each step's
//! journal records are described in [`StepInfo`] ([`observe`]).
//!
//! Boot = empty start from the world configuration plus a fixed prologue of Inputs that brings
//! every Live account through its startup gates (connections Up, `ClockOk`, account
//! configuration, reconciliation summary, kill self-test, balances, quotes, `SetMode`), the same
//! Inputs hft-core's environment produces at startup. Crash = drop the sequencer mid-run, keep
//! the published journal, cold-start from it (`BootSource::Journal`), then the restart prologue
//! (connections, `ClockOk`, kill self-test).

pub mod audit;
pub mod build;
mod decisions;
mod events;
pub mod io;
pub mod matcher;
pub mod observe;
mod state;

use hft_event::{
    AccountConfigReportHdr, BalanceReport, Bbo, CmdHdr, ConnRole, ConnState, ConnStateKind, ControlCmd,
    EventKind, LeverageItem, MarginReportHdr, MarketHdr, ReconcileReport, Timer,
};
use hft_refdata::RefData;
use hft_rt::Heartbeat;
use hft_sequencer::driver::live::{LiveCfg, LiveDriver, RingId};
use hft_sequencer::driver::replay::{Log, ReplayDriver, ReplayMode};
use hft_sequencer::{
    BootSource, EngineSetup, ParsedConfig, RecoverPlan, RecoveryFlags, SegClock, Sequencer, SnapshotSel,
    StepOutcome,
};
use hft_strategy_sdk::{SnapshotWriter, StrategySet};
use hft_types::{AccountId, GroupId, InstId, MarketKind, Notional, Px, Qty, Rate, RunMode, Tsc, VenueId};
use zerocopy::{FromZeros, IntoBytes};

use crate::dst::driver::DstDriver;
use crate::dst::obs::Obs;
use crate::dst::sut::{BootCtx, BootFail, CrashPoint, InputRec, JRec, Source, StepOut, Sut};
use crate::dst::world::{AccountKind, WorldSpec};
use crate::hash::ComponentHashes;

pub use matcher::SimMatcher;

/// The live driver of a DST engine.
pub type SeqDrv<S> = LiveDriver<S, io::DstIo<S>>;

/// Rounds one `process` / `fire_timers` call may take at most (a runaway guard).
const MAX_ROUNDS: usize = 10_000;

/// Planted defects the real engine crates understand (`planted-<name>` features). None are built
/// yet: a run that asks for one fails to boot.
pub const PLANTED: &[(&str, u8)] = &[];

/// The engine adapter.
pub struct SeqSut<S: StrategySet> {
    s: Option<Sequencer<SeqDrv<S>, S>>,
    world: WorldSpec,
    setup: EngineSetup,
    tsc_hz: u64,
    jrecs: Vec<JRec>,
    epoch: u32,
    cmd_no: u64,
    hb: &'static Heartbeat,
    /// Observation state carried across steps (`OmsHedgeNakedCapped` sums).
    pub(crate) carry: observe::Carry,
}

impl<S: StrategySet> core::fmt::Debug for SeqSut<S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SeqSut")
            .field("epoch", &self.epoch)
            .finish_non_exhaustive()
    }
}

fn ring_of(io: &io::DstIo<impl StrategySet>, i: &InputRec) -> RingId {
    let k = i.kind;
    if k == EventKind::CONTROL_CMD || k == EventKind::CONFIG_BLOB {
        return RingId::Ctl;
    }
    if k == EventKind::ADVICE {
        return RingId::Advice;
    }
    match i.src {
        Source::Hk => RingId::Hk,
        Source::Ctl => RingId::Ctl,
        Source::Advisor => RingId::Advice,
        Source::Sim => RingId::RepSim,
        Source::Venue(v) => RingId::Rep(io.venue_ix(v)),
        Source::Recon | Source::Engine => RingId::Rep(0),
        Source::Market => {
            let v = <MarketHdr as zerocopy::FromBytes>::read_from_prefix(&i.body)
                .map_or(VenueId(0), |(h, _)| h.venue);
            RingId::Bbo(io.venue_ix(v))
        }
    }
}

/// Offset of the receive stamp (`rx_tsc`) in the body of an Input kind the gws stamp.
fn rx_tsc_offset(k: EventKind) -> Option<usize> {
    Some(match k {
        EventKind::BBO
        | EventKind::TRADE
        | EventKind::FUNDING
        | EventKind::MARK_PRICE
        | EventKind::INDEX_PRICE
        | EventKind::L2_DELTA
        | EventKind::L2_SNAPSHOT
        | EventKind::L2_BATCH => 32,
        EventKind::ORDER_EVENT => 80,
        EventKind::POSITION_REPORT | EventKind::MARGIN_REPORT | EventKind::ACCOUNT_CONFIG_REPORT => 48,
        EventKind::BALANCE_REPORT => 32,
        EventKind::FUNDING_SETTLE => 40,
        EventKind::EXTERNAL_FILL => 64,
        EventKind::CONN_STATE => 16,
        _ => return None,
    })
}

/// The gw's receive stamp: a zero `rx_tsc` of a venue / market Input becomes the delivery TSC
/// (the world's actors produce exchange-side bodies; the gw stamps reception).
fn stamp_rx(k: EventKind, body: &mut [u8], tsc: u64) {
    if let Some(o) = rx_tsc_offset(k)
        && let Some(f) = body.get_mut(o..o + 8)
        && f.iter().all(|b| *b == 0)
    {
        f.copy_from_slice(&tsc.to_le_bytes());
    }
}

impl<S: StrategySet> SeqSut<S> {
    fn s(&self) -> &Sequencer<SeqDrv<S>, S> {
        match self.s.as_ref() {
            Some(s) => s,
            None => panic!("SeqSut: no engine (failed restart)"),
        }
    }
    fn s_mut(&mut self) -> &mut Sequencer<SeqDrv<S>, S> {
        match self.s.as_mut() {
            Some(s) => s,
            None => panic!("SeqSut: no engine (failed restart)"),
        }
    }
    fn io(&mut self) -> &mut io::DstIo<S> {
        &mut self.s_mut().driver_mut().io
    }
    /// The running sequencer (scenario assertions).
    pub fn sequencer(&self) -> &Sequencer<SeqDrv<S>, S> {
        self.s()
    }
    pub fn world(&self) -> &WorldSpec {
        &self.world
    }
    pub fn tsc_hz(&self) -> u64 {
        self.tsc_hz
    }
    fn set_now(&mut self, now_ns: u64) {
        self.io().drv.set_now(now_ns);
    }
    fn now_ns(&self) -> u64 {
        self.s().driver().io.drv.now_ns()
    }
    /// Virtual ns of a TSC (inverse of `tsc_at` with start 0), rounded up.
    fn ns_of(&self, tsc: u64) -> u64 {
        let hz = u128::from(self.tsc_hz.max(1));
        let ns = (u128::from(tsc) * 1_000_000_000).div_ceil(hz);
        u64::try_from(ns).unwrap_or(u64::MAX)
    }
    /// Steps until a round is idle and nothing is queued.
    fn run(&mut self) -> StepOutcome {
        let mut last = StepOutcome::Idle;
        for _ in 0..MAX_ROUNDS {
            last = self.s_mut().step();
            let io = self.io();
            io.audit.commit(io::committed_seq(&io.j));
            self.record_state();
            match last {
                StepOutcome::Busy => {}
                StepOutcome::Idle => {
                    if self.s().driver().io.queued() == 0 {
                        break;
                    }
                }
                _ => break,
            }
        }
        last
    }
    /// Copies the journal records appended since the last call into the mirror.
    fn mirror(&mut self, out: Option<&mut Vec<JRec>>) {
        let have = self.jrecs.len();
        let j = &self.s().driver().io.j;
        let new: Vec<JRec> = j
            .records()
            .skip(have)
            .map(|r| JRec {
                seq: r.seq,
                tsc: r.tsc,
                kind: r.kind,
                flags: r.flags,
                body: r.body.clone(),
            })
            .collect();
        if let Some(o) = out {
            o.extend(new.iter().cloned());
        }
        self.jrecs.extend(new);
    }
    fn remirror(&mut self) {
        self.jrecs.clear();
        self.mirror(None);
    }
    fn push<B: hft_event::Body>(&mut self, r: RingId, b: &B) {
        self.io().push(r, B::KIND, b.as_bytes().to_vec());
    }
    fn push_bytes(&mut self, r: RingId, kind: EventKind, body: Vec<u8>) {
        self.io().push(r, kind, body);
    }
    /// A confirmed Admin command from `cli:dst` on the ctl ring.
    fn cmd(&mut self, account: AccountId, c: &ControlCmd<'_>) {
        self.cmd_no += 1;
        let v = self.s().engine().risk.mode.ctl_state_version();
        let mut h = CmdHdr::new_zeroed();
        h.cmd_id[..8].copy_from_slice(&self.cmd_no.to_le_bytes());
        h.cmd_id[15] = 0xD5;
        let issuer = b"cli:dst";
        h.issuer[..issuer.len()].copy_from_slice(issuer);
        h.role = 4;
        h.account = account;
        h.cmd_ver = 1;
        h.confirmed = 1;
        h.expect_state_version = v;
        let mut buf = vec![0u8; 8_192];
        if let Ok(n) = c.encode(&h, &mut buf) {
            buf.truncate(n);
            self.push_bytes(RingId::Ctl, EventKind::CONTROL_CMD, buf);
        }
    }
    fn sys(&mut self, id: u32) {
        let t = self.s().driver().io.drv.stamp();
        let tm = Timer {
            timer_id: id,
            owner: 1,
            _pad: 0,
            group: GroupId(0),
            _pad2: 0,
            deadline_tsc: t,
        };
        self.push(RingId::Hk, &tm);
    }
    fn conn(&mut self, r: RingId, v: VenueId, k: u8, role: ConnRole, inst: InstId, a: AccountId) {
        let t = self.s().driver().io.drv.stamp();
        let c = ConnState {
            venue: v,
            conn: k,
            state: ConnStateKind::Up,
            role,
            inst,
            reason: 0,
            account: a,
            _pad2: [0; 3],
            rx_tsc: t,
        };
        self.push(r, &c);
    }
    fn live_accounts(&self) -> Vec<(AccountId, Vec<VenueId>)> {
        self.world
            .accounts
            .iter()
            .filter(|a| a.kind == AccountKind::Live)
            .map(|a| (a.id, a.venues.clone()))
            .collect()
    }
    /// Connections Up for every md feed, inst, Live order and user-stream connection.
    fn conns_up(&mut self) {
        let venues: Vec<VenueId> = self.world.venues.iter().map(|v| v.id).collect();
        for v in &venues {
            let r = RingId::Rep(self.s().driver().io.venue_ix(*v));
            self.conn(r, *v, 0, ConnRole::Md, InstId(0), AccountId(0));
            let insts: Vec<InstId> = self
                .world
                .insts
                .iter()
                .filter(|i| i.venue == *v)
                .map(|i| i.id)
                .collect();
            for i in insts {
                self.conn(r, *v, 0, ConnRole::Md, i, AccountId(0));
            }
        }
        for a in &self.world.accounts.clone() {
            for v in &a.venues {
                let r = if a.kind == AccountKind::Sim {
                    RingId::RepSim
                } else {
                    RingId::Rep(self.s().driver().io.venue_ix(*v))
                };
                for (k, role) in [
                    (0u8, ConnRole::Order),
                    (1, ConnRole::Order),
                    (0, ConnRole::UserStream),
                ] {
                    self.conn(r, *v, k, role, InstId(0), a.id);
                }
            }
        }
    }
    /// Account configuration, reconciliation summary and kill self-test of every Live account.
    fn gates(&mut self) {
        let t = self.s().driver().io.drv.stamp();
        let mut rid = 1_000_000 + self.cmd_no * 64;
        for (a, vs) in self.live_accounts() {
            for v in vs {
                let r = RingId::Rep(self.s().driver().io.venue_ix(v));
                let perp = v.market() == Some(MarketKind::LinearPerp);
                let levs: Vec<LeverageItem> = if perp {
                    self.world
                        .insts
                        .iter()
                        .filter(|i| i.venue == v && i.perp)
                        .map(|i| LeverageItem {
                            inst: i.id,
                            _pad: 0,
                            leverage: Rate(build::LEVERAGE_X * 1_000_000_000),
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                let mut h = AccountConfigReportHdr::new_zeroed();
                h.venue = v;
                h.account = a;
                let (pm, mm, ma) = if perp { (2, 2, 2) } else { (1, 1, 1) };
                h.position_mode = pm;
                h.margin_mode = mm;
                h.multi_asset = ma;
                h.source = 1;
                h.uid_keys = 3;
                h.n_lev = levs.len() as u16;
                h.rx_tsc = Tsc(t);
                let uid = build::uid_of(a, v);
                h.venue_uid[..uid.len()].copy_from_slice(uid.as_bytes());
                let mut body = h.as_bytes().to_vec();
                for l in &levs {
                    body.extend_from_slice(l.as_bytes());
                }
                self.push_bytes(r, EventKind::ACCOUNT_CONFIG_REPORT, body);
                rid += 1;
                let mut x = ReconcileReport::new_zeroed();
                x.report_id = rid;
                x.venue = v;
                x.account = a;
                x.item = 5;
                x.count = 1;
                self.push(r, &x);
            }
            self.sys(16 | (u32::from(a.0) << 16));
        }
    }
    fn balances(&mut self) {
        let t = self.s().driver().io.drv.stamp();
        for a in self
            .world
            .accounts
            .clone()
            .iter()
            .filter(|a| a.kind == AccountKind::Live)
        {
            for (v, asset, amt) in &a.balances {
                let r = RingId::Rep(self.s().driver().io.venue_ix(*v));
                let q = i64::try_from(*amt).unwrap_or(i64::MAX);
                let br = BalanceReport {
                    venue: *v,
                    account: a.id,
                    asset: *asset,
                    _pad: 0,
                    free: Qty(q),
                    locked: Qty(0),
                    ex_ts_ns: 0,
                    rx_tsc: Tsc(t),
                };
                self.push(r, &br);
                if v.market() == Some(MarketKind::LinearPerp) {
                    let mut h = MarginReportHdr::new_zeroed();
                    h.venue = *v;
                    h.account = a.id;
                    h.source = 1;
                    h.margin_balance = Notional(q);
                    h.maint_margin = Notional(0);
                    h.avail_margin = Notional(q);
                    h.margin_ratio = Rate(0);
                    h.rx_tsc = Tsc(t);
                    self.push_bytes(r, EventKind::MARGIN_REPORT, h.as_bytes().to_vec());
                }
            }
        }
    }
    /// Startup quotes: one neutral BBO for every inst (the widest bid/ask of the world on the
    /// coarsest tick), so no strategy crosses its threshold on a startup quote; the market actor's
    /// first tick brings the world's prices.
    fn quotes(&mut self) {
        let t = self.s().driver().io.drv.stamp();
        let tick = self.world.insts.iter().map(|i| i.tick.max(1)).max().unwrap_or(1);
        let lo = self.world.insts.iter().map(|i| i.bid).min().unwrap_or(0) / tick * tick;
        let hi = (self.world.insts.iter().map(|i| i.ask).max().unwrap_or(0) + tick - 1) / tick * tick;
        for mut i in self.world.insts.clone() {
            i.bid = lo;
            i.ask = hi;
            let r = RingId::Bbo(self.s().driver().io.venue_ix(i.venue));
            let mut h = MarketHdr::new_zeroed();
            h.venue = i.venue;
            h.inst = i.id;
            h.rx_tsc = t;
            let b = Bbo {
                h,
                bid_px: Px(i.bid),
                bid_qty: Qty(i.depth),
                ask_px: Px(i.ask),
                ask_qty: Qty(i.depth),
                update_id: 0,
            };
            self.push(r, &b);
        }
    }
    /// The startup prologue (see the module documentation).
    fn prologue(&mut self, modes: bool) {
        self.run();
        self.conns_up();
        self.run();
        self.sys(8);
        self.gates();
        self.run();
        if modes {
            // Startup balances; after a restart the ledger is the recovered one and the venues'
            // periodic truth reports keep it current (a resent startup balance would be stale).
            self.balances();
        }
        self.quotes();
        self.run();
        if modes {
            for b in self.world.strategies.clone() {
                for (s, g) in build::groups_of(&self.world, b.account) {
                    if s != b.instance {
                        continue;
                    }
                    let sc = hft_event::CmdScope {
                        strategy: s,
                        venue: VenueId(0),
                        account: b.account,
                        _pad: 0,
                        group: g,
                        inst: InstId(0),
                        _pad2: 0,
                    };
                    self.cmd(
                        b.account,
                        &ControlCmd::SetMode {
                            scope: sc,
                            mode: b.mode,
                        },
                    );
                    self.run();
                }
            }
        }
    }
    /// Puts the parsed object of a `ConfigBlob` Input in the Mailbox (ctl-gateway's role).
    fn mailbox_for(&mut self, i: &InputRec) {
        let Some(h) = hft_sequencer::types::read_body::<hft_event::BlobHdr>(&i.body) else {
            return;
        };
        let content = i
            .body
            .get(size_of::<hft_event::BlobHdr>()..)
            .unwrap_or(&[])
            .to_vec();
        let parsed: Option<ParsedConfig<S>> = match h.blob.kind {
            hft_event::BlobKind::StrategyParams => {
                let rd = &self.s().engine().rd;
                build::parse_spb::<S>(&content, rd)
                    .ok()
                    .map(|p| ParsedConfig::StrategyParams(Box::new(p)))
            }
            hft_event::BlobKind::RiskParams => {
                let rd = &self.s().engine().rd;
                build::risk_decode(&content, h.scope.account, rd)
                    .ok()
                    .map(|p| ParsedConfig::Risk {
                        sha256: h.blob.sha256,
                        params: Box::new(p),
                    })
            }
            hft_event::BlobKind::Refdata => {
                let cur = &self.s().engine().rd;
                Some(ParsedConfig::Refdata {
                    sha256: h.blob.sha256,
                    result: Box::new(RefData::load(&content, Some(cur))),
                })
            }
            _ => None,
        };
        if let Some(p) = parsed {
            self.io().mailbox.push(p);
        }
    }
    fn new_io(&self, now_ns: u64, clock: SegClock) -> io::DstIo<S> {
        let mut drv = DstDriver::new(self.tsc_hz, 0);
        drv.set_now(now_ns);
        let mut venues: Vec<VenueId> = self.world.venues.iter().map(|v| v.id).collect();
        venues.sort_by_key(|v| v.0);
        io::DstIo::new(
            drv,
            venues,
            clock,
            build::UTC0,
            self.world.clone(),
            self.carry.clone(),
        )
    }
    /// Cold start over the published part of the current journal at the current virtual time.
    fn restart(&mut self, sel: SnapshotSel) -> Result<(), BootFail> {
        let now = self.now_ns();
        let Some(s) = self.s.take() else {
            return Err(BootFail("no engine to restart".into()));
        };
        let st = s.engine().oms.stats();
        self.carry.dup_base += st.dup;
        self.carry.stale_gen_base += st.stale_gen;
        let drv = s.into_driver();
        let events = drv.io.events;
        let mut audit = drv.io.audit;
        let mut j = drv.io.j;
        let keep = j.committed;
        let committed_seq = io::committed_seq(&j);
        audit.restart(committed_seq);
        let mut n = 0usize;
        for seg in j.segments.iter_mut() {
            let room = keep.saturating_sub(n);
            if seg.records.len() > room {
                seg.records.truncate(room);
            }
            n += seg.records.len();
        }
        self.epoch += 1;
        let tsc = crate::dst::driver::tsc_at(0, self.tsc_hz, now);
        let clock = SegClock {
            tsc_hz_milli: self.tsc_hz.saturating_mul(1_000),
            calib_tsc: tsc,
            calib_realtime_ns: build::UTC0 + now as i64,
        };
        if STATES.with(|s| s.borrow().is_some()) {
            JOURNALS.with(|x| x.borrow_mut().push(j.clone()));
        }
        let mut io = self.new_io(now, clock);
        io.audit = audit;
        io.events = events;
        io.events
            .retain(|ev| ev.records.last().is_none_or(|r| r.seq <= committed_seq));
        io.epoch0 = (self.epoch & 0x7FFF) as u16;
        io.old = Some(Log::from_segments(&j.segments));
        j.truncate_to_event_end();
        io.j = j;
        let mut setup = self.setup.clone();
        setup.clock = clock;
        let plan = RecoverPlan {
            start: sel,
            replay_tail: true,
            epoch: self.epoch,
            flags: RecoveryFlags(0),
            quarantine: Vec::new(),
        };
        let drv = LiveDriver::new(
            io,
            LiveCfg {
                feed_stage_cap: 1_024,
                ..LiveCfg::default()
            },
        );
        let s = Sequencer::boot(BootSource::Journal(plan), &setup, drv, self.hb)
            .map_err(|e| BootFail(format!("restart: {e:?}")))?;
        self.s = Some(s);
        self.remirror();
        Ok(())
    }
    /// Hashes of a sequencer's current state.
    fn hashes_of<D: hft_sequencer::Driver<S>>(s: &Sequencer<D, S>) -> ComponentHashes {
        let mut w = SnapshotWriter::new(64 << 20);
        s.write_sections(&mut w);
        crate::hash::component_hashes_of_sections(w.as_bytes())
    }
}

impl<S: StrategySet> Sut for SeqSut<S> {
    type Matcher = SimMatcher;

    fn drain_observations(&mut self, out: &mut StepOut) {
        out.events.append(&mut self.io().events);
    }

    fn boot(ctx: &BootCtx<'_>) -> Result<Self, BootFail> {
        if let Some(p) = ctx.planted.iter().find(|p| !PLANTED.iter().any(|(n, _)| n == *p)) {
            return Err(BootFail(format!(
                "planted defect `{p}` is not built into the engine crates"
            )));
        }
        let w = ctx.world.clone();
        let rd_blob = build::refdata_blob(&w, false).map_err(BootFail)?;
        let rd = build::load_refdata(&rd_blob).map_err(BootFail)?;
        let setup = build::engine_setup(&w, ctx.tsc_hz, 1);
        let configs = build::config_set::<S>(&w, &rd, rd_blob, &setup).map_err(BootFail)?;
        let setup = EngineSetup::decode_blob(&configs.engine_blob, &rd, setup.clock, 1, 0)
            .map_err(|e| BootFail(format!("engine configuration: {e:?}")))?;
        let hb: &'static Heartbeat = Box::leak(Box::new(Heartbeat::new()));
        let mut sut = SeqSut {
            s: None,
            world: w,
            setup: setup.clone(),
            tsc_hz: ctx.tsc_hz,
            jrecs: Vec::new(),
            epoch: 1,
            cmd_no: 0,
            hb,
            carry: observe::Carry::default(),
        };
        let io = sut.new_io(0, setup.clock);
        let drv = LiveDriver::new(
            io,
            LiveCfg {
                feed_stage_cap: 1_024,
                ..LiveCfg::default()
            },
        );
        let s = Sequencer::boot(BootSource::Empty { configs }, &setup, drv, hb)
            .map_err(|e| BootFail(format!("boot: {e:?}")))?;
        sut.s = Some(s);
        sut.prologue(true);
        sut.mirror(None);
        sut.io().events.clear();
        Ok(sut)
    }

    fn process(&mut self, now_ns: u64, _tsc: u64, input: &InputRec, out: &mut StepOut) {
        self.set_now(now_ns);
        if input.kind == EventKind::CONFIG_BLOB {
            self.mailbox_for(input);
        }
        let r = ring_of(&self.s().driver().io, input);
        let before = observe::Before::take(self, input);
        let mut body = input.body.clone();
        if !matches!(
            input.src,
            Source::Ctl | Source::Hk | Source::Advisor | Source::Engine
        ) {
            let t = self.s().driver().io.drv.stamp();
            stamp_rx(input.kind, &mut body, t.max(1));
        }
        self.push_bytes(r, input.kind, body);
        self.run();
        self.finish_step(before, out);
    }

    fn next_deadline(&self) -> Option<u64> {
        let d = self.s().view().next_deadline()?;
        Some(self.ns_of(d.0).max(self.now_ns()))
    }

    fn fire_timers(&mut self, now_ns: u64, _tsc: u64, out: &mut StepOut) -> bool {
        self.set_now(now_ns);
        let stamp = self.s().driver().io.drv.stamp();
        let due = self.s().view().next_deadline().is_some_and(|d| d.0 <= stamp);
        if !due {
            return false;
        }
        let before = observe::Before::none(self);
        let n = self.jrecs.len();
        self.run();
        self.finish_step(before, out);
        self.jrecs.len() > n
    }

    fn observe(&self, o: &mut Obs) {
        observe::fill(self, o);
    }

    fn hashes(&self) -> ComponentHashes {
        Self::hashes_of(self.s())
    }

    fn rebuilt_hashes(&self) -> Option<ComponentHashes> {
        let j = &self.s().driver().io.j;
        if STATES.with(|s| s.borrow().is_some()) {
            JOURNALS.with(|x| x.borrow_mut().push(j.clone()));
        }
        let log = Log::from_segments(&j.segments);
        let snap = log
            .recs
            .iter()
            .filter(|r| r.kind == EventKind::STATE_SNAPSHOT)
            .map(|r| r.seq)
            .next_back()?;
        let setup = EngineSetup::from_snapshot(&log, snap).ok()?;
        let drv = ReplayDriver::new(ReplayMode::Verify, log, 0, setup.seq.replay_per_round);
        let plan = RecoverPlan {
            start: SnapshotSel::AtSeq(snap),
            replay_tail: true,
            epoch: 0,
            flags: RecoveryFlags(0),
            quarantine: Vec::new(),
        };
        let mut s: Sequencer<ReplayDriver, S> =
            Sequencer::boot(BootSource::Journal(plan), &setup, drv, self.hb).ok()?;
        for _ in 0..10_000_000 {
            if s.step() != StepOutcome::Busy {
                break;
            }
        }
        Some(Self::hashes_of(&s))
    }

    fn seq(&self) -> u64 {
        self.jrecs.last().map_or(0, |r| r.seq)
    }

    fn journal(&self) -> &[JRec] {
        &self.jrecs
    }

    fn crash_and_recover(&mut self, _point: CrashPoint, _input: Option<&InputRec>) -> Result<(), BootFail> {
        self.restart(SnapshotSel::Latest)?;
        self.prologue(false);
        self.mirror(None);
        Ok(())
    }

    fn clean_restart(&mut self) -> Result<(), BootFail> {
        self.cmd(AccountId(0), &ControlCmd::Drain { cancel_resting: true });
        let o = self.run();
        if !matches!(o, StepOutcome::Done { .. }) {
            // A Drain that cannot finish (in-flight orders) is a crash restart instead.
            for _ in 0..MAX_ROUNDS {
                if matches!(
                    self.s_mut().step(),
                    StepOutcome::Done { .. } | StepOutcome::Fatal(_)
                ) {
                    break;
                }
            }
        }
        self.restart(SnapshotSel::Latest)?;
        self.prologue(false);
        self.mirror(None);
        Ok(())
    }

    fn is_sim(&self, a: AccountId) -> bool {
        self.world.account(a).is_some_and(|x| x.kind == AccountKind::Sim)
    }
}

thread_local! {
    /// Debug recording of the running state after every step (section blob by last seq).
    static STATES: core::cell::RefCell<Option<std::collections::BTreeMap<u64, Vec<u8>>>> =
        const { core::cell::RefCell::new(None) };
    /// Debug: a text dump of selected engine state after every step (while recording).
    static DUMPS: core::cell::RefCell<std::collections::BTreeMap<u64, String>> = const { core::cell::RefCell::new(std::collections::BTreeMap::new()) };
    /// Debug: the journals the restarts started from (while recording).
    static JOURNALS: core::cell::RefCell<Vec<hft_sequencer::MemJournal>> = const { core::cell::RefCell::new(Vec::new()) };
}

/// Debug aid: the text dumps recorded since the last call.
pub fn recorded_dumps() -> std::collections::BTreeMap<u64, String> {
    DUMPS.with(|j| core::mem::take(&mut *j.borrow_mut()))
}

/// Debug aid: selected engine state as text.
pub fn dump_engine(e: &hft_sequencer::Engine) -> String {
    let mut s = String::new();
    for a in 1..=2u8 {
        for v in [2u8, 5] {
            s.push_str(&format!(
                "a{a}v{v} margin {:?}
",
                e.ledger.margin(AccountId(a), VenueId(v)),
            ));
        }
    }
    s
}

/// Debug aid: the journals the restarts of this thread started from since the last call.
pub fn recorded_journals() -> Vec<hft_sequencer::MemJournal> {
    JOURNALS.with(|j| core::mem::take(&mut *j.borrow_mut()))
}

/// Debug aid: start (`true`) or stop recording the state sections after every step of the
/// adapters of this thread; returns what was recorded.
pub fn record_states(on: bool) -> std::collections::BTreeMap<u64, Vec<u8>> {
    STATES.with(|s| {
        let old = s.borrow_mut().take().unwrap_or_default();
        if on {
            *s.borrow_mut() = Some(std::collections::BTreeMap::new());
        }
        old
    })
}

impl<S: StrategySet> SeqSut<S> {
    fn record_state(&self) {
        if STATES.with(|s| s.borrow().is_some()) {
            let mut w = SnapshotWriter::new(64 << 20);
            self.s().write_sections(&mut w);
            let k = self.s().driver().io.j.records().last().map_or(0, |r| r.seq);
            STATES.with(|s| {
                if let Some(m) = s.borrow_mut().as_mut() {
                    m.insert(k, w.as_bytes().to_vec());
                }
            });
            let d = dump_engine(self.s().engine());
            DUMPS.with(|x| x.borrow_mut().insert(k, d));
        }
    }
    fn finish_step(&mut self, before: observe::Before, out: &mut StepOut) {
        let mut recs = Vec::new();
        self.mirror(Some(&mut recs));

        if self.s().engine().finished.is_some() {
            // The process ends (Drain done, stop, fatal); systemd starts it again over the same
            // journal. Frames already emitted stay with this step.
            let mut frames = Vec::new();
            self.io().drv.drain_emitted(&mut frames);
            out.frames.extend(frames);
            let jlen = self.jrecs.len();
            if self.restart(SnapshotSel::Latest).is_ok() {
                self.prologue(false);
            }
            // The restart's records (boot block, recovery, prologue) are part of this step.
            self.mirror(None);
            let from = jlen.min(self.jrecs.len());
            recs.extend(self.jrecs[from..].iter().cloned());
        }
        out.records = recs;
        self.io().drv.drain_emitted(&mut out.frames);
        observe::step_info(self, &before, out);
        self.drain_observations(out);
    }
    /// The world's run mode of every cell (scenario helpers).
    pub fn mode_of(&self, a: AccountId, g: GroupId) -> Option<RunMode> {
        self.world
            .strategies
            .iter()
            .find(|s| s.account == a)
            .map(|s| s.mode)
            .filter(|_| g.0 != 0)
    }
}
