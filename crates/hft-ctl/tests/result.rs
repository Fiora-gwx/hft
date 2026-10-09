//! CTL-110–CTL-114.
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path

mod common;

use common::*;
use hft_ctl_proto::Hex32;
use serde_json::{Value, json};

fn hedge() -> Value {
    json!({"type":"HedgeFix","group":"16777317"})
}

fn cmd_count(g: &G0) -> usize {
    G0::<CtlTestSet>::cmds(&g.seen).len()
}

#[test]
fn ctl_110_result_returned() {
    let mut g = G0::new();
    let v = g.req("cli:tom", Some("acct-a"), hedge());
    let r = g.send_and_wait(&v);
    assert_eq!((status(&r).as_str(), code(&r)), ("ok", "Applied"));
    assert_eq!(
        (r.seq, r.ctl_state_version, r.engine.epoch),
        (Some(900), Some(21), 17)
    );
    let (h, _) = G0::<CtlTestSet>::cmds(&g.seen)[0].cmd();
    assert_eq!(r.cmd_hash, Some(Hex32(h.cmd_hash)));
    assert_eq!(g.core.pending_len(), 0);
}

#[test]
fn ctl_111_accepted_after_wait() {
    // .1 result read at 4 999 ms.
    let mut g = G0::new();
    g.paused = true;
    let v = g.req("cli:tom", Some("acct-a"), hedge());
    assert!(g.send_v(&v).is_none());
    g.step();
    g.at(4_999 * MS);
    g.answer_all();
    let r = &g.delivered.last().unwrap().1;
    assert_eq!((status(r).as_str(), code(r)), ("ok", "Applied"), "CTL-111.1");
    // .2 still nothing at 5 000 ms → accepted.
    let mut g = G0::new();
    g.paused = true;
    let v = g.req("cli:tom", Some("acct-a"), hedge());
    assert!(g.send_v(&v).is_none());
    g.at(5_000 * MS);
    g.step();
    let r = &g.delivered.last().unwrap().1;
    assert_eq!((status(r).as_str(), r.code), ("accepted", None), "CTL-111.2");
    // .4 resend before the result → accepted, nothing written.
    let r = g.send_v(&v).unwrap();
    assert_eq!(status(&r), "accepted", "CTL-111.4");
    assert_eq!(cmd_count(&g), 1, "CTL-111.4");
    // .3 the result arrives at 5 001 ms → cached; resend → ok Applied.
    g.at(5_001 * MS);
    g.answer_all();
    let r = g.send_v(&v).unwrap();
    assert_eq!((status(&r).as_str(), code(&r)), ("ok", "Applied"), "CTL-111.3");
    g.take_ring();
    assert_eq!(cmd_count(&g), 1, "CTL-111.3");
}

#[test]
fn ctl_112_duplicate_cmd_id() {
    // .1
    let mut g = G0::new();
    let v = g.req("cli:tom", Some("acct-a"), hedge());
    let first = g.send_and_wait(&v);
    let again = g.send_v(&v).unwrap();
    assert_eq!((code(&again), again.seq), ("Applied", first.seq), "CTL-112.1");
    g.take_ring();
    assert_eq!(cmd_count(&g), 1, "CTL-112.1");
    // .2
    let mut g = G0::new();
    g.paused = true;
    let v = g.req("cli:tom", Some("acct-a"), hedge());
    assert!(g.send_v(&v).is_none());
    assert_eq!(status(&g.send_v(&v).unwrap()), "accepted", "CTL-112.2");
    g.take_ring();
    assert_eq!(cmd_count(&g), 1, "CTL-112.2");
    // .3
    let mut g = G0::new();
    let mut v = g.req("cli:tom", Some("nope"), hedge());
    assert_eq!(detail_name(&g.send_and_wait(&v)), "UnknownName", "CTL-112.3");
    v["account"] = json!("acct-a");
    assert_eq!(code(&g.send_and_wait(&v)), "Applied", "CTL-112.3");
    assert_eq!(cmd_count(&g), 1, "CTL-112.3");
    // .4 a rebuilt gateway (cache empty): written again, the sequencer answers Duplicate.
    let mut g = G0::new();
    let v = g.req("cli:tom", Some("acct-a"), hedge());
    assert_eq!(code(&g.send_and_wait(&v)), "Applied");
    let seen_ids = g.seq_seen_ids.clone();
    let mut g2 = G0::new();
    g2.seq_seen_ids = seen_ids;
    let r = g2.send_and_wait(&v);
    assert_eq!(code(&r), "Duplicate", "CTL-112.4");
    assert_eq!(cmd_count(&g2), 1, "CTL-112.4");
}

#[test]
fn ctl_113_dedup_eviction() {
    let mk = || {
        let mut cfg = config(std::env::temp_dir().join("ctl-113.sock"));
        cfg.dedup_capacity = 4;
        G0::with_cfg(p0::p0(), 262_144, g0_active(), cfg)
    };
    let mut g = mk();
    let c: Vec<Value> = (0..5)
        .map(|_| g.req("cli:tom", Some("acct-a"), hedge()))
        .collect();
    for v in &c[..4] {
        g.send_and_wait(v);
    }
    let n = cmd_count(&g);
    assert_eq!(code(&g.send_v(&c[0]).unwrap()), "Applied", "CTL-113.1");
    g.take_ring();
    assert_eq!(cmd_count(&g), n, "CTL-113.1");
    let mut g = mk();
    for v in &c {
        g.send_and_wait(v);
    }
    let n = cmd_count(&g);
    assert_eq!(code(&g.send_and_wait(&c[0])), "Duplicate", "CTL-113.2");
    assert_eq!(cmd_count(&g), n + 1, "CTL-113.2");
    let mut g = mk();
    for v in &c {
        g.send_and_wait(v);
    }
    let n = cmd_count(&g);
    assert_eq!(code(&g.send_v(&c[1]).unwrap()), "Applied", "CTL-113.3");
    g.take_ring();
    assert_eq!(cmd_count(&g), n, "CTL-113.3");
}

#[test]
fn ctl_114_concurrent_same_cmd_id() {
    let mut g = G0::new();
    let v = g.req("cli:tom", Some("acct-a"), hedge());
    let a = g.send_v(&v);
    let b = g.send_v(&v);
    assert!(a.is_none());
    assert_eq!(status(b.as_ref().unwrap()), "accepted");
    g.step();
    assert_eq!(cmd_count(&g), 1);
    assert_eq!(g.seq_seen_ids.len(), 1);
    assert_eq!(code(&g.delivered.last().unwrap().1), "Applied");
}
