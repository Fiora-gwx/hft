//! CTL-060–CTL-068, CTL-307.
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path

use crate::common::*;
use hft_ctl::ParsedConfig;
use hft_ctl::validate::{EventErr, detail_of_event_err};
use hft_ctl_proto::ResultCode;
use hft_event::{ControlCmd, EventKind, ParamsErr};
use serde_json::{Value, json};

/// Sends; returns (response, records of this request).
fn send(
    g: &mut G0,
    issuer: &str,
    account: Option<&str>,
    cmd: Value,
) -> (hft_ctl_proto::CmdResponse, Vec<RingRec>) {
    let n = g.seen.len();
    let r = g.send(issuer, account, cmd);
    g.take_ring();
    (r, g.seen[n..].to_vec())
}

fn written(recs: &[RingRec]) -> bool {
    recs.iter().any(RingRec::is_cmd)
}

fn invalid(r: &hft_ctl_proto::CmdResponse) -> String {
    assert_eq!(code(r), "InvalidArgs", "{r:?}");
    detail_name(r)
}

#[test]
fn ctl_060_set_risk() {
    let sr = |flags: u8| json!({"type":"SetRisk","blob_b64":b64(&risk_blob(flags))});
    let mut g = G0::new();
    g.paused = true;
    let (_, recs) = send(&mut g, "cli:alice", Some("acct-a"), sr(0));
    let kinds: Vec<EventKind> = recs.iter().map(|r| r.kind).collect();
    let cmd_at = kinds.iter().position(|k| *k == EventKind::CONTROL_CMD).unwrap();
    assert!(
        kinds[..cmd_at].iter().all(|k| *k == EventKind::CONFIG_BLOB) && cmd_at > 0,
        "CTL-060.1 {kinds:?}"
    );
    let blob_sha = match recs[cmd_at].cmd().1 {
        ControlCmd::SetRisk { blob } => blob.sha256,
        c => panic!("{c:?}"),
    };
    match g.mb_take.try_take() {
        Some(ParsedConfig::Risk { sha256, .. }) => assert_eq!(sha256, blob_sha, "CTL-060.1"),
        o => panic!("CTL-060.1: {o:?}"),
    }
    for (row, account, flags, want) in [
        ("CTL-060.2", "acct-a", 1u8, Some("OutOfScope")),
        ("CTL-060.3", "acct-a", 2, Some("OutOfScope")),
        ("CTL-060.4", "*", 2, None),
    ] {
        let mut g = G0::new();
        g.paused = true;
        let (r, recs) = send(&mut g, "cli:alice", Some(account), sr(flags));
        match want {
            Some(d) => {
                assert_eq!(invalid(&r), d, "{row}");
                assert!(!written(&recs), "{row}");
                assert!(g.mb_take.try_take().is_none(), "{row}");
            }
            None => assert!(written(&recs), "{row}"),
        }
    }
    let mut g = G0::new();
    let (r, _) = send(
        &mut g,
        "cli:alice",
        Some("acct-a"),
        json!({"type":"SetRisk","blob_b64":b64(&[3u8; 2048])}),
    );
    assert_eq!(invalid(&r), "BadValue", "CTL-060.5");
}

