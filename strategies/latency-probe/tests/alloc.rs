//! Zero allocation of the fast part and the group state size (TESTS.md LPR-063).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::type_complexity
)] // allow-list: test code

mod common;

use common::*;
use hft_strategy_sdk::{
    BrokenReason, CapScopeView, CtxBufs, EventRef, ExecUpdateKind, ExternalFillView, NoticeKind, OrderUpdate,
    RiskEvent, Strategy, StuckReason, VenueNoticeView,
};
use hft_testkit::CountingAlloc;
use hft_types::{CapSource, InstId, Px, Qty, RunMode, Side, StrategyId, Tsc, VenueId};
use latency_probe::ProbeGroup;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

/// Delivers `e` 1 000 times with fresh time steps; returns the allocations made inside `on_event`.
fn drive(h: &mut H, bufs: &mut CtxBufs, mk: &dyn Fn(&H, i64) -> ev::Ev) -> u64 {
    let mut n = 0;
    for k in 0..1_000i64 {
        let t = h.now().engine_ns - E0;
        h.at_ens(E0 + t + 1_100_000_000 + k);
        let e = mk(h, k);
        let r = e.as_ref();
        bufs.begin();
        let s = CountingAlloc::scope();
        {
            let mut ctx = bufs.ctx();
            h.s.on_event(&r, &h.v, &mut ctx);
        }
        n += s.allocs();
        // Feed back plausible outcomes (placed refs) so the next call reads them.
        let ops: Vec<_> = bufs.ops().map(|(hd, _)| hd).collect();
        bufs.clear_outcomes();
        for (i, hd) in ops.into_iter().enumerate() {
            bufs.set_outcome(
                hd,
                hft_strategy_sdk::IntentOutcome::Placed(r_of(1_000 + i as u32, V2)),
            );
        }
    }
    n
}

type Mk = Box<dyn Fn(&H, i64) -> ev::Ev>;

fn events(g: hft_types::GroupId, a: InstId) -> Vec<Mk> {
    let scope = CapScopeView {
        strategy: StrategyId(1),
        venue: VenueId(0),
        account: T1,
        group: g,
        inst: InstId(0),
    };
    let upd = move |kind| OrderUpdate {
        r: r_of(1, V2),
        group: g,
        tag: probe_tag(0),
        kind,
    };
    vec![
        Box::new(move |h: &H, _| {
            let mut b = h.bbo_view(a);
            b.rx_tsc = h.now().tsc;
            ev::bbo(a, 0, b)
        }),
        Box::new(move |_, k| ev::order(0, upd(if k % 2 == 0 { acked() } else { cancelled() }))),
        Box::new(move |_, k| ev::order(0, upd(filled(100_000, 5_820_000, k % 2 == 0)))),
        Box::new(move |_, k| {
            let kinds = [
                ExecUpdateKind::Done {
                    cleared_by_engine: k % 3 == 0,
                },
                ExecUpdateKind::Stuck(StuckReason::SliceFails),
                ExecUpdateKind::Broken(BrokenReason::LinkBroken),
            ];
            ev::exec(
                0,
                hft_strategy_sdk::ExecUpdate {
                    group: g,
                    tag: 0,
                    origin: hft_strategy_sdk::TargetOrigin::Strategy,
                    kind: kinds[(k % 3) as usize],
                    target: [Qty(0); 4],
                    filled: [Qty(0); 4],
                    inflight: [Qty(0); 4],
                },
            )
        }),
        Box::new(move |_, _| {
            ev::external(
                0,
                ExternalFillView {
                    venue: V2,
                    inst: a,
                    group: g,
                    leg_ix: 0,
                    side: Side::Buy,
                    qty: Qty(100_000),
                    px: Px(6_000_000),
                    fee: 0,
                    fee_asset: USDT,
                    cause: hft_event::ExternalCause::Adl,
                    ex_ts_ns: U,
                    deadline_tsc: Tsc(0),
                },
            )
        }),
        Box::new(move |_, k| {
            let e = match k % 3 {
                0 => RiskEvent::CapSet {
                    source: CapSource::HALTED,
                    scope,
                },
                1 => RiskEvent::CapCleared {
                    source: CapSource::HALTED,
                    scope,
                },
                _ => RiskEvent::HedgeBroken { group: g },
            };
            ev::risk(T1, e)
        }),
        Box::new(move |_, _| {
            let n = VenueNoticeView {
                venue: V3,
                inst: I3,
                kind: NoticeKind::Maintenance { from_ns: 0, to_ns: 1 },
            };
            ev::notice(I3, 0, n)
        }),
        Box::new(move |_, k| ev::timer(1 + (k % 2) as u32, g, 0)),
        Box::new(move |_, k| {
            let (o, n) = if k % 2 == 0 {
                (RunMode::Live, RunMode::CloseOnly)
            } else {
                (RunMode::CloseOnly, RunMode::Live)
            };
            ev::mode(0, o, n)
        }),
        Box::new(move |_, _| ev::conn(V2, hft_event::ConnStateKind::Up)),
    ]
}

#[test]
fn lpr_063_zero_alloc_state_size() {
    assert!(CountingAlloc::is_installed());
    for (grp, a) in [(gq(), I2), (gpp(), I2), (gts(), I1)] {
        let gid = grp.spec.id;
        let mut h = H::new(&[], vec![grp]);
        h.net(gid, 0, 100_000).net(gid, 1, -100_000);
        let mut bufs = CtxBufs::new();
        for (i, mk) in events(gid, a).iter().enumerate() {
            let n = drive(&mut h, &mut bufs, mk.as_ref());
            assert_eq!(n, 0, "group {gid:?} event #{i}");
        }
        let _ = EventRef::Conn {
            venue: V1,
            state: hft_event::ConnStateKind::Up,
        };
        let _ = <latency_probe::LatencyProbe as Strategy>::DESC;
    }
    assert!(
        core::mem::size_of::<ProbeGroup>() <= 256,
        "{}",
        core::mem::size_of::<ProbeGroup>()
    );
}
