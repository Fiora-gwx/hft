//! CTL-020.
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path

use crate::common::*;
use serde_json::{Value, json};

fn hedge() -> Value {
    json!({"type":"HedgeFix","group":"16777317"})
}

fn fill_rate(g: &mut G0) {
    for _ in 0..12 {
        assert_eq!(code(&g.send("cli:tom", Some("acct-a"), hedge())), "Applied");
    }
}

/// A request issued at 0 that expired at 30 s, sent at 31 s.
fn expired(g: &mut G0, issuer: &str, account: Option<&str>, cmd: Value) -> Value {
    let mut v = g.req(issuer, account, cmd);
    v["issued_at_ns"] = json!(UTC0);
    v["expires_at_ns"] = json!(UTC0 + 30 * S as i64);
    g.at(31 * S);
    v
}

#[test]
fn ctl_020_first_failure_wins() {
    // .1 unknown uid + body not JSON.
    let mut g = G0::new();
    let r = g.send_raw(9999, b"not json").unwrap();
    assert_eq!(
        (status(&r).as_str(), r.cmd_id),
        ("unauthorized", hft_ctl_proto::Uuid::nil()),
        "CTL-020.1"
    );
    // .2 body not JSON (from a known uid) + an unknown issuer cannot be read.
    let r = g.send_raw(1100, b"not json").unwrap();
    assert_eq!(status(&r), "invalid_request", "CTL-020.2");
    // .3 unknown issuer + SetRisk.
    let v = g.req(
        "tg:9999",
        Some("acct-a"),
        json!({"type":"SetRisk","blob_b64":b64(&risk_blob(0))}),
    );
    assert_eq!(status(&g.send_and_wait(&v)), "unauthorized", "CTL-020.3");
    // .4 Viewer SetMode→Live + confirmed without expect_state_version.
    let mut v = g.req(
        "tg:1001",
        Some("acct-a"),
        json!({"type":"SetMode","strategy":"s1-main","group":"16777317","mode":"Live"}),
    );
    v["confirmed"] = json!(true);
    assert_eq!(code(&g.send_and_wait(&v)), "Forbidden", "CTL-020.4");
    // .5 Trader, same + expired.
    let mut g = G0::new();
    let mut v = expired(
        &mut g,
        "tg:1002",
        Some("acct-a"),
        json!({"type":"SetMode","strategy":"s1-main","group":"16777317","mode":"Live"}),
    );
    v["confirmed"] = json!(true);
    assert_eq!(status(&g.send_and_wait(&v)), "invalid_request", "CTL-020.5");
    // .6 cached Applied + the resend is expired.
    let mut g = G0::new();
    let v = g.req("cli:tom", Some("acct-a"), hedge());
    let first = g.send_and_wait(&v);
    assert_eq!(code(&first), "Applied");
    g.at(31 * S);
    let n = g.seen.len();
    let again = g.send_v(&v).unwrap();
    assert_eq!((code(&again), again.seq), ("Applied", first.seq), "CTL-020.6");
    g.take_ring();
    assert!(G0::<CtlTestSet>::cmds(&g.seen[n..]).is_empty(), "CTL-020.6");
    // .7 expired + rate full.
    let mut g = G0::new();
    fill_rate(&mut g);
    let v = expired(&mut g, "cli:tom", Some("acct-a"), hedge());
    assert_eq!(code(&g.send_and_wait(&v)), "Expired", "CTL-020.7");
    // .8 rate full + account `nope`.
    let mut g = G0::new();
    fill_rate(&mut g);
    assert_eq!(
        code(&g.send("cli:tom", Some("nope"), hedge())),
        "RateLimited",
        "CTL-020.8"
    );
    // .9 account `nope` + ring full.
    let mut g = G0::new();
    g.core.ring_mut().free = 0;
    let r = g.send("cli:tom", Some("nope"), hedge());
    assert_eq!(
        (code(&r), detail_name(&r).as_str()),
        ("InvalidArgs", "UnknownName"),
        "CTL-020.9"
    );
    // .10 SmokeOrder on sim-a (NotLive) + ring full.
    let mut g = G0::new();
    g.core.ring_mut().free = 0;
    let r = g.send(
        "cli:alice",
        Some("sim-a"),
        json!({"type":"SmokeOrder","venue":"bitget-spot","inst":"bitget-spot:BTCUSDT","side":"Buy"}),
    );
    assert_eq!(
        (code(&r), detail_name(&r).as_str()),
        ("InvalidArgs", "NotLive"),
        "CTL-020.10"
    );
    // .11 legal + ring full.
    let mut g = G0::new();
    g.core.ring_mut().free = 0;
    assert_eq!(
        status(&g.send("cli:tom", Some("acct-a"), hedge())),
        "busy",
        "CTL-020.11"
    );
}
