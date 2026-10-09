//! Check-chain cases (RSK-020–RSK-029, RSK-031–RSK-035, RSK-041–RSK-043, RSK-240).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)] // allow-list: test code, not a hot path

mod common;
use common::*;

use hft_event::OrderFlags;
use hft_refdata::InstStatus;
use hft_risk::mode::{ModeInput, cfg_params};
use hft_risk::{AccountScope, EquityView, IntentView, Origin, Risk, Scope, acr_rflags};
use hft_strategy_sdk::MarginLevel;
use hft_types::{CapSource, GROUP_ALL, Notional, Qty, Rate, RejectBits, RunMode, Side, StrategyId, Tif, Tsc};

fn pass(w: &mut W, it: &IntentView, id: &str) {
    let r = w.eval(it);
    assert!(r.is_ok(), "{id}: {r:?}");
}

fn rej(w: &mut W, it: &IntentView, b: RejectBits, id: &str) {
    let r = w.eval(it);
    assert_eq!(r.map(|_| ()), Err(b), "{id}");
}

fn open_a1() -> IntentView {
    open(1, G1, A1, Side::Buy, "0.01", "50000.00", Tif::Gtc)
}
fn open_b1() -> IntentView {
    open(1, G1, B1, Side::Buy, "0.01", "50000.0", Tif::Gtc)
}
fn open_g3_a2() -> IntentView {
    open(1, G3, A2, Side::Buy, "0.1", "2500.00", Tif::Gtc)
}
fn open_2() -> IntentView {
    open(2, G2, A1, Side::Buy, "0.01", "50000.00", Tif::Gtc)
}
fn hedge_b1() -> IntentView {
    hedge(1, G1, B1, Side::Sell, "0.01", "49995.0", "0.01")
}
/// `LinkOpen(1, g1, a1, Buy, 0.01 @ 50 000.00; link = (1, B, b1, Sell, 0.01, 495))`.
fn link_l() -> IntentView {
    link_open(
        1,
        G1,
        A1,
        Side::Buy,
        "0.01",
        "50000.00",
        (1, B, B1, Side::Sell, "0.01", "495"),
    )
}
fn link_l2() -> IntentView {
    link_open(
        2,
        G2,
        A1,
        Side::Buy,
        "0.01",
        "50000.00",
        (1, B, B1, Side::Sell, "0.01", "495"),
    )
}
/// Simultaneous second leg on b1 linked to a1.
fn link_sim() -> IntentView {
    link_open(
        1,
        G1,
        B1,
        Side::Sell,
        "0.01",
        "49995.0",
        (1, A, A1, Side::Buy, "0.01", "505"),
    )
}

/// The three risk-reducing intents of RSK-022/023/025 (b1 position −0.02).
fn rr_three(w: &W) -> [IntentView; 3] {
    [
        hedge_b1(),
        w.reduce(1, G1, B1, Side::Buy, "0.01", "50005.0"),
        w.flatten(1, G1, B1, Side::Buy, "0.01", "50005.0", Origin::Exec),
    ]
}

fn short_b1(w: &mut W) {
    w.mv.set_pos(1, B1, "-0.02");
}

fn acfg(a: u8, v: hft_types::VenueId) -> ModeInput<'static> {
    ModeInput::Auto(rec(
        CapSource::ACCOUNT_CFG,
        0,
        2,
        sc_av(a, v),
        cfg_params::MISMATCH,
    ))
}

#[test]
fn rsk_020_single_open_passes() {
    let mut w = r0();
    let it = open_a1();
    let before = section17(&w.snapshot());
    let a = w.eval(&it);
    let b = w.eval(&it);
    assert!(a.is_ok());
    assert_eq!(a, b);
    let now = w.nowv();
    w.risk.commit(&b.unwrap(), &now);
    let after = section17(&w.snapshot());
    let t = 1_000_000_000;
    let (vb0, vb1) = (
        bucket_row(&before, 1, A, 0).unwrap(),
        bucket_row(&after, 1, A, 0).unwrap(),
    );
    assert_eq!(vb1.tokens_10s, vb0.tokens_10s - t);
    assert_eq!(vb1.tokens_min, vb0.tokens_min - t);
    let (ib0, ib1) = (
        bucket_row(&before, 1, A, A1.0).unwrap(),
        bucket_row(&after, 1, A, A1.0).unwrap(),
    );
    assert_eq!(ib1.tokens_10s, ib0.tokens_10s - t);
    // Every other bucket row is unchanged.
    for r in before.iter().filter(|r| r.h.kind == 2) {
        if r.h.account == 1 && r.h.venue == A.0 && (r.h.inst == 0 || r.h.inst == A1.0) {
            continue;
        }
        let o = after.iter().find(|x| x.h == r.h).unwrap();
        assert_eq!(o.body, r.body, "bucket {:?} changed", r.h);
    }
}

#[test]
fn rsk_021_structure_and_binding() {
    let mut w = r0();
    let mut it = open_a1();
    it.qty = Qty(0);
    it.notional = Notional(0);
    rej(&mut w, &it, RejectBits::INVALID, "RSK-021.1");
    let it = open(1, G1, A1, Side::Buy, "0.00015", "50000.00", Tif::Gtc);
    rej(&mut w, &it, RejectBits::INVALID, "RSK-021.2");
    let it = open(1, G1, B1, Side::Buy, "0.01", "50000.05", Tif::Gtc);
    rej(&mut w, &it, RejectBits::INVALID, "RSK-021.3");
    {
        let mut w = r0_status(A1, InstStatus::ReduceOnly);
        rej(&mut w, &open_a1(), RejectBits::NOT_TRADABLE, "RSK-021.4");
        w.mv.set_pos(1, A1, "0.01");
        let it = w.reduce(1, G1, A1, Side::Sell, "0.01", "49999.00");
        pass(&mut w, &it, "RSK-021.5");
    }
    {
        let mut w = r0_status(A1, InstStatus::Halted);
        w.mv.set_pos(1, A1, "0.01");
        let it = w.flatten(1, G1, A1, Side::Sell, "0.01", "49999.00", Origin::Exec);
        rej(&mut w, &it, RejectBits::NOT_TRADABLE, "RSK-021.6");
    }
    {
        let mut w = r0_status(A1, InstStatus::Delisted);
        rej(&mut w, &open_a1(), RejectBits::NOT_TRADABLE, "RSK-021.7");
    }
    let mut it = open_a1();
    it.strategy = StrategyId(2);
    rej(&mut w, &it, RejectBits::NOT_BOUND, "RSK-021.8");
    let it = hedge_fix(1, G1, A1, Side::Buy, "0.01", "50001.00", "0.01");
    pass(&mut w, &it, "RSK-021.9");
    let it = hedge_fix(1, G2, A1, Side::Buy, "0.01", "50001.00", "0.01");
    rej(&mut w, &it, RejectBits::NOT_BOUND, "RSK-021.10");
    pass(&mut w, &smoke(1, 1), "RSK-021.11");
    rej(&mut w, &smoke(3, 3), RejectBits::INVALID, "RSK-021.12");
    let mut it = open_a1();
    it.qty = Qty(0);
    it.strategy = StrategyId(2);
    rej(&mut w, &it, RejectBits::INVALID, "RSK-021.13");
    w.ok(kill_global(true));
    let mut it = open_a1();
    it.qty = Qty(0);
    rej(&mut w, &it, RejectBits::INVALID, "RSK-021.14");
}

