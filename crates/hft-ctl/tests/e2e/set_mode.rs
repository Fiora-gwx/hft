//! CTL-200.

use super::*;

fn set_mode(mode: &str) -> Value {
    json!({"type":"SetMode","strategy":"s1-main","group":"16777317","mode":mode})
}

fn drain() -> Value {
    json!({"type":"Drain","cancel_resting":false})
}

const A: Option<&str> = Some("acct-a");

#[test]
fn ctl_200_set_mode_codes() {
    // .1 CloseOnly from Live.
    let mut r = e2e();
    r.position_g1();
    let v0 = r.version();
    let x = r.send("cli:tom", A, set_mode("CloseOnly"));
    want(&x, "Applied", None, "CTL-200.1");
    assert!(
        matches!(r.mode(S1, ACCT_A, G1), Effective::CloseOnly { .. }),
        "CTL-200.1"
    );
    assert_eq!(r.version(), v0 + 1, "CTL-200.1");
    assert_eq!(x.ctl_state_version, Some(v0 + 1), "CTL-200.1");
    // .2 the same request again (new cmd_id).
    let x = r.send("cli:tom", A, set_mode("CloseOnly"));
    want(&x, "NoOp", None, "CTL-200.2");
    assert_eq!(r.version(), v0 + 1, "CTL-200.2");

    // .3 Shadow with a position: deferred.
    let mut r = e2e();
    r.position_g1();
    let x = r.send("cli:tom", A, set_mode("Shadow"));
    want(&x, "Applied", Some("ShadowDeferred"), "CTL-200.3");
    assert!(
        matches!(r.mode(S1, ACCT_A, G1), Effective::CloseOnly { .. }),
        "CTL-200.3"
    );

    // .4 Live without confirmation.
    let mut r = e2e();
    r.send("cli:tom", A, set_mode("CloseOnly"));
    let x = r.send("cli:tom", A, set_mode("Live"));
    want(&x, "NeedsConfirm", None, "CTL-200.4");
    assert_eq!(r.desired(S1, ACCT_A, G1), Some(RunMode::CloseOnly), "CTL-200.4");

    // .5 two-step confirmation.
    let mut r = e2e();
    r.send("cli:tom", A, set_mode("CloseOnly"));
    let (p, c) = r.two_step("cli:tom", A, set_mode("Live"));
    want(&p, "Applied", None, "CTL-200.5 preview");
    let jr = r.jres(p.cmd_id).expect("preview CmdResult");
    assert_eq!(
        jr.rflags & (hft_event::CmdResult::PREVIEW | hft_event::CmdResult::NEEDS_CONFIRM),
        hft_event::CmdResult::PREVIEW | hft_event::CmdResult::NEEDS_CONFIRM,
        "CTL-200.5 rflags {:#x}",
        jr.rflags
    );
    want(&c, "Applied", None, "CTL-200.5 confirm");
    assert_eq!(r.mode(S1, ACCT_A, G1), Effective::Live, "CTL-200.5");

    // .6 the state changed between preview and confirmation.
    let mut r = e2e();
    r.send("cli:tom", A, set_mode("CloseOnly"));
    let p = r.preview("cli:tom", A, set_mode("Live"));
    let k = r.send(
        "cli:tom",
        Some("acct-b"),
        json!({"type":"Kill","level":"Account"}),
    );
    want(&k, "Applied", None, "CTL-200.6 kill");
    let c = r.confirm("cli:tom", A, set_mode("Live"), p.ctl_state_version);
    want(&c, "ConfirmInvalid", Some("StateChanged"), "CTL-200.6");
    assert_eq!(r.desired(S1, ACCT_A, G1), Some(RunMode::CloseOnly), "CTL-200.6");

    // .7 acct-a's start gate not released.
    let mut r = E2e::with(Opts {
        release: vec![ACCT_B, SIM_A],
        ..Opts::default()
    });
    let (p, c) = r.two_step("cli:tom", A, set_mode("Live"));
    want(
        &p,
        "PreconditionFailed",
        Some("GateNotReleased"),
        "CTL-200.7 preview",
    );
    want(
        &c,
        "PreconditionFailed",
        Some("GateNotReleased"),
        "CTL-200.7 confirm",
    );

    // .8 Draining.
    let mut r = e2e();
    r.send("cli:tom", A, set_mode("CloseOnly"));
    r.hold_drain();
    want(
        &r.send("cli:tom", Some("*"), drain()),
        "Applied",
        None,
        "CTL-200.8 drain",
    );
    let (p, c) = r.two_step("cli:tom", A, set_mode("Live"));
    want(&p, "PreconditionFailed", Some("Draining"), "CTL-200.8 preview");
    want(&c, "PreconditionFailed", Some("Draining"), "CTL-200.8 confirm");

    // .9 Draining: a downgrade still applies.
    let mut r = e2e();
    r.hold_drain();
    want(
        &r.send("cli:tom", Some("*"), drain()),
        "Applied",
        None,
        "CTL-200.9 drain",
    );
    let x = r.send("cli:tom", A, set_mode("CloseOnly"));
    want(&x, "Applied", None, "CTL-200.9");
}
