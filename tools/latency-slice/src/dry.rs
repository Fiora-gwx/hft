//! `--dry-run` tasks (README "dry-run"): the replay feed (hft-testkit `market_stream` rendered as
//! the product line's `bookTicker` and decoded by the venue) and the loop-back gw (venue `encode`,
//! exchange answers decoded by the venue).

use std::collections::VecDeque;

use hft_event::{
    AccountConfigReportHdr, BalanceReport, Bbo, ConnRole, ConnState, ConnStateKind, ExternalFill, FeedMsg,
    FundingSettle, GwCmd, GwOp, IndexPrice, L2BatchHdr, L2SnapshotPart, LeverageItem, Liquidation,
    MarginItem, MarginReportHdr, MarkPrice, OrderEvent, PositionReport, RepMsg, SendReport, SendStatus,
    Trade,
};
use hft_refdata::RefData;
use hft_rt::{Consumer, Producer};
use hft_testkit::{GapPolicy, MarketStreamCfg, MdEvent, market_stream_with, seed};
use hft_types::{AccountId, ClientOrderId, ConnId, InstId, Px, Qty, Side, Tsc, VenueId, write_fixed};
use hft_venue_proto::{
    ExchIdHint, MdSink, MsgOp, ReportSink, RouteId, RouteKind, RxMeta, TargetRef, VenueProtocol, WallClock,
};
use venue_binance::{FeedCtx, GwCtx, Templates, encode_req_id};

use crate::live::route;
use crate::threads::utc_ns;

/// Delay of the loop-back exchange answers.
pub const ANSWER_DELAY_US: u64 = 200;
/// Receive stamps of the dry run lag the counter by this much (`hw_rx_ns = sw_rx_ns`, SW data set).
pub const STAMP_LAG_NS: i64 = 50_000;
/// gw connection id of the loop-back (first gw number).
const GW_CONN: ConnId = ConnId(hft_venue::GW_FIRST);

fn fixed(raw: i64, exp: i8) -> String {
    let mut b = [0u8; 40];
    let n = write_fixed(raw, exp, &mut b);
    String::from_utf8_lossy(&b[..n]).into_owned()
}

/// The generated `Bbo` stream: `(offset_ns, bbo)`.
pub fn generate(
    venue: VenueId,
    rd: &RefData,
    insts: &[InstId],
    seconds: u32,
    seed_v: u64,
) -> Vec<(u64, Bbo)> {
    let Some(&first) = insts.first() else {
        return Vec::new();
    };
    let row = rd.inst(first);
    let mut cfg = MarketStreamCfg::new(
        venue,
        insts,
        (seconds as usize).saturating_mul(150).max(50),
        GapPolicy::Contiguous,
    );
    cfg.tick = Px(row.tick.max(1));
    cfg.lot = Qty(row.lot.max(1));
    // ≈ 1 000 quote units per base unit.
    cfg.start_bid = Px(1_000i64.saturating_mul(10i64.pow(u32::from((-row.px_exp).max(0) as u8))));
    let s = seed::sample(seed_v, &market_stream_with(cfg));
    s.into_iter()
        .filter_map(|e| match e {
            MdEvent::Bbo(b) => Some((b.h.rx_tsc, b)),
            _ => None,
        })
        .collect()
}

/// Pushes decoded market events into the `bbo` ring.
struct RingMd<'a> {
    out: &'a mut Producer<FeedMsg>,
    dropped: u64,
}

impl MdSink for RingMd<'_> {
    fn bbo(&mut self, e: &Bbo) {
        if self.out.try_push(FeedMsg::Bbo(*e)).is_err() {
            self.dropped += 1;
        }
    }
    fn l2_begin(&mut self, _h: &L2BatchHdr) {}
    fn l2_level(&mut self, _s: Side, _p: Px, _q: Qty) {}
    fn l2_end(&mut self) {}
    fn l2_snapshot(&mut self, _p: &L2SnapshotPart<'_>) {}
    fn trade(&mut self, _e: &Trade) {}
    fn mark(&mut self, _e: &MarkPrice) {}
    fn index(&mut self, _e: &IndexPrice) {}
    fn funding(&mut self, _e: &hft_event::Funding) {}
    fn liquidation(&mut self, _e: &Liquidation) {}
}

