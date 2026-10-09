//! CTP-021–CTP-023, CTP-025.
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path

use crate::common::*;
use hft_ctl_proto::codes::detail::*;
use hft_event::ControlCmd;
use serde_json::{Value, json};

fn acct(a: Option<&str>, c: Value) -> u8 {
    ok(a, c).hdr.account.0
}

#[test]
fn ctp_021_missing_account_by_class() {
    let blob = b64(&bytes(64, 1));
    assert_eq!(
        acct(None, json!({"type":"Kill","level":"Global"})),
        0,
        "CTP-021.1"
    );
    let n = ok(None, json!({"type":"CancelAll","venue":"*"}));
    assert_eq!(n.hdr.account.0, 0, "CTP-021.2");
    match ControlCmd::decode(&n.record).unwrap().1 {
        ControlCmd::CancelAll { scope } => assert_eq!(scope.account.0, 0, "CTP-021.2"),
        c => panic!("{c:?}"),
    }
    assert_eq!(
        acct(
            None,
            json!({"type":"SetMode","strategy":"*","group":"*","mode":"CloseOnly"})
        ),
        0,
        "CTP-021.3"
    );
    assert_eq!(
        acct(
            None,
            json!({"type":"VenueNotice","venue":"binance-spot","kind":"Delisting","at_ns":"1790100000000000000"})
        ),
        0,
        "CTP-021.4"
    );
    let n = ok(None, json!({"type":"Drain"}));
    assert_eq!(n.hdr.account.0, 0, "CTP-021.5");
    assert_eq!(
        ControlCmd::decode(&n.record).unwrap().1,
        ControlCmd::Drain { cancel_resting: true },
        "CTP-021.5"
    );
    assert_eq!(
        acct(
            None,
            json!({"type":"InjectWarmup","strategy":"s2-main","local_kind":"3","chunk_no":"0","payload_b64":"AQ=="})
        ),
        0,
        "CTP-021.6"
    );
    let live = json!({"type":"SetMode","strategy":"*","group":"*","mode":"Live"});
    assert_eq!(fail(None, live.clone()), ACCOUNT_REQUIRED, "CTP-021.7");
    assert_eq!(acct(Some("*"), live), 0, "CTP-021.8");
    let resume = json!({"type":"Resume","source":"Kill"});
    assert_eq!(fail(None, resume.clone()), ACCOUNT_REQUIRED, "CTP-021.9");
    assert_eq!(acct(Some("*"), resume), 0, "CTP-021.10");
    let risk = json!({"type":"SetRisk","blob_b64":blob});
    assert_eq!(fail(None, risk.clone()), ACCOUNT_REQUIRED, "CTP-021.11");
    assert_eq!(acct(Some("*"), risk), 0, "CTP-021.12");
    let note = json!({"type":"Annotate","kind":"OpsNote","text":"x"});
    assert_eq!(fail(None, note.clone()), ACCOUNT_REQUIRED, "CTP-021.13");
    assert_eq!(acct(Some("*"), note), 0, "CTP-021.14");
    let fi = json!({"type":"VenueNotice","venue":"binance-spot","kind":"FundingInterval","new_s":"14400"});
    assert_eq!(fail(None, fi.clone()), ACCOUNT_REQUIRED, "CTP-021.15");
    assert_eq!(acct(Some("*"), fi), 0, "CTP-021.16");
    assert_eq!(
        fail(
            None,
            json!({"type":"ReplaceRefdata","blob_b64":blob,"version":"8"})
        ),
        ACCOUNT_REQUIRED,
        "CTP-021.17"
    );
}

