//! CTL-030–CTL-040.
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path

use crate::common::*;
use hft_ctl::InternalCmd;
use hft_event::{ControlCmd, EventKind};
use hft_types::Tsc;
use serde_json::{Value, json};

fn kill(level: &str, extra: Value) -> Value {
    let mut c = json!({"type":"Kill","level":level});
    if level == "Venue" {
        c["venue"] = json!("bitget-spot");
    }
    for (k, v) in extra.as_object().unwrap() {
        c[k] = v.clone();
    }
    c
}

fn set_mode(mode: &str) -> Value {
    json!({"type":"SetMode","strategy":"s1-main","group":"16777317","mode":mode})
}

fn note(kind: &str) -> Value {
    json!({"type":"Annotate","kind":kind,"text":"x"})
}

fn hedge() -> Value {
    json!({"type":"HedgeFix","group":"16777317"})
}

/// Sends and returns (response, records written by this request).
fn run(g: &mut G0, v: &Value) -> (hft_ctl_proto::CmdResponse, Vec<RingRec>) {
    let before = g.seen.len();
    let r = g.send_and_wait(v);
    g.take_ring();
    (r, g.seen[before..].to_vec())
}

fn send(
    g: &mut G0,
    issuer: &str,
    account: Option<&str>,
    cmd: Value,
) -> (hft_ctl_proto::CmdResponse, Vec<RingRec>) {
    let v = g.req(issuer, account, cmd);
    run(g, &v)
}

#[test]
fn ctl_030_unknown_uid() {
    let mut g = G0::new();
    let v = g.req("cli:alice", Some("acct-a"), hedge());
    let r = g.send_raw(9999, &serde_json::to_vec(&v).unwrap()).unwrap();
    let j: Value = serde_json::from_slice(&r.to_json()).unwrap();
    assert_eq!(j["cmd_id"], "00000000-0000-0000-0000-000000000000");
    assert_eq!(j["status"], "unauthorized");
    assert_eq!(j["code"], "Unauthorized");
    let recs = g.take_ring();
    assert!(G0::<CtlTestSet>::cmds(&recs).is_empty());
    let a = only_audit(&recs);
    assert_eq!(
        (a.cmd_id, a.cmd_kind, a.result_code, a.role, a.issuer),
        ([0; 16], 0, 10, 0, [0; 8])
    );
    assert_eq!(a.peer_uid, 9999);
}

#[test]
fn ctl_031_issuer_forms_per_uid() {
    let rows: [(&str, u32, &str, bool); 15] = [
        ("CTL-031.1", 1100, "tg:1002", true),
        ("CTL-031.2", 1100, "ops_bot", true),
        ("CTL-031.3", 1100, "pipeline", false),
        ("CTL-031.4", 1100, "cli:alice", false),
        ("CTL-031.5", 1100, "tg:9999", false),
        ("CTL-031.6", 1100, "tg:abc", false),
        ("CTL-031.7", 1101, "pipeline", true),
        ("CTL-031.8", 1101, "tg:1002", false),
        ("CTL-031.9", 1102, "reconciler", true),
        ("CTL-031.10", 1102, "pipeline", false),
        ("CTL-031.11", 1200, "cli:alice", true),
        ("CTL-031.12", 1200, "cli:tom", false),
        ("CTL-031.13", 1201, "cli:alice", false),
        ("CTL-031.14", 1200, "cli:systemd", false),
        ("CTL-031.15", 1100, "cli:systemd", false),
    ];
    for (row, uid, issuer, pass) in rows {
        let mut g = G0::new();
        let v = g.req(issuer, Some("*"), note("OpsNote"));
        let r = g.send_raw(uid, &serde_json::to_vec(&v).unwrap());
        let unauthorized = r.as_ref().is_some_and(|r| status(r) == "unauthorized");
        assert_eq!(!unauthorized, pass, "{row}: {r:?}");
    }
}

