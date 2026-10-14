//! Warm REST order operations. A cancel-all acknowledgement is never a synthetic per-order cancel.
use crate::{Line, Templates};
use hft_event::{GwCmd, GwOp};
use hft_refdata::RefData;
use hft_types::InstId;
use hft_venue_proto::*;
use serde::Deserialize;

pub(crate) fn cancel_all(
    line: Line,
    rd: &RefData,
    t: &Templates,
    c: &GwCmd,
    clk: &WallClock,
    out: &mut RestReqBuf,
) -> Result<(), EncodeErr> {
    *out = RestReqBuf::default();
    if line != Line::Usdm || c.op != GwOp::CancelAll || c.account != t.account || c.inst.0 == 0 {
        return Err(EncodeErr::Unsupported(c.op));
    }
    if c.inst.0 == 0 || rd.inst(c.inst).venue != line.venue() {
        return Err(EncodeErr::UnknownInst);
    }
    let symbol = rd.inst_meta(c.inst).symbol.as_ref();
    if !account::safe_token(symbol) {
        return Err(EncodeErr::UnknownInst);
    }
    out.method = Method::Delete;
    out.path_query
        .push(format!("/fapi/v1/allOpenOrders?symbol={symbol}").as_bytes())
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
pub(crate) fn deadman(
    line: Line,
    rd: &RefData,
    t: &Templates,
    timeout_ms: u32,
    insts: &[InstId],
    clk: &WallClock,
    out: &mut DeadManOut,
) -> Result<(), EncodeErr> {
    out.reqs.clear();
    out.frame.clear();
    if line != Line::Usdm {
        return Err(EncodeErr::Unsupported(GwOp::DeadManRefresh));
    }
    // Validate the entire list before generating any request.
    if insts.iter().any(|i| {
        i.0 == 0
            || rd.inst(*i).venue != line.venue()
            || !account::safe_token(rd.inst_meta(*i).symbol.as_ref())
    }) {
        return Err(EncodeErr::UnknownInst);
    }
    let mut requests = Vec::with_capacity(insts.len());
    for &i in insts {
        let q = account::Query::DeadMan {
            symbol: rd.inst_meta(i).symbol.to_string(),
            timeout_ms,
        };
        let mut r = RestReqBuf::default();
        crate::account::request(
            true,
            &VenueCfg {
                env: VenueEnv::Prod,
                stale_guard_ms: t.recv_window_ms,
            },
            &t.rest,
            &q,
            clk.unix_ms,
            &mut r,
        )
        .map_err(|_| EncodeErr::FieldOverflow)?;
        requests.push(r);
    }
    out.reqs = requests;
    Ok(())
}
pub(crate) fn decode(req: &RestOrigin, status: u16, body: &[u8]) -> Result<ReportOutcome, DecodeErr> {
    if req.req.is_none_or(|r| r.op != GwOp::CancelAll) {
        return Err(DecodeErr::UnknownClass);
    }
    let mut o = ReportOutcome {
        req: req.req,
        ..ReportOutcome::default()
    };
    if !(200..300).contains(&status) {
        o.outcome_unknown = req.req;
        o.raw_code = 0x2000_0000 | u32::from(status);
        if status == 429 || status == 418 {
            o.ctl = ReportCtl::RateLimited;
        }
        return Ok(o);
    }
    #[derive(Deserialize)]
    struct Answer {
        code: i32,
    }
    let r: Answer = crate::json::from(body)?;
    if r.code != 200 {
        o.outcome_unknown = req.req;
        o.raw_code = r.code as u32;
        o.ctl = match r.code {
            -1003 | -1015 => ReportCtl::RateLimited,
            -1021 => ReportCtl::ClockSkew,
            _ => ReportCtl::None,
        };
    }
    Ok(o)
}
