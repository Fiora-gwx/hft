use crate::support::*;
use hft_types::Notional;
use hft_venue_proto::{FundingHistoryProtocol, FundingQuery, RestReqBuf};
use venue_binance::Usdm;

fn query() -> FundingQuery {
    FundingQuery {
        start_ms: 1_790_006_400_000,
        end_ms: 1_790_006_460_000,
        page: 1,
        cursor: Default::default(),
    }
}

#[test]
fn bin_332_income_history_actual_amount_and_signing() {
    let mut req = RestReqBuf::default();
    Usdm::funding_request(&cfg(5000), &hm1(), &query(), &WC, &mut req).unwrap();
    assert!(req.redacted);
    let path = std::str::from_utf8(req.path_query.as_slice()).unwrap();
    assert!(path.starts_with("/fapi/v1/income?incomeType=FUNDING_FEE&startTime="));
    assert!(path.contains("&page=1&limit=100&timestamp="));
    assert_eq!(path.split("&signature=").nth(1).unwrap().len(), 64);
    let page = Usdm::funding_decode(rd(), &query(), 200, br#"[
        {"symbol":"BTCUSDT","incomeType":"FUNDING_FEE","income":"-0.35","asset":"USDT","time":1790006400000,"tranId":7},
        {"symbol":"ETHUSDT","incomeType":"FUNDING_FEE","income":"0.35","asset":"USDT","time":1790006400000,"tranId":8},
        {"symbol":"BTCUSDT","incomeType":"COMMISSION","income":"-9","asset":"USDT","time":1790006400000,"tranId":9}
    ]"#).unwrap();
    assert!(page.next.is_none());
    assert_eq!(page.bills.len(), 2);
    assert_eq!(
        (page.bills[0].inst, page.bills[0].amount),
        (U_BTC, Notional(-350_000))
    );
    assert_eq!(
        (page.bills[1].inst, page.bills[1].amount),
        (U_ETH, Notional(350_000))
    );
    assert!(Usdm::funding_decode(rd(), &query(), 429, b"[]").is_err());
    assert!(Usdm::funding_decode(rd(), &query(), 200, b"[{]").is_err());
}

#[test]
fn bin_332_full_page_requires_following_page() {
    let row = r#"{"symbol":"BTCUSDT","incomeType":"FUNDING_FEE","income":"1","asset":"USDT","time":1790006400000,"tranId":7}"#;
    let payload = format!("[{}]", vec![row; 100].join(","));
    let page = Usdm::funding_decode(rd(), &query(), 200, payload.as_bytes()).unwrap();
    let next = page.next.unwrap();
    assert_eq!(
        (next.page, next.start_ms, next.end_ms),
        (2, query().start_ms, query().end_ms)
    );
}
