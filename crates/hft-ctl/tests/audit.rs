//! CTL-101, CTL-102.
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path

use crate::common::*;
use hft_ctl::InternalCmd;
use hft_types::Tsc;
use serde_json::{Value, json};

fn audit_after(
    g: &mut G0,
    f: impl FnOnce(&mut G0) -> Option<hft_ctl_proto::CmdResponse>,
) -> (Option<hft_ctl_proto::CmdResponse>, AuditV, Vec<RingRec>) {
    let n = g.seen.len();
    let r = f(g);
    g.step();
    let recs = g.seen[n..].to_vec();
    (r, only_audit(&recs), recs)
}

#[test]
fn ctl_101_audit_note_fields() {
    // .1
    let mut g = G0::new();
    let mut v = g.req(
        "tg:1002",
        Some("acct-a"),
        json!({"type":"SetMode","strategy":"s1-main","group":"16777317","mode":"Live"}),
    );
    v["view_account"] = json!("acct-b");
    let (_, a, recs) = audit_after(&mut g, |g| g.send_v(&v));
    let resp = g.delivered.last().unwrap().1.clone();
    assert_eq!((a.cmd_kind, a.result_code, a.role), (0x0001, 0, 2), "CTL-101.1");
    assert_eq!(
        (a.view_account, a.chat_type, a.target_account),
        (2, 1, 1),
        "CTL-101.1"
    );
    assert_eq!(&a.issuer, b"tg:1002\0", "CTL-101.1");
    let id = hft_ctl_proto::Uuid::parse_str(v["cmd_id"].as_str().unwrap()).unwrap();
    assert_eq!(a.cmd_id, *id.as_bytes(), "CTL-101.1");
    assert_eq!(&a.cmd_hash[..], &resp.cmd_hash.unwrap().0[..24], "CTL-101.1");
    assert_eq!(a.peer_uid, 1100, "CTL-101.1");
    let (h, _) = G0::<CtlTestSet>::cmds(&recs)[0].cmd();
    assert_eq!(h.peer_uid, 1100, "CTL-101.1");
    // .2
    let mut v = g.req(
        "tg:1001",
        Some("acct-a"),
        json!({"type":"SetMode","strategy":"s1-main","group":"16777317","mode":"CloseOnly"}),
    );
    v["chat_type"] = json!("group");
    let (_, a, _) = audit_after(&mut g, |g| g.send_v(&v));
    assert_eq!(
        (a.cmd_kind, a.result_code, a.role, a.chat_type),
        (0x0001, 11, 1, 2),
        "CTL-101.2"
    );
    // .3
    let (_, a, _) = audit_after(&mut g, |g| g.send_raw(9999, b"{}"));
    assert_eq!(
        (
            a.cmd_id,
            a.cmd_kind,
            a.result_code,
            a.role,
            a.issuer,
            a.chat_type,
            a.target_account,
            a.cmd_hash,
            a.peer_uid
        ),
        ([0; 16], 0, 10, 0, [0; 8], 0, 0, [0; 24], 9999),
        "CTL-101.3"
    );
    // .4
    let v = g.req(
        "cli:vera",
        None,
        json!({"type":"GetSchema","strategy_kind":"probe-taker"}),
    );
    let (_, a, _) = audit_after(&mut g, |g| g.send_v(&v));
    assert_eq!(
        (a.cmd_kind, a.result_code, a.role, a.chat_type),
        (0, 0, 1, 3),
        "CTL-101.4"
    );
    // .5
    let v = g.req(
        "pipeline",
        Some("acct-a"),
        json!({"type":"SetParams","strategy":"s1-main","blob_b64":b64(&Spb::s1(42).bytes())}),
    );
    let (_, a, _) = audit_after(&mut g, |g| g.send_v(&v));
    assert_eq!((a.cmd_kind, a.role, a.chat_type), (0x0002, 0, 4), "CTL-101.5");
    // .6
    let v = g.req("ops_bot", None, json!({"type":"Kill","level":"Global"}));
    let (_, a, _) = audit_after(&mut g, |g| g.send_v(&v));
    assert_eq!((a.role, a.chat_type, a.target_account), (0, 0, 0), "CTL-101.6");
    // .7
    let (_, a, _) = audit_after(&mut g, |g| g.send_raw(1201, b"not json"));
    assert_eq!(
        (a.cmd_id, a.cmd_kind, a.result_code),
        ([0; 16], 0, 20),
        "CTL-101.7"
    );
    // .8 Mailbox occupied → busy.
    let mut g = G0::new();
    let sp = |g: &G0, v: u64| {
        let mut s = Spb::s1(v);
        s.strategy = 1;
        g.req(
            "cli:tom",
            Some("acct-a"),
            json!({"type":"SetParams","strategy":"s1-main","blob_b64":b64(&s.bytes())}),
        )
    };
    let first = sp(&g, 42);
    assert!(g.send_v(&first).is_none());
    g.take_ring();
    let mut s2 = Spb::s1(8);
    s2.strategy = 2;
    s2.account = 2;
    s2.kind_id = 0xFF02;
    s2.digest = set::digest(0xFF02);
    s2.groups = vec![group(G2, &[leg(2, 2), leg(3, 10)])];
    let v = g.req(
        "cli:tom",
        Some("acct-b"),
        json!({"type":"SetParams","strategy":"s2-main","blob_b64":b64(&s2.bytes())}),
    );
    let (r, a, _) = audit_after(&mut g, |g| g.send_v(&v));
    assert_eq!(status(r.as_ref().unwrap()), "busy", "CTL-101.8");
    assert_eq!(a.result_code, 31, "CTL-101.8");
    // .9
    let mut g = G0::new();
    let mut v = g.req(
        "tg:1002",
        Some("acct-a"),
        json!({"type":"HedgeFix","group":"16777317"}),
    );
    v["view_account"] = json!("zzz");
    let (_, a, recs) = audit_after(&mut g, |g| g.send_v(&v));
    assert_eq!(a.view_account, 0, "CTL-101.9");
    assert_eq!(G0::<CtlTestSet>::cmds(&recs).len(), 1, "CTL-101.9");
    // .10
    let v = g.req(
        "cli:tom",
        Some("*"),
        json!({"type":"Annotate","kind":"OpsNote","text":"x"}),
    );
    let (_, a, _) = audit_after(&mut g, |g| g.send_v(&v));
    assert_eq!(a.peer_uid, 1201, "CTL-101.10");
    // .11
    let (_, a, _) = audit_after(&mut g, |g| {
        g.core
            .on_internal(InternalCmd::Drain { cancel_resting: true }, Tsc(g.now));
        None
    });
    assert_eq!(a.peer_uid, 0, "CTL-101.11");
}

