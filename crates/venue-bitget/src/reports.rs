//! Report dispatch (README "实现说明" 2), WS trade answers, the private `order` / `position` /
//! `account` channels (README "用户流事件表", "外部成交识别", "累计量与费用来源") and the REST
//! order-query answer.

use hft_event::{
    BalanceReport, ExternalCause, ExternalFill, GwOp, OrderEvent, OrderEventKind, PositionReport,
};
use hft_refdata::{FeeAssetRule, LiqMarker, RefData};
use hft_types::{
    AccountId, AssetExp, AssetId, ClientOrderId, InstId, Notional, Px, Qty, RoundMode, Side, SlotId, Tsc,
    parse_round,
};
use hft_venue_proto::{
    DecodeErr, ExchIdHint, MsgOp, ReportCtl, ReportOutcome, ReportSink, ReqRef, RestOrigin, RestWhat, RxMeta,
    Skip,
};
use serde::Deserialize;

use crate::caps::LIQ_MARKER_ARG;
use crate::cid::{decode_req_id, parse_cid};
use crate::codes::{CLOCK_SKEW, CodeClass, ORDER_GONE, RATE_LIMITED, classify_code};
use crate::json::{Arg, Arr, Code, One, UInt, fixed, fixed_or_zero, from, fx, ms_ns};
use crate::md::{event_name, inst_of, is_push};
use crate::{GwCtx, Line, ReportClass};

const HEAD: usize = 64;
/// Orders per `order` push.
const ORDERS_MAX: usize = 32;
/// Positions per `position` push.
const POSITIONS_MAX: usize = 64;
/// Coins per `account` push.
const COINS_MAX: usize = 64;

/// `QueryResult` order-status codes (`OrderEvent.detail`, contract 28).
pub(crate) mod status {
    pub const NEW: u8 = 1;
    pub const PARTIALLY_FILLED: u8 = 2;
    pub const FILLED: u8 = 3;
    pub const CANCELLED: u8 = 4;
    pub const NOT_FOUND: u8 = 7;
}

/// `CancelReject.detail` / `AmendReject.detail` class `GONE` (contract 28).
const REJECT_GONE: u8 = 1;

// ---------------------------------------------------------------- dispatch

/// The quoted value after `key` inside the head (`None` when it does not end within the head).
fn quoted_after<'a>(h: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    let at = h.windows(key.len()).position(|w| w == key)?;
    let rest = h.get(at + key.len()..)?;
    let end = rest.iter().position(|&c| c == b'"')?;
    rest.get(..end)
}

/// Channel class of a private topic name.
fn topic_class(name: &[u8]) -> ReportClass {
    match name {
        b"order" | b"orders" => ReportClass::Orders,
        b"fill" | b"fast-fill" => ReportClass::Fills,
        b"position" | b"positions" => ReportClass::Positions,
        b"account" => ReportClass::Account,
        _ => ReportClass::Unknown,
    }
}

pub(crate) fn classify_report(op: MsgOp, head: &[u8]) -> ReportClass {
    if op != MsgOp::Text {
        return ReportClass::Unknown;
    }
    let h = head.get(..head.len().min(HEAD)).unwrap_or(&[]);
    if h == b"pong" {
        return ReportClass::Pong;
    }
    if h.starts_with(b"{") && !is_push(h) {
        // A request id in the head decides; a foreign id is not ours.
        if let Some(id) = quoted_after(h, b"\"id\":\"") {
            return match decode_req_id(id) {
                Some(_) => ReportClass::WsResp,
                None => ReportClass::Unknown,
            };
        }
        // No id within the head: `decode_report` reads the whole frame (`Control` frames that
        // carry one of our request ids are decoded as trade answers there).
        return match event_name(h) {
            Some(b"trade") => ReportClass::WsResp,
            Some(b"login" | b"subscribe" | b"unsubscribe" | b"error") => ReportClass::Control,
            Some(_) => ReportClass::Unknown,
            None if h.windows(7).any(|w| w == b"\"code\":") => ReportClass::Control,
            None => ReportClass::Unknown,
        };
    }
    if !is_push(h) {
        return ReportClass::Unknown;
    }
    match quoted_after(h, b"\"topic\":\"").or_else(|| quoted_after(h, b"\"channel\":\"")) {
        Some(name) => topic_class(name),
        None => ReportClass::Push,
    }
}

/// The request id of a control frame (absent or numeric → not ours).
#[derive(Deserialize)]
struct IdOnly<'a> {
    #[serde(borrow)]
    id: &'a str,
}

/// `arg` of a push whose topic lies beyond the head.
#[derive(Deserialize)]
struct PushHead<'a> {
    #[serde(borrow)]
    arg: Arg<'a>,
}

