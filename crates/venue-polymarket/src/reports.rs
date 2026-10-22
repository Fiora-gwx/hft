//! Order reports (README "回报"): the gw state (`GwCtx`: token bindings, order hashes and
//! cumulative fills), REST order responses and the user channel's `order` events.

use hft_event::{GwOp, OrderEvent, OrderEventKind};
use hft_refdata::RefData;
use hft_types::{AccountId, InstId, Px, Qty, Side, SlotId, parse_exact};
use hft_venue_proto::{
    DecodeErr, ExchIdHint, MsgOp, ProtoErr, ReportCtl, ReportOutcome, ReportSink, RestOrigin, RestWhat,
    RxMeta, Skip, raw_code_http, raw_code_str,
};
use serde::Deserialize;
use serde::de::{Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};

use crate::bind::FeedCtx;
use crate::{ReportClass, VENUE};

/// `QueryResult` order-status codes (`ReconcileReport.order_status`, contract 28).
mod status {
    pub const NEW: u8 = 1;
    pub const PARTIALLY_FILLED: u8 = 2;
    pub const FILLED: u8 = 3;
    pub const CANCELLED: u8 = 4;
    pub const NOT_FOUND: u8 = 7;
}
/// `CancelReject.detail` class `GONE` (contract 28).
const REJECT_GONE: u8 = 1;

/// Orders remembered per account (hash, cumulative fill, terminal); oldest overwritten first.
const ORDERS: usize = 512;
/// Token bindings per account (slot insts of the venue).
const TOKENS: usize = 64;
/// Events of one user-channel frame.
const FRAME_MAX: usize = 16;

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
struct Row {
    /// First 8 bytes of the hash, big-endian (`OrderEvent.exch_order_id`); 0 = empty.
    key: u64,
    hash: [u8; 32],
    /// Cumulative filled quantity (inst qty units).
    cum: i64,
    terminal: bool,
}

/// gw state of one account: token ids bound by `BindWire`, the hashes of this process's orders
/// and their cumulative fills (README "回报").
#[derive(Clone, Debug)]
pub struct GwCtx {
    tokens: FeedCtx,
    rows: Box<[Row]>,
    next: usize,
}

impl Default for GwCtx {
    fn default() -> Self {
        GwCtx {
            tokens: FeedCtx::with_capacity(TOKENS),
            rows: vec![Row::default(); ORDERS].into_boxed_slice(),
            next: 0,
        }
    }
}

/// `exch_order_id` of an order hash; never 0.
pub(crate) fn key_of(hash: &[u8; 32]) -> u64 {
    let mut b = [0u8; 8];
    b.copy_from_slice(&hash[..8]);
    u64::from_be_bytes(b).max(1)
}

/// Decimal text of a key: the runtime's `IdTable` key (its value is the key itself).
fn key_text(key: u64, buf: &mut [u8; 20]) -> &[u8] {
    crate::orders::dec(key, buf).as_bytes()
}

impl GwCtx {
    pub(crate) fn bind(&mut self, inst: InstId, id: &[u8; 32]) -> Result<(), ProtoErr> {
        self.tokens.bind_wire(inst, id).map_err(|_| ProtoErr::Cfg)
    }
    /// Decimal token id bound to `inst`.
    pub fn token_of(&self, inst: InstId) -> Option<&[u8]> {
        self.tokens.token_of(inst)
    }
    fn inst_of(&self, token: &[u8]) -> InstId {
        self.tokens.inst_of(token)
    }
    fn row(&self, key: u64) -> Option<&Row> {
        if key == 0 {
            return None;
        }
        self.rows.iter().find(|r| r.key == key)
    }
    fn row_mut(&mut self, key: u64) -> Option<&mut Row> {
        if key == 0 {
            return None;
        }
        self.rows.iter_mut().find(|r| r.key == key)
    }
    /// Full hash of an order this process placed (`exch_order_id` → hash).
    pub fn order_hash(&self, key: u64) -> Option<[u8; 32]> {
        self.row(key).map(|r| r.hash)
    }
    /// Remembers an order hash; returns its key.
    fn remember(&mut self, hash: &[u8; 32]) -> u64 {
        let key = key_of(hash);
        if self.row(key).is_none() {
            let n = self.rows.len();
            if let Some(r) = self.rows.get_mut(self.next % n) {
                *r = Row {
                    key,
                    hash: *hash,
                    cum: 0,
                    terminal: false,
                };
            }
            self.next = (self.next + 1) % n;
        }
        key
    }
}

