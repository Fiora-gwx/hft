//! The simplified sequencer (README "简化 sequencer"): single writer of the seq journal; inputs
//! `hk > rep > bbo`, one input = one event; quote probes throttled by `Bbo` events, cancel in the
//! same event as the `Ack`, `LatencySample` at the first exchange answer of each operation.

use hft_event::{
    Bbo, ClockStatus, ConnRole, ConnState, ConnStateKind, FeedCmd, FeedMsg, GwCmd, GwOp, HkMsg,
    LatencySample, OrderAction, OrderDecision, OrderEvent, OrderEventKind, OrderFlags, RecordFlags, RepMsg,
    SendReport, SendStatus,
};
use hft_journal::{JournalWriter, RollReason};
use hft_rt::{Consumer, Producer, TscUtc};
use hft_types::{
    AccountId, AssetExp, GroupId, InstId, Notional, Px, Qty, RoundMode, Side, SlotId, StrategyId, Tif, Tsc,
    VenueId,
};
use zerocopy::FromZeros;

use crate::config::QuoteSide;
use crate::journal::{Boot, roll_info};

/// Probe slots (`orders.max_open_orders` ≤ this).
pub const SLOTS: usize = 64;

/// `EventKind` codes of the triggers.
const KIND_BBO: u16 = hft_event::EventKind::BBO.0;
const KIND_ORDER_EVENT: u16 = hft_event::EventKind::ORDER_EVENT.0;

/// `LatencySample.src` codes (hft-event `LatencySrc`).
const SRC_HW: u32 = 1;
const SRC_SW: u32 = 2;
const SRC_TSC: u32 = 3;

/// The counter clock of the sequencer (`proc_tsc`, `decided_tsc`).
pub trait Clock {
    fn now(&mut self) -> u64;
}

/// `hft_rt::tsc_now`.
#[derive(Copy, Clone, Debug, Default)]
pub struct RealClock;

impl Clock for RealClock {
    #[inline]
    fn now(&mut self) -> u64 {
        hft_rt::tsc_now().0
    }
}

/// One probed inst.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct SeqInst {
    pub inst: InstId,
    pub tick: i64,
    pub lot: i64,
    pub min_qty: i64,
    pub min_notional: Notional,
    pub exp: AssetExp,
}

/// Sequencer parameters (from the configuration).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeqCfg {
    pub venue: VenueId,
    pub account: AccountId,
    pub insts: Vec<SeqInst>,
    pub orders_enabled: bool,
    pub probe_period_ms: u32,
    pub burst_period_ms: u32,
    pub far_bp: u32,
    pub quote_side: QuoteSide,
    pub spot: bool,
    pub max_spread_bp: u32,
    pub size_margin_pct: u32,
    pub quote_rest_ms: u32,
    pub max_consec_fail: u32,
    pub max_open_orders: u16,
    pub max_order_notional: Notional,
    pub max_orders: u32,
    pub ts_expect_hw: bool,
    /// Counter ticks per millisecond.
    pub ticks_per_ms: u64,
    pub hz_milli: u64,
}

/// Why probing stopped for safety (README "安全约束" 4).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Halt {
    NotionalCap,
    ConsecFail,
    Stuck,
    Fill,
}

/// Counters of one run.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct SeqStats {
    pub bbo: u64,
    pub placed: u64,
    pub acks: u64,
    pub rejects: u64,
    pub unknown: u64,
    pub cancels_sent: u64,
    pub cancel_done: u64,
    pub cancel_rejects: u64,
    pub send_fail: u64,
    pub skipped_spread: u64,
    pub samples_new: u64,
    pub samples_cancel: u64,
    pub out_full: u64,
    pub append_err: u64,
    pub rolls: u64,
    pub conn_states: u64,
    pub clock_status: u64,
}

/// The record that triggered an operation (points 0–4 of the sample).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
struct Trig {
    seq: u64,
    kind: u16,
    ex_ts: i64,
    ex_send: i64,
    hw: i64,
    sw: i64,
    rx_tsc: u64,
    proc_tsc: u64,
    hw_src: u32,
}

