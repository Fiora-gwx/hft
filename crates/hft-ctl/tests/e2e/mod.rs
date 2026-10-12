//! End-to-end baseline `E2E` of crates/hft-ctl/TESTS.md (`e2e()`): the gateway in front of the real
//! `Sequencer<TestStrategies>` on its live driver. The gateway's ctl ring is carried into the
//! sequencer by the rig's `LiveIo` (`io.rs`); the `CmdResult`s the sequencer writes are read back
//! from the published journal tail and handed to the gateway the way `ResultSource` does, and
//! reach the client: on Linux the real `Gateway` task and `CtlClient` over a Unix socket
//! (`front.rs`), elsewhere `Core` called directly with the same request bytes (the socket part of
//! the crate is Linux-only). Order reports come from a scripted venue (`venue_round`).
#![allow(
    dead_code,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    clippy::too_many_arguments
)]

pub mod front;
pub mod io;
pub mod rec;
pub mod world;

mod adopt;
mod expiry;
mod hedge_fix;
mod idempotency;
mod kill;
mod latency;
mod maintenance;
mod misc;
mod recon;
mod resume;
mod sdk;
mod set_mode;
mod set_params;
mod set_risk;
mod smoke;
mod strategy_channel;
mod viewer_stop;
mod write_off;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use hft_ctl_proto::{CmdResponse, ResultCode, TransportStatus, Uuid};
use hft_event::{
    AccountConfigReportHdr, BalanceReport, Bbo, CmdHdr, CmdScope, ConnRole, ConnState, ConnStateKind,
    ControlCmd, EventKind, LeverageItem, MarketHdr, OrderAction, OrderDecision, OrderEvent, OrderEventKind,
    PartHdr, ReconcileReport, RecordFlags, Timer,
};
use hft_refdata::{AccountKind, RefData};
use hft_risk::mode::{CellKey, Effective};
use hft_rt::{Heartbeat, Mailbox};
use hft_sequencer::driver::live::{LiveCfg, LiveDriver, RingId};
use hft_sequencer::{BootSource, ConfigSet, EngineSetup, MemRecord, ParsedConfig, Sequencer, StepOutcome};
use hft_types::{AccountId, GroupId, InstId, Qty, Rate, RunMode, StrategyId, Tif, Tsc, VenueId};
use serde_json::{Value, json};
use zerocopy::{FromZeros, IntoBytes};

use front::{Call, Front, Ticket};
use io::{CtlQ, Elem, RigIo};
use world::*;

pub type Seq = Sequencer<LiveDriver<Set, RigIo>, Set>;

/// What the scripted venue does with an accepted new order.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Fills {
    /// IOC/FOK: acked and filled in full at the order price; other TIFs: acked and resting.
    Auto,
    /// Nothing comes back (the order stays in flight).
    Silent,
    /// Every new order is rejected by the venue.
    Reject,
}

/// Options of a run.
#[derive(Clone, Debug)]
pub struct Opts {
    /// Accounts whose start gate `steady` releases (ACCT_A, ACCT_B, SIM_A).
    pub release: Vec<AccountId>,
    pub params: Vec<Spb>,
    /// Edits of the `EngineSetup` (per-row deviations from E2E).
    pub setup: Option<fn(&mut EngineSetup)>,
    /// The starting `RiskParams` document (per-row deviations from E2E; journaled as the boot
    /// blob, so replay sees the same).
    pub risk: RiskDoc,
    /// (account, venue) pairs `steady` sends no balance report for.
    pub no_balance: Vec<(AccountId, VenueId)>,
    /// (account, venue) pairs `steady` sends no reconcile summary for.
    pub no_recon: Vec<(AccountId, VenueId)>,
}

impl Default for Opts {
    fn default() -> Opts {
        Opts {
            release: vec![ACCT_A, ACCT_B, SIM_A],
            params: start_params(),
            setup: None,
            risk: RiskDoc::start(),
            no_balance: Vec::new(),
            no_recon: Vec::new(),
        }
    }
}

/// The E2E harness.
pub struct E2e {
    pub s: Seq,
    pub tsc: Rc<Cell<u64>>,
    pub ctl: Rc<RefCell<CtlQ>>,
    pub front: Option<Front>,
    pub rd: RefData,
    pub setup: EngineSetup,
    /// Published records already read by the result tail.
    res_cursor: usize,
    /// Records already seen by the scripted venue.
    venue_cursor: usize,
    pub fills: Fills,
    /// The sequencer is not stepped while paused.
    pub paused: bool,
    pub upd: u64,
    direct_no: u64,
    /// `ctl_state_version` after `steady` (V0).
    pub v0: u64,
    /// The sequencer's final outcome (a finished Drain).
    pub done: Option<StepOutcome>,
    /// Fill prices of the scripted venue that differ from the order price: (inst, side, px).
    pub px_override: Vec<(InstId, hft_types::Side, hft_types::Px)>,
    /// Resting orders the scripted venue acked (cancelled by `Cancel`/`CancelAll`).
    pub resting: Vec<OrderDecision>,
    /// `Opts::no_balance` / `Opts::no_recon` of this run.
    pub no_balance: Vec<(AccountId, VenueId)>,
    pub no_recon: Vec<(AccountId, VenueId)>,
    /// The advisor (strategies' slow parts) once `start_advisor` ran.
    pub advisor: Option<Adv>,
}

/// The advisor of the rig: no checkpoints; its Advice goes to the sequencer's advice ring.
pub type Adv = hft_sequencer::advisor::Advisor<Set, NoCkpt, AdvRing>;

/// A checkpoint store without checkpoints.
#[derive(Debug, Default)]
pub struct NoCkpt;