/// The replay feed task.
pub struct ReplayFeed<P: VenueProtocol<FeedCtx = FeedCtx>> {
    events: Vec<(u64, Bbo)>,
    k: usize,
    start: Option<u64>,
    span_ns: u64,
    laps: u64,
    out: Producer<FeedMsg>,
    rd: &'static RefData,
    ctx: FeedCtx,
    route: RouteId,
    ticks_per_us: u64,
    pub delivered: u64,
    pub decode_err: u64,
    pub dropped: u64,
    _p: core::marker::PhantomData<P>,
}

impl<P: VenueProtocol<FeedCtx = FeedCtx>> std::fmt::Debug for ReplayFeed<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReplayFeed")
            .field("delivered", &self.delivered)
            .finish()
    }
}

impl<P: VenueProtocol<FeedCtx = FeedCtx>> ReplayFeed<P> {
    pub fn new(
        events: Vec<(u64, Bbo)>,
        out: Producer<FeedMsg>,
        rd: &'static RefData,
        ctx: FeedCtx,
        ticks_per_us: u64,
    ) -> Self {
        let span_ns = events.last().map_or(1, |e| e.0 + 1);
        ReplayFeed {
            events,
            k: 0,
            start: None,
            span_ns,
            laps: 0,
            out,
            rd,
            ctx,
            route: route::<P>(RouteKind::Bbo).unwrap_or(RouteId(0)),
            ticks_per_us: ticks_per_us.max(1),
            delivered: 0,
            decode_err: 0,
            dropped: 0,
            _p: core::marker::PhantomData,
        }
    }

    fn payload(&self, b: &Bbo, utc: i64) -> String {
        let row = self.rd.inst(b.h.inst);
        let sym = &self.rd.inst_meta(b.h.inst).symbol;
        let (pe, qe) = (row.px_wire_exp, row.qty_wire_exp);
        let core = format!(
            r#""u":{},"s":"{sym}","b":"{}","B":"{}","a":"{}","A":"{}""#,
            b.update_id + self.laps * 1_000_000,
            fixed(b.bid_px.0, pe),
            fixed(b.bid_qty.0, qe),
            fixed(b.ask_px.0, pe),
            fixed(b.ask_qty.0, qe)
        );
        if P::VENUE == VenueId::BINANCE_USDM {
            let ms = utc / 1_000_000;
            format!(r#"{{"e":"bookTicker",{core},"T":{ms},"E":{ms}}}"#)
        } else {
            format!("{{{core}}}")
        }
    }

    /// Delivers every event due at `now` (same offset = same read, one `rx_tsc`).
    pub fn poll(&mut self, now: u64) -> bool {
        let start = *self.start.get_or_insert_with(|| {
            let up = ConnState {
                venue: P::VENUE,
                conn: 0,
                state: ConnStateKind::Up,
                role: ConnRole::Md,
                inst: InstId(0),
                reason: 0,
                account: AccountId(0),
                _pad2: [0; 3],
                rx_tsc: now,
            };
            let _ = self.out.try_push(FeedMsg::ConnState(up));
            now
        });
        if self.events.is_empty() {
            return false;
        }
        let mut did = false;
        let mut last_off = None;
        let mut rx_tsc = now;
        loop {
            let Some(&(off, b)) = self.events.get(self.k) else {
                self.k = 0;
                self.laps += 1;
                continue;
            };
            let at = start + (self.laps * self.span_ns + off) / 1_000 * self.ticks_per_us;
            if at > now {
                break;
            }
            if last_off != Some(off) {
                rx_tsc = hft_rt::tsc_now().0;
                last_off = Some(off);
            }
            let utc = utc_ns();
            let p = self.payload(&b, utc);
            let rx = RxMeta {
                rx_tsc,
                hw_rx_ns: utc - STAMP_LAG_NS,
                sw_rx_ns: utc - STAMP_LAG_NS,
                prev_override: 0,
                conn: ConnId(0),
                account: AccountId(0),
                _pad: [0; 2],
                ts_flags: 0,
                rx_empty_gap_ns: 1_000,
                _pad2: 0,
            };
            let head = &p.as_bytes()[..p.len().min(64)];
            let c = P::classify(self.route, MsgOp::Text, head);
            let mut sink = RingMd {
                out: &mut self.out,
                dropped: 0,
            };
            match P::peek(self.rd, &self.ctx, c, p.as_bytes())
                .and_then(|_| P::decode_md(self.rd, &mut self.ctx, c, p.as_bytes(), &rx, &mut sink))
            {
                Ok(_) => self.delivered += 1,
                Err(_) => self.decode_err += 1,
            }
            self.dropped += sink.dropped;
            self.k += 1;
            did = true;
        }
        did
    }
}

/// Pushes decoded reports into the `rep` ring.
struct RingRep<'a> {
    out: &'a mut Producer<RepMsg>,
}

