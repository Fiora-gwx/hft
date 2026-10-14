//! CTP-017–CTP-020, CTP-024, CTP-040–CTP-054, CTP-056, CTP-177–CTP-179.
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path

use crate::common::*;
use hft_ctl_proto::codes::detail::*;
use hft_ctl_proto::{MAX_BLOB, Normalized};
use hft_event::{BlobKind, CmdHdr, CmdKind, ControlCmd, EventKind};
use hft_types::{CapSource, KillLevel, RunMode, Side};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[test]
fn ctp_178_reconciliation_receipts_use_one_atomic_record() {
    use crate::common::p0::*;
    use hft_event::{ExternalCause, ExternalFill, ReconcileBatchView};
    use hft_types::{Px, Qty, Tsc};
    use zerocopy::IntoBytes;
    let mut spec = p0_spec();
    let caps = spec.caps.iter_mut().find(|c| c.venue == V_BN_USDM).unwrap();
    caps.flags = caps.flags | hft_refdata::CapFlags::PERP_CASH;
    let rd = spec.load();
    let e = ExternalFill {
        account: ACCT_A,
        venue: V_BN_USDM,
        inst: I_BN_USDM,
        side: Side::Sell,
        cause: ExternalCause::Unknown,
        qty: Qty(100_000),
        px: Px(6_000_000),
        fee: Qty(37),
        fee_asset: USDT,
        _pad: [0; 6],
        exch_order_id: 90,
        trade_id: 7,
        ex_ts_ns: 123,
        rx_tsc: Tsc(0),
        fill_seq: 0,
        cash_qty: Qty(100_000),
        cash_amount: Qty(-999),
    };
    let mut v = e0(
        None,
        json!({"type":"ReconcileReport","report_id":"7","items":[
        {"venue":"binance-usdm","account":"acct-a","item":"Summary","view_seq":"100"},
        {"venue":"binance-usdm","account":"acct-a","item":"Position","inst":"binance-usdm:BTCUSDT",
         "engine_qty":"0.002","exch_qty":"0.001","view_seq":"100","sflags":["AUTO_EXTERNAL"]}],
        "receipts_b64":b64(e.as_bytes())}),
    );
    v["issuer"] = json!("reconciler");
    let req = parse(&v);
    assert_eq!(hft_ctl_proto::parse_request(&req.to_json()).unwrap(), req);
    let n = hft_ctl_proto::normalize(&req, &rd).unwrap();
    let records = n.ring_records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].0, EventKind::RECONCILE_BATCH);
    let b = ReconcileBatchView::read(&records[0].2).unwrap();
    assert_eq!(b.fills().collect::<Vec<_>>(), vec![e]);
    assert_eq!(b.reports().count(), 2);
    assert_eq!(n.cmd_hash, [0; 32]);
    for raw in [vec![], vec![0; 95], vec![0; 96 * 257], vec![0; 96]] {
        v["cmd"]["receipts_b64"] = json!(b64(&raw));
        assert!(hft_ctl_proto::normalize(&parse(&v), &rd).is_err());
    }
}

#[test]
fn ctp_179_reconciliation_receipts_reject_bad_fill_semantics() {
    use crate::common::p0::*;
    use hft_event::{ExternalCause, ExternalFill};
    use hft_types::{AccountId, Px, Qty, Tsc};
    use zerocopy::IntoBytes;
    let mut spec = p0_spec();
    let caps = spec.caps.iter_mut().find(|c| c.venue == V_BN_USDM).unwrap();
    caps.flags = caps.flags | hft_refdata::CapFlags::PERP_CASH;
    let rd = spec.load();
    let good = ExternalFill {
        account: ACCT_A,
        venue: V_BN_USDM,
        inst: I_BN_USDM,
        side: Side::Sell,
        cause: ExternalCause::Unknown,
        qty: Qty(100_000),
        px: Px(6_000_000),
        fee: Qty(37),
        fee_asset: USDT,
        _pad: [0; 6],
        exch_order_id: 90,
        trade_id: 7,
        ex_ts_ns: 123,
        rx_tsc: Tsc(0),
        fill_seq: 0,
        cash_qty: Qty(100_000),
        cash_amount: Qty(-999),
    };
    let req = |e: &ExternalFill| {
        let mut v = e0(
            None,
            json!({"type":"ReconcileReport","report_id":"7","items":[
            {"venue":"binance-usdm","account":"acct-a","item":"Summary","view_seq":"100"},
            {"venue":"binance-usdm","account":"acct-a","item":"Position","inst":"binance-usdm:BTCUSDT",
             "engine_qty":"0.002","exch_qty":"0.001","view_seq":"100","sflags":["AUTO_EXTERNAL"]}],
            "receipts_b64":b64(e.as_bytes())}),
        );
        v["issuer"] = json!("reconciler");
        parse(&v)
    };
    assert!(hft_ctl_proto::normalize(&req(&good), &rd).is_ok());
    let mut rows: Vec<(&str, ExternalFill)> = Vec::new();
    let mut e = good;
    e.qty = Qty(0);
    e.cash_qty = Qty(0);
    rows.push(("CTP-179.1 zero quantity", e));
    let mut e = good;
    e.px = Px(0);
    rows.push(("CTP-179.2 zero price", e));
    let mut e = good;
    e.cash_qty = Qty(99_999);
    rows.push(("CTP-179.3 partial cash coverage", e));
    let mut e = good;
    e.cause = ExternalCause::Manual;
    rows.push(("CTP-179.4 ineligible cause", e));
    let mut e = good;
    e.fill_seq = 1;
    rows.push(("CTP-179.5 trade id with fill_seq", e));
    let mut e = good;
    e.trade_id = 0;
    rows.push(("CTP-179.6 no trade id and no fill_seq", e));
    let mut e = good;
    e.account = AccountId(ACCT_A.0 + 1);
    rows.push(("CTP-179.7 other account", e));
    for (row, e) in rows {
        let Err(x) = hft_ctl_proto::normalize(&req(&e), &rd) else {
            panic!("{row} accepted");
        };
        assert_eq!(x.detail, BAD_VALUE, "{row}");
    }
}

fn dec(n: &Normalized) -> (CmdHdr, ControlCmd<'_>) {
    ControlCmd::decode(&n.record).unwrap()
}

fn scope(sc: &hft_event::CmdScope) -> (u8, u8, u8, u32, u32) {
    (sc.strategy.0, sc.venue.0, sc.account.0, sc.group.0, sc.inst.0)
}

fn cmd_scope(n: &Normalized) -> (u8, u8, u8, u32, u32) {
    match dec(n).1 {
        ControlCmd::SetMode { scope: s, .. }
        | ControlCmd::SetParams { scope: s, .. }
        | ControlCmd::Kill { scope: s, .. }
        | ControlCmd::Resume { scope: s, .. }
        | ControlCmd::HedgeFix { scope: s }
        | ControlCmd::CancelAll { scope: s } => scope(&s),
        c => panic!("no scope in {c:?}"),
    }
}

const ALL: u32 = u32::MAX;