/// One operation (new or cancel) of a probe order.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
struct Op {
    trig: Trig,
    decided: u64,
    sent: u64,
    /// The first answer was seen (sample written).
    answered: bool,
    /// The command went out (decision made).
    issued: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
enum OState {
    #[default]
    Free,
    /// New sent, no answer yet.
    New,
    /// Acked (or outcome unknown), cancel issued.
    Cancelling,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
struct Order {
    st: OState,
    r#gen: u32,
    ix: usize,
    placed_at: u64,
    last_cancel_at: u64,
    cancel_tries: u8,
    new: Op,
    cxl: Op,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
struct InstState {
    bid: i64,
    ask: i64,
    last_probe: Option<u64>,
    open: Option<usize>,
    next_side_sell: bool,
}

/// Input rings of the sequencer.
#[derive(Debug)]
pub struct SeqIn {
    pub hk: Consumer<HkMsg>,
    pub rep: Consumer<RepMsg>,
    pub bbo: Consumer<FeedMsg>,
}

/// Output rings of the sequencer.
#[derive(Debug)]
pub struct SeqOut {
    pub gw: Producer<GwCmd>,
    pub feed: Producer<FeedCmd>,
}

/// The simplified sequencer.
pub struct Seq<C: Clock> {
    cfg: SeqCfg,
    clock: C,
    w: JournalWriter,
    boot: Boot,
    utc: TscUtc,
    input: SeqIn,
    out: SeqOut,
    insts: Vec<InstState>,
    orders: [Order; SLOTS],
    gens: [u32; SLOTS],
    gw_up: bool,
    last_bbo_rx: u64,
    consec_fail: u32,
    halted: Option<Halt>,
    draining: bool,
    stats: SeqStats,
    last_sweep: u64,
}

impl<C: Clock> std::fmt::Debug for Seq<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Seq").field("stats", &self.stats).finish()
    }
}

fn receive_source(value: i64, flags: u16) -> u32 {
    if value == 0 {
        0
    } else if flags & RecordFlags::HW_TS.0 != 0 {
        SRC_HW
    } else if flags & RecordFlags::SW_TS.0 != 0 {
        SRC_SW
    } else {
        0
    }
}

impl<C: Clock> Seq<C> {
    /// `w` must have its first segment rolled (with `boot`).
    pub fn new(
        cfg: SeqCfg,
        clock: C,
        w: JournalWriter,
        boot: Boot,
        utc: TscUtc,
        input: SeqIn,
        out: SeqOut,
    ) -> Self {
        let insts = vec![InstState::default(); cfg.insts.len()];
        Seq {
            cfg,
            clock,
            w,
            boot,
            utc,
            input,
            out,
            insts,
            orders: [Order::default(); SLOTS],
            gens: [0; SLOTS],
            gw_up: false,
            last_bbo_rx: 0,
            consec_fail: 0,
            halted: None,
            draining: false,
            stats: SeqStats::default(),
            last_sweep: 0,
        }
    }

    /// Subscribes the probed insts (BBO stream) on the feed task.
    pub fn start(&mut self) {
        for i in 0..self.cfg.insts.len() {
            let inst = self.cfg.insts[i].inst;
            let _ = self.out.feed.try_push(FeedCmd::Subscribe {
                inst,
                streams: hft_venue::streams::BBO,
            });
        }
    }

    pub fn stats(&self) -> SeqStats {
        self.stats
    }

    pub fn halted(&self) -> Option<Halt> {
        self.halted
    }

    /// Unanswered or uncancelled probe orders.
    pub fn open_orders(&self) -> usize {
        self.orders.iter().filter(|o| o.st != OState::Free).count()
    }

    /// No probe order left after `begin_drain`.
    pub fn drained(&self) -> bool {
        self.draining && self.open_orders() == 0
    }

    /// The journal writer (to seal it at the end).
    pub fn into_writer(self) -> JournalWriter {
        self.w
    }

    /// Stops probing and cancels every open probe order (README "收尾顺序").
    pub fn begin_drain(&mut self, now: u64) {
        self.draining = true;
        self.cancel_all(now, None);
        self.end(now);
    }