#[test]
fn ctl_032_effective_permission() {
    let risk = json!({"type":"SetRisk","blob_b64":b64(&risk_blob(0))});
    let smoke = json!({"type":"SmokeOrder","venue":"bitget-spot","inst":"bitget-spot:BTCUSDT","side":"Buy"});
    let sp = json!({"type":"SetParams","strategy":"s1-main","blob_b64":b64(&Spb::s1(42).bytes())});
    let rows: Vec<(&str, &str, &str, Value, bool)> = vec![
        ("CTL-032.1", "tg:1003", "private", risk.clone(), false),
        ("CTL-032.2", "cli:alice", "", risk, true),
        ("CTL-032.3", "tg:1002", "group", set_mode("Live"), false),
        ("CTL-032.4", "tg:1002", "private", set_mode("Live"), true),
        ("CTL-032.5", "tg:1003", "private", sp, false),
        ("CTL-032.6", "cli:tom", "", smoke.clone(), false),
        ("CTL-032.7", "tg:1003", "group", smoke.clone(), false),
        ("CTL-032.8", "tg:1003", "private", smoke, true),
    ];
    for (row, issuer, chat, cmd, written) in rows {
        let mut g = G0::new();
        let mut v = g.req(issuer, Some("acct-a"), cmd);
        if !chat.is_empty() {
            v["chat_type"] = json!(chat);
        }
        let (r, recs) = run(&mut g, &v);
        if written {
            assert_eq!(G0::<CtlTestSet>::cmds(&recs).len(), 1, "{row}: {r:?}");
        } else {
            assert_eq!(code(&r), "Forbidden", "{row}");
            assert!(G0::<CtlTestSet>::cmds(&recs).is_empty(), "{row}");
        }
    }
}

#[test]
fn ctl_033_gateway_hdr_fields() {
    let sp = json!({"type":"SetParams","strategy":"s1-main","blob_b64":b64(&Spb::s1(42).bytes())});
    let rows: Vec<(&str, &str, Option<&str>, Value, u8, u32)> = vec![
        ("CTL-033.1", "tg:1004", None, kill("Global", json!({})), 2, 1100),
        ("CTL-033.2", "tg:1004", Some("*"), note("OpsNote"), 2, 1100),
        (
            "CTL-033.3",
            "tg:1001",
            Some("acct-a"),
            kill("Account", json!({})),
            1,
            1100,
        ),
        ("CTL-033.4", "cli:alice", Some("acct-a"), hedge(), 4, 1200),
        ("CTL-033.5", "ops_bot", None, kill("Global", json!({})), 0, 1100),
        ("CTL-033.6", "pipeline", Some("acct-a"), sp, 0, 1101),
    ];
    for (row, issuer, account, cmd, role, uid) in rows {
        let mut g = G0::new();
        let v = g.req(issuer, account, cmd);
        let (_, recs) = run(&mut g, &v);
        let cmds = G0::<CtlTestSet>::cmds(&recs);
        assert_eq!(cmds.len(), 1, "{row}");
        let (h, _) = cmds[0].cmd();
        assert_eq!((h.role, h.peer_uid), (role, uid), "{row}");
        assert_eq!(h.expires_tsc.0, 30_000_000_000, "{row}");
        // CTL-033.7: cmd_hash = normalize's, issuer = the request's.
        let req = hft_ctl_proto::parse_request(&serde_json::to_vec(&v).unwrap()).unwrap();
        let n = hft_ctl_proto::normalize(&req, g.core.refdata()).unwrap();
        assert_eq!(h.cmd_hash, n.cmd_hash, "CTL-033.7 {row}");
        assert_eq!(&h.issuer[..issuer.len()], issuer.as_bytes(), "CTL-033.7 {row}");
    }
}

