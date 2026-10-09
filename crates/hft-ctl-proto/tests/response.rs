//! CTP-013–CTP-016.
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path

use crate::common::*;
use hft_ctl_proto::codes::{Detail, detail};
use hft_ctl_proto::response::render_preview;
use hft_ctl_proto::{CmdResponse, EngineInfo, Hex32, ResultCode, TransportStatus, Uuid, render_detail};
use hft_event::CmdPreview;
use hft_types::{AccountId, InstId, Qty, VenueId};
use serde_json::{Value, json};

fn js(r: &CmdResponse) -> Value {
    serde_json::from_slice(&r.to_json()).unwrap()
}

fn resp(status: TransportStatus, code: Option<ResultCode>) -> CmdResponse {
    CmdResponse::new(Uuid::nil(), status, code, EngineInfo::default())
}

#[test]
fn ctp_013_status_code_mapping() {
    use TransportStatus as S;
    let rows: [(&str, S, Option<ResultCode>, &str, Option<&str>); 9] = [
        (
            "CTP-013.1",
            S::Ok,
            Some(ResultCode::Applied),
            "ok",
            Some("Applied"),
        ),
        ("CTP-013.2", S::Accepted, None, "accepted", None),
        (
            "CTP-013.3",
            S::InvalidRequest,
            None,
            "invalid_request",
            Some("InvalidArgs"),
        ),
        (
            "CTP-013.4",
            S::Unauthorized,
            None,
            "unauthorized",
            Some("Unauthorized"),
        ),
        ("CTP-013.5", S::Busy, None, "busy", Some("Busy")),
        (
            "CTP-013.6",
            S::InternalError,
            None,
            "internal_error",
            Some("Internal"),
        ),
        (
            "CTP-013.7",
            S::Ok,
            Some(ResultCode::Forbidden),
            "ok",
            Some("Forbidden"),
        ),
        (
            "CTP-013.8",
            S::Ok,
            Some(ResultCode::RateLimited),
            "ok",
            Some("RateLimited"),
        ),
        (
            "CTP-013.9",
            S::Ok,
            Some(ResultCode::Expired),
            "ok",
            Some("Expired"),
        ),
    ];
    for (row, s, c, want_s, want_c) in rows {
        let v = js(&resp(s, c));
        assert_eq!(v["status"], want_s, "{row}");
        match want_c {
            Some(n) => assert_eq!(v["code"], n, "{row}"),
            None => assert!(v.get("code").is_none(), "{row}"),
        }
        assert!(v.get("detail").is_none(), "{row}");
    }
    let r = CmdResponse::gateway(
        Uuid::nil(),
        ResultCode::InvalidArgs,
        u32::from(detail::ACCOUNT_REQUIRED),
        EngineInfo::default(),
    );
    let v = js(&r);
    assert_eq!(v["code"], "InvalidArgs", "CTP-013.10");
    assert_eq!(v["detail"], json!({"name":"AccountRequired"}), "CTP-013.10");
}

#[test]
fn ctp_014_detail_rendering() {
    let d = |name: &str, arg: Option<&str>| {
        Some(Detail {
            name: name.into(),
            arg: arg.map(Into::into),
        })
    };
    let rows: Vec<(&str, u32, Option<Detail>)> = vec![
        ("CTP-014.1", 0, None),
        ("CTP-014.2", 1, d("ShadowDeferred", None)),
        (
            "CTP-014.3",
            110 | (2 << 16),
            d("ParamInvalid", Some("ModelMemBudget")),
        ),
        (
            "CTP-014.4",
            110 | (3 << 16),
            d("ParamInvalid", Some("NonProdOnly")),
        ),
        (
            "CTP-014.5",
            110 | ((1 | (0xFFF << 4)) << 16),
            d("ParamInvalid", Some("Strategy")),
        ),
        (
            "CTP-014.6",
            110 | ((1 | (3 << 4)) << 16),
            d("ParamInvalid", Some("Strategy:3")),
        ),
        (
            "CTP-014.7",
            402 | (2 << 16),
            d("RefdataAssembly", Some("MissingCredential")),
        ),
        (
            "CTP-014.8",
            401 | (1 << 16),
            d("RefdataRecheck", Some("Structure")),
        ),
        ("CTP-014.9", 124 | (6 << 16), d("GroupCheck", Some("DanglingRef"))),
        ("CTP-014.10", 403 | (5 << 16), d("SmokeRiskReject", Some("HARD"))),
        ("CTP-014.11", 200, d("StateChanged", None)),
        ("CTP-014.12", 201, d("NoValidPreview", None)),
        ("CTP-014.13", 999, d("999", None)),
        ("CTP-014.14", 306, d("UnknownPresent", None)),
        ("CTP-014.15", 316, d("NotQuiescent", None)),
        ("CTP-014.15", 317, d("Hard", None)),
        ("CTP-014.15", 318, d("NothingToFix", None)),
        ("CTP-014.15", 319, d("QuoteUnavailable", None)),
        ("CTP-014.15", 320, d("ChainLeg", None)),
    ];
    for (row, raw, want) in rows {
        assert_eq!(render_detail(raw), want, "{row}");
        // JSON form: the key is omitted for 0, `arg` omitted when absent.
        let r = CmdResponse::gateway(Uuid::nil(), ResultCode::InvalidArgs, raw, EngineInfo::default());
        let v = js(&r);
        match &want {
            None => assert!(v.get("detail").is_none(), "{row}"),
            Some(w) => assert_eq!(v["detail"], serde_json::to_value(w).unwrap(), "{row}"),
        }
    }
}

