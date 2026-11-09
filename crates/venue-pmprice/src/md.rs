//! Market data (README "行情频道 → 事件"): dispatch from the first 64 B, `peek` ids, control
//! answers and decoding of `price.crypto` (→ `IndexPrice`) and `price.crypto.twap` (→ `MarkPrice`),
//! live frames and the history (`snapshot: true`) frame pushed after each subscription.

use hft_event::{IndexPrice, MarkPrice, MarketHdr};
use hft_refdata::RefData;
use hft_types::{InstId, Px};
use hft_venue_proto::{
    ControlHint, DecodeErr, MdOutcome, MdSink, MsgOp, Peek, RawSeq, RouteId, RxMeta, SeqIds, SeqRule,
    StreamKind,
};
use serde::Deserialize;

use crate::{FeedCtx, VENUE};

/// Market-data dispatch classes (README "使用本组件").
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum MdClass {
    /// `price.crypto`: Chainlink price samples → `IndexPrice`.
    Price,
    /// `price.crypto.twap`: the 60 s TWAP → `MarkPrice`.
    Twap,
    /// `{"op":"subscribed"}`: one per subscription.
    Subscribed,
    /// `{"op":"error"}`: a refused frame (`bad_channel`, `auth_required`, …).
    Error,
    /// `{"op":"pong"}`: answer to the application ping.
    Pong,
    /// `{"op":"authed"}`: the login answer (handled before the connection reaches the feed).
    Authed,
    Unknown,
}

const HEAD: usize = 64;

/// `<name>` of a frame that starts `{"op":"<name>"`.
pub(crate) fn op_name(p: &[u8]) -> Option<&[u8]> {
    let h = p.get(..p.len().min(HEAD))?;
    let rest = h.strip_prefix(b"{\"op\":\"")?;
    let end = rest.iter().position(|&c| c == b'"')?;
    rest.get(..end)
}

fn has(h: &[u8], needle: &[u8]) -> bool {
    h.windows(needle.len()).any(|w| w == needle)
}

pub(crate) fn classify(_route: RouteId, op: MsgOp, head: &[u8]) -> MdClass {
    if op != MsgOp::Text {
        return MdClass::Unknown;
    }
    if let Some(name) = op_name(head) {
        return match name {
            b"subscribed" => MdClass::Subscribed,
            b"error" => MdClass::Error,
            b"pong" => MdClass::Pong,
            b"authed" => MdClass::Authed,
            _ => MdClass::Unknown,
        };
    }
    let h = head.get(..head.len().min(HEAD)).unwrap_or(&[]);
    if !h.starts_with(b"{\"v\":") {
        MdClass::Unknown
    } else if has(h, b"\"channel\":\"price.crypto.twap\"") {
        MdClass::Twap
    } else if has(h, b"\"channel\":\"price.crypto\"") {
        MdClass::Price
    } else {
        MdClass::Unknown
    }
}

/// Dedup on the sample time (ms): live frames repeat nothing, a history frame repeats what was
/// already forwarded (README "去重").
pub(crate) fn seq_rule(c: MdClass) -> SeqRule {
    match c {
        MdClass::Price | MdClass::Twap => SeqRule::Monotonic,
        _ => SeqRule::None,
    }
}

pub(crate) fn control(c: MdClass, _payload: &[u8]) -> Option<ControlHint> {
    match c {
        MdClass::Subscribed => Some(ControlHint::SubAck { ok: true, code: 0 }),
        MdClass::Error => Some(ControlHint::SubAck { ok: false, code: 1 }),
        MdClass::Pong => Some(ControlHint::Pong),
        _ => None,
    }
}

fn stream_of(c: MdClass) -> Result<StreamKind, DecodeErr> {
    match c {
        MdClass::Price => Ok(StreamKind::Index),
        MdClass::Twap => Ok(StreamKind::Mark),
        _ => Err(DecodeErr::UnknownClass),
    }
}

/// Parses `p` into `T`; the input ending early → `Truncated`, anything else → `Shape`.
fn from<'a, T: Deserialize<'a>>(p: &'a [u8]) -> Result<T, DecodeErr> {
    let text = core::str::from_utf8(p).map_err(|_| DecodeErr::Shape)?;
    sonic_rs::from_str::<T>(text).map_err(|e| {
        if e.is_eof() {
            DecodeErr::Truncated
        } else {
            DecodeErr::Shape
        }
    })
}

