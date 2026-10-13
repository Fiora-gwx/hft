//! `MockVenue`: a matcher (the exchange truth) wrapped in the venue behaviour layer — rate limit,
//! reject-code table, dead-man countdown, market-data connection pongs, self-liquidation marker —
//! plus the fault plan's report perturbations (README "DST 的世界模型" 2). It plays exchange and gw
//! for Live accounts; Sim accounts use the same matcher through `SimActor` without faults.
//!
//! [`Matcher`] is the boundary to the matching core: `hft_sim::SimExchange` for engine runs (its
//! adapter lands with the sequencer adapter), [`ToyMatcher`] for framework self-tests.

use std::collections::{BTreeMap, VecDeque};

use hft_event::{
    ConnRole, ConnState, ConnStateKind, ExternalCause, ExternalFill, GwCmd, GwOp, OrderEvent, OrderEventKind,
    RepMsg, SendReport, SendStatus,
};
use hft_types::{AccountId, AssetId, ConnId, InstId, Px, Qty, Side, SlotId, Tif, Tsc, VenueId};
use rand_chacha::ChaCha8Rng;

use crate::dst::faults::{ConnKind, Fault};
use crate::dst::obs::TruthRow;
use crate::dst::sut::{InputRec, Source};
use crate::dst::world::{ActorId, RngExt, WorldSpec, actor_rng};

/// Reject rules of the behaviour layer's code table.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum RejectRule {
    /// Buy notional exceeds the free quote balance.
    InsufficientMargin,
    /// An error code the venue's table does not register (fault `UnknownCode`).
    Unregistered,
    /// HTTP 429 shape.
    RateLimited,
    /// Generic exchange reject (faults `SecondLegReject`, `PersistentSecondReject`).
    Generic,
}

/// Venue behaviour layer configuration.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct MockVenueCfg {
    pub venue: VenueId,
    /// New orders accepted per account per rolling second (token bucket); 0 = unlimited.
    pub rate_per_s: u32,
    /// Exchange codes of the reject rules (`OrderEvent.reject_code` = the code as `u32`).
    pub reject_codes: Vec<(RejectRule, i32)>,
    /// Dead-man countdown: all resting orders of an account are cancelled when no refresh came
    /// for this long; 0 = off.
    pub dead_man_ms: u32,
    /// Market-data connections answer a ping every this many ms (`ConnState{Alive}`).
    pub bbo_ping_ms: u32,
    /// Client-id prefix of the venue's own liquidation orders.
    pub liquidation_prefix: &'static str,
    /// Report latency (exchange + gw), virtual ns, uniform in the range.
    pub latency_ns: (u64, u64),
    /// Market-data latency.
    pub md_latency_ns: (u64, u64),
    /// Maximum wait for an already scheduled same-account Ack when the client id is absent.
    pub unmatched_hold_ms: u32,
}

impl MockVenueCfg {
    /// The W1 behaviour of a venue.
    pub fn w1(venue: VenueId) -> MockVenueCfg {
        MockVenueCfg {
            venue,
            rate_per_s: 50,
            reject_codes: vec![
                (RejectRule::InsufficientMargin, -2019),
                (RejectRule::Unregistered, 99_999),
                (RejectRule::RateLimited, 429),
                (RejectRule::Generic, -1013),
            ],
            dead_man_ms: 0,
            bbo_ping_ms: 1_000,
            liquidation_prefix: "autoclose-",
            latency_ns: (1_000_000, 3_000_000),
            md_latency_ns: (200_000, 800_000),
            unmatched_hold_ms: 5_000,
        }
    }
    pub fn code(&self, r: RejectRule) -> u32 {
        self.reject_codes
            .iter()
            .find(|(k, _)| *k == r)
            .map_or(1, |(_, c)| *c as u32)
    }
}

/// One report of the matcher with the venue-level client id.
#[derive(Clone, Debug)]
pub struct VenueOut {
    pub due_ns: u64,
    pub rep: RepMsg,
    pub client_id: String,
}

/// Original external execution, captured when exchange truth changes. Transport replay must
/// reuse this receipt; it must not settle another fill or infer cash from a balance difference.
#[derive(Clone, Debug)]
pub struct ExternalReceipt {
    pub fill: ExternalFill,
    pub client_id: String,
}

/// Test-run evidence bound. Exhaustion fails before the next execution changes exchange truth.
pub const EXTERNAL_HISTORY_CAP: usize = 4096;

/// Disjoint order-id range for bounded external executions in the test exchange.
pub const EXTERNAL_ORDER_ID_BASE: u64 = u64::MAX - EXTERNAL_HISTORY_CAP as u64;

/// The matching core behind a `MockVenue`.
pub trait Matcher: Sized {
    fn new(world: &WorldSpec, venue: VenueId, seed: u64) -> Self;
    fn on_bbo(&mut self, inst: InstId, bid: (i64, i64), ask: (i64, i64), now_ns: u64);
    fn on_command(&mut self, cmd: &GwCmd, now_ns: u64);
    /// Reports due at or before `now_ns`, in due order.
    fn drain_due(&mut self, now_ns: u64, out: &mut Vec<VenueOut>);
    fn next_due(&self) -> Option<u64>;
    /// Exchange truth of `a` on this venue.
    fn truth(&self, a: AccountId) -> TruthRow;
    /// Last 64 generated fill reports per matcher, for failure diagnostics only. The sequence
    /// belongs to the exchange, before the venue adds latency or reorders delivery.
    fn matched_tail(&self) -> Vec<(u64, hft_event::OrderEvent)> {
        Vec::new()
    }
    fn accounts(&self) -> Vec<AccountId>;
    /// Resting orders of `a`: (slot, gen).
    fn open_orders(&self, a: AccountId) -> Vec<(SlotId, u32)>;
    fn cancel_all(&mut self, a: AccountId, now_ns: u64);
    /// Free quote balance of `a` (raw quote units).
    fn free_quote(&self, a: AccountId) -> i128;
    /// Executions injected through external(), in exchange order, including missed pushes.
    /// Reading the history changes neither exchange state nor transport scheduling.
    fn external_history(&self) -> &[ExternalReceipt];
    /// A fill not caused by the engine (liquidation, ADL, manual): changes truth exactly once,
    /// retains and returns its original receipt; `push` controls only the user-stream delivery.
    #[allow(clippy::too_many_arguments)]
    fn external(
        &mut self,
        a: AccountId,
        inst: InstId,
        side: Side,
        qty: i64,
        client_id: &str,
        push: bool,
        now_ns: u64,
    ) -> ExternalFill;
    /// Settle one independently identified external order before publishing either transport
    /// representation. A zero trade id uses the first execution's cumulative quantity as key.
    fn external_order(
        &mut self,
        a: AccountId,
        inst: InstId,
        side: Side,
        qty: i64,
        positive_trade_id: bool,
        now_ns: u64,
    ) -> Result<ExternalFill, String>;
    fn state_hash(&self) -> u64;
}