#[test]
fn ctp_017_decimal_to_fixed() {
    let st = |amount: Value| json!({"type":"SimTransfer","from_venue":"binance-usdm","to_venue":"bitget-spot","asset":"BTC","amount":amount});
    let amount = |n: &Normalized| match dec(n).1 {
        ControlCmd::SimTransfer { amount, .. } => amount.0,
        c => panic!("{c:?}"),
    };
    assert_eq!(
        amount(&ok(Some("sim-a"), st(json!("0.5")))),
        50_000_000,
        "CTP-017.1"
    );
    assert_eq!(
        amount(&ok(Some("sim-a"), st(json!("0.00000001")))),
        1,
        "CTP-017.2"
    );
    assert_eq!(
        fail(Some("sim-a"), st(json!("0.000000001"))),
        BAD_VALUE,
        "CTP-017.3"
    );
    assert_eq!(
        amount(&ok(Some("sim-a"), st(json!("46116860184.27387903")))),
        hft_types::MAX_RAW,
        "CTP-017.4"
    );
    assert_eq!(
        fail(Some("sim-a"), st(json!("46116860184.27387904"))),
        BAD_VALUE,
        "CTP-017.5"
    );
    for (row, v) in [("CTP-017.6", json!("abc")), ("CTP-017.7", json!(0.5))] {
        let b = serde_json::to_vec(&e0(Some("sim-a"), st(v))).unwrap();
        let e = hft_ctl_proto::parse_request(&b).unwrap_err();
        assert_eq!(e.detail, None, "{row}");
    }
}

#[test]
fn ctp_018_name_resolution() {
    let set_mode = |s: &str| json!({"type":"SetMode","strategy":s,"group":"*","mode":"CloseOnly"});
    assert_eq!(
        cmd_scope(&ok(Some("acct-a"), set_mode("s1-main"))).0,
        1,
        "CTP-018.1"
    );
    assert_eq!(cmd_scope(&ok(Some("acct-a"), set_mode("1"))).0, 1, "CTP-018.2");
    assert_eq!(
        fail(Some("acct-a"), set_mode("S1-MAIN")),
        UNKNOWN_NAME,
        "CTP-018.3"
    );
    assert_eq!(fail(Some("acct-a"), set_mode("nope")), UNKNOWN_NAME, "CTP-018.4");
    let hedge = json!({"type":"HedgeFix","group":"33554533"});
    assert_eq!(
        dec(&ok(Some("acct-b"), hedge.clone())).0.account.0,
        2,
        "CTP-018.5"
    );
    assert_eq!(dec(&ok(Some("2"), hedge.clone())).0.account.0, 2, "CTP-018.6");
    assert_eq!(fail(Some("nope"), hedge), UNKNOWN_NAME, "CTP-018.7");
    let smoke = |v: &str, i: &str| json!({"type":"SmokeOrder","venue":v,"inst":i,"side":"Buy"});
    let si = |n: &Normalized| match dec(n).1 {
        ControlCmd::SmokeOrder { venue, inst, .. } => (venue.0, inst.0),
        c => panic!("{c:?}"),
    };
    assert_eq!(
        si(&ok(Some("acct-a"), smoke("bitget-spot", "bitget-spot:BTCUSDT"))).0,
        11,
        "CTP-018.8"
    );
    assert_eq!(
        fail(Some("acct-a"), smoke("kraken", "bitget-spot:BTCUSDT")),
        UNKNOWN_NAME,
        "CTP-018.9"
    );
    assert_eq!(
        si(&ok(Some("acct-a"), smoke("bitget-spot", "binance-spot:BTCUSDT"))).1,
        2,
        "CTP-018.10"
    );
    assert_eq!(
        fail(Some("acct-a"), smoke("bitget-spot", "binance-spot:NOPE")),
        UNKNOWN_NAME,
        "CTP-018.11"
    );
    let st = |a: &str| json!({"type":"SimTransfer","from_venue":"binance-usdm","to_venue":"bitget-spot","asset":a,"amount":"1"});
    match dec(&ok(Some("sim-a"), st("BTC"))).1 {
        ControlCmd::SimTransfer { asset, .. } => assert_eq!(asset.0, 2, "CTP-018.12"),
        c => panic!("{c:?}"),
    }
    assert_eq!(fail(Some("sim-a"), st("XYZ")), UNKNOWN_NAME, "CTP-018.13");
}

#[test]
fn ctp_019_star_and_none() {
    let n = ok(
        Some("*"),
        json!({"type":"SetMode","strategy":"*","group":"*","mode":"CloseOnly"}),
    );
    assert_eq!(cmd_scope(&n), (0, 0, 0, ALL, 0), "CTP-019.1");
    assert_eq!(n.hdr.account.0, 0, "CTP-019.1");
    let n = ok(Some("acct-a"), json!({"type":"CancelAll","venue":"*"}));
    assert_eq!(cmd_scope(&n), (0, 0, 1, ALL, 0), "CTP-019.2");
    let n = ok(
        Some("acct-a"),
        json!({"type":"StrategyCmd","strategy":"s1-main","group":"none","reduce_only":true,"payload_b64":"AQ=="}),
    );
    match dec(&n).1 {
        ControlCmd::StrategyCmd { group, .. } => assert_eq!(group.0, 0, "CTP-019.3"),
        c => panic!("{c:?}"),
    }
    let adopt = |g: Option<&str>| {
        let mut c = json!({"type":"AdoptExchange","venue":"bitget-spot","report_id":"7","item":"Balance","asset":"BTC"});
        if let Some(g) = g {
            c["group"] = json!(g);
        }
        c
    };
    for (row, g) in [("CTP-019.4", None), ("CTP-019.5", Some("none"))] {
        match dec(&ok(Some("acct-a"), adopt(g))).1 {
            ControlCmd::AdoptExchange { group, .. } => assert_eq!(group.0, 0, "{row}"),
            c => panic!("{c:?}"),
        }
    }
    assert_eq!(
        fail(
            Some("acct-a"),
            json!({"type":"SetMode","strategy":"s1-main","group":"none","mode":"CloseOnly"})
        ),
        BAD_GROUP,
        "CTP-019.6"
    );
    assert_eq!(
        fail(
            Some("acct-a"),
            json!({"type":"StrategyCmd","strategy":"s1-main","group":"*","reduce_only":true,"payload_b64":"AQ=="})
        ),
        BAD_GROUP,
        "CTP-019.7"
    );
}

#[test]
fn ctp_020_group_decimal() {
    let sm = |s: &str, g: &str| json!({"type":"SetMode","strategy":s,"group":g,"mode":"CloseOnly"});
    let acct = |s: &str| if s == "s2-main" { "acct-b" } else { "acct-a" };
    let okg = |row: &str, s: &str, g: &str| -> (u8, u32) {
        let n = norm(Some(acct(s)), sm(s, g)).unwrap_or_else(|e| panic!("{row}: {e:?}"));
        let sc = cmd_scope(&n);
        (sc.0, sc.3)
    };
    let bad = |s: &str, g: &str| fail(Some(acct(s)), sm(s, g));
    assert_eq!(okg("CTP-020.1", "s1-main", "16777317").1, 0x0100_0065);
    assert_eq!(okg("CTP-020.2", "s1-main", "33554430").1, 0x01FF_FFFE);
    assert_eq!(bad("s1-main", "33554431"), BAD_GROUP, "CTP-020.3");
    assert_eq!(bad("s1-main", "16777216"), BAD_GROUP, "CTP-020.4");
    assert_eq!(okg("CTP-020.5", "s1-main", "16777217").1, 0x0100_0001);
    assert_eq!(bad("s2-main", "16777317"), BAD_GROUP, "CTP-020.6");
    assert_eq!(okg("CTP-020.7", "*", "16777317"), (0, 0x0100_0065));
    assert_eq!(bad("*", "4278190081"), BAD_GROUP, "CTP-020.12");
    assert_eq!(bad("*", "67108865"), BAD_GROUP, "CTP-020.13");
    assert_eq!(bad("s1-main", "0"), BAD_GROUP, "CTP-020.8");
    assert_eq!(bad("s1-main", "4294967295"), BAD_GROUP, "CTP-020.9");
    assert_eq!(bad("s1-main", "4294967296"), BAD_VALUE, "CTP-020.10");
    let b = serde_json::to_vec(&e0(Some("acct-a"), sm("s1-main", "0x01000065"))).unwrap();
    assert!(hft_ctl_proto::parse_request(&b).is_err(), "CTP-020.11");
}

