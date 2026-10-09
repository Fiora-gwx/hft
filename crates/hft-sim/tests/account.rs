//! SIM-005–SIM-006: account isolation and zero allocation after `new`.

#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod common;

use common::*;
use hft_event::{Bbo, GwCmd, GwOp, L2SnapHdr, L2SnapshotPart, Level, MarkPrice, Trade};
use hft_sim::{MarketEventRef, SimInput};
use hft_testkit::CountingAlloc;
use hft_types::{Px, Qty, Side, SlotId, Tif, Tsc};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

#[test]
fn sim_005_accounts_isolated() {
    let mut s = Sim::with(&setup_with_a8(), &rd_sim());
    s.topn(ms(100), BTC_S1, &[("60000", "0.1")], &[]);
    let o9 = s.place(A9, BTC_S1, Buy, "0.1", "60000", Tif::Ioc, ms(100));
    let o8 = s.place(A8, BTC_S1, Buy, "0.1", "60000", Tif::Ioc, ms(100));
    s.run_to(ms(110));
    assert_eq!(s.fills_of(A9, o9), vec![(px("60000"), q("0.1"))]);
    assert_eq!(s.fills_of(A8, o8), vec![(px("60000"), q("0.1"))]);
    assert_eq!(s.total(A9, S1, USDT), usdt("100000").0 - usdt("6002.4").0);
    assert_eq!(s.total(A8, S1, USDT), usdt("50000").0 - usdt("6002.4").0);
    assert_eq!(s.total(A8, P2, USDT), usdt("10000").0);
}

#[test]
fn sim_006_zero_alloc() {
    assert!(CountingAlloc::is_installed());
    let mut seed = seed9_state("18000", &[(BTC_P2, "-0.1", "60000")]);
    seed.balances.push((S1, USDT, usdt("100000")));
    seed.balances.push((S1, BTC, q("1")));
    let mut s = Sim::with(&setup_sim(), &rd_sim());
    s.seed(A9, &seed);
    // Inputs prepared before counting.
    let asks: Vec<Level> = (0..10)
        .map(|k| Level {
            px: Px(px("60000").0 + k * 100),
            qty: q("0.5"),
        })
        .collect();
    let bids: Vec<Level> = (0..10)
        .map(|k| Level {
            px: Px(px("59999").0 - k * 100),
            qty: q("0.5"),
        })
        .collect();
    let ex = &mut s.ex;
    let scope = CountingAlloc::scope();
    let mut delivered = 0u64;
    let mut t = ms(100);
    for i in 0..100_000u64 {
        t += 100_000;
        let h = mhdr(
            if i % 5 == 3 { P2 } else { S1 },
            if i % 5 == 3 { BTC_P2 } else { BTC_S1 },
            t as i64,
        );
        match i % 5 {
            0 => {
                let hdr = L2SnapHdr {
                    h,
                    snap_id: i + 1,
                    origin: L2SnapHdr::TOP_N,
                    _pad: [0; 3],
                    n_bids: 10,
                    n_asks: 10,
                };
                let p = L2SnapshotPart {
                    hdr,
                    bids: &bids,
                    asks: &asks,
                    part: 0,
                    parts: 1,
                };
                ex.on_record(&rhdr(t), SimInput::Market(MarketEventRef::L2Snapshot(p)));
            }
            1 => {
                let b = Bbo {
                    h,
                    bid_px: px("59999"),
                    bid_qty: q("1"),
                    ask_px: px("60000"),
                    ask_qty: q("1"),
                    update_id: 0,
                };
                ex.on_record(&rhdr(t), SimInput::Market(MarketEventRef::Bbo(&b)));
            }
            2 => {
                let tr = Trade {
                    h,
                    px: px("59000"),
                    qty: q("0.01"),
                    trade_id: i,
                    taker_side: Side::Sell,
                    _pad: [0; 7],
                };
                ex.on_record(&rhdr(t), SimInput::Market(MarketEventRef::Trade(&tr)));
            }
            3 => {
                let m = MarkPrice {
                    h,
                    px: Px(px("60000").0 + ((i % 7) as i64) * 10_000),
                };
                ex.on_record(&rhdr(t), SimInput::Market(MarketEventRef::Mark(&m)));
            }
            _ => ex.on_record(&rhdr(t), SimInput::Clock),
        }
        if i % 10 == 0 {
            let k = i / 10;
            let (op, side, tif, p) = match k % 6 {
                0 => (GwOp::Place, Side::Buy, Tif::Ioc, px("60010")),
                1 => (GwOp::Place, Side::Sell, Tif::Ioc, px("59990")),
                2 => (GwOp::Place, Side::Buy, Tif::Gtc, px("59500")),
                3 => (GwOp::Cancel, Side::Buy, Tif::Gtc, Px(0)),
                4 => (GwOp::Query, Side::Buy, Tif::Gtc, Px(0)),
                _ => (GwOp::Place, Side::Buy, Tif::Fok, px("60010")),
            };
            let slot = (k + 1) as u32;
            let c = GwCmd {
                decision_seq: k,
                decided_tsc: Tsc(t),
                px: p,
                qty: q("0.001"),
                inst: BTC_S1,
                slot: SlotId(slot),
                r#gen: 1,
                target_slot: SlotId(slot.saturating_sub(1)),
                op,
                account: A9,
                side,
                tif,
                oflags: 0,
                _pad: [0; 2],
                arg: 0,
                arg2: 0,
            };
            ex.on_command(&c, Tsc(t));
        }
        if i % 100 == 0 {
            ex.drain_due(Tsc(t), |_| delivered += 1);
        }
    }
    ex.advance(Tsc(t + ms(1_000)));
    ex.drain_due(Tsc(u64::MAX), |_| delivered += 1);
    assert_eq!(scope.allocs(), 0);
    assert!(delivered > 10_000);
    let _ = Qty(0);
}