#[test]
fn rsk_022_hard_blocks_all() {
    {
        let mut w = r0();
        short_b1(&mut w);
        w.ok(kill_global(true));
        let mut all = vec![open_b1()];
        all.extend(rr_three(&w));
        for (k, it) in all.iter().enumerate() {
            rej(&mut w, it, RejectBits::HARD, &format!("RSK-022.1 sub {k}"));
        }
        w.mv.set_pos(3, B1, "-0.02");
        let it = w.flatten(3, GS, B1, Side::Buy, "0.01", "50005.0", Origin::Exec);
        rej(&mut w, &it, RejectBits::HARD, "RSK-022.6");
    }
    {
        let mut w = r0();
        short_b1(&mut w);
        w.ok(kill_acct(1, true));
        let it = w.flatten(1, G1, B1, Side::Buy, "0.01", "50005.0", Origin::Exec);
        rej(&mut w, &it, RejectBits::HARD, "RSK-022.2");
        pass(&mut w, &open_2(), "RSK-022.3");
        rej(&mut w, &link_l2(), RejectBits::HARD, "RSK-022.4");
    }
    {
        let mut w = r0();
        short_b1(&mut w);
        w.ok(kill_acct(1, false));
        let it = w.flatten(1, G1, B1, Side::Buy, "0.01", "50005.0", Origin::Exec);
        pass(&mut w, &it, "RSK-022.5");
    }
}

#[test]
fn rsk_023_clock_bad_blocks_open() {
    let mut w = r0();
    short_b1(&mut w);
    w.clock_bad();
    rej(&mut w, &open_a1(), RejectBits::CLOCK, "RSK-023.1");
    let mut four = rr_three(&w).to_vec();
    four.push(w.flatten(1, G1, B1, Side::Buy, "0.01", "50005.0", Origin::Strategy));
    for (k, it) in four.iter().enumerate() {
        pass(&mut w, it, &format!("RSK-023.2 sub {k}"));
    }
    {
        let mut w2 = r0();
        w2.clock_bad();
        w2.ok(kill_acct(1, false));
        rej(&mut w2, &open_a1(), RejectBits::CLOCK, "RSK-023.3");
    }
    w.ok(ModeInput::Clock { ok: true });
    pass(&mut w, &open_a1(), "RSK-023.4");
}

#[test]
fn rsk_024_cap_reject_bits() {
    use RejectBits as R;
    let case = |id: &str, set: &dyn Fn(&mut W), checks: &[(IntentView, Option<RejectBits>)]| {
        let mut w = r0();
        set(&mut w);
        for (k, (it, want)) in checks.iter().enumerate() {
            match want {
                Some(b) => rej(&mut w, it, *b, &format!("{id} intent {k}")),
                None => pass(&mut w, it, &format!("{id} intent {k}")),
            }
        }
    };
    let dflt = || open_a1();
    case(
        "RSK-024.1",
        &|w| w.ok(set_mode(1, 1, G1, RunMode::CloseOnly)),
        &[(dflt(), Some(R::CLOSE_ONLY))],
    );
    case(
        "RSK-024.2",
        &|w| w.ok(kill_global(false)),
        &[(dflt(), Some(R::KILLED))],
    );
    case(
        "RSK-024.3",
        &|w| w.ok(kill_acct(1, false)),
        &[(dflt(), Some(R::KILLED)), (open_2(), None)],
    );
    case(
        "RSK-024.4",
        &|w| w.ok(kill_venue(B, 1)),
        &[(dflt(), None), (open_b1(), Some(R::KILLED))],
    );
    case("RSK-024.5", &|w| w.ok(draining()), &[(dflt(), Some(R::DRAINING))]);
    case("RSK-024.6", &|w| w.ok(daily(1)), &[(dflt(), Some(R::DAILY_LOSS))]);
    case(
        "RSK-024.7",
        &|w| w.ok(auto(CapSource::RECON, sc_av(1, B))),
        &[(open_b1(), Some(R::RECON_DEVIATION)), (dflt(), None)],
    );
    case(
        "RSK-024.8",
        &|w| w.ok(auto(CapSource::UNKNOWN_TIMEOUT, sc_av(1, B))),
        &[(open_b1(), Some(R::CLOSE_ONLY))],
    );
    case(
        "RSK-024.9",
        &|w| {
            w.ok(broken(1, G1));
            w.ok(auto(CapSource::HEDGE_FAILURE, sc_acct(1)));
        },
        &[
            (dflt(), Some(R::CLOSE_ONLY)),
            (open_g3_a2(), Some(R::CLOSE_ONLY)),
            (open_2(), None),
        ],
    );
    case(
        "RSK-024.10",
        &|w| w.ok(margin_auto(1, 1, 1 << B.0)),
        &[(dflt(), Some(R::CLOSE_ONLY))],
    );
    case("RSK-024.11", &|w| w.storage(1), &[(dflt(), Some(R::CLOSE_ONLY))]);
    case(
        "RSK-024.12",
        &|w| w.ok(acfg(1, B)),
        &[(open_b1(), Some(R::CLOSE_ONLY)), (dflt(), None)],
    );
    case(
        "RSK-024.13",
        &|w| w.ok(auto(CapSource::RECOVERY, sc_acct(1))),
        &[(dflt(), Some(R::CLOSE_ONLY))],
    );
    case(
        "RSK-024.14",
        &|w| w.ok(halted(1, 1, G1)),
        &[(dflt(), Some(R::CLOSE_ONLY)), (open_g3_a2(), None)],
    );
    let b2_open = || open(1, G3, B2, Side::Buy, "0.1", "2500.0", Tif::Gtc);
    case(
        "RSK-024.15",
        &|w| w.delist(B, B1),
        &[(open_b1(), Some(R::CLOSE_ONLY)), (b2_open(), None)],
    );
    case(
        "RSK-024.16",
        &|w| w.delist(B, hft_types::InstId(0)),
        &[(b2_open(), Some(R::CLOSE_ONLY))],
    );
    case(
        "RSK-024.17",
        &|w| w.ok(kill_flatten(1)),
        &[(dflt(), Some(R::KILLED))],
    );
    case(
        "RSK-024.18",
        &|w| {
            w.ok(draining());
            w.ok(kill_global(false));
            w.ok(daily(1));
        },
        &[(dflt(), Some(R::DRAINING))],
    );
    case(
        "RSK-024.19",
        &|w| {
            w.ok(kill_acct(1, false));
            w.ok(daily(1));
        },
        &[(dflt(), Some(R::KILLED))],
    );
    case(
        "RSK-024.20",
        &|w| {
            w.ok(daily(1));
            w.ok(auto(CapSource::RECON, sc_av(1, B)));
        },
        &[(open_b1(), Some(R::DAILY_LOSS))],
    );
    case(
        "RSK-024.21",
        &|w| {
            w.ok(auto(CapSource::RECON, sc_av(1, B)));
            w.ok(margin_auto(1, 1, 1 << B.0));
        },
        &[(open_b1(), Some(R::RECON_DEVIATION))],
    );
    case(
        "RSK-024.22",
        &|w| w.ok(set_mode(1, 1, G1, RunMode::Shadow)),
        &[(dflt(), None)],
    );
    {
        let mut w = r0();
        w.ok(margin_auto(1, 1, 1 << B.0));
        w.mv.set_pos(1, B1, "-0.5");
        w.mv.set_pend(1, B1, Side::Buy, "0.1");
        let it = w.reduce(1, G1, B1, Side::Buy, "0.401", "50005.0");
        assert_eq!(it.flags & OrderFlags::RISK_REDUCING.0, 0);
        rej(&mut w, &it, R::CLOSE_ONLY, "RSK-024.23");
    }
}

