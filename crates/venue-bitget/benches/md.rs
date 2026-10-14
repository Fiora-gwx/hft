//! BGT-500/501: production dispatch and books1 decoding over the shared synthetic UTA fixture.
//! Payload construction and sink reservation are outside the measured calls.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use hft_venue_proto::testkit::VecSink;
use hft_venue_proto::{MsgOp, RouteId, VenueProtocol};
use venue_bitget::{BitgetSpot, BitgetUsdtFutures, FeedCtx, MdClass};

#[path = "../tests/support/mod.rs"]
mod support;

fn bgt_500_classify(c: &mut Criterion) {
    let payload = support::b1v3("123456789", "\"1790000000123\"", "1790000000125");
    c.bench_function("bench_classify_bitget", |b| {
        b.iter(|| {
            black_box(BitgetUsdtFutures::classify(
                black_box(RouteId(0)),
                MsgOp::Text,
                black_box(&payload.as_bytes()[..64]),
            ))
        })
    });
}

fn decode<P: VenueProtocol<FeedCtx = FeedCtx, MdClass = MdClass>>(
    c: &mut Criterion,
    name: &str,
    payload: &str,
) {
    let rd = support::rd();
    let mut ctx = FeedCtx::new();
    let mut sink = VecSink::default();
    sink.md.reserve(1);
    P::decode_md(
        rd,
        &mut ctx,
        MdClass::Books1,
        payload.as_bytes(),
        &support::RX,
        &mut sink,
    )
    .unwrap();
    assert_eq!(sink.md.len(), 1);
    c.bench_function(name, |b| {
        b.iter(|| {
            sink.md.clear();
            black_box(P::decode_md(
                rd,
                &mut ctx,
                MdClass::Books1,
                black_box(payload.as_bytes()),
                &support::RX,
                &mut sink,
            ))
            .unwrap();
            black_box(&sink.md);
        })
    });
}

fn bgt_501_decode_books1(c: &mut Criterion) {
    let payload = support::b1v3("123456789", "\"1790000000123\"", "1790000000125");
    decode::<BitgetUsdtFutures>(c, "bench_decode_bitget_books1", &payload);
    decode::<BitgetSpot>(
        c,
        "bench_decode_bitget_books1/spot",
        &payload.replace("usdt-futures", "spot"),
    );
}

criterion_group!(benches, bgt_500_classify, bgt_501_decode_books1);
criterion_main!(benches);