// ------------------------------------------------------------------------------- ToyMatcher

#[derive(Clone, Debug)]
struct TOrder {
    account: AccountId,
    slot: SlotId,
    r#gen: u32,
    epoch: u16,
    inst: InstId,
    side: Side,
    px: i64,
    qty: i64,
    filled: i64,
    ioc: bool,
    exch_id: u64,
}

/// Minimal deterministic matcher for framework self-tests: taker fills at the touch up to its
/// size, resting orders fill when the touch crosses them, fees at the venue's rate, spot
/// inventory and quote cash, signed perp positions (no PnL). Not a model of any exchange.
#[derive(Clone, Debug)]
pub struct ToyMatcher {
    venue: VenueId,
    fee_ppm: i64,
    fee_in_base: bool,
    insts: Vec<(InstId, AssetId, AssetId, bool)>,
    accounts: Vec<AccountId>,
    bbo: BTreeMap<u32, (i64, i64, i64, i64)>,
    orders: Vec<TOrder>,
    pending: VecDeque<VenueOut>,
    pos: BTreeMap<(u8, u32), i128>,
    inv: BTreeMap<(u8, u16), i128>,
    bal: BTreeMap<(u8, u16), i128>,
    next_exch: u64,
    next_trade: u64,
    external_history: Vec<ExternalReceipt>,
    latency_ns: u64,
    /// Fault `BaseFee`: fees in the base asset.
    pub base_fee: bool,
    /// Toy-only half-fill knob; the venue's `PartialFills` fault limits matching liquidity.
    pub partial: bool,
}

fn order_event(
    venue: VenueId,
    o: &TOrder,
    what: OrderEventKind,
    px: i64,
    qty: i64,
    fee: (i64, AssetId),
    now_ns: u64,
) -> OrderEvent {
    OrderEvent {
        venue,
        account: o.account,
        what,
        detail: 0,
        slot: o.slot,
        r#gen: o.r#gen,
        inst: o.inst,
        px: Px(px),
        qty: Qty(qty),
        cum_filled: Qty(o.filled),
        fee: Qty(fee.0),
        fee_asset: fee.1,
        epoch: o.epoch,
        reject_code: 0,
        exch_order_id: o.exch_id,
        ex_ts_ns: now_ns as i64,
        hw_rx_ns: 0,
        rx_tsc: 0,
        avg_px: Px(px),
        cash_qty: hft_types::Qty(0),
        cash_sum: hft_types::Qty(0),
        trade_id: 0,
    }
}