impl hft_sequencer::advisor::CheckpointStore for NoCkpt {
    fn load(&mut self, _: StrategyId, _: &mut Vec<u8>) -> Option<hft_sequencer::advisor::CkptHdr> {
        None
    }
    fn begin(
        &mut self,
        _: StrategyId,
        _: &hft_sequencer::advisor::CkptHdr,
    ) -> Result<(), hft_sequencer::advisor::CkptIoErr> {
        Err(hft_sequencer::advisor::CkptIoErr)
    }
    fn write_chunk(&mut self, _: &[u8]) -> Result<(), hft_sequencer::advisor::CkptIoErr> {
        Err(hft_sequencer::advisor::CkptIoErr)
    }
    fn commit(&mut self) -> Result<(), hft_sequencer::advisor::CkptIoErr> {
        Err(hft_sequencer::advisor::CkptIoErr)
    }
}

/// The advice ring (records handed to the sequencer's `RingId::Advice`).
#[derive(Debug, Default)]
pub struct AdvRing {
    pub recs: Vec<(EventKind, Vec<u8>)>,
}

impl hft_sequencer::advisor::AdviceRing for AdvRing {
    fn free(&self) -> usize {
        1 << 20
    }
    fn push(&mut self, kind: EventKind, bytes: &[u8]) -> bool {
        self.recs.push((kind, bytes.to_vec()));
        true
    }
}

/// The committed part of a journal.
pub fn committed(j: &hft_sequencer::MemJournal) -> hft_sequencer::driver::replay::Log {
    let mut l = hft_sequencer::driver::replay::Log::from_segments(&j.segments);
    l.recs.truncate(j.committed);
    l
}

fn hb() -> &'static Heartbeat {
    Box::leak(Box::new(Heartbeat::new()))
}

/// Single-slot Mailbox pair (leaked: the rig lives for one test).
fn mailboxes() -> (
    (
        hft_rt::MailSender<ParsedConfig<Set>>,
        hft_rt::MailReceiver<ParsedConfig<Set>>,
    ),
    (
        hft_rt::MailSender<ParsedConfig<Set>>,
        hft_rt::MailReceiver<ParsedConfig<Set>>,
    ),
) {
    let p: &'static Mailbox<ParsedConfig<Set>> = Box::leak(Box::new(Mailbox::new()));
    let r: &'static Mailbox<ParsedConfig<Set>> = Box::leak(Box::new(Mailbox::new()));
    (p.split(), r.split())
}

/// Baseline `E2E` in the steady state `S0`.
pub fn e2e() -> E2e {
    E2e::with(Opts::default())
}

impl E2e {
    /// Boots the sequencer (empty start) and the gateway, then brings the run to `S0` with the
    /// gates of `o.release` released.
    pub fn with(o: Opts) -> E2e {
        let mut r = E2e::boot(&o);
        r.no_balance = o.no_balance.clone();
        r.no_recon = o.no_recon.clone();
        r.steady(&o.release);
        r
    }

    /// Booted, before `steady`.
    pub fn boot(o: &Opts) -> E2e {
        let rd = rd();
        let mut setup = engine_setup();
        if let Some(f) = o.setup {
            f(&mut setup);
        }
        let tsc = Rc::new(Cell::new(T0));
        let ctl = Rc::new(RefCell::new(CtlQ::default()));
        let ((p_send, p_take), (r_send, r_take)) = mailboxes();
        let io = RigIo::new(setup.clock, tsc.clone(), ctl.clone(), p_take, r_send);
        let drv = LiveDriver::new(
            io,
            LiveCfg {
                feed_stage_cap: 1024,
                ..LiveCfg::default()
            },
        );
        let rb = o.risk.bytes();
        let risk = risk_decode(&rb, AccountId(0), &rd).unwrap_or_else(|e| panic!("{e}"));
        let configs = ConfigSet {
            refdata_blob: rd_blob(),
            engine_blob: engine_blob(),
            risk_sha256: sha(&rb),
            risk,
            risk_blob: rb,
            params: o.params.iter().map(|p| (p.bytes(), p.parsed(&rd))).collect(),
        };
        let s = Sequencer::boot(BootSource::Empty { configs }, &setup, drv, hb())
            .unwrap_or_else(|e| panic!("E2E boot: {e:?}"));
        let active = o
            .params
            .iter()
            .map(|p| ParsedConfig::StrategyParams(Box::new(p.parsed(&rd))))
            .collect();
        let front = Front::new(rd.clone(), tsc.clone(), ctl.clone(), p_send, r_take, active, None);
        E2e {
            s,
            tsc,
            ctl,
            front: Some(front),
            rd,
            setup,
            res_cursor: 0,
            venue_cursor: 0,
            fills: Fills::Auto,
            paused: false,
            upd: 1,
            direct_no: 0,
            v0: 0,
            done: None,
            px_override: Vec::new(),
            resting: Vec::new(),
            no_balance: Vec::new(),
            no_recon: Vec::new(),
            advisor: None,
        }
    }

    /// Starts the advisor over the journal (after a keyframe snapshot, its start point).
    pub fn start_advisor(&mut self) {
        self.sys_timer(1);
        self.settle();
        let log = committed(&self.io_ref().j);
        let cfg = hft_sequencer::advisor::AdvisorCfg {
            batch: 256,
            checkpoint_ms: 0,
            chunk_bytes: 1 << 20,
            max_lag_ms: 5_000,
            shard: None,
        };
        let a =
            hft_sequencer::advisor::Advisor::start(&log, &self.setup, cfg, NoCkpt, AdvRing::default(), hb())
                .unwrap_or_else(|e| panic!("advisor start: {e:?}"));
        self.advisor = Some(a);
        self.settle();
    }