#[test]
fn ctp_022_concrete_account_required() {
    let rows: Vec<(&str, Value)> = vec![
        (
            "CTP-022.1",
            json!({"type":"SetParams","strategy":"s1-main","blob_b64":spb1_b64(1, 1, 42, 16)}),
        ),
        ("CTP-022.2", json!({"type":"HedgeFix","group":"16777317"})),
        ("CTP-022.3", json!({"type":"WriteOffResidual","group":"16777317"})),
        (
            "CTP-022.4",
            json!({"type":"SmokeOrder","venue":"bitget-spot","inst":"bitget-spot:BTCUSDT","side":"Buy"}),
        ),
        (
            "CTP-022.5",
            json!({"type":"AdoptExchange","venue":"bitget-spot","report_id":"7","item":"Balance","asset":"BTC"}),
        ),
        (
            "CTP-022.6",
            json!({"type":"SetBalanceBaseline","venue":"bitget-spot"}),
        ),
        ("CTP-022.7", json!({"type":"SimReset"})),
        (
            "CTP-022.8",
            json!({"type":"SimTransfer","from_venue":"binance-usdm","to_venue":"bitget-spot","asset":"BTC","amount":"1"}),
        ),
        (
            "CTP-022.9",
            json!({"type":"RecordTransfer","from":"binance-usdm","to":"bitget-spot","asset":"USDT","amount":"1","ext_ref":"x"}),
        ),
        (
            "CTP-022.10",
            json!({"type":"StrategyCmd","strategy":"s1-main","group":"16777317","payload_b64":"AQ=="}),
        ),
        ("CTP-022.11", json!({"type":"Kill","level":"Account"})),
        (
            "CTP-022.15",
            json!({"type":"SimFault","venue":"binance-usdm","fault":"RejectNew"}),
        ),
    ];
    for (row, c) in rows {
        assert_eq!(fail(Some("*"), c), ACCOUNT_REQUIRED, "{row}");
    }
    let missing: Vec<(&str, Value)> = vec![
        ("CTP-022.12", json!({"type":"Kill","level":"Account"})),
        (
            "CTP-022.13",
            json!({"type":"SmokeOrder","venue":"bitget-spot","inst":"bitget-spot:BTCUSDT","side":"Buy"}),
        ),
        (
            "CTP-022.14",
            json!({"type":"WriteOffResidual","group":"16777317"}),
        ),
    ];
    for (row, c) in missing {
        assert_eq!(fail(None, c), ACCOUNT_REQUIRED, "{row}");
    }
}

#[test]
fn ctp_023_account_not_allowed() {
    let iw = json!({"type":"InjectWarmup","strategy":"s1-main","local_kind":"3","chunk_no":"0","last":false,"payload_b64":"AQ=="});
    let rows: Vec<(&str, Value)> = vec![
        ("CTP-023.1", iw.clone()),
        ("CTP-023.2", json!({"type":"Drain"})),
        (
            "CTP-023.3",
            json!({"type":"VenueNotice","venue":"binance-spot","kind":"Delisting","at_ns":"1790100000000000000"}),
        ),
        (
            "CTP-023.4",
            json!({"type":"ReconcileReport","report_id":"7","items":[{"item":"Summary"}]}),
        ),
        ("CTP-023.5", json!({"type":"Kill","level":"Global"})),
    ];
    for (row, c) in rows {
        assert_eq!(fail(Some("acct-a"), c), ACCOUNT_NOT_ALLOWED, "{row}");
    }
    assert_eq!(acct(Some("*"), iw), 0, "CTP-023.6");
}

#[test]
fn ctp_025_envelope_vs_body_account() {
    let so = |body: Option<&str>| {
        let mut c =
            json!({"type":"SmokeOrder","venue":"bitget-spot","inst":"bitget-spot:BTCUSDT","side":"Buy"});
        if let Some(a) = body {
            c["account"] = json!(a);
        }
        c
    };
    let n = ok(Some("acct-a"), so(Some("acct-a")));
    assert_eq!(n.hdr.account.0, 1, "CTP-025.1");
    match ControlCmd::decode(&n.record).unwrap().1 {
        ControlCmd::SmokeOrder { account, .. } => assert_eq!(account.0, 1, "CTP-025.1"),
        c => panic!("{c:?}"),
    }
    assert_eq!(
        fail(Some("acct-a"), so(Some("acct-b"))),
        ACCOUNT_MISMATCH,
        "CTP-025.2"
    );
    assert_eq!(acct(None, so(Some("acct-a"))), 1, "CTP-025.3");
    assert_eq!(acct(Some("acct-a"), so(None)), 1, "CTP-025.4");
    assert_eq!(fail(Some("*"), so(Some("acct-a"))), ACCOUNT_MISMATCH, "CTP-025.5");
    assert_eq!(fail(None, so(None)), ACCOUNT_REQUIRED, "CTP-025.6");
}
