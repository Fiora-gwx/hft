//! Market data: dispatch from the first 64 B (README "1. 分派"), `peek` ids, sequence rules, control
//! hints and decoding (M1.5: `bookTicker` of both lines).

use hft_event::{Bbo, MarketHdr};
use hft_refdata::RefData;
use hft_types::{InstId, Px, Qty};
use hft_venue_proto::{
    ControlHint, DecodeErr, MdOutcome, MdSink, MsgOp, Peek, RawSeq, ResyncMethod, RouteId, RxMeta, SeqIds,
    SeqRule, StreamKind,
};
use serde::Deserialize;
use serde::de::IgnoredAny;

use crate::json::{ErrBody, fixed, from, time_ns};
use crate::session::{ROUTE_PUBLIC_DEPTH, ROUTE_PUBLIC_TOPN, ROUTE_SPOT_TOPN};
use crate::{FeedCtx, Line, spot, usdm};

/// Bytes `classify` may look at.
const HEAD: usize = 64;

/// Both lines' classes, unified for the shared code.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum Class {
    BookTicker,
    DepthTopN,
    DepthUpdate,
    AggTrade,
    MarkPrice,
    ForceOrder,
    SpotTrade,
    SubAck,
    ServerShutdown,
    Unknown,
}

impl From<usdm::MdClass> for Class {
    fn from(c: usdm::MdClass) -> Class {
        use usdm::MdClass as M;
        match c {
            M::BookTicker => Class::BookTicker,
            M::DepthTopN => Class::DepthTopN,
            M::DepthUpdate => Class::DepthUpdate,
            M::AggTrade => Class::AggTrade,
            M::MarkPrice => Class::MarkPrice,
            M::ForceOrder => Class::ForceOrder,
            M::SubAck => Class::SubAck,
            M::ServerShutdown => Class::ServerShutdown,
            M::Unknown => Class::Unknown,
        }
    }
}

impl From<spot::MdClass> for Class {
    fn from(c: spot::MdClass) -> Class {
        use spot::MdClass as M;
        match c {
            M::JsonBookTicker => Class::BookTicker,
            M::JsonDepthTopN => Class::DepthTopN,
            M::JsonDepth => Class::DepthUpdate,
            M::JsonTrade => Class::SpotTrade,
            M::SubAck => Class::SubAck,
            M::Unknown => Class::Unknown,
        }
    }
}

/// `classify` of each line's class type.
pub(crate) trait Classify: Sized {
    fn classify(route: RouteId, op: MsgOp, head: &[u8]) -> Self;
}

/// Event name of `{"e":"<name>"` within the first 64 B.
fn event_name(head: &[u8]) -> Option<&[u8]> {
    let h = head.get(..head.len().min(HEAD))?;
    let rest = h.strip_prefix(b"{\"e\":\"")?;
    let end = rest.iter().position(|&c| c == b'"')?;
    rest.get(..end)
}

fn is_sub_answer(h: &[u8]) -> bool {
    h.starts_with(b"{\"result\":") || h.starts_with(b"{\"error\":")
}

impl Classify for usdm::MdClass {
    fn classify(route: RouteId, op: MsgOp, head: &[u8]) -> usdm::MdClass {
        use usdm::MdClass as M;
        if op != MsgOp::Text {
            return M::Unknown;
        }
        let h = head.get(..head.len().min(HEAD)).unwrap_or(&[]);
        if is_sub_answer(h) {
            return M::SubAck;
        }
        match event_name(h) {
            Some(b"bookTicker") => M::BookTicker,
            Some(b"depthUpdate") if route == ROUTE_PUBLIC_TOPN => M::DepthTopN,
            Some(b"depthUpdate") if route == ROUTE_PUBLIC_DEPTH => M::DepthUpdate,
            Some(b"aggTrade") => M::AggTrade,
            Some(b"markPriceUpdate") => M::MarkPrice,
            Some(b"forceOrder") => M::ForceOrder,
            Some(b"serverShutdown") => M::ServerShutdown,
            _ => M::Unknown,
        }
    }
}