fn listed(line: Line, rd: &RefData, name: &str) -> Skip {
    let hit = !name.is_empty()
        && rd
            .caps(line.venue())
            .ignorable_user_events
            .iter()
            .any(|e| &**e == name);
    if hit { Skip::Listed } else { Skip::Unknown }
}

pub(crate) fn decode_report(
    line: Line,
    rd: &RefData,
    ctx: &mut GwCtx,
    c: ReportClass,
    p: &[u8],
    rx: &RxMeta,
    out: &mut impl ReportSink,
) -> Result<ReportOutcome, DecodeErr> {
    match c {
        ReportClass::WsResp => ws_resp(line, rd, p, rx, out),
        ReportClass::Orders => orders(line, rd, ctx, p, rx, out),
        ReportClass::Positions => positions(line, rd, p, rx, out),
        ReportClass::Account => account(line, rd, p, rx, out),
        ReportClass::Push => {
            let h: PushHead = from(p)?;
            match topic_class(h.arg.topic.as_bytes()) {
                ReportClass::Push | ReportClass::Unknown => Ok(ReportOutcome {
                    skip: listed(line, rd, h.arg.topic),
                    ..ReportOutcome::default()
                }),
                k => decode_report(line, rd, ctx, k, p, rx, out),
            }
        }
        // The `fill` channel is not subscribed: fills come from the `order` channel.
        ReportClass::Fills | ReportClass::Ignorable => Ok(ReportOutcome {
            skip: Skip::Listed,
            ..ReportOutcome::default()
        }),
        // An error answer whose request id lies beyond the head.
        ReportClass::Control => match from::<IdOnly>(p) {
            Ok(m) if decode_req_id(m.id.as_bytes()).is_some() => ws_resp(line, rd, p, rx, out),
            _ => Ok(ReportOutcome::default()),
        },
        ReportClass::Pong => Ok(ReportOutcome::default()),
        ReportClass::Unknown => {
            let h = p.get(..p.len().min(HEAD)).unwrap_or(&[]);
            let name = quoted_after(h, b"\"topic\":\"")
                .or_else(|| quoted_after(h, b"\"channel\":\""))
                .and_then(|n| core::str::from_utf8(n).ok())
                .unwrap_or("");
            Ok(ReportOutcome {
                skip: listed(line, rd, name),
                ..ReportOutcome::default()
            })
        }
    }
}

/// Empty `OrderEvent` of this line and account carrying the frame's receive points.
fn blank(line: Line, account: AccountId, rx: &RxMeta) -> OrderEvent {
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
        cash_qty: Qty(0),
        cash_sum: Qty(0),
        trade_id: 0,
    }
}