    fn cancel_all(&mut self, now: u64, trig: Option<Trig>) {
        for k in 0..SLOTS {
            let o = self.orders[k];
            if o.st == OState::Free || o.cxl.issued {
                continue;
            }
            self.issue_cancel(k, now, trig.unwrap_or_default());
        }
    }

    fn halt(&mut self, h: Halt, now: u64, trig: Option<Trig>) {
        if self.halted.is_none() {
            self.halted = Some(h);
        }
        self.cancel_all(now, trig);
    }

    /// One input (by priority) or one timer sweep; whether anything was done.
    pub fn poll(&mut self) -> bool {
        if let Some(m) = self.input.hk.try_pop() {
            self.on_hk(m);
            return true;
        }
        if let Some(m) = self.input.rep.try_pop() {
            self.on_rep(m);
            return true;
        }
        if let Some(m) = self.input.bbo.try_pop() {
            self.on_feed(m);
            return true;
        }
        let now = self.clock.now();
        if now.saturating_sub(self.last_sweep) >= self.cfg.ticks_per_ms {
            self.last_sweep = now;
            return self.sweep(now);
        }
        false
    }

    fn append<B: hft_event::Body>(&mut self, flags: u16, tsc: u64, b: &B) -> u64 {
        match self.w.append(RecordFlags(flags), Tsc(tsc), b) {
            Ok(s) => s.0,
            Err(_) => {
                self.stats.append_err += 1;
                0
            }
        }
    }

    fn meta<B: hft_event::Body>(&mut self, tsc: u64, b: &B) {
        if self.w.append_meta(Tsc(tsc), b).is_err() {
            self.stats.append_err += 1;
        }
    }

    fn end(&mut self, now: u64) {
        self.w.end_event();
        self.w.publish();
        if let Some((reason, _)) = self.w.needs_roll(Tsc(now)) {
            self.roll(reason, now);
        }
    }

    fn roll(&mut self, reason: RollReason, now: u64) {
        let utc = self.utc.utc_ns(Tsc(now));
        let configs = [self.boot.blob_ref];
        let info = roll_info(now, utc, self.cfg.hz_milli, &configs);
        let boot = self.boot.clone();
        if boot.roll(&mut self.w, reason, &info).is_ok() {
            self.stats.rolls += 1;
        }
    }

    fn on_hk(&mut self, m: HkMsg) {
        let now = self.clock.now();
        match m {
            HkMsg::ClockStatus(c) => {
                self.stats.clock_status += 1;
                self.meta(now, &c);
                self.w.publish();
            }
            HkMsg::Note(n) => {
                self.meta(now, &n);
                self.w.publish();
            }
            HkMsg::Sys { .. } => {}
        }
    }

    /// A `ClockStatus` written directly (tests and the dry run drive it through `hk` as well).
    pub fn clock_status(&mut self, c: &ClockStatus) {
        let now = self.clock.now();
        self.meta(now, c);
        self.w.publish();
    }

    fn on_feed(&mut self, m: FeedMsg) {
        let now = self.clock.now();
        match m {
            FeedMsg::Bbo(b, flags) => self.on_bbo(&b, flags.0, now),
            FeedMsg::ConnState(c) => {
                self.stats.conn_states += 1;
                self.append(0, now, &c);
                self.end(now);
            }
            _ => {
                self.end(now);
            }
        }
    }

    fn inst_ix(&self, inst: InstId) -> Option<usize> {
        self.cfg.insts.iter().position(|i| i.inst == inst)
    }

    fn on_bbo(&mut self, b: &Bbo, flags: u16, now: u64) {
        self.stats.bbo += 1;
        let seq = self.append(flags, now, b);
        let burst = b.h.rx_tsc != 0 && b.h.rx_tsc == self.last_bbo_rx;
        self.last_bbo_rx = b.h.rx_tsc;
        if let Some(ix) = self.inst_ix(b.h.inst) {
            self.insts[ix].bid = b.bid_px.0;
            self.insts[ix].ask = b.ask_px.0;
            let trig = Trig {
                seq,
                kind: KIND_BBO,
                ex_ts: b.h.ex_ts_ns,
                ex_send: b.h.ex_send_ns,
                hw: b.h.hw_rx_ns,
                sw: b.h.sw_rx_ns,
                rx_tsc: b.h.rx_tsc,
                proc_tsc: now,
                hw_src: receive_source(b.h.hw_rx_ns, flags),
            };
            self.maybe_probe(ix, now, burst, trig);
        }
        self.end(now);
    }