/// `0x` + 64 hex digits.
fn parse_hash(s: &str) -> Option<[u8; 32]> {
    let h = s.strip_prefix("0x")?.as_bytes();
    if h.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, o) in out.iter_mut().enumerate() {
        let hi = (h[2 * i] as char).to_digit(16)?;
        let lo = (h[2 * i + 1] as char).to_digit(16)?;
        *o = (hi << 4 | lo) as u8;
    }
    Some(out)
}

fn from<'a, T: Deserialize<'a>>(b: &'a [u8]) -> Result<T, DecodeErr> {
    sonic_rs::from_slice(b).map_err(|_| DecodeErr::Shape)
}

fn blank(account: AccountId, rx: &RxMeta) -> OrderEvent {
    OrderEvent {
        venue: VENUE,
        account,
        what: OrderEventKind::Ack,
        detail: 0,
        slot: SlotId::UNRESOLVED,
        r#gen: 0,
        inst: InstId(0),
        px: Px(0),
        qty: Qty(0),
        cum_filled: Qty(0),
        fee: Qty(0),
        fee_asset: hft_types::AssetId(0),
        epoch: 0,
        reject_code: 0,
        exch_order_id: 0,
        ex_ts_ns: 0,
        hw_rx_ns: rx.hw_rx_ns,
        rx_tsc: rx.rx_tsc,
        avg_px: Px(0),
        cash_qty: Qty(0),
        cash_sum: Qty(0),
        trade_id: 0,
        fill_cash_qty: Qty(0),
        fill_cash_amount: Qty(0),
    }
}

// ---------------------------------------------------------------- REST responses

#[derive(Deserialize)]
struct PlaceResp<'a> {
    #[serde(default)]
    success: bool,
    #[serde(rename = "orderID", default, borrow)]
    order_id: &'a str,
    #[serde(rename = "errorMsg", default, borrow)]
    error_msg: &'a str,
}

#[derive(Deserialize)]
struct ErrResp<'a> {
    #[serde(default, borrow)]
    error: &'a str,
}

/// Up to `N` elements of a JSON array, without allocating; more → error.
struct Few<T, const N: usize> {
    v: [Option<T>; N],
}

impl<T, const N: usize> Default for Few<T, N> {
    fn default() -> Self {
        Few {
            v: core::array::from_fn(|_| None),
        }
    }
}

impl<T, const N: usize> Few<T, N> {
    fn iter(&self) -> impl Iterator<Item = &T> {
        self.v.iter().flatten()
    }
}

impl<'de, T: Deserialize<'de>, const N: usize> Deserialize<'de> for Few<T, N> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V<T, const N: usize>(core::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>, const N: usize> Visitor<'de> for V<T, N> {
            type Value = Few<T, N>;
            fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "an array of at most {N} elements")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut s: A) -> Result<Few<T, N>, A::Error> {
                let mut out = Few::default();
                let mut i = 0;
                while let Some(x) = s.next_element::<T>()? {
                    let slot = out
                        .v
                        .get_mut(i)
                        .ok_or_else(|| serde::de::Error::custom("too many elements"))?;
                    *slot = Some(x);
                    i += 1;
                }
                Ok(out)
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Few<T, N>, E> {
                Ok(Few::default())
            }
        }
        d.deserialize_any(V::<T, N>(core::marker::PhantomData))
    }
}

/// The first `key: value` of a JSON object of strings (the rest skipped); `null` / `{}` = none.
#[derive(Default)]
struct First<'a>(Option<(&'a str, &'a str)>);

impl<'de: 'a, 'a> Deserialize<'de> for First<'a> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V<'a>(core::marker::PhantomData<&'a ()>);
        impl<'de: 'a, 'a> Visitor<'de> for V<'a> {
            type Value = First<'a>;
            fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.write_str("an object of strings")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut m: A) -> Result<First<'a>, A::Error> {
                let mut out = First(None);
                while let Some(k) = m.next_key::<&'de str>()? {
                    if out.0.is_none() {
                        out.0 = Some((k, m.next_value::<&'de str>()?));
                    } else {
                        m.next_value::<IgnoredAny>()?;
                    }
                }
                Ok(out)
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<First<'a>, E> {
                Ok(First(None))
            }
        }
        d.deserialize_any(V(core::marker::PhantomData))
    }
}

/// Orders one cancel answer may name.
const CANCELS_MAX: usize = 64;

#[derive(Deserialize)]
struct CancelResp<'a> {
    #[serde(default, borrow)]
    canceled: Few<&'a str, CANCELS_MAX>,
    #[serde(default, borrow)]
    not_canceled: First<'a>,
}

