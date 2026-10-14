//! hft-strat-lib fast-part benchmarks (README "延迟与资源契约"; SLB-136–SLB-143, SLB-146).

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use hft_strat_lib::{
    ConfirmParams, ConfirmWindow, CostItem, CostLine, DirtyFilter, DirtyParams, Expiring, FireKind,
    Freshness, ImpactModel, LegFresh, SpreadCalc, Throttle, ThrottleParams, TriggerPx, VwapParams,
    cost_flags,
};
use hft_strategy_sdk::prelude::Level;
use hft_strategy_sdk::{BookRead, BookStatus, EngineClockAnchor, Now};
use hft_types::{Px, Qty, Rate, Side, Tsc};

/// 20-level book on each side.
struct Book20 {
    bids: [Level; 20],
    asks: [Level; 20],
}

impl Book20 {
    fn new() -> Book20 {
        let lv = |px: i64| Level {
            px: Px(px),
            qty: Qty(10_000_000),
        };
        Book20 {
            bids: core::array::from_fn(|k| lv(6_000_000 - 100 * k as i64)),
            asks: core::array::from_fn(|k| lv(6_001_000 + 100 * k as i64)),
        }
    }
}

impl BookRead for Book20 {
    fn status(&self) -> BookStatus {
        BookStatus::Live
    }
    fn upd_tsc(&self) -> Tsc {
        Tsc(0)
    }
    fn best(&self, s: Side) -> Option<Level> {
        match s {
            Side::Buy => self.bids.first().copied(),
            Side::Sell => self.asks.first().copied(),
        }
    }
    fn levels(&self, s: Side, out: &mut [Level]) -> usize {
        let src = match s {
            Side::Buy => &self.bids,
            Side::Sell => &self.asks,
        };
        let n = src.len().min(out.len());
        out[..n].copy_from_slice(&src[..n]);
        n
    }
}

fn slb_136_spread(c: &mut Criterion) {
    let mut i = 0i64;
    c.bench_function("stratlib/spread", |b| {
        b.iter(|| {
            i = (i + 1) & 1023;
            black_box(SpreadCalc::buy(
                black_box(Px(6_003_000 + i)),
                black_box(Px(6_000_000)),
            ))
        })
    });
}

fn slb_137_cost_read(c: &mut Criterion) {
    let mut cl = CostLine::default();
    cl.set(
        &[
            CostItem::new(Rate(3_000_000), 1, cost_flags::ROUND_TRIP),
            CostItem::new(Rate(500_000), 2, cost_flags::FIXED),
        ],
        Rate(2_000_000),
    )
    .unwrap();
    c.bench_function("stratlib/cost_read", |b| {
        b.iter(|| black_box(black_box(&cl).open_floor()))
    });
}

fn slb_138_trigger_update(c: &mut Criterion) {
    let mut t = TriggerPx::default();
    let mut i = 0i64;
    c.bench_function("stratlib/trigger_update", |b| {
        b.iter(|| {
            i = (i + 1) & 1023;
            t.update(
                black_box(Px(6_000_000 + i)),
                black_box(Px(6_000_010 + i)),
                Rate(500_000),
                Rate(-200_000),
                Px(10),
                i,
            );
            black_box(&t);
        })
    });
}

fn slb_139_trigger_hit(c: &mut Criterion) {
    let mut t = TriggerPx::default();
    t.update(
        Px(6_000_000),
        Px(6_000_000),
        Rate(500_000),
        Rate(-200_000),
        Px(10),
        1,
    );
    let mut i = 0i64;
    c.bench_function("stratlib/trigger_hit", |b| {
        b.iter(|| {
            i = (i + 1) & 1023;
            black_box(black_box(&t).hit_buy(black_box(Px(6_002_500 + i))))
        })
    });
}

fn slb_140_confirm(c: &mut Criterion) {
    let p = ConfirmParams {
        min_ms: 100,
        min_updates: 3,
        min_slow_updates: 1,
        min_slow_span: 2,
    }
    .ns();
    let mut w = ConfirmWindow::default();
    let mut i = 0i64;
    c.bench_function("stratlib/confirm", |b| {
        b.iter(|| {
            i += 1;
            black_box(w.observe(i % 64 != 0, i % 3 == 0, i as u64, i * 1_000_000, &p))
        })
    });
}

