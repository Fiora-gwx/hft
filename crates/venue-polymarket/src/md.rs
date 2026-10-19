//! Market channel (README "行情频道 → 事件"): dispatch, `peek`, decoding of `book`,
//! `price_change` and `last_trade_price`, and the `PONG` control frame.
//!
//! The channel has no sequence numbers. A `book` replaces the inst's book; a `price_change`
//! entry carries the new aggregate size of one price level (`"0"` deletes it). Ids of the
//! emitted `L2Snapshot`/`L2Batch` are synthetic per-inst counters of `FeedCtx`, so consecutive
//! messages form a continuous chain; a reconnect resubscribes and the exchange sends a fresh
//! `book` first.

use core::fmt;
use core::marker::PhantomData;

use hft_event::{L2BatchHdr, L2SnapHdr, L2SnapshotPart, Level, MarketHdr, Trade};
use hft_refdata::RefData;
use hft_types::{InstId, Px, Qty, Side, VenueId, parse_exact};
use hft_venue_proto::{
    Align, ControlHint, DecodeErr, MdOutcome, MdSink, MsgOp, Peek, RawSeq, ResyncMethod, RouteId, RxMeta,
    SeqIds, SeqRule, StreamKind,
};
use serde::Deserialize;
use serde::de::{Deserializer, IgnoredAny, SeqAccess, Visitor};

use crate::{FeedCtx, MdClass};

/// Levels per side of one `book` (the price grid has at most 999 levels).
pub(crate) const BOOK_MAX: usize = 1000;
/// Entries of one `price_change` message.
pub(crate) const CHANGES_MAX: usize = 512;
/// Distinct tokens in one `price_change` message (one market = two tokens).
const ASSETS_MAX: usize = 8;
/// Books in one list frame.
const LIST_MAX: usize = 16;

pub(crate) fn classify(_route: RouteId, op: MsgOp, head: &[u8]) -> MdClass {
    if op != MsgOp::Text {
        return MdClass::Unknown;
    }
    match head.first() {
        Some(b'{' | b'[') => MdClass::Push,
        _ if head == b"PONG" => MdClass::Pong,
        _ => MdClass::Unknown,
    }
}

pub(crate) fn seq_rule(_c: MdClass) -> SeqRule {
    // No exchange sequence: forward in arrival order; continuity is synthesized by the decoder.
    SeqRule::None
}

pub(crate) fn resync_method(c: MdClass) -> ResyncMethod {
    match c {
        // Resubscribing makes the exchange send a fresh `book`.
        MdClass::Push => ResyncMethod::Resubscribe,
        _ => ResyncMethod::None,
    }
}

pub(crate) fn align_first(_snap_id: u64, _raw: &RawSeq) -> Align {
    Align::Apply
}

pub(crate) fn control(c: MdClass, _payload: &[u8]) -> Option<ControlHint> {
    match c {
        MdClass::Pong => Some(ControlHint::Pong),
        _ => None,
    }
}

// ---------------------------------------------------------------- wire structs

/// Millisecond timestamp sent as a decimal string (or number).
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub(crate) struct Ms(pub Option<u64>);

impl<'de> Deserialize<'de> for Ms {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = Ms;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("milliseconds")
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Ms, E> {
                Ok(Ms(Some(v)))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Ms, E> {
                hft_venue_proto::json::uint(v.as_bytes())
                    .map(|x| Ms(Some(x)))
                    .ok_or_else(|| E::custom("ms"))
            }
            fn visit_borrowed_str<E: serde::de::Error>(self, v: &str) -> Result<Ms, E> {
                self.visit_str(v)
            }
        }
        d.deserialize_any(V)
    }
}

impl Ms {
    fn ns(self) -> Result<i64, DecodeErr> {
        match self.0 {
            None => Ok(0),
            Some(ms) => i64::try_from(ms)
                .ok()
                .and_then(|v| v.checked_mul(1_000_000))
                .ok_or(DecodeErr::Overflow),
        }
    }
}

/// `{"price":"0.42","size":"242.09"}` fields of one level or change, as borrowed text.
pub(crate) struct Fields<'a> {
    pub asset_id: &'a str,
    pub price: &'a str,
    pub size: &'a str,
    pub side: &'a str,
}

/// One object of a raw array (`{"price":…,"size":…}` or a `price_changes` entry): the fast
/// projection, else the checked decoder.
/// Borrowed string of a projected field; an absent field is empty.
fn text(raw: &[u8]) -> Option<&str> {
    if raw.is_empty() {
        Some("")
    } else {
        hft_venue_proto::json::string(raw)
    }
}

