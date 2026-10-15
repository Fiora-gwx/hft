//! USDT-M actual funding cash flows. Warm, authenticated REST; no user-stream inference.
use hft_refdata::RefData;
use hft_types::Notional;
use hft_venue_proto::{
    Credential, DecodeErr, FundingBill, FundingHistoryProtocol, FundingPage, FundingQuery, Method, ProtoErr,
    RestReqBuf, VenueCfg, VenueProtocol, WallClock,
};
use serde::Deserialize;

use crate::{Usdm, json, wr::Wr};

const LIMIT: usize = 100;

#[derive(Deserialize)]
struct Bill<'a> {
    #[serde(borrow)]
    symbol: &'a str,
    #[serde(rename = "incomeType")]
    income_type: &'a str,
    income: &'a str,
    asset: &'a str,
    time: u64,
    #[serde(rename = "tranId")]
    id: u64,
}

impl FundingHistoryProtocol for Usdm {
    fn funding_request(
        cfg: &VenueCfg,
        cred: &Credential,
        q: &FundingQuery,
        clk: &WallClock,
        out: &mut RestReqBuf,
    ) -> Result<(), ProtoErr> {
        if q.start_ms < 0 || q.end_ms < q.start_ms || q.page == 0 || !q.cursor.is_empty() {
            return Err(ProtoErr::Cfg);
        }
        let mut buf = [0; 256];
        let mut w = Wr::new(&mut buf);
        w.bytes(b"incomeType=FUNDING_FEE&startTime=")
            .i64(q.start_ms)
            .bytes(b"&endTime=")
            .i64(q.end_ms)
            .bytes(b"&page=")
            .u64(u64::from(q.page))
            .bytes(b"&limit=100");
        let n = w.finish().ok_or(ProtoErr::BufFull)?;
        let query = buf.get(..n).ok_or(ProtoErr::BufFull)?;
        *out = RestReqBuf::default();
        out.method = Method::Get;
        out.redacted = true;
        out.path_query.push(b"/fapi/v1/income?")?;
        out.path_query.push(query)?;
        crate::account::sign_request(cfg, cred, clk.unix_ms, out)
    }

    fn funding_decode(
        rd: &RefData,
        q: &FundingQuery,
        status: u16,
        body: &[u8],
    ) -> Result<FundingPage, DecodeErr> {
        if status != 200 || body.len() > 131_072 {
            return Err(DecodeErr::Shape);
        }
        let rows: Vec<Bill<'_>> = json::from(body)?;
        if rows.len() > LIMIT {
            return Err(DecodeErr::Overflow);
        }
        let next = if rows.len() == LIMIT {
            Some(FundingQuery {
                page: q.page.checked_add(1).ok_or(DecodeErr::Overflow)?,
                ..q.clone()
            })
        } else {
            None
        };
        let mut bills = Vec::with_capacity(rows.len());
        for r in rows {
            if r.income_type != "FUNDING_FEE" || r.asset != "USDT" {
                continue;
            }
            let inst = rd
                .symbols(Self::VENUE)
                .get(r.symbol.as_bytes())
                .ok_or(DecodeErr::Shape)?;
            let ms = i64::try_from(r.time).map_err(|_| DecodeErr::Overflow)?;
            if ms < q.start_ms || ms > q.end_ms {
                return Err(DecodeErr::Shape);
            }
            bills.push(FundingBill {
                id: r.id,
                inst,
                time_ns: ms.checked_mul(1_000_000).ok_or(DecodeErr::Overflow)?,
                amount: Notional(
                    hft_types::parse_round(r.income.as_bytes(), -6, hft_types::RoundMode::HalfEven)
                        .map_err(json::fx)?,
                ),
            });
        }
        Ok(FundingPage { bills, next })
    }
}