/// A data frame: a live sample (`payload.timestamp`/`full_accuracy_value`) or the history frame
/// (`snapshot: true`, `payload.data` = samples).
#[derive(Deserialize)]
struct Msg<'a> {
    #[serde(default)]
    ts: i64,
    #[serde(default)]
    snapshot: bool,
    #[serde(borrow)]
    payload: Payload<'a>,
}

#[derive(Deserialize)]
struct Payload<'a> {
    #[serde(default, borrow)]
    symbol: &'a str,
    #[serde(default)]
    timestamp: i64,
    #[serde(default, borrow)]
    full_accuracy_value: &'a str,
    #[serde(default, borrow)]
    data: Option<sonic_rs::LazyValue<'a>>,
}

#[derive(Deserialize)]
struct Item<'a> {
    timestamp: i64,
    #[serde(borrow)]
    full_accuracy_value: &'a str,
}

/// Objects of a JSON array (the history frame's `data`), without allocation.
struct Elements<'a> {
    p: &'a [u8],
    at: usize,
    done: bool,
}

fn elements(p: &[u8]) -> Result<Elements<'_>, DecodeErr> {
    let start = p.iter().position(|b| !b.is_ascii_whitespace()).unwrap_or(p.len());
    if p.get(start) != Some(&b'[') {
        return Err(DecodeErr::Shape);
    }
    Ok(Elements {
        p,
        at: start + 1,
        done: false,
    })
}

