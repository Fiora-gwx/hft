//! Signed, stateless warm reconciliation reads; no partial page reaches the sink.
use crate::{Line, Templates, json::from};
use hft_event::{OrderEvent, OrderEventKind};
use hft_refdata::RefData;
use hft_types::*;
use hft_venue_proto::{
    account::{
        AccountProtocol, Query, State, asset, average_price, decimal, instrument, notional, safe_token,
    },
    *,
};
use serde::Deserialize;

pub(crate) fn request(
    line: Line,
    rd: &RefData,
    t: &Templates,
    a: AccountId,
    q: &RoQuery,
    clk: &WallClock,
    out: &mut RestReqBuf,
) -> Result<(), EncodeErr> {
    *out = RestReqBuf::default();
    if a != t.account || q.since_ms < 0 || q.limit > 1000 {
        return Err(EncodeErr::Unsupported(hft_event::GwOp::Query));
    }
    let sym = if q.inst.0 != 0 {
        if rd.inst(q.inst).venue != line.venue() {
            return Err(EncodeErr::UnknownInst);
        }
        let s = rd.inst_meta(q.inst).symbol.as_ref();
        if !safe_token(s) {
            return Err(EncodeErr::UnknownInst);
        }
        Some(s)
    } else {
        None
    };
    let fut = line == Line::Usdm;
    let limit = if q.limit == 0 { 500 } else { q.limit };
    let path = match q.kind {
        RoKind::OpenOrders if q.cursor == 0 => format!(
            "{}/openOrders{}",
            if fut { "/fapi/v1" } else { "/api/v3" },
            sym.map_or(String::new(), |s| format!("?symbol={s}"))
        ),
        RoKind::Fills => {
            let s = sym.ok_or(EncodeErr::Unsupported(hft_event::GwOp::Query))?;
            let paging = if q.cursor == 0 {
                format!("startTime={}", q.since_ms)
            } else {
                format!("fromId={}", q.cursor)
            };
            format!(
                "{}?symbol={s}&{paging}&limit={limit}",
                if fut {
                    "/fapi/v1/userTrades"
                } else {
                    "/api/v3/myTrades"
                }
            )
        }
        RoKind::Balance if q.cursor == 0 => if fut {
            "/fapi/v3/account"
        } else {
            "/api/v3/account"
        }
        .into(),
        RoKind::Positions if fut && q.cursor == 0 => "/fapi/v3/positionRisk".into(),
        _ => return Err(EncodeErr::Unsupported(hft_event::GwOp::Query)),
    };
    out.path_query
        .push(path.as_bytes())
        .map_err(|_| EncodeErr::FieldOverflow)?;
    crate::account::sign_request(
        &VenueCfg {
            env: VenueEnv::Prod,
            stale_guard_ms: t.recv_window_ms,
        },
        &t.rest,
        clk.unix_ms,
        out,
    )
    .map_err(|_| EncodeErr::FieldOverflow)
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Open {
    symbol: String,
    order_id: u64,
    client_order_id: String,
    status: String,
    price: String,
    orig_qty: String,
    executed_qty: String,
    #[serde(default)]
    avg_price: String,
    #[serde(default)]
    cummulative_quote_qty: String,
    #[serde(default)]
    update_time: i64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fill {
    symbol: String,
    id: u64,
    order_id: u64,
    price: String,
    qty: String,
    commission: String,
    commission_asset: String,
    time: i64,
    #[serde(default)]
    side: String,
    #[serde(default)]
    is_buyer: Option<bool>,
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
        || q.limit > 1000
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
            let rows: Vec<Open> = from(body)?;
            let mut events: Vec<OrderEvent> = Vec::with_capacity(rows.len());
            for r in rows {
                let i = instrument(rd, line.venue(), &r.symbol)?;
                if q.inst.0 != 0 && i != q.inst {
                    return Err(DecodeErr::Shape);
                }
                let h = rd.inst(i);
                let mut e = crate::reports::blank(line, a, &rx(a));
                e.what = OrderEventKind::QueryResult;
                e.inst = i;
                e.exch_order_id = r.order_id;
                e.detail = match r.status.as_str() {
                    "NEW" => 1,
                    "PARTIALLY_FILLED" => 2,
                    _ => return Err(DecodeErr::Shape),
                };
                e.px = Px(decimal(&r.price, h.px_wire_exp)?);
                e.qty = Qty(decimal(&r.orig_qty, h.qty_wire_exp)?);
                e.cum_filled = Qty(decimal(&r.executed_qty, h.qty_wire_exp)?);
                e.avg_px = if e.cum_filled.0 == 0 {
                    Px(0)
                } else if line == Line::Usdm {
                    Px(average_price(&r.avg_price, h.px_wire_exp)?)
                } else {
                    Px::from_notional(
                        Notional(notional(&r.cummulative_quote_qty)?),
                        e.cum_filled,
                        AssetExp {
                            px_exp: h.px_exp,
                            qty_exp: h.qty_exp,
                        },
                        RoundMode::HalfEven,
                    )
                    .map_err(|_| DecodeErr::Number)?
                };
                e.ex_ts_ns = r.update_time.checked_mul(1_000_000).ok_or(DecodeErr::Overflow)?;
                if e.exch_order_id == 0
                    || e.qty.0 <= 0
                    || e.cum_filled.0 < 0
                    || e.cum_filled > e.qty
                    || (e.cum_filled.0 > 0 && e.avg_px.0 <= 0)
                {
                    return Err(DecodeErr::Shape);
                }
                if let Some(c) = crate::cid::parse_cid(r.client_order_id.as_bytes())
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
                next_cursor: 0,
            })
        }
        RoKind::Fills => {
            if q.inst.0 == 0 {
                return Err(DecodeErr::Shape);
            }
            let rows: Vec<Fill> = from(body)?;
            let limit = if q.limit == 0 { 500 } else { q.limit } as usize;
            if rows.len() > limit {
                return Err(DecodeErr::Shape);
            }
            let full = rows.len() == limit;
            let mut fills = Vec::<FillRow>::with_capacity(rows.len());
            for r in rows {
                let i = instrument(rd, line.venue(), &r.symbol)?;
                if i != q.inst || r.id < q.cursor || fills.iter().any(|f| f.trade_id == r.id) {
                    return Err(DecodeErr::Shape);
                }
                let h = rd.inst(i);
                let fee_asset = asset(rd, &r.commission_asset).ok_or(DecodeErr::Shape)?;
                let side = if line == Line::Usdm {
                    match r.side.as_str() {
                        "BUY" => Side::Buy,
                        "SELL" => Side::Sell,
                        _ => return Err(DecodeErr::Shape),
                    }
                } else {
                    if r.is_buyer.ok_or(DecodeErr::Shape)? {
                        Side::Buy
                    } else {
                        Side::Sell
                    }
                };
                let f = FillRow {
                    inst: i,
                    side,
                    qty: Qty(decimal(&r.qty, h.qty_wire_exp)?),
                    px: Px(decimal(&r.price, h.px_wire_exp)?),
                    fee: Qty(decimal(&r.commission, rd.asset(fee_asset).exp.qty_exp)?),
                    fee_asset,
                    exch_order_id: r.order_id,
                    trade_id: r.id,
                    ex_ts_ns: r.time.checked_mul(1_000_000).ok_or(DecodeErr::Overflow)?,
                    cid: None,
                    cause: None,
                };
                if f.qty.0 <= 0 || f.px.0 <= 0 || f.exch_order_id == 0 || r.time < 0 {
                    return Err(DecodeErr::Shape);
                }
                fills.push(f);
            }
            let next = if full {
                fills
                    .iter()
                    .map(|f| f.trade_id)
                    .max()
                    .ok_or(DecodeErr::Shape)?
                    .checked_add(1)
                    .ok_or(DecodeErr::Overflow)?
            } else {
                0
            };
            for f in &fills {
                out.fill(f);
            }
            Ok(RoPage {
                rows: fills.len() as u32,
                next_cursor: next,
            })
        }
        RoKind::FundingFlow => Err(DecodeErr::UnknownClass),
    }
}