    fn period_ticks(&self, ms: u32) -> u64 {
        u64::from(ms).saturating_mul(self.cfg.ticks_per_ms)
    }

    fn maybe_probe(&mut self, ix: usize, now: u64, burst: bool, trig: Trig) {
        if !self.cfg.orders_enabled || self.halted.is_some() || self.draining || !self.gw_up {
            return;
        }
        let st = self.insts[ix];
        let due = match st.last_probe {
            None => true,
            Some(t) => {
                now.saturating_sub(t) >= self.period_ticks(self.cfg.probe_period_ms)
                    || (self.cfg.burst_period_ms != 0
                        && burst
                        && now.saturating_sub(t) >= self.period_ticks(self.cfg.burst_period_ms))
            }
        };
        if !due || st.open.is_some() {
            return;
        }
        if self.open_orders() >= usize::from(self.cfg.max_open_orders)
            || self.stats.placed >= u64::from(self.cfg.max_orders)
        {
            return;
        }
        if st.bid <= 0 || st.ask <= 0 {
            return;
        }
        let mid = Px::mid_floor(Px(st.bid), Px(st.ask)).0;
        let spread = i128::from(st.ask) - i128::from(st.bid);
        if mid <= 0 || spread * 10_000 > i128::from(self.cfg.max_spread_bp) * i128::from(mid) {
            self.stats.skipped_spread += 1;
            return;
        }
        let si = self.cfg.insts[ix];
        let sell = match self.cfg.quote_side {
            QuoteSide::Buy => false,
            QuoteSide::Sell => true,
            QuoteSide::Alternate => st.next_side_sell,
        } && !self.cfg.spot;
        let far = i128::from(self.cfg.far_bp);
        let px = if sell {
            // ceil_to_tick(bid × (1 + far))
            let raw = (i128::from(st.bid) * (10_000 + far) + 9_999) / 10_000;
            let t = i128::from(si.tick);
            (raw + t - 1) / t * t
        } else {
            let raw = i128::from(st.ask) * (10_000 - far) / 10_000;
            let t = i128::from(si.tick);
            raw / t * t
        };
        let Ok(px) = i64::try_from(px) else { return };
        if px <= 0 {
            return;
        }
        let Some(qty) = min_qty(&si, px, self.cfg.size_margin_pct) else {
            return;
        };
        let (notional, _) = Px(px).mul_qty(Qty(qty), si.exp, RoundMode::Ceil);
        self.insts[ix].last_probe = Some(now);
        if notional.0 > self.cfg.max_order_notional.0 {
            self.halt(Halt::NotionalCap, now, None);
            return;
        }
        let Some(k) = self.orders.iter().position(|o| o.st == OState::Free) else {
            return;
        };
        self.gens[k] = self.gens[k].wrapping_add(1).max(1);
        let r#gen = self.gens[k];
        let slot = (k + 1) as u32;
        let side = if sell { Side::Sell } else { Side::Buy };
        let decided = self.clock.now();
        let d = OrderDecision {
            trigger_seq: trig.seq,
            decided_tsc: Tsc(decided),
            px: Px(px),
            qty: Qty(qty),
            group: GroupId(0),
            inst: si.inst,
            slot: SlotId(slot),
            r#gen,
            target_slot: SlotId(0),
            reject: 0,
            strategy: StrategyId(0),
            venue: self.cfg.venue,
            account: self.cfg.account,
            action: OrderAction::New,
            side,
            tif: Tif::PostOnly,
            oflags: OrderFlags::POST_ONLY.0,
            reason: 0,
            intent_no: 0,
            req_px: Px(px),
            link_role: 0,
            _pad: [0; 7],
        };
        let dseq = self.append(0, now, &d);
        let c = GwCmd {
            decision_seq: dseq,
            decided_tsc: Tsc(decided),
            px: Px(px),
            qty: Qty(qty),
            inst: si.inst,
            slot: SlotId(slot),
            r#gen,
            target_slot: SlotId(0),
            op: GwOp::Place,
            account: self.cfg.account,
            side,
            tif: Tif::PostOnly,
            oflags: OrderFlags::POST_ONLY.0,
            _pad: [0; 2],
            arg: 0,
            arg2: 0,
        };
        if self.out.gw.try_push(c).is_err() {
            self.stats.out_full += 1;
            return;
        }
        self.stats.placed += 1;
        if self.cfg.quote_side == QuoteSide::Alternate {
            self.insts[ix].next_side_sell = !self.insts[ix].next_side_sell;
        }
        self.insts[ix].open = Some(k);
        self.orders[k] = Order {
            st: OState::New,
            r#gen,
            ix,
            placed_at: now,
            last_cancel_at: 0,
            cancel_tries: 0,
            new: Op {
                trig,
                decided,
                sent: 0,
                answered: false,
                issued: true,
            },
            cxl: Op::default(),
        };
    }