#[test]
fn ctp_024_view_account_audit_only() {
    let r1 = e0(Some("acct-a"), json!({"type":"HedgeFix","group":"16777317"}));
    let mut r2 = r1.clone();
    r2["view_account"] = json!("acct-b");
    r2["chat_type"] = json!("group");
    let mut r3 = r1.clone();
    r3["view_account"] = json!("zzz");
    let n: Vec<Normalized> = [r1, r2, r3]
        .iter()
        .map(|r| hft_ctl_proto::normalize(&parse(r), rd()).unwrap())
        .collect();
    assert_eq!(n[0].record, n[1].record);
    assert_eq!(n[0].record, n[2].record);
    assert_eq!(n[0].cmd_hash, n[1].cmd_hash);
    assert_eq!(n[0].cmd_hash, n[2].cmd_hash);
    assert_eq!(n[0].hdr.account.0, 1);
}

#[test]
fn ctp_040_set_mode() {
    let sm = |s: &str, g: &str, m: &str| json!({"type":"SetMode","strategy":s,"group":g,"mode":m});
    let n = ok(Some("acct-a"), sm("s1-main", "16777317", "Live"));
    let (h, c) = dec(&n);
    assert_eq!(
        (h.cmd, h.cmd_ver, h.payload_len),
        (CmdKind::SET_MODE, 1, 24),
        "CTP-040.1"
    );
    match c {
        ControlCmd::SetMode { scope: s, mode } => {
            assert_eq!(scope(&s), (1, 0, 1, 0x0100_0065, 0), "CTP-040.1");
            assert_eq!(mode as u8, 4, "CTP-040.1");
        }
        c => panic!("{c:?}"),
    }
    let mode = |n: &Normalized| match dec(n).1 {
        ControlCmd::SetMode { mode, .. } => mode,
        c => panic!("{c:?}"),
    };
    assert_eq!(
        mode(&ok(Some("acct-a"), sm("s1-main", "16777317", "Shadow"))) as u8,
        1,
        "CTP-040.2"
    );
    assert_eq!(
        mode(&ok(Some("acct-a"), sm("s1-main", "16777317", "CloseOnly"))) as u8,
        3,
        "CTP-040.3"
    );
    assert_eq!(
        fail(Some("acct-b"), sm("s1-main", "*", "Live")),
        ACCOUNT_BOUND,
        "CTP-040.4"
    );
    assert_eq!(
        cmd_scope(&ok(Some("acct-a"), sm("*", "*", "CloseOnly"))),
        (0, 0, 1, ALL, 0),
        "CTP-040.5"
    );
    assert_eq!(
        cmd_scope(&ok(Some("sim-a"), sm("s9-sim", "*", "Live"))),
        (3, 0, 3, ALL, 0),
        "CTP-040.6"
    );
    let _ = RunMode::Live;
}

fn set_params(blob: &[u8]) -> Value {
    json!({"type":"SetParams","strategy":"s1-main","blob_b64":b64(blob)})
}

#[test]
fn ctp_041_set_params() {
    let b = spb1(1, 1, 42, 16);
    let n = ok(Some("acct-a"), set_params(&b));
    let (h, c) = dec(&n);
    assert_eq!((h.cmd, h.payload_len), (CmdKind::SET_PARAMS, 64), "CTP-041.1");
    match c {
        ControlCmd::SetParams { scope: s, blob } => {
            assert_eq!(scope(&s), (1, 0, 1, ALL, 0), "CTP-041.1");
            assert_eq!(blob.kind, BlobKind::StrategyParams, "CTP-041.1");
            assert_eq!(
                (blob.encoding, blob.len, blob.version),
                (1, b.len() as u32, 42),
                "CTP-041.1"
            );
            assert_eq!(blob.sha256, <[u8; 32]>::from(Sha256::digest(&b)), "CTP-041.1");
        }
        c => panic!("{c:?}"),
    }
    assert_eq!(n.params_blob(), Some(&b[..]), "CTP-041.1");
    let cat: Vec<u8> = n.blobs.iter().flat_map(|p| p.content().to_vec()).collect();
    assert_eq!(cat, b, "CTP-041.1");
    let mut c = set_params(&b);
    c["base_version"] = json!("41");
    assert_eq!(ok(Some("acct-a"), c).hdr.base_version, 41, "CTP-041.2");
    assert_eq!(n.hdr.base_version, 0, "CTP-041.3");
    assert_eq!(
        fail(Some("acct-a"), set_params(&spb1(2, 1, 42, 16))),
        SCOPE_MISMATCH,
        "CTP-041.4"
    );
    assert_eq!(
        fail(Some("acct-a"), set_params(&spb1(1, 2, 42, 16))),
        SCOPE_MISMATCH,
        "CTP-041.5"
    );
    let mut c = set_params(&b);
    c["blob_b64"] = json!("%%%");
    assert_eq!(fail(Some("acct-a"), c), BAD_VALUE, "CTP-041.6");
    assert_eq!(fail(Some("acct-a"), set_params(&b[..8])), BAD_VALUE, "CTP-041.7");
    let mut big = b.clone();
    big.resize(MAX_BLOB, 0);
    let n = ok(Some("acct-a"), set_params(&big));
    match dec(&n).1 {
        ControlCmd::SetParams { blob, .. } => assert_eq!(blob.len, 786_432, "CTP-041.8"),
        c => panic!("{c:?}"),
    }
    big.push(0);
    assert_eq!(fail(Some("acct-a"), set_params(&big)), TOO_LONG, "CTP-041.9");
    assert_eq!(fail(Some("acct-b"), set_params(&b)), ACCOUNT_BOUND, "CTP-041.10");
}

#[test]
fn ctp_042_set_risk() {
    let r = bytes(2_048, 42);
    let sr = |b: &str| json!({"type":"SetRisk","blob_b64":b});
    let n = ok(Some("acct-a"), sr(&b64(&r)));
    let (h, c) = dec(&n);
    assert_eq!(
        (h.cmd, h.payload_len, h.account.0),
        (CmdKind::SET_RISK, 48, 1),
        "CTP-042.1"
    );
    match c {
        ControlCmd::SetRisk { blob } => {
            assert_eq!(blob.kind, BlobKind::RiskParams, "CTP-042.1");
            assert_eq!(blob.len, 2_048, "CTP-042.1");
            assert_eq!(blob.sha256, <[u8; 32]>::from(Sha256::digest(&r)), "CTP-042.1");
        }
        c => panic!("{c:?}"),
    }
    let cat: Vec<u8> = n.blobs.iter().flat_map(|p| p.content().to_vec()).collect();
    assert_eq!(cat, r, "CTP-042.1");
    assert_eq!(ok(Some("*"), sr(&b64(&r))).hdr.account.0, 0, "CTP-042.2");
    assert_eq!(
        fail(Some("acct-a"), sr(&b64(&vec![0; 786_433]))),
        TOO_LONG,
        "CTP-042.3"
    );
    assert_eq!(fail(Some("acct-a"), sr("!!")), BAD_VALUE, "CTP-042.4");
}

fn kill(n: &Normalized) -> ((u8, u8, u8, u32, u32), KillLevel, bool, bool, bool) {
    match dec(n).1 {
        ControlCmd::Kill {
            scope: s,
            level,
            flatten,
            cancel_resting,
            hard,
        } => (scope(&s), level, flatten, cancel_resting, hard),
        c => panic!("{c:?}"),
    }
}

