//! Order frames of the WS API (README "下单:WS API", "4. 订单帧模板").

use hft_event::{GwCmd, GwOp, OrderFlags};
use hft_refdata::RefData;
use hft_types::{AccountId, Side, Tif};
use hft_venue_proto::{Credential, EncodeErr, OrderChannel, ProtoErr, TargetRef, VenueCfg, WallClock};

use crate::Line;
use crate::cid::encode_req_id;
use crate::wr::Wr;

/// Frame state of one (venue, account): the expiry window. The REST signing credential joins it
/// with the signed REST operations (M3); the M1.5 build stores no key material at all.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Templates {
    pub account: AccountId,
    /// `recvWindow` = `gw.stale_guard_ms`.
    pub recv_window_ms: u32,
}

pub(crate) fn build_templates(
    _line: Line,
    cfg: &VenueCfg,
    _rd: &RefData,
    acct: AccountId,
    _rest: &Credential,
) -> Result<Templates, ProtoErr> {
    if cfg.stale_guard_ms == 0 {
        return Err(ProtoErr::Cfg);
    }
    Ok(Templates {
        account: acct,
        recv_window_ms: cfg.stale_guard_ms,
    })
}

pub(crate) fn order_channel(line: Line, op: GwOp) -> OrderChannel {
    match (line, op) {
        // USDT-M cancel-all is REST `DELETE /fapi/v1/allOpenOrders` (README "下单:WS API").
        (Line::Usdm, GwOp::CancelAll) => OrderChannel::Rest,
        _ => OrderChannel::Ws,
    }
}

fn side(s: Side) -> &'static [u8] {
    match s {
        Side::Buy => b"BUY",
        Side::Sell => b"SELL",
    }
}

/// `"id":"<req>","method":"<m>","params":{"symbol":"<s>"` prefix.
fn head(w: &mut Wr<'_>, req: &[u8; 17], method: &[u8], sym: &[u8]) {
    w.bytes(b"{\"id\":\"")
        .bytes(req)
        .bytes(b"\",\"method\":\"")
        .bytes(method)
        .bytes(b"\",\"params\":{\"symbol\":\"")
        .bytes(sym)
        .byte(b'"');
}

fn tail(w: &mut Wr<'_>, clk: &WallClock, t: &Templates) {
    w.bytes(b",\"timestamp\":")
        .i64(clk.unix_ms)
        .bytes(b",\"recvWindow\":")
        .u64(u64::from(t.recv_window_ms))
        .bytes(b"}}");
}

/// The target's client id in canonical text.
fn cid_text(tgt: &TargetRef) -> [u8; 24] {
    let mut c = [0u8; 24];
    tgt.id.encode(&mut c);
    c
}

