//! CTP-010–CTP-012.
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path

mod common;

use common::*;
use hft_ctl_proto::codes::detail;
use hft_ctl_proto::schema::REQUEST_FIELDS;
use hft_ctl_proto::{
    AccountSel, ChatType, CmdBody, CmdResponse, EngineInfo, TransportStatus, Uuid, parse_request,
};
use hft_types::RunMode;
use serde_json::{Value, json};

#[test]
fn ctp_010_parse_valid_request() {
    let j = br#"{"v":2,"cmd_id":"5b0a0000-0000-4000-8000-000000000002","issuer":"tg:1002","issued_at_ns":1790000000000000000,"expires_at_ns":1790000030000000000,"account":"acct-a","preview":false,"confirmed":true,"expect_state_version":4182,"view_account":"acct-b","chat_type":"private","cmd":{"type":"SetMode","strategy":"s1-main","group":"*","mode":"Live"}}"#;
    let r = parse_request(j).unwrap();
    assert_eq!(r.v, 2);
    assert_eq!(
        r.cmd_id,
        Uuid::parse_str("5b0a0000-0000-4000-8000-000000000002").unwrap()
    );
    assert_eq!(r.issuer, "tg:1002");
    assert_eq!(r.issued_at_ns, 1_790_000_000_000_000_000);
    assert_eq!(r.expires_at_ns, 1_790_000_030_000_000_000);
    assert_eq!(r.account, AccountSel::Name("acct-a".into()));
    assert!(!r.preview);
    assert!(r.confirmed);
    assert_eq!(r.expect_state_version, Some(4182));
    assert_eq!(r.view_account.as_deref(), Some("acct-b"));
    assert_eq!(r.chat_type, Some(ChatType::Private));
    assert_eq!(
        r.cmd,
        CmdBody::SetMode {
            strategy: Some("s1-main".into()),
            group: Some("*".into()),
            account: None,
            mode: RunMode::Live,
        }
    );
}

/// Renders a schema error the way the gateway does (`invalid_request`, `InvalidArgs`).
fn render_ir(e: &hft_ctl_proto::SchemaErr) -> Value {
    let mut r = CmdResponse::new(
        e.cmd_id.unwrap_or(Uuid::nil()),
        TransportStatus::InvalidRequest,
        None,
        EngineInfo::default(),
    );
    r.detail = e.detail.map(hft_ctl_proto::codes::Detail::code);
    serde_json::from_slice(&r.to_json()).unwrap()
}

#[test]
fn ctp_011_no_token_fields() {
    assert_eq!(
        REQUEST_FIELDS,
        [
            "v",
            "cmd_id",
            "issuer",
            "issued_at_ns",
            "expires_at_ns",
            "account",
            "preview",
            "confirmed",
            "expect_state_version",
            "view_account",
            "chat_type",
            "cmd"
        ]
    );
    // A response with every optional key present: no key names a token.
    let mut resp = CmdResponse::new(
        Uuid::nil(),
        TransportStatus::Ok,
        Some(hft_ctl_proto::ResultCode::Applied),
        EngineInfo::default(),
    );
    resp.detail = Some(hft_ctl_proto::codes::Detail::code(detail::ACCOUNT_REQUIRED));
    resp.seq = Some(1);
    resp.cmd_hash = Some(hft_ctl_proto::Hex32([1; 32]));
    resp.ctl_state_version = Some(1);
    let v: Value = serde_json::from_slice(&resp.to_json()).unwrap();
    let resp_keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    for k in resp_keys.iter().chain(REQUEST_FIELDS.iter()) {
        assert!(!k.contains("token"), "{k}");
    }
    let mut j = e0(
        Some("acct-a"),
        json!({"type":"SetMode","strategy":"s1-main","group":"*","mode":"Live"}),
    );
    j["confirm_token"] = json!("abc");
    let e = parse_request(&serde_json::to_vec(&j).unwrap()).unwrap_err();
    let r = render_ir(&e);
    assert_eq!(r["status"], "invalid_request");
    assert_eq!(r["code"], "InvalidArgs");
    assert!(r.get("detail").is_none());
}