#[derive(Deserialize)]
struct OpenOrder<'a> {
    #[serde(borrow)]
    id: &'a str,
    #[serde(borrow)]
    status: &'a str,
    #[serde(borrow)]
    asset_id: &'a str,
    #[serde(borrow)]
    original_size: &'a str,
    #[serde(borrow)]
    size_matched: &'a str,
    #[serde(borrow)]
    price: &'a str,
}

/// HTTP statuses of a refused request (README "结果未知码表"): the order never reached matching.
fn refused(status: u16) -> bool {
    matches!(status, 400 | 401 | 403 | 404 | 425 | 429)
}

/// Raw code of an error answer: the `error` / `errorMsg` text, else the HTTP status.
fn err_code(status: u16, body: &[u8]) -> u32 {
    let msg = from::<ErrResp>(body).map(|e| e.error).unwrap_or("");
    if msg.is_empty() {
        raw_code_http(status)
    } else {
        raw_code_str(msg.as_bytes())
    }
}

pub(crate) fn decode_rest(
    rd: &RefData,
    ctx: &mut GwCtx,
    req: &RestOrigin,
    status: u16,
    body: &[u8],
    rx: &RxMeta,
    out: &mut impl ReportSink,
) -> Result<ReportOutcome, DecodeErr> {
    let r = match (req.what, req.req) {
        (RestWhat::Order, Some(r)) => r,
        _ => return Err(DecodeErr::UnknownClass),
    };
    let mut o = ReportOutcome {
        req: Some(r),
        ..ReportOutcome::default()
    };
    if matches!(status, 425 | 429) {
        o.ctl = ReportCtl::RateLimited;
    }
    let ok = (200..300).contains(&status);
    match r.op {
        GwOp::Place => {
            let mut e = blank(rx.account, rx);
            e.slot = SlotId(r.slot);
            e.r#gen = r.r#gen;
            e.inst = req.inst;
            if ok {
                let p: PlaceResp = from(body)?;
                if p.success
                    && p.error_msg.is_empty()
                    && let Some(h) = parse_hash(p.order_id)
                {
                    let key = ctx.remember(&h);
                    let mut buf = [0u8; 20];
                    let _ = out.exch_id(key_text(key, &mut buf), None);
                    e.exch_order_id = key;
                    out.order(&e);
                    o.events = 1;
                    return Ok(o);
                }
                // `success: false` with a message: refused by the exchange.
                if !p.error_msg.is_empty() {
                    e.what = OrderEventKind::Reject;
                    e.reject_code = raw_code_str(p.error_msg.as_bytes());
                    o.raw_code = e.reject_code;
                    out.order(&e);
                    o.events = 1;
                    return Ok(o);
                }
                o.outcome_unknown = Some(r);
                return Ok(o);
            }
            if refused(status) {
                e.what = OrderEventKind::Reject;
                e.reject_code = err_code(status, body);
                o.raw_code = e.reject_code;
                out.order(&e);
                o.events = 1;
                return Ok(o);
            }
            o.outcome_unknown = Some(r);
            o.raw_code = if status == 0 { 0 } else { raw_code_http(status) };
            Ok(o)
        }
        GwOp::Cancel | GwOp::CancelAll => {
            if !ok {
                if r.op == GwOp::Cancel && !refused(status) {
                    o.outcome_unknown = Some(r);
                }
                o.raw_code = if status == 0 { 0 } else { err_code(status, body) };
                return Ok(o);
            }
            let c: CancelResp = from(body)?;
            for id in c.canceled.iter() {
                let h = parse_hash(id).ok_or(DecodeErr::Shape)?;
                cancelled(ctx, &h, req.inst, 0, rx, out, &mut o);
            }
            // A single cancel the exchange refused (already filled, cancelled or unknown).
            if r.op == GwOp::Cancel
                && let Some((_, why)) = c.not_canceled.0
            {
                let mut e = blank(rx.account, rx);
                e.what = OrderEventKind::CancelReject;
                e.slot = SlotId(r.slot);
                e.r#gen = r.r#gen;
                e.inst = req.inst;
                e.detail = REJECT_GONE;
                e.reject_code = raw_code_str(why.as_bytes());
                o.raw_code = e.reject_code;
                out.order(&e);
                o.events += 1;
            }
            Ok(o)
        }
        GwOp::Query => {
            let mut e = blank(rx.account, rx);
            e.what = OrderEventKind::QueryResult;
            e.slot = SlotId(r.slot);
            e.r#gen = r.r#gen;
            e.inst = req.inst;
            if !ok {
                if status == 404 {
                    e.detail = status::NOT_FOUND;
                    out.order(&e);
                    o.events = 1;
                    return Ok(o);
                }
                o.outcome_unknown = Some(r);
                o.raw_code = if status == 0 { 0 } else { err_code(status, body) };
                return Ok(o);
            }
            if body.trim_ascii().is_empty() || body.trim_ascii() == b"null" {
                e.detail = status::NOT_FOUND;
                out.order(&e);
                o.events = 1;
                return Ok(o);
            }
            let q: OpenOrder = from(body)?;
            let h = parse_hash(q.id).ok_or(DecodeErr::Shape)?;
            let inst = ctx.inst_of(q.asset_id.as_bytes());
            if inst.0 != 0 && req.inst.0 != 0 && inst != req.inst {
                return Err(DecodeErr::Shape);
            }
            let row = rd.inst(req.inst);
            let orig = parse_exact(q.original_size.as_bytes(), row.qty_exp).map_err(|_| DecodeErr::Number)?;
            let cum = parse_exact(q.size_matched.as_bytes(), row.qty_exp).map_err(|_| DecodeErr::Number)?;
            let px = parse_exact(q.price.as_bytes(), row.px_exp).map_err(|_| DecodeErr::Number)?;
            e.detail = match q.status {
                "LIVE" | "DELAYED" if cum == 0 => status::NEW,
                "LIVE" | "DELAYED" => status::PARTIALLY_FILLED,
                "MATCHED" => status::FILLED,
                "CANCELED" | "CANCELED_MARKET_RESOLVED" | "UNMATCHED" | "INVALID" => status::CANCELLED,
                _ => return Err(DecodeErr::Shape),
            };
            if cum < 0 || cum > orig {
                return Err(DecodeErr::Shape);
            }
            e.exch_order_id = key_of(&h);
            e.cum_filled = Qty(cum);
            e.avg_px = if cum > 0 { Px(px) } else { Px(0) };
            out.order(&e);
            o.events = 1;
            Ok(o)
        }
        _ => Err(DecodeErr::UnknownClass),
    }
}

