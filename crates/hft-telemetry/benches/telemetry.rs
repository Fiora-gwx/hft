//! hft-telemetry benchmarks (README "延迟与资源契约"; TEL-085–TEL-087, TEL-090).

use std::hint::black_box;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use hft_event::{Body, RecordFlags, RecordHeader, record_len};
use hft_telemetry::{HistParams, Telemetry, event_weighted, series_bytes};
use hft_types::Tsc;

#[path = "../tests/common/mod.rs"]
mod common;

use common::{P0, T0, bbo, cfg, cs, ew_cfg, mhdr, rd, row, std_cal, std_sample};

/// `(header, body)` records: per 1 ms a `ClockStatus` every 1 000 records, `Bbo` triggers and
/// their `LatencySample`s.
fn records(n: u64) -> Vec<(RecordHeader, Vec<u8>)> {
    fn rec<B: Body>(seq: u64, tsc: u64, b: &B) -> (RecordHeader, Vec<u8>) {
        let body = b.as_bytes().to_vec();
        let h = RecordHeader {
            len: record_len(body.len(), false),
            kind: B::KIND.0,
            flags: RecordFlags::HW_TS.0,
            seq,
            tsc,
        };
        (h, body)
    }
    let mut v = Vec::with_capacity(n as usize);
    let mut seq = 0;
    let mut i = 0u64;
    while seq < n {
        let base = T0 + i * 10_000;
        if i.is_multiple_of(500) {
            seq += 1;
            v.push(rec(seq, base, &cs(&row(base, P0 + (base - T0) as i64))));
        }
        seq += 1;
        let trig = seq;
        let hw = P0 + (base - T0) as i64 - 12_000;
        v.push(rec(seq, base + 3_000, &bbo(mhdr(hw, hw + 5_000, base, 10))));
        let mut s = std_sample();
        s.trigger_seq = trig;
        s.hw_rx_ns = hw;
        s.sw_rx_ns = hw + 5_000;
        s.rx_tsc = Tsc(base);
        s.proc_tsc = Tsc(base + 3_000);
        s.decided_tsc = Tsc(base + 8_000);
        s.sent_tsc = Tsc(base + 16_000);
        s.ack_hw_rx_ns = hw + 1_000_000;
        seq += 1;
        v.push(rec(seq, base + 20_000, &s));
        i += 1;
    }
    v
}

fn tel_085_ingest(c: &mut Criterion) {
    let recs = records(30_000);
    let rd = rd();
    // 30 000 records per iteration.
    c.bench_function("telemetry/ingest", |b| {
        b.iter_batched(
            || Telemetry::new(&cfg(), &rd, &hft_telemetry::StrategyCatalog::empty()),
            |mut t| {
                for (h, body) in &recs {
                    t.ingest(h, body);
                }
                t
            },
            BatchSize::LargeInput,
        )
    });
}

fn tel_086_stages(c: &mut Criterion) {
    let s = std_sample();
    let cal = std_cal();
    c.bench_function("telemetry/stages", |b| {
        b.iter(|| Telemetry::stages(black_box(&s), black_box(&cal)))
    });
}

fn tel_087_event_weighted(c: &mut Criterion) {
    let mut j = common::Jw::new([1; 32], 64);
    let mut base = T0;
    for i in 0..100_000u64 {
        if i.is_multiple_of(1_000) {
            j.clock(&row(base, P0 + (base - T0) as i64));
        }
        let hw = P0 + (base - T0) as i64 - 12_000;
        j.input(
            base + 3_000,
            RecordFlags::HW_TS.0,
            &bbo(mhdr(hw, hw + 5_000, base, 10)),
        );
        base += 10_000;
    }
    j.clock(&row(base, P0 + (base - T0) as i64));
    let rd = rd();
    // 100 000 market records per iteration.
    c.bench_function("telemetry/event_weighted", |b| {
        b.iter(|| event_weighted(&mut j.reader(), &rd, &ew_cfg()).unwrap())
    });
}

fn tel_090_series_bytes(c: &mut Criterion) {
    let p = HistParams {
        sig_digits: 3,
        min_ns: 1,
        max_ns: 10_000_000_000,
    };
    // The measured quantity is the size; the timing is that of creating one series' histograms.
    eprintln!("telemetry/series_bytes: {} B per series", series_bytes(&p));
    c.bench_function("telemetry/series_bytes", |b| {
        b.iter(|| (p.new_hist(), p.new_hist()))
    });
}

criterion_group!(benches, tel_086_stages, tel_090_series_bytes);
criterion_group! {
    name = throughput;
    config = Criterion::default().sample_size(10);
    targets = tel_085_ingest, tel_087_event_weighted
}
criterion_main!(benches, throughput);
