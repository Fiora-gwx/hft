//! CTL-210.

use hft_types::CapSource;

use super::*;

const ALL: Option<&str> = Some("*");
const SIM: Option<&str> = Some("sim-a");

fn delisting() -> Value {
    json!({"type":"VenueNotice","venue":"bitget-spot","inst":"bitget-spot:BTCUSDT","kind":"Delisting",
           "at_ns":(UTC0 + 86_400_000_000_000).to_string()})
}

fn sim_usdt(r: &E2e, v: VenueId) -> i64 {
    r.s.engine().ledger.cash_view(SIM_A, v, USDT).free.0
}

fn delisted(r: &mut E2e, row: &str) {
    rec::clear();
    let x = r.send("cli:tom", ALL, delisting());
    want(&x, "Applied", None, row);
    let caps: Vec<_> =
        r.s.engine()
            .risk
            .mode
            .caps()
            .iter()
            .filter(|c| c.source == CapSource::DELISTING)
            .map(|c| (c.scope.venue, c.scope.inst))
            .collect();
    assert_eq!(caps, vec![(V_BG_SPOT, I_BG_SPOT)], "{row}");
    let got: Vec<StrategyId> = rec::log()
        .into_iter()
        .filter(|g| g.call == "on_event" && g.ev == "Notice")
        .map(|g| g.s)
        .collect();
    assert!(got.contains(&S1) && got.contains(&S9), "{row}: {got:?}");
    assert!(!got.contains(&S2), "{row}: {got:?}");
}

#[test]
fn ctl_210_misc_codes() {
    // .1 / .2 Delisting of bitget-spot:BTCUSDT (s1-main and s9-sim subscribe it, s2-main does
    // not).
    let mut r = e2e();
    delisted(&mut r, "CTL-210.1");
    let mut r = e2e();
    r.hold_drain();
    want(
        &r.send("cli:tom", ALL, json!({"type":"Drain","cancel_resting":false})),
        "Applied",
        None,
        "CTL-210.2 drain",
    );
    delisted(&mut r, "CTL-210.2");

    // .3 / .4 FundingInterval and Maintenance.
    let mut r = e2e();
    let x = r.send(
        "cli:tom",
        ALL,
        json!({"type":"VenueNotice","venue":"binance-usdm","kind":"FundingInterval","new_s":"14400"}),
    );
    want(&x, "Applied", None, "CTL-210.3");
    let from = UTC0 + 3_600_000_000_000;
    let x = r.send(
        "cli:tom",
        ALL,
        json!({"type":"VenueNotice","venue":"binance-usdm","kind":"Maintenance",
               "from_ns":from.to_string(),"to_ns":(from + 1_800_000_000_000).to_string()}),
    );
    want(&x, "Applied", None, "CTL-210.4");

    // .5 sim-a idle (CloseOnly, nothing in flight): SimReset restores the initial balances.
    let mut r = e2e();
    want(
        &r.send(
            "cli:tom",
            SIM,
            json!({"type":"SetMode","strategy":"s9-sim","mode":"CloseOnly"}),
        ),
        "Applied",
        None,
        "CTL-210.5 mode",
    );
    want(
        &r.send(
            "cli:tom",
            SIM,
            json!({"type":"SimTransfer","from_venue":"binance-usdm","to_venue":"bitget-spot","asset":"USDT","amount":"100"}),
        ),
        "Applied",
        None,
        "CTL-210.5 transfer",
    );
    let (_, c) = r.two_step("cli:tom", SIM, json!({"type":"SimReset"}));
    want(&c, "Applied", None, "CTL-210.5");
    assert_eq!(sim_usdt(&r, V_BN_USDM), 1_000_000_000, "CTL-210.5 balance");
    assert_eq!(sim_usdt(&r, V_BG_SPOT), 1_000_000_000, "CTL-210.5 balance");
    assert_eq!(
        r.s.engine().ledger.pos(SIM_A, I_BN_USDM).qty.0,
        0,
        "CTL-210.5 position"
    );

    // .6 a Live cell of sim-a.
    let mut r = e2e();
    let (_, c) = r.two_step("cli:tom", SIM, json!({"type":"SimReset"}));
    want(&c, "PreconditionFailed", Some("SimNotIdle"), "CTL-210.6");

    // .7 a non-terminal IOC slot of sim-a (s9-sim's TakerSlice point, the order unanswered;
    // then CloseOnly).
    let mut o = Opts::default();
    o.params[2].values[tf::T_SLICE_MS] = T_SLICE_MS;
    let mut r = E2e::with(o);
    r.fills = Fills::Silent;
    let m = r.mark();
    while !r
        .decisions_since(m)
        .iter()
        .any(|d| d.account == SIM_A && d.action == OrderAction::New && d.reject == 0)
    {
        r.tick_ms(500);
        assert!(
            r.s.engine().now.engine_ns < 200_000_000_000,
            "CTL-210.7: no sim order"
        );
    }
    want(
        &r.send(
            "cli:tom",
            SIM,
            json!({"type":"SetMode","strategy":"s9-sim","mode":"CloseOnly"}),
        ),
        "Applied",
        None,
        "CTL-210.7 mode",
    );
    let (_, c) = r.two_step("cli:tom", SIM, json!({"type":"SimReset"}));
    want(&c, "PreconditionFailed", Some("SimNotIdle"), "CTL-210.7");

    // .8 without confirmation.
    let mut r = e2e();
    want(
        &r.send("cli:tom", SIM, json!({"type":"SimReset"})),
        "NeedsConfirm",
        None,
        "CTL-210.8",
    );

    // .9 / .10 SimTransfer.
    let mut r = e2e();
    let t = |amount: &str| json!({"type":"SimTransfer","from_venue":"binance-usdm","to_venue":"bitget-spot","asset":"USDT","amount":amount});
    want(&r.send("cli:tom", SIM, t("100")), "Applied", None, "CTL-210.9");
    assert_eq!(sim_usdt(&r, V_BN_USDM), 900_000_000, "CTL-210.9");
    let mut r = e2e();
    want(
        &r.send("cli:tom", SIM, t("1000.000001")),
        "PreconditionFailed",
        Some("InsufficientBalance"),
        "CTL-210.10",
    );

    // .11 RecordTransfer.
    let mut r = e2e();
    let x = r.send(
        "cli:tom",
        Some("acct-a"),
        json!({"type":"RecordTransfer","from":"binance-usdm","to":"bitget-spot","asset":"USDT","amount":"100","ext_ref":"TX123"}),
    );
    want(&x, "Applied", None, "CTL-210.11");

    // .12 / .13 Annotate: no mode, cap or account state changes (only the de-duplication table
    // records the cmd_id).
    let mut r = e2e();
    let state = |r: &E2e| {
        let e = r.s.engine();
        (
            e.risk.mode.ctl_state_version(),
            e.risk.mode.caps().to_vec(),
            [ACCT_A, ACCT_B, SIM_A].map(|a| e.ledger.equity(a).total),
        )
    };
    let before = state(&r);
    rec::clear();
    let x = r.send(
        "tg:1001",
        ALL,
        json!({"type":"Annotate","kind":"OpsNote","text":"x"}),
    );
    want(&x, "Applied", None, "CTL-210.12");
    assert!(before == state(&r), "CTL-210.12 state");
    assert!(rec::log().is_empty(), "CTL-210.12 strategy calls");
    let x = r.send(
        "cli:tom",
        ALL,
        json!({"type":"Annotate","kind":"Incident","text":"y"}),
    );
    want(&x, "Applied", None, "CTL-210.13");
}