#[test]
fn ctl_102_view_account_audit_only() {
    let mut g = G0::new();
    let cmd: Value = json!({"type":"HedgeFix","group":"16777317"});
    let v1 = g.req("tg:1002", Some("acct-a"), cmd.clone());
    let mut v2 = g.req("tg:1002", Some("acct-a"), cmd);
    v2["view_account"] = json!("acct-b");
    let (_, a1, r1) = audit_after(&mut g, |g| g.send_v(&v1));
    let (_, a2, r2) = audit_after(&mut g, |g| g.send_v(&v2));
    let c1 = G0::<CtlTestSet>::cmds(&r1)[0].logical().to_vec();
    let c2 = G0::<CtlTestSet>::cmds(&r2)[0].logical().to_vec();
    assert_eq!(c1[16..], c2[16..], "records equal except cmd_id");
    assert_ne!(c1[..16], c2[..16]);
    let (h, _) = G0::<CtlTestSet>::cmds(&r1)[0].cmd();
    assert_eq!(h.account.0, 1);
    assert_ne!(a1.cmd_id, a2.cmd_id);
    assert_eq!((a1.view_account, a2.view_account), (0, 2));
    let strip = |a: &AuditV| AuditV {
        cmd_id: [0; 16],
        view_account: 0,
        ..a.clone()
    };
    assert_eq!(strip(&a1), strip(&a2));
}