    /// Issues (or re-issues) the cancel of slot index `k`.
    fn issue_cancel(&mut self, k: usize, now: u64, trig: Trig) {
        let o = self.orders[k];
        let si = self.cfg.insts[o.ix];
        let decided = self.clock.now();
        let slot = (k + 1) as u32;
        let d = OrderDecision {
            trigger_seq: trig.seq,
            decided_tsc: Tsc(decided),
            px: Px(0),
            qty: Qty(0),
            group: GroupId(0),
            inst: si.inst,
            slot: SlotId(slot),
            r#gen: o.r#gen,
            target_slot: SlotId(slot),
            reject: 0,
            strategy: StrategyId(0),
            venue: self.cfg.venue,
            account: self.cfg.account,
            action: OrderAction::Cancel,
            side: Side::Buy,
            tif: Tif::Gtc,
            oflags: 0,
            reason: 0,
            intent_no: 0,
            req_px: Px(0),
            link_role: 0,
            _pad: [0; 7],
        };
        let dseq = self.append(0, now, &d);
        let c = GwCmd {
            decision_seq: dseq,
            decided_tsc: Tsc(decided),
            px: Px(0),
            qty: Qty(0),
            inst: si.inst,
            slot: SlotId(slot),
            r#gen: o.r#gen,
            target_slot: SlotId(slot),
            op: GwOp::Cancel,
            account: self.cfg.account,
            side: Side::Buy,
            tif: Tif::Gtc,
            oflags: 0,
            _pad: [0; 2],
            arg: 0,
            arg2: 0,
        };
        if self.out.gw.try_push(c).is_err() {
            self.stats.out_full += 1;
            return;
        }
        self.stats.cancels_sent += 1;
        let o = &mut self.orders[k];
        o.st = OState::Cancelling;
        o.last_cancel_at = now;
        o.cancel_tries = o.cancel_tries.saturating_add(1);
        if !o.cxl.issued {
            o.cxl = Op {
                trig,
                decided,
                sent: 0,
                answered: false,
                issued: true,
            };
        }
    }

    fn free(&mut self, k: usize) {
        let ix = self.orders[k].ix;
        if self.insts.get(ix).and_then(|s| s.open) == Some(k) {
            self.insts[ix].open = None;
        }
        self.orders[k] = Order::default();
    }

    fn on_rep(&mut self, m: RepMsg) {
        let now = self.clock.now();
        match m {
            RepMsg::SendReport(r) => self.on_send(&r, now),
            RepMsg::OrderEvent(e, flags) => self.on_order(&e, flags.0, now),
            RepMsg::ConnState(c) => self.on_conn(&c, now),
            _ => self.end(now),
        }
    }

    fn on_conn(&mut self, c: &ConnState, now: u64) {
        self.stats.conn_states += 1;
        self.append(0, now, c);
        if c.role == ConnRole::Order && c.inst.0 == 0 {
            match c.state {
                ConnStateKind::Up => self.gw_up = true,
                ConnStateKind::Down => self.gw_up = false,
                _ => {}
            }
        }
        self.end(now);
    }