#[test]
fn rsk_025_risk_reducing_passes_caps() {
    let case = |id: &str, set: &dyn Fn(&mut W)| {
        let mut w = r0();
        short_b1(&mut w);
        set(&mut w);
        for (k, it) in rr_three(&w).iter().enumerate() {
            pass(&mut w, it, &format!("{id} intent {k}"));
        }
    };
    case("RSK-025.1", &|w| w.ok(kill_global(false)));
    case("RSK-025.2", &|w| w.ok(kill_flatten(1)));
    case("RSK-025.3", &|w| w.ok(kill_venue(B, 1)));
    case("RSK-025.4", &|w| w.ok(draining()));
    case("RSK-025.5", &|w| w.clock_bad());
    case("RSK-025.6 daily", &|w| w.ok(daily(1)));
    case("RSK-025.6 recon", &|w| w.ok(auto(CapSource::RECON, sc_av(1, B))));
    case("RSK-025.6 unknown", &|w| {
        w.ok(auto(CapSource::UNKNOWN_TIMEOUT, sc_av(1, B)))
    });
    case("RSK-025.6 recovery", &|w| {
        w.ok(auto(CapSource::RECOVERY, sc_acct(1)))
    });
    case("RSK-025.6 halted", &|w| w.ok(halted(1, 1, G1)));
    case("RSK-025.7", &|w| {
        w.ok(broken(1, G1));
        w.ok(auto(CapSource::HEDGE_FAILURE, sc_acct(1)));
    });
    case("RSK-025.8 margin", &|w| w.ok(margin_auto(1, 3, 1 << B.0)));
    case("RSK-025.8 storage", &|w| w.storage(2));
    case("RSK-025.8 cfg", &|w| w.ok(acfg(1, B)));
    case("RSK-025.9", &|w| w.delist(B, B1));
    case("RSK-025.10", &|w| w.ok(set_mode(1, 1, G1, RunMode::CloseOnly)));
    case("RSK-025.11", &|w| {
        w.mv.not_ready.insert((B.0, 1));
        w.mv.unknown.insert((1, G1.0));
        w.legs(1, G1, "0.9", "1000");
    });
}

#[test]
fn rsk_026_condition_gates() {
    {
        let mut w = r0();
        w.mv.not_ready.insert((A.0, 1));
        rej(&mut w, &open_a1(), RejectBits::VENUE_NOT_READY, "RSK-026.1");
        w.mv.set_pos(1, A1, "0.01");
        let three = [
            w.reduce(1, G1, A1, Side::Sell, "0.01", "49999.00"),
            hedge(1, G1, A1, Side::Buy, "0.01", "50001.00", "0.01"),
            w.flatten(1, G1, A1, Side::Sell, "0.01", "49999.00", Origin::Exec),
        ];
        for (k, it) in three.iter().enumerate() {
            pass(&mut w, it, &format!("RSK-026.2 sub {k}"));
        }
        w.mv.unknown.insert((1, G1.0));
        rej(&mut w, &open_a1(), RejectBits::VENUE_NOT_READY, "RSK-026.7");
    }
    {
        let mut w = r0();
        w.mv.unknown.insert((1, G1.0));
        rej(&mut w, &open_a1(), RejectBits::UNKNOWN_PENDING, "RSK-026.3");
        pass(&mut w, &open_g3_a2(), "RSK-026.5");
    }
    {
        let mut w = r0_with(|p| with_acct(&p, 1, |a| a.unknown_blocks_open = false));
        w.mv.unknown.insert((1, G1.0));
        pass(&mut w, &open_a1(), "RSK-026.4");
    }
    {
        let mut w = r0();
        w.mv.not_ready.insert((B.0, 1));
        rej(&mut w, &link_l(), RejectBits::VENUE_NOT_READY, "RSK-026.6");
    }
}

const MS_TICKS: u64 = 1_000_000;