    /// The advisor catches up with the committed journal; its Advice goes to the advice ring.
    fn advisor_round(&mut self) -> bool {
        let Some(mut a) = self.advisor.take() else {
            return false;
        };
        let log = committed(&self.io_ref().j);
        let mut busy = false;
        for _ in 0..10_000 {
            if !a.poll_once(&log) {
                break;
            }
            busy = true;
        }
        for (k, b) in std::mem::take(&mut a.ring.recs) {
            self.push_e(RingId::Advice, k, b);
        }
        self.advisor = Some(a);
        busy
    }

    // --------------------------------------------------------------------------- clock

    pub fn now(&self) -> u64 {
        self.tsc.get()
    }
    pub fn adv(&mut self, ns: u64) {
        self.tsc.set(self.tsc.get() + ns);
    }
    pub fn adv_ms(&mut self, ms: u64) {
        self.adv(ms * MS);
    }
    /// UTC of the current TSC (the gateway's clock).
    pub fn utc(&self) -> i64 {
        UTC0 + self.now() as i64
    }

    // --------------------------------------------------------------------------- sequencer side

    pub fn io(&mut self) -> &mut RigIo {
        &mut self.s.driver_mut().io
    }
    pub fn io_ref(&self) -> &RigIo {
        &self.s.driver().io
    }
    pub fn recs(&self) -> Vec<MemRecord> {
        self.io_ref().j.records().cloned().collect()
    }
    pub fn mark(&self) -> usize {
        self.io_ref().j.len()
    }
    pub fn since(&self, m: usize) -> Vec<MemRecord> {
        self.io_ref().j.records().skip(m).cloned().collect()
    }
    pub fn push_e(&mut self, r: RingId, kind: EventKind, body: Vec<u8>) {
        self.io().ring(r).push_back(Elem { kind, body });
    }
    pub fn push<B: hft_event::Body>(&mut self, r: RingId, b: &B) {
        self.push_e(r, B::KIND, b.as_bytes().to_vec());
    }

    /// One sequencer round (unless paused), the scripted venue and the result tail.
    pub fn round(&mut self) -> bool {
        let mut busy = false;
        if let Some(f) = self.front.as_mut() {
            busy |= f.poll(Tsc(self.tsc.get()));
        }
        if !self.paused && self.done.is_none() {
            match self.s.step() {
                StepOutcome::Busy => busy = true,
                StepOutcome::Idle => {}
                StepOutcome::Fatal(f) => panic!("sequencer fatal: {f:?}"),
                // A finished Drain: the process flushes its journal and exits; the rig stops
                // stepping.
                o => {
                    use hft_sequencer::JournalSink as _;
                    self.io().j.publish();
                    self.done = Some(o);
                }
            }
            busy |= self.venue_round();
        }
        busy |= self.tail_results();
        busy |= self.advisor_round();
        busy
    }

    /// Rounds until nothing moves (at most `max`).
    pub fn settle_max(&mut self, max: usize) {
        let mut idle = 0;
        for _ in 0..max {
            if self.round() {
                idle = 0;
            } else {
                idle += 1;
                if idle >= 2 {
                    return;
                }
            }
        }
    }
    pub fn settle(&mut self) {
        self.settle_max(2_000);
    }

    /// `CmdResult`s of the published journal tail → the gateway (the `ResultSource` path).
    fn tail_results(&mut self) -> bool {
        let new: Vec<hft_event::CmdResult> = {
            let j = &self.s.driver().io.j;
            let out = j
                .published()
                .skip(self.res_cursor)
                .filter(|r| r.kind == EventKind::CMD_RESULT)
                .filter_map(|r| r.read::<hft_event::CmdResult>())
                .collect();
            self.res_cursor = j.committed;
            out
        };
        let any = !new.is_empty();
        if let Some(f) = self.front.as_mut() {
            for r in new {
                f.result(r);
            }
        }
        any
    }

    // --------------------------------------------------------------------------- scripted venue

    fn rep_ring(&self, a: AccountId) -> RingId {
        if matches!(self.rd.account(a).kind, AccountKind::Sim(_)) {
            RingId::RepSim
        } else {
            RingId::Rep(0)
        }
    }

    pub fn epoch(&self) -> u16 {
        self.s.view().engine_epoch() as u16
    }

    pub fn order_event(&self, d: &OrderDecision, what: OrderEventKind) -> OrderEvent {
        OrderEvent {
            venue: d.venue,
            account: d.account,
            what,
            detail: 0,
            slot: d.slot,
            r#gen: d.r#gen,
            inst: d.inst,
            px: d.px,
            qty: d.qty,
            cum_filled: Qty(0),
            fee: Qty(0),
            fee_asset: hft_types::AssetId(0),
            epoch: self.epoch(),
            reject_code: 0,
            exch_order_id: 1_000 + u64::from(d.slot.0),
            ex_ts_ns: 0,
            hw_rx_ns: 0,
            rx_tsc: self.now(),
            avg_px: d.px,
            cash_qty: hft_types::Qty(0),
            cash_sum: hft_types::Qty(0),
        }
    }