#[test]
fn ctl_034_viewer_limits() {
    let mut g = G0::new();
    let mut v = g.req("tg:1001", None, kill("Global", json!({})));
    v["preview"] = json!(true);
    let (_, recs) = run(&mut g, &v);
    let (h, _) = G0::<CtlTestSet>::cmds(&recs)[0].cmd();
    assert_eq!((h.role, h.hflags), (1, 1), "CTL-034.1");
    let mut v = g.req("tg:1001", Some("acct-a"), kill("Account", json!({})));
    v["confirmed"] = json!(true);
    let (_, recs) = run(&mut g, &v);
    let (h, _) = G0::<CtlTestSet>::cmds(&recs)[0].cmd();
    assert_eq!(h.expect_state_version, 0, "CTL-034.2");
    for k in ["OpsNote", "LocalWrite"] {
        let (_, recs) = send(&mut g, "tg:1001", Some("*"), note(k));
        assert_eq!(G0::<CtlTestSet>::cmds(&recs).len(), 1, "CTL-034.3 {k}");
    }
    let (r, _) = send(
        &mut g,
        "tg:1001",
        None,
        json!({"type":"GetSchema","strategy_kind":"probe-taker"}),
    );
    assert_eq!(code(&r), "Applied", "CTL-034.4");
    assert!(r.body.get().contains("probe-taker"), "CTL-034.4");
    let forbidden: Vec<(&str, Option<&str>, Value)> = vec![
        ("CTL-034.5", Some("acct-a"), kill("Account", json!({"hard":true}))),
        (
            "CTL-034.6",
            Some("acct-a"),
            kill("Account", json!({"flatten":true})),
        ),
        ("CTL-034.7", None, kill("Global", json!({"cancel_resting":false}))),
        ("CTL-034.8", None, kill("Venue", json!({}))),
        (
            "CTL-034.9",
            Some("acct-a"),
            json!({"type":"Resume","source":"Kill"}),
        ),
        ("CTL-034.10", Some("acct-a"), set_mode("CloseOnly")),
        ("CTL-034.11", Some("acct-a"), json!({"type":"CancelAll"})),
        (
            "CTL-034.12",
            Some("acct-a"),
            json!({"type":"StrategyCmd","strategy":"s1-main","group":"none","reduce_only":true,"payload_b64":"Ag=="}),
        ),
        ("CTL-034.13", Some("*"), note("Incident")),
    ];
    for (row, account, cmd) in forbidden {
        let (r, recs) = send(&mut g, "tg:1001", account, cmd);
        assert_eq!(code(&r), "Forbidden", "{row}");
        assert!(G0::<CtlTestSet>::cmds(&recs).is_empty(), "CTL-034.14 {row}");
        let a = only_audit(&recs);
        assert_eq!((a.result_code, a.role), (11, 1), "CTL-034.14 {row}");
    }
}

#[test]
fn ctl_035_ops_bot_self() {
    let mut g = G0::new();
    let (_, recs) = send(&mut g, "ops_bot", Some("acct-a"), kill("Account", json!({})));
    let (h, _) = G0::<CtlTestSet>::cmds(&recs)[0].cmd();
    assert_eq!((h.role, h.confirmed), (0, 0), "CTL-035.1");
    let (_, recs) = send(&mut g, "ops_bot", None, kill("Global", json!({})));
    assert_eq!(G0::<CtlTestSet>::cmds(&recs).len(), 1, "CTL-035.2");
    let (_, recs) = send(&mut g, "ops_bot", Some("*"), note("OpsNote"));
    assert_eq!(G0::<CtlTestSet>::cmds(&recs).len(), 1, "CTL-035.3");
    let rows: Vec<(&str, Option<&str>, Value)> = vec![
        ("CTL-035.4", Some("acct-a"), kill("Account", json!({"hard":true}))),
        (
            "CTL-035.5",
            Some("acct-a"),
            kill("Account", json!({"flatten":true})),
        ),
        ("CTL-035.6", Some("*"), note("LocalWrite")),
        ("CTL-035.7", Some("acct-a"), set_mode("CloseOnly")),
        (
            "CTL-035.8",
            None,
            json!({"type":"GetSchema","strategy_kind":"probe-taker"}),
        ),
    ];
    for (row, account, cmd) in rows {
        let (r, _) = send(&mut g, "ops_bot", account, cmd);
        assert_eq!(code(&r), "Forbidden", "{row}");
    }
}

