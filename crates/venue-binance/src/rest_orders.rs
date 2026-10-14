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
    if !matches!(
        req.what,
        RestWhat::Order | RestWhat::Keepalive | RestWhat::DeadMan
    ) || (req.what == RestWhat::Order && req.req.is_none_or(|r| r.op != GwOp::CancelAll))
    {
        return Err(DecodeErr::UnknownClass);
    }
    let mut o = ReportOutcome {
        req: req.req,
        ..ReportOutcome::default()
    };
    #[derive(Deserialize)]
    struct Answer {
        code: Option<i32>,
        #[serde(rename = "listenKey")]
        listen_key: Option<String>,
        #[serde(rename = "countdownTime")]
        countdown: Option<Number>,
    }
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Number {
        Integer(u64),
        Text(String),
    }
    let parsed = crate::json::from::<Answer>(body);
    if !(200..300).contains(&status) || parsed.as_ref().is_ok_and(|r| r.code.is_some_and(|c| c != 200)) {
        o.outcome_unknown = req.req;
        let code = parsed.as_ref().ok().and_then(|r| r.code).filter(|c| *c < 0);
        o.raw_code = code.map_or(0x2000_0000 | u32::from(status), |c| c as u32);
        o.ctl = match code {
            Some(-1125) if req.what == RestWhat::Keepalive => ReportCtl::UserStreamEnded,
            Some(-1003 | -1015) => ReportCtl::RateLimited,
            Some(-1021) => ReportCtl::ClockSkew,
            _ if matches!(status, 418 | 429) => ReportCtl::RateLimited,
            _ => ReportCtl::None,
        };
        return Ok(o);
    }
    let r = parsed?;
    match req.what {
        RestWhat::Order if r.code != Some(200) => return Err(DecodeErr::Shape),
        RestWhat::Keepalive if r.listen_key.as_ref().is_none_or(String::is_empty) => {
            return Err(DecodeErr::Shape);
        }
        RestWhat::DeadMan => {
            let _countdown = match r.countdown.ok_or(DecodeErr::Shape)? {
                Number::Integer(n) => n,
                Number::Text(s) => s.parse::<u64>().map_err(|_| DecodeErr::Number)?,
            };
        }
        _ => {}
    }
    Ok(o)
}