/// `<s>@depth<N>@100ms` stream name of a combined-stream head; returns the lowercase symbol.
fn combined_depth(h: &[u8]) -> Option<&[u8]> {
    let rest = h.strip_prefix(b"{\"stream\":\"")?;
    let end = rest.iter().position(|&c| c == b'"')?;
    let name = rest.get(..end)?;
    let after = rest.get(end..)?;
    if !after.starts_with(b"\",\"data\":") {
        return None;
    }
    combined_depth_name(name)
}

impl Classify for spot::MdClass {
    fn classify(route: RouteId, op: MsgOp, head: &[u8]) -> spot::MdClass {
        use spot::MdClass as M;
        if op != MsgOp::Text {
            return M::Unknown;
        }
        let h = head.get(..head.len().min(HEAD)).unwrap_or(&[]);
        if is_sub_answer(h) {
            return M::SubAck;
        }
        if h.starts_with(b"{\"u\":") {
            return M::JsonBookTicker;
        }
        if route == ROUTE_SPOT_TOPN && combined_depth(h).is_some() {
            return M::JsonDepthTopN;
        }
        match event_name(h) {
            Some(b"trade") => M::JsonTrade,
            Some(b"depthUpdate") => M::JsonDepth,
            _ => M::Unknown,
        }
    }
}

pub(crate) fn seq_rule(c: Class) -> SeqRule {
    match c {
        Class::BookTicker | Class::AggTrade | Class::SpotTrade => SeqRule::Monotonic,
        Class::DepthTopN => SeqRule::Snapshot,
        Class::DepthUpdate => SeqRule::Chain,
        Class::MarkPrice => SeqRule::ByTime,
        Class::ForceOrder | Class::SubAck | Class::ServerShutdown | Class::Unknown => SeqRule::None,
    }
}

pub(crate) fn resync_method(c: Class) -> ResyncMethod {
    match c {
        Class::DepthUpdate => ResyncMethod::RestSnapshot,
        _ => ResyncMethod::None,
    }
}

pub(crate) fn control(c: Class, payload: &[u8]) -> Option<ControlHint> {
    #[derive(Deserialize)]
    struct Ans {
        #[serde(default)]
        error: Option<ErrBody>,
        #[serde(default)]
        #[allow(dead_code)]
        result: Option<IgnoredAny>,
    }
    match c {
        Class::SubAck => {
            let a: Ans = from(payload).ok()?;
            Some(match a.error {
                None => ControlHint::SubAck { ok: true, code: 0 },
                Some(e) => ControlHint::SubAck {
                    ok: false,
                    code: e.code as u32,
                },
            })
        }
        Class::ServerShutdown => Some(ControlHint::ServerShutdown),
        _ => None,
    }
}

/// Symbol → inst of this line (0 = not in refdata).
pub(crate) fn inst_of(line: Line, rd: &RefData, sym: &[u8]) -> InstId {
    rd.symbols(line.venue()).get(sym).unwrap_or(InstId(0))
}

fn ids(last: u64) -> SeqIds {
    SeqIds {
        prev_id: 0,
        last_id: last,
    }
}

fn peek_of(inst: InstId, stream: StreamKind, last: u64) -> Peek {
    Peek {
        inst,
        stream,
        ids: ids(last),
        raw: RawSeq::default(),
    }
}