impl ReportSink for RingRep<'_> {
    fn order(&mut self, e: &OrderEvent) {
        let _ = self.out.try_push(RepMsg::OrderEvent(*e));
    }
    fn position(&mut self, _e: &PositionReport) {}
    fn balance(&mut self, _e: &BalanceReport) {}
    fn funding_settle(&mut self, _e: &FundingSettle) {}
    fn external_fill(&mut self, _e: &ExternalFill) {}
    fn margin(&mut self, _h: &MarginReportHdr, _i: &[MarginItem]) {}
    fn account_config(&mut self, _h: &AccountConfigReportHdr, _l: &[LeverageItem]) {}
    fn exch_id(&mut self, _key: &[u8], _owner: Option<&ClientOrderId>) -> ExchIdHint {
        ExchIdHint::Recorded
    }
}

/// The loop-back gw task.
pub struct LoopGw<P: VenueProtocol<GwCtx = GwCtx, Templates = Templates>> {
    cmds: Consumer<GwCmd>,
    out: Producer<RepMsg>,
    rd: &'static RefData,
    tpl: Templates,
    ctx: GwCtx,
    account: AccountId,
    epoch: u16,
    pend: VecDeque<(u64, String)>,
    gens: Vec<u32>,
    next_id: u64,
    delay_ticks: u64,
    started: bool,
    frame: Vec<u8>,
    pub sent: u64,
    pub answered: u64,
    pub encode_err: u64,
    _p: core::marker::PhantomData<P>,
}

impl<P: VenueProtocol<GwCtx = GwCtx, Templates = Templates>> std::fmt::Debug for LoopGw<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoopGw").field("sent", &self.sent).finish()
    }
}