    /// Answers the `OrderDecision`s written since the last call per `self.fills`.
    pub fn venue_round(&mut self) -> bool {
        let m = self.venue_cursor;
        let ds: Vec<OrderDecision> = self
            .io_ref()
            .j
            .records()
            .skip(m)
            .filter(|r| r.kind == EventKind::ORDER_DECISION)
            .filter_map(|r| hft_sequencer::types::read_body::<OrderDecision>(&r.body))
            .collect();
        self.venue_cursor = self.io_ref().j.len();
        if self.fills == Fills::Silent {
            return false;
        }
        let mut any = false;
        for d in ds {
            if d.reject != 0 {
                continue;
            }
            let ring = self.rep_ring(d.account);
            match d.action {
                OrderAction::New if self.fills == Fills::Reject => {
                    self.reject(&d);
                    any = true;
                }
                OrderAction::New => {
                    let ack = self.order_event(&d, OrderEventKind::Ack);
                    self.push(ring, &ack);
                    if !matches!(d.tif, Tif::Ioc | Tif::Fok) {
                        self.resting.push(d);
                    }
                    if matches!(d.tif, Tif::Ioc | Tif::Fok) {
                        let mut f = self.order_event(&d, OrderEventKind::Fill);
                        f.cum_filled = d.qty;
                        if let Some((_, _, p)) = self
                            .px_override
                            .iter()
                            .find(|(i, sd, _)| *i == d.inst && *sd == d.side)
                        {
                            f.px = *p;
                            f.avg_px = *p;
                        }
                        self.push(ring, &f);
                    }
                    any = true;
                }
                OrderAction::Cancel => {
                    let c = self.order_event(&d, OrderEventKind::CancelAck);
                    self.push(ring, &c);
                    self.resting.retain(|x| x.slot != d.slot || x.venue != d.venue);
                    any = true;
                }
                OrderAction::CancelAll => {
                    let (hit, keep): (Vec<OrderDecision>, Vec<OrderDecision>) = self
                        .resting
                        .iter()
                        .partition(|x| x.venue == d.venue && x.account == d.account);
                    self.resting = keep;
                    for x in hit {
                        let c = self.order_event(&x, OrderEventKind::CancelAck);
                        self.push(ring, &c);
                    }
                    any = true;
                }
                _ => {}
            }
        }
        any
    }

    // --------------------------------------------------------------------------- inputs

    pub fn conn(&mut self, venue: VenueId, k: u8, role: ConnRole, inst: InstId, account: AccountId) {
        let ring = if account.0 != 0 && account == SIM_A {
            RingId::RepSim
        } else {
            RingId::Rep(0)
        };
        let c = ConnState {
            venue,
            conn: k,
            state: ConnStateKind::Up,
            role,
            inst,
            reason: 0,
            account,
            _pad2: [0; 3],
            rx_tsc: self.now(),
        };
        self.push(ring, &c);
    }

    pub fn sys_timer(&mut self, id: u32) {
        let t = Timer {
            timer_id: id,
            owner: 1,
            _pad: 0,
            group: GroupId(0),
            _pad2: 0,
            deadline_tsc: self.now(),
        };
        self.push(RingId::Hk, &t);
    }

    /// A BBO of `i` (quantity 1 on each side).
    pub fn quote(&mut self, i: InstId, bid: &str, ask: &str) {
        let mut h = MarketHdr::new_zeroed();
        h.venue = self.rd.inst(i).venue;
        h.conn = hft_types::ConnId(0);
        h.inst = i;
        h.rx_tsc = self.now();
        self.upd += 1;
        let b = Bbo {
            h,
            bid_px: px(bid),
            bid_qty: q("1"),
            ask_px: px(ask),
            ask_qty: q("1"),
            update_id: self.upd,
        };
        self.push(RingId::Bbo(0), &b);
    }

    /// A `MarkPrice` of `i` (md ring).
    pub fn mark_px(&mut self, i: InstId, p: &str) {
        let mut h = MarketHdr::new_zeroed();
        h.venue = self.rd.inst(i).venue;
        h.inst = i;
        h.rx_tsc = self.now();
        let m = hft_event::MarkPrice { h, px: px(p) };
        self.push(RingId::Md(0), &m);
    }

    /// The S0 quotes: no `probe-taker` opening (leg A's bid below leg B's ask).
    pub fn quotes(&mut self) {
        self.quote(I_BN_USDM, "50000", "50001");
        self.quote(I_BG_SPOT, "50000", "50001");
        self.quote(I_BG_FUT, "50000", "50001");
        self.quote(I_BN_SPOT, "50010", "50011");
    }

    pub fn account_venues(&self, a: AccountId) -> Vec<VenueId> {
        let bits = self.rd.account(a).venues;
        (0..64u8)
            .filter(|v| bits & (1u64 << v) != 0)
            .map(VenueId)
            .collect()
    }

