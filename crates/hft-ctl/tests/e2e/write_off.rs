//! CTL-207.

use hft_types::CapSource;

use super::*;

const B: Option<&str> = Some("acct-b");

/// `Note{Ledger, ResidualWrittenOff}`.
const NOTE_WRITTEN_OFF: u32 = 0x0900_0006;

fn write_off(group: &str) -> Value {
    json!({"type":"WriteOffResidual","group":group})
}

/// `g2` quiescent with 0.0004 BTC of the perpetual unhedged (below the lot 0.001, above the dust
/// line): the first leg fills 0.001, the hedge fills 0.0006 and expires; 5 s later.
fn residual_g2(r: &mut E2e) {
    let d = r.taker_first_leg();
    let f = r.fills;
    r.fills = Fills::Silent;
    let m = r.mark();
    r.fill_at(&d, "50100");
    r.settle();
    let h = r.new_orders_since(m, I_BN_SPOT)[0];
    let ring = RingId::Rep(0);
    let ack = r.order_event(&h, OrderEventKind::Ack);
    r.push(ring, &ack);
    let mut pf = r.order_event(&h, OrderEventKind::PartialFill);
    pf.qty = q("0.0006");
    pf.cum_filled = q("0.0006");
    r.push(ring, &pf);
    let mut x = r.order_event(&h, OrderEventKind::Expired);
    x.cum_filled = q("0.0006");
    r.push(ring, &x);
    r.settle();
    // The link's retry of the unhedged remainder (below the lot) comes to nothing; quiescent.
    r.tick_ms(5_000);
    r.fills = f;
    let lv = r.s.engine().oms.legs(ACCT_B, G2).view;
    assert_eq!(lv.naked_qty, q("0.0004"), "residual_g2");
}

fn legs(r: &E2e) -> Vec<(i64, i64)> {
    let lv = r.s.engine().oms.legs(ACCT_B, G2).view;
    lv.leg[..2].iter().map(|l| (l.net.0, l.residual.0)).collect()
}

#[test]
fn ctl_207_write_off_codes() {
    // .1 without confirmation.
    let mut r = e2e();
    residual_g2(&mut r);
    want(
        &r.send("cli:alice", B, write_off("33554533")),
        "NeedsConfirm",
        None,
        "CTL-207.1",
    );

    // .2 two-step: the 0.0004 BTC leaves the group, every residual is 0, no order.
    let m = r.mark();
    let (p, c) = r.two_step("cli:alice", B, write_off("33554533"));
    want(&p, "Applied", None, "CTL-207.2 preview");
    want(&c, "Applied", None, "CTL-207.2");
    assert_eq!(legs(&r), vec![(-60_000, 0), (60_000, 0)], "CTL-207.2 legs");
    assert_eq!(r.s.engine().oms.legs(ACCT_B, G2).view.naked_qty.0, 0, "CTL-207.2");
    assert!(
        r.notes_since(m).iter().any(|n| n.code == NOTE_WRITTEN_OFF),
        "CTL-207.2 note"
    );
    assert!(r.decisions_since(m).is_empty(), "CTL-207.2 orders");

    // .3 g2 Broken: Broken clears in the same Input, HedgeFailure stays.
    let mut r = e2e();
    r.broken_g2();
    let (_, c) = r.two_step("cli:alice", B, write_off("33554533"));
    want(&c, "Applied", None, "CTL-207.3");
    assert!(!r.has_cap(CapSource::BROKEN, ACCT_B), "CTL-207.3 Broken");
    assert!(
        r.has_cap(CapSource::HEDGE_FAILURE, ACCT_B),
        "CTL-207.3 HedgeFailure"
    );

    // .4 an order in flight on g2.
    let mut r = e2e();
    let _ = r.taker_first_leg();
    let (_, c) = r.two_step("cli:alice", B, write_off("33554533"));
    want(&c, "PreconditionFailed", Some("NotQuiescent"), "CTL-207.4");

    // .5 a group of s2-main not in its parameters (counter 999).
    let mut r = e2e();
    let (_, c) = r.two_step("cli:alice", B, write_off("33555431"));
    want(&c, "InvalidArgs", Some("UnknownGroup"), "CTL-207.5");

    // .6 Draining.
    let mut r = e2e();
    residual_g2(&mut r);
    r.hold_drain();
    want(
        &r.send(
            "cli:alice",
            Some("*"),
            json!({"type":"Drain","cancel_resting":false}),
        ),
        "Applied",
        None,
        "CTL-207.6 drain",
    );
    let (_, c) = r.two_step("cli:alice", B, write_off("33554533"));
    want(&c, "PreconditionFailed", Some("Draining"), "CTL-207.6");

    // .7 acct-b's start gate not released.
    let mut r = E2e::with(Opts {
        release: vec![ACCT_A, SIM_A],
        ..Opts::default()
    });
    let (_, c) = r.two_step("cli:alice", B, write_off("33554533"));
    want(&c, "PreconditionFailed", Some("GateNotReleased"), "CTL-207.7");

    // .8 both legs hedged (no extra leg): Applied, nothing moves.
    let mut r = e2e();
    r.taker_signal();
    r.settle();
    let before = legs(&r);
    assert_eq!(before, vec![(-100_000, 0), (100_000, 0)], "CTL-207.8 hedged");
    let (_, c) = r.two_step("cli:alice", B, write_off("33554533"));
    want(&c, "Applied", None, "CTL-207.8");
    assert_eq!(legs(&r), before, "CTL-207.8 legs");

    // .9 the Sim account.
    let mut r = e2e();
    let (_, c) = r.two_step("cli:alice", Some("sim-a"), write_off("50331749"));
    want(&c, "Applied", None, "CTL-207.9");

    // .10 Trader.
    let mut r = e2e();
    want(
        &r.send("cli:tom", B, write_off("33554533")),
        "Forbidden",
        None,
        "CTL-207.10",
    );
}