fn slb_141_dirty(c: &mut Criterion) {
    let p = DirtyParams {
        stale_fast_ms: 2_000,
        stale_slow_ms: 1_000,
        max_abs_spread: Rate(10_000_000),
        junk_after: 3,
    }
    .tsc(3_000_000);
    let fr = Freshness {
        fast: LegFresh::from_quote(Tsc(1_000_000_000), true, false),
        slow: LegFresh::from_quote(Tsc(1_000_000_000), true, false),
        no_quote_streak: 0,
    };
    let mut i = 0u64;
    c.bench_function("stratlib/dirty", |b| {
        b.iter(|| {
            i = (i + 1) & 1023;
            black_box(DirtyFilter::check(
                Rate(i as i64),
                &fr,
                Tsc(1_000_000_000 + i),
                &p,
            ))
        })
    });
}

fn slb_142_throttle(c: &mut Criterion) {
    let p = ThrottleParams {
        cooldown_ms: 500,
        max_consec_fail: 3,
        fallback_ms: 60_000,
        hard_rearm_ms: 1_000,
    }
    .ns();
    let mut t = Throttle::default();
    t.on_fire(FireKind::Flatten, 0, &p);
    let mut i = 0i64;
    c.bench_function("stratlib/throttle", |b| {
        b.iter(|| {
            i = (i + 1) & 1023;
            black_box(black_box(&t).can_fire(FireKind::Open, i))
        })
    });
}

fn slb_143_vwap20(c: &mut Criterion) {
    let book = Book20::new();
    let p = VwapParams {
        book_frac: Rate(500_000_000),
        max_levels: 20,
    };
    c.bench_function("stratlib/vwap20", |b| {
        b.iter(|| {
            black_box(ImpactModel::vwap(
                &book,
                Side::Buy,
                black_box(Qty(195_000_000)),
                &p,
            ))
        })
    });
}

fn slb_146_expiring(c: &mut Criterion) {
    let clk = EngineClockAnchor {
        base_engine_ns: 5_000_000_000,
        base_tsc: Tsc(0),
        tsc_hz_milli: 3_000_000_000_000,
        utc_tsc: Tsc(0),
        utc_ns: 0,
    };
    let now = Now::new(Tsc(30_000_000_000), 100, 50, true, clk);
    let mut e = Expiring::new(0i64);
    e.put(7, 20_000_000_000, 60);
    c.bench_function("stratlib/expiring", |b| {
        b.iter(|| black_box(black_box(&e).get(black_box(&now))))
    });
}

fn slb_144_funding_observe(c: &mut Criterion) {
    let mut stats = hft_strat_lib::FundingStats::new();
    let f = hft_strategy_sdk::FundingView {
        rate: Rate(100_000),
        next_settle_ns: 28_800_000_000_000,
        interval_s: 28_800,
        ..hft_strategy_sdk::FundingView::EMPTY
    };
    c.bench_function("stratlib/funding_observe", |b| {
        b.iter(|| black_box(stats.observe(black_box(&f), 1)))
    });
}
fn slb_145_funding_summary(c: &mut Criterion) {
    let mut stats = hft_strat_lib::FundingStats::new();
    for i in 1..=257 {
        stats.observe(
            &hft_strategy_sdk::FundingView {
                rate: Rate(100_000),
                next_settle_ns: i * 3_600_000_000_000,
                interval_s: 3_600,
                ..hft_strategy_sdk::FundingView::EMPTY
            },
            i * 3_600_000_000_000 - 1,
        );
    }
    c.bench_function("stratlib/funding_summary", |b| {
        b.iter(|| {
            black_box(stats.summary(
                300 * 3_600_000_000_000,
                black_box(257 * 3_600_000_000_000),
                3_600_000_000_000,
            ))
        })
    });
}

criterion_group!(
    benches,
    slb_136_spread,
    slb_137_cost_read,
    slb_138_trigger_update,
    slb_139_trigger_hit,
    slb_140_confirm,
    slb_141_dirty,
    slb_142_throttle,
    slb_143_vwap20,
    slb_146_expiring,
    slb_144_funding_observe,
    slb_145_funding_summary
);
criterion_main!(benches);