#[test]
fn ctp_043_kill_combinations() {
    let n = ok(None, json!({"type":"Kill","level":"Global"}));
    let (h, _) = dec(&n);
    assert_eq!(
        (h.cmd, h.cmd_ver, h.payload_len),
        (CmdKind::KILL, 2, 24),
        "CTP-043.1"
    );
    assert_eq!(
        kill(&n),
        ((0, 0, 0, ALL, 0), KillLevel::Global, false, true, false),
        "CTP-043.1"
    );
    let k = kill(&ok(Some("acct-a"), json!({"type":"Kill","level":"Account"})));
    assert_eq!((k.0.2, k.1 as u8, k.3), (1, 3, true), "CTP-043.2");
    let k = kill(&ok(
        Some("*"),
        json!({"type":"Kill","level":"Venue","venue":"bitget-spot"}),
    ));
    assert_eq!((k.0, k.1 as u8), ((0, 11, 0, ALL, 0), 2), "CTP-043.3");
    let k = kill(&ok(
        Some("acct-a"),
        json!({"type":"Kill","level":"Venue","venue":"bitget-spot"}),
    ));
    assert_eq!(k.0, (0, 11, 1, ALL, 0), "CTP-043.4");
    assert_eq!(
        fail(Some("*"), json!({"type":"Kill","level":"Venue"})),
        BAD_COMBINATION,
        "CTP-043.5"
    );
    assert_eq!(
        fail(Some("*"), json!({"type":"Kill","level":"Venue","venue":"*"})),
        BAD_COMBINATION,
        "CTP-043.6"
    );
    let k = kill(&ok(None, json!({"type":"Kill","level":"Global","hard":true})));
    assert_eq!((k.4, k.1 as u8), (true, 4), "CTP-043.7");
    let k = kill(&ok(
        Some("acct-a"),
        json!({"type":"Kill","level":"Account","hard":true}),
    ));
    assert_eq!((k.4, k.1 as u8), (true, 3), "CTP-043.8");
    assert_eq!(
        fail(
            Some("*"),
            json!({"type":"Kill","level":"Venue","venue":"bitget-spot","hard":true})
        ),
        BAD_COMBINATION,
        "CTP-043.9"
    );
    assert!(
        kill(&ok(
            Some("acct-a"),
            json!({"type":"Kill","level":"Account","flatten":true})
        ))
        .2,
        "CTP-043.10"
    );
    assert_eq!(
        fail(None, json!({"type":"Kill","level":"Global","flatten":true})),
        BAD_COMBINATION,
        "CTP-043.11"
    );
    assert_eq!(
        fail(
            Some("acct-a"),
            json!({"type":"Kill","level":"Venue","venue":"bitget-spot","flatten":true})
        ),
        BAD_COMBINATION,
        "CTP-043.12"
    );
    assert_eq!(
        fail(
            Some("acct-a"),
            json!({"type":"Kill","level":"Account","flatten":true,"hard":true})
        ),
        BAD_COMBINATION,
        "CTP-043.13"
    );
    assert!(
        !kill(&ok(
            Some("acct-a"),
            json!({"type":"Kill","level":"Account","cancel_resting":false})
        ))
        .3,
        "CTP-043.14"
    );
    let schema = |c: Value, a: Option<&str>| {
        let b = serde_json::to_vec(&e0(a, c)).unwrap();
        hft_ctl_proto::parse_request(&b).unwrap_err().detail
    };
    assert_eq!(
        schema(json!({"type":"Kill","level":"Strategy"}), None),
        Some(UNKNOWN_DISCRIMINANT),
        "CTP-043.15"
    );
    assert_eq!(schema(json!({"type":"Kill"}), None), None, "CTP-043.16");
    assert_eq!(
        schema(
            json!({"type":"Kill","level":"Account","hard":"true"}),
            Some("acct-a")
        ),
        None,
        "CTP-043.17"
    );
}

#[test]
fn ctp_044_resume() {
    let res = |n: &Normalized| match dec(n).1 {
        ControlCmd::Resume { scope: s, source } => (scope(&s), source.0),
        c => panic!("{c:?}"),
    };
    let n = ok(Some("acct-a"), json!({"type":"Resume","source":"Kill"}));
    assert_eq!(
        (dec(&n).0.cmd, dec(&n).0.payload_len),
        (CmdKind::RESUME, 24),
        "CTP-044.1"
    );
    assert_eq!(res(&n), ((0, 0, 1, ALL, 0), 1), "CTP-044.1");
    assert_eq!(
        res(&ok(Some("*"), json!({"type":"Resume","source":"Kill"}))).0.2,
        0,
        "CTP-044.2"
    );
    assert_eq!(
        res(&ok(
            Some("acct-a"),
            json!({"type":"Resume","source":"Recon","venue":"bitget-spot"})
        )),
        ((0, 11, 1, ALL, 0), 3),
        "CTP-044.3"
    );
    assert_eq!(
        res(&ok(
            Some("acct-a"),
            json!({"type":"Resume","source":"Halted","strategy":"s1-main","group":"16777317"})
        )),
        ((1, 0, 1, 0x0100_0065, 0), 8),
        "CTP-044.4"
    );
    assert_eq!(
        res(&ok(
            Some("acct-a"),
            json!({"type":"Resume","source":"Broken","group":"16777317"})
        )),
        ((0, 0, 1, 0x0100_0065, 0), 5),
        "CTP-044.5"
    );
    assert_eq!(
        res(&ok(
            Some("*"),
            json!({"type":"Resume","source":"Delisting","venue":"binance-spot","inst":"binance-spot:BTCUSDT"})
        )),
        ((0, 2, 0, ALL, 2), 14),
        "CTP-044.6"
    );
    assert_eq!(
        res(&ok(Some("*"), json!({"type":"Resume","source":"ClockBad"}))).1,
        CapSource::CLOCK_BAD.0,
        "CTP-044.7"
    );
    let b = serde_json::to_vec(&e0(Some("acct-a"), json!({"type":"Resume","source":"Nope"}))).unwrap();
    assert_eq!(
        hft_ctl_proto::parse_request(&b).unwrap_err().detail,
        Some(UNKNOWN_DISCRIMINANT),
        "CTP-044.8"
    );
}

