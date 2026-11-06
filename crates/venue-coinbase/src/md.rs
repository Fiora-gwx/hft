//! Market data (README "行情频道 → 事件"): dispatch from the first 64 B, `peek` ids, sequence
//! rules, control hints and decoding of `ticker` (→ `Bbo`) and `match` (→ `Trade`).

use hft_event::{Bbo, MarketHdr, Trade};
use hft_refdata::RefData;
use hft_types::{FxErr, InstId, Px, Qty, Side, parse_exact};
use hft_venue_proto::json as wire;
use hft_venue_proto::{
    ControlHint, DecodeErr, MdOutcome, MdSink, MsgOp, Peek, RawSeq, RouteId, RxMeta, SeqIds, SeqRule,
    StreamKind,
};
use serde::Deserialize;

use crate::VENUE;
use crate::time::iso8601_ns;

/// Market-data dispatch classes (README "使用本组件").
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum MdClass {
    /// `ticker`: best bid/ask after each match → `Bbo` (`update_id` = `sequence`).
    Ticker,
    /// `match` → `Trade`.
    Match,
    /// `last_match`: the product's latest trade, pushed once on subscribe; sets the trade
    /// watermark, produces no event (README "订阅").
    LastMatch,
    /// `heartbeat`: one per product per second → `ControlHint::Pong`.
    Heartbeat,
    /// `subscriptions`: answer to every (un)subscribe frame.
    Subscriptions,
    /// `error`: a refused frame (followed by an empty `subscriptions`).
    Error,
    Unknown,
}

const HEAD: usize = 64;

/// `{"type":"<name>"` within the first 64 B.
fn type_name(head: &[u8]) -> Option<&[u8]> {
    let h = head.get(..head.len().min(HEAD))?;
    let rest = h.strip_prefix(b"{\"type\":\"")?;
    let end = rest.iter().position(|&c| c == b'"')?;
    rest.get(..end)
}

pub(crate) fn classify(_route: RouteId, op: MsgOp, head: &[u8]) -> MdClass {
    if op != MsgOp::Text {
        return MdClass::Unknown;
    }
    match type_name(head) {
        Some(b"ticker") => MdClass::Ticker,
        Some(b"match") => MdClass::Match,
        Some(b"last_match") => MdClass::LastMatch,
        Some(b"heartbeat") => MdClass::Heartbeat,
        Some(b"subscriptions") => MdClass::Subscriptions,
        Some(b"error") => MdClass::Error,
        _ => MdClass::Unknown,
    }
}

pub(crate) fn seq_rule(c: MdClass) -> SeqRule {
    match c {
        MdClass::Ticker | MdClass::Match | MdClass::LastMatch => SeqRule::Monotonic,
        _ => SeqRule::None,
    }
}

/// `"channels":[]` (after optional whitespace): the answer lists no subscription.
fn channels_empty(p: &[u8]) -> bool {
    const KEY: &[u8] = b"\"channels\":";
    let Some(at) = p.windows(KEY.len()).position(|w| w == KEY) else {
        return false;
    };
    let rest = p.get(at + KEY.len()..).unwrap_or(&[]);
    let mut it = rest.iter().copied().filter(|b| !b.is_ascii_whitespace());
    it.next() == Some(b'[') && it.next() == Some(b']')
}

/// Subscription answers and heartbeats (README "订阅", "心跳与 BBO 探测").
pub(crate) fn control(c: MdClass, payload: &[u8]) -> Option<ControlHint> {
    match c {
        // A non-empty list answers our frame (the `heartbeat` channel is never unsubscribed);
        // an empty one only trails an `error`, which already answered it.
        MdClass::Subscriptions if !channels_empty(payload) => Some(ControlHint::SubAck { ok: true, code: 0 }),
        MdClass::Error => Some(ControlHint::SubAck { ok: false, code: 1 }),
        MdClass::Heartbeat => Some(ControlHint::Pong),
        _ => None,
    }
}