/// `CancelAck` for a cancelled order hash, once per order; only for orders the runtime can map.
fn cancelled(
    ctx: &mut GwCtx,
    h: &[u8; 32],
    inst: InstId,
    ex_ts_ns: i64,
    rx: &RxMeta,
    out: &mut impl ReportSink,
    o: &mut ReportOutcome,
) {
    let key = key_of(h);
    if ctx.row(key).is_some_and(|r| r.terminal) {
        return;
    }
    let mut buf = [0u8; 20];
    let ExchIdHint::Resolved { epoch, slot, r#gen } = out.exch_id(key_text(key, &mut buf), None) else {
        return;
    };
    if let Some(r) = ctx.row_mut(key) {
        r.terminal = true;
    }
    let mut e = blank(rx.account, rx);
    e.what = OrderEventKind::CancelAck;
    e.inst = inst;
    e.epoch = epoch;
    e.slot = slot;
    e.r#gen = r#gen;
    e.exch_order_id = key;
    e.ex_ts_ns = ex_ts_ns;
    e.cum_filled = Qty(ctx.row(key).map_or(0, |r| r.cum));
    out.order(&e);
    o.events += 1;
}

// ---------------------------------------------------------------- user channel

pub(crate) fn classify_report(op: MsgOp, head: &[u8]) -> ReportClass {
    if op != MsgOp::Text {
        return ReportClass::Unknown;
    }
    match head.first() {
        Some(b'{' | b'[') => ReportClass::Push,
        _ if head == b"PONG" => ReportClass::Ignorable,
        _ => ReportClass::Unknown,
    }
}

#[derive(Deserialize)]
struct Event<'a> {
    #[serde(default, borrow)]
    event_type: &'a str,
    #[serde(default, borrow)]
    id: &'a str,
    #[serde(default, borrow)]
    asset_id: &'a str,
    #[serde(default, borrow)]
    side: &'a str,
    #[serde(default, borrow)]
    original_size: &'a str,
    #[serde(default, borrow)]
    size_matched: &'a str,
    #[serde(default, borrow)]
    price: &'a str,
    #[serde(rename = "type", default, borrow)]
    kind: &'a str,
    #[serde(default, borrow)]
    timestamp: &'a str,
}

