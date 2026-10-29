//! Offline market source and the repository's simulated exchange, on separate tasks.
use crate::{Error, config::Paper, io};
use hft_event::*;
use hft_refdata::{AccountKind, RefData};
use hft_rt::{ByteProducer, Consumer, Producer};
use hft_sim::{SimExchange, SimSetup};
use hft_types::{AccountId, InstId, Px, Qty, Tsc, VenueId};
use std::collections::VecDeque;
use zerocopy::FromZeros;

/// The venue model of `[sim]`: `maker_fill`, and the calibration table's options and condition
/// thresholds when `calib` is set (with the file's SHA-256).
#[derive(Debug, Clone)]
pub struct Model {
    pub maker_fill: hft_sim::MakerFill,
    pub venue: hft_sim::VenueModel,
    pub cond: hft_sim::CondCfg,
    pub calib_sha: Option<String>,
}

pub fn model(p: &Paper) -> Result<Model, Error> {
    let mut m = Model {
        maker_fill: match p.maker_fill {
            crate::config::MakerFillKind::TradeThrough => hft_sim::MakerFill::TradeThrough,
            crate::config::MakerFillKind::Queue => hft_sim::MakerFill::Queue,
        },
        venue: hft_sim::VenueModel::default(),
        cond: hft_sim::CondCfg::default(),
        calib_sha: None,
    };
    if let Some(path) = &p.calib {
        let text = std::fs::read(path)
            .map_err(|e| Error::config("SimConfig", format!("calib {}: {e}", path.display())))?;
        hft_sim::calib::apply(&text, &mut m.maker_fill, &mut m.venue, &mut m.cond)
            .map_err(|e| Error::config("SimConfig", format!("calib {}: {e}", path.display())))?;
        use sha2::Digest;
        m.calib_sha = Some(
            sha2::Sha256::digest(&text)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
        );
    }
    Ok(m)
}

