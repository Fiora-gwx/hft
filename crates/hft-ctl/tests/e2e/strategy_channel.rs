//! CTL-212.

use hft_strategy_sdk::params::encode_cmd;
use hft_testkit::strategies::probe_target::CMDS;

use super::*;
use crate::common::b64;

const A: Option<&str> = Some("acct-a");

/// `op 1` (`set_thr`, not reduce-only) with thr 7.
pub fn op1() -> Vec<u8> {
    encode_cmd(&CMDS[0], &[7]).unwrap_or_else(|e| panic!("{e:?}"))
}

/// `op 2` (`exit_all`, reduce-only).
pub fn op2() -> Vec<u8> {
    encode_cmd(&CMDS[1], &[]).unwrap_or_else(|e| panic!("{e:?}"))
}

pub fn strategy_cmd(group: &str, ro: bool, payload: &[u8]) -> Value {
    json!({"type":"StrategyCmd","strategy":"s1-main","group":group,"reduce_only":ro,"payload_b64":b64(payload)})
}

fn drain(r: &mut E2e, row: &str) {
    r.hold_drain();
    want(
        &r.send(
            "cli:tom",
            Some("*"),
            json!({"type":"Drain","cancel_resting":false}),
        ),
        "Applied",
        None,
        row,
    );
}

/// `pipeline` injects chunks 0, 1, 2 for `s1-main`; each `Applied`, and s1-main's Model receives
/// them in order.
fn warmup(r: &mut E2e, row: &str) {
    r.start_advisor();
    rec::clear();
    for k in 0..3u32 {
        let x = r.send(
            "pipeline",
            None,
            json!({"type":"InjectWarmup","strategy":"s1-main","local_kind":"3","chunk_no":k.to_string(),
                   "last":k == 2,"payload_b64":b64(&[k as u8; 16])}),
        );
        want(&x, "Applied", None, &format!("{row} chunk {k}"));
    }
    let got: Vec<(u32, bool)> = rec::log()
        .into_iter()
        .filter(|g| g.s == S1 && g.ev == "Warmup" && g.call == "model_on_event")
        .map(|g| (g.n, g.flag))
        .collect();
    assert_eq!(got, vec![(0, false), (1, false), (2, true)], "{row}");
}

#[test]
fn ctl_212_strategy_channel() {
    // .1 op 1 without confirmation.
    let mut r = e2e();
    want(
        &r.send("cli:tom", A, strategy_cmd("16777317", false, &op1())),
        "NeedsConfirm",
        None,
        "CTL-212.1",
    );
    // .2 two-step: Manual in the same Input, same payload, gix of g1.
    rec::clear();
    let (_, c) = r.two_step("cli:tom", A, strategy_cmd("16777317", false, &op1()));
    want(&c, "Applied", None, "CTL-212.2");
    let m: Vec<rec::Got> = rec::log()
        .into_iter()
        .filter(|g| g.call == "on_event" && g.ev == "Manual")
        .collect();
    assert_eq!(m.len(), 1, "CTL-212.2 {m:?}");
    assert_eq!((m[0].s, m[0].gix, m[0].flag), (S1, 0, false), "CTL-212.2");
    assert_eq!(m[0].payload, op1(), "CTL-212.2 payload");

    // .3 op 2 reduce-only without confirmation.
    let mut r = e2e();
    want(
        &r.send("cli:tom", A, strategy_cmd("16777317", true, &op2())),
        "Applied",
        None,
        "CTL-212.3",
    );

    // .4 / .5 Draining.
    let mut r = e2e();
    drain(&mut r, "CTL-212.4 drain");
    want(
        &r.send("cli:tom", A, strategy_cmd("16777317", true, &op2())),
        "Applied",
        None,
        "CTL-212.4",
    );
    let (_, c) = r.two_step("cli:tom", A, strategy_cmd("16777317", false, &op1()));
    want(&c, "PreconditionFailed", Some("Draining"), "CTL-212.5");

    // .6 a group not in s1-main's parameters (counter 999).
    let mut r = e2e();
    let (_, c) = r.two_step("cli:tom", A, strategy_cmd("16778215", false, &op1()));
    want(&c, "InvalidArgs", Some("UnknownGroup"), "CTL-212.6");

    // .7 / .8 acct-a's start gate not released.
    let mut r = E2e::with(Opts {
        release: vec![ACCT_B, SIM_A],
        ..Opts::default()
    });
    want(
        &r.send("cli:tom", A, strategy_cmd("16777317", true, &op2())),
        "Applied",
        None,
        "CTL-212.7",
    );
    let (_, c) = r.two_step("cli:tom", A, strategy_cmd("16777317", false, &op1()));
    want(&c, "PreconditionFailed", Some("GateNotReleased"), "CTL-212.8");

    // .10 / .11 / .12 InjectWarmup.
    let mut r = e2e();
    warmup(&mut r, "CTL-212.10");
    let mut r = e2e();
    drain(&mut r, "CTL-212.11 drain");
    warmup(&mut r, "CTL-212.11");
    let mut r = E2e::with(Opts {
        release: Vec::new(),
        ..Opts::default()
    });
    warmup(&mut r, "CTL-212.12");
}
