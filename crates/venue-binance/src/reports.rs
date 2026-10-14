//! Report dispatch (README "1. 分派") and WS API response decoding (README "5. 回报解码",
//! "结果未知码表"). The user-data stream events decode from M3.

use hft_event::{GwOp, OrderEvent, OrderEventKind};
use hft_refdata::{LimitBucket, LimitScope, RefData};
use hft_types::{AccountId, AssetExp, AssetId, ClientOrderId, InstId, Px, Qty, RoundMode, SlotId};
use hft_venue_proto::{
    DecodeErr, MsgOp, RateObs, ReportCtl, ReportOutcome, ReportSink, ReqRef, RxMeta, Skip,
};

use crate::cid::{IdKind, id_kind, parse_cid};
use crate::errors::{
    CANCEL_GONE, CLOCK_SKEW, CodeClass, ORDER_LIMIT, QUERY_NOT_FOUND, WEIGHT_LIMIT, classify_code, raw_code,
};
use crate::json::{OrderResult, RateKind, WsResp, fixed, from, fx};
use crate::md::inst_of;
use crate::{GwCtx, Line, spot, usdm};

const HEAD: usize = 64;

/// `QueryResult` order-status codes (`ReconcileReport.order_status`, contract 28).
pub(crate) mod status {
    pub const NEW: u8 = 1;
    pub const PARTIALLY_FILLED: u8 = 2;
    pub const FILLED: u8 = 3;
    pub const CANCELLED: u8 = 4;
    pub const REJECTED: u8 = 5;
    pub const EXPIRED: u8 = 6;
    pub const NOT_FOUND: u8 = 7;
}

/// `CancelReject.detail` class `GONE` (contract 28).
const REJECT_GONE: u8 = 1;

/// Both lines' report classes, unified.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum RClass {
    WsResp,
    UserEvent,
    Control,
    Ignorable,
    Unknown,
}

impl From<usdm::ReportClass> for RClass {
    fn from(c: usdm::ReportClass) -> RClass {
        use usdm::ReportClass as R;
        match c {
            R::WsResp => RClass::WsResp,
            R::OrderTradeUpdate
            | R::AccountUpdate
            | R::MarginCall
            | R::AccountConfigUpdate
            | R::ListenKeyExpired => RClass::UserEvent,
            R::Control => RClass::Control,
            R::Ignorable => RClass::Ignorable,
            R::Unknown => RClass::Unknown,
        }
    }
}

impl From<spot::ReportClass> for RClass {
    fn from(c: spot::ReportClass) -> RClass {
        use spot::ReportClass as R;
        match c {
            R::WsResp => RClass::WsResp,
            R::ExecutionReport | R::OutboundAccountPosition | R::EventStreamTerminated => RClass::UserEvent,
            R::Control => RClass::Control,
            R::Ignorable => RClass::Ignorable,
            R::Unknown => RClass::Unknown,
        }
    }
}

pub(crate) trait ClassifyReport: Sized {
    fn classify_report(op: MsgOp, head: &[u8]) -> Self;
}

/// The id of a `{"id":"<17>"` head.
fn head_id(h: &[u8]) -> Option<IdKind> {
    let rest = h.strip_prefix(b"{\"id\":\"")?;
    let end = rest.iter().position(|&c| c == b'"')?;
    Some(id_kind(rest.get(..end)?))
}

/// The event name after `prefix` (`{"e":"` or the spot event wrapper).
fn name_after<'a>(h: &'a [u8], prefix: &[u8]) -> Option<&'a [u8]> {
    let rest = h.strip_prefix(prefix)?;
    let end = rest.iter().position(|&c| c == b'"')?;
    rest.get(..end)
}

/// The spot user-data wrapper `{"subscriptionId":<n>,"event":{"e":"<name>"`.
fn spot_event_name(h: &[u8]) -> Option<&[u8]> {
    let rest = h.strip_prefix(b"{\"subscriptionId\":")?;
    let digits = rest.iter().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    name_after(rest.get(digits..)?, b",\"event\":{\"e\":\"")
}

const USDM_IGNORABLE: [&[u8]; 4] = [
    b"TRADE_LITE",
    b"STRATEGY_UPDATE",
    b"GRID_UPDATE",
    b"CONDITIONAL_ORDER_TRIGGER_REJECT",
];
const SPOT_IGNORABLE: [&[u8]; 3] = [b"balanceUpdate", b"listStatus", b"externalLockUpdate"];