#[test]
fn rsk_027_quote_health() {
    let fresh = || {
        let mut w = r0();
        short_b1(&mut w);
        w
    };
    {
        let mut w = fresh();
        bid_empty(&mut w, A1);
        rej(&mut w, &open_a1(), RejectBits::NO_MID, "RSK-027.1");
        w.mv.bbo_mut(A1).conn_up = false;
        rej(&mut w, &open_a1(), RejectBits::NO_MID, "RSK-027.10");
    }
    {
        let mut w = fresh();
        w.mv.bbo_mut(A1).conn_up = false;
        rej(&mut w, &open_a1(), RejectBits::STALE_MID, "RSK-027.2");
    }
    {
        let mut w = fresh();
        w.mv.bbo_mut(A1).resync = true;
        rej(&mut w, &open_a1(), RejectBits::STALE_MID, "RSK-027.3");
    }
    {
        let mut w = fresh();
        for (id, s, ok) in [
            ("RSK-027.4", 2_000_000_000u64, true),
            ("RSK-027.5", 2_000_000_001, false),
            ("RSK-027.6", 1_999_999_999, true),
        ] {
            w.mv.silence.insert(A1.0, s);
            if ok {
                pass(&mut w, &open_a1(), id);
            } else {
                rej(&mut w, &open_a1(), RejectBits::STALE_MID, id);
            }
        }
        w.mv.bbo_mut(A1).rx_tsc = Tsc(w.now as u64 - 10_000 * MS_TICKS);
        w.mv.silence.insert(A1.0, 500 * MS_TICKS);
        pass(&mut w, &open_a1(), "RSK-027.7");
    }
    {
        let mut w = fresh();
        w.mv.bbo_mut(B1).conn_up = false;
        rej(&mut w, &hedge_b1(), RejectBits::STALE_MID, "RSK-027.8");
    }
    {
        let mut w = fresh();
        w.mv.bbo_mut(B1).resync = true;
        let it = w.flatten(1, G1, B1, Side::Buy, "0.01", "50005.0", Origin::Exec);
        rej(&mut w, &it, RejectBits::STALE_MID, "RSK-027.9");
    }
    {
        let mut w = fresh();
        let mut p = w.p.clone();
        p.conn_liveness_ms = 1_000;
        assert_eq!(w.risk.set_params(&p, AccountScope::All), Ok(()));
        w.mv.silence.insert(A1.0, 1_500 * MS_TICKS);
        rej(&mut w, &open_a1(), RejectBits::STALE_MID, "RSK-027.11");
    }
}

#[test]
fn rsk_028_price_band_per_inst() {
    let setup = |w: &mut W| {
        short_b1(w);
        w.mv.set_pos(1, A1, "0.02");
    };
    let mut w = r0();
    setup(&mut w);
    let o = |side: Side, p: &str, tif: Tif| open(1, G1, A1, side, "0.01", p, tif);
    let h = |i, side: Side, p: &str| hedge(1, G1, i, side, "0.01", p, "0.01");
    let rows: Vec<(&str, IntentView, bool)> = vec![
        ("RSK-028.1", o(Side::Buy, "51500.00", Tif::Gtc), true),
        ("RSK-028.2", o(Side::Buy, "51500.01", Tif::Gtc), false),
        ("RSK-028.3 a", o(Side::Sell, "48500.00", Tif::Gtc), true),
        ("RSK-028.3 b", o(Side::Sell, "48499.99", Tif::Gtc), false),
        ("RSK-028.4", o(Side::Buy, "50251.00", Tif::Ioc), true),
        ("RSK-028.5", o(Side::Buy, "50251.01", Tif::Ioc), false),
        ("RSK-028.6 a", o(Side::Sell, "49749.01", Tif::Ioc), true),
        ("RSK-028.6 b", o(Side::Sell, "49749.00", Tif::Ioc), false),
        ("RSK-028.7", o(Side::Buy, "50300.00", Tif::Gtc), true),
        ("RSK-028.8", h(A1, Side::Buy, "50500.00"), true),
        ("RSK-028.9", h(A1, Side::Buy, "50500.01"), false),
        ("RSK-028.10 a", h(B1, Side::Sell, "49500.0"), true),
        ("RSK-028.10 b", h(B1, Side::Sell, "49499.9"), false),
        ("RSK-028.11", h(A1, Side::Buy, "40000.00"), true),
        (
            "RSK-028.12 a",
            hedge(1, G3, A2, Side::Buy, "0.1", "2550.00", "0.1"),
            true,
        ),
        (
            "RSK-028.12 b",
            hedge(1, G3, A2, Side::Buy, "0.1", "2550.01", "0.1"),
            false,
        ),
        (
            "RSK-028.13 a",
            open(1, G3, A2, Side::Buy, "0.1", "2625.00", Tif::Gtc),
            true,
        ),
        (
            "RSK-028.13 b",
            open(1, G3, A2, Side::Buy, "0.1", "2625.01", Tif::Gtc),
            false,
        ),
        (
            "RSK-028.20",
            w.reduce(1, G1, B1, Side::Buy, "0.01", "50600.0"),
            false,
        ),
    ];
    for (id, it, ok) in rows {
        if ok {
            pass(&mut w, &it, id);
        } else {
            rej(&mut w, &it, RejectBits::PX_BAND, id);
        }
    }
    let l3 = |x: &str| {
        let x = r(x);
        let mut w = r0_with(|mut p| {
            p.l3_max_cross = x;
            p
        });
        setup(&mut w);
        w
    };
    let mut w = l3("0");
    let fl = |w: &W, p: &str, o: Origin| w.flatten(1, G1, B1, Side::Buy, "0.01", p, o);
    let it = fl(&w, "50750.0", Origin::Exec);
    rej(&mut w, &it, RejectBits::PX_BAND, "RSK-028.14");
    let mut w = l3("0.02");
    let it = fl(&w, "50750.0", Origin::Exec);
    pass(&mut w, &it, "RSK-028.15");
    let it = fl(&w, "51000.0", Origin::Exec);
    pass(&mut w, &it, "RSK-028.16 a");
    let it = fl(&w, "51000.1", Origin::Exec);
    rej(&mut w, &it, RejectBits::PX_BAND, "RSK-028.16 b");
    let it = fl(&w, "50750.0", Origin::Strategy);
    rej(&mut w, &it, RejectBits::PX_BAND, "RSK-028.18");
    let it = hedge_fix(1, G1, B1, Side::Buy, "0.01", "50750.0", "0.01");
    rej(&mut w, &it, RejectBits::PX_BAND, "RSK-028.19");
    let mut w = l3("0.005");
    let it = fl(&w, "50750.0", Origin::Exec);
    rej(&mut w, &it, RejectBits::PX_BAND, "RSK-028.17");
}

