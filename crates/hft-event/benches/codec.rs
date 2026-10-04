//! Codec benchmarks (EVT-190 – EVT-193); ids equal the `基准名` in TESTS.md and README.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use hft_event::*;
use hft_types::*;
use zerocopy::{FromZeros, IntoBytes};

fn note() -> Note {
    Note {
        code: note_code(NoteSource::Oms, 3),
        level: NoteLevel::P1,
        source: NoteSource::Oms,
        strategy: StrategyId(1),
        _pad: 0,
        group: GROUP_NONE,
        inst: InstId(7),
        v: [1, 2, 3, 4, 5, 6, 7, 8],
    }
}

fn evt_190_encode_fixed_80b(c: &mut Criterion) {
    let n = note();
    let mut out = vec![0u8; 256];
    c.bench_function("event_encode_80b", |b| {
        b.iter(|| {
            encode_fixed(
                &mut out,
                black_box(1),
                black_box(2),
                RecordFlags::EVENT_END,
                black_box(&n),
            )
        })
    });
}

fn evt_191_encode_advice_1k(c: &mut Criterion) {
    let mut h = AdviceHdr::new_zeroed();
    h.strategy = StrategyId(3);
    h.len = 1024;
    let payload = vec![7u8; 1024];
    let mut out = vec![0u8; 2048];
    c.bench_function("event_encode_advice_1k", |b| {
        b.iter(|| {
            encode_var(
                &mut out,
                black_box(1),
                black_box(2),
                RecordFlags(0),
                black_box(&h),
                black_box(&payload),
            )
        })
    });
}

fn evt_192_decode_view(c: &mut Criterion) {
    let mut h = MarketHdr::new_zeroed();
    h.inst = InstId(9);
    let bbo = Bbo {
        h,
        bid_px: Px(1),
        bid_qty: Qty(2),
        ask_px: Px(3),
        ask_qty: Qty(4),
        update_id: 5,
    };
    let mut store = vec![0u64; 32];
    let buf: &mut [u8] = store.as_mut_bytes();
    let n = encode_fixed(buf, 1, 2, RecordFlags(0), &bbo) as usize;
    let rec = &buf[..n];
    c.bench_function("event_decode_view", |b| {
        b.iter(|| {
            let r = decode_record(black_box(rec)).unwrap();
            black_box(r.view::<Bbo>().unwrap().ask_px)
        })
    });
}

fn evt_193_crc_64k(c: &mut Criterion) {
    // One full 64 KiB segment with CRC; decode_record verifies CRC32C over 65 532 bytes.
    let chunk = vec![0x5Au8; PART_MAX_CHUNK];
    let ph = PartHdr {
        total_len: chunk.len() as u32,
        chunk_len: chunk.len() as u32,
        part: 0,
        parts: 1,
        _rsv: 0,
    };
    let mut out = vec![0u8; 65_536];
    let n = encode_part(
        &mut out,
        1,
        2,
        RecordFlags::CRC,
        EventKind::CONFIG_BLOB,
        &ph,
        &chunk,
    )
    .unwrap() as usize;
    assert_eq!(n, 65_536);
    c.bench_function("event_crc_64k", |b| {
        b.iter(|| decode_record(black_box(&out[..n])).is_ok())
    });
}

criterion_group!(
    benches,
    evt_190_encode_fixed_80b,
    evt_191_encode_advice_1k,
    evt_192_decode_view,
    evt_193_crc_64k
);
criterion_main!(benches);