#[test]
fn ctp_012_schema_errors() {
    let base = || {
        e0(
            None,
            json!({"type":"SetMode","strategy":"s1-main","group":"*","mode":"CloseOnly"}),
        )
    };
    let smoke = |ttl: Value| {
        let mut v = base();
        v["account"] = json!("acct-a");
        v["cmd"] = json!({"type":"SmokeOrder","venue":"bitget-spot","inst":"bitget-spot:BTCUSDT","side":"Buy","ttl_ms":ttl});
        v
    };
    let with = |f: &dyn Fn(&mut Value)| {
        let mut v = base();
        f(&mut v);
        serde_json::to_vec(&v).unwrap()
    };
    // (row, body, expected detail; None = success expected)
    let ir = Some(None::<u16>);
    let ud = Some(Some(detail::UNKNOWN_DISCRIMINANT));
    let rows: Vec<(&str, Vec<u8>, Option<Option<u16>>)> = vec![
        ("CTP-012.1", vec![0xFF, 0xFE, 0x7B, 0x7D], ir),
        ("CTP-012.2", b"{".to_vec(), ir),
        ("CTP-012.3", b"[]".to_vec(), ir),
        ("CTP-012.4", with(&|v| v["v"] = json!(1)), ir),
        ("CTP-012.5", with(&|v| v["v"] = json!(3)), ir),
        (
            "CTP-012.6",
            with(&|v| {
                v.as_object_mut().unwrap().remove("cmd_id");
            }),
            ir,
        ),
        (
            "CTP-012.7",
            with(&|v| {
                v.as_object_mut().unwrap().remove("expires_at_ns");
            }),
            ir,
        ),
        (
            "CTP-012.8",
            with(&|v| {
                v.as_object_mut().unwrap().remove("cmd");
            }),
            ir,
        ),
        ("CTP-012.9", with(&|v| v["foo"] = json!(1)), ir),
        ("CTP-012.10", with(&|v| v["cmd"]["bar"] = json!("1")), ir),
        (
            "CTP-012.11",
            with(&|v| v["issued_at_ns"] = json!("1790000000000000000")),
            ir,
        ),
        (
            "CTP-012.12",
            with(&|v| v["expect_state_version"] = json!(4182.0)),
            ir,
        ),
        (
            "CTP-012.13",
            serde_json::to_vec(&smoke(json!(10000))).unwrap(),
            ir,
        ),
        (
            "CTP-012.14",
            serde_json::to_vec(&smoke(json!("1e4"))).unwrap(),
            ir,
        ),
        ("CTP-012.15", with(&|v| v["cmd"]["type"] = json!("Foo")), ud),
        ("CTP-012.16", with(&|v| v["cmd"]["mode"] = json!("Paper")), ud),
        (
            "CTP-012.17",
            with(&|v| {
                v["preview"] = json!(true);
                v["confirmed"] = json!(true);
            }),
            ir,
        ),
        ("CTP-012.18", with(&|v| v["expires_at_ns"] = json!(ISSUED)), ir),
        (
            "CTP-012.19",
            with(&|v| v["expires_at_ns"] = json!(ISSUED - 1)),
            ir,
        ),
        (
            "CTP-012.20",
            with(&|v| v["expires_at_ns"] = json!(ISSUED + 300_000_000_001)),
            ir,
        ),
        (
            "CTP-012.21",
            with(&|v| v["expires_at_ns"] = json!(ISSUED + 300_000_000_000)),
            None,
        ),
        (
            "CTP-012.22",
            with(&|v| v["expires_at_ns"] = json!(ISSUED + 299_999_999_999)),
            None,
        ),
        ("CTP-012.23", with(&|v| v["issuer"] = json!("tg:1002")), ir),
        (
            "CTP-012.24",
            with(&|v| {
                v["issuer"] = json!("tg:1002");
                v["chat_type"] = json!("channel");
            }),
            ud,
        ),
        (
            "CTP-012.25",
            with(&|v| v["issuer"] = json!("cli:abcdefghijklmnopqrstu")),
            ir,
        ),
        (
            "CTP-012.26",
            with(&|v| v["issuer"] = json!("cli:abcdefghijklmnopqrst")),
            None,
        ),
        (
            "CTP-012.27",
            with(&|v| v["cmd"] = json!({"type":"GetSchema","strategy_kind":"probe-taker","kind_id":"65281"})),
            ir,
        ),
        (
            "CTP-012.28",
            with(&|v| v["cmd"] = json!({"type":"GetSchema"})),
            ir,
        ),
        (
            "CTP-012.29",
            with(&|v| v["cmd"] = json!({"type":"ReconcileReport","report_id":"7","items":[]})),
            ir,
        ),
    ];
    for (row, bytes, want) in rows {
        let got = parse_request(&bytes);
        match want {
            None => assert!(got.is_ok(), "{row}: {got:?}"),
            Some(d) => {
                let e = got.expect_err(row);
                assert_eq!(e.detail, d, "{row}: {}", e.why);
                let r = render_ir(&e);
                assert_eq!(r["status"], "invalid_request", "{row}");
                assert_eq!(r["code"], "InvalidArgs", "{row}");
                assert!(r.get("cmd_hash").is_none(), "{row}");
                match d {
                    None => assert!(r.get("detail").is_none(), "{row}"),
                    Some(_) => assert_eq!(r["detail"]["name"], "UnknownDiscriminant", "{row}"),
                }
                if ["CTP-012.1", "CTP-012.2", "CTP-012.6"].contains(&row) {
                    assert_eq!(r["cmd_id"], "00000000-0000-0000-0000-000000000000", "{row}");
                }
            }
        }
    }
    // The 25-byte issuer row checks the issuer length (24 bytes accepted).
    assert_eq!("cli:abcdefghijklmnopqrstu".len(), 25);
    assert_eq!("cli:abcdefghijklmnopqrst".len(), 24);
}
