//! CTL-050–CTL-055.
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

fn written(g: &mut G0, issuer: &str, account: Option<&str>, cmd: Value) -> Result<(), String> {
    let r = g.send(issuer, account, cmd);
    if code(&r) == "Applied" {
        Ok(())
    } else {
        Err(code(&r).to_owned())
    }
}

fn fill_12(g: &mut G0, issuer: &str) {
    for k in 0..12u64 {
        g.at(k * S);
        written(g, issuer, Some("acct-a"), hedge()).unwrap();
    }
}

#[test]
fn ctl_050_write_rate_window() {
    let mut g = G0::new();
    fill_12(&mut g, "cli:tom");
    g.at(59_999 * MS);
    let before = g.seen.len();
    assert_eq!(
        written(&mut g, "cli:tom", Some("acct-a"), hedge()),
        Err("RateLimited".into()),
        "CTL-050.1"
    );
    g.take_ring();
    let recs = &g.seen[before..];
    assert!(G0::<CtlTestSet>::cmds(recs).is_empty(), "CTL-050.1");
    assert_eq!(only_audit(recs).result_code, 30, "CTL-050.1");
    g.at(60_000 * MS);
    assert_eq!(
        written(&mut g, "cli:tom", Some("acct-a"), hedge()),
        Ok(()),
        "CTL-050.2"
    );
    assert_eq!(
        written(&mut g, "cli:tom", Some("acct-a"), hedge()),
        Err("RateLimited".into()),
        "CTL-050.3"
    );
    g.at(61_000 * MS);
    assert_eq!(
        written(&mut g, "cli:tom", Some("acct-a"), hedge()),
        Ok(()),
        "CTL-050.4"
    );
}

#[test]
fn ctl_051_uncounted_classes() {
    let mut g = G0::new();
    fill_12(&mut g, "cli:tom");
    g.at(12 * S);
    let rows: Vec<(&str, Option<&str>, Value)> = vec![
        ("CTL-051.1", None, json!({"type":"Kill","level":"Global"})),
        ("CTL-051.2", Some("acct-a"), json!({"type":"CancelAll"})),
        (
            "CTL-051.3",
            Some("acct-a"),
            json!({"type":"SetMode","strategy":"s1-main","group":"16777317","mode":"CloseOnly"}),
        ),
        (
            "CTL-051.4",
            None,
            json!({"type":"VenueNotice","venue":"binance-usdm","kind":"Delisting","at_ns":"1790100000000000000"}),
        ),
        (
            "CTL-051.5",
            None,
            json!({"type":"GetSchema","strategy_kind":"probe-taker"}),
        ),
        (
            "CTL-051.6",
            None,
            json!({"type":"InjectWarmup","strategy":"s1-main","local_kind":"3","chunk_no":"0","payload_b64":"AQ=="}),
        ),
    ];
    for (row, account, cmd) in rows {
        assert_eq!(written(&mut g, "cli:tom", account, cmd), Ok(()), "{row}");
    }
    assert_eq!(
        written(&mut g, "cli:tom", Some("acct-a"), hedge()),
        Err("RateLimited".into()),
        "CTL-051.7"
    );
}

#[test]
fn ctl_052_rejected_still_counted() {
    let mut g = G0::new();
    for _ in 0..12 {
        let r = g.send("cli:tom", Some("nope"), hedge());
        assert_eq!(
            (code(&r), detail_name(&r).as_str()),
            ("InvalidArgs", "UnknownName")
        );
    }
    g.at(20 * S);
    assert_eq!(
        written(&mut g, "cli:tom", Some("acct-a"), hedge()),
        Err("RateLimited".into())
    );
    g.at(30 * S);
    assert_eq!(
        written(&mut g, "cli:tom", Some("acct-a"), hedge()),
        Err("RateLimited".into())
    );
    g.at(60_001 * MS);
    assert_eq!(written(&mut g, "cli:tom", Some("acct-a"), hedge()), Ok(()));
}

#[test]
fn ctl_053_preview_counted() {
    let mut g = G0::new();
    let live = json!({"type":"SetMode","strategy":"s1-main","group":"16777317","mode":"Live"});
    for k in 0..6u64 {
        g.at(k * S);
        let mut v = g.req("tg:1002", Some("acct-a"), live.clone());
        v["preview"] = json!(true);
        assert_eq!(code(&g.send_and_wait(&v)), "Applied");
        let mut v = g.req("tg:1002", Some("acct-a"), live.clone());
        v["confirmed"] = json!(true);
        v["expect_state_version"] = json!(21);
        assert_eq!(code(&g.send_and_wait(&v)), "Applied");
    }
    let mut v = g.req("tg:1002", Some("acct-a"), live);
    v["preview"] = json!(true);
    assert_eq!(code(&g.send_and_wait(&v)), "RateLimited");
}

#[test]
fn ctl_054_per_issuer() {
    let mut g = G0::new();
    fill_12(&mut g, "tg:1002");
    assert_eq!(written(&mut g, "tg:1003", Some("acct-a"), hedge()), Ok(()));
    assert_eq!(
        written(&mut g, "tg:1002", Some("acct-a"), hedge()),
        Err("RateLimited".into())
    );
}

#[test]
fn ctl_055_warmup_rate() {
    let mut g = G0::new();
    let chunk = b64(&[9u8; 4096]);
    let warm = |k: u64| json!({"type":"InjectWarmup","strategy":"s2-main","local_kind":"3","chunk_no":k.to_string(),"payload_b64":chunk});
    for k in 0..50u64 {
        g.at(k * 10 * MS);
        assert_eq!(
            written(&mut g, "pipeline", None, warm(k)),
            Ok(()),
            "CTL-055.1 block {}",
            k + 1
        );
        if k == 25 {
            let sp = json!({"type":"SetParams","strategy":"s1-main","blob_b64":b64(&Spb::s1(42).bytes())});
            assert_eq!(
                written(&mut g, "pipeline", Some("acct-a"), sp),
                Ok(()),
                "CTL-055.4"
            );
        }
    }
    g.at(500 * MS);
    assert_eq!(
        written(&mut g, "pipeline", None, warm(50)),
        Err("RateLimited".into()),
        "CTL-055.2"
    );
    for k in 51..100u64 {
        g.at(k * 10 * MS);
        let _ = written(&mut g, "pipeline", None, warm(k));
    }
    g.at(1_000 * MS);
    assert_eq!(written(&mut g, "pipeline", None, warm(100)), Ok(()), "CTL-055.3");
}