impl<'a> Iterator for Elements<'a> {
    type Item = &'a [u8];
    fn next(&mut self) -> Option<&'a [u8]> {
        if self.done {
            return None;
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

/// Samples of one history frame at most (two minutes of seconds, with room).
pub const HISTORY_MAX: usize = 1_024;

/// A decimal `[0-9]+(\.[0-9]+)?` rounded half up to `10^exp` (`exp ≤ 0`): the feed carries up to
/// 18 fractional digits, far finer than any price grid (README "价格精度").
pub fn round_dec(s: &[u8], exp: i8) -> Result<i64, DecodeErr> {
    if exp > 0 || s.is_empty() {
        return Err(DecodeErr::Number);
    }
    let keep = usize::from(exp.unsigned_abs());
    let (int, frac) = match s.iter().position(|&c| c == b'.') {
        Some(i) => (s.get(..i).unwrap_or(&[]), s.get(i + 1..).unwrap_or(&[])),
        None => (s, &[][..]),
    };
    if int.is_empty() || (s.contains(&b'.') && frac.is_empty()) {
        return Err(DecodeErr::Number);
    }
    let mut v: i64 = 0;
    let digit = |c: u8| -> Result<i64, DecodeErr> {
        if c.is_ascii_digit() {
            Ok(i64::from(c - b'0'))
        } else {
            Err(DecodeErr::Number)
        }
    };
    for &c in int {
        let d = digit(c)?;
        v = v
            .checked_mul(10)
            .and_then(|x| x.checked_add(d))
            .ok_or(DecodeErr::Overflow)?;
    }
    for k in 0..keep {
        let d = match frac.get(k) {
            Some(&c) => digit(c)?,
            None => 0,
        };
        v = v
            .checked_mul(10)
            .and_then(|x| x.checked_add(d))
            .ok_or(DecodeErr::Overflow)?;
    }
    let mut up = false;
    for (k, &c) in frac.iter().enumerate().skip(keep) {
        let d = digit(c)?;
        if k == keep {
            up = d >= 5;
        }
    }
    if up {
        v = v.checked_add(1).ok_or(DecodeErr::Overflow)?;
    }
    Ok(v)
}

fn inst_of(rd: &RefData, symbol: &str) -> InstId {
    rd.symbols(VENUE).get(symbol.as_bytes()).unwrap_or(InstId(0))
}

/// The newest sample time (ms) of a frame; 0 for an empty history.
fn newest(m: &Msg<'_>) -> Result<i64, DecodeErr> {
    match (&m.payload.data, m.snapshot) {
        (Some(d), true) => {
            let mut t = 0i64;
            for (k, el) in elements(d.as_raw_str().as_bytes())?.enumerate() {
                if k >= HISTORY_MAX {
                    return Err(DecodeErr::Shape);
                }
                let it: Item = from(el)?;
                t = t.max(it.timestamp);
            }
            Ok(t)
        }
        (None, false) => Ok(m.payload.timestamp),
        _ => Err(DecodeErr::Shape),
    }
}

pub(crate) fn peek(rd: &RefData, c: MdClass, p: &[u8]) -> Result<Peek, DecodeErr> {
    let stream = stream_of(c)?;
    let m: Msg = from(p)?;
    let last = newest(&m)?;
    Ok(Peek {
        inst: inst_of(rd, m.payload.symbol),
        stream,
        ids: SeqIds {
            prev_id: 0,
            last_id: u64::try_from(last).map_err(|_| DecodeErr::Shape)?,
        },
        raw: RawSeq::default(),
    })
}

/// Every receive field of `rx` copied unchanged (contract 3).
fn hdr(inst: InstId, ex_ts_ns: i64, ex_send_ns: i64, rx: &RxMeta) -> MarketHdr {
    MarketHdr {
        venue: VENUE,
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

fn ms_ns(ms: i64) -> Result<i64, DecodeErr> {
    ms.checked_mul(1_000_000).ok_or(DecodeErr::Overflow)
}

/// One sample → one event if it is newer than the last forwarded of its (inst, stream).
#[allow(clippy::too_many_arguments)]
fn sample(
    ctx: &mut FeedCtx,
    inst: InstId,
    stream: StreamKind,
    exp: i8,
    ts_ms: i64,
    value: &str,
    send_ms: i64,
    rx: &RxMeta,
    out: &mut impl MdSink,
) -> Result<u16, DecodeErr> {
    let px = Px(round_dec(value.as_bytes(), exp)?);
    if px.0 <= 0 || ts_ms <= 0 {
        return Err(DecodeErr::Shape);
    }
    if !ctx.advance(inst, stream, ts_ms) {
        return Ok(0);
    }
    let h = hdr(inst, ms_ns(ts_ms)?, ms_ns(send_ms)?, rx);
    if stream == StreamKind::Index {
        out.index(&IndexPrice { h, px });
    } else {
        out.mark(&MarkPrice { h, px });
    }
    Ok(1)
}

pub(crate) fn decode_md(
    rd: &RefData,
    ctx: &mut FeedCtx,
    c: MdClass,
    p: &[u8],
    rx: &RxMeta,
    out: &mut impl MdSink,
) -> Result<MdOutcome, DecodeErr> {
    let stream = stream_of(c)?;
    let m: Msg = from(p)?;
    let inst = inst_of(rd, m.payload.symbol);
    if inst.0 == 0 {
        // Validate the shape, emit nothing (a symbol this instance does not list).
        newest(&m)?;
        return Ok(MdOutcome { events: 0, inst });
    }
    let exp = rd.inst(inst).px_wire_exp;
    let mut events = 0u16;
    match (&m.payload.data, m.snapshot) {
        (Some(d), true) => {
            let raw = d.as_raw_str().as_bytes();
            // Validate every sample before the first event.
            for (n, el) in elements(raw)?.enumerate() {
                if n >= HISTORY_MAX {
                    return Err(DecodeErr::Shape);
                }
                let it: Item = from(el)?;
                round_dec(it.full_accuracy_value.as_bytes(), exp)?;
            }
            for el in elements(raw)? {
                let it: Item = from(el)?;
                events += sample(
                    ctx,
                    inst,
                    stream,
                    exp,
                    it.timestamp,
                    it.full_accuracy_value,
                    m.ts,
                    rx,
                    out,
                )?;
            }
        }
        (None, false) => {
            events += sample(
                ctx,
                inst,
                stream,
                exp,
                m.payload.timestamp,
                m.payload.full_accuracy_value,
                m.ts,
                rx,
                out,
            )?;
        }
        _ => return Err(DecodeErr::Shape),
    }
    Ok(MdOutcome { events, inst })
}
