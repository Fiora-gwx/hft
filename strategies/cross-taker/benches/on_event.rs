//! Real strategy entry points; fixture/reset and output inspection are outside the timed region.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: bench code

use std::hint::black_box;
use std::time::{Duration, Instant};

use criterion::{Criterion, criterion_group, criterion_main};
use cross_taker::{CrossTaker, XtParams};
use hft_strategy_sdk::{
    params::{RawParams, encode_body},
    prelude::*,
    testing::{MockViews, RefFixture, ev},
};
use hft_testkit::dst::{
    seq::build,
    world::{ACCT_A, G1, P1, S1, VP, WorldSpec},
};
use zerocopy::FromZeros;

// The same synthetic two-venue fixture and parameters as registry/tests/cross_taker_e2e.rs.
fn fixture() -> (CrossTaker, MockViews) {
    let mut w = WorldSpec::w1();
    w.accounts.truncate(1);
    w.groups.truncate(1);
    w.strategies.truncate(1);
    let other = VenueId(10);
    for v in &mut w.venues {
        if v.id == VP {
            v.id = other;
        }
    }
    for i in &mut w.insts {
        if i.venue == VP {
            i.venue = other;
        }
    }
    for a in &mut w.accounts {
        for v in &mut a.venues {
            if *v == VP {
                *v = other;
            }
        }
        for (v, _, _) in &mut a.balances {
            if *v == VP {
                *v = other;
            }
        }
    }
    let rd = build::load_refdata(&build::refdata_blob(&w, false).unwrap()).unwrap();
    let leg = |i| LegDef {
        inst: i,
        venue: rd.inst(i).venue,
        kind: if i == S1 { 1 } else { 2 },
        _pad: 0,
        route: 0,
        _pad2: 0,
    };
    let groups = GroupTable::new(
        &[GroupSpec {
            id: G1,
            asset: rd.inst(S1).asset,
            n_legs: 2,
            open_allowed: 1,
            legs: [leg(S1), leg(P1), LegDef::new_zeroed(), LegDef::new_zeroed()],
            _rsv: 0,
        }],
        4,
    );
    let raw = RawParams {
        defaults: vec![
            1,
            1_000_000,
            0,
            500_000_000,
            120_000_000,
            240_000_000,
            1000,
            10,
            3,
        ],
        overrides: vec![],
    };
    let bytes = encode_body(&XtParams::SCHEMA, 1, &raw).unwrap();
    let params = XtParams::decode(&bytes, 1, &groups).unwrap();
    let strategy = CrossTaker::new(
        StrategyInstance {
            strategy: StrategyId(1),
            account: ACCT_A,
            kind_id: 3,
        },
        &params,
        &groups,
        &RefView::new(&rd, ACCT_A),
    )
    .unwrap();
    let mut views = MockViews::new(RefFixture::new(rd, ACCT_A));
    views.at(Now::new(
        Tsc(10_000_000),
        20,
        1,
        true,
        EngineClockAnchor {
            base_engine_ns: 0,
            base_tsc: Tsc(0),
            tsc_hz_milli: 1_000_000_000_000,
            utc_tsc: Tsc(0),
            utc_ns: hft_testkit::dst::world::UTC0,
        },
    ));
    for (i, bid, ask) in [(S1, 599_999_900, 600_000_000), (P1, 606_000_000, 606_001_000)] {
        let venue = Views::inst(&views, i).venue;
        MockViews::bbo(
            &mut views,
            i,
            (Px(bid), Qty(200_000_000)),
            (Px(ask), Qty(200_000_000)),
        );
        views.conn(i, Tsc(10_000_000), true, false);
        MockViews::venue_ready(&mut views, ACCT_A, venue, true);
    }
    MockViews::risk(
        &mut views,
        ACCT_A,
        RiskStateView {
            conn_liveness_ms: 1000,
            ..RiskStateView::EMPTY
        },
    );
    MockViews::mode(&mut views, ACCT_A, G1, RunMode::Live);
    (strategy, views)
}

fn measure(c: &mut Criterion, name: &str, mut strategy: CrossTaker, views: MockViews, event: ev::Ev) {
    let mut snapshot = SnapshotWriter::new(1024);
    strategy.snapshot(&mut snapshot);
    let mut bufs = CtxBufs::new();
    c.bench_function(name, |b| {
        b.iter_custom(|n| {
            let mut elapsed = Duration::ZERO;
            for _ in 0..n {
                // Restore the initial state so every fire/broken sample exercises real work.
                strategy
                    .restore(&mut SnapshotReader::with_desc_version(
                        snapshot.as_bytes(),
                        CrossTaker::DESC.version,
                    ))
                    .unwrap();
                bufs.begin();
                let mut ctx = bufs.ctx();
                let event = event.as_ref();
                let start = Instant::now();
                strategy.on_event(black_box(&event), black_box(&views), &mut ctx);
                elapsed += start.elapsed();
                black_box(ctx);
            }
            elapsed
        });
    });
}

fn on_event(c: &mut Criterion) {
    let (strategy, mut views) = fixture();
    MockViews::bbo(
        &mut views,
        P1,
        (Px(599_999_900), Qty(200_000_000)),
        (Px(600_000_000), Qty(200_000_000)),
    );
    let event = ev::bbo(P1, 0, Views::bbo(&views, P1));
    measure(c, "strategies/cross-taker/tick_miss", strategy, views, event);

    let (strategy, views) = fixture();
    let event = ev::bbo(P1, 0, Views::bbo(&views, P1));
    measure(c, "strategies/cross-taker/tick_fire", strategy, views, event);

    let (strategy, views) = fixture();
    let event = ev::exec(
        0,
        ExecUpdate {
            group: G1,
            tag: 1,
            origin: TargetOrigin::Strategy,
            kind: ExecUpdateKind::Broken(BrokenReason::LinkBroken),
            target: [Qty(0); 4],
            filled: [Qty(0); 4],
            inflight: [Qty(0); 4],
        },
    );
    measure(c, "strategies/cross-taker/exec_update", strategy, views, event);

    let (strategy, views) = fixture();
    measure(
        c,
        "strategies/cross-taker/risk_event",
        strategy,
        views,
        ev::risk(ACCT_A, RiskEvent::HedgeBroken { group: G1 }),
    );
}

criterion_group!(benches, on_event);
criterion_main!(benches);
