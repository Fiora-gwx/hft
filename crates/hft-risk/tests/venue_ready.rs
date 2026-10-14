//! B-6 (docs/tests/scenarios.md §5): a venue that is not ready (an `Unknown` slot) blocks only
//! Opens; hedges, repairs, reductions and engine flattens pass check #4.
//!
//! Mapping to world W1: `VS` ≙ A (spot), `VP` ≙ B (USDT perp), `S1` ≙ a1, `P1` ≙ b1, account
//! `a` ≙ 1, `g1` ≙ G1.

use crate::common;
use common::*;

use hft_risk::{IntentView, Origin};
use hft_strategy_sdk::LinkRole;
use hft_types::{RejectBits, Side, Tif};

/// (A, 1) has an `Unknown` slot of g1 (not ready), (B, 1) is ready; `unknown_blocks_open = false`;
/// g1 holds a1 +0.01 / b1 −0.01.
fn world() -> W {
    let mut w = r0_with(|p| with_acct(&p, 1, |a| a.unknown_blocks_open = false));
    w.mv.not_ready.insert((A.0, 1));
    w.mv.unknown.insert((1, G1.0));
    w.mv.set_pos(1, A1, "0.01");
    w.mv.set_pos(1, B1, "-0.01");
    w
}

#[test]
fn b_6_venue_not_ready_blocks_open_only() {
    let w0 = world();
    let second = hedge(1, G1, A1, Side::Buy, "0.01", "50001.00", "0.01");
    let mut retry = second;
    retry.link_role = Some(LinkRole::SecondRetry);
    let mut fix = hedge_fix(1, G1, A1, Side::Sell, "0.005", "49999.00", "0.005");
    fix.tif = Tif::Ioc;
    let rows: Vec<(&str, IntentView, Result<(), RejectBits>)> = vec![
        (
            "B-6.1",
            open(1, G1, A1, Side::Buy, "0.01", "50001.00", Tif::Ioc),
            err(RejectBits::VENUE_NOT_READY),
        ),
        ("B-6.2", second, Ok(())),
        ("B-6.3", retry, Ok(())),
        ("B-6.4", fix, Ok(())),
        (
            "B-6.5",
            w0.reduce(1, G1, A1, Side::Sell, "0.005", "49999.00"),
            Ok(()),
        ),
        (
            "B-6.6",
            w0.flatten(1, G1, A1, Side::Sell, "0.01", "49999.00", Origin::Exec),
            Ok(()),
        ),
        (
            "B-6.7",
            link_open(
                1,
                G1,
                B1,
                Side::Sell,
                "0.01",
                "50000.0",
                (1, A, A1, Side::Buy, "0.01", "500"),
            ),
            err(RejectBits::VENUE_NOT_READY),
        ),
        (
            "B-6.8",
            open(1, G1, B1, Side::Sell, "0.01", "50000.0", Tif::Gtc),
            Ok(()),
        ),
        ("B-6.9", smoke(1, 1), err(RejectBits::VENUE_NOT_READY)),
    ];
    for (id, it, want) in rows {
        let mut w = world();
        assert_eq!(res(w.eval(&it)), want, "{id}: {it:?}");
    }
    // The reduction and the flatten are classified risk-reducing (not Open) against the position.
    let w = world();
    let red = w.reduce(1, G1, A1, Side::Sell, "0.005", "49999.00");
    assert_ne!(red.class, hft_strategy_sdk::IntentClass::Open, "B-6.5 class");
    let fl = w.flatten(1, G1, A1, Side::Sell, "0.01", "49999.00", Origin::Exec);
    assert_ne!(fl.class, hft_strategy_sdk::IntentClass::Open, "B-6.6 class");
}
