//! CTL-090–CTL-093.
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path

use crate::common::*;
use hft_strategy_sdk::StrategySet;
use serde_json::{Value, json};

fn gs(name: &str) -> Value {
    json!({"type":"GetSchema","strategy_kind":name})
}

fn gk(id: &str) -> Value {
    json!({"type":"GetSchema","kind_id":id})
}

#[test]
fn ctl_090_get_schema_local() {
    let mut g = G0::new();
    let r = g.send("cli:vera", None, gs("probe-taker"));
    assert_eq!((status(&r).as_str(), code(&r)), ("ok", "Applied"));
    assert!(r.cmd_hash.is_none());
    assert_eq!(r.body.get(), CtlTestSet::schema_json(0xFF01).unwrap());
    let recs = g.take_ring();
    assert!(!recs.iter().any(RingRec::is_cmd));
    let a = only_audit(&recs);
    assert_eq!((a.cmd_kind, a.result_code, a.role, a.chat_type), (0, 0, 1, 3));
    assert!(g.unanswered.is_empty());
}

#[test]
fn ctl_091_get_schema_lookup() {
    let mut g = G0::new();
    let r = g.send("cli:tom", None, gk("65281"));
    assert_eq!(
        r.body.get(),
        CtlTestSet::schema_json(0xFF01).unwrap(),
        "CTL-091.1"
    );
    let r = g.send("cli:tom", None, gk("65282"));
    assert_eq!(
        r.body.get(),
        CtlTestSet::schema_json(0xFF02).unwrap(),
        "CTL-091.2"
    );
    for (row, cmd) in [("CTL-091.3", gs("nope")), ("CTL-091.4", gk("1"))] {
        let r = g.send("cli:tom", None, cmd);
        assert_eq!(
            (code(&r), detail_name(&r).as_str()),
            ("InvalidArgs", "UnknownName"),
            "{row}"
        );
    }
}

#[test]
fn ctl_092_get_schema_access() {
    let mut g = G0::new();
    for k in 0..12 {
        g.at(k * S);
        assert_eq!(
            code(&g.send(
                "cli:tom",
                Some("acct-a"),
                json!({"type":"HedgeFix","group":"16777317"})
            )),
            "Applied"
        );
    }
    assert_eq!(
        code(&g.send(
            "cli:tom",
            Some("acct-a"),
            json!({"type":"HedgeFix","group":"16777317"})
        )),
        "RateLimited"
    );
    assert_eq!(
        code(&g.send("cli:tom", None, gs("probe-taker"))),
        "Applied",
        "CTL-092.1"
    );
    for k in 0..200u64 {
        g.at(12 * S + k * 5 * MS);
        assert_eq!(
            code(&g.send("cli:tom", None, gs("probe-taker"))),
            "Applied",
            "CTL-092.2 #{k}"
        );
    }
    let mut v = g.req("tg:1001", None, gs("probe-taker"));
    v["chat_type"] = json!("group");
    assert_eq!(code(&g.send_and_wait(&v)), "Applied", "CTL-092.3");
    assert_eq!(
        code(&g.send("pipeline", None, gs("probe-taker"))),
        "Applied",
        "CTL-092.4"
    );
    assert_eq!(
        code(&g.send("ops_bot", None, gs("probe-taker"))),
        "Forbidden",
        "CTL-092.5"
    );
    assert_eq!(
        code(&g.send("reconciler", None, gs("probe-taker"))),
        "Forbidden",
        "CTL-092.6"
    );
}

#[test]
fn ctl_093_get_schema_matches_export() {
    let dir = std::env::temp_dir().join(format!("ctl-093-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // The export: one `<kind_id>-<name>.schema.json` per kind from `S::schema_json`.
    for d in CtlTestSet::kinds() {
        let p = dir.join(format!("{}-{}.schema.json", d.kind_id, d.name));
        std::fs::write(&p, CtlTestSet::schema_json(d.kind_id).unwrap()).unwrap();
    }
    let mut g = G0::new();
    for d in CtlTestSet::kinds() {
        let r = g.send("cli:vera", None, gs(d.name));
        let file = std::fs::read(dir.join(format!("{}-{}.schema.json", d.kind_id, d.name))).unwrap();
        assert_eq!(r.body.get().as_bytes(), &file[..], "{}", d.name);
    }
    let _ = std::fs::remove_dir_all(&dir);
}