#[test]
fn ctl_061_sim_commands() {
    let st = |from: &str, to: &str| json!({"type":"SimTransfer","from_venue":from,"to_venue":to,"asset":"USDT","amount":"100"});
    let sf = |v: &str| json!({"type":"SimFault","venue":v,"fault":"RejectNew"});
    let rows: Vec<(&str, &str, &str, Value, Option<&str>)> = vec![
        ("CTL-061.1", "cli:tom", "sim-a", json!({"type":"SimReset"}), None),
        (
            "CTL-061.2",
            "cli:tom",
            "acct-a",
            json!({"type":"SimReset"}),
            Some("NotSim"),
        ),
        (
            "CTL-061.3",
            "cli:tom",
            "sim-a",
            st("binance-usdm", "bitget-spot"),
            None,
        ),
        (
            "CTL-061.4",
            "cli:tom",
            "acct-a",
            st("binance-usdm", "bitget-spot"),
            Some("NotSim"),
        ),
        (
            "CTL-061.5",
            "cli:tom",
            "sim-a",
            st("binance-spot", "bitget-spot"),
            Some("UnknownName"),
        ),
        (
            "CTL-061.6",
            "cli:tom",
            "sim-a",
            st("binance-usdm", "bitget-usdt-futures"),
            Some("UnknownName"),
        ),
        ("CTL-061.7", "cli:alice", "sim-a", sf("binance-usdm"), None),
        (
            "CTL-061.8",
            "cli:alice",
            "acct-a",
            sf("binance-usdm"),
            Some("NotSim"),
        ),
        (
            "CTL-061.9",
            "cli:alice",
            "sim-a",
            sf("okx-swap"),
            Some("UnknownName"),
        ),
    ];
    for (row, issuer, account, cmd, want) in rows {
        let mut g = G0::new();
        let (r, recs) = send(&mut g, issuer, Some(account), cmd);
        match want {
            None => assert!(written(&recs), "{row}: {r:?}"),
            Some(d) => {
                assert_eq!(invalid(&r), d, "{row}");
                assert!(!written(&recs), "{row}");
            }
        }
    }
}

#[test]
fn ctl_062_venue_notice() {
    let vn = |extra: Value| {
        let mut c = json!({"type":"VenueNotice","venue":"binance-usdm"});
        for (k, v) in extra.as_object().unwrap() {
            c[k] = v.clone();
        }
        c
    };
    let mut rows: Vec<(String, Value, Option<&str>)> = vec![
        (
            "CTL-062.1".into(),
            vn(json!({"inst":"binance-usdm:BTCUSDT","kind":"Delisting","at_ns":"1790100000000000000"})),
            None,
        ),
        (
            "CTL-062.2".into(),
            vn(json!({"inst":"bitget-spot:BTCUSDT","kind":"Delisting","at_ns":"1790100000000000000"})),
            Some("UnknownName"),
        ),
    ];
    for s in ["3600", "7200", "14400", "28800"] {
        rows.push((
            format!("CTL-062.3 {s}"),
            vn(json!({"kind":"FundingInterval","new_s":s})),
            None,
        ));
    }
    for (row, s) in [
        ("CTL-062.4", "3599"),
        ("CTL-062.5", "3601"),
        ("CTL-062.6", "1800"),
        ("CTL-062.7", "86400"),
    ] {
        rows.push((
            row.into(),
            vn(json!({"kind":"FundingInterval","new_s":s})),
            Some("BadValue"),
        ));
    }
    let m = |from: &str, to: &str| vn(json!({"kind":"Maintenance","from_ns":from,"to_ns":to}));
    rows.push((
        "CTL-062.8".into(),
        m("1790100000000000000", "1790100000000000001"),
        None,
    ));
    rows.push((
        "CTL-062.9".into(),
        m("1790100000000000000", "1790100000000000000"),
        Some("BadValue"),
    ));
    rows.push((
        "CTL-062.10".into(),
        m("1790100000000000001", "1790100000000000000"),
        Some("BadValue"),
    ));
    for (row, cmd, want) in rows {
        let mut g = G0::new();
        let (r, recs) = send(&mut g, "cli:tom", Some("*"), cmd);
        match want {
            None => assert!(written(&recs), "{row}: {r:?}"),
            Some(d) => assert_eq!(invalid(&r), d, "{row}"),
        }
    }
}

