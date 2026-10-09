//! EXE-115–EXE-120: quote availability by connection health.
mod common;
use common::*;
use hft_exec::*;
use hft_strategy_sdk::{BookStatus, ExecSpec, MarginLevel, TargetState};
use hft_types::{Px, Qty, Side};

#[test]
fn exe_115_quote_ok_boundary() {
    type Edit = fn(&mut Fx);
    let rows: [(&str, Edit, bool); 7] = [
        ("EXE-115.1", |fx| fx.stale(P, 1999), true),
        ("EXE-115.2", |fx| fx.stale(P, 2000), true),
        ("EXE-115.3", |fx| fx.stale(P, 2001), false),
        (
            "EXE-115.4",
            |fx| {
                let b = fx.v.m.bbo_of(P);
                hft_strategy_sdk::testing::MockViews::conn(&mut fx.v.m, P, b.conn_live_tsc, true, true);
            },
            false,
        ),
        ("EXE-115.5", |fx| fx.quote_down(P), false),
        (
            "EXE-115.6",
            |fx| {
                let b = fx.v.m.bbo_of(P);
                hft_strategy_sdk::testing::MockViews::bbo(
                    &mut fx.v.m,
                    P,
                    (Px(0), Qty(0)),
                    (b.ask_px, b.ask_qty),
                );
            },
            false,
        ),
        (
            "EXE-115.7",
            |fx| {
                let b = fx.v.m.bbo_of(P);
                hft_strategy_sdk::testing::MockViews::bbo(
                    &mut fx.v.m,
                    P,
                    (b.bid_px, b.bid_qty),
                    (Px(0), Qty(0)),
                );
            },
            false,
        ),
    ];
    for (row, edit, order) in rows {
        let mut fx = Fx::new();
        fx.at(10_000);
        edit(&mut fx);
        let r = fx.go(&t1("-10.0"));
        assert_eq!(r.orders.len(), usize::from(order), "{row}");
        if !order && row == "EXE-115.3" {
            assert_eq!(fx.cell(A1, G8).gated_since, t(10_000), "{row}");
        }
    }
}

#[test]
fn exe_116_quiet_book_prices() {
    // The P quote last changed at t 0; only pong evidence refreshes conn_live_tsc.
    let at = 10_000;
    // .1
    let mut fx = Fx::new();
    fx.at(at);
    let r = fx.go(&t1("-10"));
    assert_eq!(r.orders[0].intent.px, px("99.31"), "EXE-116.1");
    // .2
    let mut fx = Fx::new();
    fx.at(at);
    fx.legs(A1, G7, &["1.00", "-1.5"]);
    fx.leg_adjust(A1, G7);
    let o = fx.adv().ord(ExecOrderKind::HedgeFix)[0];
    assert_eq!(
        (o.intent.side, o.intent.qty, o.intent.px),
        (Side::Buy, q("0.5"), px("100.70")),
        "EXE-116.2"
    );
    // .3
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["1.00", "-1.5"]);
    {
        let c = fx.cm(A1, G7);
        c.view.state = TargetState::Broken(hft_strategy_sdk::BrokenReason::LinkBroken);
        c.broken_since = t(0);
        c.auto_fix.phase = AutoFixPhase::Primed as u8;
        c.auto_fix.due_tsc = t(at);
    }
    fx.at(at);
    fx.touch(P);
    let r = fx.adv();
    assert!(r.has_dec(14, pack(4, 1, 0)), "EXE-116.3");
    assert_eq!(r.ord(ExecOrderKind::AutoFix).len(), 1, "EXE-116.3");
    // .4
    let mut fx = Fx::new();
    fx.at(at);
    fx.legs(A1, G8, &["-10"]);
    fx.risk(A1, |r| r.margin_level = MarginLevel::L3);
    fx.force(A1, B, "0.2");
    assert_eq!(fx.adv().ord(ExecOrderKind::ForceStep).len(), 1, "EXE-116.4");
}

#[test]
fn exe_117_threshold_from_risk_view() {
    for (row, ago, order) in [("EXE-117.1", 500u64, true), ("EXE-117.2", 501, false)] {
        let mut fx = Fx::new();
        fx.at(10_000);
        fx.risk(A1, |r| r.conn_liveness_ms = 500);
        fx.stale(P, ago);
        assert_eq!(fx.go(&t1("-10")).orders.len(), usize::from(order), "{row}");
    }
}

#[test]
fn exe_118_non_live_book_top_only() {
    for (row, st, qty) in [
        ("EXE-118.1", None, "25.0"),
        ("EXE-118.2", Some(BookStatus::Resyncing), "5.0"),
        ("EXE-118.3", Some(BookStatus::Uninit), "5.0"),
    ] {
        let mut fx = Fx::new();
        fx.risk(A1, |r| r.max_order_notional = usd("100000"));
        if let Some(s) = st {
            fx.v.book_status.push((P, s));
        }
        let mut tg = t1("-100.0");
        if let ExecSpec::TakerSlice(p) = &mut tg.exec {
            p.depth_frac = rate("0.1");
            p.max_chunk_notional = usd("100000");
        }
        assert_eq!(fx.go(&tg).orders[0].intent.qty, q(qty), "{row}");
    }
}

#[test]
fn exe_119_engine_quote_wait_deadline_runs() {
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["1.00", "-1.5"]);
    fx.quote_down(P);
    fx.leg_adjust(A1, G7);
    let r = fx.adv();
    assert!(r.orders.is_empty());
    assert_eq!(fx.cell(A1, G7).fix.first_tsc, t(0));
    let mut ms = 500;
    while ms <= 10_000 {
        fx.at(ms);
        fx.touch(P);
        let r = fx.adv();
        assert!(r.orders.is_empty() && r.states().is_empty(), "t {ms}");
        assert_eq!(fx.cell(A1, G7).fix.count, 0);
        ms += 500;
    }
    fx.at(10_001);
    fx.touch(P);
    let r = fx.adv();
    assert!(r.states().contains(&(1, 4, pack(3, 3, 0))));
    let n = r.notes_n(hft_exec::decision::note::EXEC_BROKEN);
    assert_eq!(n[0].level, hft_event::NoteLevel::P0);
}

#[test]
fn exe_120_force_quote_wait_no_deadline() {
    let mut fx = Fx::new();
    fx.legs(A1, G8, &["-10.0"]);
    fx.quote_down(P);
    fx.flatten(A1);
    let mut stuck = Vec::new();
    let mut ms = 0;
    while ms <= 60_000 {
        fx.at(ms);
        fx.touch(P);
        let r = fx.adv();
        assert!(r.orders.iter().all(|o| o.intent.group != G8));
        for s in r.states() {
            if s.1 == 3 {
                stuck.push((ms, s.2));
            }
        }
        ms += 500;
    }
    assert_eq!(stuck, vec![(1000, pack(2, 4, 0))]);
    assert!(!matches!(fx.cell(A1, G8).view.state, TargetState::Broken(_)));
}
