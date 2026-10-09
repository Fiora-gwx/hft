//! hft-sim benchmarks (README "延迟与资源契约"; SIM-210, SIM-211).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use hft_event::{GwCmd, GwOp, L2BatchHdr, L2BatchView, L2SnapHdr, L2SnapshotPart, Level};
use hft_sim::{MarketEventRef, SimExchange, SimInput};
use hft_types::{Px, Qty, Side, SlotId, Tif, Tsc};

#[path = "../tests/common/mod.rs"]
mod common;

use common::*;

fn sim_210_on_l2(c: &mut Criterion) {
    let mut ex = SimExchange::new(&setup_sim(), &rd_sim()).unwrap();
    // Live incremental-depth book (Exchange snapshot), then batches of 10 levels.
    let bids: Vec<Level> = (1..=20)
        .map(|k| Level {
            px: Px(600_000_000 - k * 100),
            qty: q("1"),
        })
        .collect();
    let asks: Vec<Level> = (0..20)
        .map(|k| Level {
            px: Px(600_000_000 + k * 100),
            qty: q("1"),
        })
        .collect();
    let sh = L2SnapHdr {
        h: mhdr(S1, BTC_S1, 1),
        snap_id: 1,
        origin: L2SnapHdr::EXCHANGE,
        _pad: [0; 3],
        n_bids: 20,
        n_asks: 20,
    };
    let part = L2SnapshotPart {
        hdr: sh,
        bids: &bids,
        asks: &asks,
        part: 0,
        parts: 1,
    };
    ex.on_record(&rhdr(1), SimInput::Market(MarketEventRef::L2Snapshot(part)));
    let mut id = 1u64;
    let mut t = 2u64;
    c.bench_function("sim/on_l2", |b| {
        b.iter(|| {
            let lv: [Level; 10] = core::array::from_fn(|k| Level {
                px: Px(600_000_000 + (k as i64) * 100),
                qty: Qty(((id + k as u64) % 7 + 1) as i64 * 1_000_000),
            });
            let hdr = L2BatchHdr {
                h: mhdr(S1, BTC_S1, t as i64),
                prev_id: id,
                last_id: id + 1,
                n_bids: 0,
                n_asks: 10,
                bflags: 0,
                _pad: 0,
            };
            id += 1;
            t += 1_000;
            ex.on_record(
                &rhdr(t),
                SimInput::Market(MarketEventRef::L2Batch(L2BatchView { hdr, levels: &lv })),
            );
            black_box(&ex);
        })
    });
}

fn sim_211_place_match(c: &mut Criterion) {
    let mut setup = setup_sim();
    setup.tuning.consume_decay_ms = 0;
    let mut ex = SimExchange::new(&setup, &rd_sim()).unwrap();
    let asks: Vec<Level> = (0..10)
        .map(|k| Level {
            px: Px(600_000_000 + k * 100),
            qty: q("0.001"),
        })
        .collect();
    let sh = L2SnapHdr {
        h: mhdr(S1, BTC_S1, 1),
        snap_id: 1,
        origin: L2SnapHdr::TOP_N,
        _pad: [0; 3],
        n_bids: 0,
        n_asks: 10,
    };
    let part = L2SnapshotPart {
        hdr: sh,
        bids: &[],
        asks: &asks,
        part: 0,
        parts: 1,
    };
    ex.on_record(&rhdr(1), SimInput::Market(MarketEventRef::L2Snapshot(part)));
    let mut t = 10u64;
    let mut slot = 0u32;
    c.bench_function("sim/place_match", |b| {
        b.iter(|| {
            slot += 1;
            let cmd = GwCmd {
                decision_seq: 0,
                decided_tsc: Tsc(t),
                px: Px(600_000_000 + 900),
                qty: q("0.01"),
                inst: BTC_S1,
                slot: SlotId(slot),
                r#gen: 1,
                target_slot: SlotId(0),
                op: GwOp::Place,
                account: A9,
                side: Side::Buy,
                tif: Tif::Ioc,
                oflags: 0,
                _pad: [0; 2],
                arg: 0,
                arg2: 0,
            };
            ex.on_command(&cmd, Tsc(t));
            t += ms(10);
            ex.advance(Tsc(t));
            ex.drain_due(Tsc(t), |r| {
                black_box(r.msg);
            });
            // Keep the balances from running out.
            if slot.is_multiple_of(1_000) {
                ex.on_command(&GwCmd::sim_reset(A9), Tsc(t));
                ex.drain_due(Tsc(t), |_| {});
            }
        })
    });
}

criterion_group!(benches, sim_210_on_l2, sim_211_place_match);
criterion_main!(benches);