fn inst_of(rd: &RefData, product: &str) -> InstId {
    rd.symbols(VENUE).get(product.as_bytes()).unwrap_or(InstId(0))
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

/// Parses `p` into `T`; the input ending early → `Truncated`, anything else → `Shape`.
fn from<'a, T: Deserialize<'a>>(p: &'a [u8]) -> Result<T, DecodeErr> {
    let text = core::str::from_utf8(p).map_err(|_| DecodeErr::Shape)?;
    sonic_rs::from_str::<T>(text).map_err(|e| if e.is_eof() { DecodeErr::Truncated } else { DecodeErr::Shape })
}

fn fixed(s: &str, exp: i8) -> Result<i64, DecodeErr> {
    parse_exact(s.as_bytes(), exp).map_err(|e| {
        if e.0 & FxErr::OVERFLOW.0 != 0 {
            DecodeErr::Overflow
        } else {
            DecodeErr::Number
        }
    })
}

/// `(product_id, <id>)` of a frame, fast path first.
fn ids<'a>(p: &'a [u8], id_key: &[u8]) -> Result<(&'a str, u64), DecodeErr> {
    let fast = || {
        let [product, id] = wire::project(p, |key| {
            if key == b"product_id" {
                Some(0)
            } else if key == id_key {
                Some(1)
            } else {
                None
            }
        })?;
        Some((wire::string(product)?, wire::uint(id)?))
    };
    if let Some(v) = fast() {
        return Ok(v);
    }
    #[derive(Deserialize)]
    struct Seq<'a> {
        #[serde(borrow)]
        product_id: &'a str,
        sequence: u64,
    }
    #[derive(Deserialize)]
    struct Tid<'a> {
        #[serde(borrow)]
        product_id: &'a str,
        trade_id: u64,
    }
    if id_key == b"sequence" {
        let m: Seq = from(p)?;
        Ok((m.product_id, m.sequence))
    } else {
        let m: Tid = from(p)?;
        Ok((m.product_id, m.trade_id))
    }
}

pub(crate) fn peek(rd: &RefData, c: MdClass, p: &[u8]) -> Result<Peek, DecodeErr> {
    match c {
        MdClass::Ticker => {
            let (product, seq) = ids(p, b"sequence")?;
            Ok(peek_of(inst_of(rd, product), StreamKind::Bbo, seq))
        }
        MdClass::Match | MdClass::LastMatch => {
            let (product, tid) = ids(p, b"trade_id")?;
            Ok(peek_of(inst_of(rd, product), StreamKind::Trade, tid))
        }
        _ => Err(DecodeErr::UnknownClass),
    }
}

/// The fields of `ticker` this crate uses.
#[derive(Deserialize)]
struct Ticker<'a> {
    sequence: u64,
    #[serde(borrow)]
    product_id: &'a str,
    #[serde(borrow)]
    best_bid: &'a str,
    #[serde(borrow)]
    best_bid_size: &'a str,
    #[serde(borrow)]
    best_ask: &'a str,
    #[serde(borrow)]
    best_ask_size: &'a str,
    #[serde(borrow)]
    time: &'a str,
}

/// Wire order of a feed `ticker` (2026-11-06 capture); any variation takes the checked decoder.
const TICKER_KEYS: [&[u8]; 17] = [
    b"\"type\":",
    b"\"sequence\":",
    b"\"product_id\":",
    b"\"price\":",
    b"\"open_24h\":",
    b"\"volume_24h\":",
    b"\"low_24h\":",
    b"\"high_24h\":",
    b"\"volume_30d\":",
    b"\"best_bid\":",
    b"\"best_bid_size\":",
    b"\"best_ask\":",
    b"\"best_ask_size\":",
    b"\"side\":",
    b"\"time\":",
    b"\"trade_id\":",
    b"\"last_size\":",
];

fn ticker(p: &[u8]) -> Result<Ticker<'_>, DecodeErr> {
    let fast = || {
        let f = wire::ordered(p, &TICKER_KEYS)?;
        Some(Ticker {
            sequence: wire::uint(f[1])?,
            product_id: wire::string(f[2])?,
            best_bid: wire::string(f[9])?,
            best_bid_size: wire::string(f[10])?,
            best_ask: wire::string(f[11])?,
            best_ask_size: wire::string(f[12])?,
            time: wire::string(f[14])?,
        })
    };
    fast().map_or_else(|| from(p), Ok)
}