#[test]
fn ctp_015_response_cmd_hash() {
    let n = ok(
        Some("acct-a"),
        json!({"type":"SetMode","strategy":"s1-main","group":"16777317","mode":"Live"}),
    );
    let mut r = resp(TransportStatus::Ok, Some(ResultCode::Applied));
    r.cmd_hash = Some(Hex32(n.cmd_hash));
    let v = js(&r);
    let h = v["cmd_hash"].as_str().unwrap();
    assert_eq!(h.len(), 64, "CTP-015.1");
    assert!(
        h.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "CTP-015.1"
    );
    assert_eq!(Hex32::from_hex(h), Some(Hex32(n.cmd_hash)), "CTP-015.1");
    // Read-only, schema-rejected, forbidden, normalize-rejected and ReconcileReport responses
    // are built without a hash: the key is absent.
    for (row, r) in [
        ("CTP-015.2", resp(TransportStatus::Ok, Some(ResultCode::Applied))),
        ("CTP-015.3", resp(TransportStatus::InvalidRequest, None)),
        (
            "CTP-015.4",
            resp(TransportStatus::Ok, Some(ResultCode::Forbidden)),
        ),
        (
            "CTP-015.5",
            CmdResponse::gateway(
                Uuid::nil(),
                ResultCode::InvalidArgs,
                u32::from(detail::UNKNOWN_NAME),
                EngineInfo::default(),
            ),
        ),
        ("CTP-015.6", resp(TransportStatus::Ok, Some(ResultCode::Applied))),
    ] {
        assert!(js(&r).get("cmd_hash").is_none(), "{row}");
    }
    // A ReconcileReport normalizes without a hash (CTP-015.6).
    let rr = ok(
        None,
        json!({"type":"ReconcileReport","report_id":"7","items":[{"item":"Summary"}]}),
    );
    assert_eq!(rr.cmd_hash, [0; 32], "CTP-015.6");
}

fn preview() -> CmdPreview {
    let mut p = <CmdPreview as zerocopy::FromZeros>::new_zeroed();
    p.target_account = AccountId(1);
    p.target_kind = 1;
    p.affected_cells = 3;
    p.open_orders = 2;
    p
}

fn render(p: &CmdPreview, rflags: u32) -> Option<Value> {
    render_preview(p, rflags, rd()).map(|r| serde_json::from_str(r.get()).unwrap())
}

#[test]
fn ctp_016_preview_rendering() {
    let p = preview();
    assert!(render(&p, 0).is_none(), "CTP-016.1");
    let v = render(&p, 1).unwrap();
    assert_eq!(v["target_account"], "acct-a", "CTP-016.2");
    assert_eq!(v["target_kind"], "Live", "CTP-016.2");
    assert!(v.get("confirm_within_s").is_none(), "CTP-016.2");
    let v = render(&p, 5).unwrap();
    assert_eq!(v["confirm_within_s"], 60, "CTP-016.3");
    let v = render(&p, 3).unwrap();
    assert!(v.get("confirm_within_s").is_none(), "CTP-016.4");
    let mut w = p;
    w.wo_venue = VenueId(11);
    w.wo_leg_ix = 1;
    w.wo_inst = InstId(4);
    w.wo_qty = Qty(40_000);
    let v = render(&w, 3).unwrap();
    assert_eq!(v["wo_venue"], "bitget-spot", "CTP-016.5");
    assert_eq!(v["wo_leg_ix"], "1", "CTP-016.5");
    assert_eq!(v["wo_inst"], "bitget-spot:BTCUSDT", "CTP-016.5");
    assert_eq!(v["wo_qty"], "0.0004", "CTP-016.5");
    let v = render(&p, 1).unwrap();
    assert!(
        v.as_object().unwrap().keys().all(|k| !k.starts_with("wo_")),
        "CTP-016.6"
    );
    // The response carries the rendered block under `preview`.
    let mut r = resp(TransportStatus::Ok, Some(ResultCode::Applied));
    r.preview = render_preview(&p, 5, rd());
    assert_eq!(js(&r)["preview"]["confirm_within_s"], 60);
}
