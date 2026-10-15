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
    // Unit inputs, not claimed as captured Binance funding settlements.
    for (wire, expected) in [
        ("0.00238005", 2380),
        ("-0.00238005", -2380),
        ("0.0000005", 0),
        ("0.0000015", 2),
        ("-0.0000025", -2),
    ] {
        let body = format!(
            r#"[{{"symbol":"ETHUSDT","incomeType":"FUNDING_FEE","income":"{wire}","asset":"USDT","time":1790006400000,"tranId":10}}]"#
        );
        let page = Usdm::funding_decode(rd(), &query(), 200, body.as_bytes()).unwrap();
        assert_eq!(page.bills[0].amount, Notional(expected));
    }
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

#[test]
fn bin_332_ed25519_income_history_signing() {
    use base64::Engine;
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};
    let mut req = RestReqBuf::default();
    Usdm::funding_request(&cfg(1500), &ed1(), &query(), &WC, &mut req).unwrap();
    assert!(req.redacted);
    assert_eq!(req.headers.as_slice(), b"X-MBX-APIKEY: TESTKEY\r\n");
    let path = std::str::from_utf8(req.path_query.as_slice()).unwrap();
    let (query, encoded) = path
        .strip_prefix("/fapi/v1/income?")
        .unwrap()
        .split_once("&signature=")
        .unwrap();
    assert_eq!(
        query,
        "incomeType=FUNDING_FEE&startTime=1790006400000&endTime=1790006460000&page=1&limit=100&timestamp=1790000000000&recvWindow=1500"
    );
    assert!(!encoded.contains(['+', '/', '=']));
    let decoded = encoded
        .replace("%2B", "+")
        .replace("%2F", "/")
        .replace("%3D", "=");
    let signature = base64::engine::general_purpose::STANDARD.decode(decoded).unwrap();
    let key = VerifyingKey::from_bytes(&hex(ED1_PUB).try_into().unwrap()).unwrap();
    key.verify(query.as_bytes(), &Signature::from_slice(&signature).unwrap())
        .unwrap();
}