pub fn exchange(p: &Paper, rd: &RefData, hz: u64) -> Result<SimExchange, Error> {
    let m = model(p)?;
    let accounts = (1..=255)
        .filter_map(|a| {
            let m = rd.account(AccountId(a));
            match &m.kind {
                AccountKind::Sim(s) if m.id.0 != 0 => Some((m.id, s.clone())),
                _ => None,
            }
        })
        .collect();
    let mut ex = SimExchange::new(
        &SimSetup {
            tuning: hft_sim::SimTuning {
                maker_fill: m.maker_fill,
                ..hft_sim::SimTuning::with_seed(p.seed)
            },
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
    .map_err(|e| Error::config("SimConfig", format!("{e:?}")))?;
    ex.set_venue_model(m.venue);
    Ok(ex)
}
#[derive(Debug)]
pub struct Feed {
    pub venue: VenueId,
    pub out: Producer<FeedMsg>,
    pub commands: Consumer<FeedCmd>,
    pub quotes: Vec<crate::config::Quote>,
    pub subscribed: Vec<bool>,
    /// The published TSC → UTC line (synthetic quotes' `ex_ts_ns`).
    pub clock: hft_rt::UtcClock,
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
            h.ex_ts_ns = self.clock.utc_ns(now);
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
            let _ = self.out.try_push(FeedMsg::Bbo(b, hft_event::RecordFlags(0)));
        }
        Ok(true)
    }
}
pub struct Gateway {
    pub exchange: SimExchange,
    pub reader: hft_journal::JournalReader,
    /// Opens the seq tail reader at a sequence number (after a tail gap or read error).
    reopen: Reopen,
    tail: TailGuard,
    pub orders: Consumer<GwCmd>,
    pub replies: ByteProducer,
    rd: &'static RefData,
    seen: u64,
    event_tsc: u64,
    awaiting: Option<(u64, VenueId)>,
    epoch: u16,
    pending: VecDeque<(EventKind, Vec<u8>)>,
    /// Decoded levels of the current L2 record (bodies are not 8-byte aligned).
    levels: Vec<Level>,
    /// Latency condition of each command (`[sim] calib` with pools) and the TSC → UTC line.
    cond: Option<(hft_sim::CondTracker, hft_rt::UtcClock)>,
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
        reopen: Reopen,
        orders: Consumer<GwCmd>,
        replies: ByteProducer,
        rd: &'static RefData,
        epoch: u32,
    ) -> Self {
        Self {
            exchange,
            reader,
            reopen,
            tail: TailGuard::default(),
            orders,
            replies,
            rd,
            seen: 0,
            event_tsc: 0,
            awaiting: None,
            epoch: (epoch & 0x7fff) as u16,
            pending: VecDeque::with_capacity(hft_sim::PENDING_REPLY_CAP),
            levels: Vec::with_capacity(4096),
            cond: None,
        }
    }
    /// Commands draw from the calibration table's conditional pools: `t` follows the market
    /// records read from the journal, `utc` gives each decision's UTC (window phase).
    pub fn with_condition(mut self, t: hft_sim::CondTracker, utc: hft_rt::UtcClock) -> Self {
        self.cond = Some((t, utc));
        self
    }
    /// The tail reader failed (`what`): reopen it at the record after the last one consumed.
    fn tail_failed(&mut self, what: String) -> Result<(), Error> {
        let seq = self.seen + 1;
        let reopen = &mut self.reopen;
        if let Some(r) = self.tail.on_error(&what, || reopen(seq))? {
            self.reader = r;
            eprintln!("hft-core: sim-gw journal tail {what}: reader reopened at seq {seq}");
        }
        Ok(())
    }
    pub fn poll(&mut self, now: Tsc) -> Result<bool, Error> {
        use hft_journal::Next;
        let mut busy = false;
        let mut caught_up = false;
        let mut tail_err: Option<String> = None;
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
            if let Some((seq, venue)) = self.awaiting {
                let Some(cmd) = self.orders.try_pop() else {
                    break;
                };
                if cmd.decision_seq != 0 && cmd.decision_seq != seq {
                    return Err(Error::runtime(
                        "PaperOrder",
                        "out-sim and journal decision sequence differ",
                        4,
                    ));
                }
                if let Some(report) = written(&cmd, venue, now) {
                    io::rep_parts(&RepMsg::SendReport(report), &[], |kind, parts| {
                        self.pending.push_back((kind, parts.concat()));
                    });
                }
                if let Some((t, utc)) = &self.cond {
                    self.exchange
                        .set_condition(t.condition(cmd.decided_tsc, utc.utc_ns(cmd.decided_tsc)));
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
                    self.tail.ok();
                    self.seen = r.hdr.seq;
                    self.event_tsc = self.event_tsc.max(r.hdr.tsc);
                    busy = true;
                    match EventKind(r.hdr.kind) {
                        EventKind::ORDER_DECISION => {
                            if let Ok(d) = r.read::<OrderDecision>()
                                && d.reject == 0
                                && r.hdr.flags & RecordFlags::SHADOW.0 == 0
                            {
                                self.awaiting = Some((r.hdr.seq, d.venue));
                            }
                        }
                        kind => {
                            let fed = hft_sim::glue::feed_record(
                                &mut self.exchange,
                                &r.hdr,
                                kind,
                                r.body,
                                &mut self.levels,
                            );
                            if fed == hft_sim::glue::Fed::Market
                                && let Some((t, _)) = &mut self.cond
                            {
                                t.on_market(kind, r.body, r.hdr.tsc);
                            }
                        }
                    }
                }
                Next::Pending | Next::End => {
                    caught_up = true;
                    break;
                }
                // A gap or read error while tailing (seen at a segment roll under CPU contention)
                // reopens the reader at the next sequence number; only repeated failures stop sim-gw.
                Next::Err(e) => {
                    tail_err = Some(format!("read error {e:?}"));
                    break;
                }
                Next::Gap(g) => {
                    tail_err = Some(format!("gap {g:?}"));
                    break;
                }
                // Multi-part records (large L2 snapshots) arrive reassembled.
                Next::Event(ev) => {
                    self.tail.ok();
                    if matches!(ev.kind, EventKind::L2_SNAPSHOT | EventKind::L2_BATCH) {
                        self.seen = ev.seq;
                        self.event_tsc = self.event_tsc.max(ev.tsc);
                        busy = true;
                        let hdr = RecordHeader {
                            len: ev.body.len() as u32,
                            kind: ev.kind.0,
                            flags: ev.flags.0,
                            seq: ev.seq,
                            tsc: ev.tsc,
                        };
                        l2(&mut self.exchange, &mut self.levels, &hdr, ev.kind, ev.body);
                        if let Some((t, _)) = &mut self.cond {
                            t.on_market(ev.kind, ev.body, ev.tsc);
                        }
                    }
                }
            }
        }
        if let Some(what) = tail_err {
            self.tail_failed(what)?;
        }
        // Commands that project no decision (SimSettle, SimReset, SimTransfer) run once the
        // journal is consumed and no decision is awaited: they must not wait for the next order.
        if caught_up && self.awaiting.is_none() {
            while let Some(cmd) = pop_unsequenced(&mut self.orders) {
                self.exchange.on_command(&cmd, cmd.decided_tsc);
                busy = true;
            }
        }
        // Paper time is the consumed journal prefix, never wall time ahead of that prefix.
        let due = Tsc(self.event_tsc.min(now.0));
        if self.awaiting.is_none() {
            self.exchange.advance(due);
        }
        self.exchange.drain_due(due, |r| {
            reply_parts(self.rd, self.epoch, &r.msg, r.bulk, |kind, parts| {
                self.pending.push_back((kind, parts.concat()));
            });
        });
        if self.pending.len() > hft_sim::PENDING_REPLY_CAP {
            return Err(Error::runtime("PaperReplies", "pending replies exhausted", 3));
        }
        Ok(busy)
    }
}