pub(crate) fn peek(line: Line, rd: &RefData, ctx: &FeedCtx, c: Class, p: &[u8]) -> Result<Peek, DecodeErr> {
    #[derive(Deserialize)]
    struct SymU<'a> {
        #[serde(borrow)]
        s: &'a str,
        u: u64,
    }
    #[derive(Deserialize)]
    struct Agg<'a> {
        #[serde(borrow)]
        s: &'a str,
        a: u64,
    }
    #[derive(Deserialize)]
    struct SpotTrade<'a> {
        #[serde(borrow)]
        s: &'a str,
        t: u64,
    }
    #[derive(Deserialize)]
    struct Mark<'a> {
        #[serde(borrow)]
        s: &'a str,
        #[serde(rename = "E")]
        e: i64,
    }
    #[derive(Deserialize)]
    struct ForceO<'a> {
        #[serde(borrow)]
        s: &'a str,
    }
    #[derive(Deserialize)]
    struct Force<'a> {
        #[serde(borrow)]
        o: ForceO<'a>,
    }
    #[derive(Deserialize)]
    struct Diff<'a> {
        #[serde(borrow)]
        s: &'a str,
        #[serde(rename = "U")]
        first: u64,
        u: u64,
        #[serde(default)]
        pu: Option<u64>,
    }
    #[derive(Deserialize)]
    struct LastUpd {
        #[serde(rename = "lastUpdateId")]
        last: u64,
    }
    #[derive(Deserialize)]
    struct Combined<'a> {
        #[serde(borrow)]
        stream: &'a str,
        data: LastUpd,
    }
    match c {
        Class::BookTicker => {
            let m: SymU = from(p)?;
            Ok(peek_of(inst_of(line, rd, m.s.as_bytes()), StreamKind::Bbo, m.u))
        }
        Class::DepthTopN if line == Line::Usdm => {
            let m: SymU = from(p)?;
            Ok(peek_of(
                inst_of(line, rd, m.s.as_bytes()),
                StreamKind::DepthTopN,
                m.u,
            ))
        }
        Class::DepthTopN => {
            let m: Combined = from(p)?;
            let lower = combined_depth_name(m.stream.as_bytes()).ok_or(DecodeErr::Shape)?;
            let mut up = [0u8; 32];
            let n = lower.len().min(up.len());
            for (d, s) in up.iter_mut().zip(lower) {
                *d = s.to_ascii_uppercase();
            }
            let inst = inst_of(line, rd, up.get(..n).unwrap_or(&[]));
            Ok(peek_of(inst, StreamKind::DepthTopN, m.data.last))
        }
        Class::AggTrade => {
            let m: Agg = from(p)?;
            Ok(peek_of(inst_of(line, rd, m.s.as_bytes()), StreamKind::Trade, m.a))
        }
        Class::SpotTrade => {
            let m: SpotTrade = from(p)?;
            Ok(peek_of(inst_of(line, rd, m.s.as_bytes()), StreamKind::Trade, m.t))
        }
        Class::MarkPrice => {
            let m: Mark = from(p)?;
            let ns = time_ns(m.e, ctx.time_unit.ns())?;
            Ok(peek_of(
                inst_of(line, rd, m.s.as_bytes()),
                StreamKind::Mark,
                ns as u64,
            ))
        }
        Class::ForceOrder => {
            let m: Force = from(p)?;
            Ok(peek_of(
                inst_of(line, rd, m.o.s.as_bytes()),
                StreamKind::Liquidation,
                0,
            ))
        }
        Class::DepthUpdate => {
            let m: Diff = from(p)?;
            let (prev, has_prev) = match (line, m.pu) {
                (Line::Usdm, Some(pu)) => (pu, true),
                _ => (m.first.saturating_sub(1), false),
            };
            Ok(Peek {
                inst: inst_of(line, rd, m.s.as_bytes()),
                stream: StreamKind::DepthIncr,
                ids: SeqIds {
                    prev_id: prev,
                    last_id: m.u,
                },
                raw: RawSeq {
                    first: m.first,
                    last: m.u,
                    prev: m.pu.unwrap_or(0),
                    has_prev,
                    _pad: [0; 7],
                },
            })
        }
        Class::SubAck | Class::ServerShutdown | Class::Unknown => Err(DecodeErr::UnknownClass),
    }
}

