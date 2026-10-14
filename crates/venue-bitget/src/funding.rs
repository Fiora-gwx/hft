//! UTA financial records (trading account, not the similarly named funding-wallet endpoint).
use hft_refdata::RefData;
use hft_types::{AccountId, Notional};
use hft_venue_proto::{
    ArrayBuf, Credential, DecodeErr, FundingBill, FundingHistoryProtocol, FundingPage, FundingQuery, Method,
    ProtoErr, RestReqBuf, VenueCfg, VenueProtocol, WallClock,
};
use serde::Deserialize;

use crate::{BitgetUsdtFutures, json, orders, wr::Wr};

#[derive(Deserialize)]
struct Response<'a> {
    #[serde(borrow)]
    code: &'a str,
    data: Page<'a>,
}
#[derive(Deserialize)]
struct Page<'a> {
    #[serde(borrow)]
    list: Vec<Bill<'a>>,
    #[serde(default)]
    cursor: Option<&'a str>,
}
#[derive(Deserialize)]
struct Bill<'a> {
    #[serde(borrow)]
    id: &'a str,
    symbol: &'a str,
    coin: &'a str,
    category: &'a str,
    #[serde(rename = "type")]
    kind: &'a str,
    amount: &'a str,
    ts: &'a str,
}

impl FundingHistoryProtocol for BitgetUsdtFutures {
    fn funding_request(
        cfg: &VenueCfg,
        cred: &Credential,
        q: &FundingQuery,
        clk: &WallClock,
        out: &mut RestReqBuf,
    ) -> Result<(), ProtoErr> {
        if q.start_ms < 0
            || q.end_ms < q.start_ms
            || q.end_ms - q.start_ms > 30 * 86_400_000
            || !q.cursor.as_slice().iter().all(u8::is_ascii_digit)
        {
            return Err(ProtoErr::Cfg);
        }
        let t = orders::build_templates(cfg, AccountId(0), cred)?;
        let mut buf = [0; 512];
        let mut w = Wr::new(&mut buf);
        w.bytes(b"/api/v3/account/financial-records?category=USDT-FUTURES&coin=USDT&limit=100&startTime=")
            .i64(q.start_ms)
            .bytes(b"&endTime=")
            .i64(q.end_ms);
        if !q.cursor.is_empty() {
            w.bytes(b"&cursor=").bytes(q.cursor.as_slice());
        }
        let n = w.finish().ok_or(ProtoErr::BufFull)?;
        *out = RestReqBuf::default();
        out.method = Method::Get;
        out.path_query.push(buf.get(..n).ok_or(ProtoErr::BufFull)?)?;
        orders::sign_headers(&t, cred, clk, out)
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
        let r: Response<'_> = json::from(body)?;
        if r.code != "00000" || r.data.list.len() > 100 {
            return Err(DecodeErr::Shape);
        }
        let next = match r.data.cursor.filter(|s| !s.is_empty()) {
            Some(c)
                if !r.data.list.is_empty()
                    && c.as_bytes().iter().all(u8::is_ascii_digit)
                    && c.as_bytes() != q.cursor.as_slice() =>
            {
                Some(FundingQuery {
                    cursor: ArrayBuf::from_bytes(c.as_bytes()).ok_or(DecodeErr::Overflow)?,
                    page: q.page.checked_add(1).ok_or(DecodeErr::Overflow)?,
                    ..q.clone()
                })
            }
            Some(_) if !r.data.list.is_empty() => return Err(DecodeErr::Shape),
            _ => None,
        };
        let mut bills = Vec::with_capacity(r.data.list.len());
        for b in r.data.list {
            let credit = match b.kind {
                "CONTRACT_MAIN_SETTLE_FEE_USER_IN" => true,
                "CONTRACT_MAIN_SETTLE_FEE_USER_OUT" => false,
                _ => continue,
            };
            if b.coin != "USDT" || b.category != "USDT-FUTURES" {
                return Err(DecodeErr::Shape);
            }
            let inst = rd
                .symbols(Self::VENUE)
                .get(b.symbol.as_bytes())
                .ok_or(DecodeErr::Shape)?;
            let ms = b.ts.parse::<i64>().map_err(|_| DecodeErr::Number)?;
            if ms < q.start_ms || ms > q.end_ms {
                return Err(DecodeErr::Shape);
            }
            let raw = json::fixed(b.amount, -6)?;
            // Direction is explicit in the bill type; accept signed or magnitude-form amounts.
            let amount = raw.checked_abs().ok_or(DecodeErr::Overflow)?;
            bills.push(FundingBill {
                id: b.id.parse().map_err(|_| DecodeErr::Number)?,
                inst,
                time_ns: ms.checked_mul(1_000_000).ok_or(DecodeErr::Overflow)?,
                amount: Notional(if credit { amount } else { -amount }),
            });
        }
        Ok(FundingPage { bills, next })
    }
}