#[test]
fn ctl_063_adopt_exchange() {
    let ad = |extra: Value| {
        let mut c = json!({"type":"AdoptExchange","venue":"bitget-spot","report_id":"7"});
        for (k, v) in extra.as_object().unwrap() {
            c[k] = v.clone();
        }
        c
    };
    let mut g = G0::new();
    let (_, recs) = send(
        &mut g,
        "cli:tom",
        Some("acct-a"),
        ad(json!({"item":"Balance","asset":"BTC","group":"16777317"})),
    );
    assert!(written(&recs), "CTL-063.1");
    let (_, recs) = send(
        &mut g,
        "cli:tom",
        Some("acct-a"),
        ad(json!({"item":"Balance","asset":"BTC"})),
    );
    match recs.iter().find(|r| r.is_cmd()).unwrap().cmd().1 {
        ControlCmd::AdoptExchange { group, .. } => assert_eq!(group.0, 0, "CTL-063.2"),
        c => panic!("{c:?}"),
    }
    let (r, _) = send(
        &mut g,
        "cli:tom",
        Some("acct-a"),
        ad(json!({"item":"Balance","asset":"BTC","group":"33554533"})),
    );
    assert_eq!(invalid(&r), "BadGroup", "CTL-063.3");
    let (r, _) = send(
        &mut g,
        "cli:tom",
        Some("acct-a"),
        ad(json!({"item":"Balance","asset":"XYZ"})),
    );
    assert_eq!(invalid(&r), "UnknownName", "CTL-063.4");
    let (_, recs) = send(
        &mut g,
        "cli:tom",
        Some("acct-a"),
        ad(json!({"venue":"binance-usdm","item":"Position","inst":"binance-usdm:BTCUSDT"})),
    );
    assert!(written(&recs), "CTL-063.5");
}

#[test]
fn ctl_064_smoke_order() {
    let so = |extra: Value| {
        let mut c =
            json!({"type":"SmokeOrder","venue":"bitget-spot","inst":"bitget-spot:BTCUSDT","side":"Buy"});
        for (k, v) in extra.as_object().unwrap() {
            c[k] = v.clone();
        }
        c
    };
    let mut g = G0::new();
    let (_, recs) = send(&mut g, "cli:alice", Some("acct-a"), so(json!({})));
    match recs.iter().find(|r| r.is_cmd()).unwrap().cmd().1 {
        ControlCmd::SmokeOrder { ttl_ms, .. } => assert_eq!(ttl_ms, 10_000, "CTL-064.1"),
        c => panic!("{c:?}"),
    }
    let rows: Vec<(&str, &str, Value, Option<&str>)> = vec![
        ("CTL-064.2", "sim-a", so(json!({})), Some("NotLive")),
        (
            "CTL-064.3",
            "acct-a",
            so(json!({"venue":"binance-spot","inst":"binance-spot:BTCUSDT"})),
            Some("UnknownName"),
        ),
        (
            "CTL-064.4",
            "acct-a",
            so(json!({"inst":"binance-usdm:BTCUSDT"})),
            Some("UnknownName"),
        ),
        ("CTL-064.5", "acct-a", so(json!({"ttl_ms":"0"})), Some("BadValue")),
        (
            "CTL-064.6",
            "acct-a",
            so(json!({"ttl_ms":"60001"})),
            Some("BadValue"),
        ),
        ("CTL-064.7", "acct-a", so(json!({"ttl_ms":"60000"})), None),
    ];
    for (row, account, cmd, want) in rows {
        let (r, recs) = send(&mut g, "cli:alice", Some(account), cmd);
        match want {
            None => assert!(written(&recs), "{row}"),
            Some(d) => assert_eq!(invalid(&r), d, "{row}"),
        }
    }
}

#[test]
fn ctl_065_write_off_residual() {
    let mut g = G0::new();
    for (row, grp, want) in [
        ("CTL-065.1", "16777317", None),
        ("CTL-065.2", "33554533", Some("BadGroup")),
        ("CTL-065.3", "*", Some("BadGroup")),
        ("CTL-065.4", "0", Some("BadGroup")),
    ] {
        let (r, recs) = send(
            &mut g,
            "cli:alice",
            Some("acct-a"),
            json!({"type":"WriteOffResidual","group":grp}),
        );
        match want {
            None => assert!(written(&recs), "{row}"),
            Some(d) => assert_eq!(invalid(&r), d, "{row}"),
        }
    }
}

fn refdata_v(version: u64) -> Vec<u8> {
    let mut s = p0::p0_spec();
    s.version = version;
    s.build()
}