    fn slot_ix(&self, slot: u32, r#gen: u32) -> Option<usize> {
        let k = (slot as usize).checked_sub(1)?;
        let o = self.orders.get(k)?;
        (o.st != OState::Free && o.r#gen == r#gen).then_some(k)
    }

    fn on_send(&mut self, r: &SendReport, now: u64) {
        self.append(0, now, r);
        let (slot, r#gen) = match r.op {
            GwOp::Place => (r.slot.0, r.r#gen),
            _ => (r.target_slot.0, r.target_gen),
        };
        if let Some(k) = self.slot_ix(slot, r#gen) {
            let ok = r.status == SendStatus::Written;
            match r.op {
                GwOp::Place => {
                    if ok {
                        self.orders[k].new.sent = r.sent_tsc.0;
                    } else {
                        self.stats.send_fail += 1;
                        self.free(k);
                    }
                }
                GwOp::Cancel => {
                    if ok {
                        if self.orders[k].cxl.sent == 0 {
                            self.orders[k].cxl.sent = r.sent_tsc.0;
                        }
                    } else {
                        self.stats.send_fail += 1;
                    }
                }
                _ => {}
            }
        }
        self.end(now);
    }

    fn rep_trig(&self, e: &OrderEvent, flags: u16, seq: u64, now: u64) -> Trig {
        Trig {
            seq,
            kind: KIND_ORDER_EVENT,
            ex_ts: e.ex_ts_ns,
            ex_send: 0,
            hw: e.hw_rx_ns,
            sw: if flags & RecordFlags::SW_TS.0 != 0 {
                e.hw_rx_ns
            } else {
                0
            },
            rx_tsc: e.rx_tsc,
            proc_tsc: now,
            hw_src: receive_source(e.hw_rx_ns, flags),
        }
    }

    fn on_order(&mut self, e: &OrderEvent, flags: u16, now: u64) {
        let seq = self.append(flags, now, e);
        let trig = self.rep_trig(e, flags, seq, now);
        if matches!(e.what, OrderEventKind::PartialFill | OrderEventKind::Fill) {
            if let Some(k) = self.slot_ix(e.slot.0, e.r#gen) {
                self.free(k);
            }
            self.halt(Halt::Fill, now, Some(trig));
            self.end(now);
            return;
        }
        let Some(k) = self.slot_ix(e.slot.0, e.r#gen) else {
            self.end(now);
            return;
        };
        let o = self.orders[k];
        match (o.st, e.what) {
            (OState::New, OrderEventKind::Ack) => {
                self.stats.acks += 1;
                self.consec_fail = 0;
                self.sample(k, false, e, flags, seq, now);
                self.orders[k].new.answered = true;
                self.issue_cancel(k, now, trig);
            }
            (OState::New, OrderEventKind::Reject) => {
                self.stats.rejects += 1;
                self.sample(k, false, e, flags, seq, now);
                self.free(k);
                self.consec_fail += 1;
                if self.consec_fail >= self.cfg.max_consec_fail {
                    self.halt(Halt::ConsecFail, now, Some(trig));
                }
            }
            (OState::New, OrderEventKind::OutcomeUnknown) => {
                self.stats.unknown += 1;
                self.orders[k].new.answered = true;
                self.issue_cancel(k, now, trig);
            }
            (OState::Cancelling, OrderEventKind::OutcomeUnknown) => {
                self.stats.unknown += 1;
            }
            (_, OrderEventKind::CancelAck | OrderEventKind::Expired | OrderEventKind::CancelReject) => {
                if o.cxl.issued && !o.cxl.answered {
                    self.sample(k, true, e, flags, seq, now);
                    self.orders[k].cxl.answered = true;
                }
                let gone = e.what != OrderEventKind::CancelReject || e.detail == 1;
                if gone {
                    self.stats.cancel_done += 1;
                    self.free(k);
                } else {
                    self.stats.cancel_rejects += 1;
                }
            }
            _ => {}
        }
        self.end(now);
    }

    /// `LatencySample` of the first answer of operation new (`cancel = false`) or cancel.
    fn sample(&mut self, k: usize, cancel: bool, ack: &OrderEvent, flags: u16, ack_seq: u64, now: u64) {
        let o = self.orders[k];
        let op = if cancel { o.cxl } else { o.new };
        if op.answered {
            return;
        }
        let t = op.trig;
        let p = |v: u64| if v != 0 { SRC_TSC } else { 0 };
        let mut src = 0u32;
        src |= u32::from(t.ex_ts != 0);
        src |= u32::from(t.ex_send != 0) << 2;
        src |= t.hw_src << 4;
        src |= p(t.rx_tsc) << 6;
        src |= p(t.proc_tsc) << 8;
        src |= p(op.decided) << 10;
        src |= p(op.sent) << 12;
        src |= receive_source(ack.hw_rx_ns, flags) << 14;
        let mut s = LatencySample::new_zeroed();
        s.trigger_seq = t.seq;
        s.ack_seq = ack_seq;
        s.slot = SlotId((k + 1) as u32);
        s.r#gen = o.r#gen;
        s.venue = self.cfg.venue;
        s.account = self.cfg.account;
        s.strategy = StrategyId(0);
        s.src = src;
        s.ex_ts_ns = t.ex_ts;
        s.ex_send_ns = t.ex_send;
        s.hw_rx_ns = t.hw;
        s.rx_tsc = Tsc(t.rx_tsc);
        s.proc_tsc = Tsc(t.proc_tsc);
        s.decided_tsc = Tsc(op.decided);
        s.sent_tsc = Tsc(op.sent);
        s.ack_hw_rx_ns = ack.hw_rx_ns;
        s.ack_rx_tsc = Tsc(ack.rx_tsc);
        s.ack_ex_ts_ns = ack.ex_ts_ns;
        s.sw_rx_ns = t.sw;
        s.link_role = 0;
        s.action = if cancel { 1 } else { 0 };
        s.trigger_kind = t.kind;
        self.meta(now, &s);
        if cancel {
            self.stats.samples_cancel += 1;
        } else {
            self.stats.samples_new += 1;
        }
    }

    /// Timer sweep: cancels of probe orders older than `quote_rest_ms` (README "简化 sequencer").
    fn sweep(&mut self, now: u64) -> bool {
        let rest = self.period_ticks(self.cfg.quote_rest_ms);
        let mut did = false;
        for k in 0..SLOTS {
            let o = self.orders[k];
            if o.st == OState::Free {
                continue;
            }
            let since = if o.cancel_tries == 0 {
                o.placed_at
            } else {
                o.last_cancel_at
            };
            if now.saturating_sub(since) < rest {
                continue;
            }
            did = true;
            if o.cancel_tries >= 3 {
                self.halt(Halt::Stuck, now, None);
                self.orders[k].last_cancel_at = now;
                continue;
            }
            self.issue_cancel(k, now, Trig::default());
        }
        if did {
            self.end(now);
        }
        did
    }
}

/// Smallest quantity (lot multiple, ≥ `min_qty`) with `qty × px ≥ min_notional × (1 + margin)`.
pub fn min_qty(si: &SeqInst, px: i64, margin_pct: u32) -> Option<i64> {
    // need_notional (exp −6) × 10^(−6 − px_exp − qty_exp) / px = qty raw, rounded up.
    let need = i128::from(si.min_notional.0) * i128::from(100 + margin_pct);
    let t = -6i32 - i32::from(si.exp.px_exp) - i32::from(si.exp.qty_exp);
    let (num, den) = if t >= 0 {
        (
            need * 10i128.checked_pow(u32::try_from(t).ok()?)?,
            i128::from(px) * 100,
        )
    } else {
        (
            need,
            i128::from(px) * 100 * 10i128.checked_pow(u32::try_from(-t).ok()?)?,
        )
    };
    if den <= 0 {
        return None;
    }
    let q = (num + den - 1) / den;
    let lot = i128::from(si.lot.max(1));
    let q = ((q + lot - 1) / lot * lot).max(i128::from(si.min_qty));
    let q = (q + lot - 1) / lot * lot;
    i64::try_from(q).ok()
}
