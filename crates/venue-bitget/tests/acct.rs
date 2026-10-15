use crate::support::*;
use hft_types::*;
use hft_venue_proto::{
    DecodeErr,
    account::{AccountProtocol, Query, State},
};
use venue_bitget::BitgetUsdtFutures as P;

#[test]
fn bgt_034_rest_funding_schedule_validation() {
    let rd = account_rd(false);
    let mut s = State::default();
    let next = 1_790_006_400_000i64;
    let q = Query::FundingSchedule {
        symbol: "BTCUSDT".into(),
    };
    for hours in [1u32, 2, 4, 8] {
        let body = format!(
            r#"{{"code":"00000","data":[{{"symbol":"BTCUSDT","fundingRateInterval":"{hours}","nextUpdate":"{next}"}}]}}"#
        );
        P::account_decode(rd, ACCT, &q, 200, body.as_bytes(), next - 1000, &mut s).unwrap();
        assert_eq!(s.funding_schedule.last().unwrap().interval_s, hours * 3600);
        for status in [429, 500] {
            assert!(P::account_decode(rd, ACCT, &q, status, body.as_bytes(), next - 1000, &mut s).is_err());
        }
        assert!(P::account_decode(rd, ACCT, &q, 200, body.as_bytes(), next, &mut s).is_err());
        assert!(
            P::account_decode(
                rd,
                ACCT,
                &q,
                200,
                body.as_bytes(),
                next - i64::from(hours) * 3_600_000 - 1,
                &mut s
            )
            .is_err()
        );
        assert!(
            P::account_decode(
                rd,
                ACCT,
                &q,
                200,
                body.replace("BTCUSDT", "ETHUSDT").as_bytes(),
                next - 1000,
                &mut s
            )
            .is_err()
        );
        assert!(
            P::account_decode(
                rd,
                ACCT,
                &q,
                200,
                body.replace(&format!("\"{hours}\""), "\"0\"").as_bytes(),
                next - 1000,
                &mut s
            )
            .is_err()
        );
    }
}

#[test]
fn bgt_078_rest_headers() {
    use base64::Engine;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut out = hft_venue_proto::RestReqBuf::default();
    P::account_request(&cfg(), &key(), &Query::Settings, WC.unix_ms, &mut out).unwrap();
    assert!(out.redacted);
    assert_eq!(out.endpoint, 0);
    let headers = std::str::from_utf8(out.headers.as_slice()).unwrap();
    let mut mac = Hmac::<Sha256>::new_from_slice(b"secret").unwrap();
    mac.update(format!("{}GET", WC.unix_ms).as_bytes());
    mac.update(out.path_query.as_slice());
    let signature = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());
    assert_eq!(signature.len(), 44);
    for h in [
        "ACCESS-KEY: k".to_owned(),
        format!("ACCESS-SIGN: {signature}"),
        format!("ACCESS-TIMESTAMP: {}", WC.unix_ms),
        "ACCESS-PASSPHRASE: p".to_owned(),
    ] {
        assert_eq!(headers.lines().filter(|line| *line == h).count(), 1, "{h}");
    }
    assert_eq!(
        headers.lines().filter(|line| line.starts_with("ACCESS-")).count(),
        4
    );
}

#[test]
fn bgt_006_rest_endpoint_index() {
    fn check<Q: AccountProtocol>(queries: &[Query]) {
        for q in queries {
            let mut out = hft_venue_proto::RestReqBuf::default();
            Q::account_request(&cfg(), &key(), q, WC.unix_ms, &mut out).unwrap();
            assert_eq!(out.endpoint, 0, "{q:?}");
        }
    }
    let common = [
        Query::Instruments,
        Query::Settings,
        Query::SymbolSettings,
        Query::Permissions,
        Query::Balances,
        Query::OpenOrders {
            cursor: String::new(),
        },
        Query::Cancel {
            symbol: "BTCUSDT".into(),
            order_id: "123".into(),
        },
    ];
    check::<P>(&common);
    check::<venue_bitget::BitgetSpot>(&common);
    check::<P>(&[Query::Positions]);
}

