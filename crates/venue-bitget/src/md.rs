//! Market data: dispatch by route (README "实现说明" 2), `peek` ids, sequence rules, control hints
//! and decoding of `books1`, `books5`/`books50`, `books` (incremental), `publicTrade` and the
//! perpetual `ticker` (mark, index, funding).

use core::fmt;

use hft_event::{
    Bbo, Funding, IndexPrice, L2BatchHdr, L2SnapHdr, L2SnapshotPart, Level, MarkPrice, MarketHdr, Trade,
};
use hft_refdata::RefData;
use hft_types::{InstId, Px, Qty, Rate, Side};
use hft_venue_proto::{
    Align, ControlHint, DecodeErr, MdOutcome, MdSink, MsgOp, Peek, RawSeq, ResyncMethod, RouteId, RxMeta,
    SeqIds, SeqRule, StreamKind,
};
use serde::Deserialize;
use serde::de::{Deserializer, SeqAccess, Visitor};

use crate::json::{Arg, Arr, Code, One, Raw, UInt, fixed, from, levels, ms_ns};
use crate::session::{Event, ROUTE_BOOKS, ROUTE_BOOKS1, ROUTE_TICKER, ROUTE_TOPN, ROUTE_TRADE};
use crate::{FeedCtx, Line, MdClass};

/// Bytes `classify` may look at.
const HEAD: usize = 64;
/// Levels per side kept from a TopN snapshot (`books50`).
pub(crate) const TOPN_MAX: usize = 50;
/// Levels per side of an incremental `books` message (`maxDepth` ≤ 1000).
pub(crate) const BOOKS_MAX: usize = 1000;
/// Trades per `publicTrade` push.
const TRADES_MAX: usize = 100;

/// The `{"event":"<name>"` of a control frame.
pub(crate) fn event_name(h: &[u8]) -> Option<&[u8]> {
    let rest = h.strip_prefix(b"{\"event\":\"")?;
    let end = rest.iter().position(|&c| c == b'"')?;
    rest.get(..end)
}

/// A data push: `{"action":`, `{"arg":` or `{"data":` first.
pub(crate) fn is_push(h: &[u8]) -> bool {
    h.starts_with(b"{\"action\":") || h.starts_with(b"{\"arg\":") || h.starts_with(b"{\"data\":")
}

pub(crate) fn classify(line: Line, route: RouteId, op: MsgOp, head: &[u8]) -> MdClass {
    if op != MsgOp::Text {
        return MdClass::Unknown;
    }
    let h = head.get(..head.len().min(HEAD)).unwrap_or(&[]);
    if h == b"pong" {
        return MdClass::Pong;
    }
    if let Some(n) = event_name(h) {
        return match n {
            b"subscribe" | b"unsubscribe" | b"error" => MdClass::SubAck,
            _ => MdClass::Unknown,
        };
    }
    if !is_push(h) {
        return MdClass::Unknown;
    }
    match route {
        ROUTE_BOOKS1 => MdClass::Books1,
        ROUTE_TOPN => MdClass::BooksTopN,
        ROUTE_TRADE => MdClass::Trade,
        ROUTE_TICKER if line == Line::Fut => MdClass::Ticker,
        ROUTE_BOOKS => MdClass::BooksIncr,
        _ => MdClass::Unknown,
    }
}

pub(crate) fn seq_rule(c: MdClass) -> SeqRule {
    match c {
        MdClass::Books1 | MdClass::BooksTopN => SeqRule::Snapshot,
        MdClass::BooksIncr => SeqRule::Chain,
        MdClass::Trade => SeqRule::Monotonic,
        MdClass::Ticker => SeqRule::ByTime,
        MdClass::SubAck | MdClass::Pong | MdClass::Unknown => SeqRule::None,
    }
}

pub(crate) fn resync_method(c: MdClass) -> ResyncMethod {
    match c {
        // Resubscribing yields a fresh snapshot (README "行情频道 → 事件").
        MdClass::BooksIncr => ResyncMethod::Resubscribe,
        _ => ResyncMethod::None,
    }
}

/// First increment after a snapshot: the snapshot's `seq` must fall in `[pseq, seq]`.
pub(crate) fn align_first(snap_id: u64, raw: &RawSeq) -> Align {
    if raw.last <= snap_id {
        Align::Drop
    } else if raw.prev <= snap_id {
        Align::Apply
    } else {
        Align::Gap
    }
}

