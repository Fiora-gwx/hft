//! `SimMatcher`: the [`Matcher`] of engine runs, an adapter over `hft_sim::SimExchange` (README
//! "DST 的世界模型" 2). One exchange per world venue, built from the world refdata with every account
//! as a Sim account (its private copy; the engine's refdata keeps Live accounts Live). Replies of
//! other venues of the same account are dropped. Fills not caused by the engine (liquidation, ADL,
//! manual; `Matcher::external`) are kept as an overlay on the exchange truth, because
//! `SimExchange` books only its own matches.

use std::collections::{BTreeMap, VecDeque};

use hft_event::{Bbo, ExternalCause, ExternalFill, GwCmd, GwOp, MarketHdr, RecordHeader, RepMsg};
use hft_refdata::{RefData, SimAccountCfg};
use hft_sim::{
    LatencyModel, MakerFill, MarginModel, MarketEventRef, MmrSource, SimBalance, SimExchange, SimInput,
    SimSetup, SimTuning,
};
use hft_types::{AccountId, AssetId, InstId, Px, Qty, Rate, Side, SlotId, Tif, Tsc, VenueId};
use zerocopy::FromZeros;

use crate::dst::obs::TruthRow;
use crate::dst::seq::build;
use crate::dst::venue_behavior::{Matcher, VenueOut};
use crate::dst::world::WorldSpec;

/// Slots probed by `open_orders` (the OMS table size of a DST run).
const SLOT_SCAN: u32 = 64;

/// Venue of a reply.
pub fn rep_venue(r: &RepMsg) -> VenueId {
    match r {
        RepMsg::OrderEvent(e) => e.venue,
        RepMsg::SendReport(e) => e.venue,
        RepMsg::PositionReport(e) => e.venue,
        RepMsg::BalanceReport(e) => e.venue,
        RepMsg::FundingSettle(e) => e.venue,
        RepMsg::ConnState(e) => e.venue,
        RepMsg::ExternalFill(e) => e.venue,
        RepMsg::AccountConfigReport(e) => e.venue,
        RepMsg::MarginReport(e) => e.venue,
    }
}

/// The matching core of a DST venue over `SimExchange`.
pub struct SimMatcher {
    venue: VenueId,
    ex: Option<SimExchange>,
    rd: Option<RefData>,
    accounts: Vec<AccountId>,
    /// (inst, base, quote, perp) of this venue.
    insts: Vec<(InstId, AssetId, AssetId, bool)>,
    out: VecDeque<VenueOut>,
    bbo: BTreeMap<u32, i64>,
    next_trade: u64,
    upd: u64,
    last_ns: u64,
    /// Epoch of each (account, slot) from its `Place` (`GwCmd.arg`): the gw stamps it on the
    /// slot's reports (`SimExchange` leaves the field to its gw).
    epochs: BTreeMap<(u8, u32), u16>,
    /// (gen, epoch) of the latest `Query` per (account, slot): the gw answers a query with the
    /// client id it asked about, also when the exchange does not know the order.
    queries: BTreeMap<(u8, u32), (u32, u16)>,
}

impl core::fmt::Debug for SimMatcher {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SimMatcher")
            .field("venue", &self.venue)
            .finish_non_exhaustive()
    }
}

fn setup(w: &WorldSpec, venue: VenueId, seed: u64) -> SimSetup {
    let accounts = w
        .accounts
        .iter()
        .filter(|a| a.venues.contains(&venue))
        .map(|a| {
            let bal: Vec<hft_refdata::SimBalance> = a
                .balances
                .iter()
                .map(|(v, asset, amt)| hft_refdata::SimBalance {
                    venue: *v,
                    asset: *asset,
                    qty: Qty(i64::try_from(*amt).unwrap_or(i64::MAX)),
                })
                .collect();
            (
                a.id,
                SimAccountCfg {
                    initial_balances: bal.into(),
                    latency_model: 0,
                    fee_tier: 0,
                    margin_model: 0,
                },
            )
        })
        .collect();
    SimSetup {
        tuning: SimTuning {
            seed,
            consume_decay_ms: 1_000,
            take_avail_bp: 10_000,
            quiet_ms: 50,
            report_interval_s: 60,
            transfer_delay_ms: 300_000,
            rate_limit: false,
            late_fee: false,
            maker_fill: MakerFill::TradeThrough,
        },
        tsc_hz_milli: 1_000_000_000_000,
        latency_models: vec![LatencyModel::Fixed {
            entry_us: 1_000,
            resp_us: 1_000,
        }],
        margin_models: vec![MarginModel {
            mmr: MmrSource::Flat(Rate(5_000_000)),
            leverage: build::LEVERAGE_X as u8,
            liq_fee: Rate(5_000_000),
            liq_slippage_bp: 50,
        }],
        accounts,
    }
}

