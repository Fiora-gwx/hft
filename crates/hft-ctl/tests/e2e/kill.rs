//! CTL-203.

use hft_event::OrderFlags;
use hft_types::{CapSource, Side};

use super::*;

const A: Option<&str> = Some("acct-a");
const B: Option<&str> = Some("acct-b");

fn kill(level: &str) -> Value {
    json!({"type":"Kill","level":level})
}

fn kill_flatten() -> Value {
    json!({"type":"Kill","level":"Account","flatten":true})
}

/// Quotes that make `probe-taker` (`s2-main`, `g2`) open: leg A's bid 0.18 % above leg B's ask.
fn taker_signal(r: &mut E2e) {
    r.quote(I_BG_FUT, "50100", "50101");
    r.quote(I_BN_SPOT, "50010", "50011");
}

/// Accepted new orders of `inst` among `ds`.
fn placed(ds: &[OrderDecision], inst: InstId) -> Vec<OrderDecision> {
    ds.iter()
        .filter(|d| d.inst == inst && d.action == OrderAction::New && d.reject == 0)
        .copied()
        .collect()
}

fn drain(r: &mut E2e, row: &str) {
    r.hold_drain();
    let x = r.send(
        "cli:tom",
        Some("*"),
        json!({"type":"Drain","cancel_resting":false}),
    );
    want(&x, "Applied", None, row);
}

#[test]
fn ctl_203_kill_codes() {
    // .1 Kill{Account(acct-b)}: (a) acct-b's next opening is refused, acct-a unaffected;
    // (b) the hedge of a first leg filled after the Kill still goes out.
    let mut r = e2e();
    let x = r.send("cli:tom", B, kill("Account"));
    want(&x, "Applied", None, "CTL-203.1");
    assert!(r.has_cap(CapSource::KILL, ACCT_B), "CTL-203.1 cap");
    let m = r.mark();
    taker_signal(&mut r);
    r.settle();
    assert!(
        placed(&r.decisions_since(m), I_BG_FUT).is_empty(),
        "CTL-203.1: acct-b's Open went out"
    );
    assert_eq!(r.mode(S1, ACCT_A, G1), Effective::Live, "CTL-203.1 acct-a");
    let mut r = e2e();
    r.fills = Fills::Silent;
    let m = r.mark();
    taker_signal(&mut r);
    r.settle();
    let first = placed(&r.decisions_since(m), I_BG_FUT);
    assert_eq!(first.len(), 1, "CTL-203.1 first leg");
    let d = first[0];
    let ring = RingId::Rep(0);
    let ack = r.order_event(&d, OrderEventKind::Ack);
    r.push(ring, &ack);
    r.settle();
    // The Kill cancels the first leg; the venue fills it before the cancel lands.
    want(
        &r.send("cli:tom", B, kill("Account")),
        "Applied",
        None,
        "CTL-203.1 kill",
    );
    let m = r.mark();
    let mut f = r.order_event(&d, OrderEventKind::Fill);
    f.cum_filled = d.qty;
    r.push(ring, &f);
    r.fills = Fills::Auto;
    r.settle();
    let hedge = placed(&r.decisions_since(m), I_BN_SPOT);
    assert_eq!(hedge.len(), 1, "CTL-203.1 hedge");
    assert_eq!(hedge[0].side, Side::Buy, "CTL-203.1 hedge");

    // .2 flatten without confirmation.
    let mut r = e2e();
    want(
        &r.send("cli:tom", A, kill_flatten()),
        "NeedsConfirm",
        None,
        "CTL-203.2",
    );
    assert!(!r.has_cap(CapSource::KILL, ACCT_A), "CTL-203.2");

    // .3 flatten, two-step: flatten lock, reducing orders go out.
    let mut r = e2e();
    r.position_g1();
    assert_ne!(r.leg_net(ACCT_A, G1, 1), 0, "CTL-203.3 position");
    let m = r.mark();
    let (p, c) = r.two_step("cli:tom", A, kill_flatten());
    want(&p, "Applied", None, "CTL-203.3 preview");
    want(&c, "Applied", None, "CTL-203.3");
    assert!(r.s.engine().risk.mode.flatten_lock(ACCT_A), "CTL-203.3 lock");
    r.tick_ms(2_000);
    let reducing: Vec<OrderDecision> = r
        .decisions_since(m)
        .into_iter()
        .filter(|d| d.account == ACCT_A && d.action == OrderAction::New && d.reject == 0)
        .collect();
    assert!(!reducing.is_empty(), "CTL-203.3 reducing orders");
    assert!(
        reducing
            .iter()
            .all(|d| d.oflags & OrderFlags::RISK_REDUCING.0 != 0),
        "CTL-203.3 {reducing:?}"
    );

    // .4 Draining: 203.3.
    let mut r = e2e();
    drain(&mut r, "CTL-203.4 drain");
    let (_, c) = r.two_step("cli:tom", A, kill_flatten());
    want(&c, "Applied", None, "CTL-203.4");

    // .5 acct-a's start gate not released: 203.3.
    let mut r = E2e::with(Opts {
        release: vec![ACCT_B, SIM_A],
        ..Opts::default()
    });
    let (_, c) = r.two_step("cli:tom", A, kill_flatten());
    want(&c, "Applied", None, "CTL-203.5");

    // .6 Draining: Kill{Global}.
    let mut r = e2e();
    drain(&mut r, "CTL-203.6 drain");
    want(
        &r.send("cli:tom", Some("*"), kill("Global")),
        "Applied",
        None,
        "CTL-203.6",
    );

    // .7 ops_bot, Kill{Account(acct-a)} without confirmation fields.
    let mut r = e2e();
    want(
        &r.send("ops_bot", A, kill("Account")),
        "Applied",
        None,
        "CTL-203.7",
    );
    assert!(r.has_cap(CapSource::KILL, ACCT_A), "CTL-203.7");

    // .8 no start gate released: Kill{Global}.
    let mut r = E2e::with(Opts {
        release: Vec::new(),
        ..Opts::default()
    });
    want(
        &r.send("cli:tom", Some("*"), kill("Global")),
        "Applied",
        None,
        "CTL-203.8",
    );
}
