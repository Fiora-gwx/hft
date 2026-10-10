//! CTL-202.

use super::*;
use crate::common::b64;

const A: Option<&str> = Some("acct-a");

fn set_risk(version: u64, max_order_usdt: i64) -> Value {
    json!({"type":"SetRisk","blob_b64":b64(&risk_blob(version, max_order_usdt, 0))})
}

fn risk_version(r: &E2e) -> u64 {
    r.s.engine().risk.params().version
}

#[test]
fn ctl_202_set_risk_codes() {
    // .1 tighter, no confirmation.
    let mut r = e2e();
    want(
        &r.send("cli:alice", A, set_risk(8, 500)),
        "Applied",
        None,
        "CTL-202.1",
    );
    assert_eq!(risk_version(&r), 8, "CTL-202.1");
    // .2 looser, no confirmation.
    let mut r = e2e();
    want(
        &r.send("cli:alice", A, set_risk(8, 2_000)),
        "NeedsConfirm",
        None,
        "CTL-202.2",
    );
    // .3 the same, two-step.
    let (p, c) = r.two_step("cli:alice", A, set_risk(8, 2_000));
    want(&p, "Applied", None, "CTL-202.3 preview");
    want(&c, "Applied", None, "CTL-202.3");
    assert_eq!(risk_version(&r), 8, "CTL-202.3");
    // .4 below acct-a's force_slice.max_chunk_notional (200 USDT).
    let mut r = e2e();
    want(
        &r.send("cli:alice", A, set_risk(8, 199)),
        "InvalidArgs",
        Some("ExecRiskMismatch"),
        "CTL-202.4",
    );
    assert_eq!(risk_version(&r), 7, "CTL-202.4");
    // .5 document version 7 (= current).
    let mut r = e2e();
    want(
        &r.send("cli:alice", A, set_risk(7, 500)),
        "StaleVersion",
        None,
        "CTL-202.5",
    );
    // .6 Draining: the loosening, two-step.
    let mut r = e2e();
    r.hold_drain();
    want(
        &r.send(
            "cli:alice",
            Some("*"),
            json!({"type":"Drain","cancel_resting":false}),
        ),
        "Applied",
        None,
        "CTL-202.6 drain",
    );
    let (p, c) = r.two_step("cli:alice", A, set_risk(8, 2_000));
    want(&p, "PreconditionFailed", Some("Draining"), "CTL-202.6 preview");
    want(&c, "PreconditionFailed", Some("Draining"), "CTL-202.6");
    // .7 Draining: the tightening.
    want(
        &r.send("cli:alice", A, set_risk(8, 500)),
        "Applied",
        None,
        "CTL-202.7",
    );
}
