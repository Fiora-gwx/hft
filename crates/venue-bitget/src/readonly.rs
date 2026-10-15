//! UTA V3 read-only account observations, staged atomically before sink publication.
use crate::{Line, Templates, json::from};
use hft_event::{OrderEvent, OrderEventKind};
use hft_refdata::RefData;
use hft_types::*;
use hft_venue_proto::{
    account::{AccountProtocol, Query, State, asset, average_price, decimal, instrument},
    *,
};
use serde::Deserialize;

#[allow(clippy::too_many_arguments)] // Explicit borrowed credentials keep templates secret-free.
pub(crate) fn request(
    line: Line,
    rd: &RefData,
    t: &Templates,
    cred: &Credential,
    a: AccountId,
    q: &RoQuery,
    clk: &WallClock,
    out: &mut RestReqBuf,
) -> Result<(), EncodeErr> {
    *out = RestReqBuf::default();
    if a != t.account || q.since_ms < 0 || q.limit > 100 {
        return Err(EncodeErr::Unsupported(hft_event::GwOp::Query));
    }
    if q.inst.0 != 0 && rd.inst(q.inst).venue != line.venue() {
        return Err(EncodeErr::UnknownInst);
    }
    let category = if line == Line::Fut { "USDT-FUTURES" } else { "SPOT" };
    let limit = if q.limit == 0 { 100 } else { q.limit };
    let cursor = if q.cursor == 0 {
        String::new()
    } else {
        format!("&cursor={}", q.cursor)
    };
    let path = match q.kind {
        RoKind::OpenOrders => {
            format!("/api/v3/trade/unfilled-orders?category={category}&limit={limit}{cursor}")
        }
        RoKind::Fills => format!(
            "/api/v3/trade/fills?category={category}&startTime={}&limit={limit}{cursor}",
            q.since_ms
        ),
        RoKind::Balance if q.cursor == 0 => "/api/v3/account/assets".into(),
        RoKind::Positions if line == Line::Fut && q.cursor == 0 => {
            format!("/api/v3/position/current-position?category={category}")
        }
        _ => return Err(EncodeErr::Unsupported(hft_event::GwOp::Query)),
    };
    out.path_query
        .push(path.as_bytes())
        .map_err(|_| EncodeErr::FieldOverflow)?;
    crate::orders::sign_headers(t, cred, clk, out).map_err(|_| EncodeErr::FieldOverflow)
}
#[derive(Deserialize)]
struct Envelope<T> {
    code: String,
    data: T,
}
#[derive(Deserialize)]
#[serde(bound(deserialize = "T: serde::de::DeserializeOwned"))]
struct Page<T> {
    #[serde(deserialize_with = "crate::json::nullable_list")]
    list: Vec<T>,
    #[serde(default)]
    cursor: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Open {
    symbol: String,
    order_id: String,
    client_oid: String,
    order_status: String,
    category: String,
    price: String,
    qty: String,
    cum_exec_qty: String,
    avg_price: String,
    updated_time: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fee {
    fee_coin: String,
    fee: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fill {
    symbol: String,
    category: String,
    exec_id: String,
    order_id: String,
    #[serde(default)]
    client_oid: String,
    side: String,
    exec_price: String,
    exec_qty: String,
    created_time: String,
    fee_detail: Vec<Fee>,
    #[serde(default)]
    trade_side: String,
}
fn rx(a: AccountId) -> RxMeta {
    RxMeta {
        rx_tsc: 0,
        hw_rx_ns: 0,
        sw_rx_ns: 0,
        prev_override: 0,
        conn: ConnId(0),
        account: a,
        _pad: [0; 2],
        ts_flags: 0,
        rx_empty_gap_ns: 0,
        _pad2: 0,
    }
}
fn page<T: serde::de::DeserializeOwned>(body: &[u8], q: &RoQuery) -> Result<(Vec<T>, u64), DecodeErr> {
    let e: Envelope<Page<T>> = from(body)?;
    if e.code != "00000" {
        return Err(DecodeErr::Shape);
    }
    let limit = if q.limit == 0 { 100 } else { q.limit } as usize;
    if e.data.list.len() > limit {
        return Err(DecodeErr::Shape);
    }
    let cursor = e.data.cursor.as_deref().unwrap_or("");
    let c = if cursor.is_empty() {
        0
    } else {
        cursor.parse::<u64>().map_err(|_| DecodeErr::Number)?
    };
    if c != 0 && (c == q.cursor || e.data.list.is_empty()) {
        return Err(DecodeErr::Shape);
    }
    Ok((e.data.list, c))
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn decode<P: AccountProtocol>(
    line: Line,
    rd: &RefData,
    a: AccountId,
    q: &RoQuery,
    status: u16,
    body: &[u8],
    out: &mut impl RoSink,
) -> Result<RoPage, DecodeErr> {
    if !(200..300).contains(&status)
        || q.limit > 100
        || (q.inst.0 != 0 && rd.inst(q.inst).venue != line.venue())
    {
        return Err(DecodeErr::Shape);
    }
    match q.kind {
        RoKind::Balance | RoKind::Positions => {
            if q.kind == RoKind::Positions && line == Line::Spot {
                return Err(DecodeErr::UnknownClass);
            }
            let mut s = State::default();
            P::account_decode(
                rd,
                a,
                &if q.kind == RoKind::Balance {
                    Query::Balances
                } else {
                    Query::Positions
                },
                status,
                body,
                0,
                &mut s,
            )?;
            let mut rows = 0;
            for b in &s.balances {
                out.balance(b);
                rows += 1;
            }
            for p in s.positions.iter().filter(|p| q.inst.0 == 0 || p.inst == q.inst) {
                out.position(p);
                rows += 1;
            }
            Ok(RoPage { rows, next_cursor: 0 })
        }
        RoKind::OpenOrders => {
            let (rows, next_cursor) = page::<Open>(body, q)?;
            let mut events = Vec::<OrderEvent>::with_capacity(rows.len());
            for r in rows {
                if !line.is_category(&r.category) {
                    return Err(DecodeErr::Shape);
                }
                let i = instrument(rd, line.venue(), &r.symbol)?;
                if q.inst.0 != 0 && i != q.inst {
                    continue;
                }
                let h = rd.inst(i);
                let mut e = crate::reports::blank(line, a, &rx(a));
                e.what = OrderEventKind::QueryResult;
                e.inst = i;
                e.exch_order_id = r.order_id.parse().map_err(|_| DecodeErr::Number)?;
                e.detail = match r.order_status.as_str() {
                    "live" | "new" | "init" => 1,
                    "partially_filled" | "partial-fill" => 2,
                    _ => return Err(DecodeErr::Shape),
                };
                e.px = Px(decimal(&r.price, h.px_wire_exp)?);
                e.qty = Qty(decimal(&r.qty, h.qty_wire_exp)?);
                e.cum_filled = Qty(decimal(&r.cum_exec_qty, h.qty_wire_exp)?);
                e.avg_px = if e.cum_filled.0 == 0 {
                    Px(0)
                } else {
                    Px(average_price(&r.avg_price, h.px_wire_exp)?)
                };
                e.ex_ts_ns = r
                    .updated_time
                    .parse::<i64>()
                    .map_err(|_| DecodeErr::Number)?
                    .checked_mul(1_000_000)
                    .ok_or(DecodeErr::Overflow)?;
                if e.exch_order_id == 0
                    || e.qty.0 <= 0
                    || e.cum_filled.0 < 0
                    || e.cum_filled > e.qty
                    || (e.cum_filled.0 > 0 && e.avg_px.0 <= 0)
                {
                    return Err(DecodeErr::Shape);
                }
                if let Some(c) = crate::cid::parse_cid(r.client_oid.as_bytes())
                    .filter(|c| c.venue == line.venue() && c.account == a)
                {
                    e.slot = SlotId(c.slot);
                    e.r#gen = c.r#gen;
                    e.epoch = c.epoch;
                }
                if events
                    .iter()
                    .any(|o| o.inst == i && o.exch_order_id == e.exch_order_id)
                {
                    return Err(DecodeErr::Shape);
                }
                events.push(e);
            }
            for e in &events {
                out.open_order(e);
            }
            Ok(RoPage {
                rows: events.len() as u32,
                next_cursor,
            })
        }
        RoKind::Fills => {
            let (rows, next_cursor) = page::<Fill>(body, q)?;
            let mut fills = Vec::<FillRow>::with_capacity(rows.len());
            for r in rows {
                if !line.is_category(&r.category) {
                    return Err(DecodeErr::Shape);
                }
                let i = instrument(rd, line.venue(), &r.symbol)?;
                if q.inst.0 != 0 && i != q.inst {
                    continue;
                }
                let h = rd.inst(i);
                let first = r.fee_detail.first().ok_or(DecodeErr::Shape)?;
                let fee_asset = asset(rd, &first.fee_coin).ok_or(DecodeErr::Shape)?;
                let mut fee = 0i64;
                for item in &r.fee_detail {
                    if item.fee_coin != first.fee_coin {
                        return Err(DecodeErr::Shape);
                    }
                    fee = fee
                        .checked_add(decimal(&item.fee, rd.asset(fee_asset).exp.qty_exp)?)
                        .ok_or(DecodeErr::Overflow)?;
                }
                let ms = r.created_time.parse::<i64>().map_err(|_| DecodeErr::Number)?;
                let f = FillRow {
                    inst: i,
                    side: match r.side.as_str() {
                        "buy" => Side::Buy,
                        "sell" => Side::Sell,
                        _ => return Err(DecodeErr::Shape),
                    },
                    qty: Qty(decimal(&r.exec_qty, h.qty_wire_exp)?),
                    px: Px(decimal(&r.exec_price, h.px_wire_exp)?),
                    fee: Qty(fee),
                    fee_asset,
                    exch_order_id: r.order_id.parse().map_err(|_| DecodeErr::Number)?,
                    trade_id: r.exec_id.parse().map_err(|_| DecodeErr::Number)?,
                    ex_ts_ns: ms.checked_mul(1_000_000).ok_or(DecodeErr::Overflow)?,
                    cid: crate::cid::parse_cid(r.client_oid.as_bytes())
                        .filter(|c| c.venue == line.venue() && c.account == a),
                    cause: crate::reports::marker(&r.trade_side, ""),
                };
                if f.qty.0 <= 0
                    || f.px.0 <= 0
                    || f.exch_order_id == 0
                    || ms < 0
                    || fills
                        .iter()
                        .any(|old| old.inst == i && old.trade_id == f.trade_id)
                {
                    return Err(DecodeErr::Shape);
                }
                fills.push(f);
            }
            for f in &fills {
                out.fill(f);
            }
            Ok(RoPage {
                rows: fills.len() as u32,
                next_cursor,
            })
        }
        RoKind::FundingFlow => Err(DecodeErr::UnknownClass),
    }
}