#[test]
fn rsk_029_link_quote_health() {
    let mut w = r0();
    pass(&mut w, &link_l(), "RSK-029.1");
    let fresh = |f: &dyn Fn(&mut W)| {
        let mut w = r0();
        f(&mut w);
        w
    };
    let mut w = fresh(&|w| w.mv.bbo_mut(B1).conn_up = false);
    rej(&mut w, &link_l(), RejectBits::LINK_STALE, "RSK-029.2");
    let mut w = fresh(&|w| w.mv.bbo_mut(B1).resync = true);
    rej(&mut w, &link_l(), RejectBits::LINK_STALE, "RSK-029.3");
    let mut w = fresh(&|w| {
        w.mv.silence.insert(B1.0, 2_000 * MS_TICKS);
    });
    pass(&mut w, &link_l(), "RSK-029.4 a");
    w.mv.silence.insert(B1.0, 2_000 * MS_TICKS + 1);
    rej(&mut w, &link_l(), RejectBits::LINK_STALE, "RSK-029.4 b");
    let mut w = fresh(&|w| {
        w.mv.bbo_mut(B1).rx_tsc = Tsc(w.now as u64 - 10_000 * MS_TICKS);
        w.mv.silence.insert(B1.0, 500 * MS_TICKS);
    });
    pass(&mut w, &link_l(), "RSK-029.5");
    let mut w = fresh(&|w| {
        w.mv.bbo_mut(A1).conn_up = false;
        w.mv.bbo_mut(B1).conn_up = false;
    });
    rej(&mut w, &link_l(), RejectBits::STALE_MID, "RSK-029.6");
    let mut w = fresh(&|w| bid_empty(w, B1));
    pass(&mut w, &link_l(), "RSK-029.7");
    let mut w = fresh(&|w| w.mv.bbo_mut(A1).conn_up = false);
    rej(&mut w, &link_sim(), RejectBits::LINK_STALE, "RSK-029.8");
}

#[test]
fn rsk_031_notional_definition() {
    let nt = |qty: &str, p: &str, rate: &str| Risk::notional(q(qty), px(p), EXP, r(rate));
    assert_eq!(nt("0.01", "50000.00", "1"), n("500"), "RSK-031.1");
    assert_eq!(nt("0.0001", "50000.01", "1"), n("5.000001"), "RSK-031.2");
    assert_eq!(nt("0.00000001", "0.01", "1"), n("0.000001"), "RSK-031.3");
    assert_eq!(nt("0.01", "50000.00", "0.999"), n("499.5"), "RSK-031.4");
    assert_eq!(nt("1", "1.00", "0.333333333"), n("0.333334"), "RSK-031.5");
    assert_eq!(nt("1", "50000.01", "1"), n("50000.01"), "RSK-031.6 a");
    assert_eq!(nt("1", "50000.00", "1"), n("50000"), "RSK-031.6 b");
    assert_eq!(nt("1000000", "1000000.00", "1"), n("1000000000000"), "RSK-031.7");
}

#[test]
fn rsk_032_open_notional_levels() {
    let with = |nv: &str| {
        let mut it = open_a1();
        it.notional = n(nv);
        it
    };
    let run = |id: &str, set: &dyn Fn(&mut W), nv: &str, want: Option<RejectBits>| {
        let mut w = r0();
        set(&mut w);
        match want {
            Some(b) => rej(&mut w, &with(nv), b, id),
            None => pass(&mut w, &with(nv), id),
        }
    };
    use RejectBits as R;
    let g = Scope::Group(G1);
    run("RSK-032.1", &|_| {}, "10000", None);
    run("RSK-032.2", &|_| {}, "10000.000001", Some(R::NOTIONAL_ORDER));
    run("RSK-032.3", &|w| w.mv.set_held(1, g, "45000"), "5000", None);
    run(
        "RSK-032.4",
        &|w| {
            w.mv.set_held(1, g, "45000");
            w.mv.set_open_n(1, g, "0.000001");
        },
        "5000",
        Some(R::NOTIONAL_GROUP),
    );
    run(
        "RSK-032.5",
        &|w| w.mv.set_held(1, Scope::Account, "95000"),
        "5000",
        None,
    );
    run(
        "RSK-032.6",
        &|w| w.mv.set_held(1, Scope::Account, "95000.000001"),
        "5000",
        Some(R::NOTIONAL_ACCOUNT),
    );
    let eq = |w: &mut W, total: &str, valid: bool| {
        w.mv.equity.insert(
            1,
            EquityView {
                total: n(total),
                valid,
            },
        );
    };
    run(
        "RSK-032.7 a",
        &|w| {
            eq(w, "90000", true);
            w.mv.set_held(1, Scope::Account, "85000");
        },
        "5000",
        None,
    );
    run(
        "RSK-032.7 b",
        &|w| {
            eq(w, "90000", true);
            w.mv.set_held(1, Scope::Account, "85000.000001");
        },
        "5000",
        Some(R::NOTIONAL_ACCOUNT),
    );
    run(
        "RSK-032.8",
        &|w| {
            eq(w, "1000", false);
            w.mv.set_held(1, Scope::Account, "95000");
        },
        "5000",
        None,
    );
    run(
        "RSK-032.9 a",
        &|w| w.mv.set_held(1, Scope::Venue(A), "75000"),
        "5000",
        None,
    );
    run(
        "RSK-032.9 b",
        &|w| w.mv.set_held(1, Scope::Venue(A), "75000.000001"),
        "5000",
        Some(R::NOTIONAL_VENUE),
    );
    run(
        "RSK-032.10",
        &|w| w.mv.set_held(1, g, "45000"),
        "10000.000001",
        Some(R::NOTIONAL_ORDER),
    );
}