fn exch_key(id: u64, buf: &mut [u8; 20]) -> &[u8] {
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

/// Epoch, slot and gen: from the client id when it is ours, else from the request id (epoch 0).
fn ids_of(cid: Option<ClientOrderId>, req: ReqRef) -> (u16, u32, u32) {
    match cid {
        Some(c) => (c.epoch, c.slot, c.r#gen),
        None => (0, req.slot, req.r#gen),
    }
}

// ---------------------------------------------------------------- WS trade answers

#[derive(Deserialize, Default)]
struct RespArg<'a> {
    #[serde(rename = "orderId", default)]
    order_id: UInt,
    #[serde(rename = "clientOid", default, borrow)]
    client_oid: &'a str,
    #[serde(default, borrow)]
    symbol: &'a str,
    #[serde(rename = "cTime", default)]
    c_time: UInt,
}

#[derive(Deserialize)]
struct WsResp<'a> {
    #[serde(borrow)]
    event: &'a str,
    #[serde(borrow)]
    id: &'a str,
    #[serde(default, borrow)]
    args: Option<One<RespArg<'a>>>,
    #[serde(default)]
    code: Code,
    #[serde(default)]
    ts: UInt,
}

fn ws_resp(
    line: Line,
    rd: &RefData,
    p: &[u8],
    rx: &RxMeta,
    out: &mut impl ReportSink,
) -> Result<ReportOutcome, DecodeErr> {
    let m: WsResp = from(p)?;
    let req = decode_req_id(m.id.as_bytes()).ok_or(DecodeErr::Shape)?;
    let mut o = ReportOutcome {
        req: Some(req),
        ..ReportOutcome::default()
    };
    let account = rx.account;
    if m.event == "trade" && (m.code == Code::Absent || m.code.is_zero()) {
        // Cancel and modify answers only acknowledge receipt (official "Best practices"): the
        // order channel's `cancelled` push is the CancelAck; a modify has no confirmation (AMEND
        // is not a capability). Only a place answer produces an event.
        if req.op == GwOp::Place {
            let a = m.args.map(|x| x.0).unwrap_or_default();
            let id = a.order_id.get()?;
            let cid = parse_cid(a.client_oid.as_bytes());
            let (epoch, slot, r#gen) = ids_of(cid, req);
            let mut e = blank(line, account, rx);
            e.what = OrderEventKind::Ack;
            e.epoch = epoch;
            e.slot = SlotId(slot);
            e.r#gen = r#gen;
            e.inst = inst_of(line, rd, a.symbol);
            e.exch_order_id = id;
            e.ex_ts_ns = match a.c_time.opt()? {
                Some(ms) => ms_ns(ms)?,
                None => m.ts.opt()?.map_or(Ok(0), ms_ns)?,
            };
            if let Some(c) = cid.as_ref() {
                let mut b = [0u8; 20];
                let _ = out.exch_id(exch_key(id, &mut b), Some(c));
            }
            out.order(&e);
            o.events = 1;
        }
        return Ok(o);
    }
    let code = m.code.num();
    let raw = m.code.raw(0);
    o.raw_code = raw;
    match code {
        Some(CLOCK_SKEW) => o.ctl = ReportCtl::ClockSkew,
        Some(RATE_LIMITED) => o.ctl = ReportCtl::RateLimited,
        _ => {}
    }
    let mut e = blank(line, account, rx);
    e.slot = SlotId(req.slot);
    e.r#gen = req.r#gen;
    e.reject_code = raw;
    match (req.op, code) {
        (GwOp::Cancel, Some(ORDER_GONE)) => {
            e.what = OrderEventKind::CancelReject;
            e.detail = REJECT_GONE;
        }
        (GwOp::Amend, Some(ORDER_GONE)) => {
            e.what = OrderEventKind::AmendReject;
            e.detail = REJECT_GONE;
        }
        (op, _) => match classify_code(code, 0) {
            CodeClass::Unknown => {
                o.outcome_unknown = Some(req);
                return Ok(o);
            }
            CodeClass::Reject => match op {
                GwOp::Place => e.what = OrderEventKind::Reject,
                GwOp::Cancel => e.what = OrderEventKind::CancelReject,
                GwOp::Amend => e.what = OrderEventKind::AmendReject,
                _ => return Ok(o),
            },
        },
    }
    out.order(&e);
    o.events = 1;
    Ok(o)
}

// ---------------------------------------------------------------- order channel

#[derive(Deserialize, Default, Clone, Copy)]
struct FeeRow<'a> {
    #[serde(rename = "feeCoin", default, borrow)]
    coin: &'a str,
    #[serde(default, borrow)]
    fee: &'a str,
}

/// One order of the `order` push (UTA names; Classic V2 `orders` names as aliases).
#[derive(Deserialize)]
struct OrderRow<'a> {
    #[serde(default, borrow)]
    category: &'a str,
    #[serde(alias = "instId", borrow)]
    symbol: &'a str,
    #[serde(rename = "orderId")]
    order_id: UInt,
    #[serde(rename = "clientOid", default, borrow)]
    client_oid: &'a str,
    #[serde(default, borrow)]
    side: &'a str,
    #[serde(rename = "timeInForce", alias = "force", default, borrow)]
    tif: &'a str,
    #[serde(rename = "orderStatus", alias = "status", borrow)]
    status: &'a str,
    #[serde(rename = "tradeSide", default, borrow)]
    trade_side: &'a str,
    #[serde(rename = "delegateType", default, borrow)]
    delegate_type: &'a str,
    #[serde(rename = "cancelReason", default, borrow)]
    cancel_reason: &'a str,
    #[serde(rename = "cumExecQty", alias = "accBaseVolume", default, borrow)]
    cum_qty: Option<&'a str>,
    #[serde(rename = "cumExecValue", default, borrow)]
    cum_value: Option<&'a str>,
    #[serde(rename = "avgPrice", alias = "priceAvg", default, borrow)]
    avg_px: Option<&'a str>,
    #[serde(rename = "feeDetail", default, borrow)]
    fee_detail: Option<Arr<FeeRow<'a>, 4>>,
    // Classic V2 per-fill fields (absent on UTA).
    #[serde(rename = "baseVolume", default, borrow)]
    fill_qty: Option<&'a str>,
    #[serde(rename = "fillPrice", default, borrow)]
    fill_px: Option<&'a str>,
    #[serde(rename = "fillFee", default, borrow)]
    fill_fee: Option<&'a str>,
    #[serde(rename = "fillFeeCoin", default, borrow)]
    fill_fee_coin: Option<&'a str>,
    #[serde(rename = "tradeId", default, borrow)]
    trade_id: Option<&'a str>,
    #[serde(rename = "fillTime", default)]
    fill_time: UInt,
    #[serde(rename = "updatedTime", alias = "uTime", default)]
    updated: UInt,
}

#[derive(Deserialize)]
struct OrderMsg<'a> {
    #[serde(borrow)]
    data: Arr<OrderRow<'a>, ORDERS_MAX>,
    #[serde(default)]
    ts: UInt,
}