pub(crate) fn control(c: MdClass, payload: &[u8]) -> Option<ControlHint> {
    match c {
        MdClass::SubAck => {
            let e: Event = from(payload).ok()?;
            Some(match e.event {
                "subscribe" | "unsubscribe" if e.code == Code::Absent || e.code.is_zero() => {
                    ControlHint::SubAck { ok: true, code: 0 }
                }
                _ => ControlHint::SubAck {
                    ok: false,
                    code: if e.code == Code::Absent { 0 } else { e.code.raw(0) },
                },
            })
        }
        MdClass::Pong => Some(ControlHint::Pong),
        _ => None,
    }
}

/// Symbol → inst of this line (0 = not in refdata).
pub(crate) fn inst_of(line: Line, rd: &RefData, sym: &str) -> InstId {
    rd.symbols(line.venue()).get(sym.as_bytes()).unwrap_or(InstId(0))
}

/// Every receive field of `rx` copied unchanged (contract 3).
pub(crate) fn hdr(line: Line, inst: InstId, rx: &RxMeta, ex_ts_ns: i64, ex_send_ns: i64) -> MarketHdr {
    MarketHdr {
        venue: line.venue(),
        conn: rx.conn,
        _pad: 0,
        inst,
        ex_ts_ns,
        ex_send_ns,
        hw_rx_ns: rx.hw_rx_ns,
        rx_tsc: rx.rx_tsc,
        sw_rx_ns: rx.sw_rx_ns,
        rx_empty_gap_ns: rx.rx_empty_gap_ns,
        _rsv: 0,
    }
}

// ---------------------------------------------------------------- peek

#[derive(Deserialize)]
struct SeqOnly {
    #[serde(default)]
    seq: UInt,
    #[serde(default)]
    pseq: UInt,
}

#[derive(Deserialize)]
struct PeekBook<'a> {
    #[serde(borrow)]
    arg: Arg<'a>,
    data: One<SeqOnly>,
}

#[derive(Deserialize)]
struct TsOnly {
    #[serde(default)]
    ts: UInt,
}

#[derive(Deserialize)]
struct PeekTicker<'a> {
    #[serde(borrow)]
    arg: Arg<'a>,
    data: One<TsOnly>,
    #[serde(default)]
    ts: UInt,
}

/// Largest trade id of a `publicTrade` push (`i` / V2 `tradeId`), without parsing prices.
struct MaxTradeId(Result<u64, DecodeErr>);

#[derive(Deserialize)]
struct TradeIdOnly {
    #[serde(alias = "tradeId")]
    i: UInt,
}

impl<'de> Deserialize<'de> for MaxTradeId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = MaxTradeId;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("trades")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut s: A) -> Result<MaxTradeId, A::Error> {
                let mut best: Result<u64, DecodeErr> = Ok(0);
                while let Some(t) = s.next_element::<TradeIdOnly>()? {
                    best = match (best, t.i.get()) {
                        (Ok(b), Ok(v)) => Ok(b.max(v)),
                        (Err(e), _) | (_, Err(e)) => Err(e),
                    };
                }
                Ok(MaxTradeId(best))
            }
        }
        d.deserialize_seq(V)
    }
}

#[derive(Deserialize)]
struct PeekTrade<'a> {
    #[serde(borrow)]
    arg: Arg<'a>,
    data: MaxTradeId,
}

fn peek_of(inst: InstId, stream: StreamKind, last: u64) -> Peek {
    Peek {
        inst,
        stream,
        ids: SeqIds {
            prev_id: 0,
            last_id: last,
        },
        raw: RawSeq::default(),
    }
}