fn fields(el: &[u8]) -> Result<Fields<'_>, DecodeErr> {
    let fast = hft_venue_proto::json::project::<4>(el, |k| match k {
        b"asset_id" => Some(0),
        b"price" => Some(1),
        b"size" => Some(2),
        b"side" => Some(3),
        _ => None,
    });
    if let Some([a, p, z, d]) = fast
        && let (Some(a), Some(p), Some(z), Some(d)) = (text(a), text(p), text(z), text(d))
    {
        return Ok(Fields {
            asset_id: a,
            price: p,
            size: z,
            side: d,
        });
    }
    let f: FieldsSlow = from(el)?;
    Ok(Fields {
        asset_id: f.asset_id,
        price: f.price,
        size: f.size,
        side: f.side,
    })
}

#[derive(Deserialize)]
struct FieldsSlow<'a> {
    #[serde(default, borrow)]
    asset_id: &'a str,
    #[serde(borrow)]
    price: &'a str,
    #[serde(borrow)]
    size: &'a str,
    #[serde(default, borrow)]
    side: &'a str,
}

/// Every message shape of the market channel; absent fields stay empty. Arrays stay raw text
/// (no per-level storage on the stack).
#[derive(Deserialize)]
pub(crate) struct Msg<'a> {
    #[serde(default, borrow)]
    event_type: &'a str,
    #[serde(default, borrow)]
    asset_id: &'a str,
    #[serde(default)]
    timestamp: Ms,
    #[serde(default, borrow)]
    bids: Option<sonic_rs::LazyValue<'a>>,
    #[serde(default, borrow)]
    asks: Option<sonic_rs::LazyValue<'a>>,
    #[serde(default, borrow)]
    price_changes: Option<sonic_rs::LazyValue<'a>>,
    #[serde(default, borrow)]
    price: &'a str,
    #[serde(default, borrow)]
    size: &'a str,
    #[serde(default, borrow)]
    side: &'a str,
}

/// `peek` view: the type and the first token, nothing else decoded.
#[derive(Deserialize)]
struct PeekMsg<'a> {
    #[serde(default, borrow)]
    event_type: &'a str,
    #[serde(default, borrow)]
    asset_id: &'a str,
    #[serde(default, borrow)]
    price_changes: Option<First<AssetOnly<'a>>>,
}

#[derive(Deserialize)]
struct AssetOnly<'a> {
    #[serde(borrow)]
    asset_id: &'a str,
}

/// First element of an array (the rest skipped); empty → `None`.
struct First<T>(Option<T>);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for First<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V<T>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for V<T> {
            type Value = First<T>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("array")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut s: A) -> Result<First<T>, A::Error> {
                let first = s.next_element::<T>()?;
                while s.next_element::<IgnoredAny>()?.is_some() {}
                Ok(First(first))
            }
        }
        d.deserialize_seq(V(PhantomData))
    }
}

pub(crate) fn from<'a, T: Deserialize<'a>>(p: &'a [u8]) -> Result<T, DecodeErr> {
    let text = core::str::from_utf8(p).map_err(|_| DecodeErr::Shape)?;
    sonic_rs::from_str::<T>(text).map_err(|e| {
        if e.is_eof() {
            DecodeErr::Truncated
        } else {
            DecodeErr::Shape
        }
    })
}

/// Top-level elements of a JSON array frame (`[{…},{…}]`), as byte slices; an object frame is
/// its own single element. Strings and nesting are tracked; anything malformed ends the scan.
pub(crate) fn elements(p: &[u8]) -> Elements<'_> {
    let start = p.iter().position(|b| !b.is_ascii_whitespace()).unwrap_or(p.len());
    match p.get(start) {
        Some(b'[') => Elements {
            p,
            at: start + 1,
            list: true,
            done: false,
        },
        _ => Elements {
            p,
            at: start,
            list: false,
            done: false,
        },
    }
}

pub(crate) struct Elements<'a> {
    p: &'a [u8],
    at: usize,
    list: bool,
    done: bool,
}

impl<'a> Iterator for Elements<'a> {
    type Item = &'a [u8];
    fn next(&mut self) -> Option<&'a [u8]> {
        if self.done {
            return None;
        }
        if !self.list {
            self.done = true;
            return self.p.get(self.at..);
        }
        let p = self.p;
        let mut i = self.at;
        while p.get(i).is_some_and(|b| b.is_ascii_whitespace() || *b == b',') {
            i += 1;
        }
        if p.get(i) != Some(&b'{') {
            self.done = true;
            return None;
        }
        let start = i;
        let (mut depth, mut in_str, mut esc) = (0usize, false, false);
        while let Some(&b) = p.get(i) {
            i += 1;
            if in_str {
                if esc {
                    esc = false;
                } else if b == b'\\' {
                    esc = true;
                } else if b == b'"' {
                    in_str = false;
                }
                continue;
            }
            match b {
                b'"' => in_str = true,
                b'{' | b'[' => depth += 1,
                b'}' | b']' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        self.at = i;
                        return p.get(start..i);
                    }
                }
                _ => {}
            }
        }
        self.done = true;
        None
    }
}