#[test]
fn ctl_036_reconciler() {
    let mut g = G0::new();
    let items = json!([{"venue":"binance-usdm","account":"acct-a","item":"Position","inst":"binance-usdm:BTCUSDT"},{"item":"Summary"}]);
    let (r, recs) = send(
        &mut g,
        "reconciler",
        None,
        json!({"type":"ReconcileReport","report_id":"7","items":items}),
    );
    assert_eq!(code(&r), "Applied", "CTL-036.1");
    assert_eq!(
        recs.iter()
            .filter(|r| r.kind == EventKind::RECONCILE_REPORT)
            .count(),
        2,
        "CTL-036.1"
    );
    for (row, account, cmd) in [
        ("CTL-036.2", None, kill("Global", json!({}))),
        ("CTL-036.3", Some("acct-a"), set_mode("CloseOnly")),
        ("CTL-036.4", Some("*"), note("OpsNote")),
    ] {
        let (r, _) = send(&mut g, "reconciler", account, cmd);
        assert_eq!(code(&r), "Forbidden", "{row}");
    }
}

#[test]
fn ctl_037_internal_channel() {
    let mut g = G0::new();
    assert!(
        g.core
            .on_internal(InternalCmd::Drain { cancel_resting: true }, Tsc(0))
    );
    let recs = g.take_ring();
    let cmds = G0::<CtlTestSet>::cmds(&recs);
    let (h, c) = cmds[0].cmd();
    assert_eq!(c, ControlCmd::Drain { cancel_resting: true }, "CTL-037.1");
    assert_eq!(&h.issuer[..11], b"cli:systemd", "CTL-037.1");
    assert_eq!((h.role, h.peer_uid, h.account.0), (0, 0, 0), "CTL-037.1");
    assert_eq!(only_audit(&recs).chat_type, 0, "CTL-037.1");
    g.core.on_internal(
        InternalCmd::Drain {
            cancel_resting: false,
        },
        Tsc(1),
    );
    let recs = g.take_ring();
    assert_eq!(
        G0::<CtlTestSet>::cmds(&recs)[0].cmd().1,
        ControlCmd::Drain {
            cancel_resting: false
        },
        "CTL-037.2"
    );
    g.core.on_internal(InternalCmd::CancelAll, Tsc(2));
    let recs = g.take_ring();
    let (h, c) = G0::<CtlTestSet>::cmds(&recs)[0].cmd();
    match c {
        ControlCmd::CancelAll { scope } => {
            assert_eq!(
                (
                    scope.strategy.0,
                    scope.venue.0,
                    scope.account.0,
                    scope.group.0,
                    scope.inst.0
                ),
                (0, 0, 0, u32::MAX, 0),
                "CTL-037.3"
            );
        }
        c => panic!("{c:?}"),
    }
    assert_eq!(&h.issuer[..11], b"cli:systemd", "CTL-037.3");
    let v = g.req("cli:systemd", None, json!({"type":"Drain"}));
    let r = g.send_raw(1200, &serde_json::to_vec(&v).unwrap()).unwrap();
    assert_eq!(status(&r), "unauthorized", "CTL-037.4");
    assert!(G0::<CtlTestSet>::cmds(&g.take_ring()).is_empty(), "CTL-037.4");
}

#[test]
fn ctl_038_group_chat() {
    let rows: Vec<(&str, &str, Option<&str>, Value, bool)> = vec![
        ("CTL-038.1", "tg:1002", None, kill("Global", json!({})), true),
        ("CTL-038.2", "tg:1003", None, kill("Venue", json!({})), true),
        (
            "CTL-038.3",
            "tg:1002",
            Some("acct-a"),
            set_mode("CloseOnly"),
            true,
        ),
        (
            "CTL-038.4",
            "tg:1001",
            Some("acct-a"),
            kill("Account", json!({})),
            true,
        ),
        (
            "CTL-038.5",
            "tg:1001",
            None,
            json!({"type":"GetSchema","strategy_kind":"probe-taker"}),
            true,
        ),
        (
            "CTL-038.6",
            "tg:1002",
            None,
            kill("Global", json!({"hard":true})),
            false,
        ),
        (
            "CTL-038.7",
            "tg:1002",
            Some("acct-a"),
            kill("Account", json!({"flatten":true})),
            false,
        ),
        ("CTL-038.8", "tg:1002", Some("acct-a"), set_mode("Shadow"), false),
        (
            "CTL-038.9",
            "tg:1002",
            Some("acct-a"),
            json!({"type":"CancelAll"}),
            false,
        ),
        ("CTL-038.10", "tg:1002", None, json!({"type":"Drain"}), false),
        ("CTL-038.11", "tg:1001", Some("*"), note("OpsNote"), false),
    ];
    for (row, issuer, account, cmd, ok) in rows {
        let mut g = G0::new();
        let mut v = g.req(issuer, account, cmd);
        v["chat_type"] = json!("group");
        if row == "CTL-038.4" {
            v["preview"] = json!(true);
        }
        let (r, _) = run(&mut g, &v);
        if ok {
            assert_eq!(code(&r), "Applied", "{row}");
        } else {
            assert_eq!(code(&r), "Forbidden", "{row}");
        }
    }
}

