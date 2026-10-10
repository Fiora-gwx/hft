//! CTL-208.

use super::*;

const A: Option<&str> = Some("acct-a");

fn smoke() -> Value {
    json!({"type":"SmokeOrder","venue":"bitget-spot","inst":"bitget-spot:BTCUSDT","side":"Buy","ttl_ms":"30000"})
}

#[test]
fn ctl_208_smoke_order_codes() {
    // .1 without confirmation.
    let mut r = e2e();
    want(
        &r.send("cli:alice", A, smoke()),
        "NeedsConfirm",
        None,
        "CTL-208.1",
    );

    // .2 HARD Kill of acct-a: the check chain refuses the smoke order.
    let mut r = e2e();
    want(
        &r.send(
            "cli:alice",
            A,
            json!({"type":"Kill","level":"Account","hard":true}),
        ),
        "Applied",
        None,
        "CTL-208.2 kill",
    );
    let (_, c) = r.two_step("cli:alice", A, smoke());
    want(&c, "Rejected", Some("SmokeRiskReject"), "CTL-208.2");
    assert_eq!(arg(&c), "HARD", "CTL-208.2 arg");

    // .3 Draining.
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
        "CTL-208.3 drain",
    );
    let (p, c) = r.two_step("cli:alice", A, smoke());
    want(&p, "PreconditionFailed", Some("Draining"), "CTL-208.3 preview");
    want(&c, "PreconditionFailed", Some("Draining"), "CTL-208.3");

    // .4 acct-a's start gate not released.
    let mut r = E2e::with(Opts {
        release: vec![ACCT_B, SIM_A],
        ..Opts::default()
    });
    let (p, c) = r.two_step("cli:alice", A, smoke());
    want(
        &p,
        "PreconditionFailed",
        Some("GateNotReleased"),
        "CTL-208.4 preview",
    );
    want(&c, "PreconditionFailed", Some("GateNotReleased"), "CTL-208.4");

    // .5 Telegram Admin, private chat.
    let mut r = e2e();
    let m = r.mark();
    let (_, c) = r.two_step("tg:1003", A, smoke());
    want(&c, "Applied", None, "CTL-208.5");
    assert_eq!(r.new_orders_since(m, I_BG_SPOT).len(), 1, "CTL-208.5 order");
}
