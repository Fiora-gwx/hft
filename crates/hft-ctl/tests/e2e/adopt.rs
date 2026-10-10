//! CTL-209.

use super::*;

const A: Option<&str> = Some("acct-a");

fn adopt() -> Value {
    json!({"type":"AdoptExchange","venue":"binance-usdm","report_id":"7","item":"Position","inst":"binance-usdm:BTCUSDT"})
}

fn baseline() -> Value {
    json!({"type":"SetBalanceBaseline","venue":"bitget-spot"})
}

/// `report_id 7`: acct-a's `binance-usdm:BTCUSDT` position on the exchange is 0.01 BTC above
/// the engine's, `view_seq` = `view_seq` (after a mark price of 50 000, the adoption price).
fn report7(r: &mut E2e, view_seq: u64) {
    r.mark_px(I_BN_USDM, "50000");
    r.settle();
    let engine = r.s.engine().ledger.pos(ACCT_A, I_BN_USDM).qty;
    let exch = hft_types::Qty(engine.0 + q("0.01").0);
    let fmt = |x: hft_types::Qty| {
        let neg = x.0 < 0;
        let a = x.0.unsigned_abs();
        format!(
            "{}{}.{:08}",
            if neg { "-" } else { "" },
            a / 100_000_000,
            a % 100_000_000
        )
    };
    let items = json!([
        {"venue":"binance-usdm","account":"acct-a","item":"Position","inst":"binance-usdm:BTCUSDT",
         "engine_qty":fmt(engine),"exch_qty":fmt(exch),"diff":"500","severity":"Warn","view_seq":view_seq.to_string()},
        {"venue":"binance-usdm","account":"acct-a","item":"Summary","view_seq":view_seq.to_string()}
    ]);
    want(&r.report(7, items), "Applied", None, "report 7");
}

#[test]
fn ctl_209_adopt_and_baseline_codes() {
    // .1 / .2 adopt the +0.01 BTC.
    let mut r = e2e();
    let v = r.last_seq();
    report7(&mut r, v);
    want(&r.send("cli:tom", A, adopt()), "NeedsConfirm", None, "CTL-209.1");
    let before = r.s.engine().ledger.pos(ACCT_A, I_BN_USDM).qty;
    let (_, c) = r.two_step("cli:tom", A, adopt());
    want(&c, "Applied", None, "CTL-209.2");
    let after = r.s.engine().ledger.pos(ACCT_A, I_BN_USDM).qty;
    assert_eq!(after.0 - before.0, q("0.01").0, "CTL-209.2 ledger");

    // .3 a fill of the inst after view_seq.
    let mut r = e2e();
    let v = r.last_seq();
    r.position_g1();
    report7(&mut r, v);
    let (_, c) = r.two_step("cli:tom", A, adopt());
    want(&c, "PreconditionFailed", Some("StaleReport"), "CTL-209.3");

    // .4 the Sim account.
    let mut r = e2e();
    let v = r.last_seq();
    report7(&mut r, v);
    let (_, c) = r.two_step("cli:tom", Some("sim-a"), adopt());
    want(&c, "InvalidArgs", Some("NotLive"), "CTL-209.4");

    // .5 Draining.
    let mut r = e2e();
    let v = r.last_seq();
    report7(&mut r, v);
    r.hold_drain();
    want(
        &r.send(
            "cli:tom",
            Some("*"),
            json!({"type":"Drain","cancel_resting":false}),
        ),
        "Applied",
        None,
        "CTL-209.5 drain",
    );
    let (_, c) = r.two_step("cli:tom", A, adopt());
    want(&c, "PreconditionFailed", Some("Draining"), "CTL-209.5");

    // .6 / .9 SetBalanceBaseline.
    let mut r = e2e();
    want(
        &r.send("cli:tom", A, baseline()),
        "NeedsConfirm",
        None,
        "CTL-209.9",
    );
    let (_, c) = r.two_step("cli:tom", A, baseline());
    want(&c, "Applied", None, "CTL-209.6");

    // .7 no balance view of (acct-a, bitget-spot).
    let mut r = E2e::with(Opts {
        no_balance: vec![(ACCT_A, V_BG_SPOT)],
        ..Opts::default()
    });
    let (_, c) = r.two_step("cli:tom", A, baseline());
    want(&c, "PreconditionFailed", Some("NoBalanceView"), "CTL-209.7");

    // .8 the Sim account.
    let mut r = e2e();
    let (_, c) = r.two_step("cli:tom", Some("sim-a"), baseline());
    want(&c, "InvalidArgs", Some("NotLive"), "CTL-209.8");
}
