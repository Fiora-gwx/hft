//! EXE-475: every public entry except `on_groups_changed` / `restore` allocates nothing after `new`.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod common;
use common::*;

use hft_exec::*;
use hft_strategy_sdk::{
    ExternalCause, MarginLevel, OrderRef, OrderUpdate, OrderUpdateKind, SnapshotWriter, TargetIntent, Views,
};
use hft_testkit::alloc::CountingAlloc;
use hft_types::{AccountId, CapSource, RunMode, Side, Tsc};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

#[test]
fn exe_475_zero_alloc() {
    assert!(CountingAlloc::is_installed());
    let mut fx = fx_many(64);
    for k in 0..64 {
        fx.legs(A1, many(k), &["1.00", "-1.0"]);
        fx.spot(A1, A, many(k), "1", "1");
    }
    // Everything the loop needs is built before counting starts.
    let tgs: Vec<TargetIntent> = (0..128u32)
        .map(|k| {
            let s = if k % 2 == 0 { "3" } else { "2" };
            let p = if k % 2 == 0 { "-3" } else { "-2" };
            let mut t = t0(s, p);
            t.group = many(k / 2);
            t
        })
        .collect();
    let links: Vec<LinkNotice> = (0..64u32)
        .map(|k| {
            let ph = [LinkPhase::Open, LinkPhase::Done, LinkPhase::Broken][k as usize % 3];
            link(k + 1, A1, many(k), ph, LinkBrokenCause::Deadline)
        })
        .collect();
    let xfs: Vec<_> = (0..64u32)
        .map(|k| {
            let c = [
                ExternalCause::Liquidation,
                ExternalCause::Manual,
                ExternalCause::Adl,
            ][k as usize % 3];
            xfill(many(k), 1, P, B, Side::Buy, "0.1", c)
        })
        .collect();
    let risks = [
        ms_margin(MarginLevel::L1),
        ms_margin(MarginLevel::L3),
        cap_set(CapSource::BROKEN, A1, many(3)),
        cap_clr(CapSource::BROKEN, A1, many(3)),
        ms_margin(MarginLevel::None),
    ];
    let frac = rate("0.2");
    let mut w = SnapshotWriter::new(4 << 20);
    let base = Views::now(&fx.v.m).tsc.0;
    let mut sink = 0u64;
    let mut n_orders = 0u32;
    let sc = CountingAlloc::scope();
    for i in 0..1000u64 {
        let k = (i % 64) as u32;
        let g = many(k);
        fx.v.m.at_tsc(Tsc(base + i * 1_000_000));
        fx.v.m.seq(10 + i);
        let now = Views::now(&fx.v.m);
        let tg = &tgs[(i % 128) as usize];
        let _ = fx.ex.set_target(I1, tg, i % 7 == 0, &fx.v, now, &mut fx.out);
        fx.ex.advance(&fx.v, now, &mut fx.out);
        let first = fx.out.orders.as_slice().first().copied();
        fx.out.clear();
        if let Some(o) = first {
            n_orders += 1;
            let r = OrderRef::new(o.intent.venue, o.intent.account, i as u32 + 1, 1);
            let res = if i % 5 == 0 {
                PlaceResult::Rejected(hft_types::RejectBits::RATE)
            } else {
                PlaceResult::Placed(r)
            };
            fx.ex.on_place_result(o.id, res, now, &mut fx.out);
            let kind = if i % 3 == 0 {
                OrderUpdateKind::Cancelled
            } else {
                OrderUpdateKind::Filled {
                    d: o.intent.qty,
                    px: o.intent.px,
                    fee: 0,
                    fee_asset: USDT,
                    cum: o.intent.qty,
                    done: true,
                }
            };
            let u = OrderUpdate {
                r,
                group: o.intent.group,
                tag: 0,
                kind,
            };
            fx.ex.on_order_update(&u, &fx.v, now, &mut fx.out);
        }
        fx.ex.on_link(&links[k as usize], now, &mut fx.out);
        let token = u64::from(ExecTimerReq::id(ExecTimerKind::Wake, A1)) | (u64::from(g.0) << 32);
        fx.ex.on_timer(token, now, &mut fx.out);
        fx.ex.on_timer(4, now, &mut fx.out);
        fx.ex.on_risk(A1, &risks[(i % 5) as usize], now);
        fx.ex
            .on_mode(A1, g, RunMode::Live, RunMode::Live, now, &mut fx.out);
        fx.ex.touch_inst(if i % 2 == 0 { S } else { P });
        fx.ex.on_leg_adjust(A1, g);
        sink ^= fx.ex.on_external(A1, &xfs[k as usize], &fx.v, now, &mut fx.out).0;
        fx.ex.on_venue_gate_open(A1, B, now);
        fx.ex.on_gate_open(A1, now);
        let _ = fx.ex.hedge_fix_cmd(A1, g, &fx.v, now, &mut fx.out);
        sink ^= u64::from(fx.ex.resolve_broken(A1, g, &fx.v));
        fx.ex.note_strategy_action(A1, g, now, &mut fx.out);
        if i % 4 == 0 {
            fx.ex.clear_target(I1, A1, g, now, &mut fx.out);
        }
        if i % 100 == 50 {
            fx.ex.force_deleverage(A1, B, frac, now, &mut fx.out);
        }
        if i % 250 == 249 {
            fx.ex.clear_account(A1, now, &mut fx.out);
            fx.ex.engine_flatten(A2, now, &mut fx.out);
        }
        fx.ex.advance(&fx.v, now, &mut fx.out);
        w.clear();
        fx.ex.snapshot(&mut w);
        sink ^= w.len() as u64;
        if i % 100 == 99 {
            fx.ex
                .recovery_end(now.tsc, now.tsc, 1_000_000_000_000, 1_000_000_000_000);
            fx.ex.on_gate_open(A1, now);
        }
        sink ^= fx.ex.recovery_timers().count() as u64;
        sink ^= u64::from(fx.ex.drain_ready()) ^ u64::from(fx.ex.drain_ready_acct(A1));
        sink ^= u64::from(fx.ex.engine_reduce_pending()) ^ u64::from(fx.ex.has_pending());
        sink ^= fx.ex.view(A1, g).map_or(0, |x| x.set_seq);
        sink ^= fx.ex.cell(A1, g).map_or(0, |c| u64::from(c.n_legs));
        sink ^= fx.ex.acct_state(AccountId(1)).map_or(0, |s| s.force_venues);
        sink ^= fx.ex.cix(A1, g).map_or(0, u64::from) ^ fx.ex.cell_count() as u64 ^ fx.ex.capacity() as u64;
        fx.out.clear();
    }
    assert_eq!(sc.allocs(), 0);
    assert_ne!(sink, 1);
    assert!(n_orders > 10, "{n_orders}");
}