impl SimMatcher {
    fn hdr(now_ns: u64) -> RecordHeader {
        let mut h = RecordHeader::new_zeroed();
        h.tsc = now_ns;
        h
    }
    fn pull(&mut self, now_ns: u64) {
        let Some(ex) = self.ex.as_mut() else { return };
        ex.advance(Tsc(now_ns));
        let venue = self.venue;
        let out = &mut self.out;
        let epochs = &self.epochs;
        let queries = &self.queries;
        ex.drain_due(Tsc(now_ns), |r| {
            // The venue configuration is static in a DST world (the engine gets it at startup);
            // margin reports travel without their per-inst items (the ring element only).
            let mut msg = r.msg;
            match &mut msg {
                RepMsg::AccountConfigReport(_) => return,
                RepMsg::MarginReport(h) => h.n = 0,
                RepMsg::OrderEvent(e) => {
                    if e.what == hft_event::OrderEventKind::QueryResult
                        && e.r#gen == 0
                        && let Some((g, ep)) = queries.get(&(e.account.0, e.slot.0))
                    {
                        e.r#gen = *g;
                        e.epoch = *ep;
                    }
                    if e.epoch == 0
                        && let Some(x) = epochs.get(&(e.account.0, e.slot.0))
                    {
                        e.epoch = *x;
                    }
                }
                _ => {}
            }
            if rep_venue(&msg) == venue {
                out.push_back(VenueOut {
                    due_ns: r.due_tsc.0,
                    rep: msg,
                    client_id: String::new(),
                });
            }
        });
    }
    fn inst(&self, i: InstId) -> Option<(InstId, AssetId, AssetId, bool)> {
        self.insts.iter().copied().find(|x| x.0 == i)
    }
}

