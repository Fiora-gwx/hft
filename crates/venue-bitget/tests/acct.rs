use crate::support::*;
use hft_types::*;
use hft_venue_proto::{
    DecodeErr,
    account::{AccountProtocol, Query, State},
};
use venue_bitget::BitgetUsdtFutures as P;

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