#[test]
fn rsk_033_hedge_and_reduce_limits() {
    let mut w = r0();
    let h = |qty: &str, p: &str, cap: &str| hedge(1, G1, B1, Side::Sell, qty, p, cap);
    pass(&mut w, &h("0.01", "49995.0", "0.01"), "RSK-033.1");
    let over = h("0.011", "49995.0", "0.01");
    rej(&mut w, &over, RejectBits::HEDGE_LIMIT, "RSK-033.2");
    pass(&mut w, &h("0.4", "50000.0", "0.4"), "RSK-033.3");
    let mut it = h("0.4", "50000.0", "0.4");
    it.notional = n("20000.000001");
    rej(&mut w, &it, RejectBits::HEDGE_LIMIT, "RSK-033.4");
    pass(&mut w, &h("0.3", "50000.0", "0.3"), "RSK-033.5");
    let it = hedge_fix(1, G1, B1, Side::Buy, "0.011", "50005.0", "0.01");
    rej(&mut w, &it, RejectBits::HEDGE_LIMIT, "RSK-033.6");
    rej(&mut w, &over, RejectBits::HEDGE_LIMIT, "RSK-033.11 first");
    rej(&mut w, &over, RejectBits::HEDGE_LIMIT, "RSK-033.11 second");
    w.mv.set_pos(1, B1, "-1");
    let red = w.reduce(1, G1, B1, Side::Buy, "0.2", "50000.0");
    pass(&mut w, &red, "RSK-033.7");
    let mut it = red;
    it.notional = n("10000.000001");
    rej(&mut w, &it, RejectBits::NOTIONAL_ORDER, "RSK-033.8");
    let mut it = w.flatten(1, G1, B1, Side::Buy, "0.2", "50000.0", Origin::Exec);
    it.notional = n("10000.000001");
    rej(&mut w, &it, RejectBits::NOTIONAL_ORDER, "RSK-033.9");
    w.mv.set_held(1, Scope::Account, "200000");
    pass(&mut w, &red, "RSK-033.10");
}

#[test]
fn rsk_034_position_inflight() {
    let fresh = || {
        r0_with(|p| {
            with_acct(&p, 1, |a| {
                a.max_order_notional = n("50000");
                a.max_reduce_notional = n("50000");
                a.max_hedge_notional = n("50000");
            })
        })
    };
    let o = |side: Side, qty: &str, p: &str| open(1, G1, B1, side, qty, p, Tif::Gtc);
    {
        let mut w = fresh();
        w.mv.set_pos(1, B1, "0.5");
        w.mv.set_open(1, B1, Side::Buy, "0.4");
        pass(&mut w, &o(Side::Buy, "0.1", "50000.0"), "RSK-034.1");
        rej(
            &mut w,
            &o(Side::Buy, "0.101", "50000.0"),
            RejectBits::POSITION,
            "RSK-034.2",
        );
    }
    {
        let mut w = fresh();
        w.mv.set_pos(1, B1, "-0.5");
        w.mv.set_open(1, B1, Side::Sell, "0.4");
        pass(&mut w, &o(Side::Sell, "0.1", "49995.0"), "RSK-034.3 a");
        rej(
            &mut w,
            &o(Side::Sell, "0.101", "49995.0"),
            RejectBits::POSITION,
            "RSK-034.3 b",
        );
    }
    {
        let mut w = fresh();
        w.mv.set_pos(1, B1, "0.5");
        pass(&mut w, &o(Side::Sell, "0.3", "49995.0"), "RSK-034.4");
    }
    {
        let mut w = fresh();
        w.mv.set_pos(1, B1, "0.5");
        w.mv.set_open_imm(1, B1, Side::Buy, "0.2");
        w.mv.set_open(1, B1, Side::Buy, "0.5");
        pass(
            &mut w,
            &hedge(1, G1, B1, Side::Buy, "0.3", "50005.0", "0.3"),
            "RSK-034.5",
        );
        let it = hedge(1, G1, B1, Side::Buy, "0.301", "50005.0", "0.301");
        rej(&mut w, &it, RejectBits::POSITION, "RSK-034.6");
    }
    {
        let mut w = fresh();
        w.mv.set_pos(1, B1, "0.5");
        w.mv.set_pend_imm(1, B1, Side::Sell, "0.1");
        w.mv.set_pend(1, B1, Side::Sell, "0.5");
        let ro = |qty: &str| {
            let mut it = hedge(1, G1, B1, Side::Sell, qty, "49995.0", qty);
            it.flags |= OrderFlags::REDUCE_ONLY.0;
            it
        };
        pass(&mut w, &ro("0.4"), "RSK-034.7");
        rej(&mut w, &ro("0.401"), RejectBits::REDUCE_INVALID, "RSK-034.8");
        let it = w.flatten(1, G1, B1, Side::Sell, "0.4", "49995.0", Origin::Exec);
        pass(&mut w, &it, "RSK-034.9 a");
        let it = w.flatten(1, G1, B1, Side::Sell, "0.401", "49995.0", Origin::Exec);
        rej(&mut w, &it, RejectBits::REDUCE_INVALID, "RSK-034.9 b");
        let it = w.reduce(1, G1, B1, Side::Sell, "0.001", "49995.0");
        rej(&mut w, &it, RejectBits::REDUCE_INVALID, "RSK-034.10");
    }
    {
        let mut w = fresh();
        w.mv.set_pos(1, B1, "0.5");
        w.mv.set_pend(1, B1, Side::Sell, "0.1");
        let it = w.reduce(1, G1, B1, Side::Sell, "0.4", "49995.0");
        pass(&mut w, &it, "RSK-034.11");
    }
    {
        let mut w = fresh();
        w.mv.set_pos(1, B1, "-0.9");
        w.mv.set_open(1, B1, Side::Sell, "0.05");
        let l = |lq: &str| {
            link_open(
                1,
                G1,
                A1,
                Side::Buy,
                "0.05",
                "50000.00",
                (1, B, B1, Side::Sell, lq, "2475"),
            )
        };
        pass(&mut w, &l("0.05"), "RSK-034.12 a");
        rej(&mut w, &l("0.051"), RejectBits::POSITION, "RSK-034.12 b");
    }
    for (id, max, ok) in [("RSK-034.13 a", "0.001", true), ("RSK-034.13 b", "0.0001", false)] {
        let m = q(max);
        let mut w = r0_with(|p| with_acct(&p, 1, |a| a.default_group.max_pos_qty = m));
        if ok {
            pass(&mut w, &smoke(1, 1), id);
        } else {
            rej(&mut w, &smoke(1, 1), RejectBits::POSITION, id);
        }
    }
}