fn settings(uid: Option<&str>) -> Vec<u8> {
    let mut s = serde_json::json!({"code":"00000","data": {
    "accountMode":"unified","accountLevel":"basic","holdMode":"one_way_mode",
    "symbolConfigList":[
        {"symbol":"BTCUSDT","category":"USDT-FUTURES","marginMode":"crossed","leverage":"5"},
        {"symbol":"ETHUSDT","category":"USDT-FUTURES","marginMode":"crossed","leverage":"10"}
    ]}});
    if let Some(uid) = uid {
        s["data"]["uid"] = uid.into();
    }
    serde_json::to_vec(&s).unwrap()
}
fn permissions(uid: Option<&str>, withdraw: bool) -> Vec<u8> {
    let mut s = serde_json::json!({"code":"00000","data":{"permType":"read-and-write","permissions":if withdraw { vec!["uta_trade","withdraw"] } else { vec!["uta_trade"] }}});
    if let Some(uid) = uid {
        s["data"]["userId"] = uid.into();
    }
    serde_json::to_vec(&s).unwrap()
}
#[test]
fn bgt_142_config_uid_and_permissions_fail_closed() {
    let rd = account_rd(false);
    let mut s = State::default();
    P::account_decode(
        rd,
        ACCT,
        &Query::Settings,
        200,
        &settings(Some("acct-0001")),
        1,
        &mut s,
    )
    .unwrap();
    P::account_decode(
        rd,
        ACCT,
        &Query::Permissions,
        200,
        &permissions(Some("acct-0001"), false),
        1,
        &mut s,
    )
    .unwrap();
    assert_eq!(s.config.unwrap().uid_keys, 3);
    assert!(P::account_decode(rd, ACCT, &Query::Settings, 200, &settings(None), 1, &mut s).is_err());
    assert!(
        P::account_decode(
            rd,
            ACCT,
            &Query::Settings,
            200,
            &settings(Some("acct-0002")),
            1,
            &mut s
        )
        .is_err()
    );
    assert!(
        P::account_decode(
            rd,
            ACCT,
            &Query::Permissions,
            200,
            &permissions(Some("acct-0001"), true),
            1,
            &mut s
        )
        .is_err()
    );
    let rd = account_rd(true);
    P::account_decode(rd, ACCT, &Query::Settings, 200, &settings(None), 1, &mut s).unwrap();
    P::account_decode(
        rd,
        ACCT,
        &Query::Permissions,
        200,
        &permissions(None, false),
        1,
        &mut s,
    )
    .unwrap();
    let c = s.config.unwrap();
    assert_eq!((c.uid_keys, c.venue_uid, c.mismatches), (0, [0; 24], 0));
    // An actual identity contradiction is rejected even on an unverifiable account.
    P::account_decode(rd, ACCT, &Query::Settings, 200, &settings(Some("123")), 1, &mut s).unwrap();
    assert!(
        P::account_decode(
            rd,
            ACCT,
            &Query::Permissions,
            200,
            &permissions(Some("456"), false),
            1,
            &mut s
        )
        .is_err()
    );
}
#[test]
fn bgt_145_leverage_items() {
    let mut s = State::default();
    P::account_decode(
        account_rd(false),
        ACCT,
        &Query::Settings,
        200,
        &settings(Some("acct-0001")),
        1,
        &mut s,
    )
    .unwrap();
    assert_eq!(
        s.leverage
            .iter()
            .map(|l| (l.inst.0, l.leverage.0))
            .collect::<Vec<_>>(),
        vec![(1, 5_000_000_000), (2, 10_000_000_000)]
    );
    let bad = String::from_utf8(settings(Some("acct-0001")))
        .unwrap()
        .replace("crossed", "isolated");
    assert!(
        P::account_decode(
            account_rd(false),
            ACCT,
            &Query::Settings,
            200,
            bad.as_bytes(),
            1,
            &mut s
        )
        .is_err()
    );
}
const ASSETS: &[u8] = br#"{"code":"00000","data":{"usdtEquity":"100","accountEquity":"200","mmr":"0.1","assets":[{"coin":"USDT","balance":"100","available":"90","locked":"0","debt":"0"}]}}"#;
const POSITIONS: &[u8] = br#"{"code":"00000","data":{"list":[{"symbol":"BTCUSDT","holdMode":"one_way_mode","marginMode":"crossed","posSide":"short","total":"0.001","avgPrice":"60000","unrealisedPnl":"-1","liquidationPrice":"100000"}]}}"#;
#[test]
fn bgt_146_margin_report_and_position_items() {
    let rd = account_rd(false);
    let mut s = State::default();
    P::account_decode(rd, ACCT, &Query::Balances, 200, ASSETS, 10, &mut s).unwrap();
    P::account_decode(rd, ACCT, &Query::Positions, 200, POSITIONS, 10, &mut s).unwrap();
    let m = s.margin.unwrap();
    assert_eq!(
        (
            m.margin_balance.0,
            m.maint_margin.0,
            m.avail_margin.0,
            m.margin_ratio.0
        ),
        (100_000_000, 50_000, 90_000_000, 500_000)
    );
    assert_eq!(s.margin_items.len(), 1);
    assert_eq!(
        (
            s.margin_items[0].inst,
            s.margin_items[0].liq_px,
            s.margin_items[0].adl_quantile
        ),
        (InstId(1), Px(10_000_000), 255)
    );
    assert_eq!(s.positions[0].qty, Qty(-100_000));
    assert_eq!(s.positions[1].qty, Qty(0));
    P::account_decode(
        rd,
        ACCT,
        &Query::Positions,
        200,
        br#"{"code":"00000","data":{"list":[]}}"#,
        20,
        &mut s,
    )
    .unwrap();
    assert!(s.margin_items.is_empty());
    assert!(s.positions.iter().all(|p| p.qty.0 == 0));
}
#[test]
fn bgt_149_margin_failures() {
    let rd = account_rd(false);
    for b in [
        b"bad".to_vec(),
        String::from_utf8(POSITIONS.to_vec())
            .unwrap()
            .replace("0.001", "-0.001")
            .into_bytes(),
        String::from_utf8(POSITIONS.to_vec())
            .unwrap()
            .replace("BTCUSDT", "UNKNOWN")
            .into_bytes(),
    ] {
        assert!(P::account_decode(rd, ACCT, &Query::Positions, 200, &b, 1, &mut State::default()).is_err());
    }
    assert_eq!(
        P::account_decode(rd, ACCT, &Query::Balances, 500, ASSETS, 1, &mut State::default()),
        Err(DecodeErr::Shape)
    );
    let b = String::from_utf8(ASSETS.to_vec())
        .unwrap()
        .replace("\"debt\":\"0\"", "\"debt\":\"1\"");
    assert!(
        P::account_decode(
            rd,
            ACCT,
            &Query::Balances,
            200,
            b.as_bytes(),
            1,
            &mut State::default()
        )
        .is_err()
    );
}

