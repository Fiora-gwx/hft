//! Stratified probe composition (TEL-036, TEL-038).

mod common;

use common::*;
use hft_event::RecordFlags;
use hft_telemetry::{event_weighted, stratified_probe};
use hft_types::Tsc;

fn within(got: i64, want: i64) -> bool {
    (got - want).abs() as f64 <= want as f64 * 0.001
}

#[test]
fn tel_036_composite() {
    // Record shares: Burst 10 000 of 50 000 = 0.2, Calm 0.8.
    let j = probe_journal(10_000, 10_000, 30_000);
    let ew = event_weighted(&mut j.reader(), &rd(), &ew_cfg()).unwrap();
    assert_eq!(ew.len(), 1);
    let r = stratified_probe(&mut j.reader(), &rd(), &ew_cfg(), &ew[0]).unwrap();
    assert_eq!(r.groups.len(), 1, "{:?}", r.groups);
    let g = &r.groups[0];
    assert_eq!(g.n, [10_000, 10_000]);
    assert!(
        (g.weight[0] - 0.2).abs() < 1e-12 && (g.weight[1] - 0.8).abs() < 1e-12,
        "{:?}",
        g.weight
    );
    let (p50, p99) = g.composite_p50_p99.unwrap();
    assert!(within(p50, 6_250_000), "p50 {p50}");
    assert!(within(p99, 19_500_000), "p99 {p99}");
    for (i, q) in g.by_stratum.iter().enumerate() {
        let q = q.unwrap();
        assert!(q.p99.is_some(), "stratum {i}");
        assert!(q.p50 > 0);
    }
}

#[test]
fn tel_038_trigger_stratum() {
    let mut j = Jw::new([7; 32], 1);
    j.clock(&std_a());
    let rx1 = T0 + 100_000;
    let rx2 = T0 + 200_000;
    let hw = |rx: u64| P0 + (rx - T0) as i64;
    let r1 = j.input(rx1 + 500, RecordFlags::HW_TS.0, &bbo(mhdr(hw(rx1), 0, rx1, 10)));
    let r2 = j.input(rx2 + 500, RecordFlags::HW_TS.0, &bbo(mhdr(hw(rx2), 0, rx2, 10)));
    // Another record of the same read as R2.
    j.input(rx2 + 600, RecordFlags::HW_TS.0, &bbo(mhdr(hw(rx2), 0, rx2, 10)));
    for (trig, rx) in [(r1, rx1), (r2, rx2)] {
        let mut s = std_sample();
        s.trigger_seq = trig;
        s.hw_rx_ns = hw(rx);
        s.sw_rx_ns = 0;
        s.rx_tsc = Tsc(rx);
        s.proc_tsc = Tsc(rx + 500);
        s.decided_tsc = Tsc(rx + 600);
        s.sent_tsc = Tsc(rx + 5_000);
        s.ack_hw_rx_ns = hw(rx) + 1_000_000;
        j.meta(rx + 6_000, &s);
    }
    j.clock(&std_b());
    let ew = event_weighted(&mut j.reader(), &rd(), &ew_cfg()).unwrap();
    let r = stratified_probe(&mut j.reader(), &rd(), &ew_cfg(), &ew[0]).unwrap();
    assert_eq!(r.groups.len(), 1);
    assert_eq!(r.groups[0].n, [1, 1], "[Burst, Calm]");
    assert_eq!(r.groups[0].by_stratum[0].unwrap().max, 5_000);
}
