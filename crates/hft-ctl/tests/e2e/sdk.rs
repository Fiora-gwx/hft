//! SDK-019, SDK-054, SDK-060, SDK-119 (hft-strategy-sdk cases that need the gateway): the
//! strategy calls are those of `RecSet` (`rec.rs`), the recording wrapper of the E2E strategy
//! set.

use super::*;
use crate::common::b64;
use crate::e2e::set_params::sp;
use crate::e2e::strategy_channel::{op1, op2, strategy_cmd};

const A: Option<&str> = Some("acct-a");

/// Strategy calls other than the gateway's static checks (`decode_params`/`validate_params`).
fn instance_calls() -> Vec<rec::Got> {
    rec::log()
        .into_iter()
        .filter(|g| g.call != "decode_params" && g.call != "validate_params")
        .collect()
}

/// `(s1-main, acct-a)` version 42 with field `field`'s default written as `value` (past the
/// schema's checks: the bytes are patched after encoding).
fn spb_raw(field: usize, value: i64) -> Vec<u8> {
    let p = Spb::s1(42);
    let mut b = p.bytes();
    let n_fields = p.values.len();
    let body = b.len() - (8 + 8 * n_fields);
    let at = body + 8 + 8 * field;
    b[at..at + 8].copy_from_slice(&value.to_le_bytes());
    b
}

#[test]
fn sdk_019_gateway_reject_no_strategy_call() {
    let mut r = e2e();
    rec::clear();
    let m = r.mark();
    let v = json!({"type":"SetParams","strategy":"s1-main","blob_b64":b64(&spb_raw(tf::WIN, 1_000_001))});
    let x = r.send("cli:tom", A, v);
    want(&x, "InvalidArgs", Some("ParamInvalid"), "SDK-019");
    let d = x.detail.as_ref().expect("detail");
    assert_eq!(d.arg.as_deref(), Some("Strategy:0"), "SDK-019 arg {d:?}");
    assert!(instance_calls().is_empty(), "SDK-019 {:?}", instance_calls());
    assert!(
        r.kinds_since(m, EventKind::CONTROL_CMD).is_empty(),
        "SDK-019 written"
    );
    assert_eq!(r.params_version(S1), 41, "SDK-019");
}

#[test]
fn sdk_054_no_cmds_no_manual() {
    let mut r = e2e();
    rec::clear();
    let x = r.send(
        "cli:tom",
        Some("acct-b"),
        json!({"type":"StrategyCmd","strategy":"s2-main","group":"none","reduce_only":false,
               "payload_b64":b64(&[1, 1, 0, 0, 0, 0, 0, 0])}),
    );
    want(&x, "InvalidArgs", Some("ParamInvalid"), "SDK-054");
    assert!(!rec::log().iter().any(|g| g.ev == "Manual"), "SDK-054");
}

#[test]
fn sdk_060_too_many_groups_at_gateway() {
    let mut r = e2e();
    let mut p = Spb::s1(42);
    p.groups = (101..110u32).map(|k| g_target(GroupId((1 << 24) | k))).collect();
    rec::clear();
    let m = r.mark();
    let x = r.send("pipeline", A, sp(&p, None));
    want(&x, "InvalidArgs", Some("ParamInvalid"), "SDK-060");
    let d = x.detail.as_ref().expect("detail");
    assert!(
        d.arg.as_deref().is_some_and(|a| a.starts_with("Strategy")),
        "SDK-060 {d:?}"
    );
    assert!(
        r.kinds_since(m, EventKind::CONTROL_CMD).is_empty(),
        "SDK-060 written"
    );
    assert_eq!(r.params_version(S1), 41, "SDK-060");
    assert!(instance_calls().is_empty(), "SDK-060 {:?}", instance_calls());
}

#[test]
fn sdk_119_gateway_cmd_checks() {
    let mut op9 = op1();
    op9[1] = 9;
    let mut big = op1();
    let n = big.len();
    big[n - 8..].copy_from_slice(&10_000_000_001i64.to_le_bytes());
    let rows: Vec<(&str, bool, Vec<u8>, &str, Option<&str>)> = vec![
        ("SDK-119.1", true, op2(), "Applied", None),
        ("SDK-119.2", false, op2(), "InvalidArgs", Some("ParamInvalid")),
        ("SDK-119.3", true, op1(), "InvalidArgs", Some("ParamInvalid")),
        ("SDK-119.4", false, op9, "InvalidArgs", Some("ParamInvalid")),
        ("SDK-119.5", false, big, "InvalidArgs", Some("ParamInvalid")),
        ("SDK-119.6", true, vec![0u8; 257], "InvalidArgs", Some("TooLong")),
    ];
    for (row, ro, payload, code, det) in rows {
        let mut r = e2e();
        rec::clear();
        let x = if code == "Applied" {
            r.send("cli:tom", A, strategy_cmd("16777317", ro, &payload))
        } else {
            // Rejected before any confirmation matters: a confirmed request with the version.
            let v = r.version();
            r.confirm("cli:tom", A, strategy_cmd("16777317", ro, &payload), Some(v))
        };
        want(&x, code, det, row);
        let manual: Vec<rec::Got> = rec::log()
            .into_iter()
            .filter(|g| g.call == "on_event" && g.ev == "Manual")
            .collect();
        if code == "Applied" {
            assert_eq!(manual.len(), 1, "{row}");
            assert!(manual[0].flag, "{row}: reduce_only");
            assert_eq!(manual[0].payload, payload, "{row}");
        } else {
            assert!(manual.is_empty(), "{row}");
        }
    }
}