#[test]
fn bgt_147_margin_ratio_boundaries() {
    for (equity, maintenance, want) in [
        ("10000", "0", 0),
        ("0", "250", i64::MAX),
        ("10000", "20000", 2_000_000_000),
        ("10000", "10000", 1_000_000_000),
        ("0.000001", "0", 0),
        ("10000", "250", 25_000_000),
    ] {
        let mut body: serde_json::Value = serde_json::from_slice(ASSETS).unwrap();
        body["data"]["usdtEquity"] = equity.into();
        body["data"]["accountEquity"] = "10000".into();
        body["data"]["mmr"] = maintenance.into();
        body["data"]["marginRatio"] = "0.9".into();
        let mut s = State::default();
        P::account_decode(
            account_rd(false),
            ACCT,
            &Query::Balances,
            200,
            &serde_json::to_vec(&body).unwrap(),
            1,
            &mut s,
        )
        .unwrap();
        assert_eq!(s.margin.unwrap().margin_ratio.0, want);
    }
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(1000))]
    #[test]
    fn bgt_148_margin_i128_reference(equity in 1i64..1_000_000_000, maintenance in 0i64..1_000_000_000) {
        let mut body: serde_json::Value = serde_json::from_slice(ASSETS).unwrap();
        body["data"]["usdtEquity"] = equity.to_string().into();
        body["data"]["accountEquity"] = equity.to_string().into();
        body["data"]["mmr"] = maintenance.to_string().into();
        let mut s = State::default();
        P::account_decode(rd(), ACCT, &Query::Balances, 200, &serde_json::to_vec(&body).unwrap(), 1, &mut s).unwrap();
        let want = (i128::from(maintenance) * 1_000_000_000 / i128::from(equity)).min(i128::from(i64::MAX)) as i64;
        proptest::prop_assert_eq!(s.margin.unwrap().margin_ratio.0, want);
    }
}

