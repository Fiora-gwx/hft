use crate::support::*;
use hft_types::Notional;
use hft_venue_proto::{FundingHistoryProtocol, FundingQuery, RestReqBuf};
use venue_bitget::BitgetUsdtFutures as P;

fn query() -> FundingQuery {
    FundingQuery {
        start_ms: 1_790_006_400_000,
        end_ms: 1_790_006_460_000,
        page: 1,
        cursor: Default::default(),
    }
}

#[test]
fn bgt_336_funding_bills_are_cash_not_ticker_or_wallet() {
    let mut out = RestReqBuf::default();
    P::funding_request(&cfg(), &key(), &query(), &WC, &mut out).unwrap();
    assert!(out.redacted);
    assert!(
        out.path_query
            .as_slice()
            .starts_with(b"/api/v3/account/financial-records?category=USDT-FUTURES")
    );
    assert!(
        std::str::from_utf8(out.headers.as_slice())
            .unwrap()
            .contains("ACCESS-SIGN:")
    );
    let msg = br#"{"code":"00000","data":{"list":[
        {"id":"7","symbol":"BTCUSDT","coin":"USDT","category":"USDT-FUTURES","type":"CONTRACT_MAIN_SETTLE_FEE_USER_IN","amount":"0.35","balance":"9999","ts":"1790006400000"},
        {"id":"8","symbol":"ETHUSDT","coin":"USDT","category":"USDT-FUTURES","type":"CONTRACT_MAIN_SETTLE_FEE_USER_OUT","amount":"0.20","balance":"9999","ts":"1790006400000"},
        {"id":"9","symbol":"BTCUSDT","coin":"USDT","category":"USDT-FUTURES","type":"ORDER_PLF_FEE_OUT","amount":"9","ts":"1790006400000"}
    ],"cursor":""}}"#;
    let p = P::funding_decode(rd(), &query(), 200, msg).unwrap();
    assert!(p.next.is_none());
    assert_eq!(p.bills.len(), 2);
    assert_eq!((p.bills[0].inst, p.bills[0].amount), (F_BTC, Notional(350_000)));
    assert_eq!((p.bills[1].inst, p.bills[1].amount), (F_ETH, Notional(-200_000)));
    assert!(P::funding_decode(rd(), &query(), 403, msg).is_err());
    assert!(P::funding_decode(rd(), &query(), 200, br#"{"code":"40009","data":{}}"#).is_err());
    let paged = String::from_utf8(msg.to_vec())
        .unwrap()
        .replace(r#""cursor":"""#, r#""cursor":"9""#);
    let next = P::funding_decode(rd(), &query(), 200, paged.as_bytes())
        .unwrap()
        .next
        .unwrap();
    assert_eq!(next.cursor.as_slice(), b"9");
    assert!(P::funding_decode(rd(), &next, 200, paged.as_bytes()).is_err());
    // An empty page with a continuation cannot advance the durable funding watermark.
    for cursor in ["9", "unsafe&cursor"] {
        let empty =
            serde_json::to_vec(&serde_json::json!({"code":"00000","data":{"list":[],"cursor":cursor}}))
                .unwrap();
        assert!(P::funding_decode(rd(), &query(), 200, &empty).is_err());
    }
}