/// Order status of a push or a query.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum St {
    Open,
    Partial,
    Filled,
    Cancelled,
}

fn status_of(s: &str) -> Result<St, DecodeErr> {
    match s {
        "live" | "new" | "init" => Ok(St::Open),
        "partially_filled" | "partial-fill" => Ok(St::Partial),
        "filled" | "full-fill" => Ok(St::Filled),
        "cancelled" | "canceled" => Ok(St::Cancelled),
        _ => Err(DecodeErr::Shape),
    }
}

/// Liquidation / ADL / delivery marker of a perpetual order (README "外部成交识别").
fn marker(trade_side: &str, delegate_type: &str) -> Option<ExternalCause> {
    match delegate_type {
        "liquidation" => return Some(ExternalCause::Liquidation),
        "delivery_close_long" | "delivery_close_short" => return Some(ExternalCause::Delivery),
        _ => {}
    }
    if trade_side.is_empty() || !LIQ_MARKER_ARG.split('|').any(|x| x == trade_side) {
        return None;
    }
    Some(if trade_side.starts_with("dte_sys_adl") {
        ExternalCause::Adl
    } else if trade_side.starts_with("delivery") {
        ExternalCause::Delivery
    } else {
        ExternalCause::Liquidation
    })
}

/// Asset of a coin name among the fee-discount token and the assets of both Bitget lines' insts
/// (0 = not in refdata).
pub(crate) fn asset_by_name(line: Line, rd: &RefData, name: &str) -> AssetId {
    if name.is_empty() {
        return AssetId(0);
    }
    let token = match rd.caps(line.venue()).fee_asset_rule {
        FeeAssetRule::ReceivedAssetUnlessDiscount { token } | FeeAssetRule::Configured(token) => token,
        _ => AssetId(0),
    };
    if token.0 != 0 && &*rd.asset(token).name == name {
        return token;
    }
    // This line first, then the other Bitget line (a fee may be charged in any account coin).
    let other = match line {
        Line::Fut => Line::Spot,
        Line::Spot => Line::Fut,
    };
    for v in [line.venue(), other.venue()] {
        for &i in rd.insts_of_venue(v) {
            let h = rd.inst(i);
            for a in [h.asset, h.quote] {
                if a.0 != 0 && &*rd.asset(a).name == name {
                    return a;
                }
            }
        }
    }
    AssetId(0)
}

/// Whether `fee_asset` breaks the effective `fee_asset_rule` for a fill on `inst` with `side`.
fn fee_deviates(line: Line, rd: &RefData, inst: InstId, side: Option<Side>, fee_asset: AssetId) -> bool {
    let h = rd.inst(inst);
    let received = match side {
        Some(Side::Buy) => h.asset,
        Some(Side::Sell) => h.quote,
        None => return false,
    };
    match rd.caps(line.venue()).fee_asset_rule {
        FeeAssetRule::Quote => fee_asset != h.quote,
        FeeAssetRule::ReceivedAsset => fee_asset != received,
        FeeAssetRule::ReceivedAssetUnlessDiscount { token } => fee_asset != received && fee_asset != token,
        FeeAssetRule::Configured(a) => fee_asset != a,
    }
}

fn side_of(s: &str) -> Option<Side> {
    match s {
        "buy" | "Buy" | "BUY" => Some(Side::Buy),
        "sell" | "Sell" | "SELL" => Some(Side::Sell),
        _ => None,
    }
}

/// Absolute value of a signed decimal at `exp` (V2 reports fees negative).
fn abs_fixed(s: &str, exp: i8) -> Result<i64, DecodeErr> {
    Ok(fixed_or_zero(s, exp)?.checked_abs().unwrap_or(i64::MAX))
}

/// One validated order row, before the `GwCtx` comparison.
#[derive(Copy, Clone)]
struct Parsed {
    inst: InstId,
    order_id: u64,
    st: St,
    expired: bool,
    side: Option<Side>,
    cum: Qty,
    cum_value: Option<Notional>,
    avg_px: Px,
    /// Classic V2: this fill's size, price and fee.
    fill_qty: Option<Qty>,
    fill_px: Option<Px>,
    fill_fee: Option<Qty>,
    /// Cumulative fee of the order (UTA `feeDetail`).
    cum_fee: Qty,
    fee_asset: AssetId,
    trade_id: u64,
    ex_ts_ns: i64,
    cid: Option<ClientOrderId>,
    cid_empty: bool,
    cause: Option<ExternalCause>,
}