#[test]
fn ctl_039_confirm_fields_after_role() {
    let rows: Vec<(&str, &str, Option<&str>, Value, Option<u64>, &str)> = vec![
        (
            "CTL-039.1",
            "tg:1002",
            Some("acct-a"),
            set_mode("Live"),
            None,
            "invalid_request",
        ),
        (
            "CTL-039.2",
            "tg:1002",
            Some("acct-a"),
            set_mode("Live"),
            Some(0),
            "invalid_request",
        ),
        (
            "CTL-039.3",
            "tg:1002",
            Some("acct-a"),
            set_mode("Live"),
            Some(21),
            "written",
        ),
        (
            "CTL-039.4",
            "tg:1001",
            Some("acct-a"),
            kill("Account", json!({})),
            None,
            "written",
        ),
        (
            "CTL-039.5",
            "tg:1004",
            Some("acct-a"),
            kill("Account", json!({})),
            None,
            "invalid_request",
        ),
        (
            "CTL-039.6",
            "tg:1002",
            None,
            kill("Global", json!({})),
            None,
            "invalid_request",
        ),
        (
            "CTL-039.7",
            "tg:1001",
            Some("acct-a"),
            set_mode("Live"),
            None,
            "Forbidden",
        ),
    ];
    for (row, issuer, account, cmd, esv, want) in rows {
        let mut g = G0::new();
        let mut v = g.req(issuer, account, cmd);
        v["confirmed"] = json!(true);
        if let Some(e) = esv {
            v["expect_state_version"] = json!(e);
        }
        let (r, recs) = run(&mut g, &v);
        match want {
            "written" => {
                let (h, _) = G0::<CtlTestSet>::cmds(&recs)[0].cmd();
                assert_eq!(h.expect_state_version, esv.unwrap_or(0), "{row}");
            }
            "Forbidden" => assert_eq!(code(&r), "Forbidden", "{row}"),
            s => assert_eq!(status(&r), s, "{row}"),
        }
    }
}

#[test]
fn ctl_040_admin_from_issuer_table_only() {
    let dir = std::env::temp_dir().join(format!("ctl-040-{}", std::process::id()));
    std::fs::create_dir_all(dir.join(".github")).unwrap();
    std::fs::write(dir.join(".github/hft-admins.txt"), "alice\n").unwrap();
    let mut cfg = config(dir.join("ctl.sock"));
    cfg.principals = hft_ctl::PrincipalTable::from_parts(
        vec![(
            1200,
            hft_ctl::UidKind::Cli {
                login: "alice".into(),
            },
        )],
        vec![("cli:alice".into(), hft_ctl_proto::RoleSet::TRADER)],
    );
    let mut g = G0::with_cfg(p0::p0(), 262_144, g0_active(), cfg);
    let (r, recs) = send(
        &mut g,
        "cli:alice",
        Some("acct-a"),
        json!({"type":"SmokeOrder","venue":"bitget-spot","inst":"bitget-spot:BTCUSDT","side":"Buy"}),
    );
    assert_eq!(code(&r), "Forbidden");
    assert!(G0::<CtlTestSet>::cmds(&recs).is_empty());
    // The gateway never reads repository files: the admins file is untouched by the core,
    // whose only inputs are the issuer table it was built with.
    assert_eq!(
        std::fs::read_to_string(dir.join(".github/hft-admins.txt")).unwrap(),
        "alice\n"
    );
}
