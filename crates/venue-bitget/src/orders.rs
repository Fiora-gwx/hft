//! UTA V3 WS trade frames (`op = trade`: `place-order`, `cancel-order`, `modify-order`) and the
//! signed REST order query (README "下单:UTA V3 WS", "实现说明" 8).

use hft_event::{GwCmd, GwOp, OrderFlags};
use hft_refdata::RefData;
use hft_types::{AccountId, Side, Tif};
use hft_venue_proto::{
    Credential, EncodeErr, Method, OrderChannel, ProtoErr, RestReqBuf, SignAlg, TargetRef, VenueCfg,
    VenueEnv, WallClock,
};

use crate::Line;
use crate::cid::encode_req_id;
use crate::sign::rest_sign;
use crate::wr::Wr;

/// Frame state of one (venue, account): the REST signing credential (contract 7; `Debug` never
/// prints it) and whether requests go to demo trading.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Templates {
    pub account: AccountId,
    /// REST requests carry `paptrading: 1` (README "测试网/模拟盘").
    pub demo: bool,
    pub(crate) rest: Credential,
}

pub(crate) fn build_templates(
    cfg: &VenueCfg,
    acct: AccountId,
    rest: &Credential,
) -> Result<Templates, ProtoErr> {
    if rest.alg != SignAlg::HmacSha256 || rest.api_key.is_empty() || rest.secret.0.is_empty() {
        return Err(ProtoErr::Cfg);
    }
    Ok(Templates {
        account: acct,
        demo: cfg.env == VenueEnv::Testnet,
        rest: *rest,
    })
}

pub(crate) fn order_channel(op: GwOp) -> OrderChannel {
    match op {
        // Order details are a REST query (README "下单:UTA V3 WS").
        GwOp::Query => OrderChannel::Rest,
        _ => OrderChannel::Ws,
    }
}

fn side(s: Side) -> &'static [u8] {
    match s {
        Side::Buy => b"buy",
        Side::Sell => b"sell",
    }
}

fn tif(t: Tif) -> &'static [u8] {
    match t {
        Tif::Gtc => b"gtc",
        Tif::Ioc => b"ioc",
        Tif::Fok => b"fok",
        Tif::PostOnly => b"post_only",
    }
}