#[test]
fn ctp_045_small_commands() {
    let n = ok(
        Some("acct-a"),
        json!({"type":"CancelAll","venue":"bitget-spot","inst":"bitget-spot:BTCUSDT"}),
    );
    assert_eq!(
        (dec(&n).0.cmd, dec(&n).0.payload_len),
        (CmdKind::CANCEL_ALL, 16),
        "CTP-045.1"
    );
    assert_eq!(cmd_scope(&n), (0, 11, 1, ALL, 4), "CTP-045.1");
    assert_eq!(
        cmd_scope(&ok(Some("*"), json!({"type":"CancelAll"}))),
        (0, 0, 0, ALL, 0),
        "CTP-045.2"
    );
    let drain = |n: &Normalized| match dec(n).1 {
        ControlCmd::Drain { cancel_resting } => cancel_resting,
        c => panic!("{c:?}"),
    };
    let n = ok(None, json!({"type":"Drain"}));
    assert_eq!(
        (dec(&n).0.cmd, dec(&n).0.payload_len),
        (CmdKind::DRAIN, 8),
        "CTP-045.3"
    );
    assert!(drain(&n), "CTP-045.3");
    assert!(
        !drain(&ok(None, json!({"type":"Drain","cancel_resting":false}))),
        "CTP-045.4"
    );
    let n = ok(Some("acct-a"), json!({"type":"HedgeFix","group":"16777317"}));
    assert_eq!(
        (dec(&n).0.cmd, dec(&n).0.payload_len),
        (CmdKind::HEDGE_FIX, 16),
        "CTP-045.5"
    );
    assert_eq!(cmd_scope(&n), (0, 0, 1, 0x0100_0065, 0), "CTP-045.5");
    assert_eq!(
        fail(Some("acct-a"), json!({"type":"HedgeFix","group":"*"})),
        BAD_GROUP,
        "CTP-045.6"
    );
    assert_eq!(
        fail(Some("acct-a"), json!({"type":"HedgeFix"})),
        BAD_GROUP,
        "CTP-045.7"
    );
    let n = ok(
        Some("acct-a"),
        json!({"type":"WriteOffResidual","group":"16777317"}),
    );
    assert_eq!(
        (dec(&n).0.cmd, dec(&n).0.payload_len),
        (CmdKind::WRITE_OFF_RESIDUAL, 8),
        "CTP-045.8"
    );
    match dec(&n).1 {
        ControlCmd::WriteOffResidual { account, group, .. } => {
            assert_eq!((account.0, group.0), (1, 0x0100_0065), "CTP-045.8")
        }
        c => panic!("{c:?}"),
    }
    assert_eq!(
        fail(Some("acct-a"), json!({"type":"WriteOffResidual","group":"*"})),
        BAD_GROUP,
        "CTP-045.9"
    );
    let n = ok(
        Some("acct-a"),
        json!({"type":"SetBalanceBaseline","venue":"bitget-spot"}),
    );
    assert_eq!(
        (dec(&n).0.cmd, dec(&n).0.payload_len),
        (CmdKind::SET_BALANCE_BASELINE, 8),
        "CTP-045.10"
    );
    match dec(&n).1 {
        ControlCmd::SetBalanceBaseline { account, venue } => {
            assert_eq!((account.0, venue.0), (1, 11), "CTP-045.10")
        }
        c => panic!("{c:?}"),
    }
    let n = ok(Some("sim-a"), json!({"type":"SimReset"}));
    assert_eq!(
        (dec(&n).0.cmd, dec(&n).0.payload_len),
        (CmdKind::SIM_RESET, 8),
        "CTP-045.11"
    );
    match dec(&n).1 {
        ControlCmd::SimReset { account } => assert_eq!(account.0, 3, "CTP-045.11"),
        c => panic!("{c:?}"),
    }
    ok(Some("acct-a"), json!({"type":"SimReset"}));
}

#[test]
fn ctp_046_smoke_order() {
    let so = |extra: Value| {
        let mut c =
            json!({"type":"SmokeOrder","venue":"bitget-spot","inst":"bitget-spot:BTCUSDT","side":"Buy"});
        for (k, v) in extra.as_object().unwrap() {
            c[k] = v.clone();
        }
        c
    };
    let f = |n: &Normalized| match dec(n).1 {
        ControlCmd::SmokeOrder {
            account,
            venue,
            inst,
            side,
            ttl_ms,
            ..
        } => (account.0, venue.0, inst.0, side, ttl_ms),
        c => panic!("{c:?}"),
    };
    let n = ok(Some("acct-a"), so(json!({})));
    assert_eq!(
        (dec(&n).0.cmd, dec(&n).0.payload_len),
        (CmdKind::SMOKE_ORDER, 16),
        "CTP-046.1"
    );
    assert_eq!(f(&n), (1, 11, 4, Side::Buy, 10_000), "CTP-046.1");
    assert_eq!(
        f(&ok(Some("acct-a"), so(json!({"ttl_ms":"1"})))).4,
        1,
        "CTP-046.2"
    );
    assert_eq!(
        fail(Some("acct-a"), so(json!({"ttl_ms":"0"}))),
        BAD_VALUE,
        "CTP-046.3"
    );
    assert_eq!(
        f(&ok(Some("acct-a"), so(json!({"ttl_ms":"60000"})))).4,
        60_000,
        "CTP-046.4"
    );
    assert_eq!(
        fail(Some("acct-a"), so(json!({"ttl_ms":"60001"}))),
        BAD_VALUE,
        "CTP-046.5"
    );
    assert_eq!(
        f(&ok(Some("acct-a"), so(json!({"ttl_ms":"59999"})))).4,
        59_999,
        "CTP-046.6"
    );
    assert_eq!(
        fail(Some("acct-a"), so(json!({"ttl_ms":"4294967296"}))),
        BAD_VALUE,
        "CTP-046.7"
    );
    assert_eq!(
        f(&ok(Some("acct-a"), so(json!({"side":"Sell"})))).3 as u8,
        2,
        "CTP-046.8"
    );
    let b = serde_json::to_vec(&e0(Some("acct-a"), so(json!({"side":"Hold"})))).unwrap();
    assert_eq!(
        hft_ctl_proto::parse_request(&b).unwrap_err().detail,
        Some(UNKNOWN_DISCRIMINANT),
        "CTP-046.9"
    );
    ok(Some("acct-a"), so(json!({"inst":"binance-usdm:BTCUSDT"})));
}

#[test]
fn ctp_047_adopt_exchange() {
    let ad = |extra: Value| {
        let mut c = json!({"type":"AdoptExchange","venue":"binance-usdm","report_id":"7"});
        for (k, v) in extra.as_object().unwrap() {
            c[k] = v.clone();
        }
        c
    };
    let f = |n: &Normalized| match dec(n).1 {
        ControlCmd::AdoptExchange {
            account,
            venue,
            inst,
            report_id,
            item,
            asset,
            group,
            entry_px,
            ..
        } => (
            account.0, venue.0, inst.0, report_id, item, asset.0, group.0, entry_px.0,
        ),
        c => panic!("{c:?}"),
    };
    let n = ok(
        Some("acct-a"),
        ad(json!({"item":"Position","inst":"binance-usdm:BTCUSDT"})),
    );
    let h = dec(&n).0;
    assert_eq!(
        (h.cmd, h.cmd_ver, h.payload_len),
        (CmdKind::ADOPT_EXCHANGE, 2, 32),
        "CTP-047.1"
    );
    assert_eq!(f(&n), (1, 1, 1, 7, 0, 0, 0, 0), "CTP-047.1");
    let n = ok(
        Some("acct-a"),
        ad(json!({"venue":"bitget-spot","item":"Balance","asset":"BTC","group":"16777317"})),
    );
    let v = f(&n);
    assert_eq!((v.1, v.4, v.5, v.6, v.2), (11, 1, 2, 0x0100_0065, 0), "CTP-047.2");
    let n = ok(
        Some("acct-a"),
        ad(json!({"venue":"bitget-spot","item":"Balance","asset":"BTC"})),
    );
    assert_eq!(f(&n).6, 0, "CTP-047.3");
    let schema = |c: Value| {
        let b = serde_json::to_vec(&e0(Some("acct-a"), c)).unwrap();
        hft_ctl_proto::parse_request(&b).is_err()
    };
    assert!(schema(ad(json!({"item":"Position"}))), "CTP-047.4");
    assert!(
        schema(ad(json!({"venue":"bitget-spot","item":"Balance"}))),
        "CTP-047.5"
    );
    assert!(
        schema(ad(
            json!({"item":"Position","inst":"binance-usdm:BTCUSDT","report_id":"x7"})
        )),
        "CTP-047.6"
    );
}

