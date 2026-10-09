//! Zero allocation of the fast-part components and their sizes (TESTS.md SLB-132).

mod book;
mod common;

use core::mem::size_of;
use std::hint::black_box;

use hft_strat_lib::{
    ConfirmParams, ConfirmParamsNs, ConfirmWindow, CostItem, CostLine, DirtyFilter, DirtyParams,
    DirtyParamsTsc, Expiring, FireKind, Freshness, ImpactModel, LegFresh, Outcome, SpreadCalc, Throttle,
    ThrottleParams, ThrottleParamsNs, TriggerPx, VwapParams, cost_flags,
};
use hft_testkit::CountingAlloc;
use hft_types::{Px, Qty, Rate, Side, Tsc};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

const N: i64 = 10_000;

#[test]
fn slb_132_zero_alloc_and_sizes() {
    assert!(CountingAlloc::is_installed());
    // Setup (may allocate): a 20-level book and the parameters.
    let asks: Vec<(i64, i64)> = (0..20).map(|k| (6_001_000 + k * 100, 10_000_000)).collect();
    let bids: Vec<(i64, i64)> = (0..20).map(|k| (6_000_000 - k * 100, 10_000_000)).collect();
    let b = book::TestBook::live(&bids, &asks);
    let vp = VwapParams {
        book_frac: Rate(500_000_000),
        max_levels: 20,
    };
    let mut cl = CostLine::default();
    cl.set(
        &[CostItem::new(Rate(3_000_000), 1, cost_flags::ROUND_TRIP)],
        Rate(0),
    )
    .unwrap();
    let cp = ConfirmParams {
        min_ms: 10,
        min_updates: 3,
        min_slow_updates: 1,
        min_slow_span: 1,
    }
    .ns();
    let dp = DirtyParams {
        stale_fast_ms: 2_000,
        stale_slow_ms: 2_000,
        max_abs_spread: Rate(10_000_000),
        junk_after: 3,
    }
    .tsc(3_000_000);
    let tp = ThrottleParams {
        cooldown_ms: 1,
        max_consec_fail: 3,
        fallback_ms: 1,
        hard_rearm_ms: 1,
    }
    .ns();
    let now = common::now(10_000_000_000, 1);
    let mut trig = TriggerPx::default();
    let mut cw = ConfirmWindow::default();
    let mut th = Throttle::default();
    let mut ex = Expiring::new(0i64);
    let fr = Freshness {
        fast: LegFresh::from_quote(Tsc(1_000), true, false),
        slow: LegFresh::from_quote(Tsc(1_000), true, false),
        no_quote_streak: 0,
    };

    let s = CountingAlloc::scope();
    for i in 0..N {
        black_box(SpreadCalc::buy(Px(6_000_000 + i), Px(6_000_000)));
        black_box(SpreadCalc::sell(Px(6_000_000 - i), Px(6_000_000)));
        black_box(SpreadCalc::net(Rate(i), Rate(1)));
        black_box((cl.round_trip(), cl.open_floor(), cl.out_of_quote()));
        trig.update(
            Px(6_000_000 + i),
            Px(6_000_000),
            Rate(500_000),
            Rate(-200_000),
            Px(10),
            i,
        );
        black_box((trig.hit_buy(Px(6_003_000)), trig.hit_sell(Px(5_998_800))));
        black_box(cw.observe(i % 7 != 0, i % 2 == 0, i as u64, i * 1_000_000, &cp));
        let _ = black_box(DirtyFilter::check(Rate(i), &fr, Tsc(2_000 + i as u64), &dp));
        for k in [FireKind::Open, FireKind::Close, FireKind::Flatten, FireKind::Hard] {
            let _ = black_box(th.can_fire(k, i));
            th.on_fire(k, i, &tp);
        }
        black_box(th.on_outcome(Outcome::NoFill, i, &tp));
        th.set_halted(i % 3 == 0);
        th.set_backoff(i);
        black_box(th.in_fallback(i));
        th.reset_open_state();
        black_box(ImpactModel::vwap(&b, Side::Buy, Qty(150_000_000), &vp));
        ex.put(i, i, 1);
        ex.put_advice(i, 2, Tsc(30_000_000_000), 5, &now);
        black_box(ex.get(&now));
    }
    assert_eq!(s.allocs(), 0);

    for (name, sz) in [
        ("TriggerPx", size_of::<TriggerPx>()),
        ("ConfirmWindow", size_of::<ConfirmWindow>()),
        ("Throttle", size_of::<Throttle>()),
        ("LegFresh", size_of::<LegFresh>()),
        ("Freshness", size_of::<Freshness>()),
        ("DirtyParamsTsc", size_of::<DirtyParamsTsc>()),
        ("ThrottleParamsNs", size_of::<ThrottleParamsNs>()),
        ("ConfirmParamsNs", size_of::<ConfirmParamsNs>()),
        ("VwapParams", size_of::<VwapParams>()),
    ] {
        assert!(sz <= 128, "{name} is {sz} B");
    }
    assert!(size_of::<Expiring<i64>>() <= 136);
    assert_eq!(size_of::<CostLine>(), 168);
}