fn parse_row(line: Line, rd: &RefData, r: &OrderRow<'_>, top_ts: UInt) -> Result<Option<Parsed>, DecodeErr> {
    if !r.category.is_empty() && !line.is_category(r.category) {
        return Ok(None);
    }
    let order_id = r.order_id.get()?;
    let st = status_of(r.status)?;
    let inst = inst_of(line, rd, r.symbol);
    if inst.0 == 0 {
        return Ok(None);
    }
    let h = rd.inst(inst);
    let (pe, qe) = (h.px_wire_exp, h.qty_wire_exp);
    let cum = match r.cum_qty {
        Some(s) => Qty(fixed_or_zero(s, qe)?),
        None if st == St::Open || st == St::Cancelled => Qty(0),
        None => return Err(DecodeErr::Shape),
    };
    let avg_px = Px(match r.avg_px {
        Some(s) => fixed_or_zero(s, pe)?,
        None => 0,
    });
    // Contract 9: a fill without an average price is a decode error.
    if cum.0 > 0 && avg_px.0 == 0 {
        return Err(DecodeErr::Shape);
    }
    let cum_value = match r.cum_value {
        Some(s) if !s.is_empty() => Some(Notional(
            parse_round(s.as_bytes(), -6, RoundMode::HalfEven).map_err(fx)?,
        )),
        _ => None,
    };
    let fill_qty = match r.fill_qty {
        Some(s) if !s.is_empty() => Some(Qty(fixed(s, qe)?)),
        _ => None,
    };
    let fill_px = match r.fill_px {
        Some(s) if !s.is_empty() => Some(Px(fixed(s, pe)?)),
        _ => None,
    };
    if st == St::Filled && cum.0 == 0 && fill_qty.is_none() {
        return Err(DecodeErr::Shape);
    }
    let (mut fee_coin, mut cum_fee, mut fill_fee) = ("", Qty(0), None);
    if let Some(d) = r.fee_detail.as_ref()
        && let Some(f) = d.iter().find(|f| !f.fee.is_empty())
    {
        fee_coin = f.coin;
        let a = asset_by_name(line, rd, f.coin);
        let exp = if a.0 == 0 { -8 } else { rd.asset(a).exp.qty_exp };
        cum_fee = Qty(abs_fixed(f.fee, exp)?);
    }
    if let Some(s) = r.fill_fee
        && !s.is_empty()
    {
        fee_coin = r.fill_fee_coin.unwrap_or(fee_coin);
        let a = asset_by_name(line, rd, fee_coin);
        let exp = if a.0 == 0 { -8 } else { rd.asset(a).exp.qty_exp };
        fill_fee = Some(Qty(abs_fixed(s, exp)?));
    }
    let fee_asset = asset_by_name(line, rd, fee_coin);
    let trade_id = match r.trade_id {
        // Missing, 0 or negative ids are "absent" (contract 22).
        Some(s) if !s.is_empty() && !s.starts_with('-') => s.parse::<u64>().map_err(|_| DecodeErr::Number)?,
        _ => 0,
    };
    let ts = match r.fill_time.opt()? {
        Some(v) => Some(v),
        None => r.updated.opt()?,
    };
    let ex_ts_ns = match ts {
        Some(ms) => ms_ns(ms)?,
        None => top_ts.opt()?.map_or(Ok(0), ms_ns)?,
    };
    let expired = matches!(r.tif, "ioc" | "fok" | "IOC" | "FOK")
        || matches!(
            r.cancel_reason,
            "ioc_not_full_cancel" | "fok_not_full_cancel" | "post_only_fill_cancel"
        );
    let cause =
        if line == Line::Fut && rd.caps(line.venue()).own_liquidation_marker == LiqMarker::OrderTypeField {
            marker(r.trade_side, r.delegate_type)
        } else {
            None
        };
    Ok(Some(Parsed {
        inst,
        order_id,
        st,
        expired,
        side: side_of(r.side),
        cum,
        cum_value,
        avg_px,
        fill_qty,
        fill_px,
        fill_fee,
        cum_fee,
        fee_asset,
        trade_id,
        ex_ts_ns,
        cid: parse_cid(r.client_oid.as_bytes()),
        cid_empty: r.client_oid.is_empty(),
        cause,
    }))
}

fn orders(
    line: Line,
    rd: &RefData,
    ctx: &mut GwCtx,
    p: &[u8],
    rx: &RxMeta,
    out: &mut impl ReportSink,
) -> Result<ReportOutcome, DecodeErr> {
    let m: OrderMsg = from(p)?;
    if m.data.over {
        return Err(DecodeErr::Shape);
    }
    // Validate every row first: a failing message writes no event and leaves `ctx` unchanged.
    let mut rows: [Option<Parsed>; ORDERS_MAX] = [None; ORDERS_MAX];
    for (slot, r) in rows.iter_mut().zip(m.data.iter()) {
        *slot = parse_row(line, rd, r, m.ts)?;
    }
    let mut o = ReportOutcome::default();
    for x in rows.iter().flatten() {
        emit(line, rd, ctx, x, rx, out, &mut o);
    }
    Ok(o)
}