impl<P: VenueProtocol<GwCtx = GwCtx, Templates = Templates>> LoopGw<P> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cmds: Consumer<GwCmd>,
        out: Producer<RepMsg>,
        rd: &'static RefData,
        tpl: Templates,
        account: AccountId,
        epoch: u16,
        ticks_per_us: u64,
    ) -> Self {
        LoopGw {
            cmds,
            out,
            rd,
            tpl,
            ctx: GwCtx,
            account,
            epoch,
            pend: VecDeque::new(),
            gens: vec![0; 4096],
            next_id: 1_000,
            delay_ticks: ANSWER_DELAY_US * ticks_per_us.max(1),
            started: false,
            frame: vec![0; P::ORDER_FRAME_CAP],
            sent: 0,
            answered: 0,
            encode_err: 0,
            _p: core::marker::PhantomData,
        }
    }

    fn cid(&self, slot: u32, r#gen: u32) -> ClientOrderId {
        ClientOrderId {
            epoch: self.epoch,
            venue: P::VENUE,
            account: self.account,
            slot,
            r#gen,
        }
    }

    fn answer(&mut self, op: GwOp, slot: u32, r#gen: u32, inst: InstId) -> String {
        let req = encode_req_id(op, slot, r#gen)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
        let mut t = [0u8; 24];
        self.cid(slot, r#gen).encode(&mut t);
        let cid = String::from_utf8_lossy(&t).into_owned();
        let sym = &self.rd.inst_meta(inst).symbol;
        let utc = utc_ns();
        self.next_id += 1;
        let oid = self.next_id;
        let spot = P::VENUE == VenueId::BINANCE_SPOT;
        let time = if spot {
            format!(r#""transactTime":{}"#, utc / 1_000)
        } else {
            format!(r#""updateTime":{}"#, utc / 1_000_000)
        };
        match (op, spot) {
            (GwOp::Place, _) => format!(
                r#"{{"id":"{req}","status":200,"result":{{"orderId":{oid},"symbol":"{sym}","status":"NEW","clientOrderId":"{cid}",{time}}}}}"#
            ),
            (_, false) => format!(
                r#"{{"id":"{req}","status":200,"result":{{"orderId":{oid},"symbol":"{sym}","status":"CANCELED","clientOrderId":"{cid}",{time}}}}}"#
            ),
            (_, true) => format!(
                r#"{{"id":"{req}","status":200,"result":{{"symbol":"{sym}","origClientOrderId":"{cid}","orderId":{oid},"clientOrderId":"x{oid}",{time},"status":"CANCELED"}}}}"#
            ),
        }
    }

    pub fn poll(&mut self, now: u64) -> bool {
        let mut did = false;
        if !self.started {
            self.started = true;
            let up = ConnState {
                venue: P::VENUE,
                conn: GW_CONN.0,
                state: ConnStateKind::Up,
                role: ConnRole::Order,
                inst: InstId(0),
                reason: 0,
                account: self.account,
                _pad2: [0; 3],
                rx_tsc: now,
            };
            let _ = self.out.try_push(RepMsg::ConnState(up));
            did = true;
        }
        while let Some(c) = self.cmds.try_pop() {
            did = true;
            let (slot, r#gen) = match c.op {
                GwOp::Place => (c.slot.0, c.r#gen),
                _ => (
                    c.target_slot.0,
                    self.gens.get(c.target_slot.0 as usize).copied().unwrap_or(0),
                ),
            };
            let tgt = TargetRef {
                id: self.cid(slot, r#gen),
                exch_order_id: 0,
            };
            let utc = utc_ns();
            let clk = WallClock {
                unix_ms: utc / 1_000_000,
                unix_us: utc / 1_000,
            };
            let ok = P::encode(self.rd, &self.tpl, &c, &tgt, &clk, &mut self.frame).is_ok();
            let sent = hft_rt::tsc_now().0;
            let r = SendReport {
                decision_seq: c.decision_seq,
                sent_tsc: Tsc(sent),
                slot: c.slot,
                r#gen: c.r#gen,
                target_slot: c.target_slot,
                target_gen: if c.op == GwOp::Place { 0 } else { r#gen },
                venue: P::VENUE,
                account: c.account,
                conn: GW_CONN,
                status: if ok {
                    SendStatus::Written
                } else {
                    SendStatus::EncodeError
                },
                op: c.op,
                _pad: [0; 3],
                os_errno: 0,
                _pad2: 0,
            };
            let _ = self.out.try_push(RepMsg::SendReport(r));
            if !ok {
                self.encode_err += 1;
                continue;
            }
            self.sent += 1;
            if c.op == GwOp::Place
                && let Some(g) = self.gens.get_mut(slot as usize)
            {
                *g = r#gen;
            }
            let a = self.answer(c.op, slot, r#gen, c.inst);
            self.pend.push_back((now + self.delay_ticks, a));
        }
        while let Some((due, _)) = self.pend.front() {
            if *due > now {
                break;
            }
            let Some((_, p)) = self.pend.pop_front() else {
                break;
            };
            let utc = utc_ns();
            let rx = RxMeta {
                rx_tsc: hft_rt::tsc_now().0,
                hw_rx_ns: utc - STAMP_LAG_NS,
                sw_rx_ns: utc - STAMP_LAG_NS,
                prev_override: 0,
                conn: GW_CONN,
                account: self.account,
                _pad: [0; 2],
                ts_flags: 0,
                rx_empty_gap_ns: 0,
                _pad2: 0,
            };
            let head = &p.as_bytes()[..p.len().min(64)];
            let c = P::classify_report(MsgOp::Text, head);
            let mut sink = RingRep { out: &mut self.out };
            if P::decode_report(self.rd, &mut self.ctx, c, p.as_bytes(), &rx, &mut sink).is_ok() {
                self.answered += 1;
            }
            did = true;
        }
        did
    }
}