#[test]
fn bgt_150_account_pages_commit_only_after_complete_validation() {
    let rd = account_rd(false);
    let mut s = State::default();
    for (q, body) in [
        (Query::Settings, settings(Some("acct-0001"))),
        (Query::Balances, ASSETS.to_vec()),
        (Query::Positions, POSITIONS.to_vec()),
    ] {
        P::account_decode(rd, ACCT, &q, 200, &body, 10, &mut s).unwrap();
    }
    let before = format!("{s:?}");
    let mut balances: serde_json::Value = serde_json::from_slice(ASSETS).unwrap();
    let mut duplicate = balances["data"]["assets"][0].clone();
    duplicate["balance"] = "200".into();
    balances["data"]["assets"].as_array_mut().unwrap().push(duplicate);
    let mut positions: serde_json::Value = serde_json::from_slice(POSITIONS).unwrap();
    let mut bad = positions["data"]["list"][0].clone();
    bad["symbol"] = "ETHUSDT".into();
    bad["total"] = "bad".into();
    positions["data"]["list"].as_array_mut().unwrap().push(bad);
    let mut config: serde_json::Value = serde_json::from_slice(&settings(Some("acct-0001"))).unwrap();
    config["data"]["symbolConfigList"][0]["leverage"] = "3".into();
    config["data"]["symbolConfigList"][1]["marginMode"] = "isolated".into();
    for (q, body) in [
        (Query::Balances, serde_json::to_vec(&balances).unwrap()),
        (Query::Positions, serde_json::to_vec(&positions).unwrap()),
        (Query::Settings, serde_json::to_vec(&config).unwrap()),
        (Query::Permissions, permissions(Some("wrong"), false)),
    ] {
        assert!(
            P::account_decode(rd, ACCT, &q, 200, &body, 11, &mut s).is_err(),
            "{q:?}"
        );
        assert_eq!(format!("{s:?}"), before, "{q:?}");
    }
    // Repeating a valid observation replaces, rather than appends to, each page.
    for _ in 0..1000 {
        P::account_decode(rd, ACCT, &Query::Balances, 200, ASSETS, 10, &mut s).unwrap();
        P::account_decode(rd, ACCT, &Query::Positions, 200, POSITIONS, 10, &mut s).unwrap();
    }
    assert_eq!(format!("{s:?}"), before);
    // Interleaving account snapshots must neither change report dedup nor vice versa.
    use hft_venue_proto::{MsgOp, VenueProtocol};
    const PUSH: &str = r#"{"action":"snapshot","arg":{"instType":"UTA","topic":"order"},"data":[{"category":"usdt-futures","symbol":"BTCUSDT","orderId":"1234567890123","clientOid":"100030a020002a000000078d","price":"65000.1","qty":"0.004","tradeSide":"buy_single","orderType":"limit","timeInForce":"gtc","side":"buy","cumExecQty":"0.004","cumExecValue":"260","avgPrice":"65000","orderStatus":"filled","cancelReason":"","feeDetail":[{"feeCoin":"USDT","fee":"0.04"}],"createdTime":"1790000000400","updatedTime":"1790000000500"}],"ts":1790000000501}"#;
    let mut plain = venue_bitget::GwCtx::default();
    let mut interleaved = venue_bitget::GwCtx::default();
    let class = P::classify_report(MsgOp::Text, &PUSH.as_bytes()[..64]);
    for n in 0..10_000 {
        let mut left = RepSink::default();
        let mut right = RepSink::default();
        let a = P::decode_report(rd, &mut plain, class, PUSH.as_bytes(), &RX, &mut left).unwrap();
        if n % 10 == 0 {
            P::account_decode(rd, ACCT, &Query::Positions, 200, POSITIONS, 10, &mut s).unwrap();
        }
        let b = P::decode_report(rd, &mut interleaved, class, PUSH.as_bytes(), &RX, &mut right).unwrap();
        assert_eq!(a, b);
        assert_eq!(format!("{left:?}"), format!("{right:?}"));
    }
    assert_eq!(format!("{s:?}"), before);
}