/// `{"op":"trade","id":"<req>","category":"<c>","topic":"<t>","args":[{` prefix.
fn head(w: &mut Wr<'_>, line: Line, req: &[u8; 17], topic: &[u8]) {
    w.bytes(b"{\"op\":\"trade\",\"id\":\"")
        .bytes(req)
        .bytes(b"\",\"category\":\"")
        .bytes(line.ws_type())
        .bytes(b"\",\"topic\":\"")
        .bytes(topic)
        .bytes(b"\",\"args\":[{");
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
    _t: &Templates,
    c: &GwCmd,
    tgt: &TargetRef,
    _clk: &WallClock,
    body: &mut [u8],
) -> Result<usize, EncodeErr> {
    if !matches!(c.op, GwOp::Place | GwOp::Cancel | GwOp::Amend) {
        return Err(EncodeErr::Unsupported(c.op));
    }
    let row = rd.inst(c.inst);
    if c.inst.0 == 0 || row.venue != line.venue() {
        return Err(EncodeErr::UnknownInst);
    }
    let sym = rd.inst_meta(c.inst).symbol.as_bytes();
    let flags = OrderFlags(c.oflags);
    if line == Line::Spot && flags.contains(OrderFlags::REDUCE_ONLY) {
        return Err(EncodeErr::Unsupported(c.op));
    }
    let mut w = Wr::new(body);
    match c.op {
        GwOp::Place => {
            let req = encode_req_id(GwOp::Place, c.slot.0, c.r#gen).ok_or(EncodeErr::Unsupported(c.op))?;
            head(&mut w, line, &req, b"place-order");
            w.bytes(b"\"symbol\":\"")
                .bytes(sym)
                .bytes(b"\",\"side\":\"")
                .bytes(side(c.side))
                .bytes(b"\",\"orderType\":\"limit\",\"price\":\"")
                .fixed(c.px.0, row.px_wire_exp)
                .bytes(b"\",\"qty\":\"")
                .fixed(c.qty.0, row.qty_wire_exp)
                .bytes(b"\",\"timeInForce\":\"")
                .bytes(tif(c.tif))
                // For a Place the gw hands over the new order's own client id.
                .bytes(b"\",\"clientOid\":\"")
                .bytes(&cid_text(tgt))
                .byte(b'"');
            if flags.contains(OrderFlags::REDUCE_ONLY) {
                w.bytes(b",\"reduceOnly\":\"YES\"");
            }
        }
        GwOp::Cancel => {
            let req =
                encode_req_id(GwOp::Cancel, tgt.id.slot, tgt.id.r#gen).ok_or(EncodeErr::Unsupported(c.op))?;
            head(&mut w, line, &req, b"cancel-order");
            w.bytes(b"\"clientOid\":\"").bytes(&cid_text(tgt)).byte(b'"');
        }
        GwOp::Amend => {
            let req =
                encode_req_id(GwOp::Amend, tgt.id.slot, tgt.id.r#gen).ok_or(EncodeErr::Unsupported(c.op))?;
            head(&mut w, line, &req, b"modify-order");
            w.bytes(b"\"symbol\":\"")
                .bytes(sym)
                .bytes(b"\",\"clientOid\":\"")
                .bytes(&cid_text(tgt))
                .bytes(b"\",\"price\":\"")
                .fixed(c.px.0, row.px_wire_exp)
                .bytes(b"\",\"qty\":\"")
                .fixed(c.qty.0, row.qty_wire_exp)
                .bytes(b"\",\"autoCancel\":\"no\"");
        }
        _ => return Err(EncodeErr::Unsupported(c.op)),
    }
    w.bytes(b"}]}");
    w.finish().ok_or(EncodeErr::FieldOverflow)
}

/// Order-details path (official UTA REST).
pub(crate) const ORDER_INFO: &[u8] = b"/api/v3/trade/order-info";

/// Signed REST headers: `ACCESS-KEY`, `ACCESS-SIGN`, `ACCESS-TIMESTAMP`, `ACCESS-PASSPHRASE`,
/// `locale`, and `paptrading: 1` on demo trading.
pub(crate) fn sign_headers(t: &Templates, clk: &WallClock, out: &mut RestReqBuf) -> Result<(), ProtoErr> {
    let mut ts = [0u8; 24];
    let mut w = Wr::new(&mut ts);
    w.i64(clk.unix_ms);
    let n = w.finish().ok_or(ProtoErr::BufFull)?;
    let ts = ts.get(..n).unwrap_or(&[]);
    let method: &[u8] = match out.method {
        Method::Get => b"GET",
        Method::Post => b"POST",
        Method::Put => b"PUT",
        Method::Delete => b"DELETE",
    };
    let sig = rest_sign(
        t.rest.secret.0.as_slice(),
        ts,
        method,
        out.path_query.as_slice(),
        out.body.as_slice(),
    )
    .ok_or(ProtoErr::Cfg)?;
    let h = &mut out.headers;
    h.clear();
    h.push(b"ACCESS-KEY: ")?;
    h.push(t.rest.api_key.as_slice())?;
    h.push(b"\r\nACCESS-SIGN: ")?;
    h.push(&sig)?;
    h.push(b"\r\nACCESS-TIMESTAMP: ")?;
    h.push(ts)?;
    h.push(b"\r\nACCESS-PASSPHRASE: ")?;
    h.push(t.rest.passphrase.as_slice())?;
    h.push(b"\r\nlocale: en-US\r\n")?;
    if t.demo {
        h.push(b"paptrading: 1\r\n")?;
    }
    out.redacted = true;
    Ok(())
}

/// `Query` → `GET /api/v3/trade/order-info?clientOid=<cid>` (signed).
pub(crate) fn encode_rest(
    line: Line,
    rd: &RefData,
    t: &Templates,
    c: &GwCmd,
    tgt: &TargetRef,
    clk: &WallClock,
    out: &mut RestReqBuf,
) -> Result<(), EncodeErr> {
    if c.op != GwOp::Query {
        return Err(EncodeErr::Unsupported(c.op));
    }
    if c.inst.0 == 0 || rd.inst(c.inst).venue != line.venue() {
        return Err(EncodeErr::UnknownInst);
    }
    out.endpoint = 0;
    out.method = Method::Get;
    out.path_query.clear();
    out.body.clear();
    let ovf = |_| EncodeErr::FieldOverflow;
    out.path_query.push(ORDER_INFO).map_err(ovf)?;
    out.path_query.push(b"?clientOid=").map_err(ovf)?;
    out.path_query.push(&cid_text(tgt)).map_err(ovf)?;
    sign_headers(t, clk, out).map_err(|e| match e {
        ProtoErr::Cfg => EncodeErr::Unsupported(c.op),
        _ => EncodeErr::FieldOverflow,
    })
}
