//! Public REST BBO refresh. No authentication; no mutation of feed dedup/funding state.
use crate::{
    Line,
    json::{fixed, from, time_ns},
    md::{hdr, inst_of},
};
use hft_event::Bbo;
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
    *out = RestReqBuf::default();
    if insts.is_empty() || (line == Line::Usdm && insts.len() != 1) {
        return Err(ProtoErr::BufFull);
    }
    out.path_query.push(if line == Line::Usdm {
        b"/fapi/v1/ticker/bookTicker?symbol="
    } else {
        b"/api/v3/ticker/bookTicker?symbols=%5B"
    })?;
    for (n, inst) in insts.iter().enumerate() {
        if inst.0 == 0 || rd.inst(*inst).venue != line.venue() {
            return Err(ProtoErr::Cfg);
        }
        let sym = rd.inst_meta(*inst).symbol.as_bytes();
        if sym.is_empty() || !sym.iter().all(u8::is_ascii_alphanumeric) {
            return Err(ProtoErr::Cfg);
        }
        if line == Line::Spot {
            if n != 0 {
                out.path_query.push(b"%2C")?;
            }
            out.path_query.push(b"%22")?;
        }
        out.path_query.push(sym)?;
        if line == Line::Spot {
            out.path_query.push(b"%22")?;
        }
    }
    if line == Line::Spot {
        out.path_query.push(b"%5D")?;
    }
    Ok(true)
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Ticker<'a> {
    #[serde(borrow)]
    symbol: &'a str,
    bid_price: &'a str,
    bid_qty: &'a str,
    ask_price: &'a str,
    ask_qty: &'a str,
    #[serde(default)]
    last_update_id: u64,
    #[serde(default)]
    time: i64,
}
fn event(
    line: Line,
    rd: &RefData,
    insts: &[InstId],
    m: &Ticker<'_>,
    rx: &RxMeta,
) -> Result<Option<Bbo>, DecodeErr> {
    let inst = inst_of(line, rd, m.symbol.as_bytes());
    if inst.0 == 0 || !insts.contains(&inst) {
        return Ok(None);
    }
    let i = rd.inst(inst);
    let mut h = hdr(line, inst, rx);
    if line == Line::Usdm {
        h.ex_ts_ns = time_ns(m.time, 1_000_000)?;
    }
    let e = Bbo {
        h,
        bid_px: Px(fixed(m.bid_price, i.px_wire_exp)?),
        bid_qty: Qty(fixed(m.bid_qty, i.qty_wire_exp)?),
        ask_px: Px(fixed(m.ask_price, i.px_wire_exp)?),
        ask_qty: Qty(fixed(m.ask_qty, i.qty_wire_exp)?),
        update_id: if line == Line::Usdm { m.last_update_id } else { 0 },
    };
    if e.bid_px.0 < 0 || e.ask_px.0 < 0 || e.bid_qty.0 < 0 || e.ask_qty.0 < 0 {
        return Err(DecodeErr::Number);
    }
    Ok(Some(e))
}
pub(crate) fn decode(
    line: Line,
    rd: &RefData,
    insts: &[InstId],
    body: &[u8],
    rx: &RxMeta,
    out: &mut impl MdSink,
) -> Result<u16, DecodeErr> {
    if insts.is_empty()
        || insts
            .iter()
            .any(|i| i.0 == 0 || rd.inst(*i).venue != line.venue())
    {
        return Err(DecodeErr::Shape);
    }
    if line == Line::Usdm {
        if insts.len() != 1 {
            return Err(DecodeErr::Shape);
        }
        let m: Ticker<'_> = from(body)?;
        if let Some(e) = event(line, rd, insts, &m, rx)? {
            out.bbo(&e);
            return Ok(1);
        }
        return Ok(0);
    }
    // Warm snapshot task only. Validate the complete response before publishing any quote.
    let rows: Vec<Ticker<'_>> = from(body)?;
    let mut n = 0u16;
    for row in &rows {
        if event(line, rd, insts, row, rx)?.is_some() {
            n = n.checked_add(1).ok_or(DecodeErr::Overflow)?;
        }
    }
    for row in &rows {
        if let Some(e) = event(line, rd, insts, row, rx)? {
            out.bbo(&e);
        }
    }
    Ok(n)
}