#[test]
fn rsk_035_link_affordability() {
    let spot_buy = || {
        link_open(
            1,
            G1,
            B1,
            Side::Sell,
            "0.01",
            "50000.0",
            (1, A, A1, Side::Buy, "0.01", "505"),
        )
    };
    let spot_sell = || {
        link_open(
            1,
            G1,
            B1,
            Side::Buy,
            "0.01",
            "50000.0",
            (1, A, A1, Side::Sell, "0.01", "495"),
        )
    };
    let perp = link_l;
    let quote = |w: &mut W, v: Option<&str>| {
        w.mv.avail_quote.insert((1, A.0), v.map(|x| n(x).0));
    };
    let margin = |w: &mut W, v: Option<&str>| {
        w.mv.avail_margin.insert((1, B.0), v.map(|x| n(x).0));
    };
    let mut w = r0();
    quote(&mut w, Some("530.25"));
    pass(&mut w, &spot_buy(), "RSK-035.1");
    quote(&mut w, Some("530.249999"));
    rej(&mut w, &spot_buy(), RejectBits::LINK_FUNDS, "RSK-035.2");
    quote(&mut w, None);
    rej(&mut w, &spot_buy(), RejectBits::LINK_FUNDS, "RSK-035.3");
    let mut w = r0();
    let mut it = spot_buy();
    it.link_notional = n("20000");
    pass(&mut w, &it, "RSK-035.4");
    it.link_notional = n("20000.000001");
    rej(&mut w, &it, RejectBits::HEDGE_LIMIT, "RSK-035.5");
    let mut w = r0_with(|p| with_acct(&p, 1, |a| a.funds_buffer = Rate(0)));
    quote(&mut w, Some("505"));
    pass(&mut w, &spot_buy(), "RSK-035.6 a");
    quote(&mut w, Some("504.999999"));
    rej(&mut w, &spot_buy(), RejectBits::LINK_FUNDS, "RSK-035.6 b");
    let mut w = r0();
    w.mv.set_pos(1, A1, "0.01");
    pass(&mut w, &spot_sell(), "RSK-035.7");
    w.mv.set_pend(1, A1, Side::Sell, "0.0001");
    rej(&mut w, &spot_sell(), RejectBits::LINK_FUNDS, "RSK-035.8");
    let mut w = r0();
    margin(&mut w, Some("173.25"));
    pass(&mut w, &perp(), "RSK-035.9");
    margin(&mut w, Some("173.249999"));
    rej(&mut w, &perp(), RejectBits::LINK_FUNDS, "RSK-035.10");
    margin(&mut w, None);
    rej(&mut w, &perp(), RejectBits::LINK_FUNDS, "RSK-035.14");
    let report = |w: &mut W, rflags: u16, levs: &[hft_event::LeverageItem]| {
        let mut h = hdr(1, B, 0, KEYS);
        h.rflags = rflags;
        w.risk.on_account_cfg(&h, levs);
    };
    let mut w = r0();
    report(&mut w, 0, &[lev(B1, "0"), lev(B2, "3.00")]);
    rej(&mut w, &perp(), RejectBits::LINK_FUNDS, "RSK-035.11");
    let mut w = r0();
    report(&mut w, 0, &[lev(B2, "3.00")]);
    rej(&mut w, &perp(), RejectBits::LINK_FUNDS, "RSK-035.12");
    let mut w = r0();
    report(&mut w, acr_rflags::LEV_TRUNCATED, &[lev(B2, "3.00")]);
    rej(&mut w, &perp(), RejectBits::LINK_FUNDS, "RSK-035.13");
    {
        let mut sp = r0_spec();
        for s in sp.insts.iter_mut().filter(|s| s.id == B1) {
            s.max_leverage = 20;
        }
        let rd = load(&sp);
        let mut w = world(rd, p0_for(rd, 3), MC, CAPS);
        margin(&mut w, Some("173.25"));
        pass(&mut w, &perp(), "RSK-035.15 a");
        margin(&mut w, Some("26.0"));
        rej(&mut w, &perp(), RejectBits::LINK_FUNDS, "RSK-035.15 b");
    }
    let mut w = r0();
    report(&mut w, acr_rflags::PARTIAL, &[lev(B2, "5.00")]);
    margin(&mut w, Some("173.25"));
    pass(&mut w, &perp(), "RSK-035.16");
    margin(&mut w, None);
    pass(&mut w, &hedge_b1(), "RSK-035.17");
}

#[test]
fn rsk_041_exposure_gate() {
    let run = |id: &str, legs: &[(&str, &str)], it: IntentView, ok: bool| {
        let mut w = r0();
        for (ra, na) in legs {
            w.legs(1, G1, ra, na);
        }
        if ok {
            pass(&mut w, &it, id);
        } else {
            rej(&mut w, &it, RejectBits::EXPOSURE_GATE, id);
        }
    };
    run("RSK-041.1", &[("0.5", "1000")], open_a1(), true);
    run("RSK-041.2", &[("0.500000001", "100")], open_a1(), false);
    run("RSK-041.3", &[("0.500000001", "99.999999")], open_a1(), true);
    run("RSK-041.4 a", &[("0.9", "1000")], hedge_b1(), true);
    run("RSK-041.4 b", &[("0.9", "1000")], open_g3_a2(), true);
    run("RSK-041.5", &[("0.9", "1000"), ("0.1", "1000")], open_a1(), true);
    run("RSK-041.6", &[], open_a1(), true);
}