impl ClassifyReport for usdm::ReportClass {
    fn classify_report(op: MsgOp, head: &[u8]) -> usdm::ReportClass {
        use usdm::ReportClass as R;
        if op != MsgOp::Text {
            return R::Unknown;
        }
        let h = head.get(..head.len().min(HEAD)).unwrap_or(&[]);
        match head_id(h) {
            Some(IdKind::Order(_)) => return R::WsResp,
            Some(IdKind::Control) => return R::Control,
            Some(IdKind::Foreign) => return R::Unknown,
            None => {}
        }
        match name_after(h, b"{\"e\":\"") {
            Some(b"ORDER_TRADE_UPDATE") => R::OrderTradeUpdate,
            Some(b"ACCOUNT_UPDATE") => R::AccountUpdate,
            Some(b"MARGIN_CALL") => R::MarginCall,
            Some(b"ACCOUNT_CONFIG_UPDATE") => R::AccountConfigUpdate,
            Some(b"listenKeyExpired") => R::ListenKeyExpired,
            Some(n) if USDM_IGNORABLE.contains(&n) => R::Ignorable,
            _ => R::Unknown,
        }
    }
}

impl ClassifyReport for spot::ReportClass {
    fn classify_report(op: MsgOp, head: &[u8]) -> spot::ReportClass {
        use spot::ReportClass as R;
        if op != MsgOp::Text {
            return R::Unknown;
        }
        let h = head.get(..head.len().min(HEAD)).unwrap_or(&[]);
        match head_id(h) {
            Some(IdKind::Order(_)) => return R::WsResp,
            Some(IdKind::Control) => return R::Control,
            Some(IdKind::Foreign) => return R::Unknown,
            None => {}
        }
        match spot_event_name(h) {
            Some(b"executionReport") => R::ExecutionReport,
            Some(b"outboundAccountPosition") => R::OutboundAccountPosition,
            Some(b"eventStreamTerminated") => R::EventStreamTerminated,
            Some(n) if SPOT_IGNORABLE.contains(&n) => R::Ignorable,
            _ => R::Unknown,
        }
    }
}

/// Empty `OrderEvent` of this line and account carrying the frame's receive points.
pub(crate) fn blank(line: Line, account: AccountId, rx: &RxMeta) -> OrderEvent {
    OrderEvent {
        venue: line.venue(),
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
        fee_asset: AssetId(0),
        epoch: 0,
        reject_code: 0,
        exch_order_id: 0,
        ex_ts_ns: 0,
        hw_rx_ns: rx.hw_rx_ns,
        rx_tsc: rx.rx_tsc,
        avg_px: Px(0),
        cash_qty: hft_types::Qty(0),
        cash_sum: hft_types::Qty(0),
        trade_id: 0,
    }
}

/// `RateObs` set to the limit (rate-limited answers, README "结果未知码表"): the effective refdata
/// rule of `bucket` (orders: the shortest window).
fn full(line: Line, rd: &RefData, bucket: LimitBucket) -> Option<RateObs> {
    let r = rd
        .rate_limits(line.venue())
        .iter()
        .filter(|l| {
            l.bucket == bucket
                && matches!(
                    (bucket, l.scope),
                    (LimitBucket::Weight, LimitScope::Ip) | (LimitBucket::Orders, LimitScope::Account)
                )
        })
        .min_by_key(|l| l.window_ms)?;
    Some(RateObs {
        bucket,
        used: r.limit,
        limit: r.limit,
        window_ms: r.window_ms,
    })
}

