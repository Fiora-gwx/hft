//! Private-report benchmark: fresh fills, dedup hits, and external fills are separate samples.
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use hft_venue_proto::{MsgOp, VenueProtocol};
use std::hint::black_box;
#[path = "support/sink.rs"]
mod sink;
#[path = "../tests/support/mod.rs"]
mod support;
use venue_binance::{GwCtx, Usdm};
const PAYLOAD: &str = r#"{"e":"ORDER_TRADE_UPDATE","o":{"s":"BTCUSDT","c":"1000301020002a000000073d","S":"BUY","o":"LIMIT","x":"TRADE","X":"PARTIALLY_FILLED","i":325078477,"l":"0.001","z":"0.003","L":"65000.1","ap":"65000.05","n":"0.0130","N":"USDT","rp":"0","T":1790000000010,"t":9001}}"#;
fn bin_506_decode_otu(c: &mut Criterion) {
    let rd = support::rd();
    for (label, payload) in [
        ("own", PAYLOAD.to_owned()),
        (
            "external",
            PAYLOAD.replace("1000301020002a000000073d", "web_external"),
        ),
    ] {
        let class = Usdm::classify_report(MsgOp::Text, &payload.as_bytes()[..64]);
        let mut ctx = GwCtx::default();
        let mut sink = sink::CountSink::default();
        assert_eq!(
            Usdm::decode_report(rd, &mut ctx, class, payload.as_bytes(), &support::RX, &mut sink)
                .unwrap()
                .events,
            1
        );
        c.bench_function(&format!("bench_decode_otu/{label}"), |b| {
            b.iter_batched(
                || (GwCtx::default(), sink::CountSink::default()),
                |(mut ctx, mut sink)| {
                    black_box(Usdm::decode_report(
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
        c.bench_function(&format!("bench_decode_otu/{label}-duplicate"), |b| {
            b.iter(|| {
                let out = Usdm::decode_report(
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
fn bin_507_skip_event_usdm(c: &mut Criterion) {
    for (label, payload) in [
        ("listed", br#"{"e":"TRADE_LITE"}"#.as_slice()),
        ("unknown", br#"{"e":"FUTURE_EVENT"}"#.as_slice()),
    ] {
        let mut ctx = GwCtx::default();
        let mut sink = sink::CountSink::default();
        let rd = support::rd();
        let class = Usdm::classify_report(MsgOp::Text, payload);
        c.bench_function(&format!("bench_skip_event_usdm/{label}"), |b| {
            b.iter(|| {
                black_box(Usdm::decode_report(
                    rd,
                    &mut ctx,
                    class,
                    black_box(payload),
                    &support::RX,
                    &mut sink,
                ))
                .unwrap()
            })
        });
    }
}

// Warm bootstrap decoder: allocations are included, never interpreted as a hot acct_* result.
fn bin_508_account_positions(c: &mut Criterion) {
    use hft_venue_proto::account::{AccountProtocol, Query, State};
    let rd = support::rd();
    let mut rows = Vec::new();
    let body = loop {
        let i = rd
            .insts_of_venue(Usdm::VENUE)
            .get(rows.len())
            .expect("enough configured symbols");
        rows.push(serde_json::json!({"symbol":rd.inst_meta(*i).symbol.as_ref(), "positionSide":"BOTH", "positionAmt":"0.001", "entryPrice":"65000.125", "unRealizedProfit":"0", "liquidationPrice":"60000.555"}));
        let body = serde_json::to_vec(&rows).unwrap();
        if body.len() >= 50 * 1024 {
            break body;
        }
    };
    let mut state = State::default();
    Usdm::account_decode(rd, support::ACCT, &Query::Positions, 200, &body, 1, &mut state).unwrap();
    assert_eq!(state.margin_items.len(), rows.len());
    c.bench_function("bench_acct_decode_position_risk", |b| {
        b.iter(|| {
            Usdm::account_decode(
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
    bin_506_decode_otu,
    bin_507_skip_event_usdm,
    bin_508_account_positions
);
criterion_main!(benches);