// ---------------------------------------------------------------- peek

pub(crate) fn peek(_rd: &RefData, ctx: &FeedCtx, c: MdClass, p: &[u8]) -> Result<Peek, DecodeErr> {
    if c != MdClass::Push {
        return Err(DecodeErr::UnknownClass);
    }
    let first = elements(p).next().ok_or(DecodeErr::Shape)?;
    let m: PeekMsg = from(first)?;
    let (stream, token) = match m.event_type {
        "book" => (StreamKind::DepthTopN, m.asset_id),
        "price_change" => (
            StreamKind::DepthTopN,
            m.price_changes.and_then(|f| f.0).map_or("", |a| a.asset_id),
        ),
        "last_trade_price" => (StreamKind::Trade, m.asset_id),
        // tick_size_change, best_bid_ask, new_market, market_resolved: not decoded.
        _ => return Err(DecodeErr::UnknownClass),
    };
    Ok(Peek {
        inst: ctx.inst_of(token.as_bytes()),
        stream,
        ids: SeqIds::default(),
        raw: RawSeq::default(),
    })
}

// ---------------------------------------------------------------- decode

const ZERO: Level = Level {
    px: Px(0),
    qty: Qty(0),
};

pub(crate) fn hdr(venue: VenueId, inst: InstId, rx: &RxMeta, ex_ts_ns: i64) -> MarketHdr {
    MarketHdr {
        venue,
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

fn fx(e: hft_types::FxErr) -> DecodeErr {
    if e.0 & hft_types::FxErr::OVERFLOW.0 != 0 {
        DecodeErr::Overflow
    } else {
        DecodeErr::Number
    }
}

fn fixed(s: &str, exp: i8) -> Result<i64, DecodeErr> {
    parse_exact(s.as_bytes(), exp).map_err(fx)
}

fn side(s: &str) -> Result<Side, DecodeErr> {
    match s {
        "BUY" | "buy" | "Buy" => Ok(Side::Buy),
        "SELL" | "sell" | "Sell" => Ok(Side::Sell),
        _ => Err(DecodeErr::Shape),
    }
}

fn levels(raw: &[u8], pe: i8, qe: i8, out: &mut [Level]) -> Result<usize, DecodeErr> {
    let mut n = 0usize;
    for el in array(raw)? {
        let f = fields(el)?;
        let qty = fixed(f.size, qe)?;
        let px = fixed(f.price, pe)?;
        if qty == 0 {
            continue;
        }
        let slot = out.get_mut(n).ok_or(DecodeErr::Shape)?;
        *slot = Level {
            px: Px(px),
            qty: Qty(qty),
        };
        n += 1;
    }
    Ok(n)
}

/// Objects of a raw JSON array; not an array → `Shape`.
fn array(raw: &[u8]) -> Result<Elements<'_>, DecodeErr> {
    let t = raw
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(raw.len());
    if raw.get(t) != Some(&b'[') {
        return Err(DecodeErr::Shape);
    }
    Ok(elements(raw))
}

pub(crate) fn decode_md(
    venue: VenueId,
    rd: &RefData,
    ctx: &mut FeedCtx,
    c: MdClass,
    p: &[u8],
    rx: &RxMeta,
    out: &mut impl MdSink,
) -> Result<MdOutcome, DecodeErr> {
    if c != MdClass::Push {
        return Err(DecodeErr::UnknownClass);
    }
    let mut events: u16 = 0;
    let mut first_inst = InstId(0);
    for (k, el) in elements(p).enumerate() {
        if k >= LIST_MAX {
            return Err(DecodeErr::Shape);
        }
        let m: Msg = from(el)?;
        let ex = m.timestamp.ns()?;
        let (n, inst) = match m.event_type {
            "book" => book(venue, rd, ctx, &m, ex, rx, out)?,
            "price_change" => price_change(venue, rd, ctx, &m, ex, rx, out)?,
            "last_trade_price" => trade(venue, rd, ctx, &m, ex, rx, out)?,
            _ => (0, InstId(0)),
        };
        events = events.saturating_add(n);
        if first_inst.0 == 0 {
            first_inst = inst;
        }
    }
    Ok(MdOutcome {
        events,
        inst: first_inst,
    })
}

fn book(
    venue: VenueId,
    rd: &RefData,
    ctx: &mut FeedCtx,
    m: &Msg<'_>,
    ex: i64,
    rx: &RxMeta,
    out: &mut impl MdSink,
) -> Result<(u16, InstId), DecodeErr> {
    let Some((ix, inst)) = ctx.lookup(m.asset_id.as_bytes()) else {
        return Ok((0, InstId(0)));
    };
    let row = rd.inst(inst);
    let (pe, qe) = (row.px_wire_exp, row.qty_wire_exp);
    let mut bids = [ZERO; BOOK_MAX];
    let mut asks = [ZERO; BOOK_MAX];
    let nb = match &m.bids {
        Some(b) => levels(b.as_raw_str().as_bytes(), pe, qe, &mut bids)?,
        None => return Err(DecodeErr::Shape),
    };
    let na = match &m.asks {
        Some(a) => levels(a.as_raw_str().as_bytes(), pe, qe, &mut asks)?,
        None => return Err(DecodeErr::Shape),
    };
    let snap_id = ctx.next_snapshot(ix);
    out.l2_snapshot(&L2SnapshotPart {
        hdr: L2SnapHdr {
            h: hdr(venue, inst, rx, ex),
            snap_id,
            origin: L2SnapHdr::EXCHANGE,
            _pad: [0; 3],
            n_bids: u16::try_from(nb).map_err(|_| DecodeErr::Shape)?,
            n_asks: u16::try_from(na).map_err(|_| DecodeErr::Shape)?,
        },
        bids: bids.get(..nb).ok_or(DecodeErr::Shape)?,
        asks: asks.get(..na).ok_or(DecodeErr::Shape)?,
        part: 0,
        parts: 1,
    });
    Ok((1, inst))
}

fn price_change(
    venue: VenueId,
    rd: &RefData,
    ctx: &mut FeedCtx,
    m: &Msg<'_>,
    ex: i64,
    rx: &RxMeta,
    out: &mut impl MdSink,
) -> Result<(u16, InstId), DecodeErr> {
    let raw = m
        .price_changes
        .as_ref()
        .ok_or(DecodeErr::Shape)?
        .as_raw_str()
        .as_bytes();
    // Distinct bound tokens in order of appearance; every entry is validated before any event.
    let mut assets: [(&str, usize, InstId, u16, u16); ASSETS_MAX] = [("", 0, InstId(0), 0, 0); ASSETS_MAX];
    let mut na = 0usize;
    for (k, el) in array(raw)?.enumerate() {
        if k >= CHANGES_MAX {
            return Err(DecodeErr::Shape);
        }
        let e = fields(el)?;
        let sd = side(e.side)?;
        let Some((ix, inst)) = ctx.lookup(e.asset_id.as_bytes()) else {
            continue;
        };
        let row = rd.inst(inst);
        fixed(e.price, row.px_wire_exp)?;
        fixed(e.size, row.qty_wire_exp)?;
        let at = match assets.iter().take(na).position(|a| a.0 == e.asset_id) {
            Some(i) => i,
            None => {
                let slot = assets.get_mut(na).ok_or(DecodeErr::Shape)?;
                *slot = (e.asset_id, ix, inst, 0, 0);
                na += 1;
                na - 1
            }
        };
        if let Some(a) = assets.get_mut(at) {
            if sd == Side::Buy {
                a.3 += 1;
            } else {
                a.4 += 1;
            }
        }
    }
    let mut events = 0u16;
    for &(token, ix, inst, nb, nk) in assets.iter().take(na) {
        let row = rd.inst(inst);
        let (pe, qe) = (row.px_wire_exp, row.qty_wire_exp);
        let (prev_id, last_id) = ctx.next_batch(ix);
        out.l2_begin(&L2BatchHdr {
            h: hdr(venue, inst, rx, ex),
            prev_id,
            last_id,
            n_bids: nb,
            n_asks: nk,
            bflags: 0,
            _pad: 0,
        });
        for want in [Side::Buy, Side::Sell] {
            for el in array(raw)? {
                let e = fields(el)?;
                if e.asset_id == token && side(e.side)? == want {
                    out.l2_level(want, Px(fixed(e.price, pe)?), Qty(fixed(e.size, qe)?));
                }
            }
        }
        out.l2_end();
        events += 1;
    }
    Ok((events, assets.first().map_or(InstId(0), |a| a.2)))
}

fn trade(
    venue: VenueId,
    rd: &RefData,
    ctx: &mut FeedCtx,
    m: &Msg<'_>,
    ex: i64,
    rx: &RxMeta,
    out: &mut impl MdSink,
) -> Result<(u16, InstId), DecodeErr> {
    let Some((ix, inst)) = ctx.lookup(m.asset_id.as_bytes()) else {
        return Ok((0, InstId(0)));
    };
    let row = rd.inst(inst);
    let px = Px(fixed(m.price, row.px_wire_exp)?);
    let qty = Qty(fixed(m.size, row.qty_wire_exp)?);
    let taker_side = side(m.side)?;
    let trade_id = ctx.next_trade(ix);
    out.trade(&Trade {
        h: hdr(venue, inst, rx, ex),
        px,
        qty,
        trade_id,
        taker_side,
        _pad: [0; 7],
    });
    Ok((1, inst))
}