#[test]
fn ctl_066_refdata_assembly() {
    let rr = |b: &[u8]| json!({"type":"ReplaceRefdata","blob_b64":b64(b),"version":"2"});
    for (row, item, name) in [
        ("CTL-066.1", 1u16, "AccountNamePrefix"),
        ("CTL-066.2", 2, "MissingCredential"),
        ("CTL-066.3", 3, "SimConfig"),
        ("CTL-066.4", 4, "ThreadPlan"),
        ("CTL-066.5", 5, "DustBelowMinNotional"),
    ] {
        ASSEMBLY_FAIL.with(|c| c.set(item));
        let mut g = G0::new();
        g.paused = true;
        let (r, recs) = send(&mut g, "cli:alice", Some("acct-a"), rr(&refdata_v(2)));
        assert_eq!((status(&r).as_str(), code(&r)), ("ok", "Rejected"), "{row}");
        assert_eq!(
            (detail_name(&r).as_str(), detail_arg(&r).as_str()),
            ("RefdataAssembly", name),
            "{row}"
        );
        // CTL-066.6
        assert!(recs.iter().all(|r| r.is_note()), "CTL-066.6 {row}");
        let a = only_audit(&recs);
        assert_eq!((a.result_code, a.cmd_kind), (27, 0x0009), "CTL-066.6 {row}");
        assert!(g.mb_take.try_take().is_none(), "CTL-066.6 {row}");
        assert!(g.unanswered.is_empty(), "CTL-066.6 {row}");
    }
    ASSEMBLY_FAIL.with(|c| c.set(0));
    for (row, version, ok) in [("CTL-066.7", 2u64, true), ("CTL-066.8", 1, false)] {
        let mut g = G0::new();
        g.paused = true;
        let (_, recs) = send(&mut g, "cli:alice", Some("acct-a"), rr(&refdata_v(version)));
        let kinds: Vec<EventKind> = recs.iter().map(|r| r.kind).collect();
        let at = kinds.iter().position(|k| *k == EventKind::CONTROL_CMD).unwrap();
        assert!(
            at > 0 && kinds[..at].iter().all(|k| *k == EventKind::CONFIG_BLOB),
            "{row}"
        );
        match g.mb_take.try_take() {
            Some(ParsedConfig::Refdata { result, .. }) => assert_eq!(result.is_ok(), ok, "{row}: {result:?}"),
            o => panic!("{row}: {o:?}"),
        }
    }
}

#[test]
fn ctl_067_strategy_cmd() {
    use hft_strategy_sdk::params::encode_cmd;
    let p1 = |qty: i64| {
        encode_cmd(&set::CMDS[0], &[qty]).unwrap_or_else(|_| {
            let mut b = vec![1u8, 1, 1, 0, 0, 0, 0, 0];
            b.extend_from_slice(&qty.to_le_bytes());
            b
        })
    };
    let p2 = encode_cmd(&set::CMDS[1], &[]).unwrap();
    let mut op9 = p1(5);
    op9[1] = 9;
    let sc = |group: &str, ro: bool, payload: &[u8]| json!({"type":"StrategyCmd","strategy":"s1-main","group":group,"reduce_only":ro,"payload_b64":b64(payload)});
    let mut g = G0::new();
    g.paused = true;
    let (_, recs) = send(&mut g, "cli:tom", Some("acct-a"), sc("16777317", false, &p1(5)));
    assert!(written(&recs), "CTL-067.1");
    assert!(g.mb_take.try_take().is_none(), "CTL-067.1");
    let (_, recs) = send(&mut g, "cli:tom", Some("acct-a"), sc("none", false, &p1(5)));
    match recs.iter().find(|r| r.is_cmd()).unwrap().cmd().1 {
        ControlCmd::StrategyCmd { group, .. } => assert_eq!(group.0, 0, "CTL-067.2"),
        c => panic!("{c:?}"),
    }
    let (r, _) = send(&mut g, "cli:tom", Some("acct-b"), sc("16777317", false, &p1(5)));
    assert_eq!(invalid(&r), "AccountBound", "CTL-067.3");
    let (r, _) = send(&mut g, "cli:tom", Some("acct-a"), sc("*", false, &p1(5)));
    assert_eq!(invalid(&r), "BadGroup", "CTL-067.4");
    let (r, _) = send(&mut g, "cli:tom", Some("acct-a"), sc("16777317", false, &op9));
    assert_eq!(
        (invalid(&r).as_str(), detail_arg(&r).as_str()),
        ("ParamInvalid", "Strategy"),
        "CTL-067.5"
    );
    let (r, _) = send(&mut g, "cli:tom", Some("acct-a"), sc("16777317", false, &p1(101)));
    assert_eq!(
        (invalid(&r).as_str(), detail_arg(&r).as_str()),
        ("ParamInvalid", "Strategy:0"),
        "CTL-067.6"
    );
    let (r, _) = send(&mut g, "cli:tom", Some("acct-a"), sc("16777317", false, &p2));
    assert_eq!(
        (invalid(&r).as_str(), detail_arg(&r).as_str()),
        ("ParamInvalid", "Strategy"),
        "CTL-067.7"
    );
    let (_, recs) = send(&mut g, "cli:tom", Some("acct-a"), sc("16777317", true, &p2));
    assert!(written(&recs), "CTL-067.8");
    let (r, _) = send(
        &mut g,
        "cli:tom",
        Some("acct-a"),
        sc("16777317", false, &[1u8; 257]),
    );
    assert_eq!(invalid(&r), "TooLong", "CTL-067.9");
}

