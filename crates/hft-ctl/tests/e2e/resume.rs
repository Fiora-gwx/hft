//! CTL-204.

use hft_types::CapSource;

use super::*;

const A: Option<&str> = Some("acct-a");
const B: Option<&str> = Some("acct-b");
const ALL: Option<&str> = Some("*");

fn resume(source: &str) -> Value {
    json!({"type":"Resume","source":source})
}

fn kill(level: &str) -> Value {
    json!({"type":"Kill","level":level})
}

/// Two-step `Resume`; asserts preview and confirmation give the same code.
#[track_caller]
fn two_step_want(
    r: &mut E2e,
    issuer: &str,
    acct: Option<&str>,
    cmd: Value,
    code: &str,
    det: Option<&str>,
    row: &str,
) {
    let (p, c) = r.two_step(issuer, acct, cmd);
    want(&p, code, det, &format!("{row} preview"));
    want(&c, code, det, row);
}

/// A reconcile group of `(acct-a, bitget-spot)` from `reconciler`: a BTC balance difference of
/// 0.01 (500 USDT, above `recon_closeonly_notional` = 100 USDT) and the summary.
fn recon_diff(r: &mut E2e, report_id: u64) -> CmdResponse {
    let items = json!([
        {"venue":"bitget-spot","account":"acct-a","item":"Balance","asset":"BTC","engine_qty":"0","exch_qty":"0.01","diff":"500","severity":"Warn"},
        {"venue":"bitget-spot","account":"acct-a","item":"Summary"}
    ]);
    r.send(
        "reconciler",
        None,
        json!({"type":"ReconcileReport","report_id":report_id.to_string(),"items":items}),
    )
}