#[test]
fn rsk_042_smoke_order_scope() {
    use RejectBits as R;
    let s = || smoke(1, 1);
    let run = |id: &str, set: &dyn Fn(&mut W), want: Option<RejectBits>| {
        let mut w = r0();
        set(&mut w);
        match want {
            Some(b) => rej(&mut w, &s(), b, id),
            None => pass(&mut w, &s(), id),
        }
    };
    run("RSK-042.1", &|_| {}, None);
    run(
        "RSK-042.2 a",
        &|w| {
            w.ok(ModeInput::SetMode {
                scope: sc_cell(1, 1, GROUP_ALL),
                mode: RunMode::Shadow,
            })
        },
        None,
    );
    run(
        "RSK-042.2 b",
        &|w| {
            w.ok(ModeInput::SetMode {
                scope: sc_cell(1, 1, GROUP_ALL),
                mode: RunMode::CloseOnly,
            })
        },
        None,
    );
    let nine: [(&str, &dyn Fn(&mut W)); 9] = [
        ("recovery", &|w| w.ok(auto(CapSource::RECOVERY, sc_acct(1)))),
        ("margin", &|w| w.ok(margin_auto(1, 1, 1 << B.0))),
        ("broken", &|w| {
            w.ok(broken(1, G1));
            w.ok(auto(CapSource::HEDGE_FAILURE, sc_acct(1)));
        }),
        ("daily", &|w| w.ok(daily(1))),
        ("recon", &|w| w.ok(auto(CapSource::RECON, sc_av(1, A)))),
        ("cfg", &|w| w.ok(acfg(1, A))),
        ("unknown", &|w| {
            w.ok(auto(CapSource::UNKNOWN_TIMEOUT, sc_av(1, A)))
        }),
        ("storage", &|w| w.storage(1)),
        ("halted", &|w| w.ok(halted(1, 1, G1))),
    ];
    for (name, f) in nine {
        run(&format!("RSK-042.3 {name}"), f, None);
    }
    run("RSK-042.4", &|w| w.ok(kill_acct(1, false)), Some(R::KILLED));
    run("RSK-042.5 a", &|w| w.ok(kill_venue(A, 1)), Some(R::KILLED));
    run("RSK-042.5 b", &|w| w.ok(kill_venue(B, 1)), None);
    run("RSK-042.6", &|w| w.ok(kill_global(false)), Some(R::KILLED));
    run("RSK-042.7", &|w| w.ok(kill_acct(1, true)), Some(R::HARD));
    run("RSK-042.8", &|w| w.ok(draining()), Some(R::DRAINING));
    run("RSK-042.9", &|w| w.delist(A, A1), Some(R::CLOSE_ONLY));
    run(
        "RSK-042.10 a",
        &|w| w.mv.bbo_mut(A1).conn_up = false,
        Some(R::STALE_MID),
    );
    run("RSK-042.10 b", &|w| bid_empty(w, A1), Some(R::NO_MID));
    run("RSK-042.11", &|w| w.clock_bad(), Some(R::CLOCK));
    run(
        "RSK-042.12",
        &|w| {
            w.mv.not_ready.insert((A.0, 1));
        },
        Some(R::VENUE_NOT_READY),
    );
    run(
        "RSK-042.13",
        &|w| {
            let n_it = open(1, G1, A1, Side::Buy, "0.0001", "50000.00", Tif::Gtc);
            w.commits(&n_it, 80);
        },
        Some(R::RATE),
    );
}

#[test]
fn rsk_043_linked_open_both_venues() {
    use RejectBits as R;
    let run = |id: &str, set: &dyn Fn(&mut W), it: IntentView, want: Option<RejectBits>| {
        let mut w = r0();
        set(&mut w);
        match want {
            Some(b) => rej(&mut w, &it, b, id),
            None => pass(&mut w, &it, id),
        }
    };
    run("RSK-043.1", &|_| {}, link_l(), None);
    run(
        "RSK-043.2",
        &|w| w.ok(kill_venue(B, 1)),
        link_l(),
        Some(R::KILLED),
    );
    run(
        "RSK-043.3",
        &|w| w.ok(auto(CapSource::RECON, sc_av(1, B))),
        link_l(),
        Some(R::RECON_DEVIATION),
    );
    run(
        "RSK-043.4 cfg",
        &|w| w.ok(acfg(1, B)),
        link_l(),
        Some(R::CLOSE_ONLY),
    );
    run(
        "RSK-043.4 unknown",
        &|w| w.ok(auto(CapSource::UNKNOWN_TIMEOUT, sc_av(1, B))),
        link_l(),
        Some(R::CLOSE_ONLY),
    );
    run(
        "RSK-043.4 delisting",
        &|w| w.delist(B, B1),
        link_l(),
        Some(R::CLOSE_ONLY),
    );
    run(
        "RSK-043.5",
        &|w| {
            w.mv.not_ready.insert((B.0, 1));
        },
        link_l(),
        Some(R::VENUE_NOT_READY),
    );
    run(
        "RSK-043.6",
        &|w| w.ok(margin_auto(1, 1, 1 << B.0)),
        link_l2(),
        Some(R::CLOSE_ONLY),
    );
    run(
        "RSK-043.7",
        &|w| w.ok(kill_acct(1, false)),
        link_l2(),
        Some(R::KILLED),
    );
    run("RSK-043.8", &|w| w.ok(halted(1, 1, G1)), link_l2(), None);
    run(
        "RSK-043.9",
        &|w| w.ok(kill_venue(A, 1)),
        link_sim(),
        Some(R::KILLED),
    );
}

#[test]
fn rsk_240_views() {
    let mut w = r0_with(|mut p| {
        p.l3_max_cross = r("0.02");
        p
    });
    w.ok(kill_flatten(1));
    w.margin(1, B, &msample("0.15", "0.10", T0), T0);
    let d = T0 + 30 * SEC;
    w.hit(B, 1);
    let now = w.nowv();
    let v = w.risk.view(acc(1), &now);
    assert_eq!(v.margin_level, MarginLevel::L2);
    assert_eq!(v.liq_dist, r("0.15"));
    assert_eq!(v.deadline_tsc, now.tsc_of(d));
    assert!(v.open_blocked);
    assert!(!v.hard);
    assert!(v.flatten_lock);
    assert_eq!(v.max_order_notional, n("10000"));
    assert_eq!(v.max_hedge_notional, n("20000"));
    assert_eq!(v.max_reduce_notional, n("10000"));
    assert_eq!(v.conn_liveness_ms, 2_000);
    let b = w.risk.view_for(acc(1), B1, &now);
    assert_eq!(
        (
            b.max_px_dev,
            b.ioc_max_cross,
            b.rr_max_cross,
            b.l3_max_cross,
            b.rate_backoff
        ),
        (r("0.03"), r("0.005"), r("0.01"), r("0.02"), true)
    );
    let a = w.risk.view_for(acc(1), A2, &now);
    assert_eq!(
        (
            a.max_px_dev,
            a.ioc_max_cross,
            a.rr_max_cross,
            a.l3_max_cross,
            a.rate_backoff
        ),
        (r("0.05"), r("0.01"), r("0.02"), r("0.02"), false)
    );
    let h = w.risk.hedge_px_limits(acc(1), B, B1);
    assert_eq!((h.conn_liveness_ms, h.rr_max_cross), (2_000, r("0.01")));
}