impl ToyMatcher {
    fn inst(&self, i: InstId) -> Option<(InstId, AssetId, AssetId, bool)> {
        self.insts.iter().copied().find(|x| x.0 == i)
    }
    fn push(&mut self, due_ns: u64, rep: RepMsg, client_id: String) {
        self.pending.push_back(VenueOut {
            due_ns,
            rep,
            client_id,
        });
    }
    /// Books a fill of `qty` at `px` for order `ix` and queues its report.
    fn fill(&mut self, ix: usize, px: i64, qty: i64, now_ns: u64) {
        let (inst, base, quote, perp) = match self.inst(self.orders[ix].inst) {
            Some(x) => x,
            None => return,
        };
        let o = &mut self.orders[ix];
        o.filled += qty;
        let a = o.account.0;
        let s: i128 = if o.side == Side::Buy { 1 } else { -1 };
        let notional = i128::from(qty) * i128::from(px) / 1_000_000;
        let base_fee = self.base_fee || self.fee_in_base;
        let fee = if base_fee {
            i128::from(qty) * i128::from(self.fee_ppm) / 1_000_000
        } else {
            notional * i128::from(self.fee_ppm) / 1_000_000
        };
        if perp {
            *self.pos.entry((a, inst.0)).or_default() += s * i128::from(qty);
            *self.bal.entry((a, quote.0)).or_default() -= if base_fee { 0 } else { fee };
        } else {
            *self.inv.entry((a, base.0)).or_default() += s * i128::from(qty) - if base_fee { fee } else { 0 };
            *self.bal.entry((a, quote.0)).or_default() -= s * notional + if base_fee { 0 } else { fee };
        }
        let o = self.orders[ix].clone();
        let what = if o.filled >= o.qty {
            OrderEventKind::Fill
        } else {
            OrderEventKind::PartialFill
        };
        let fee_asset = if base_fee { base } else { quote };
        let ev = order_event(self.venue, &o, what, px, qty, (fee as i64, fee_asset), now_ns);
        self.push(
            now_ns + self.latency_ns,
            RepMsg::OrderEvent(ev),
            format!("o-{}-{}", o.slot.0, o.r#gen),
        );
    }
    fn try_cross(&mut self, ix: usize, now_ns: u64) {
        let o = &self.orders[ix];
        let Some(&(bid, bq, ask, aq)) = self.bbo.get(&o.inst.0) else {
            return;
        };
        let (touch, depth, crosses) = match o.side {
            Side::Buy => (ask, aq, o.px >= ask),
            Side::Sell => (bid, bq, o.px <= bid),
        };
        if !crosses || depth <= 0 {
            return;
        }
        let mut q = (o.qty - o.filled).min(depth);
        if self.partial && o.ioc {
            q = (q / 2).max(1);
        }
        if q > 0 {
            self.fill(ix, touch, q, now_ns);
        }
    }
}

impl Matcher for ToyMatcher {
    fn new(world: &WorldSpec, venue: VenueId, _seed: u64) -> Self {
        let vs = world.venue(venue);
        let insts = world
            .insts
            .iter()
            .filter(|i| i.venue == venue)
            .map(|i| (i.id, i.base, i.quote, i.perp))
            .collect();
        let mut bal = BTreeMap::new();
        let mut accounts = Vec::new();
        for a in &world.accounts {
            if a.venues.contains(&venue) {
                accounts.push(a.id);
            }
            for (v, asset, amt) in &a.balances {
                if *v == venue {
                    bal.insert((a.id.0, asset.0), *amt);
                }
            }
        }
        let mut bbo = BTreeMap::new();
        for i in world.insts.iter().filter(|i| i.venue == venue) {
            bbo.insert(i.id.0, (i.bid, i.depth, i.ask, i.depth));
        }
        ToyMatcher {
            venue,
            fee_ppm: vs.map_or(0, |v| v.fee_ppm),
            fee_in_base: vs.is_some_and(|v| v.fee_in_base),
            insts,
            accounts,
            bbo,
            orders: Vec::new(),
            pending: VecDeque::new(),
            pos: BTreeMap::new(),
            inv: BTreeMap::new(),
            bal,
            next_exch: 1,
            next_trade: 1,
            external_history: Vec::new(),
            latency_ns: 1_000_000,
            base_fee: false,
            partial: false,
        }
    }
    fn on_bbo(&mut self, inst: InstId, bid: (i64, i64), ask: (i64, i64), now_ns: u64) {
        self.bbo.insert(inst.0, (bid.0, bid.1, ask.0, ask.1));
        for ix in 0..self.orders.len() {
            if self.orders[ix].inst == inst && self.orders[ix].filled < self.orders[ix].qty {
                self.try_cross(ix, now_ns);
            }
        }
        self.orders.retain(|o| o.filled < o.qty);
    }
    fn on_command(&mut self, cmd: &GwCmd, now_ns: u64) {
        let due = now_ns + self.latency_ns;
        match cmd.op {
            GwOp::Place => {
                let o = TOrder {
                    account: cmd.account,
                    slot: cmd.slot,
                    r#gen: cmd.r#gen,
                    epoch: cmd.arg as u16,
                    inst: cmd.inst,
                    side: cmd.side,
                    px: cmd.px.0,
                    qty: cmd.qty.0,
                    filled: 0,
                    ioc: matches!(cmd.tif, Tif::Ioc | Tif::Fok),
                    exch_id: self.next_exch,
                };
                self.next_exch += 1;
                let ack = order_event(
                    self.venue,
                    &o,
                    OrderEventKind::Ack,
                    o.px,
                    0,
                    (0, AssetId(0)),
                    now_ns,
                );
                self.push(
                    due,
                    RepMsg::OrderEvent(ack),
                    format!("o-{}-{}", o.slot.0, o.r#gen),
                );
                self.orders.push(o);
                let ix = self.orders.len() - 1;
                self.try_cross(ix, now_ns);
                let o = self.orders[ix].clone();
                if o.ioc && o.filled < o.qty {
                    let ev = order_event(
                        self.venue,
                        &o,
                        OrderEventKind::Expired,
                        o.px,
                        0,
                        (0, AssetId(0)),
                        now_ns,
                    );
                    self.push(due, RepMsg::OrderEvent(ev), format!("o-{}-{}", o.slot.0, o.r#gen));
                    self.orders.remove(ix);
                } else if o.filled >= o.qty {
                    self.orders.remove(ix);
                }
            }
            GwOp::Cancel => {
                let pos = self
                    .orders
                    .iter()
                    .position(|o| o.account == cmd.account && o.slot == cmd.target_slot);
                match pos {
                    Some(ix) => {
                        let o = self.orders.remove(ix);
                        let ev = order_event(
                            self.venue,
                            &o,
                            OrderEventKind::CancelAck,
                            o.px,
                            0,
                            (0, AssetId(0)),
                            now_ns,
                        );
                        self.push(due, RepMsg::OrderEvent(ev), format!("o-{}-{}", o.slot.0, o.r#gen));
                    }
                    None => {
                        let o = TOrder {
                            account: cmd.account,
                            slot: cmd.target_slot,
                            r#gen: cmd.r#gen,
                            epoch: 0,
                            inst: cmd.inst,
                            side: cmd.side,
                            px: 0,
                            qty: 0,
                            filled: 0,
                            ioc: false,
                            exch_id: 0,
                        };
                        let ev = order_event(
                            self.venue,
                            &o,
                            OrderEventKind::CancelReject,
                            0,
                            0,
                            (0, AssetId(0)),
                            now_ns,
                        );
                        self.push(due, RepMsg::OrderEvent(ev), String::new());
                    }
                }
            }
            GwOp::CancelAll => self.cancel_all(cmd.account, now_ns),
            _ => {}
        }
    }
    fn drain_due(&mut self, now_ns: u64, out: &mut Vec<VenueOut>) {
        while self.pending.front().is_some_and(|r| r.due_ns <= now_ns) {
            if let Some(r) = self.pending.pop_front() {
                out.push(r);
            }
        }
    }
    fn next_due(&self) -> Option<u64> {
        self.pending.front().map(|r| r.due_ns)
    }
    fn truth(&self, a: AccountId) -> TruthRow {
        let mut t = TruthRow {
            account: a,
            venue: self.venue,
            positions: Vec::new(),
            inventory: Vec::new(),
            balances: Vec::new(),
            undelivered: self.pending.iter().any(|r| rep_account(&r.rep) == Some(a)),
            excluded: Vec::new(),
        };
        for ((acc, i), q) in &self.pos {
            if *acc == a.0 {
                t.positions.push((InstId(*i), *q));
            }
        }
        for ((acc, x), q) in &self.inv {
            if *acc == a.0 {
                t.inventory.push((AssetId(*x), *q));
            }
        }
        for ((acc, x), q) in &self.bal {
            if *acc == a.0 {
                t.balances.push((AssetId(*x), *q));
            }
        }
        t
    }
    fn accounts(&self) -> Vec<AccountId> {
        self.accounts.clone()
    }
    fn open_orders(&self, a: AccountId) -> Vec<(SlotId, u32)> {
        self.orders
            .iter()
            .filter(|o| o.account == a)
            .map(|o| (o.slot, o.r#gen))
            .collect()
    }
    fn cancel_all(&mut self, a: AccountId, now_ns: u64) {
        let due = now_ns + self.latency_ns;
        let (mine, rest): (Vec<TOrder>, Vec<TOrder>) = self.orders.drain(..).partition(|o| o.account == a);
        self.orders = rest;
        for o in mine {
            let ev = order_event(
                self.venue,
                &o,
                OrderEventKind::CancelAck,
                o.px,
                0,
                (0, AssetId(0)),
                now_ns,
            );
            self.push(due, RepMsg::OrderEvent(ev), format!("o-{}-{}", o.slot.0, o.r#gen));
        }
    }
    fn free_quote(&self, a: AccountId) -> i128 {
        self.insts
            .first()
            .map_or(0, |(_, _, q, _)| self.bal.get(&(a.0, q.0)).copied().unwrap_or(0))
    }
    fn external_history(&self) -> &[ExternalReceipt] {
        &self.external_history
    }
    fn external(
        &mut self,
        a: AccountId,
        inst: InstId,
        side: Side,
        qty: i64,
        client_id: &str,
        push: bool,
        now_ns: u64,
    ) -> ExternalFill {
        let (_, base, _, perp) = self.inst(inst).expect("invalid external instrument");
        assert!(self.accounts.contains(&a) && qty > 0, "invalid external fill");
        assert!(
            self.external_history.len() < EXTERNAL_HISTORY_CAP,
            "external history full"
        );
        let next_trade = self
            .next_trade
            .checked_add(1)
            .expect("external trade id exhausted");
        let s: i128 = if side == Side::Buy { 1 } else { -1 };
        if perp {
            *self.pos.entry((a.0, inst.0)).or_default() += s * i128::from(qty);
        } else {
            *self.inv.entry((a.0, base.0)).or_default() += s * i128::from(qty);
        }
        let px = self.bbo.get(&inst.0).map_or(0, |b| b.0);
        let trade_id = self.next_trade;
        self.next_trade = next_trade;
        let x = ExternalFill {
            venue: self.venue,
            account: a,
            side,
            cause: ExternalCause::Unknown,
            inst,
            qty: Qty(qty),
            px: Px(px),
            fee: Qty(0),
            fee_asset: AssetId(0),
            _pad: [0; 6],
            exch_order_id: 0,
            trade_id,
            ex_ts_ns: now_ns as i64,
            rx_tsc: Tsc(0),
            fill_seq: 0,
            cash_qty: hft_types::Qty(0),
            cash_amount: hft_types::Qty(0),
        };
        self.external_history.push(ExternalReceipt {
            fill: x,
            client_id: client_id.to_string(),
        });
        if push {
            self.push(
                now_ns + self.latency_ns,
                RepMsg::ExternalFill(x),
                client_id.to_string(),
            );
        }
        x
    }
    fn external_order(
        &mut self,
        a: AccountId,
        inst: InstId,
        side: Side,
        qty: i64,
        positive_trade_id: bool,
        now_ns: u64,
    ) -> Result<ExternalFill, String> {
        if !self.accounts.contains(&a)
            || self.inst(inst).is_none()
            || qty <= 0
            || self.bbo.get(&inst.0).is_none_or(|b| b.0 <= 0)
            || self.external_history.len() == EXTERNAL_HISTORY_CAP
        {
            return Err("invalid or full external order history".into());
        }
        let mut fill = self.external(a, inst, side, qty, "foreign-order", false, now_ns);
        fill.exch_order_id = u64::MAX - fill.trade_id;
        if !positive_trade_id {
            fill.trade_id = 0;
            fill.fill_seq = qty as u64;
        }
        self.external_history.last_mut().expect("new receipt").fill = fill;
        Ok(fill)
    }
    fn state_hash(&self) -> u64 {
        let s = format!(
            "{:?}{:?}{:?}{:?}",
            self.pos,
            self.inv,
            self.bal,
            self.orders.len()
        );
        xxhash_rust::xxh3::xxh3_64(s.as_bytes())
    }
}

/// Account of a report.
pub fn rep_account(r: &RepMsg) -> Option<AccountId> {
    match r {
        RepMsg::OrderEvent(e) => Some(e.account),
        RepMsg::SendReport(e) => Some(e.account),
        RepMsg::PositionReport(e) => Some(e.account),
        RepMsg::BalanceReport(e) => Some(e.account),
        RepMsg::FundingSettle(e) => Some(e.account),
        RepMsg::ConnState(e) => Some(e.account),
        RepMsg::ExternalFill(e) => Some(e.account),
        RepMsg::AccountConfigReport(e) => Some(e.account),
        RepMsg::MarginReport(e) => Some(e.account),
    }
}

/// A report as the engine Input it becomes.
pub fn rep_input(r: &RepMsg, src: Source) -> InputRec {
    match r {
        RepMsg::OrderEvent(e) => InputRec::of(e, src),
        RepMsg::SendReport(e) => InputRec::of(e, src),
        RepMsg::PositionReport(e) => InputRec::of(e, src),
        RepMsg::BalanceReport(e) => InputRec::of(e, src),
        RepMsg::FundingSettle(e) => InputRec::of(e, src),
        RepMsg::ConnState(e) => InputRec::of(e, src),
        RepMsg::ExternalFill(e) => InputRec::of(e, src),
        RepMsg::AccountConfigReport(e) => InputRec::of(e, src),
        RepMsg::MarginReport(e) => InputRec::of(e, src),
    }
}

// --------------------------------------------------------------------------------- MockVenue

/// One-shot faults applied to the next order of the venue.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum NextOrder {
    Timeout,
    ServerError,
    UnknownCode,
    Reject,
    FillBeforeAck,
    /// The next fill also arrives without a client id; a previously delivered Ack may resolve it.
    NoClientId,
}

/// Latest advertised book, kept separately from the faulted matching liquidity.
#[derive(Clone, Copy, Debug)]
struct MatchBook {
    inst: InstId,
    lot: i64,
    bid: (i64, i64),
    ask: (i64, i64),
}

/// A venue in the DST: matcher + behaviour layer + fault perturbation.
#[derive(Debug)]
pub struct MockVenue<M: Matcher> {
    pub cfg: MockVenueCfg,
    pub m: M,
    rng: ChaCha8Rng,
    /// Unfaulted (SimActor use): no behaviour layer, no perturbation.
    pub clean: bool,
    rate: BTreeMap<u8, VecDeque<u64>>,
    dead_man: BTreeMap<u8, u64>,
    next_ping: u64,
    windows: Vec<(u64, u64, Fault)>,
    next_order: Vec<NextOrder>,
    late_fees: VecDeque<u32>,
    books: Vec<MatchBook>,
    partial_until: Option<u64>,
    /// Scheduled Ack arrival and identity, scoped to this venue and account. An Ack scheduled
    /// after a fill cannot resolve that fill early. This models the gw's exchange-id lookup;
    /// it must not treat the matcher-supplied slot as evidence for a no-client-id report.
    ack_ids: BTreeMap<(u8, u64), (u64, OrderEvent)>,
    md_down_until: u64,
    order_down_until: u64,
    userstream_down_until: u64,
    /// Original receipts waiting for the user stream. The optional gap remains in flight
    /// until the replay is scheduled and its last_due boundary has passed.
    adl_replays: Vec<(u64, ExternalFill)>,
    /// (original receipt, replay after booking). These gaps have no numeric truth exclusions.
    reconcile_gaps: Vec<(ExternalFill, bool)>,
    fault_error: Option<String>,
    quiet: BTreeMap<u32, u64>,
    last_due: BTreeMap<u8, u64>,
    /// (account, inst) whose truth changed without a push (ADL in a user-stream gap): INV-16
    /// does not compare them until the engine synthesises the fill.
    excluded: Vec<(u8, u32)>,
    /// Inputs produced, kept for tests (`(due, kind)`).
    pub produced: u64,
}

impl<M: Matcher> MockVenue<M> {
    pub fn new(cfg: MockVenueCfg, world: &WorldSpec, seed: u64) -> MockVenue<M> {
        let v = cfg.venue;
        MockVenue {
            m: M::new(world, v, seed),
            rng: actor_rng(seed, ActorId::Venue(v.0)),
            next_ping: u64::from(cfg.bbo_ping_ms) * 1_000_000,
            cfg,
            clean: false,
            rate: BTreeMap::new(),
            dead_man: BTreeMap::new(),
            windows: Vec::new(),
            next_order: Vec::new(),
            late_fees: VecDeque::new(),
            books: world
                .insts
                .iter()
                .filter(|i| i.venue == v)
                .map(|i| MatchBook {
                    inst: i.id,
                    lot: i.lot,
                    bid: (i.bid, i.depth),
                    ask: (i.ask, i.depth),
                })
                .collect(),
            partial_until: None,
            ack_ids: BTreeMap::new(),
            md_down_until: 0,
            order_down_until: 0,
            userstream_down_until: 0,
            adl_replays: Vec::new(),
            reconcile_gaps: Vec::new(),
            fault_error: None,
            quiet: BTreeMap::new(),
            last_due: BTreeMap::new(),
            excluded: Vec::new(),
            produced: 0,
        }
    }
    fn src(&self) -> Source {
        if self.clean {
            Source::Sim
        } else {
            Source::Venue(self.cfg.venue)
        }
    }
    fn active(&self, now: u64, f: impl Fn(&Fault) -> bool) -> Option<Fault> {
        self.windows
            .iter()
            .find(|(s, e, x)| *s <= now && now < *e && f(x))
            .map(|(_, _, x)| *x)
    }

    /// Restore the latest book at the window boundary, even when no market update arrives.
    /// Collection is capped at this boundary, so it cannot match with stale thin liquidity
    /// beyond the window. SimExchange processes commands strictly before a market record.
    fn finish_partial(&mut self, now: u64) {
        if let Some(end) = self.partial_until
            && now >= end
        {
            for b in &self.books {
                self.m.on_bbo(b.inst, b.bid, b.ask, end);
            }
            self.partial_until = None;
        }
    }

    fn matching_book(&self, b: MatchBook, now: u64) -> ((i64, i64), (i64, i64)) {
        if self.partial_until.is_some_and(|end| now < end) {
            ((b.bid.0, b.bid.1.min(b.lot)), (b.ask.0, b.ask.1.min(b.lot)))
        } else {
            (b.bid, b.ask)
        }
    }
    fn conn_state(&self, role: ConnRole, state: ConnStateKind, account: AccountId) -> InputRec {
        InputRec::of(
            &ConnState {
                venue: self.cfg.venue,
                conn: 0,
                state,
                role,
                inst: InstId(0),
                reason: 0,
                account,
                _pad2: [0; 3],
                rx_tsc: 0,
            },
            self.src(),
        )
    }

    /// A market update: the matcher always sees it; the engine gets a `Bbo` unless the md
    /// connection is down or silent or the inst is quiet.
    pub fn on_market(
        &mut self,
        inst: InstId,
        bid: (i64, i64),
        ask: (i64, i64),
        now: u64,
    ) -> Option<(u64, InputRec)> {
        self.finish_partial(now);
        let book = self.books.iter_mut().find(|b| b.inst == inst).map(|b| {
            b.bid = bid;
            b.ask = ask;
            *b
        });
        let (match_bid, match_ask) = book.map_or((bid, ask), |b| self.matching_book(b, now));
        self.m.on_bbo(inst, match_bid, match_ask, now);
        if now < self.md_down_until || self.quiet.get(&inst.0).is_some_and(|t| now < *t) {
            return None;
        }
        let lat = self.rng.range(self.cfg.md_latency_ns.0, self.cfg.md_latency_ns.1);
        let mut h: hft_event::MarketHdr = zerocopy::FromZeros::new_zeroed();
        h.venue = self.cfg.venue;
        h.conn = ConnId(0);
        h.inst = inst;
        h.ex_ts_ns = now as i64;
        let b = hft_event::Bbo {
            h,
            bid_px: Px(bid.0),
            bid_qty: Qty(bid.1),
            ask_px: Px(ask.0),
            ask_qty: Qty(ask.1),
            update_id: 0,
        };
        self.produced += 1;
        Some((now + lat, InputRec::of(&b, Source::Market)))
    }

    /// A frame from the engine: gw `SendReport`, behaviour layer, then the matcher.
    pub fn on_frame(&mut self, cmd: &GwCmd, now: u64) -> Vec<(u64, InputRec)> {
        self.finish_partial(now);
        let mut out = Vec::new();
        let a = cmd.account;
        if !self.clean {
            self.dead_man.entry(a.0).or_insert(now);
        }
        if cmd.op == GwOp::DeadManRefresh {
            self.dead_man.insert(a.0, now);
            return out;
        }
        let order_class = matches!(
            cmd.op,
            GwOp::Place | GwOp::Cancel | GwOp::Amend | GwOp::CancelAll | GwOp::Query
        );
        if !order_class {
            return out;
        }
        let down = !self.clean && now < self.order_down_until;
        let status = if down {
            SendStatus::NoConn
        } else {
            SendStatus::Written
        };
        let sr = SendReport {
            decision_seq: cmd.decision_seq,
            sent_tsc: Tsc(0),
            slot: cmd.slot,
            r#gen: cmd.r#gen,
            target_slot: cmd.target_slot,
            target_gen: 0,
            venue: self.cfg.venue,
            account: a,
            conn: ConnId(0),
            status,
            op: cmd.op,
            _pad: [0; 3],
            os_errno: 0,
            _pad2: 0,
        };
        out.push((now + 10_000, InputRec::of(&sr, self.src())));
        if down {
            return out;
        }
        if cmd.op == GwOp::Place && !self.clean {
            // Rate limit (429 shape).
            if self.cfg.rate_per_s > 0 {
                let h = self.rate.entry(a.0).or_default();
                while h.front().is_some_and(|t| now - *t >= 1_000_000_000) {
                    h.pop_front();
                }
                let forced = self
                    .windows
                    .iter()
                    .any(|(s, e, f)| *s <= now && now < *e && matches!(f, Fault::RateLimit { .. }));
                if forced || h.len() >= self.cfg.rate_per_s as usize {
                    out.push(self.reject(cmd, RejectRule::RateLimited, now));
                    return out;
                }
                h.push_back(now);
            }
            // Margin check (code table).
            let spot_buy_notional = i128::from(cmd.qty.0) * i128::from(cmd.px.0) / 1_000_000;
            if cmd.side == Side::Buy && spot_buy_notional > self.m.free_quote(a) {
                out.push(self.reject(cmd, RejectRule::InsufficientMargin, now));
                return out;
            }
            if self
                .active(now, |f| matches!(f, Fault::PersistentSecondReject { .. }))
                .is_some()
            {
                out.push(self.reject(cmd, RejectRule::Generic, now));
                return out;
            }
            match self.next_order.pop() {
                Some(NextOrder::Reject) => {
                    out.push(self.reject(cmd, RejectRule::Generic, now));
                    return out;
                }
                Some(NextOrder::UnknownCode) => {
                    out.push(self.reject(cmd, RejectRule::Unregistered, now));
                    return out;
                }
                Some(NextOrder::Timeout) => {
                    self.m.on_command(cmd, now);
                    let mut tmp = Vec::new();
                    self.m.drain_due(u64::MAX, &mut tmp);
                    return out;
                }
                Some(NextOrder::ServerError) => {
                    let mut e = self.blank_event(cmd, now);
                    e.what = OrderEventKind::OutcomeUnknown;
                    out.push((now + 1_000_000, InputRec::of(&e, self.src())));
                    self.m.on_command(cmd, now);
                    return out;
                }
                Some(other) => self.next_order.push(other),
                None => {}
            }
        }
        self.m.on_command(cmd, now);
        out.extend(self.collect(now));
        out
    }

    fn blank_event(&self, cmd: &GwCmd, now: u64) -> OrderEvent {
        OrderEvent {
            venue: self.cfg.venue,
            account: cmd.account,
            what: OrderEventKind::Reject,
            detail: 0,
            slot: cmd.slot,
            r#gen: cmd.r#gen,
            inst: cmd.inst,
            px: cmd.px,
            qty: Qty(0),
            cum_filled: Qty(0),
            fee: Qty(0),
            fee_asset: AssetId(0),
            epoch: 0,
            reject_code: 0,
            exch_order_id: 0,
            ex_ts_ns: now as i64,
            hw_rx_ns: 0,
            rx_tsc: 0,
            avg_px: Px(0),
            cash_qty: hft_types::Qty(0),
            cash_sum: hft_types::Qty(0),
            trade_id: 0,
        }
    }

    fn reject(&mut self, cmd: &GwCmd, r: RejectRule, now: u64) -> (u64, InputRec) {
        let mut e = self.blank_event(cmd, now);
        e.reject_code = self.cfg.code(r);
        let lat = self.rng.range(self.cfg.latency_ns.0, self.cfg.latency_ns.1);
        self.last_due.insert(cmd.account.0, now + lat);
        (now + lat, InputRec::of(&e, self.src()))
    }

    /// Matcher reports that are due, perturbed by the active report faults.
    fn collect(&mut self, now: u64) -> Vec<(u64, InputRec)> {
        self.finish_partial(now);
        let mut reps = Vec::new();
        let horizon = now.saturating_add(10_000_000);
        self.m.drain_due(
            self.partial_until.map_or(horizon, |end| horizon.min(end)),
            &mut reps,
        );
        if now >= self.userstream_down_until {
            let mut pending = Vec::new();
            for (at, fill) in self.adl_replays.drain(..) {
                if at <= now {
                    reps.push(VenueOut {
                        due_ns: now,
                        rep: RepMsg::ExternalFill(fill),
                        client_id: "adl-replay".to_string(),
                    });
                } else {
                    pending.push((at, fill));
                }
            }
            self.adl_replays = pending;
        }
        let mut out = Vec::new();
        let mut fba = self.next_order.contains(&NextOrder::FillBeforeAck);
        for r in reps {
            let mut rep = r.rep;
            if let RepMsg::ExternalFill(x) = &mut rep {
                x.cause = if r.client_id.starts_with(self.cfg.liquidation_prefix) {
                    ExternalCause::Liquidation
                } else if r.client_id.starts_with("adl-") {
                    ExternalCause::Adl
                } else {
                    ExternalCause::Manual
                };
            }
            let mut due =
                r.due_ns.max(now) + self.rng.range(0, self.cfg.latency_ns.1 - self.cfg.latency_ns.0);
            if fba && matches!(&rep, RepMsg::OrderEvent(e) if e.what == OrderEventKind::Ack) {
                due += 2_000_000;
                self.next_order.retain(|x| *x != NextOrder::FillBeforeAck);
                fba = false;
            }
            if !self.clean {
                if let Some(Fault::ReportReorder { window, .. }) =
                    self.active(now, |f| matches!(f, Fault::ReportReorder { .. }))
                {
                    due += self.rng.below(u64::from(window) * 1_000_000);
                }
                if let Some(Fault::ReportDelay { max_ms, .. }) =
                    self.active(now, |f| matches!(f, Fault::ReportDelay { .. }))
                {
                    let u = self.rng.below(1_000);
                    due += u * u * u * u64::from(max_ms) / 1_000_000;
                }
                if let Some(Fault::GrayConn { .. }) =
                    self.active(now, |f| matches!(f, Fault::GrayConn { .. }))
                {
                    due += (due - now) * 99;
                }
            }
            if let RepMsg::OrderEvent(e) = &rep
                && e.what == OrderEventKind::Ack
                && e.exch_order_id != 0
                && e.slot != SlotId::UNRESOLVED
            {
                let key = (e.account.0, e.exch_order_id);
                match self.ack_ids.get(&key) {
                    Some((at, old))
                        if (old.epoch, old.slot, old.r#gen, old.inst)
                            == (e.epoch, e.slot, e.r#gen, e.inst)
                            && *at <= due => {}
                    _ => {
                        self.ack_ids.insert(key, (due, *e));
                    }
                }
            }
            // The matcher has already charged the fee once. Split that same fee out of the
            // fill without changing its truth. Ack/reject/zero-fee reports do not consume it.
            // ReportDup duplicates the fee-free fill; the separately delayed fee is emitted
            // once (FeeOnly is a delta, without a duplicate-report idempotency key).
            let mut fee_due = due;
            if let RepMsg::OrderEvent(e) = &mut rep
                && matches!(e.what, OrderEventKind::Fill | OrderEventKind::PartialFill)
                && e.fee.0 != 0
                && let Some(delay_ms) = self.late_fees.pop_front()
            {
                let mut fee = *e;
                fee.what = OrderEventKind::FeeOnly;
                fee.qty = Qty(0);
                fee_due = due.saturating_add(u64::from(delay_ms) * 1_000_000);
                out.push((fee_due, InputRec::of(&fee, self.src())));
                e.fee.0 = 0;
                e.fee_asset = AssetId(0);
            }
            let input = rep_input(&rep, self.src());
            if let Some(a) = rep_account(&rep) {
                let e = self.last_due.entry(a.0).or_insert(0);
                *e = (*e).max(fee_due);
            }
            if !self.clean
                && let RepMsg::OrderEvent(e) = &rep
                && matches!(e.what, OrderEventKind::PartialFill | OrderEventKind::Fill)
                && let Some(p) = self.next_order.iter().position(|x| *x == NextOrder::NoClientId)
            {
                self.next_order.remove(p);
                let mut copy = *e;
                copy.slot = SlotId::UNRESOLVED;
                copy.r#gen = 0;
                copy.epoch = 0;
                let mut copy_due = due;
                if let Some((ack_due, ack)) = self.ack_ids.get(&(e.account.0, e.exch_order_id))
                    && *ack_due <= due.saturating_add(u64::from(self.cfg.unmatched_hold_ms) * 1_000_000)
                    && ack.inst == e.inst
                {
                    // The world knows the Ack schedule. Delay the held copy so the engine
                    // sees no resolved identity before that Ack actually arrives.
                    copy_due = copy_due.max(*ack_due);
                    copy.slot = ack.slot;
                    copy.r#gen = ack.r#gen;
                    copy.epoch = ack.epoch;
                }
                self.last_due
                    .entry(e.account.0)
                    .and_modify(|last| *last = (*last).max(copy_due))
                    .or_insert(copy_due);
                out.push((copy_due, InputRec::of(&copy, self.src())));
            }
            if !self.clean
                && self
                    .active(now, |f| matches!(f, Fault::ReportDup { .. }))
                    .is_some()
            {
                out.push((due + 1, input.clone()));
            }
            out.push((due, input));
        }
        self.produced += out.len() as u64;
        out
    }

    /// Periodic work at `now`: pongs, dead-man, due reports, recoveries.
    pub fn tick(&mut self, now: u64) -> Vec<(u64, InputRec)> {
        self.finish_partial(now);
        let mut out = Vec::new();
        if !self.clean && self.cfg.bbo_ping_ms > 0 {
            while self.next_ping <= now {
                let silent = self.next_ping < self.md_down_until
                    || self
                        .active(self.next_ping, |f| matches!(f, Fault::SecondLegSilent { .. }))
                        .is_some();
                if !silent {
                    out.push((
                        self.next_ping,
                        self.conn_state(ConnRole::Md, ConnStateKind::Alive, AccountId(0)),
                    ));
                }
                self.next_ping += u64::from(self.cfg.bbo_ping_ms) * 1_000_000;
            }
        }
        if !self.clean && self.cfg.dead_man_ms > 0 {
            let lim = u64::from(self.cfg.dead_man_ms) * 1_000_000;
            let due: Vec<u8> = self
                .dead_man
                .iter()
                .filter(|(_, t)| now >= **t + lim)
                .map(|(a, _)| *a)
                .collect();
            for a in due {
                self.m.cancel_all(AccountId(a), now);
                self.dead_man.insert(a, now);
            }
        }
        out.extend(self.collect(now));
        out
    }

    /// Next time `tick` has work.
    pub fn next_wake(&self, now: u64) -> u64 {
        let mut t = self.m.next_due().unwrap_or(u64::MAX);
        if let Some(end) = self.partial_until {
            t = t.min(end);
        }
        if let Some(at) = self.adl_replays.iter().map(|(at, _)| *at).min() {
            t = t.min(at.max(self.userstream_down_until));
        }
        if !self.clean && self.cfg.bbo_ping_ms > 0 {
            t = t.min(self.next_ping);
        }
        if !self.clean && self.cfg.dead_man_ms > 0 {
            let lim = u64::from(self.cfg.dead_man_ms) * 1_000_000;
            if let Some(d) = self.dead_man.values().map(|t| t + lim).min() {
                t = t.min(d);
            }
        }
        t.max(now)
    }

    /// Starts fault `f` at `now`; returns Inputs it produces immediately.
    pub fn apply_fault(&mut self, f: &Fault, now: u64) -> Vec<(u64, InputRec)> {
        let mut out = Vec::new();
        if self.clean || self.fault_error.is_some() {
            return out;
        }
        let ms = |x: u32| u64::from(x) * 1_000_000;
        match *f {
            Fault::ConnDown { conn, down_ms, .. } => {
                let until = now + ms(down_ms);
                let role = match conn {
                    ConnKind::Md => {
                        self.md_down_until = until;
                        ConnRole::Md
                    }
                    ConnKind::Order => {
                        self.order_down_until = until;
                        ConnRole::Order
                    }
                    ConnKind::UserStream => {
                        self.userstream_down_until = self.userstream_down_until.max(until);
                        ConnRole::UserStream
                    }
                };
                for a in self.m.accounts() {
                    let acct = if role == ConnRole::Md { AccountId(0) } else { a };
                    out.push((now, self.conn_state(role, ConnStateKind::Down, acct)));
                    out.push((until, self.conn_state(role, ConnStateKind::Up, acct)));
                    if role == ConnRole::Md {
                        break;
                    }
                }
            }
            Fault::SecondLegSilent { dur_ms, .. } | Fault::L3OtherLegSilent { dur_ms, .. } => {
                self.windows.push((now, now + ms(dur_ms), *f));
                self.md_down_until = now + ms(dur_ms);
            }
            Fault::QuietBook { inst, dur_ms } => {
                self.quiet.insert(inst.0, now + ms(dur_ms));
            }
            Fault::ReportReorder { dur_ms, .. }
            | Fault::ReportDup { dur_ms, .. }
            | Fault::ReportDelay { dur_ms, .. }
            | Fault::GrayConn { dur_ms, .. }
            | Fault::RateLimit { dur_ms, .. }
            | Fault::PersistentSecondReject { dur_ms, .. } => self.windows.push((now, now + ms(dur_ms), *f)),
            Fault::PartialFills { dur_ms, .. } => {
                self.finish_partial(now);
                if dur_ms == 0 {
                    return out;
                }
                let Some(end) = now.checked_add(ms(dur_ms)) else {
                    self.fault_error = Some("partial fill window exceeds virtual time".into());
                    return out;
                };
                if self.books.iter().any(|b| b.lot <= 0) {
                    self.fault_error = Some("partial fills require positive instrument lots".into());
                    return out;
                }
                let was_active = self.partial_until.is_some();
                self.partial_until = Some(self.partial_until.map_or(end, |old| old.max(end)));
                if !was_active {
                    for b in &self.books {
                        let (bid, ask) = self.matching_book(*b, now);
                        self.m.on_bbo(b.inst, bid, ask, now);
                    }
                }
            }
            Fault::Timeout { .. } => self.next_order.push(NextOrder::Timeout),
            Fault::ServerError { .. } => self.next_order.push(NextOrder::ServerError),
            Fault::UnknownCode { .. } => self.next_order.push(NextOrder::UnknownCode),
            Fault::SecondLegReject { .. } => self.next_order.push(NextOrder::Reject),
            Fault::FillBeforeAck { .. } => self.next_order.push(NextOrder::FillBeforeAck),
            Fault::LateFeeOnly { delay_ms, .. } => self.late_fees.push_back(delay_ms),
            Fault::NoClientIdFill { .. } => self.next_order.push(NextOrder::NoClientId),
            Fault::ForeignOrderFill {
                account,
                inst,
                qty,
                buy,
                positive_trade_id,
                unresolved,
                raw_first,
            } => {
                if now < self.userstream_down_until {
                    self.fault_error = Some("foreign order fill requires a connected user stream".into());
                    return out;
                }
                // A stale client id must name a real previously acknowledged slot, with a
                // different generation. Never turn a missing prerequisite into a no-op.
                let identity = if unresolved {
                    (SlotId::UNRESOLVED, 0, 0)
                } else {
                    let Some((_, ack)) = self
                        .ack_ids
                        .values()
                        .find(|(at, e)| *at <= now && e.account == account && e.inst == inst)
                    else {
                        self.fault_error = Some("stale fill requires an acknowledged order identity".into());
                        return out;
                    };
                    (ack.slot, ack.r#gen ^ (1 << 31), ack.epoch)
                };
                let side = if buy { Side::Buy } else { Side::Sell };
                let fill = match self
                    .m
                    .external_order(account, inst, side, qty, positive_trade_id, now)
                {
                    Ok(fill) => fill,
                    Err(e) => {
                        self.fault_error = Some(e);
                        return out;
                    }
                };
                if raw_first {
                    out.push((now + 1_000_000, InputRec::of(&fill, self.src())));
                }
                let order = OrderEvent {
                    venue: fill.venue,
                    account,
                    inst,
                    what: OrderEventKind::Fill,
                    detail: side as u8,
                    slot: identity.0,
                    r#gen: identity.1,
                    epoch: identity.2,
                    px: fill.px,
                    qty: fill.qty,
                    cum_filled: fill.qty,
                    fee: fill.fee,
                    fee_asset: fill.fee_asset,
                    reject_code: 0,
                    exch_order_id: fill.exch_order_id,
                    ex_ts_ns: fill.ex_ts_ns,
                    hw_rx_ns: 0,
                    rx_tsc: 0,
                    avg_px: fill.px,
                    cash_qty: fill.cash_qty,
                    cash_sum: fill.cash_amount,
                    trade_id: fill.trade_id,
                };
                let due = now + 2_000_000;
                out.push((due, InputRec::of(&order, self.src())));
                self.last_due
                    .entry(account.0)
                    .and_modify(|d| *d = (*d).max(due))
                    .or_insert(due);
                self.produced += out.len() as u64;
            }
            Fault::SelfLiquidation { account, inst } | Fault::ManualFill { account, inst } => {
                let prefix = if matches!(f, Fault::SelfLiquidation { .. }) {
                    self.cfg.liquidation_prefix
                } else {
                    "web-"
                };
                let t = self.m.truth(account);
                if let Some((_, q)) = t.positions.iter().find(|(i, _)| *i == inst)
                    && *q != 0
                {
                    let side = if *q > 0 { Side::Sell } else { Side::Buy };
                    self.m.external(
                        account,
                        inst,
                        side,
                        q.unsigned_abs() as i64,
                        &format!("{prefix}1"),
                        true,
                        now,
                    );
                }
                out.extend(self.collect(now));
            }
            Fault::Adl {
                account,
                inst,
                gap,
                replay,
            } => {
                let t = self.m.truth(account);
                if let Some((_, q)) = t.positions.iter().find(|(i, _)| *i == inst)
                    && *q != 0
                {
                    let side = if *q > 0 { Side::Sell } else { Side::Buy };
                    let fill = self.m.external(
                        account,
                        inst,
                        side,
                        (q.abs() / 2).max(1) as i64,
                        "adl-1",
                        !gap,
                        now,
                    );
                    if replay {
                        self.adl_replays.push((now.saturating_add(1_000_000), fill));
                    } else if gap {
                        self.excluded.push((account.0, inst.0));
                    }
                }
                out.extend(self.collect(now));
            }
            Fault::ReconciledAdl {
                account,
                inst,
                full,
                replay,
            } => {
                // Failing the prerequisite is a failed scenario, never an empty success.
                if now >= self.userstream_down_until {
                    self.fault_error = Some("reconciled ADL requires a stream gap".into());
                    return out;
                }
                if self.reconcile_gaps.len() == EXTERNAL_HISTORY_CAP {
                    self.fault_error = Some("ADL gap capacity exceeded".into());
                    return out;
                }
                let t = self.m.truth(account);
                let q = t
                    .positions
                    .iter()
                    .find(|(i, _)| *i == inst)
                    .map_or(0, |(_, q)| *q);
                if q == 0 {
                    self.fault_error = Some("reconciled ADL requires a nonzero exchange position".into());
                    return out;
                }
                let qty = if full {
                    q.unsigned_abs()
                } else {
                    (q.unsigned_abs() / 2).max(1)
                };
                let Ok(qty) = i64::try_from(qty) else {
                    self.fault_error = Some("ADL quantity exceeds receipt capacity".into());
                    return out;
                };
                let fill = self.m.external(
                    account,
                    inst,
                    if q > 0 { Side::Sell } else { Side::Buy },
                    qty,
                    "adl-reconciled",
                    false,
                    now,
                );
                self.reconcile_gaps.push((fill, replay));
                out.extend(self.collect(now));
            }
            Fault::MarginPath { account, .. } => {
                let mut h: hft_event::MarginReportHdr = zerocopy::FromZeros::new_zeroed();
                h.venue = self.cfg.venue;
                h.account = account;
                h.margin_ratio = hft_types::Rate(9_000_000);
                out.push((now + 1_000_000, InputRec::of(&h, self.src())));
            }
            _ => {}
        }
        out
    }

    /// Latched fault prerequisite/capacity failure; no execution is created after it.
    pub fn fault_error(&self) -> Option<&str> {
        self.fault_error.as_deref()
    }

    /// Clear only receipts independently proved booked at a committed engine boundary.
    /// Called after an actual ReconcileBatch event, regardless of enabled invariant checkers.
    pub fn acknowledge_reconciled(
        &mut self,
        sut: &impl crate::dst::sut::Sut,
        now: u64,
    ) -> Result<bool, String> {
        if self.reconcile_gaps.is_empty() {
            return Ok(false);
        }
        let fills: Vec<_> = self.reconcile_gaps.iter().map(|(fill, _)| *fill).collect();
        let booked = sut.committed_receipts(&fills)?;
        if booked.len() != fills.len() {
            return Err("reconciled ADL: missing committed receipt observations".into());
        }
        let mut pending = Vec::new();
        let mut changed = false;
        for ((fill, replay), booked) in self.reconcile_gaps.drain(..).zip(booked) {
            if booked {
                changed = true;
                if replay {
                    self.adl_replays.push((now.saturating_add(1_000_000), fill));
                }
            } else {
                pending.push((fill, replay));
            }
        }
        self.reconcile_gaps = pending;
        Ok(changed)
    }

    /// Exchange truth of every account of the venue (INV-16).
    pub fn truth(&self, now: u64) -> Vec<TruthRow> {
        self.m
            .accounts()
            .into_iter()
            .map(|a| {
                let mut t = self.m.truth(a);
                t.undelivered |= self.last_due.get(&a.0).is_some_and(|d| *d >= now);
                t.undelivered |= self.adl_replays.iter().any(|(_, f)| f.account == a);
                t.undelivered |= self.reconcile_gaps.iter().any(|(f, _)| f.account == a);
                t.excluded
                    .extend(self.excluded.iter().filter(|(x, _)| *x == a.0).map(|(_, i)| *i));
                t
            })
            .collect()
    }
}