fn emit(
    line: Line,
    rd: &RefData,
    ctx: &mut GwCtx,
    x: &Parsed,
    rx: &RxMeta,
    out: &mut impl ReportSink,
    o: &mut ReportOutcome,
) {
    let prev = ctx.get(x.order_id);
    let (prev_cum, prev_value, prev_fee) = prev.map_or((0, None, 0), |e| {
        (e.cum, (e.value >= 0).then_some(e.value), e.fee)
    });
    let advanced = x.cum.0 > prev_cum;
    let terminal = matches!(x.st, St::Filled | St::Cancelled);
    ctx.put(
        x.order_id,
        x.cum.0.max(prev_cum),
        x.cum_value.map_or(-1, |v| v.0),
        x.cum_fee.0.max(prev_fee),
        terminal,
    );
    let what = match x.st {
        St::Open if advanced => OrderEventKind::PartialFill,
        St::Open => OrderEventKind::Ack,
        St::Partial if advanced => OrderEventKind::PartialFill,
        St::Filled if advanced => OrderEventKind::Fill,
        St::Partial | St::Filled => return,
        St::Cancelled if x.expired => OrderEventKind::Expired,
        St::Cancelled => OrderEventKind::CancelAck,
    };
    // This fill (when the cumulative quantity advanced).
    let (qty, px, fee) = if advanced {
        let d = Qty(x.cum.0 - prev_cum);
        let qty = x.fill_qty.unwrap_or(d);
        let px = x.fill_px.unwrap_or_else(|| match (x.cum_value, prev_value) {
            (Some(v), Some(pv)) if d.0 > 0 => {
                let h = rd.inst(x.inst);
                let exp = AssetExp {
                    px_exp: h.px_exp,
                    qty_exp: h.qty_exp,
                };
                Px::from_notional(Notional(v.0 - pv), d, exp, RoundMode::HalfEven).unwrap_or(x.avg_px)
            }
            (Some(v), None) if prev_cum == 0 && d.0 > 0 => {
                let h = rd.inst(x.inst);
                let exp = AssetExp {
                    px_exp: h.px_exp,
                    qty_exp: h.qty_exp,
                };
                Px::from_notional(v, d, exp, RoundMode::HalfEven).unwrap_or(x.avg_px)
            }
            _ => x.avg_px,
        });
        let fee = x.fill_fee.unwrap_or(Qty((x.cum_fee.0 - prev_fee).max(0)));
        (qty, px, fee)
    } else {
        (Qty(0), Px(0), Qty(0))
    };
    if fee.0 != 0 && x.fee_asset.0 != 0 && fee_deviates(line, rd, x.inst, x.side, x.fee_asset) {
        o.fee_rule_deviation = true;
    }
    let mut key_buf = [0u8; 20];
    let key = exch_key(x.order_id, &mut key_buf);
    // External fills: own-liquidation markers win over the client id (README "外部成交识别").
    let foreign = x.cause.is_some() || (x.cid.is_none() && !x.cid_empty);
    if foreign {
        if advanced {
            out.external_fill(&ExternalFill {
                venue: line.venue(),
                account: rx.account,
                side: x.side.unwrap_or(Side::Buy),
                cause: x.cause.unwrap_or(ExternalCause::Unknown),
                inst: x.inst,
                qty,
                px,
                fee,
                fee_asset: if fee.0 == 0 { AssetId(0) } else { x.fee_asset },
                _pad: [0; 6],
                exch_order_id: x.order_id,
                trade_id: x.trade_id,
                ex_ts_ns: x.ex_ts_ns,
                rx_tsc: Tsc(rx.rx_tsc),
                fill_seq: if x.trade_id == 0 { x.cum.0 as u64 } else { 0 },
                cash_qty: Qty(0),
                cash_amount: Qty(0),
            });
            o.events += 1;
        }
        return;
    }
    let (epoch, slot, r#gen) = match x.cid {
        Some(c) => (c.epoch, SlotId(c.slot), c.r#gen),
        // No client id: fills go to the runtime's IdTable (contract 18); other events are dropped.
        None => {
            if !advanced {
                return;
            }
            match out.exch_id(key, None) {
                ExchIdHint::Resolved { epoch, slot, r#gen } => (epoch, slot, r#gen),
                _ => (0, SlotId::UNRESOLVED, 0),
            }
        }
    };
    let mut e = blank(line, rx.account, rx);
    e.what = what;
    e.epoch = epoch;
    e.slot = slot;
    e.r#gen = r#gen;
    e.inst = x.inst;
    e.px = px;
    e.qty = qty;
    e.cum_filled = x.cum;
    e.avg_px = x.avg_px;
    e.fee = fee;
    e.fee_asset = if fee.0 == 0 { AssetId(0) } else { x.fee_asset };
    e.exch_order_id = x.order_id;
    e.ex_ts_ns = x.ex_ts_ns;
    e.trade_id = if advanced { x.trade_id } else { 0 };
    out.order(&e);
    o.events += 1;
}

// ---------------------------------------------------------------- position channel

#[derive(Deserialize)]
struct PosRow<'a> {
    #[serde(alias = "instId", borrow)]
    symbol: &'a str,
    #[serde(alias = "total", borrow)]
    size: &'a str,
    #[serde(rename = "posSide", alias = "holdSide", default, borrow)]
    pos_side: &'a str,
    #[serde(rename = "avgPrice", alias = "openPriceAvg", default, borrow)]
    avg_px: &'a str,
    #[serde(rename = "unrealisedPnl", alias = "unrealizedPL", default, borrow)]
    upnl: &'a str,
    #[serde(rename = "liqPrice", alias = "liquidationPrice", default, borrow)]
    liq_px: &'a str,
    #[serde(rename = "updatedTime", alias = "uTime", default)]
    updated: UInt,
}

#[derive(Deserialize)]
struct PosMsg<'a> {
    #[serde(borrow)]
    data: Arr<PosRow<'a>, POSITIONS_MAX>,
    #[serde(default)]
    ts: UInt,
}