/// Lowercase symbol of a full combined-stream name `<s>@depth<N>@100ms`.
fn combined_depth_name(name: &[u8]) -> Option<&[u8]> {
    let at = name.iter().position(|&c| c == b'@')?;
    let tail = name.get(at..)?.strip_prefix(b"@depth")?;
    let digits = tail.iter().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 || tail.get(digits..) != Some(&b"@100ms"[..]) {
        return None;
    }
    name.get(..at)
}

/// `bookTicker` of USDT-M (with `T`/`E`).
#[derive(Deserialize)]
struct UsdmTicker<'a> {
    u: u64,
    #[serde(borrow)]
    s: &'a str,
    #[serde(borrow)]
    b: &'a str,
    #[serde(rename = "B", borrow)]
    bq: &'a str,
    #[serde(borrow)]
    a: &'a str,
    #[serde(rename = "A", borrow)]
    aq: &'a str,
    #[serde(rename = "T")]
    t: i64,
    #[serde(rename = "E")]
    e: i64,
}

/// Spot JSON `bookTicker` (no time fields).
#[derive(Deserialize)]
struct SpotTicker<'a> {
    u: u64,
    #[serde(borrow)]
    s: &'a str,
    #[serde(borrow)]
    b: &'a str,
    #[serde(rename = "B", borrow)]
    bq: &'a str,
    #[serde(borrow)]
    a: &'a str,
    #[serde(rename = "A", borrow)]
    aq: &'a str,
}

/// Every receive field of `rx` copied unchanged (contract 3).
pub(crate) fn hdr(line: Line, inst: InstId, rx: &RxMeta) -> MarketHdr {
    MarketHdr {
        venue: line.venue(),
        conn: rx.conn,
        _pad: 0,
        inst,
        ex_ts_ns: 0,
        ex_send_ns: 0,
        hw_rx_ns: rx.hw_rx_ns,
        rx_tsc: rx.rx_tsc,
        sw_rx_ns: rx.sw_rx_ns,
        rx_empty_gap_ns: rx.rx_empty_gap_ns,
        _rsv: 0,
    }
}

pub(crate) fn decode_md(
    line: Line,
    rd: &RefData,
    ctx: &mut FeedCtx,
    c: Class,
    p: &[u8],
    rx: &RxMeta,
    out: &mut impl MdSink,
) -> Result<MdOutcome, DecodeErr> {
    match c {
        Class::BookTicker => {
            let (u, s, b, bq, a, aq, ex) = match line {
                Line::Usdm => {
                    let m: UsdmTicker = from(p)?;
                    let unit = ctx.time_unit.ns();
                    let ex = (time_ns(m.t, unit)?, time_ns(m.e, unit)?);
                    (m.u, m.s, m.b, m.bq, m.a, m.aq, ex)
                }
                Line::Spot => {
                    let m: SpotTicker = from(p)?;
                    (m.u, m.s, m.b, m.bq, m.a, m.aq, (0, 0))
                }
            };
            let inst = inst_of(line, rd, s.as_bytes());
            if inst.0 == 0 {
                return Ok(MdOutcome { events: 0, inst });
            }
            let row = rd.inst(inst);
            let (pe, qe) = (row.px_wire_exp, row.qty_wire_exp);
            let mut e = Bbo {
                h: hdr(line, inst, rx),
                bid_px: Px(fixed(b, pe)?),
                bid_qty: Qty(fixed(bq, qe)?),
                ask_px: Px(fixed(a, pe)?),
                ask_qty: Qty(fixed(aq, qe)?),
                update_id: u,
            };
            e.h.ex_ts_ns = ex.0;
            e.h.ex_send_ns = ex.1;
            out.bbo(&e);
            Ok(MdOutcome { events: 1, inst })
        }
        // The other classes decode from M3 (README "已知限制与后续工作"); control classes never.
        _ => Err(DecodeErr::UnknownClass),
    }
}
