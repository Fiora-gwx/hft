//! Offline market source and the repository's simulated exchange, on separate tasks.
use crate::{Error, config::Paper, io};
use hft_event::*;
use hft_refdata::{AccountKind, RefData};
use hft_rt::{ByteProducer, Consumer, Producer, TscClock};
use hft_sim::{MarketEventRef, SimExchange, SimInput, SimSetup};
use hft_types::{AccountId, InstId, Px, Qty, Tsc, VenueId};
use std::collections::VecDeque;
use zerocopy::FromZeros;

pub fn exchange(p: &Paper, rd: &RefData, hz: u64) -> Result<SimExchange, Error> {
    let accounts = (1..=255)
        .filter_map(|a| {
            let m = rd.account(AccountId(a));
            match &m.kind {
                AccountKind::Sim(s) if m.id.0 != 0 => Some((m.id, s.clone())),
                _ => None,
            }
        })
        .collect();
    SimExchange::new(
        &SimSetup {
            tuning: hft_sim::SimTuning::with_seed(p.seed),
            tsc_hz_milli: hz,
            accounts,
            latency_models: vec![hft_sim::LatencyModel::Fixed {
                entry_us: p.entry_us,
                resp_us: p.resp_us,
            }],
            margin_models: vec![hft_sim::MarginModel {
                mmr: hft_sim::MmrSource::Flat(hft_types::Rate(p.mmr_raw)),
                leverage: p.leverage,
                liq_fee: hft_types::Rate(p.liq_fee_raw),
                liq_slippage_bp: p.liq_slippage_bp,
            }],
        },
        rd,
    )
    .map_err(|e| Error::config("SimConfig", format!("{e:?}")))
}
#[derive(Debug)]
pub struct Feed {
    pub venue: VenueId,
    pub out: Producer<FeedMsg>,
    pub commands: Consumer<FeedCmd>,
    pub quotes: Vec<crate::config::Quote>,
    pub subscribed: Vec<bool>,
    pub clock: TscClock,
    pub interval_ticks: u64,
    pub next: u64,
    pub update: u64,
}
impl Feed {
    pub fn poll(&mut self, now: Tsc) -> Result<bool, Error> {
        for _ in 0..64 {
            let Some(cmd) = self.commands.try_pop() else { break };
            match cmd {
                FeedCmd::Subscribe { inst, .. } | FeedCmd::Unsubscribe { inst } => {
                    if let Some(ix) = self.quotes.iter().position(|q| q.inst == inst.0) {
                        self.subscribed[ix] = matches!(cmd, FeedCmd::Subscribe { .. });
                    }
                }
                _ => {}
            }
        }
        if now.0 < self.next {
            return Ok(false);
        }
        self.next = now.0.saturating_add(self.interval_ticks);
        self.update += 1;
        for (q, on) in self.quotes.iter().zip(&self.subscribed) {
            if !on {
                continue;
            }
            // Health is connection based; each recorded quote comes from this synthetic member.
            let state = ConnState {
                venue: self.venue,
                conn: 0,
                state: ConnStateKind::Up,
                role: ConnRole::Md,
                inst: InstId(q.inst),
                reason: 0,
                account: AccountId(0),
                _pad2: [0; 3],
                rx_tsc: now.0,
            };
            let mut h = MarketHdr::new_zeroed();
            h.venue = self.venue;
            h.inst = InstId(q.inst);
            h.rx_tsc = now.0;
            h.ex_ts_ns = self.clock.to_utc_ns(now);
            let b = Bbo {
                h,
                bid_px: Px(q.bid_raw),
                ask_px: Px(q.ask_raw),
                bid_qty: Qty(q.qty_raw),
                ask_qty: Qty(q.qty_raw),
                update_id: self.update,
            };
            if self.out.free_hint() < 3 {
                return Err(Error::runtime("PaperFeedFull", "feed ring exhausted", 3));
            }
            let mut connection = state;
            connection.inst = InstId(0);
            let _ = self.out.try_push(FeedMsg::ConnState(connection));
            let _ = self.out.try_push(FeedMsg::ConnState(state));
            let _ = self.out.try_push(FeedMsg::Bbo(b));
        }
        Ok(true)
    }
}
pub struct Gateway {
    pub exchange: SimExchange,
    pub reader: hft_journal::JournalReader,
    pub orders: Consumer<GwCmd>,
    pub replies: ByteProducer,
    rd: &'static RefData,
    seen: u64,
    event_tsc: u64,
    awaiting: Option<u64>,
    pending_order: Option<GwCmd>,
    pending: VecDeque<(EventKind, Vec<u8>)>,
}
impl std::fmt::Debug for Gateway {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaperGateway")
            .field("seen", &self.seen)
            .finish_non_exhaustive()
    }
}
impl Gateway {
    pub fn new(
        exchange: SimExchange,
        reader: hft_journal::JournalReader,
        orders: Consumer<GwCmd>,
        replies: ByteProducer,
        rd: &'static RefData,
    ) -> Self {
        Self {
            exchange,
            reader,
            orders,
            replies,
            rd,
            seen: 0,
            event_tsc: 0,
            awaiting: None,
            pending_order: None,
            pending: VecDeque::with_capacity(hft_sim::PENDING_REPLY_CAP),
        }
    }
    pub fn poll(&mut self, now: Tsc) -> Result<bool, Error> {
        use hft_journal::Next;
        let mut busy = false;
        while let Some((kind, bytes)) = self.pending.front() {
            if !io::write_record(&mut self.replies, *kind, &[bytes]) {
                return Ok(busy);
            }
            self.pending.pop_front();
            busy = true;
        }
        // An accepted decision is a barrier: publication precedes the out-ring push.
        // Never let later market data overtake a command still being handed off.
        for _ in 0..512 {
            if let Some(seq) = self.awaiting {
                let Some(cmd) = self.pending_order.take().or_else(|| self.orders.try_pop()) else {
                    break;
                };
                if cmd.decision_seq != 0 && cmd.decision_seq != seq {
                    return Err(Error::runtime(
                        "PaperOrder",
                        "out-sim and journal decision sequence differ",
                        4,
                    ));
                }
                self.exchange.on_command(&cmd, cmd.decided_tsc);
                if cmd.decision_seq == seq {
                    self.awaiting = None;
                }
                busy = true;
                continue;
            }
            match self.reader.poll() {
                Next::Record(r) => {
                    self.seen = r.hdr.seq;
                    self.event_tsc = self.event_tsc.max(r.hdr.tsc);
                    busy = true;
                    match EventKind(r.hdr.kind) {
                        EventKind::BBO => {
                            if let Ok(b) = r.read::<Bbo>() {
                                self.exchange
                                    .on_record(&r.hdr, SimInput::Market(MarketEventRef::Bbo(&b)));
                            }
                        }
                        EventKind::TRADE => {
                            if let Ok(b) = r.read::<Trade>() {
                                self.exchange
                                    .on_record(&r.hdr, SimInput::Market(MarketEventRef::Trade(&b)));
                            }
                        }
                        EventKind::MARK_PRICE => {
                            if let Ok(b) = r.read::<MarkPrice>() {
                                self.exchange
                                    .on_record(&r.hdr, SimInput::Market(MarketEventRef::Mark(&b)));
                            }
                        }
                        EventKind::FUNDING => {
                            if let Ok(b) = r.read::<Funding>() {
                                self.exchange
                                    .on_record(&r.hdr, SimInput::Market(MarketEventRef::Funding(&b)));
                            }
                        }
                        EventKind::ORDER_DECISION => {
                            if let Ok(d) = r.read::<OrderDecision>()
                                && d.reject == 0
                                && r.hdr.flags & RecordFlags::SHADOW.0 == 0
                            {
                                self.awaiting = Some(r.hdr.seq);
                            }
                        }
                        _ => {
                            self.exchange.on_record(&r.hdr, SimInput::Clock);
                        }
                    }
                }
                Next::Pending | Next::End => break,
                Next::Err(e) => return Err(Error::runtime("PaperJournal", format!("{e:?}"), 4)),
                Next::Gap(_) => return Err(Error::runtime("PaperJournal", "journal gap", 4)),
                Next::Event(_) => {}
            }
        }
        // Paper time is the consumed journal prefix, never wall time ahead of that prefix.
        let due = Tsc(self.event_tsc.min(now.0));
        if self.awaiting.is_none() {
            self.exchange.advance(due);
        }
        self.exchange.drain_due(due, |r| {
            reply_parts(self.rd, &r.msg, r.bulk, |kind, parts| {
                self.pending.push_back((kind, parts.concat()));
            });
        });
        if self.pending.len() > hft_sim::PENDING_REPLY_CAP {
            return Err(Error::runtime("PaperReplies", "pending replies exhausted", 3));
        }
        Ok(busy)
    }
}

/// Expand SimExchange's account-wide connection fact to its configured venue scope.
pub fn reply_parts(rd: &RefData, msg: &RepMsg, bulk: &[u8], mut emit: impl FnMut(EventKind, &[&[u8]])) {
    if let RepMsg::ConnState(c) = msg
        && c.venue.0 == 0
        && c.account.0 != 0
    {
        let venues = rd.account(c.account).venues;
        for v in 1..64 {
            if venues & (1u64 << v) != 0 {
                let mut state = *c;
                state.venue = VenueId(v);
                io::rep_parts(&RepMsg::ConnState(state), &[], &mut emit);
            }
        }
    } else {
        io::rep_parts(msg, bulk, emit);
    }
}