fn positions(
    line: Line,
    rd: &RefData,
    p: &[u8],
    rx: &RxMeta,
    out: &mut impl ReportSink,
) -> Result<ReportOutcome, DecodeErr> {
    let m: PosMsg = from(p)?;
    if m.data.over {
        return Err(DecodeErr::Shape);
    }
    let mut parsed = [None; POSITIONS_MAX];
    if line == Line::Fut {
        for (slot, r) in parsed.iter_mut().zip(m.data.iter()) {
            let inst = inst_of(line, rd, r.symbol);
            if inst.0 == 0 {
                continue;
            }
            let h = rd.inst(inst);
            let size = fixed_or_zero(r.size, h.qty_wire_exp)?;
            let qty = match r.pos_side {
                "short" => size.checked_neg().ok_or(DecodeErr::Overflow)?,
                _ => size,
            };
            let upnl = if r.upnl.is_empty() {
                0
            } else {
                parse_round(r.upnl.as_bytes(), -6, RoundMode::HalfEven).map_err(fx)?
            };
            let ts = match r.updated.opt()? {
                Some(ms) => ms_ns(ms)?,
                None => m.ts.opt()?.map_or(Ok(0), ms_ns)?,
            };
            *slot = Some(PositionReport {
                venue: line.venue(),
                account: rx.account,
                _pad: 0,
                inst,
                qty: Qty(qty),
                entry_px: Px(fixed_or_zero(r.avg_px, h.px_wire_exp)?),
                upnl: Notional(upnl),
                liq_px: Px(fixed_or_zero(r.liq_px, h.px_wire_exp)?.max(0)),
                ex_ts_ns: ts,
                rx_tsc: Tsc(rx.rx_tsc),
            });
        }
    }
    let mut o = ReportOutcome::default();
    for e in parsed.iter().flatten() {
        out.position(e);
        o.events += 1;
    }
    Ok(o)
}

// ---------------------------------------------------------------- account channel

#[derive(Deserialize)]
struct CoinRow<'a> {
    #[serde(borrow)]
    coin: &'a str,
    #[serde(borrow)]
    available: &'a str,
    #[serde(alias = "frozen", default, borrow)]
    locked: &'a str,
}

#[derive(Deserialize)]
struct AcctRow<'a> {
    #[serde(borrow)]
    coin: Arr<CoinRow<'a>, COINS_MAX>,
}

#[derive(Deserialize)]
struct AcctMsg<'a> {
    #[serde(borrow)]
    data: One<AcctRow<'a>>,
    #[serde(default)]
    ts: UInt,
}

fn account(
    line: Line,
    rd: &RefData,
    p: &[u8],
    rx: &RxMeta,
    out: &mut impl ReportSink,
) -> Result<ReportOutcome, DecodeErr> {
    let m: AcctMsg = from(p)?;
    let coins = &m.data.0.coin;
    if coins.over {
        return Err(DecodeErr::Shape);
    }
    let ts = m.ts.opt()?.map_or(Ok(0), ms_ns)?;
    let mut parsed = [None; COINS_MAX];
    for (slot, c) in parsed.iter_mut().zip(coins.iter()) {
        let asset = asset_by_name(line, rd, c.coin);
        if asset.0 == 0 {
            continue;
        }
        let qe = rd.asset(asset).exp.qty_exp;
        *slot = Some(BalanceReport {
            venue: line.venue(),
            account: rx.account,
            asset,
            _pad: 0,
            free: Qty(fixed_or_zero(c.available, qe)?),
            locked: Qty(fixed_or_zero(c.locked, qe)?),
            ex_ts_ns: ts,
            rx_tsc: Tsc(rx.rx_tsc),
        });
    }
    let mut o = ReportOutcome::default();
    for e in parsed.iter().flatten() {
        out.balance(e);
        o.events += 1;
    }
    Ok(o)
}