impl Matcher for SimMatcher {
    fn new(world: &WorldSpec, venue: VenueId, seed: u64) -> Self {
        // SeqSut validates the same world before matchers are constructed. A direct caller
        // receives a clear configuration error instead of an inert matcher with no exchange.
        let blob = build::refdata_blob(world, true).expect("invalid SimMatcher world");
        let rd = Some(RefData::load(&blob, None).expect("invalid SimMatcher refdata"));
        let s = setup(world, venue, seed);
        let ex = rd.as_ref().and_then(|rd| SimExchange::new(&s, rd).ok());
        let mut m = SimMatcher {
            venue,
            ex,
            rd,
            accounts: s.accounts.iter().map(|(a, _)| *a).collect(),
            insts: world
                .insts
                .iter()
                .filter(|i| i.venue == venue)
                .map(|i| (i.id, i.base, i.quote, i.perp))
                .collect(),
            out: VecDeque::new(),
            bbo: BTreeMap::new(),
            next_trade: 1,
            upd: 0,
            last_ns: 0,
            epochs: BTreeMap::new(),
            queries: BTreeMap::new(),
        };
        for i in world.insts.iter().filter(|i| i.venue == venue) {
            m.on_bbo(i.id, (i.bid, i.depth), (i.ask, i.depth), 0);
        }
        m
    }
    fn on_bbo(&mut self, inst: InstId, bid: (i64, i64), ask: (i64, i64), now_ns: u64) {
        self.bbo.insert(inst.0, bid.0);
        self.upd += 1;
        let mut h = MarketHdr::new_zeroed();
        h.venue = self.venue;
        h.inst = inst;
        h.ex_ts_ns = now_ns as i64;
        h.rx_tsc = now_ns;
        let b = Bbo {
            h,
            bid_px: Px(bid.0),
            bid_qty: Qty(bid.1),
            ask_px: Px(ask.0),
            ask_qty: Qty(ask.1),
            update_id: self.upd,
        };
        let t = now_ns.max(self.last_ns);
        self.last_ns = t;
        if let Some(ex) = self.ex.as_mut() {
            ex.on_record(&Self::hdr(t), SimInput::Market(MarketEventRef::Bbo(&b)));
        }
    }
    fn on_command(&mut self, cmd: &GwCmd, now_ns: u64) {
        match cmd.op {
            GwOp::Place => {
                self.epochs.insert((cmd.account.0, cmd.slot.0), cmd.arg as u16);
            }
            GwOp::Query => {
                let ep = cmd.arg as u16;
                self.queries.insert((cmd.account.0, cmd.slot.0), (cmd.r#gen, ep));
            }
            _ => {}
        }
        let t = now_ns.max(self.last_ns);
        self.last_ns = t;
        if let Some(ex) = self.ex.as_mut() {
            ex.advance(Tsc(t));
            ex.on_command(cmd, Tsc(t));
        }
    }
    fn drain_due(&mut self, now_ns: u64, out: &mut Vec<VenueOut>) {
        let t = now_ns.max(self.last_ns);
        self.last_ns = t;
        self.pull(t);
        let mut keep = VecDeque::new();
        while let Some(r) = self.out.pop_front() {
            if r.due_ns <= now_ns {
                out.push(r);
            } else {
                keep.push_back(r);
            }
        }
        self.out = keep;
        out.sort_by_key(|r| r.due_ns);
    }
    fn next_due(&self) -> Option<u64> {
        let a = self.ex.as_ref().and_then(|e| e.next_due()).map(|t| t.0);
        let b = self.out.iter().map(|r| r.due_ns).min();
        match (a, b) {
            (Some(x), Some(y)) => Some(x.min(y)),
            (x, y) => x.or(y),
        }
    }
    fn truth(&self, a: AccountId) -> TruthRow {
        let mut t = TruthRow {
            account: a,
            venue: self.venue,
            positions: Vec::new(),
            inventory: Vec::new(),
            balances: Vec::new(),
            undelivered: !self.out.is_empty() || self.ex.as_ref().is_some_and(|e| e.pending_replies() > 0),
            excluded: Vec::new(),
        };
        let view = self.ex.as_ref().and_then(|e| e.account(a));
        let mut bases: Vec<AssetId> = Vec::new();
        for (i, base, _, perp) in &self.insts {
            if *perp {
                let q = view.as_ref().map_or(0, |v| i128::from(v.position(*i).qty.0));
                if q != 0 {
                    t.positions.push((*i, q));
                }
            } else if !bases.contains(base) {
                bases.push(*base);
            }
        }
        if let Some(v) = view.as_ref() {
            for (venue, asset, b) in v.balances() {
                if venue != self.venue {
                    continue;
                }
                let SimBalance { total, .. } = b;
                if bases.contains(&asset) {
                    t.inventory.push((asset, i128::from(total.0)));
                } else {
                    t.balances.push((asset, i128::from(total.0)));
                }
            }
        }
        t.positions.sort_by_key(|p| p.0.0);
        t.inventory.retain(|p| p.1 != 0);
        t.inventory.sort_by_key(|p| p.0.0);
        t.balances.sort_by_key(|p| p.0.0);
        t
    }
    fn accounts(&self) -> Vec<AccountId> {
        self.accounts.clone()
    }
    fn open_orders(&self, a: AccountId) -> Vec<(SlotId, u32)> {
        let Some(v) = self.ex.as_ref().and_then(|e| e.account(a)) else {
            return Vec::new();
        };
        (0..SLOT_SCAN)
            .filter_map(|s| v.order(SlotId(s)))
            .filter(|o| o.resting)
            .map(|o| (o.slot, o.r#gen))
            .collect()
    }
    fn cancel_all(&mut self, a: AccountId, now_ns: u64) {
        let mut c = GwCmd::sim_reset(a);
        c.op = GwOp::CancelAll;
        c.side = Side::Buy;
        c.tif = Tif::Gtc;
        self.on_command(&c, now_ns);
    }
    fn free_quote(&self, a: AccountId) -> i128 {
        let Some(q) = self.insts.first().map(|x| x.2) else {
            return 0;
        };
        self.ex
            .as_ref()
            .and_then(|e| e.account(a))
            .map_or(0, |v| i128::from(v.balance(self.venue, q).free.0))
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
    ) {
        if self.inst(inst).is_none() {
            return;
        }
        let px = self.bbo.get(&inst.0).copied().unwrap_or(0);
        // The exchange truth settles it like a fill (position, cost, realised PnL, balances).
        if let Some(ex) = self.ex.as_mut() {
            ex.external_fill(a, inst, side, Qty(qty), Px(px));
        }
        let trade_id = self.next_trade;
        self.next_trade += 1;
        if push {
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
            };
            self.out.push_back(VenueOut {
                due_ns: now_ns + 1_000_000,
                rep: RepMsg::ExternalFill(x),
                client_id: client_id.to_string(),
            });
        }
    }
    fn state_hash(&self) -> u64 {
        self.ex.as_ref().map_or(0, |e| e.state_hash())
    }
}

impl SimMatcher {
    /// The matcher's private refdata (tests).
    pub fn refdata(&self) -> Option<&RefData> {
        self.rd.as_ref()
    }
    /// `true` when the exchange could be built from the world.
    pub fn is_ready(&self) -> bool {
        self.ex.is_some()
    }
}