/// Epoch, slot and gen of the order: from its client id when present (contract 13), else from the
/// request id with epoch 0 (error answers carry no client id; README "已知限制与后续工作").
fn ids_of(cid: Option<ClientOrderId>, req: ReqRef) -> (u16, u32, u32) {
    match cid {
        Some(c) => (c.epoch, c.slot, c.r#gen),
        None => (0, req.slot, req.r#gen),
    }
}

pub(crate) fn exch_key(id: u64, buf: &mut [u8; 20]) -> &[u8] {
    let mut i = buf.len();
    let mut x = id;
    loop {
        i -= 1;
        buf[i] = b'0' + (x % 10) as u8;
        x /= 10;
        if x == 0 || i == 0 {
            break;
        }
    }
    &buf[i..]
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn decode_report(
    line: Line,
    rd: &RefData,
    ctx: &mut GwCtx,
    c: RClass,
    p: &[u8],
    rx: &RxMeta,
    out: &mut impl ReportSink,
) -> Result<ReportOutcome, DecodeErr> {
    match c {
        RClass::WsResp => ws_resp(line, rd, p, rx, out),
        RClass::Control => Ok(ReportOutcome::default()),
        RClass::Ignorable => Ok(ReportOutcome {
            skip: Skip::Listed,
            ..ReportOutcome::default()
        }),
        RClass::Unknown => {
            let h = p.get(..p.len().min(HEAD)).unwrap_or(&[]);
            let name = match line {
                Line::Usdm => name_after(h, b"{\"e\":\""),
                Line::Spot => spot_event_name(h),
            };
            let listed = name.is_some_and(|n| {
                rd.caps(line.venue())
                    .ignorable_user_events
                    .iter()
                    .any(|e| e.as_bytes() == n)
            });
            Ok(ReportOutcome {
                skip: if listed { Skip::Listed } else { Skip::Unknown },
                ..ReportOutcome::default()
            })
        }
        // ORDER_TRADE_UPDATE, ACCOUNT_UPDATE, executionReport, …: M3.
        RClass::UserEvent => crate::user_stream::decode(line, rd, ctx, p, rx, out),
    }
}

fn ws_resp(
    line: Line,
    rd: &RefData,
    p: &[u8],
    rx: &RxMeta,
    out: &mut impl ReportSink,
) -> Result<ReportOutcome, DecodeErr> {
    let m: WsResp = from(p)?;
    let req = match id_kind(m.id.as_bytes()) {
        IdKind::Order(r) => r,
        _ => return Err(DecodeErr::Shape),
    };
    let mut o = ReportOutcome {
        req: Some(req),
        rate: m
            .rate_limits
            .and_then(|r| r.best)
            .map(|(k, used, limit, window_ms)| RateObs {
                bucket: match k {
                    RateKind::Orders => LimitBucket::Orders,
                    RateKind::Weight => LimitBucket::Weight,
                },
                used,
                limit,
                window_ms,
            }),
        ..ReportOutcome::default()
    };
    let account = rx.account;
    let res = m.result.and_then(|r| r.obj).unwrap_or_default();
    if m.status == 200 && m.error.is_none() {
        let e = ok_event(line, rd, req, &res, account, rx, out)?;
        if let Some(e) = e {
            out.order(&e);
            o.events = 1;
        }
        return Ok(o);
    }
    let code = m.error.map(|e| e.code);
    let raw = raw_code(code, m.status);
    o.raw_code = raw;
    match code {
        Some(CLOCK_SKEW) => o.ctl = ReportCtl::ClockSkew,
        Some(WEIGHT_LIMIT) => {
            o.ctl = ReportCtl::RateLimited;
            o.rate = full(line, rd, LimitBucket::Weight).or(o.rate);
        }
        Some(ORDER_LIMIT) => {
            o.ctl = ReportCtl::RateLimited;
            o.rate = full(line, rd, LimitBucket::Orders).or(o.rate);
        }
        None if m.status == 429 || m.status == 418 => {
            o.ctl = ReportCtl::RateLimited;
            o.rate = full(line, rd, LimitBucket::Weight).or(o.rate);
        }
        _ => {}
    }
    let mut e = blank(line, account, rx);
    e.slot = SlotId(req.slot);
    e.r#gen = req.r#gen;
    e.reject_code = raw;
    match (req.op, code) {
        (GwOp::Cancel, Some(CANCEL_GONE)) => {
            e.what = OrderEventKind::CancelReject;
            e.detail = REJECT_GONE;
        }
        (GwOp::Query, Some(QUERY_NOT_FOUND)) => {
            e.what = OrderEventKind::QueryResult;
            e.detail = status::NOT_FOUND;
        }
        (op, _) => match classify_code(code, m.status) {
            CodeClass::Unknown => {
                if op != GwOp::CancelAll && op != GwOp::Query {
                    o.outcome_unknown = Some(req);
                }
                return Ok(o);
            }
            CodeClass::Reject => match op {
                GwOp::Place => e.what = OrderEventKind::Reject,
                GwOp::Cancel => e.what = OrderEventKind::CancelReject,
                GwOp::Amend => e.what = OrderEventKind::AmendReject,
                // Spot cancel-all and query answers that fail produce no order event.
                _ => return Ok(o),
            },
        },
    }
    out.order(&e);
    o.events = 1;
    Ok(o)
}

/// The order event of a successful answer (`None` for spot cancel-all).
fn ok_event(
    line: Line,
    rd: &RefData,
    req: ReqRef,
    r: &OrderResult<'_>,
    account: AccountId,
    rx: &RxMeta,
    out: &mut impl ReportSink,
) -> Result<Option<OrderEvent>, DecodeErr> {
    let (what, cid_text, order_id, symbol) = match req.op {
        GwOp::Place => (OrderEventKind::Ack, r.client_order_id, r.order_id, r.symbol),
        GwOp::Cancel => (
            OrderEventKind::CancelAck,
            // The cancelled order's own id: spot answers carry it as `origClientOrderId` (their
            // `clientOrderId` is the cancel request's), USDT-M as `clientOrderId`.
            match line {
                Line::Spot => r.orig_client_order_id,
                Line::Usdm => r.client_order_id,
            },
            r.order_id,
            r.symbol,
        ),
        GwOp::Amend => match (&r.amended, line) {
            (Some(a), Line::Spot) => (
                OrderEventKind::AmendAck,
                a.orig_client_order_id,
                a.order_id,
                a.symbol,
            ),
            _ => (OrderEventKind::AmendAck, r.client_order_id, r.order_id, r.symbol),
        },
        GwOp::Query => (
            OrderEventKind::QueryResult,
            r.client_order_id,
            r.order_id,
            r.symbol,
        ),
        _ => return Ok(None),
    };
    let cid = cid_text.and_then(|t| parse_cid(t.as_bytes()));
    let (epoch, slot, r#gen) = ids_of(cid, req);
    let mut e = blank(line, account, rx);
    e.what = what;
    e.epoch = epoch;
    e.slot = SlotId(slot);
    e.r#gen = r#gen;
    e.inst = symbol.map_or(InstId(0), |s| inst_of(line, rd, s.as_bytes()));
    e.exch_order_id = order_id.unwrap_or(0);
    let unit = match line {
        Line::Usdm => 1_000_000,
        Line::Spot => 1_000,
    };
    e.ex_ts_ns = r
        .update_time
        .or(r.transact_time)
        .and_then(|t| t.checked_mul(unit))
        .unwrap_or(0);
    if what == OrderEventKind::QueryResult {
        query_fields(line, rd, r, &mut e)?;
    }
    if let (Some(id), Some(c)) = (order_id, cid.as_ref())
        && matches!(what, OrderEventKind::Ack)
    {
        let mut b = [0u8; 20];
        let key = exch_key(id, &mut b);
        let _ = out.exch_id(key, Some(c));
    }
    Ok(Some(e))
}

/// `order.status` → status code, `cum_filled`, `avg_px` (README "累计量与费用来源"; contract 9).
fn query_fields(line: Line, rd: &RefData, r: &OrderResult<'_>, e: &mut OrderEvent) -> Result<(), DecodeErr> {
    e.detail = match r.status {
        Some("NEW") => status::NEW,
        Some("PARTIALLY_FILLED") => status::PARTIALLY_FILLED,
        Some("FILLED") => status::FILLED,
        Some("CANCELED") => status::CANCELLED,
        Some("REJECTED") => status::REJECTED,
        Some("EXPIRED" | "EXPIRED_IN_MATCH") => status::EXPIRED,
        _ => return Err(DecodeErr::Shape),
    };
    if e.inst.0 == 0 {
        return Err(DecodeErr::Shape);
    }
    let row = rd.inst(e.inst);
    let z = Qty(fixed(r.executed_qty.ok_or(DecodeErr::Shape)?, row.qty_wire_exp)?);
    e.cum_filled = z;
    e.avg_px = match line {
        Line::Usdm => Px(fixed(r.avg_price.ok_or(DecodeErr::Shape)?, row.px_wire_exp)?),
        Line::Spot => {
            let quote = r.cum_quote.ok_or(DecodeErr::Shape)?;
            if z.0 == 0 {
                Px(0)
            } else {
                let n = hft_types::parse_round(quote.as_bytes(), -6, RoundMode::HalfEven).map_err(fx)?;
                let exp = AssetExp {
                    px_exp: row.px_exp,
                    qty_exp: row.qty_exp,
                };
                Px::from_notional(hft_types::Notional(n), z, exp, RoundMode::HalfEven).map_err(fx)?
            }
        }
    };
    if e.cum_filled.0 > 0 && e.avg_px.0 == 0 {
        return Err(DecodeErr::Shape);
    }
    Ok(())
}