// ---------------------------------------------------------------- REST order query

#[derive(Deserialize)]
struct QueryData<'a> {
    #[serde(rename = "orderId")]
    order_id: UInt,
    #[serde(rename = "clientOid", default, borrow)]
    client_oid: &'a str,
    #[serde(default, borrow)]
    symbol: &'a str,
    #[serde(rename = "orderStatus", alias = "status", borrow)]
    status: &'a str,
    #[serde(rename = "cumExecQty", alias = "baseVolume", borrow)]
    cum_qty: &'a str,
    #[serde(rename = "avgPrice", alias = "priceAvg", default, borrow)]
    avg_px: &'a str,
    #[serde(rename = "updatedTime", alias = "uTime", default)]
    updated: UInt,
}

#[derive(Deserialize)]
struct QueryResp<'a> {
    #[serde(default)]
    code: Code,
    #[serde(default, borrow)]
    data: Option<QueryData<'a>>,
}

/// Code envelope of an error body (any `data`).
#[derive(Deserialize)]
struct CodeOnly {
    #[serde(default)]
    code: Code,
}

pub(crate) fn decode_rest(
    line: Line,
    rd: &RefData,
    req: &RestOrigin,
    status: u16,
    body: &[u8],
    rx: &RxMeta,
    out: &mut impl ReportSink,
) -> Result<ReportOutcome, DecodeErr> {
    let r = match (req.what, req.req) {
        (RestWhat::Order, Some(r)) if r.op == GwOp::Query => r,
        _ => return Err(DecodeErr::UnknownClass),
    };
    let mut o = ReportOutcome {
        req: Some(r),
        ..ReportOutcome::default()
    };
    let code = from::<CodeOnly>(body).map_or(Code::Absent, |c| c.code);
    let ok = (200..300).contains(&status) && code.is_zero();
    let mut e = blank(line, rx.account, rx);
    e.what = OrderEventKind::QueryResult;
    e.slot = SlotId(r.slot);
    e.r#gen = r.r#gen;
    e.inst = req.inst;
    if !ok {
        if code.num() == Some(ORDER_GONE) {
            // "Order does not exist": NotFound, never a final state (no expiry guard, contract 10).
            e.detail = status::NOT_FOUND;
            out.order(&e);
            o.events = 1;
            return Ok(o);
        }
        o.raw_code = if code == Code::Absent {
            code.raw(status)
        } else {
            code.raw(0)
        };
        o.outcome_unknown = Some(r);
        if status == 429 || code.num() == Some(RATE_LIMITED) {
            o.ctl = ReportCtl::RateLimited;
        } else if code.num() == Some(CLOCK_SKEW) {
            o.ctl = ReportCtl::ClockSkew;
        }
        return Ok(o);
    }
    let m: QueryResp = from(body)?;
    let d = m.data.ok_or(DecodeErr::Shape)?;
    let _ = m.code;
    let inst = if d.symbol.is_empty() {
        req.inst
    } else {
        inst_of(line, rd, d.symbol)
    };
    if inst != req.inst {
        return Err(DecodeErr::Shape);
    }
    if inst.0 == 0 || rd.inst(inst).venue != line.venue() {
        return Err(DecodeErr::Shape);
    }
    let h = rd.inst(inst);
    e.inst = inst;
    e.detail = match status_of(d.status)? {
        St::Open => status::NEW,
        St::Partial => status::PARTIALLY_FILLED,
        St::Filled => status::FILLED,
        St::Cancelled => status::CANCELLED,
    };
    e.cum_filled = Qty(fixed_or_zero(d.cum_qty, h.qty_wire_exp)?);
    e.avg_px = Px(fixed_or_zero(d.avg_px, h.px_wire_exp)?);
    if e.cum_filled.0 < 0 || e.avg_px.0 < 0 || (e.cum_filled.0 > 0 && e.avg_px.0 == 0) {
        return Err(DecodeErr::Shape);
    }
    e.exch_order_id = d.order_id.get()?;
    if !d.client_oid.is_empty() {
        let c = parse_cid(d.client_oid.as_bytes()).ok_or(DecodeErr::Shape)?;
        if c.venue != line.venue() || c.account != rx.account || c.slot != r.slot || c.r#gen != r.r#gen {
            return Err(DecodeErr::Shape);
        }
        e.epoch = c.epoch;
    }
    e.ex_ts_ns = d.updated.opt()?.map_or(Ok(0), ms_ns)?;
    out.order(&e);
    o.events = 1;
    Ok(o)
}