/// The fields of `match` / `last_match`.
#[derive(Deserialize)]
struct Match<'a> {
    trade_id: u64,
    #[serde(borrow)]
    side: &'a str,
    #[serde(borrow)]
    size: &'a str,
    #[serde(borrow)]
    price: &'a str,
    #[serde(borrow)]
    product_id: &'a str,
    #[serde(borrow)]
    time: &'a str,
}

const MATCH_KEYS: [&[u8]; 10] = [
    b"\"type\":",
    b"\"trade_id\":",
    b"\"maker_order_id\":",
    b"\"taker_order_id\":",
    b"\"side\":",
    b"\"size\":",
    b"\"price\":",
    b"\"product_id\":",
    b"\"sequence\":",
    b"\"time\":",
];

fn trade(p: &[u8]) -> Result<Match<'_>, DecodeErr> {
    let fast = || {
        let f = wire::ordered(p, &MATCH_KEYS)?;
        Some(Match {
            trade_id: wire::uint(f[1])?,
            side: wire::string(f[4])?,
            size: wire::string(f[5])?,
            price: wire::string(f[6])?,
            product_id: wire::string(f[7])?,
            time: wire::string(f[9])?,
        })
    };
    fast().map_or_else(|| from(p), Ok)
}

/// Every receive field of `rx` copied unchanged (contract 3).
fn hdr(inst: InstId, ex_ts_ns: i64, rx: &RxMeta) -> MarketHdr {
    MarketHdr {
        venue: VENUE,
        conn: rx.conn,
        _pad: 0,
        inst,
        ex_ts_ns,
        ex_send_ns: 0,
        hw_rx_ns: rx.hw_rx_ns,
        rx_tsc: rx.rx_tsc,
        sw_rx_ns: rx.sw_rx_ns,
        rx_empty_gap_ns: rx.rx_empty_gap_ns,
        _rsv: 0,
    }
}

pub(crate) fn decode_md(
    rd: &RefData,
    c: MdClass,
    p: &[u8],
    rx: &RxMeta,
    out: &mut impl MdSink,
) -> Result<MdOutcome, DecodeErr> {
    match c {
        MdClass::Ticker => {
            let m = ticker(p)?;
            let inst = inst_of(rd, m.product_id);
            if inst.0 == 0 {
                return Ok(MdOutcome { events: 0, inst });
            }
            let row = rd.inst(inst);
            let (pe, qe) = (row.px_wire_exp, row.qty_wire_exp);
            let e = Bbo {
                h: hdr(inst, iso8601_ns(m.time.as_bytes())?, rx),
                bid_px: Px(fixed(m.best_bid, pe)?),
                bid_qty: Qty(fixed(m.best_bid_size, qe)?),
                ask_px: Px(fixed(m.best_ask, pe)?),
                ask_qty: Qty(fixed(m.best_ask_size, qe)?),
                update_id: m.sequence,
            };
            out.bbo(&e);
            Ok(MdOutcome { events: 1, inst })
        }
        MdClass::Match => {
            let m = trade(p)?;
            let inst = inst_of(rd, m.product_id);
            if inst.0 == 0 {
                return Ok(MdOutcome { events: 0, inst });
            }
            // `side` is the maker's side: a `sell` maker means a buying taker (README).
            let taker_side = match m.side {
                "sell" => Side::Buy,
                "buy" => Side::Sell,
                _ => return Err(DecodeErr::Shape),
            };
            let row = rd.inst(inst);
            let e = Trade {
                h: hdr(inst, iso8601_ns(m.time.as_bytes())?, rx),
                px: Px(fixed(m.price, row.px_wire_exp)?),
                qty: Qty(fixed(m.size, row.qty_wire_exp)?),
                trade_id: m.trade_id,
                taker_side,
                _pad: [0; 7],
            };
            out.trade(&e);
            Ok(MdOutcome { events: 1, inst })
        }
        MdClass::LastMatch => {
            // Validated, not forwarded: it repeats a trade from before the subscription.
            let m = trade(p)?;
            Ok(MdOutcome {
                events: 0,
                inst: inst_of(rd, m.product_id),
            })
        }
        _ => Err(DecodeErr::UnknownClass),
    }
}