/// Opens the seq tail reader at a sequence number.
pub type Reopen = Box<dyn FnMut(u64) -> Result<hft_journal::JournalReader, Error> + Send>;

/// Consecutive tail failures (gap, read error or a failed reopen) without a record in between
/// after which sim-gw stops with `PaperJournal`.
pub const TAIL_MAX_FAILS: u32 = 1_000;

/// Counts consecutive tail failures of sim-gw's seq reader (crates/hft-sim README "sim-gw 任务"
/// step 1).
#[derive(Debug, Default)]
pub struct TailGuard {
    pub fails: u32,
    pub reopened: u64,
}
impl TailGuard {
    /// A record or event was read: the failure streak ends.
    pub fn ok(&mut self) {
        self.fails = 0;
    }
    /// A failure `what`: `reopen` the reader (its new reader replaces the failed one), or stop
    /// after `TAIL_MAX_FAILS` consecutive failures.
    pub fn on_error<R>(
        &mut self,
        what: &str,
        reopen: impl FnOnce() -> Result<R, Error>,
    ) -> Result<Option<R>, Error> {
        self.fails += 1;
        if self.fails >= TAIL_MAX_FAILS {
            return Err(Error::runtime(
                "PaperJournal",
                format!("journal tail failed {} times in a row, last: {what}", self.fails),
                4,
            ));
        }
        match reopen() {
            Ok(r) => {
                self.reopened += 1;
                Ok(Some(r))
            }
            Err(_) => Ok(None),
        }
    }
}

/// The shared journal glue (hft-sim "journal 衔接"); re-exported under the names sim-gw has used.
pub use hft_sim::glue::{reply_parts, written};

/// An `L2Snapshot` (exchange or TopN origin; engine keyframes are book state, not market
/// data) or `L2Batch` record to the simulated exchange: venues without a Bbo stream
/// (Polymarket) have no other source of displayed depth for the queue model.
pub fn l2(
    exchange: &mut SimExchange,
    levels: &mut Vec<Level>,
    hdr: &RecordHeader,
    kind: EventKind,
    body: &[u8],
) {
    hft_sim::glue::feed_record(exchange, hdr, kind, body, levels);
}

/// Pops the `out-sim` head when it projects no order decision (`decision_seq = 0`); an order
/// command stays at the head for the decision barrier of `Gateway::poll`.
pub fn pop_unsequenced(orders: &mut Consumer<GwCmd>) -> Option<GwCmd> {
    let b = orders.read_batch(1);
    let head = b.slices().0.first().copied()?;
    if head.decision_seq != 0 {
        return None;
    }
    b.release();
    Some(head)
}

#[cfg(test)]
mod tail_guard_tests {
    use super::{TAIL_MAX_FAILS, TailGuard};
    use crate::Error;

    /// CORE-436: a failure reopens the reader; a record ends the streak; `TAIL_MAX_FAILS` failures in
    /// a row stop sim-gw with `PaperJournal` naming the last failure.
    #[test]
    fn core_436_tail_reopen() {
        let mut g = TailGuard::default();
        // .1 reopen succeeds: the new reader is returned.
        assert_eq!(
            g.on_error("gap", || Ok::<u32, Error>(7)).unwrap(),
            Some(7),
            "CORE-436.1"
        );
        assert_eq!((g.fails, g.reopened), (1, 1), "CORE-436.1");
        // .2 a record resets the streak.
        g.ok();
        assert_eq!(g.fails, 0, "CORE-436.2");
        // .3 reopen fails: no reader, keep going below the limit.
        for _ in 1..TAIL_MAX_FAILS {
            assert_eq!(
                g.on_error("gap", || Err::<u32, Error>(Error::runtime(
                    "JournalReader",
                    "x",
                    4
                )))
                .unwrap(),
                None,
                "CORE-436.3"
            );
        }
        // .4 the TAIL_MAX_FAILS-th failure in a row stops sim-gw with the reason.
        let e = g
            .on_error("gap MissingSegment", || Ok::<u32, Error>(1))
            .unwrap_err();
        let msg = format!("{e}");
        assert!(
            msg.contains("PaperJournal") && msg.contains("gap MissingSegment"),
            "CORE-436.4 {msg}"
        );
    }
}
