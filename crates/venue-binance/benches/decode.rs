//! venue-binance decode benchmarks (README "延迟与资源契约"; BIN-500–BIN-502, BIN-504).

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use hft_venue_proto::testkit::VecSink;
use hft_venue_proto::{MsgOp, VenueProtocol};
use venue_binance::{FeedCtx, Spot, Usdm, spot, usdm};

#[path = "../tests/support/mod.rs"]
mod support;

use support::{BT1, RX, SBT1, rd};

fn bin_500_classify_usdm(c: &mut Criterion) {
    let route = Usdm::routes()[0].id;
    let head = &BT1.as_bytes()[..64];
    c.bench_function("bench_classify_usdm", |b| {
        b.iter(|| Usdm::classify(black_box(route), MsgOp::Text, black_box(head)))
    });
}

fn bin_501_peek_bookticker(c: &mut Criterion) {
    let r = rd();
    let ctx = FeedCtx::USDM;
    c.bench_function("bench_peek_bookticker", |b| {
        b.iter(|| Usdm::peek(r, &ctx, usdm::MdClass::BookTicker, black_box(BT1.as_bytes())))
    });
}

fn bin_502_decode_bookticker(c: &mut Criterion) {
    let r = rd();
    let mut ctx = FeedCtx::USDM;
    let mut sink = VecSink::default();
    sink.md.reserve(1 << 20);
    c.bench_function("bench_decode_bookticker", |b| {
        b.iter(|| {
            sink.md.clear();
            Usdm::decode_md(
                r,
                &mut ctx,
                usdm::MdClass::BookTicker,
                black_box(BT1.as_bytes()),
                &RX,
                &mut sink,
            )
        })
    });
}

fn bin_504_decode_spot_bookticker(c: &mut Criterion) {
    let r = rd();
    let mut ctx = FeedCtx::SPOT;
    let mut sink = VecSink::default();
    sink.md.reserve(1 << 20);
    c.bench_function("bench_decode_spot_bookticker", |b| {
        b.iter(|| {
            sink.md.clear();
            Spot::decode_md(
                r,
                &mut ctx,
                spot::MdClass::JsonBookTicker,
                black_box(SBT1.as_bytes()),
                &RX,
                &mut sink,
            )
        })
    });
}

criterion_group!(
    benches,
    bin_500_classify_usdm,
    bin_501_peek_bookticker,
    bin_502_decode_bookticker,
    bin_504_decode_spot_bookticker
);
criterion_main!(benches);