/// The `event_type` of a frame, for an unknown-event note (never the frame itself).
pub(crate) fn event_name(payload: &[u8]) -> &[u8] {
    const K: &[u8] = b"\"event_type\":\"";
    let Some(at) = payload.windows(K.len()).position(|w| w == K) else {
        return &[];
    };
    let rest = payload.get(at + K.len()..).unwrap_or(&[]);
    let end = rest.iter().position(|b| *b == b'"').unwrap_or(0).min(32);
    rest.get(..end).unwrap_or(&[])
}

pub(crate) fn decode_report(
    rd: &RefData,
    ctx: &mut GwCtx,
    c: ReportClass,
    payload: &[u8],
    rx: &RxMeta,
    out: &mut impl ReportSink,
) -> Result<ReportOutcome, DecodeErr> {
    let mut o = ReportOutcome::default();
    match c {
        ReportClass::Ignorable => {
            o.skip = Skip::Listed;
            return Ok(o);
        }
        ReportClass::Unknown => {
            o.skip = Skip::Unknown;
            return Ok(o);
        }
        ReportClass::Push => {}
    }
    let mut evs: Few<Event, FRAME_MAX> = Few::default();
    if payload.first() == Some(&b'[') {
        evs = from(payload)?;
    } else {
        evs.v[0] = Some(from(payload)?);
    }
    let mut known = false;
    for ev in evs.iter() {
        match ev.event_type {
            "order" => {
                known = true;
                order_event(rd, ctx, ev, rx, out, &mut o)?;
            }
            // Fills are taken from the order's cumulative `size_matched` (README "累计量与费用来源").
            "trade" => known = true,
            _ => {}
        }
    }
    if !known {
        o.skip = Skip::Unknown;
    } else if o.events == 0 {
        o.skip = Skip::Listed;
    }
    Ok(o)
}

fn order_event(
    rd: &RefData,
    ctx: &mut GwCtx,
    ev: &Event<'_>,
    rx: &RxMeta,
    out: &mut impl ReportSink,
    o: &mut ReportOutcome,
) -> Result<(), DecodeErr> {
    let h = parse_hash(ev.id).ok_or(DecodeErr::Shape)?;
    let inst = ctx.inst_of(ev.asset_id.as_bytes());
    // A token no slot is bound to: an order outside this engine (or of an unbound window).
    if inst.0 == 0 {
        return Ok(());
    }
    let row = rd.inst(inst);
    let num = |s: &str, exp| parse_exact(s.as_bytes(), exp).map_err(|_| DecodeErr::Number);
    let orig = num(ev.original_size, row.qty_exp)?;
    let cum = num(ev.size_matched, row.qty_exp)?;
    let px = num(ev.price, row.px_exp)?;
    let side = match ev.side {
        "BUY" => Side::Buy,
        "SELL" => Side::Sell,
        _ => return Err(DecodeErr::Shape),
    };
    if cum < 0 || orig <= 0 || cum > orig || px <= 0 {
        return Err(DecodeErr::Shape);
    }
    let ms: i64 = ev.timestamp.parse().map_err(|_| DecodeErr::Number)?;
    let ex_ts_ns = ms.checked_mul(1_000_000).ok_or(DecodeErr::Overflow)?;
    let key = ctx.remember(&h);
    let (prev, terminal) = ctx.row(key).map_or((0, false), |r| (r.cum, r.terminal));
    let mut buf = [0u8; 20];
    let mapped = out.exch_id(key_text(key, &mut buf), None);
    let (epoch, slot, r#gen) = match mapped {
        ExchIdHint::Resolved { epoch, slot, r#gen } => (epoch, slot, r#gen),
        _ => (0, SlotId::UNRESOLVED, 0),
    };
    if cum > prev {
        let mut e = blank(rx.account, rx);
        e.what = if cum == orig {
            OrderEventKind::Fill
        } else {
            OrderEventKind::PartialFill
        };
        e.detail = side as u8;
        e.epoch = epoch;
        e.slot = slot;
        e.r#gen = r#gen;
        e.inst = inst;
        // Resting orders fill at their own price.
        e.px = Px(px);
        e.qty = Qty(cum - prev);
        e.cum_filled = Qty(cum);
        e.avg_px = Px(px);
        e.exch_order_id = key;
        e.ex_ts_ns = ex_ts_ns;
        if let Some(r) = ctx.row_mut(key) {
            r.cum = cum;
            r.terminal |= cum == orig;
        }
        out.order(&e);
        o.events += 1;
    }
    if ev.kind == "CANCELLATION" && !terminal && matches!(mapped, ExchIdHint::Resolved { .. }) {
        cancelled(ctx, &h, inst, ex_ts_ns, rx, out, o);
    }
    Ok(())
}