#[test]
fn bgt_006_cancel_reply_and_order_page_identity() {
    let q = Query::Cancel {
        symbol: "BTCUSDT".into(),
        order_id: "123".into(),
    };
    let mut s = State::default();
    for (id, valid) in [("123", true), ("124", false), ("", false)] {
        let b = serde_json::to_vec(&serde_json::json!({"code":"00000","data":{"orderId":id}})).unwrap();
        assert_eq!(
            P::account_decode(account_rd(false), ACCT, &q, 200, &b, 1, &mut s).is_ok(),
            valid
        );
    }
    let q = Query::OpenOrders { cursor: "456".into() };
    for data in [
        serde_json::json!({"list":[{"symbol":"BTCUSDT","orderId":"123"}],"cursor":"456"}),
        serde_json::json!({"list":[{"symbol":"BTCUSDT","orderId":"123"},{"symbol":"BTCUSDT","orderId":"123"}]}),
        serde_json::json!({"list":[{"symbol":"BTCUSDT","orderId":"0"}]}),
    ] {
        let b = serde_json::to_vec(&serde_json::json!({"code":"00000","data":data})).unwrap();
        assert!(P::account_decode(account_rd(false), ACCT, &q, 200, &b, 1, &mut s).is_err());
        assert!(s.orders.is_empty() && s.cursor.is_empty());
    }
}

#[test]
fn bgt_144_single_key_identity() {
    use hft_venue_proto::{RestReqBuf, VenueProtocol};
    for supported in [
        P::session_identity_request(&cfg(), &key(), WC.unix_ms, &mut RestReqBuf::default()),
        venue_bitget::BitgetSpot::session_identity_request(
            &cfg(),
            &key(),
            WC.unix_ms,
            &mut RestReqBuf::default(),
        ),
    ] {
        assert_eq!(supported, Ok(false));
    }
    fn check<T: AccountProtocol>(a: AccountId) {
        let mut s = State::default();
        T::account_decode(
            account_rd(false),
            a,
            &Query::Settings,
            200,
            &settings(Some("acct-0001")),
            1,
            &mut s,
        )
        .unwrap();
        T::account_decode(
            account_rd(false),
            a,
            &Query::Permissions,
            200,
            &permissions(Some("acct-0001"), false),
            1,
            &mut s,
        )
        .unwrap();
        assert_eq!(s.config.unwrap().uid_keys, 3);
    }
    check::<P>(ACCT);
    check::<venue_bitget::BitgetSpot>(AccountId(1));
}
