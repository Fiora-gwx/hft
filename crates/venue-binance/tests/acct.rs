//! Warm AccountProtocol margin boundaries; the frozen allocation-free acct_* path is separate.
use crate::support::*;
use hft_venue_proto::account::{AccountProtocol, Query, State};
use venue_binance::Usdm;

fn margin(equity: &str, maintenance: &str) -> State {
    let body = serde_json::json!({
        "totalMarginBalance": equity, "totalMaintMargin": maintenance,
        "availableBalance": "0", "marginRatio": "0.5", "assets": []
    });
    let mut state = State::default();
    Usdm::account_decode(
        rd(),
        ACCT,
        &Query::Balances,
        200,
        &serde_json::to_vec(&body).unwrap(),
        10,
        &mut state,
    )
    .unwrap();
    state
}

#[test]
fn bin_149_margin_ratio_boundaries() {
    for (equity, maintenance, ratio) in [
        ("0", "250", i64::MAX),
        ("-10", "250", i64::MAX),
        ("0.000001", "0", 0),
        ("250", "250", 1_000_000_000),
        ("125", "250", 2_000_000_000),
        ("10000", "250", 25_000_000),
    ] {
        let s = margin(equity, maintenance);
        let m = s.margin.unwrap();
        assert_eq!(m.margin_ratio.0, ratio);
        assert_eq!(m.ex_ts_ns, 10_000_000);
        assert!(s.balances.iter().all(|b| b.free.0 == 0 && b.locked.0 == 0));
    }
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(1000))]
    #[test]
    fn bin_151_margin_i128_reference(equity in 1i64..1_000_000_000, maintenance in 0i64..1_000_000_000) {
        let s = margin(&equity.to_string(), &maintenance.to_string());
        let want = (i128::from(maintenance) * 1_000_000_000 / i128::from(equity)).min(i128::from(i64::MAX)) as i64;
        proptest::prop_assert_eq!(s.margin.unwrap().margin_ratio.0, want);
    }
}

#[test]
fn bin_148_position_baseline_and_missing_assets() {
    let body = br#"[{"symbol":"BTCUSDT","positionSide":"BOTH","positionAmt":"-0.001","entryPrice":"65000.125","unRealizedProfit":"-1.0000001","liquidationPrice":"70000.555"}]"#;
    let mut s = margin("10000", "250");
    Usdm::account_decode(rd(), ACCT, &Query::Positions, 200, body, 12, &mut s).unwrap();
    let p = s.positions.iter().find(|p| p.inst == U_BTC).unwrap();
    assert_eq!(
        (p.qty.0, p.entry_px.0, p.upnl.0, p.liq_px.0),
        (-100_000, 6_500_012, -1_000_000, 7_000_056)
    );
    assert_eq!(s.margin_items.len(), 1);
    assert_eq!(s.margin_items[0].adl_quantile, 255);
    Usdm::account_decode(rd(), ACCT, &Query::Positions, 200, b"[]", 13, &mut s).unwrap();
    assert!(s.positions.iter().all(|p| p.qty.0 == 0));
    assert!(s.margin_items.is_empty());
    for status in [403, 429, 500, 503] {
        assert!(
            Usdm::account_decode(
                rd(),
                ACCT,
                &Query::Positions,
                status,
                body,
                14,
                &mut State::default()
            )
            .is_err()
        );
    }
}

#[test]
fn bin_213_unverifiable_only_exempts_missing_uid() {
    let settings = |uid: serde_json::Value| {
        serde_json::to_vec(&serde_json::json!({
            "canTrade": true, "dualSidePosition": false, "multiAssetsMargin": false, "uid": uid
        }))
        .unwrap()
    };
    let rd = account_rd(true);
    let mut s = State::default();
    Usdm::account_decode(
        &rd,
        ACCT,
        &Query::Settings,
        200,
        &settings(serde_json::Value::Null),
        1,
        &mut s,
    )
    .unwrap();
    assert_eq!(
        (s.config.unwrap().venue_uid, s.config.unwrap().uid_keys),
        ([0; 24], 0)
    );
    for uid in [serde_json::json!(12345), serde_json::json!("acct-0001")] {
        Usdm::account_decode(&rd, ACCT, &Query::Settings, 200, &settings(uid), 1, &mut s).unwrap();
        assert_ne!(s.config.unwrap().venue_uid, [0; 24]);
        assert_ne!(s.config.unwrap().uid_keys, 0);
    }
    Usdm::account_decode(
        &rd,
        ACCT,
        &Query::Settings,
        200,
        &settings(serde_json::json!("acct-0001")),
        1,
        &mut s,
    )
    .unwrap();
    assert!(
        Usdm::account_decode(
            &rd,
            ACCT,
            &Query::Permissions,
            200,
            &settings(serde_json::json!("acct-0002")),
            1,
            &mut s
        )
        .is_err()
    );
    let verified = account_rd(false);
    Usdm::account_decode(
        &verified,
        ACCT,
        &Query::Settings,
        200,
        &settings(serde_json::json!("acct-0001")),
        1,
        &mut s,
    )
    .unwrap();
    for uid in [
        serde_json::Value::Null,
        serde_json::json!("wrong"),
        serde_json::json!("1234567890123456789012345"),
    ] {
        assert!(
            Usdm::account_decode(&verified, ACCT, &Query::Settings, 200, &settings(uid), 1, &mut s).is_err()
        );
    }
    let mut bad: serde_json::Value = serde_json::from_slice(&settings(serde_json::Value::Null)).unwrap();
    bad["multiAssetsMargin"] = true.into();
    assert!(
        Usdm::account_decode(
            &rd,
            ACCT,
            &Query::Settings,
            200,
            &serde_json::to_vec(&bad).unwrap(),
            1,
            &mut s
        )
        .is_err()
    );
}

