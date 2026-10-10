//! CTL-206.

use hft_event::OrderFlags;
use hft_types::Side;

use super::*;

const B: Option<&str> = Some("acct-b");

fn hedge_fix() -> Value {
    json!({"type":"HedgeFix","group":"33554533"})
}

/// `HedgeFix{g2}` without confirmation: Applied, and in the same Input a `RISK_REDUCING` buy of
/// the unhedged 0.001 BTC perpetual.
fn fixed(r: &mut E2e, row: &str) {
    let m = r.mark();
    let x = r.send("cli:tom", B, hedge_fix());
    want(&x, "Applied", None, row);
    let res = r.jres(x.cmd_id).expect("CmdResult");
    let ds: Vec<OrderDecision> = r
        .decisions_since(m)
        .into_iter()
        .filter(|d| d.trigger_seq == res.cmd_seq && d.action == OrderAction::New)
        .collect();
    assert_eq!(ds.len(), 1, "{row}: {ds:?}");
    let d = ds[0];
    assert_eq!(
        (d.inst, d.side, d.qty),
        (I_BG_FUT, Side::Buy, q("0.001")),
        "{row}"
    );
    assert!(d.oflags & OrderFlags::RISK_REDUCING.0 != 0, "{row}");
    assert_eq!(d.reject, 0, "{row}");
}

#[test]
fn ctl_206_hedge_fix() {
    // .1
    let mut r = e2e();
    r.broken_g2();
    fixed(&mut r, "CTL-206.1");
    // .2 Draining.
    let mut r = e2e();
    r.broken_g2();
    r.hold_drain();
    want(
        &r.send(
            "cli:tom",
            Some("*"),
            json!({"type":"Drain","cancel_resting":false}),
        ),
        "Applied",
        None,
        "CTL-206.2 drain",
    );
    fixed(&mut r, "CTL-206.2");
    // .3 acct-b's start gate not released (the gate latches once released, so g2 has nothing to
    // fix): the neutral command is not refused for the gate.
    let mut r = E2e::with(Opts {
        release: vec![ACCT_A, SIM_A],
        ..Opts::default()
    });
    want(
        &r.send("cli:tom", B, hedge_fix()),
        "PreconditionFailed",
        Some("NothingToFix"),
        "CTL-206.3",
    );
}
