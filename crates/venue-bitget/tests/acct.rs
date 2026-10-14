use crate::support::*;
use hft_types::*;
use hft_venue_proto::{
    DecodeErr,
    account::{AccountProtocol, Query, State},
};
use venue_bitget::BitgetUsdtFutures as P;

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