#[test]
fn bin_149_negative_maintenance_is_not_a_safe_ratio() {
    let mut s = State::default();
    let body =
        br#"{"totalMarginBalance":"100","totalMaintMargin":"-1","availableBalance":"100","assets":[]}"#;
    assert!(Usdm::account_decode(rd(), ACCT, &Query::Balances, 200, body, 1, &mut s).is_err());
    assert!(s.margin.is_none());
}

#[test]
fn bin_153_failed_account_page_preserves_baseline() {
    let mut s = margin("10000", "250");
    let body = br#"[{"symbol":"BTCUSDT","positionSide":"BOTH","positionAmt":"-0.001","entryPrice":"65000","unRealizedProfit":"-1","liquidationPrice":"70000"}]"#;
    Usdm::account_decode(rd(), ACCT, &Query::Positions, 200, body, 12, &mut s).unwrap();
    let before = format!("{s:?}");
    let mut rows: serde_json::Value = serde_json::from_slice(body).unwrap();
    let mut bad = rows[0].clone();
    bad["symbol"] = "ETHUSDT".into();
    bad["positionAmt"] = "bad".into();
    rows.as_array_mut().unwrap().push(bad);
    let duplicate = br#"{"totalMarginBalance":"10000","totalMaintMargin":"250","availableBalance":"100","assets":[{"asset":"USDT","walletBalance":"100"},{"asset":"USDT","walletBalance":"200"}]}"#;
    for (q, b) in [
        (Query::Positions, serde_json::to_vec(&rows).unwrap()),
        (Query::Balances, duplicate.to_vec()),
    ] {
        assert!(Usdm::account_decode(rd(), ACCT, &q, 200, &b, 13, &mut s).is_err());
        assert_eq!(format!("{s:?}"), before);
    }
    for _ in 0..1000 {
        Usdm::account_decode(rd(), ACCT, &Query::Positions, 200, body, 12, &mut s).unwrap();
    }
    assert_eq!(format!("{s:?}"), before);
    // Interleaving account snapshots must neither change report dedup nor vice versa.
    use hft_venue_proto::{MsgOp, VenueProtocol};
    const PUSH: &str = r#"{"e":"ORDER_TRADE_UPDATE","o":{"s":"BTCUSDT","c":"1000301020002a000000073d","S":"BUY","o":"LIMIT","x":"TRADE","X":"PARTIALLY_FILLED","i":325078477,"l":"0.001","z":"0.003","L":"65000.1","ap":"65000.05","n":"0.0130","N":"USDT","rp":"0","T":1790000000010,"t":9001}}"#;
    let mut plain = venue_binance::GwCtx::default();
    let mut interleaved = venue_binance::GwCtx::default();
    let class = Usdm::classify_report(MsgOp::Text, &PUSH.as_bytes()[..64]);
    for n in 0..10_000 {
        let mut left = RepSink::default();
        let mut right = RepSink::default();
        let a = Usdm::decode_report(rd(), &mut plain, class, PUSH.as_bytes(), &RX, &mut left).unwrap();
        if n % 10 == 0 {
            Usdm::account_decode(rd(), ACCT, &Query::Positions, 200, body, 12, &mut s).unwrap();
        }
        let b = Usdm::decode_report(rd(), &mut interleaved, class, PUSH.as_bytes(), &RX, &mut right).unwrap();
        assert_eq!(a, b);
        assert_eq!(format!("{left:?}"), format!("{right:?}"));
    }
    assert_eq!(format!("{s:?}"), before);
}

#[test]
fn bin_006_warm_cancel_response_matches_request() {
    let q = Query::Cancel {
        symbol: "BTCUSDT".into(),
        order_id: "123".into(),
    };
    for (symbol, id, valid) in [
        ("BTCUSDT", 123, true),
        ("ETHUSDT", 123, false),
        ("BTCUSDT", 124, false),
    ] {
        let body = serde_json::to_vec(&serde_json::json!({"symbol":symbol,"orderId":id})).unwrap();
        assert_eq!(
            Usdm::account_decode(rd(), ACCT, &q, 200, &body, 1, &mut State::default()).is_ok(),
            valid
        );
    }
}

#[test]
fn bin_142_rest_hmac_request_matches_independent_vector() {
    use hft_venue_proto::{Method, RestReqBuf};
    let mut out = RestReqBuf::default();
    Usdm::account_request(&cfg(1500), &hm1(), &Query::Settings, WC.unix_ms, &mut out).unwrap();
    assert_eq!(out.method, Method::Get);
    assert_eq!(out.endpoint, 0);
    assert!(out.redacted);
    // Independently calculated using Python hmac over the public HM1 documentation key.
    assert_eq!(out.path_query.as_slice(), b"/fapi/v1/accountConfig?timestamp=1790000000000&recvWindow=1500&signature=cd74c1947d93de5156a92463317b9b8e3c0c89c2f52753b8baf66b1c2c1f133f");
    assert_eq!(out.headers.as_slice(), b"X-MBX-APIKEY: TESTKEY\r\n");
}