#[test]
fn ctp_048_venue_notice() {
    let vn = |extra: Value| {
        let mut c = json!({"type":"VenueNotice","venue":"binance-spot"});
        for (k, v) in extra.as_object().unwrap() {
            c[k] = v.clone();
        }
        c
    };
    let f = |n: &Normalized| match dec(n).1 {
        ControlCmd::VenueNotice {
            venue,
            kind,
            inst,
            a,
            b,
            ..
        } => (venue.0, kind, inst.0, a, b),
        c => panic!("{c:?}"),
    };
    let n = ok(
        None,
        vn(json!({"inst":"binance-spot:BTCUSDT","kind":"Delisting","at_ns":"1790100000000000000"})),
    );
    assert_eq!(
        (dec(&n).0.cmd, dec(&n).0.payload_len),
        (CmdKind::VENUE_NOTICE, 24),
        "CTP-048.1"
    );
    assert_eq!(f(&n), (2, 1, 2, 1_790_100_000_000_000_000, 0), "CTP-048.1");
    assert_eq!(
        f(&ok(
            None,
            vn(json!({"kind":"Delisting","at_ns":"1790100000000000000"}))
        ))
        .2,
        0,
        "CTP-048.2"
    );
    let v = f(&ok(
        Some("*"),
        vn(json!({"kind":"FundingInterval","new_s":"14400"})),
    ));
    assert_eq!((v.1, v.3, v.4), (2, 14_400, 0), "CTP-048.3");
    let v = f(&ok(
        Some("*"),
        vn(json!({"kind":"Maintenance","from_ns":"1790100000000000000","to_ns":"1790103600000000000"})),
    ));
    assert_eq!(
        (v.1, v.3, v.4),
        (3, 1_790_100_000_000_000_000, 1_790_103_600_000_000_000),
        "CTP-048.4"
    );
    let schema = |c: Value, a: Option<&str>| {
        let b = serde_json::to_vec(&e0(a, c)).unwrap();
        hft_ctl_proto::parse_request(&b).unwrap_err().detail
    };
    assert_eq!(
        schema(vn(json!({"kind":"Halt"})), Some("*")),
        Some(UNKNOWN_DISCRIMINANT),
        "CTP-048.5"
    );
    assert_eq!(schema(vn(json!({"kind":"Delisting"})), None), None, "CTP-048.6");
}

#[test]
fn ctp_049_transfers_and_annotate() {
    let n = ok(
        Some("sim-a"),
        json!({"type":"SimTransfer","from_venue":"binance-usdm","to_venue":"bitget-spot","asset":"BTC","amount":"0.5"}),
    );
    assert_eq!(
        (dec(&n).0.cmd, dec(&n).0.payload_len),
        (CmdKind::SIM_TRANSFER, 16),
        "CTP-049.1"
    );
    match dec(&n).1 {
        ControlCmd::SimTransfer {
            account,
            from,
            to,
            asset,
            amount,
        } => assert_eq!(
            (account.0, from.0, to.0, asset.0, amount.0),
            (3, 1, 11, 2, 50_000_000),
            "CTP-049.1"
        ),
        c => panic!("{c:?}"),
    }
    let rt = |r: &str| json!({"type":"RecordTransfer","from":"binance-usdm","to":"bitget-spot","asset":"USDT","amount":"100","ext_ref":r});
    let n = ok(Some("acct-a"), rt("TX123"));
    assert_eq!(
        (dec(&n).0.cmd, dec(&n).0.payload_len),
        (CmdKind::RECORD_TRANSFER, 80),
        "CTP-049.2"
    );
    match dec(&n).1 {
        ControlCmd::RecordTransfer {
            account,
            from,
            to,
            asset,
            amount,
            reference,
        } => {
            assert_eq!(
                (account.0, from.0, to.0, asset.0, amount.0),
                (1, 1, 11, 1, 100_000_000),
                "CTP-049.2"
            );
            let mut want = [0u8; 64];
            want[..5].copy_from_slice(b"TX123");
            assert_eq!(reference, want, "CTP-049.2");
        }
        c => panic!("{c:?}"),
    }
    let r64 = "R".repeat(64);
    match dec(&ok(Some("acct-a"), rt(&r64))).1 {
        ControlCmd::RecordTransfer { reference, .. } => assert_eq!(reference, [b'R'; 64], "CTP-049.3"),
        c => panic!("{c:?}"),
    }
    assert_eq!(fail(Some("acct-a"), rt(&"R".repeat(65))), TOO_LONG, "CTP-049.4");
    let an = |n: &Normalized| match dec(n).1 {
        ControlCmd::Annotate {
            kind,
            text_len,
            digest,
            text,
        } => (kind, text_len, digest, text),
        c => panic!("{c:?}"),
    };
    let n = ok(Some("*"), json!({"type":"Annotate","kind":"OpsNote","text":"hi"}));
    assert_eq!(
        (dec(&n).0.cmd, dec(&n).0.payload_len),
        (CmdKind::ANNOTATE, 296),
        "CTP-049.5"
    );
    let (k, l, d, t) = an(&n);
    let mut want = [0u8; 256];
    want[..2].copy_from_slice(b"hi");
    assert_eq!((k, l, d, t), (1, 2, [0; 32], want), "CTP-049.5");
    let hex = "ab".repeat(32);
    let (k, _, d, _) = an(&ok(
        Some("*"),
        json!({"type":"Annotate","kind":"LocalWrite","text":"x","digest":hex}),
    ));
    assert_eq!((k, d), (2, [0xAB; 32]), "CTP-049.6");
    let t256 = "é".repeat(128);
    assert_eq!(t256.len(), 256);
    let (k, l, _, _) = an(&ok(
        Some("*"),
        json!({"type":"Annotate","kind":"Incident","text":t256}),
    ));
    assert_eq!((k, l), (3, 256), "CTP-049.7");
    assert_eq!(
        fail(
            Some("*"),
            json!({"type":"Annotate","kind":"OpsNote","text":"x".repeat(257)})
        ),
        TOO_LONG,
        "CTP-049.8"
    );
    assert_eq!(
        fail(
            Some("*"),
            json!({"type":"Annotate","kind":"OpsNote","text":"x","digest":"abc"})
        ),
        BAD_VALUE,
        "CTP-049.9"
    );
}

#[test]
fn ctp_050_replace_refdata() {
    let d = bytes(30_000, 50);
    let rr = |b: &[u8], v: Option<&str>| {
        let mut c = json!({"type":"ReplaceRefdata","blob_b64":b64(b)});
        if let Some(v) = v {
            c["version"] = json!(v);
        }
        c
    };
    let n = ok(Some("acct-a"), rr(&d, Some("8")));
    assert_eq!(
        (dec(&n).0.cmd, dec(&n).0.payload_len),
        (CmdKind::REPLACE_REFDATA, 48),
        "CTP-050.1"
    );
    match dec(&n).1 {
        ControlCmd::ReplaceRefdata { blob } => {
            assert_eq!(
                (blob.kind, blob.len, blob.version),
                (BlobKind::Refdata, 30_000, 8),
                "CTP-050.1"
            );
            assert_eq!(blob.sha256, <[u8; 32]>::from(Sha256::digest(&d)), "CTP-050.1");
        }
        c => panic!("{c:?}"),
    }
    let cat: Vec<u8> = n.blobs.iter().flat_map(|p| p.content().to_vec()).collect();
    assert_eq!(cat, d, "CTP-050.1");
    let b = serde_json::to_vec(&e0(Some("acct-a"), rr(&d, None))).unwrap();
    assert!(hft_ctl_proto::parse_request(&b).is_err(), "CTP-050.2");
    assert_eq!(
        fail(Some("acct-a"), rr(&vec![0; 786_433], Some("8"))),
        TOO_LONG,
        "CTP-050.3"
    );
    assert_eq!(ok(Some("*"), rr(&d, Some("8"))).hdr.account.0, 0, "CTP-050.4");
}