pub(crate) fn peek(
    line: Line,
    rd: &RefData,
    _ctx: &FeedCtx,
    c: MdClass,
    p: &[u8],
) -> Result<Peek, DecodeErr> {
    match c {
        MdClass::Books1 | MdClass::BooksTopN => {
            let m: PeekBook = from(p)?;
            let stream = if c == MdClass::Books1 {
                StreamKind::Bbo
            } else {
                StreamKind::DepthTopN
            };
            Ok(peek_of(
                inst_of(line, rd, m.arg.symbol),
                stream,
                m.data.0.seq.get()?,
            ))
        }
        MdClass::BooksIncr => {
            let m: PeekBook = from(p)?;
            let seq = m.data.0.seq.get()?;
            let pseq = m.data.0.pseq.opt()?.unwrap_or(0);
            Ok(Peek {
                inst: inst_of(line, rd, m.arg.symbol),
                stream: StreamKind::DepthIncr,
                ids: SeqIds {
                    prev_id: pseq,
                    last_id: seq,
                },
                raw: RawSeq {
                    first: pseq,
                    last: seq,
                    prev: pseq,
                    has_prev: true,
                    _pad: [0; 7],
                },
            })
        }
        MdClass::Trade => {
            let m: PeekTrade = from(p)?;
            Ok(peek_of(
                inst_of(line, rd, m.arg.symbol),
                StreamKind::Trade,
                m.data.0?,
            ))
        }
        MdClass::Ticker if line == Line::Fut => {
            let m: PeekTicker = from(p)?;
            let ms = match m.data.0.ts.opt()? {
                Some(v) => v,
                None => m.ts.get()?,
            };
            Ok(peek_of(
                inst_of(line, rd, m.arg.symbol),
                StreamKind::Mark,
                ms_ns(ms)? as u64,
            ))
        }
        _ => Err(DecodeErr::UnknownClass),
    }
}

// ---------------------------------------------------------------- decode

/// One book message (`a`/`b` UTA, `asks`/`bids` Classic V2); levels kept as raw text.
#[derive(Deserialize)]
struct Book<'a> {
    #[serde(alias = "asks", borrow)]
    a: Raw<'a>,
    #[serde(alias = "bids", borrow)]
    b: Raw<'a>,
    seq: UInt,
    #[serde(default)]
    pseq: UInt,
    #[serde(default)]
    ts: UInt,
}

#[derive(Deserialize)]
struct BookMsg<'a> {
    #[serde(borrow)]
    arg: Arg<'a>,
    #[serde(default, borrow)]
    action: &'a str,
    #[serde(borrow)]
    data: One<Book<'a>>,
    #[serde(default)]
    ts: UInt,
}

fn opt_ms_ns(v: UInt) -> Result<i64, DecodeErr> {
    match v.opt()? {
        Some(ms) => ms_ns(ms),
        None => Ok(0),
    }
}

const ZERO_LEVEL: Level = Level {
    px: Px(0),
    qty: Qty(0),
};

/// One trade (`i`/`p`/`v`/`S`/`T` UTA, `tradeId`/`price`/`size`/`side`/`ts` Classic V2).
#[derive(Deserialize)]
struct TradeRow<'a> {
    #[serde(alias = "tradeId")]
    i: UInt,
    #[serde(alias = "price", borrow)]
    p: &'a str,
    #[serde(alias = "size", borrow)]
    v: &'a str,
    #[serde(rename = "S", alias = "side", borrow)]
    side: &'a str,
    #[serde(rename = "T", alias = "ts", default)]
    t: UInt,
}

#[derive(Deserialize)]
struct TradeMsg<'a> {
    #[serde(borrow)]
    arg: Arg<'a>,
    #[serde(default, borrow)]
    action: &'a str,
    #[serde(borrow)]
    data: Arr<TradeRow<'a>, TRADES_MAX>,
    #[serde(default)]
    ts: UInt,
}

/// Perpetual `ticker` fields used here.
#[derive(Deserialize)]
struct TickerRow<'a> {
    #[serde(rename = "markPrice", borrow)]
    mark: &'a str,
    #[serde(rename = "indexPrice", borrow)]
    index: &'a str,
    #[serde(rename = "fundingRate", borrow)]
    rate: &'a str,
    #[serde(rename = "nextFundingTime")]
    next: UInt,
    #[serde(default)]
    ts: UInt,
}

#[derive(Deserialize)]
struct TickerMsg<'a> {
    #[serde(borrow)]
    arg: Arg<'a>,
    #[serde(borrow)]
    data: One<TickerRow<'a>>,
    #[serde(default)]
    ts: UInt,
}

fn taker(s: &str) -> Result<Side, DecodeErr> {
    match s {
        "buy" | "Buy" | "BUY" => Ok(Side::Buy),
        "sell" | "Sell" | "SELL" => Ok(Side::Sell),
        _ => Err(DecodeErr::Shape),
    }
}

