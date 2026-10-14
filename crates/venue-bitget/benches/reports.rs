//! Private-report benchmark: fresh fills, dedup hits, and external fills are separate samples.
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use hft_venue_proto::{MsgOp, VenueProtocol};
use std::hint::black_box;
#[path = "support/sink.rs"]
mod sink;
#[path = "../tests/support/mod.rs"]
mod support;
use venue_bitget::{BitgetUsdtFutures as Fut, GwCtx};
const PAYLOAD: &str = r#"{"action":"snapshot","arg":{"instType":"UTA","topic":"order"},"data":[{"category":"usdt-futures","symbol":"BTCUSDT","orderId":"1234567890123","clientOid":"100030a020002a000000078d","price":"65000.1","qty":"0.004","tradeSide":"buy_single","orderType":"limit","timeInForce":"gtc","side":"buy","cumExecQty":"0.004","cumExecValue":"260","avgPrice":"65000","orderStatus":"filled","cancelReason":"","feeDetail":[{"feeCoin":"USDT","fee":"0.04"}],"createdTime":"1790000000400","updatedTime":"1790000000500"}],"ts":1790000000501}"#;
fn bgt_504_decode_order(c: &mut Criterion) {
    let rd = support::rd();
    for (label, payload) in [
        ("own", PAYLOAD.to_owned()),
        (
            "external",
            PAYLOAD.replace("100030a020002a000000078d", "web_external"),
        ),
    ] {
        let class = Fut::classify_report(MsgOp::Text, &payload.as_bytes()[..64]);
        let mut ctx = GwCtx::new();
        let mut sink = sink::CountSink::default();
        assert_eq!(
            Fut::decode_report(rd, &mut ctx, class, payload.as_bytes(), &support::RX, &mut sink)
                .unwrap()
                .events,
            1
        );
        c.bench_function(&format!("bench_decode_bitget_order/{label}"), |b| {
            b.iter_batched(
                || (GwCtx::new(), sink::CountSink::default()),
                |(mut ctx, mut sink)| {
                    black_box(Fut::decode_report(
                        rd,
                        &mut ctx,
                        class,
                        black_box(payload.as_bytes()),
                        &support::RX,
                        &mut sink,
                    ))
                    .unwrap();
                },
                BatchSize::PerIteration,
            )
        });
        c.bench_function(&format!("bench_decode_bitget_order/{label}-duplicate"), |b| {
            b.iter(|| {
                let out = Fut::decode_report(
                    rd,
                    &mut ctx,
                    class,
                    black_box(payload.as_bytes()),
                    &support::RX,
                    &mut sink,
                )
                .unwrap();
                assert_eq!(out.events, 0);
                black_box(out);
            })
        });
    }
}
use hft_event::{GwCmd, GwOp};
use hft_types::{Px, Qty, Side, SlotId, Tif, Tsc};
use hft_venue_proto::TargetRef;
use support::{ACCT, F_BTC, WC, cfg, cid, key, rd};
fn bgt_503_encode_place(c: &mut Criterion) {
    let r = rd();
    let t = Fut::build_templates(&cfg(), r, ACCT, &key()).unwrap();
    let cmd = GwCmd {
        decision_seq: 0,
        decided_tsc: Tsc(0),
        px: Px(6_500_010),
        qty: Qty(400_000),
        inst: F_BTC,
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
        id: cid(support::FUT, 3, 42, 7),
        exch_order_id: 0,
    };
    let mut body = [0u8; 2048];
    c.bench_function("bench_encode_bitget_place", |b| {
        b.iter(|| Fut::encode(r, &t, black_box(&cmd), &tgt, &WC, &mut body))
    });
}

// Warm bootstrap decoder: allocations are included, never interpreted as a hot acct_* result.
fn bgt_505_account_positions(c: &mut Criterion) {
    use hft_venue_proto::account::{AccountProtocol, Query, State};
    let rd = support::rd();
    let mut rows = Vec::new();
    let body = loop {
        let i = rd
            .insts_of_venue(Fut::VENUE)
            .get(rows.len())
            .expect("enough configured symbols");
        rows.push(serde_json::json!({"symbol":rd.inst_meta(*i).symbol.as_ref(), "holdMode":"one_way_mode", "marginMode":"crossed", "posSide":"long", "total":"0.001", "avgPrice":"65000.125", "unrealisedPnl":"0", "liquidationPrice":"60000.555"}));
        let body = serde_json::to_vec(&serde_json::json!({"code":"00000","data":{"list":rows}})).unwrap();
        if body.len() >= 50 * 1024 {
            break body;
        }
    };
    let mut state = State::default();
    Fut::account_decode(rd, support::ACCT, &Query::Positions, 200, &body, 1, &mut state).unwrap();
    assert_eq!(state.margin_items.len(), rows.len());
    c.bench_function("bench_acct_decode_bitget", |b| {
        b.iter(|| {
            Fut::account_decode(
                rd,
                support::ACCT,
                &Query::Positions,
                200,
                black_box(&body),
                1,
                &mut state,
            )
            .unwrap()
        })
    });
}
criterion_group!(
    benches,
    bgt_503_encode_place,
    bgt_504_decode_order,
    bgt_505_account_positions
);
criterion_main!(benches);