#[test]
fn ctp_051_strategy_cmd() {
    let sc = |p: &[u8]| json!({"type":"StrategyCmd","strategy":"s1-main","group":"16777317","reduce_only":false,"payload_b64":b64(p)});
    let f = |n: &Normalized| match dec(n).1 {
        ControlCmd::StrategyCmd {
            instance,
            reduce_only,
            len,
            group,
            payload,
        } => (instance.0, reduce_only, len, group.0, payload),
        c => panic!("{c:?}"),
    };
    let n = ok(Some("acct-a"), sc(&[1, 2, 3]));
    assert_eq!(
        (dec(&n).0.cmd, dec(&n).0.payload_len),
        (CmdKind::STRATEGY_CMD, 264),
        "CTP-051.1"
    );
    let mut want = [0u8; 256];
    want[..3].copy_from_slice(&[1, 2, 3]);
    assert_eq!(f(&n), (1, 0, 3, 0x0100_0065, want), "CTP-051.1");
    assert_eq!(n.hdr.account.0, 1, "CTP-051.1");
    assert_eq!(f(&ok(Some("acct-a"), sc(&[7; 256]))).2, 256, "CTP-051.2");
    assert_eq!(f(&ok(Some("acct-a"), sc(&[7; 255]))).2, 255, "CTP-051.3");
    assert_eq!(fail(Some("acct-a"), sc(&[7; 257])), TOO_LONG, "CTP-051.4");
    let e = f(&ok(Some("acct-a"), sc(&[])));
    assert_eq!((e.2, e.4), (0, [0; 256]), "CTP-051.5");
    let mut c = sc(&[1]);
    c["reduce_only"] = json!(true);
    assert_eq!(f(&ok(Some("acct-a"), c)).1, 1, "CTP-051.6");
    assert_eq!(fail(Some("acct-b"), sc(&[1])), ACCOUNT_BOUND, "CTP-051.7");
    let mut c = sc(&[1]);
    c["group"] = json!("33554533");
    assert_eq!(fail(Some("acct-a"), c), BAD_GROUP, "CTP-051.8");
    let mut c = sc(&[1]);
    c["strategy"] = json!("nope");
    assert_eq!(fail(Some("acct-a"), c), UNKNOWN_NAME, "CTP-051.9");
}

#[test]
fn ctp_052_inject_warmup() {
    let iw = |p: &[u8], extra: Value| {
        let mut c = json!({"type":"InjectWarmup","strategy":"s2-main","local_kind":"3","chunk_no":"0","last":false,"payload_b64":b64(p)});
        for (k, v) in extra.as_object().unwrap() {
            c[k] = v.clone();
        }
        c
    };
    let f = |n: &Normalized| match dec(n).1 {
        ControlCmd::InjectWarmup {
            instance,
            last,
            local_kind,
            chunk_no,
            len,
            ..
        } => (instance.0, last, local_kind, chunk_no, len),
        c => panic!("{c:?}"),
    };
    let n = ok(None, iw(&[5; 4096], json!({})));
    assert_eq!(
        (dec(&n).0.cmd, dec(&n).0.payload_len),
        (CmdKind::INJECT_WARMUP, 4_112),
        "CTP-052.1"
    );
    assert_eq!(f(&n), (2, 0, 3, 0, 4_096), "CTP-052.1");
    assert_eq!(n.hdr.account.0, 0, "CTP-052.1");
    assert_eq!(fail(None, iw(&[5; 4097], json!({}))), TOO_LONG, "CTP-052.2");
    let n = ok(None, iw(&[5; 4095], json!({})));
    assert_eq!((f(&n).4, dec(&n).0.payload_len), (4_095, 4_112), "CTP-052.3");
    assert_eq!(dec(&ok(None, iw(&[5], json!({})))).0.payload_len, 24, "CTP-052.4");
    assert_eq!(f(&ok(None, iw(&[5], json!({"last":true})))).1, 1, "CTP-052.5");
    assert_eq!(
        f(&ok(None, iw(&[5], json!({"local_kind":"65535"})))).2,
        65_535,
        "CTP-052.6"
    );
    assert_eq!(
        fail(None, iw(&[5], json!({"local_kind":"65536"}))),
        BAD_VALUE,
        "CTP-052.7"
    );
    assert_eq!(
        f(&ok(None, iw(&[5], json!({"chunk_no":"4294967295"})))).3,
        4_294_967_295,
        "CTP-052.8"
    );
    assert_eq!(
        fail(None, iw(&[5], json!({"chunk_no":"4294967296"}))),
        BAD_VALUE,
        "CTP-052.9"
    );
    assert_eq!(
        fail(None, iw(&[5], json!({"strategy":"nope"}))),
        UNKNOWN_NAME,
        "CTP-052.10"
    );
}

#[test]
fn ctp_053_reconcile_report() {
    let rr = |items: Value| {
        let mut v = e0(
            None,
            json!({"type":"ReconcileReport","report_id":"7","items":items}),
        );
        v["issuer"] = json!("reconciler");
        v
    };
    let r = parse(&rr(json!([
        {"venue":"binance-usdm","account":"acct-a","item":"Position","severity":"Ignore","inst":"binance-usdm:BTCUSDT","engine_qty":"0","exch_qty":"0","view_seq":"100"},
        {"venue":"bitget-spot","account":"acct-a","item":"Balance","severity":"Ignore","asset":"BTC","engine_qty":"0","exch_qty":"0","view_seq":"100"},
        {"item":"Summary"}
    ])));
    let n = hft_ctl_proto::normalize(&r, rd()).unwrap();
    assert_eq!(n.kind, EventKind::RECONCILE_REPORT, "CTP-053.1");
    assert_eq!(n.reports.len(), 3, "CTP-053.1");
    for (i, x) in n.reports.iter().enumerate() {
        assert_eq!((x.idx, x.count, x.report_id), (i as u16, 3, 7), "CTP-053.1");
    }
    let x = &n.reports[0];
    assert_eq!(
        (x.item, x.severity, x.inst.0, x.view_seq),
        (1, 1, 1, 100),
        "CTP-053.1"
    );
    assert!(n.record.is_empty() && n.cmd_hash == [0; 32], "CTP-053.1");
    let recs = n.ring_records();
    assert_eq!(recs.len(), 3, "CTP-053.1");
    assert!(
        recs.iter().all(|(k, _, _)| *k == EventKind::RECONCILE_REPORT),
        "CTP-053.1"
    );
    let one = |item: Value| hft_ctl_proto::normalize(&parse(&rr(json!([item]))), rd());
    assert_eq!(
        one(json!({"item":"Position","sflags":["AUTO_EXTERNAL"]}))
            .unwrap()
            .reports[0]
            .sflags,
        8,
        "CTP-053.2"
    );
    assert_eq!(
        one(json!({"item":"Summary","sflags":["USERSTREAM_DOWN","TRUNCATED"]}))
            .unwrap()
            .reports[0]
            .sflags,
        3,
        "CTP-053.3"
    );
    let b = serde_json::to_vec(&rr(json!([{"item":"Position","severity":"Severe"}]))).unwrap();
    assert_eq!(
        hft_ctl_proto::parse_request(&b).unwrap_err().detail,
        Some(UNKNOWN_DISCRIMINANT),
        "CTP-053.4"
    );
    assert_eq!(
        one(json!({"item":"Position","account":"nope"}))
            .unwrap_err()
            .detail,
        UNKNOWN_NAME,
        "CTP-053.5"
    );
}

