//! UTA public orderbook refresh; preserve decimal tokens, including JSON numbers, without f64.
use crate::{
    Line,
    json::{Code, Raw, UInt, from, levels, ms_ns},
};
use hft_event::{Bbo, Level, MarketHdr};
use hft_refdata::RefData;
use hft_types::{InstId, Px, Qty};
use hft_venue_proto::{DecodeErr, MdSink, ProtoErr, RestReqBuf, RxMeta};
use serde::Deserialize;

pub(crate) fn request(
    line: Line,
    rd: &RefData,
    insts: &[InstId],
    out: &mut RestReqBuf,
) -> Result<bool, ProtoErr> {
    if insts.is_empty() {
        return Err(ProtoErr::Unsupported);
    }
    let [inst] = insts else {
        return Err(ProtoErr::BufFull);
    };
    if inst.0 == 0 || rd.inst(*inst).venue != line.venue() {
        return Err(ProtoErr::Unsupported);
    }
    *out = RestReqBuf::default();
    let sym = rd.inst_meta(*inst).symbol.as_bytes();
    if sym.is_empty() || !sym.iter().all(u8::is_ascii_alphanumeric) {
        return Err(ProtoErr::Cfg);
    }
    out.path_query.push(b"/api/v3/market/orderbook?category=")?;
    out.path_query.push(line.rest_category())?;
    out.path_query.push(b"&symbol=")?;
    out.path_query.push(sym)?;
    out.path_query.push(b"&limit=1")?;
    Ok(true)
}
#[derive(Deserialize)]
struct Book<'a> {
    #[serde(borrow)]
    a: Raw<'a>,
    #[serde(borrow)]
    b: Raw<'a>,
    ts: UInt,
}
#[derive(Deserialize)]
struct Response<'a> {
    code: Code,
    #[serde(borrow)]
    data: Book<'a>,
}
pub(crate) fn decode(
    line: Line,
    rd: &RefData,
    insts: &[InstId],
    body: &[u8],
    rx: &RxMeta,
    out: &mut impl MdSink,
) -> Result<u16, DecodeErr> {
    let [inst] = insts else {
        return Err(DecodeErr::Shape);
    };
    if inst.0 == 0 || rd.inst(*inst).venue != line.venue() {
        return Err(DecodeErr::Shape);
    }
    let m: Response<'_> = from(body)?;
    if !m.code.is_zero() {
        return Err(DecodeErr::Shape);
    }
    let i = rd.inst(*inst);
    let mut bids = [Level {
        px: Px(0),
        qty: Qty(0),
    }; 1];
    let mut asks = bids;
    let nb = levels(
        m.data.b.text(),
        i.px_wire_exp,
        i.qty_wire_exp,
        false,
        true,
        &mut bids,
    )?;
    let na = levels(
        m.data.a.text(),
        i.px_wire_exp,
        i.qty_wire_exp,
        false,
        true,
        &mut asks,
    )?;
    if nb == 0 || na == 0 {
        return Err(DecodeErr::Shape);
    }
    if bids[0].px.0 < 0 || bids[0].qty.0 < 0 || asks[0].px.0 < 0 || asks[0].qty.0 < 0 {
        return Err(DecodeErr::Number);
    }
    let e = Bbo {
        h: MarketHdr {
            venue: line.venue(),
            inst: *inst,
            conn: rx.conn,
            _pad: 0,
            ex_ts_ns: ms_ns(m.data.ts.get()?)?,
            ex_send_ns: 0,
            hw_rx_ns: rx.hw_rx_ns,
            rx_tsc: rx.rx_tsc,
            sw_rx_ns: rx.sw_rx_ns,
            rx_empty_gap_ns: rx.rx_empty_gap_ns,
            _rsv: 0,
        },
        bid_px: bids[0].px,
        bid_qty: bids[0].qty,
        ask_px: asks[0].px,
        ask_qty: asks[0].qty,
        update_id: 0,
    };
    out.bbo(&e);
    Ok(1)
}
