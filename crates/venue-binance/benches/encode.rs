//! venue-binance encode benchmark (README "延迟与资源契约"; BIN-505).

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use hft_event::{GwCmd, GwOp};
use hft_types::{Px, Qty, Side, SlotId, Tif, Tsc, VenueId};
use hft_venue_proto::{TargetRef, VenueProtocol};
use venue_binance::Usdm;

#[path = "../tests/support/mod.rs"]
mod support;

use support::{ACCT, U_BTC, WC, cfg, cid, hm1, rd};

fn bin_505_encode_place(c: &mut Criterion) {
    let r = rd();
    let t = Usdm::build_templates(&cfg(1500), r, ACCT, &hm1()).unwrap();
    let cmd = GwCmd {
        decision_seq: 0,
        decided_tsc: Tsc(0),
        px: Px(6_500_010),
        qty: Qty(400_000),
        inst: U_BTC,
        slot: SlotId(42),
        r#gen: 7,
        target_slot: SlotId(0),
        op: GwOp::Place,
        account: ACCT,
        side: Side::Buy,
        tif: Tif::PostOnly,
        oflags: 0,
        _pad: [0; 2],
        arg: 0,
        arg2: 0,
    };
    let tgt = TargetRef {
        id: cid(VenueId::BINANCE_USDM, 3, 42, 7),
        exch_order_id: 0,
    };
    let mut body = [0u8; 2048];
    c.bench_function("bench_encode_place", |b| {
        b.iter(|| Usdm::encode(r, &t, black_box(&cmd), &tgt, &WC, &mut body))
    });
}

criterion_group!(benches, bin_505_encode_place);
criterion_main!(benches);