#[test]
fn ctp_054_cmd_hdr_fields() {
    let base = || e0(Some("acct-a"), json!({"type":"Resume","source":"Kill"}));
    let n = hft_ctl_proto::normalize(&parse(&base()), rd()).unwrap();
    let h = dec(&n).0;
    assert_eq!(
        h.cmd_id,
        [0, 0, 0, 0, 0, 0, 0x40, 0, 0x80, 0, 0, 0, 0, 0, 0, 1],
        "CTP-054.1"
    );
    let mut iss = [0u8; 24];
    iss[..9].copy_from_slice(b"cli:alice");
    assert_eq!(h.issuer, iss, "CTP-054.1");
    assert_eq!((h.issued_at_ns, h.expires_at_ns), (ISSUED, EXPIRES), "CTP-054.1");
    assert_eq!((h.expires_tsc.0, h.peer_uid, h.role), (0, 0, 0), "CTP-054.1");
    assert_eq!(
        (h.cmd, h.cmd_ver, h.account.0),
        (CmdKind::RESUME, 1, 1),
        "CTP-054.1"
    );
    assert_eq!((h.confirmed, h.hflags, h.payload_len), (0, 0, 24), "CTP-054.1");
    assert_eq!((h.base_version, h.expect_state_version), (0, 0), "CTP-054.1");
    let with = |f: &dyn Fn(&mut Value)| {
        let mut v = base();
        f(&mut v);
        dec(&hft_ctl_proto::normalize(&parse(&v), rd()).unwrap()).0
    };
    let h = with(&|v| v["preview"] = json!(true));
    assert_eq!((h.hflags, h.confirmed), (1, 0), "CTP-054.2");
    let h = with(&|v| {
        v["confirmed"] = json!(true);
        v["expect_state_version"] = json!(4182);
    });
    assert_eq!(
        (h.confirmed, h.expect_state_version, h.hflags),
        (1, 4182, 0),
        "CTP-054.3"
    );
    let h = with(&|v| v["expect_state_version"] = json!(9));
    assert_eq!((h.confirmed, h.expect_state_version), (0, 9), "CTP-054.4");
    let h = with(&|v| v["cmd"] = json!({"type":"Kill","level":"Account"}));
    assert_eq!(h.cmd_ver, 2, "CTP-054.5");
    let h = with(
        &|v| v["cmd"] = json!({"type":"AdoptExchange","venue":"binance-usdm","report_id":"7","item":"Position","inst":"binance-usdm:BTCUSDT"}),
    );
    assert_eq!(h.cmd_ver, 2, "CTP-054.6");
    // CTP-054.7: payload sizes of hft-event for every core command.
    let sizes = [
        (CmdKind::SET_MODE, 24u32),
        (CmdKind::SET_PARAMS, 64),
        (CmdKind::SET_RISK, 48),
        (CmdKind::KILL, 24),
        (CmdKind::REPLACE_REFDATA, 48),
        (CmdKind::RESUME, 24),
        (CmdKind::HEDGE_FIX, 16),
        (CmdKind::CANCEL_ALL, 16),
        (CmdKind::DRAIN, 8),
        (CmdKind::RECORD_TRANSFER, 80),
        (CmdKind::ANNOTATE, 296),
        (CmdKind::ADOPT_EXCHANGE, 32),
        (CmdKind::SIM_RESET, 8),
        (CmdKind::SIM_TRANSFER, 16),
        (CmdKind::VENUE_NOTICE, 24),
        (CmdKind::SET_BALANCE_BASELINE, 8),
        (CmdKind::STRATEGY_CMD, 264),
        (CmdKind::SMOKE_ORDER, 16),
        (CmdKind::WRITE_OFF_RESIDUAL, 8),
        (CmdKind::SIM_FAULT, 8),
    ];
    let mut seen = 0;
    for (name, acct, cmd) in core_cmds() {
        if name == "GetSchema" || name == "ReconcileReport" {
            continue;
        }
        let n = ok(acct, cmd);
        let h = dec(&n).0;
        let want = if h.cmd == CmdKind::INJECT_WARMUP {
            match dec(&n).1 {
                ControlCmd::InjectWarmup { len, .. } => 16 + u32::from(len).div_ceil(8) * 8,
                _ => unreachable!(),
            }
        } else {
            sizes.iter().find(|(k, _)| *k == h.cmd).unwrap().1
        };
        assert_eq!(h.payload_len, want, "CTP-054.7 {name}");
        assert_eq!(n.record.len(), 128 + want as usize, "CTP-054.7 {name}");
        seen += 1;
    }
    assert!(seen >= 25);
}

#[test]
fn ctp_056_first_error_by_step() {
    assert_eq!(
        fail(None, json!({"type":"HedgeFix","group":"67108965"})),
        ACCOUNT_REQUIRED,
        "CTP-056.1"
    );
    assert_eq!(
        fail(
            Some("sim-a"),
            json!({"type":"SimTransfer","from_venue":"binance-usdm","to_venue":"bitget-spot","asset":"XYZ","amount":"0.000000001"})
        ),
        UNKNOWN_NAME,
        "CTP-056.2"
    );
    assert_eq!(
        fail(Some("*"), set_params(&spb1(1, 2, 42, 16))),
        ACCOUNT_REQUIRED,
        "CTP-056.3"
    );
    let mut big = spb1(2, 1, 42, 16);
    big.resize(786_433, 0);
    assert_eq!(fail(Some("acct-a"), set_params(&big)), TOO_LONG, "CTP-056.4");
    assert_eq!(
        fail(
            Some("acct-b"),
            json!({"type":"StrategyCmd","strategy":"s1-main","group":"16777317","payload_b64":b64(&[1; 257])})
        ),
        ACCOUNT_BOUND,
        "CTP-056.5"
    );
}

#[test]
fn ctp_177_simfault_args() {
    let sf = |f: &str, d: &str| json!({"type":"SimFault","venue":"binance-usdm","fault":f,"delay_ms":d});
    let f = |n: &Normalized| match dec(n).1 {
        ControlCmd::SimFault { fault, delay_ms, .. } => (fault, delay_ms),
        c => panic!("{c:?}"),
    };
    let n = ok(Some("sim-a"), sf("RejectNew", "0"));
    assert_eq!(dec(&n).0.cmd, CmdKind::SIM_FAULT, "CTP-177.1");
    assert_eq!(f(&n), (2, 0), "CTP-177.1");
    assert_eq!(f(&ok(Some("sim-a"), sf("DelayReports", "1"))).1, 1, "CTP-177.2");
    assert_eq!(
        fail(Some("sim-a"), sf("DelayReports", "0")),
        BAD_VALUE,
        "CTP-177.3"
    );
    assert_eq!(
        f(&ok(Some("sim-a"), sf("DelayReports", "60000"))).1,
        60_000,
        "CTP-177.4"
    );
    assert_eq!(
        fail(Some("sim-a"), sf("DelayReports", "60001")),
        BAD_VALUE,
        "CTP-177.5"
    );
    assert_eq!(fail(Some("sim-a"), sf("Disconnect", "5")), BAD_VALUE, "CTP-177.6");
    assert_eq!(f(&ok(Some("sim-a"), sf("Off", "0"))).0, 1, "CTP-177.7");
    let b = serde_json::to_vec(&e0(Some("sim-a"), sf("Pause", "0"))).unwrap();
    assert_eq!(
        hft_ctl_proto::parse_request(&b).unwrap_err().detail,
        Some(UNKNOWN_DISCRIMINANT),
        "CTP-177.8"
    );
}