    /// The configuration report, reconcile summary, balances and margin of a Live account (the
    /// gate inputs besides connections, clock and kill self-test).
    pub fn account_reports(&mut self, a: AccountId, report_base: u64) {
        let mut rid = report_base;
        for v in self.account_venues(a) {
            let perp = v.market() == Some(hft_types::MarketKind::LinearPerp);
            let levs: Vec<LeverageItem> = if perp {
                self.rd
                    .insts_of_venue(v)
                    .iter()
                    .map(|i| LeverageItem {
                        inst: *i,
                        _pad: 0,
                        leverage: Rate(3_000_000_000),
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
            let uid = self.rd.venue_uid(a, v).unwrap_or("").to_owned();
            h.venue_uid[..uid.len()].copy_from_slice(uid.as_bytes());
            let mut body = h.as_bytes().to_vec();
            for l in &levs {
                body.extend_from_slice(l.as_bytes());
            }
            self.push_e(RingId::Rep(0), EventKind::ACCOUNT_CONFIG_REPORT, body);
            rid += 1;
            if !self.no_recon.contains(&(a, v)) {
                let rs = recon_summary(a, v, rid, 0);
                self.push(RingId::Rep(0), &rs);
            }
        }
        self.sys_timer(16 | (u32::from(a.0) << 16));
    }

    /// A `MarginReport` of each perpetual venue of `a`.
    pub fn margin(&mut self, a: AccountId) {
        for v in self.account_venues(a) {
            if v.market() == Some(hft_types::MarketKind::LinearPerp) {
                let mut hd = hft_event::MarginReportHdr::new_zeroed();
                hd.venue = v;
                hd.account = a;
                hd.source = 1;
                hd.margin_balance = usdt(100_000);
                hd.maint_margin = usdt(0);
                hd.avail_margin = usdt(100_000);
                hd.margin_ratio = Rate(0);
                hd.rx_tsc = Tsc(self.now());
                self.push_e(RingId::Rep(0), EventKind::MARGIN_REPORT, hd.as_bytes().to_vec());
            }
        }
    }

    /// USDT balances of every venue of `a`, then its margin reports (the margin estimate reads the
    /// wallet balance).
    pub fn balances(&mut self, a: AccountId) {
        for v in self.account_venues(a) {
            if self.no_balance.contains(&(a, v)) {
                continue;
            }
            let br = BalanceReport {
                venue: v,
                account: a,
                asset: USDT,
                _pad: 0,
                free: Qty(100_000_000_000),
                locked: Qty(0),
                ex_ts_ns: 0,
                rx_tsc: Tsc(self.now()),
            };
            self.push(RingId::Rep(0), &br);
        }
        self.margin(a);
    }

    /// Steady state `S0`: EngineClock, connections up, clock ok, the gate inputs of `release`,
    /// quotes, every cell desired Live; records `V0`.
    pub fn steady(&mut self, release: &[AccountId]) {
        self.settle();
        for v in LIVE_VENUES {
            self.conn(v, 0, ConnRole::Md, InstId(0), AccountId(0));
            for i in self.rd.insts_of_venue(v).to_vec() {
                self.conn(v, 0, ConnRole::Md, i, AccountId(0));
            }
        }
        for a in [ACCT_A, ACCT_B, SIM_A] {
            for v in self.account_venues(a) {
                for (k, role) in [
                    (0u8, ConnRole::Order),
                    (1, ConnRole::Order),
                    (0, ConnRole::UserStream),
                ] {
                    self.conn(v, k, role, InstId(0), a);
                }
            }
        }
        self.settle();
        self.sys_timer(8);
        for (k, a) in [ACCT_A, ACCT_B].into_iter().enumerate() {
            if release.contains(&a) {
                self.account_reports(a, 1_000 + 10 * k as u64);
            }
        }
        self.settle();
        for a in [ACCT_A, ACCT_B] {
            self.balances(a);
        }
        // sim-gw's truth reports of sim-a's initial balances (1 000 USDT per venue).
        for v in self.account_venues(SIM_A) {
            let br = BalanceReport {
                venue: v,
                account: SIM_A,
                asset: USDT,
                _pad: 0,
                free: Qty(1_000_000_000),
                locked: Qty(0),
                ex_ts_ns: 0,
                rx_tsc: Tsc(self.now()),
            };
            self.push(RingId::RepSim, &br);
        }
        self.quotes();
        self.settle();
        for (s, a, g) in [(S1, ACCT_A, G1), (S2, ACCT_B, G2), (S9, SIM_A, G9)] {
            if !release.contains(&a) {
                continue;
            }
            let v = self.version();
            self.direct(
                "cli:alice",
                CmdHdr::ROLE_ADMIN,
                a,
                &ControlCmd::SetMode {
                    scope: scope(s, a, g),
                    mode: RunMode::Live,
                },
                |h| {
                    h.confirmed = 1;
                    h.expect_state_version = v;
                },
            );
            self.settle();
        }
        self.v0 = self.version();
    }

    /// A command written straight into the ctl ring (setup only: no gateway, no audit Note).
    pub fn direct(
        &mut self,
        issuer: &str,
        role: u8,
        account: AccountId,
        c: &ControlCmd<'_>,
        f: impl FnOnce(&mut CmdHdr),
    ) -> [u8; 16] {
        self.direct_no += 1;
        let mut h = CmdHdr::zeroed();
        h.cmd_id[..8].copy_from_slice(&self.direct_no.to_le_bytes());
        h.cmd_id[15] = 0xD1;
        h.issuer[..issuer.len()].copy_from_slice(issuer.as_bytes());
        h.role = role;
        h.account = account;
        h.cmd_ver = 1;
        f(&mut h);
        let mut buf = vec![0u8; 8192];
        let n = c.encode(&h, &mut buf).unwrap_or_else(|e| panic!("{e:?}"));
        buf.truncate(n);
        let ph = PartHdr {
            total_len: n as u32,
            chunk_len: n as u32,
            part: 0,
            parts: 1,
            _rsv: 0,
        };
        let mut body = ph.as_bytes().to_vec();
        body.extend_from_slice(&buf);
        let mut q = self.ctl.borrow_mut();
        q.used += hft_ctl::ring::rec_len(&(EventKind::CONTROL_CMD, RecordFlags(0), body.clone()));
        q.recs.push_back((EventKind::CONTROL_CMD, RecordFlags(0), body));
        h.cmd_id
    }

    /// A confirmed `SmokeOrder` (Buy) of `a` on `inst` written straight into the ring; asserts it
    /// went out.
    pub fn smoke_direct(&mut self, a: AccountId, inst: InstId) {
        let v = self.version();
        let c = ControlCmd::SmokeOrder {
            account: a,
            venue: self.rd.inst(inst).venue,
            _pad: 0,
            inst,
            side: hft_types::Side::Buy,
            _pad2: [0; 3],
            ttl_ms: 600_000,
        };
        let id = self.direct("cli:alice", CmdHdr::ROLE_ADMIN, a, &c, |h| {
            h.confirmed = 1;
            h.expect_state_version = v;
        });
        self.settle();
        let st = self
            .results_since(0)
            .into_iter()
            .find(|r| r.cmd_id == id)
            .map(|r| r.status);
        assert_eq!(st, Some(0), "smoke order");
    }

    /// Keeps a later `Drain` from finishing: a `SmokeOrder` of `acct-b` on `binance-spot` that the
    /// venue never answers (a `PendingNew` slot; Drain waits for it).
    pub fn hold_drain(&mut self) {
        let f = self.fills;
        self.fills = Fills::Silent;
        let v = self.version();
        let c = ControlCmd::SmokeOrder {
            account: ACCT_B,
            venue: V_BN_SPOT,
            _pad: 0,
            inst: I_BN_SPOT,
            side: hft_types::Side::Buy,
            _pad2: [0; 3],
            ttl_ms: 600_000,
        };
        let id = self.direct("cli:alice", CmdHdr::ROLE_ADMIN, ACCT_B, &c, |h| {
            h.confirmed = 1;
            h.expect_state_version = v;
        });
        self.settle();
        self.fills = f;
        let st = self
            .results_since(0)
            .into_iter()
            .find(|r| r.cmd_id == id)
            .map(|r| r.status);
        assert_eq!(st, Some(0), "hold_drain: smoke order");
    }

    /// Ack and a full fill of `d` at `price`.
    pub fn fill_at(&mut self, d: &OrderDecision, price: &str) {
        let ring = self.rep_ring(d.account);
        let ack = self.order_event(d, OrderEventKind::Ack);
        self.push(ring, &ack);
        let mut f = self.order_event(d, OrderEventKind::Fill);
        f.cum_filled = d.qty;
        f.px = px(price);
        f.avg_px = f.px;
        self.push(ring, &f);
    }

    /// A venue `Reject` of `d`.
    pub fn reject(&mut self, d: &OrderDecision) {
        let ring = self.rep_ring(d.account);
        let e = self.order_event(d, OrderEventKind::Reject);
        self.push(ring, &e);
    }

    /// Quotes that make `probe-taker` (`s2-main`, `g2`) open: leg A's bid 0.18 % above leg B's
    /// ask.
    pub fn taker_signal(&mut self) {
        self.quote(I_BG_FUT, "50100", "50101");
        self.quote(I_BN_SPOT, "50010", "50011");
    }

    /// `g2` Broken (and `HedgeFailure` on `acct-b`): the first leg fills, every hedge attempt is
    /// rejected until the link breaks.
    pub fn broken_g2(&mut self) {
        let d = self.taker_first_leg();
        self.fills = Fills::Reject;
        self.fill_at(&d, "50100");
        self.settle();
        for _ in 0..60 {
            if self.has_cap(hft_types::CapSource::BROKEN, ACCT_B) {
                break;
            }
            self.tick_ms(500);
        }
        self.fills = Fills::Auto;
        assert!(
            self.has_cap(hft_types::CapSource::BROKEN, ACCT_B),
            "g2 not Broken"
        );
    }

    /// `acct-b`'s first leg (sell `bitget-usdt-futures`), sent and not answered.
    pub fn taker_first_leg(&mut self) -> OrderDecision {
        let f = self.fills;
        self.fills = Fills::Silent;
        let m = self.mark();
        self.taker_signal();
        self.settle();
        self.fills = f;
        let d = self.new_orders_since(m, I_BG_FUT);
        assert_eq!(d.len(), 1, "probe-taker first leg");
        d[0]
    }

    /// Accepted new orders of `inst` since a mark.
    pub fn new_orders_since(&self, m: usize, inst: InstId) -> Vec<OrderDecision> {
        self.decisions_since(m)
            .into_iter()
            .filter(|d| d.inst == inst && d.action == OrderAction::New && d.reject == 0)
            .collect()
    }

    // --------------------------------------------------------------------------- state

    pub fn version(&self) -> u64 {
        self.s.engine().risk.mode.ctl_state_version()
    }
    pub fn mode(&self, s: StrategyId, a: AccountId, g: GroupId) -> Effective {
        self.s.engine().risk.mode.effective(CellKey {
            strategy: s,
            account: a,
            group: g,
        })
    }
    pub fn desired(&self, s: StrategyId, a: AccountId, g: GroupId) -> Option<RunMode> {
        self.s.engine().risk.mode.desired(CellKey {
            strategy: s,
            account: a,
            group: g,
        })
    }
    pub fn has_cap(&self, source: hft_types::CapSource, account: AccountId) -> bool {
        self.s
            .engine()
            .risk
            .mode
            .caps()
            .iter()
            .any(|c| c.source == source && c.scope.account == account)
    }
    pub fn caps_of(&self, source: hft_types::CapSource) -> usize {
        self.s
            .engine()
            .risk
            .mode
            .caps()
            .iter()
            .filter(|c| c.source == source)
            .count()
    }
    pub fn params_version(&self, s: StrategyId) -> u64 {
        self.s.instances().get(s).map_or(0, |x| x.params_version())
    }
    /// Records of a kind since a mark.
    pub fn kinds_since(&self, m: usize, k: EventKind) -> Vec<MemRecord> {
        self.since(m).into_iter().filter(|r| r.kind == k).collect()
    }
    pub fn decisions_since(&self, m: usize) -> Vec<OrderDecision> {
        self.kinds_since(m, EventKind::ORDER_DECISION)
            .iter()
            .filter_map(|r| hft_sequencer::types::read_body::<OrderDecision>(&r.body))
            .collect()
    }
    pub fn results_since(&self, m: usize) -> Vec<hft_event::CmdResult> {
        self.kinds_since(m, EventKind::CMD_RESULT)
            .iter()
            .filter_map(|r| r.read::<hft_event::CmdResult>())
            .collect()
    }
    /// The journal's `CmdResult` of a cmd_id (the last one when it was answered twice).
    pub fn jres(&self, id: Uuid) -> Option<hft_event::CmdResult> {
        self.results_since(0)
            .into_iter()
            .rfind(|r| r.cmd_id == *id.as_bytes())
    }
    pub fn notes_since(&self, m: usize) -> Vec<hft_event::Note> {
        self.kinds_since(m, EventKind::NOTE)
            .iter()
            .filter_map(|r| r.read::<hft_event::Note>())
            .collect()
    }

    // --------------------------------------------------------------------------- positions

    /// `g1` gets a position: `probe-target`'s TakerSlice point (`t_slice_ms` of the start
    /// parameters) is reached and both legs fill (+1 lot `bitget-spot`, −1 lot `binance-usdm`).
    pub fn position_g1(&mut self) {
        let t = T_SLICE_MS;
        let ens_now = self.s.engine().now.engine_ns.max(0);
        if ens_now < t * MS as i64 {
            self.tick_ms(((t * MS as i64 - ens_now) / MS as i64) as u64 + 1);
        }
        self.tick_ms(5_000);
    }

    /// Advances `ms` of virtual time in 500 ms steps keeping the run healthy: quotes every step
    /// (BBO connections stay live), margin reports of the Live accounts every 5 s.
    pub fn tick_ms(&mut self, ms: u64) {
        let mut left = ms;
        let mut since_margin = 0u64;
        while left > 0 {
            let d = left.min(500);
            self.adv_ms(d);
            left -= d;
            since_margin += d;
            self.quotes();
            if since_margin >= 5_000 {
                since_margin = 0;
                for a in [ACCT_A, ACCT_B] {
                    self.margin(a);
                }
            }
            self.settle();
        }
    }

    /// Net quantity of leg `leg_ix` of `(a, g)` in the ledger.
    pub fn leg_net(&self, a: AccountId, g: GroupId, leg_ix: u8) -> i64 {
        self.s.engine().ledger.leg_net(a, g, leg_ix).0
    }

    // --------------------------------------------------------------------------- requests

    /// A request of the G0 conventions at the current virtual time.
    pub fn req(&self, issuer: &str, account: Option<&str>, cmd: Value) -> Value {
        let mut v = json!({
            "v": 2,
            "cmd_id": crate::common::new_id().hyphenated().to_string(),
            "issuer": issuer,
            "issued_at_ns": self.utc(),
            "expires_at_ns": self.utc() + 30 * SEC as i64,
            "cmd": cmd,
        });
        if let Some(a) = account {
            v["account"] = json!(a);
        }
        if issuer.starts_with("tg:") {
            v["chat_type"] = json!("private");
        }
        v
    }

    pub fn start(&mut self, c: Call) -> Ticket {
        let now = Tsc(self.now());
        self.front.as_mut().expect("gateway is down").start(c, now)
    }

    /// Pumps until the ticket's response arrives.
    pub fn finish(&mut self, mut t: Ticket) -> CmdResponse {
        for _ in 0..100_000 {
            if let Some(r) = self.front.as_mut().expect("gateway is down").done(&mut t) {
                return r;
            }
            if !self.round() {
                self.front.as_mut().expect("gateway is down").idle_wait();
            }
        }
        panic!("no response");
    }

    pub fn call(&mut self, c: Call) -> CmdResponse {
        let t = self.start(c);
        let r = self.finish(t);
        self.settle();
        r
    }

    /// `send` of a request built now.
    pub fn send(&mut self, issuer: &str, account: Option<&str>, cmd: Value) -> CmdResponse {
        let v = self.req(issuer, account, cmd);
        self.call(Call::Send(v))
    }
    pub fn send_v(&mut self, v: Value) -> CmdResponse {
        self.call(Call::Send(v))
    }
    pub fn preview(&mut self, issuer: &str, account: Option<&str>, cmd: Value) -> CmdResponse {
        let v = self.req(issuer, account, cmd);
        self.call(Call::Preview(v))
    }
    pub fn confirm(
        &mut self,
        issuer: &str,
        account: Option<&str>,
        cmd: Value,
        sv: Option<u64>,
    ) -> CmdResponse {
        let v = self.req(issuer, account, cmd);
        self.call(Call::Confirm(v, sv))
    }
    /// Two-step confirmation: `preview()`, then `confirm()` with the preview's version.
    pub fn two_step(
        &mut self,
        issuer: &str,
        account: Option<&str>,
        cmd: Value,
    ) -> (CmdResponse, CmdResponse) {
        let p = self.preview(issuer, account, cmd.clone());
        let sv = p.ctl_state_version;
        let c = self.confirm(issuer, account, cmd, sv);
        (p, c)
    }
    pub fn result(&mut self, issuer: &str, id: Uuid) -> CmdResponse {
        self.call(Call::Result(issuer.to_owned(), id))
    }

    /// A `ReconcileReport` group from `reconciler`.
    pub fn report(&mut self, report_id: u64, items: Value) -> CmdResponse {
        self.send(
            "reconciler",
            None,
            json!({"type":"ReconcileReport","report_id":report_id.to_string(),"items":items}),
        )
    }

    /// Seq of the last journal record.
    pub fn last_seq(&self) -> u64 {
        self.io_ref().j.next_seq.saturating_sub(1)
    }

    /// Verify replay of the journal from its first snapshot: the mismatching and never reached
    /// Outputs (empty = the replay reproduces every Output, `CmdResult`s included).
    pub fn verify(&self) -> Vec<hft_sequencer::driver::replay::Mismatch> {
        use hft_sequencer::driver::replay::{Log, ReplayDriver, ReplayMode};
        use hft_sequencer::{RecoverPlan, RecoveryFlags, SnapshotSel};
        let j = &self.io_ref().j;
        let log = Log::from_segments(&j.segments);
        let first = log
            .recs
            .iter()
            .find(|r| r.kind == EventKind::STATE_SNAPSHOT)
            .map_or(0, |r| r.seq);
        let drv = ReplayDriver::new(ReplayMode::Verify, log, 0, self.setup.seq.replay_per_round);
        let p = RecoverPlan {
            start: SnapshotSel::AtSeq(first),
            replay_tail: true,
            epoch: 0,
            flags: RecoveryFlags(0),
            quarantine: Vec::new(),
        };
        let mut s: Sequencer<ReplayDriver, Set> =
            Sequencer::boot(BootSource::Journal(p), &self.setup, drv, hb())
                .unwrap_or_else(|e| panic!("verify boot: {e:?}"));
        for _ in 0..1_000_000 {
            if !matches!(s.step(), StepOutcome::Busy) {
                break;
            }
        }
        let d = s.driver();
        for (seq, a, b) in d.sink.detail.iter().take(2) {
            let off = a.iter().zip(b.iter()).position(|(x, y)| x != y);
            eprintln!(
                "verify mismatch at {seq}: len {} vs {}, first diff at {off:?}",
                a.len(),
                b.len()
            );
            if let Some(o) = off {
                let lo = o.saturating_sub(32);
                eprintln!(
                    "  log {:?}
  got {:?}",
                    &a[lo..(o + 32).min(a.len())],
                    &b[lo..(o + 32).min(b.len())]
                );
            }
            if let (Ok(x), Ok(y)) = (
                hft_sequencer::snapshot::read_seq_sections(&a[96..]),
                hft_sequencer::snapshot::read_seq_sections(&b[96..]),
            ) {
                let (xs, ys) = (format!("{x:#?}"), format!("{y:#?}"));
                for (l1, l2) in xs.lines().zip(ys.lines()) {
                    if l1 != l2 {
                        eprintln!(
                            "  - {l1}
  + {l2}"
                        );
                    }
                }
            }
        }
        let mut v = d.mismatches().to_vec();
        v.extend(d.leftover());
        v
    }

    /// Stops the gateway task (its socket closes, the socket file stays; the sequencer keeps
    /// running). Linux: returns the socket path the gateway listened on.
    pub fn gateway_down(&mut self) -> Option<std::path::PathBuf> {
        #[cfg(target_os = "linux")]
        {
            let mut f = self.front.take()?;
            f.keep_dir = true;
            let p = f.socket.clone();
            drop(f);
            Some(p)
        }
        #[cfg(not(target_os = "linux"))]
        {
            self.front = None;
            None
        }
    }

    /// One gateway poll (no sequencer round).
    pub fn gateway_poll(&mut self) -> bool {
        let now = Tsc(self.now());
        self.front.as_mut().is_some_and(|f| f.poll(now))
    }

    /// A new gateway (empty caches) over the same ring and journal tail.
    pub fn restart_gateway(&mut self) {
        let ((p_send, p_take), (r_send, r_take)) = mailboxes();
        {
            let io = self.io();
            io.parsed = p_take;
            io.retired = r_send;
        }
        let active = start_params()
            .iter()
            .map(|p| ParsedConfig::StrategyParams(Box::new(p.parsed(&self.rd))))
            .collect();
        let prev = self.front.take();
        self.front = Some(Front::new(
            self.rd.clone(),
            self.tsc.clone(),
            self.ctl.clone(),
            p_send,
            r_take,
            active,
            prev,
        ));
    }
}

/// `op 2` (reduce-only) of `s1-main` (CTL-121's request list).
pub fn strategy_channel_op2() -> Vec<u8> {
    strategy_channel::op2()
}

/// A command scope; `GroupId(0)` = every group.
pub fn scope(s: StrategyId, a: AccountId, g: GroupId) -> CmdScope {
    let group = if g.0 == 0 { hft_types::GROUP_ALL } else { g };
    CmdScope {
        strategy: s,
        venue: VenueId(0),
        account: a,
        _pad: 0,
        group,
        inst: InstId(0),
        _pad2: 0,
    }
}

pub fn recon_summary(a: AccountId, v: VenueId, report_id: u64, view_seq: u64) -> ReconcileReport {
    let mut x = ReconcileReport::new_zeroed();
    x.report_id = report_id;
    x.venue = v;
    x.account = a;
    x.item = 5;
    x.count = 1;
    x.view_seq = view_seq;
    x
}

// ------------------------------------------------------------------------------- response helpers

pub fn code(r: &CmdResponse) -> Option<ResultCode> {
    r.code
}
pub fn is(r: &CmdResponse, c: ResultCode) -> bool {
    r.status == TransportStatus::Ok && r.code == Some(c)
}
pub fn detail(r: &CmdResponse) -> String {
    r.detail.as_ref().map_or(String::new(), |d| d.name.clone())
}
pub fn arg(r: &CmdResponse) -> String {
    r.detail.as_ref().and_then(|d| d.arg.clone()).unwrap_or_default()
}
/// `"<code>"` or `"<code>{<detail>}"`.
pub fn show(r: &CmdResponse) -> String {
    let c = r
        .code
        .map_or_else(|| format!("{:?}", r.status), |c| c.name().to_owned());
    match &r.detail {
        Some(d) => format!("{c}{{{}}}", d.name),
        None => c,
    }
}
/// Asserts the code (and detail name, when given) of a response.
#[track_caller]
pub fn want(r: &CmdResponse, code: &str, det: Option<&str>, row: &str) {
    let c = r.code.map_or("", |c| c.name());
    assert_eq!(c, code, "{row}: {}", show(r));
    if let Some(d) = det {
        assert_eq!(detail(r), d, "{row}: {}", show(r));
    }
}
pub fn rflags(r: &CmdResponse) -> Option<Value> {
    r.preview
        .as_ref()
        .and_then(|p| serde_json::from_str::<Value>(p.get()).ok())
}
pub fn uuid(v: &Value) -> Uuid {
    Uuid::parse_str(v["cmd_id"].as_str().unwrap_or("")).unwrap_or(Uuid::nil())
}