pub(crate) fn decode_md(
    line: Line,
    rd: &RefData,
    ctx: &mut FeedCtx,
    c: MdClass,
    p: &[u8],
    rx: &RxMeta,
    out: &mut impl MdSink,
) -> Result<MdOutcome, DecodeErr> {
    match c {
        MdClass::Books1 => {
            let m: BookMsg = from(p)?;
            let d = &m.data.0;
            let seq = d.seq.get()?;
            let (ex, send) = (opt_ms_ns(d.ts)?, opt_ms_ns(m.ts)?);
            let inst = inst_of(line, rd, m.arg.symbol);
            if inst.0 == 0 {
                return Ok(MdOutcome { events: 0, inst });
            }
            let row = rd.inst(inst);
            let (pe, qe) = (row.px_wire_exp, row.qty_wire_exp);
            let side = |raw: &[u8]| -> Result<(Px, Qty), DecodeErr> {
                let mut l = [ZERO_LEVEL; 1];
                levels(raw, pe, qe, false, true, &mut l)?;
                Ok((l[0].px, l[0].qty))
            };
            let (bid_px, bid_qty) = side(d.b.text())?;
            let (ask_px, ask_qty) = side(d.a.text())?;
            out.bbo(&Bbo {
                h: hdr(line, inst, rx, ex, send),
                bid_px,
                bid_qty,
                ask_px,
                ask_qty,
                update_id: seq,
            });
            Ok(MdOutcome { events: 1, inst })
        }
        MdClass::BooksTopN => {
            let m: BookMsg = from(p)?;
            let d = &m.data.0;
            let seq = d.seq.get()?;
            let (ex, send) = (opt_ms_ns(d.ts)?, opt_ms_ns(m.ts)?);
            let inst = inst_of(line, rd, m.arg.symbol);
            if inst.0 == 0 {
                return Ok(MdOutcome { events: 0, inst });
            }
            let row = rd.inst(inst);
            let mut bids = [ZERO_LEVEL; TOPN_MAX];
            let mut asks = [ZERO_LEVEL; TOPN_MAX];
            let nb = levels(
                d.b.text(),
                row.px_wire_exp,
                row.qty_wire_exp,
                true,
                true,
                &mut bids,
            )?;
            let na = levels(
                d.a.text(),
                row.px_wire_exp,
                row.qty_wire_exp,
                true,
                true,
                &mut asks,
            )?;
            snapshot(
                line,
                inst,
                rx,
                (ex, send),
                seq,
                L2SnapHdr::TOP_N,
                &bids,
                nb,
                &asks,
                na,
                out,
            )?;
            Ok(MdOutcome { events: 1, inst })
        }
        MdClass::BooksIncr => {
            let m: BookMsg = from(p)?;
            let d = &m.data.0;
            let seq = d.seq.get()?;
            let pseq = d.pseq.opt()?.unwrap_or(0);
            let (ex, send) = (opt_ms_ns(d.ts)?, opt_ms_ns(m.ts)?);
            let inst = inst_of(line, rd, m.arg.symbol);
            if inst.0 == 0 {
                return Ok(MdOutcome { events: 0, inst });
            }
            let row = rd.inst(inst);
            let mut bids = [ZERO_LEVEL; BOOKS_MAX];
            let mut asks = [ZERO_LEVEL; BOOKS_MAX];
            // A snapshot (or the reset push `pseq = 0`) replaces the book; zero sizes never appear
            // in it. An update keeps zero sizes (= delete the level).
            let full = m.action == "snapshot" || pseq == 0;
            let nb = levels(
                d.b.text(),
                row.px_wire_exp,
                row.qty_wire_exp,
                full,
                false,
                &mut bids,
            )?;
            let na = levels(
                d.a.text(),
                row.px_wire_exp,
                row.qty_wire_exp,
                full,
                false,
                &mut asks,
            )?;
            if full {
                snapshot(
                    line,
                    inst,
                    rx,
                    (ex, send),
                    seq,
                    L2SnapHdr::EXCHANGE,
                    &bids,
                    nb,
                    &asks,
                    na,
                    out,
                )?;
            } else {
                out.l2_begin(&L2BatchHdr {
                    h: hdr(line, inst, rx, ex, send),
                    prev_id: pseq,
                    last_id: seq,
                    n_bids: u16::try_from(nb).map_err(|_| DecodeErr::Shape)?,
                    n_asks: u16::try_from(na).map_err(|_| DecodeErr::Shape)?,
                    bflags: 0,
                    _pad: 0,
                });
                for (side, l, n) in [(Side::Buy, &bids, nb), (Side::Sell, &asks, na)] {
                    for x in l.get(..n).unwrap_or(&[]) {
                        out.l2_level(side, x.px, x.qty);
                    }
                }
                out.l2_end();
            }
            Ok(MdOutcome { events: 1, inst })
        }
        MdClass::Trade => {
            let m: TradeMsg = from(p)?;
            let inst = inst_of(line, rd, m.arg.symbol);
            // The first push after subscribing is a history batch (`action = snapshot`): dropped.
            if inst.0 == 0 || m.action == "snapshot" {
                return Ok(MdOutcome { events: 0, inst });
            }
            if m.data.over {
                return Err(DecodeErr::Shape);
            }
            let send = opt_ms_ns(m.ts)?;
            // Validate every row first: a failing message writes no event.
            let mut n: u16 = 0;
            let row = rd.inst(inst);
            let mut parsed = [None; TRADES_MAX];
            for (slot, t) in parsed.iter_mut().zip(m.data.iter()) {
                let id = t.i.get()?;
                let px = Px(fixed(t.p, row.px_wire_exp)?);
                let qty = Qty(fixed(t.v, row.qty_wire_exp)?);
                let side = taker(t.side)?;
                let ex = opt_ms_ns(t.t)?;
                *slot = Some((id, px, qty, side, ex));
            }
            for (id, px, qty, side, ex) in parsed.iter().map_while(|x| *x) {
                out.trade(&Trade {
                    h: hdr(line, inst, rx, ex, send),
                    px,
                    qty,
                    trade_id: id,
                    taker_side: side,
                    _pad: [0; 7],
                });
                n += 1;
            }
            Ok(MdOutcome { events: n, inst })
        }
        MdClass::Ticker if line == Line::Fut => {
            let m: TickerMsg = from(p)?;
            let d = &m.data.0;
            let send = opt_ms_ns(m.ts)?;
            let ex = match d.ts.opt()? {
                Some(ms) => ms_ns(ms)?,
                None => send,
            };
            let next_ms = d.next.get()?;
            let next_ns = ms_ns(next_ms)?;
            let inst = inst_of(line, rd, m.arg.symbol);
            if inst.0 == 0 {
                return Ok(MdOutcome { events: 0, inst });
            }
            let row = rd.inst(inst);
            let mark = Px(fixed(d.mark, row.px_wire_exp)?);
            let index = Px(fixed(d.index, row.px_wire_exp)?);
            let rate = Rate(fixed(d.rate, -9)?);
            let interval_s = ctx.funding_interval(inst, next_ms);
            let h = hdr(line, inst, rx, ex, send);
            out.mark(&MarkPrice { h, px: mark });
            out.index(&IndexPrice { h, px: index });
            out.funding(&Funding {
                h,
                rate,
                next_funding_ns: next_ns,
                interval_s,
                _pad: 0,
            });
            Ok(MdOutcome { events: 3, inst })
        }
        _ => Err(DecodeErr::UnknownClass),
    }
}

#[allow(clippy::too_many_arguments)]
fn snapshot(
    line: Line,
    inst: InstId,
    rx: &RxMeta,
    (ex, send): (i64, i64),
    seq: u64,
    origin: u8,
    bids: &[Level],
    nb: usize,
    asks: &[Level],
    na: usize,
    out: &mut impl MdSink,
) -> Result<(), DecodeErr> {
    let bids = bids.get(..nb).ok_or(DecodeErr::Shape)?;
    let asks = asks.get(..na).ok_or(DecodeErr::Shape)?;
    out.l2_snapshot(&L2SnapshotPart {
        hdr: L2SnapHdr {
            h: hdr(line, inst, rx, ex, send),
            snap_id: seq,
            origin,
            _pad: [0; 3],
            n_bids: u16::try_from(nb).map_err(|_| DecodeErr::Shape)?,
            n_asks: u16::try_from(na).map_err(|_| DecodeErr::Shape)?,
        },
        bids,
        asks,
        part: 0,
        parts: 1,
    });
    Ok(())
}
