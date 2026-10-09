//! CTP-070–CTP-072.
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path

use crate::common::*;
use hft_ctl_proto::cmd_hash;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[test]
fn ctp_070_cmd_hash_definition() {
    let n = ok(
        Some("acct-a"),
        json!({"type":"SetMode","strategy":"s1-main","group":"16777317","mode":"Live"}),
    );
    let p = &n.record[128..];
    assert_eq!(p.len(), 24);
    let mut h = Sha256::new();
    h.update([0x01]);
    h.update([0x01, 0x00]);
    h.update([0x01, 0x00]);
    h.update(p);
    let want: [u8; 32] = h.finalize().into();
    assert_eq!(n.cmd_hash, want);
    assert_eq!(&n.record[80..112], &want);
    assert_eq!(n.hdr.cmd_hash, want);
}

fn hash_of(v: &Value) -> [u8; 32] {
    hft_ctl_proto::normalize(&parse(v), rd()).unwrap().cmd_hash
}

type Edit = Box<dyn Fn(&mut Value)>;

#[test]
fn ctp_071_envelope_not_hashed() {
    let mut q = e0(Some("acct-a"), json!({"type":"Kill","level":"Account"}));
    q["issuer"] = json!("tg:1001");
    q["chat_type"] = json!("private");
    q["preview"] = json!(true);
    let base = hash_of(&q);
    let rows: Vec<(&str, Edit)> = vec![
        (
            "CTP-071.1",
            Box::new(|v| v["cmd_id"] = json!("11111111-2222-4333-8444-555555555555")),
        ),
        ("CTP-071.2", Box::new(|v| v["issuer"] = json!("tg:1002"))),
        (
            "CTP-071.3",
            Box::new(|v| {
                v["issued_at_ns"] = json!(ISSUED + 20_000_000_000);
                v["expires_at_ns"] = json!(EXPIRES + 20_000_000_000);
            }),
        ),
        (
            "CTP-071.4",
            Box::new(|v| {
                v["preview"] = json!(false);
                v["confirmed"] = json!(true);
            }),
        ),
        ("CTP-071.5", Box::new(|v| v["expect_state_version"] = json!(77))),
        (
            "CTP-071.6",
            Box::new(|v| {
                v["view_account"] = json!("acct-b");
                v["chat_type"] = json!("group");
            }),
        ),
    ];
    for (row, f) in rows {
        let mut v = q.clone();
        f(&mut v);
        assert_eq!(hash_of(&v), base, "{row}");
    }
    let sp = e0(
        Some("acct-a"),
        json!({"type":"SetParams","strategy":"s1-main","blob_b64":spb1_b64(1, 1, 42, 16)}),
    );
    let mut spb = sp.clone();
    spb["cmd"]["base_version"] = json!("41");
    assert_eq!(hash_of(&sp), hash_of(&spb), "CTP-071.7");
}

#[test]
fn ctp_072_hash_distinguishes() {
    let h = |a: Option<&str>, c: Value| ok(a, c).cmd_hash;
    let ka = json!({"type":"Kill","level":"Account"});
    assert_ne!(
        h(None, json!({"type":"Kill","level":"Global"})),
        h(Some("acct-a"), ka.clone()),
        "CTP-072.1"
    );
    assert_ne!(
        h(Some("acct-a"), ka.clone()),
        h(Some("acct-b"), ka.clone()),
        "CTP-072.2"
    );
    assert_ne!(
        h(Some("acct-a"), ka.clone()),
        h(
            Some("acct-a"),
            json!({"type":"Kill","level":"Account","hard":true})
        ),
        "CTP-072.3"
    );
    assert_ne!(
        h(Some("acct-a"), ka),
        h(
            Some("acct-a"),
            json!({"type":"Kill","level":"Account","cancel_resting":false})
        ),
        "CTP-072.4"
    );
    let scope = [0u8; 16];
    assert_ne!(
        cmd_hash(1, 0x000E, 1, &scope),
        cmd_hash(1, 0x000F, 1, &scope),
        "CTP-072.5"
    );
    let b = spb1(1, 1, 42, 16);
    let mut b2 = b.clone();
    *b2.last_mut().unwrap() = 1;
    let sp = |b: &[u8]| json!({"type":"SetParams","strategy":"s1-main","blob_b64":b64(b)});
    assert_ne!(h(Some("acct-a"), sp(&b)), h(Some("acct-a"), sp(&b2)), "CTP-072.6");
    let iw = |c: &str| json!({"type":"InjectWarmup","strategy":"s2-main","local_kind":"3","chunk_no":c,"payload_b64":"AQ=="});
    assert_ne!(h(None, iw("0")), h(None, iw("1")), "CTP-072.7");
}