#[test]
fn ctl_204_resume_codes() {
    // .1 / .2 acct-a's Kill cap.
    let mut r = e2e();
    want(
        &r.send("cli:tom", A, kill("Account")),
        "Applied",
        None,
        "CTL-204.1 kill",
    );
    want(
        &r.send("cli:tom", A, resume("Kill")),
        "NeedsConfirm",
        None,
        "CTL-204.1",
    );
    two_step_want(&mut r, "cli:tom", A, resume("Kill"), "Applied", None, "CTL-204.2");
    assert!(!r.has_cap(CapSource::KILL, ACCT_A), "CTL-204.2");

    // .3 no Kill cap on acct-b.
    let mut r = e2e();
    two_step_want(
        &mut r,
        "cli:tom",
        B,
        resume("Kill"),
        "PreconditionFailed",
        Some("NoMatchingCap"),
        "CTL-204.3",
    );

    // .4 Kill{Global}; Resume{Kill} for every account.
    let mut r = e2e();
    want(
        &r.send("cli:tom", ALL, kill("Global")),
        "Applied",
        None,
        "CTL-204.4 kill",
    );
    two_step_want(
        &mut r,
        "cli:tom",
        ALL,
        resume("Kill"),
        "Applied",
        None,
        "CTL-204.4",
    );
    assert_eq!(r.caps_of(CapSource::KILL), 0, "CTL-204.4");

    // .5 / .6 not resumable.
    let mut r = e2e();
    for (row, src) in [("CTL-204.5", "ClockBad"), ("CTL-204.6", "Draining")] {
        two_step_want(
            &mut r,
            "cli:tom",
            ALL,
            resume(src),
            "InvalidArgs",
            Some("NotResumable"),
            row,
        );
    }

    // .7 / .8 g2 Broken with HedgeFailure on acct-b.
    let mut r = e2e();
    r.broken_g2();
    assert!(
        r.has_cap(CapSource::HEDGE_FAILURE, ACCT_B),
        "CTL-204.7 HedgeFailure"
    );
    two_step_want(
        &mut r,
        "cli:tom",
        B,
        resume("HedgeFailure"),
        "Rejected",
        Some("BrokenRemaining"),
        "CTL-204.7",
    );
    two_step_want(
        &mut r,
        "cli:tom",
        B,
        json!({"type":"Resume","source":"Broken","group":"33554533"}),
        "PreconditionFailed",
        Some("NakedNotCleared"),
        "CTL-204.8",
    );

    // .9 DailyLoss after one resume today: acct-b's line 5 USDT; probe-taker exits on Risk
    // events (`close_on = Risk`), a Kill makes it exit, and the venue fills the perpetual
    // buy-back at 60 000 (a realized loss of 9.9 USDT a round).
    let mut o = Opts {
        risk: RiskDoc {
            daily_loss_abs_b: 5,
            ..RiskDoc::start()
        },
        ..Opts::default()
    };
    o.params[1].values[kf::CLOSE_ON] = 4;
    let mut r = E2e::with(o);
    r.px_override.push((I_BG_FUT, hft_types::Side::Buy, px("60000")));
    let round = |r: &mut E2e, n: &str| {
        r.taker_signal();
        r.settle();
        r.quotes();
        r.settle();
        want(&r.send("cli:tom", B, kill("Account")), "Applied", None, n);
        r.tick_ms(2_000);
        assert!(r.has_cap(CapSource::DAILY_LOSS, ACCT_B), "{n}: no DailyLoss");
        let (_, c) = r.two_step("cli:tom", B, resume("Kill"));
        want(&c, "Applied", None, n);
    };
    round(&mut r, "CTL-204.9 first breach");
    two_step_want(
        &mut r,
        "cli:tom",
        B,
        resume("DailyLoss"),
        "Applied",
        None,
        "CTL-204.9 resume 1",
    );
    round(&mut r, "CTL-204.9 second breach");
    two_step_want(
        &mut r,
        "cli:tom",
        B,
        resume("DailyLoss"),
        "PreconditionFailed",
        Some("ResumeLimit"),
        "CTL-204.9",
    );

    // .10 Recon(acct-a, bitget-spot) (set by the second round with the same difference), the
    // last round still differs.
    let mut r = e2e();
    want(&recon_diff(&mut r, 9), "Applied", None, "CTL-204.10 report");
    want(&recon_diff(&mut r, 10), "Applied", None, "CTL-204.10 report");
    assert!(r.has_cap(CapSource::RECON, ACCT_A), "CTL-204.10 Recon cap");
    two_step_want(
        &mut r,
        "cli:tom",
        A,
        json!({"type":"Resume","source":"Recon","venue":"bitget-spot"}),
        "PreconditionFailed",
        Some("ReconNotClean"),
        "CTL-204.10",
    );

    // .11 UnknownTimeout(acct-a, bitget-spot) with the Unknown slot still there.
    let mut r = e2e();
    r.fills = Fills::Silent;
    let v = r.version();
    let smoke = ControlCmd::SmokeOrder {
        account: ACCT_A,
        venue: V_BG_SPOT,
        _pad: 0,
        inst: I_BG_SPOT,
        side: hft_types::Side::Buy,
        _pad2: [0; 3],
        ttl_ms: 600_000,
    };
    r.direct("cli:alice", CmdHdr::ROLE_ADMIN, ACCT_A, &smoke, |h| {
        h.confirmed = 1;
        h.expect_state_version = v;
    });
    r.settle();
    r.tick_ms(45_000);
    assert!(r.has_cap(CapSource::UNKNOWN_TIMEOUT, ACCT_A), "CTL-204.11 cap");
    two_step_want(
        &mut r,
        "cli:tom",
        A,
        json!({"type":"Resume","source":"UnknownTimeout","venue":"bitget-spot"}),
        "PreconditionFailed",
        Some("UnknownPresent"),
        "CTL-204.11",
    );

    // .12 Kill cap, then Draining.
    let mut r = e2e();
    want(
        &r.send("cli:tom", A, kill("Account")),
        "Applied",
        None,
        "CTL-204.12 kill",
    );
    r.hold_drain();
    want(
        &r.send("cli:tom", ALL, json!({"type":"Drain","cancel_resting":false})),
        "Applied",
        None,
        "CTL-204.12 drain",
    );
    two_step_want(
        &mut r,
        "cli:tom",
        A,
        resume("Kill"),
        "PreconditionFailed",
        Some("Draining"),
        "CTL-204.12",
    );

    // .13 acct-a's start gate not released.
    let mut r = E2e::with(Opts {
        release: vec![ACCT_B, SIM_A],
        ..Opts::default()
    });
    two_step_want(
        &mut r,
        "cli:tom",
        A,
        resume("Recovery"),
        "PreconditionFailed",
        Some("GateNotReleased"),
        "CTL-204.13",
    );

    // .14 HARD Kill of acct-a, then a reconcile round with a non-Ignore difference.
    let mut r = e2e();
    want(
        &r.send("cli:tom", A, json!({"type":"Kill","level":"Account","hard":true})),
        "Applied",
        None,
        "CTL-204.14 kill",
    );
    want(&recon_diff(&mut r, 10), "Applied", None, "CTL-204.14 report");
    two_step_want(
        &mut r,
        "cli:tom",
        A,
        resume("Kill"),
        "PreconditionFailed",
        Some("ReconNotClean"),
        "CTL-204.14",
    );

    // .15 Viewer: the gateway refuses; nothing reaches the ring.
    let mut r = e2e();
    want(
        &r.send("cli:tom", A, kill("Account")),
        "Applied",
        None,
        "CTL-204.15 kill",
    );
    let m = r.mark();
    let x = r.send("tg:1001", A, resume("Kill"));
    want(&x, "Forbidden", None, "CTL-204.15");
    assert!(
        r.kinds_since(m, EventKind::CONTROL_CMD).is_empty(),
        "CTL-204.15 written"
    );
}
