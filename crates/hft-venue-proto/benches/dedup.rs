use criterion::{Criterion, criterion_group, criterion_main};
use hft_types::InstId;
use hft_venue_proto::{TradeIdRing, TradeKey, dedup_key};
fn vpr_140_trade_id_ring(c: &mut Criterion) {
    let mut ring = TradeIdRing::<1024>::new();
    for k in 1..=1024 {
        ring.seen_or_insert(k);
    }
    let mut key = 1024u64;
    c.bench_function("bench_trade_id_ring", |b| {
        b.iter(|| {
            key = key.wrapping_add(1).max(1);
            std::hint::black_box(ring.seen_or_insert(std::hint::black_box(key)))
        })
    });
}
fn vpr_141_dedup_key(c: &mut Criterion) {
    c.bench_function("bench_dedup_key", |b| {
        b.iter(|| {
            std::hint::black_box(dedup_key(
                std::hint::black_box(InstId(201)),
                TradeKey::OrderSeq {
                    exch_order_id: 7,
                    fill_seq: 12345,
                },
            ))
        })
    });
}
criterion_group!(benches, vpr_140_trade_id_ring, vpr_141_dedup_key);
criterion_main!(benches);