pub(crate) fn encode(
    line: Line,
    rd: &RefData,
    t: &Templates,
    c: &GwCmd,
    tgt: &TargetRef,
    clk: &WallClock,
    body: &mut [u8],
) -> Result<usize, EncodeErr> {
    let is_order = matches!(
        c.op,
        GwOp::Place | GwOp::Cancel | GwOp::Amend | GwOp::Query | GwOp::CancelAll
    );
    if !is_order || order_channel(line, c.op) != OrderChannel::Ws {
        return Err(EncodeErr::Unsupported(c.op));
    }
    if c.op == GwOp::CancelAll && c.inst.0 == 0 {
        return Err(EncodeErr::Unsupported(GwOp::CancelAll));
    }
    let row = rd.inst(c.inst);
    if c.inst.0 == 0 || row.venue != line.venue() {
        return Err(EncodeErr::UnknownInst);
    }
    let sym = rd.inst_meta(c.inst).symbol.as_bytes();
    let flags = OrderFlags(c.oflags);
    let mut w = Wr::new(body);
    match c.op {
        GwOp::Place => {
            if line == Line::Spot && flags.contains(OrderFlags::REDUCE_ONLY) {
                return Err(EncodeErr::Unsupported(GwOp::Place));
            }
            let req = encode_req_id(GwOp::Place, c.slot.0, c.r#gen).ok_or(EncodeErr::Unsupported(c.op))?;
            // For a Place the gw hands over the new order's own client id.
            let cid = cid_text(tgt);
            head(&mut w, &req, b"order.place", sym);
            w.bytes(b",\"side\":\"").bytes(side(c.side)).byte(b'"');
            match (line, c.tif) {
                (Line::Spot, Tif::PostOnly) => {
                    w.bytes(b",\"type\":\"LIMIT_MAKER\"");
                }
                (_, tif) => {
                    let tif: &[u8] = match tif {
                        Tif::Gtc => b"GTC",
                        Tif::Ioc => b"IOC",
                        Tif::Fok => b"FOK",
                        Tif::PostOnly => b"GTX",
                    };
                    w.bytes(b",\"type\":\"LIMIT\",\"timeInForce\":\"")
                        .bytes(tif)
                        .byte(b'"');
                }
            }
            w.bytes(b",\"price\":\"")
                .fixed(c.px.0, row.px_wire_exp)
                .bytes(b"\",\"quantity\":\"")
                .fixed(c.qty.0, row.qty_wire_exp)
                .byte(b'"');
            if line == Line::Usdm && flags.contains(OrderFlags::REDUCE_ONLY) {
                w.bytes(b",\"reduceOnly\":\"true\"");
            }
            w.bytes(b",\"newClientOrderId\":\"")
                .bytes(&cid)
                .bytes(b"\",\"newOrderRespType\":\"ACK\"");
            if line == Line::Spot {
                w.bytes(b",\"selfTradePreventionMode\":\"EXPIRE_MAKER\"");
            }
        }
        GwOp::Cancel | GwOp::Query => {
            let (op, method): (GwOp, &[u8]) = if c.op == GwOp::Cancel {
                (GwOp::Cancel, b"order.cancel")
            } else {
                (GwOp::Query, b"order.status")
            };
            let req = encode_req_id(op, tgt.id.slot, tgt.id.r#gen).ok_or(EncodeErr::Unsupported(c.op))?;
            head(&mut w, &req, method, sym);
            w.bytes(b",\"origClientOrderId\":\"")
                .bytes(&cid_text(tgt))
                .byte(b'"');
        }
        GwOp::Amend => {
            let req =
                encode_req_id(GwOp::Amend, tgt.id.slot, tgt.id.r#gen).ok_or(EncodeErr::Unsupported(c.op))?;
            match line {
                Line::Usdm => {
                    head(&mut w, &req, b"order.modify", sym);
                    w.bytes(b",\"side\":\"")
                        .bytes(side(c.side))
                        .bytes(b"\",\"origClientOrderId\":\"")
                        .bytes(&cid_text(tgt))
                        .bytes(b"\",\"price\":\"")
                        .fixed(c.px.0, row.px_wire_exp)
                        .bytes(b"\",\"quantity\":\"")
                        .fixed(c.qty.0, row.qty_wire_exp)
                        .byte(b'"');
                }
                Line::Spot => {
                    head(&mut w, &req, b"order.amend.keepPriority", sym);
                    w.bytes(b",\"origClientOrderId\":\"")
                        .bytes(&cid_text(tgt))
                        .bytes(b"\",\"newQty\":\"")
                        .fixed(c.qty.0, row.qty_wire_exp)
                        .byte(b'"');
                }
            }
        }
        GwOp::CancelAll => {
            // Spot only (USDT-M goes through REST, refused above).
            let req = encode_req_id(GwOp::CancelAll, 0, 0).ok_or(EncodeErr::Unsupported(c.op))?;
            head(&mut w, &req, b"openOrders.cancelAll", sym);
        }
        _ => return Err(EncodeErr::Unsupported(c.op)),
    }
    tail(&mut w, clk, t);
    w.finish().ok_or(EncodeErr::FieldOverflow)
}