#[test]
fn ctl_068_inject_warmup() {
    let iw = |strategy: &str, n: usize| json!({"type":"InjectWarmup","strategy":strategy,"local_kind":"3","chunk_no":"0","last":false,"payload_b64":b64(&vec![1u8; n])});
    let mut g = G0::new();
    let (_, recs) = send(&mut g, "pipeline", None, iw("s2-main", 4096));
    let (h, _) = recs.iter().find(|r| r.is_cmd()).unwrap().cmd();
    assert_eq!(h.account.0, 0, "CTL-068.1");
    let (r, _) = send(&mut g, "pipeline", Some("acct-b"), iw("s2-main", 4096));
    assert_eq!(invalid(&r), "AccountNotAllowed", "CTL-068.2");
    let (r, _) = send(&mut g, "pipeline", None, iw("nope", 1));
    assert_eq!(invalid(&r), "UnknownName", "CTL-068.3");
    let (r, _) = send(&mut g, "pipeline", None, iw("s2-main", 4097));
    assert_eq!(invalid(&r), "TooLong", "CTL-068.4");
}

#[test]
fn ctl_307_event_err_detail() {
    use hft_ctl_proto::codes::detail;
    let rows: [(&str, EventErr, u16); 9] = [
        (
            "CTL-307.1",
            EventErr::Params(ParamsErr::DupGroup),
            detail::GROUP_ID_DUP,
        ),
        (
            "CTL-307.2",
            EventErr::Params(ParamsErr::Truncated),
            detail::BAD_GROUP,
        ),
        (
            "CTL-307.3",
            EventErr::Params(ParamsErr::BadLen),
            detail::BAD_GROUP,
        ),
        (
            "CTL-307.4",
            EventErr::Params(ParamsErr::BadGroup),
            detail::BAD_GROUP,
        ),
        (
            "CTL-307.5",
            EventErr::Params(ParamsErr::BadLeg),
            detail::BAD_GROUP,
        ),
        (
            "CTL-307.6",
            EventErr::Params(ParamsErr::Padding),
            detail::BAD_GROUP,
        ),
        (
            "CTL-307.7",
            EventErr::Params(ParamsErr::BadHdr),
            detail::BAD_GROUP,
        ),
        ("CTL-307.8", EventErr::UnknownKind, detail::UNKNOWN_DISCRIMINANT),
        ("CTL-307.9", EventErr::InvalidEnum, detail::UNKNOWN_DISCRIMINANT),
    ];
    for (row, e, d) in rows {
        assert_eq!(detail_of_event_err(e), (ResultCode::InvalidArgs, d), "{row}");
    }
}
