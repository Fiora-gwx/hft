//! Calibration sequence, interpolation and error bounds (TEL-011–TEL-019).

mod common;

use common::*;
use hft_telemetry::{
    CLOCK_STEPPED, ClockCal, ClockStatusRow, ClockTrust, CounterName, Domain, HistKey, Stage, Telemetry,
    TsSrc, WindowReport,
};
use hft_types::Tsc;
use proptest::prelude::*;

fn hist_count(hist: &[(HistKey, hft_telemetry::Quantiles)], stage: Stage) -> u64 {
    hist.iter()
        .filter(|(k, _)| k.stage == stage)
        .map(|(_, q)| q.count)
        .sum()
}

fn counter(c: &[(hft_telemetry::CounterKey, u64)], name: CounterName, stage: Option<Stage>) -> u64 {
    c.iter()
        .filter(|(k, _)| k.name == name && k.stage == stage)
        .map(|(_, n)| n)
        .sum()
}

fn std_window(a: ClockStatusRow, b: ClockStatusRow, s: hft_event::LatencySample) -> WindowReport {
    let mut f = Feed::new(&cfg());
    f.tel.window(0);
    f.clock(&a);
    let mut s = s;
    let t = f.rec(
        s.proc_tsc.0,
        hft_event::RecordFlags::HW_TS.0,
        &bbo(mhdr(s.hw_rx_ns, s.sw_rx_ns, s.rx_tsc.0, 0)),
    );
    s.trigger_seq = t;
    f.sample(&s);
    f.clock(&b);
    f.tel.window(10_000_000_000).unwrap()
}

#[test]
fn tel_011_interpolation_vs_step() {
    // TSC 0.5 ppm fast against PHC: 1 s of PHC = 1 000 000 500 ticks (nominal 1 GHz).
    let a = row(T0, P0);
    let b = row(T0 + 1_000_000_500, P0 + 1_000_000_000);
    let cal = ClockCal::new(None, a, Some(b), 1_000);
    // True kernel_rx 12 000 ns for a sample in the middle of the interval.
    let hw = P0 + 500_000_000;
    let rx_tsc = T0 + 500_000_250 + 12_000; // + 12 000 × 0.5 ppm < 1 tick
    let mut s = std_sample();
    s.hw_rx_ns = hw;
    s.rx_tsc = Tsc(rx_tsc);
    s.proc_tsc = Tsc(rx_tsc + 3_000);
    s.decided_tsc = Tsc(rx_tsc + 8_000);
    s.sent_tsc = Tsc(rx_tsc + 16_000);
    s.ack_hw_rx_ns = hw + 1_000_000;
    s.sw_rx_ns = hw + 5_000;
    let k = Telemetry::stages(&s, &cal).get(Stage::KernelRx).unwrap();
    assert!((k - 12_000).abs() < 50, "interpolated {k}");
    // Step conversion from the most recent ClockStatus at the nominal rate.
    let step = a.phc_ns + (rx_tsc - a.tsc) as i64 - hw;
    assert!((step - 12_000).abs() >= 200, "step {step}");
}

#[test]
fn tel_012_error_bound_formula() {
    let prev = row(0, P0 - 1_000_000_000);
    let mut a = row(T0, P0);
    a.calib_err_ns = 100;
    let mut b = row(2 * T0, P0 + 1_000_000_500);
    b.calib_err_ns = 300;
    let cal = ClockCal::new(Some(prev), a, Some(b), 1_000);
    let mut s = std_sample();
    s.hw_rx_ns = P0 + 500_000_000;
    s.rx_tsc = Tsc(T0 + 500_012_000);
    let st = Telemetry::stages(&s, &cal);
    assert_eq!(st.err_bound_ns, 550);
    assert!(st.cal_ok);
}

#[test]
fn tel_013_cal_threshold() {
    for (row_id, err, ok) in [
        ("TEL-013.1", 999u32, true),
        ("TEL-013.2", 1_000, true),
        ("TEL-013.3", 1_001, false),
        ("TEL-013.4", 1_500, false),
    ] {
        let mut a = std_a();
        a.calib_err_ns = err;
        let mut b = std_b();
        b.calib_err_ns = 0;
        let cal = ClockCal::new(None, a, Some(b), 1_000);
        assert_eq!(Telemetry::stages(&std_sample(), &cal).cal_ok, ok, "{row_id}");
        let w = std_window(a, b, std_sample());
        let phc = hist_count(&w.hist, Stage::KernelRx) + hist_count(&w.hist, Stage::TickToTrade);
        assert_eq!(phc, if ok { 2 } else { 0 }, "{row_id}");
        assert_eq!(
            counter(&w.counters, CounterName::UnsyncedCal, None),
            u64::from(!ok),
            "{row_id}"
        );
        assert_eq!(
            hist_count(&w.cal_err, Stage::KernelRx),
            1,
            "{row_id}: error bound distribution"
        );
    }
}

#[test]
fn tel_014_missing_next_clockstatus() {
    let mut f = Feed::new(&cfg());
    f.tel.window(0);
    f.clock(&std_a());
    let mut s = std_sample();
    s.trigger_seq = f.rec(
        s.proc_tsc.0,
        hft_event::RecordFlags::HW_TS.0,
        &bbo(mhdr(s.hw_rx_ns, s.sw_rx_ns, s.rx_tsc.0, 0)),
    );
    f.sample(&s);
    // Before b arrives nothing is computed.
    let w = f.tel.window(10_000_000_000).unwrap();
    assert!(w.hist.is_empty() && w.counters.is_empty(), "{w:?}");
    // The journal ends.
    f.tel.finish();
    let d = f.tel.day_close(19_675);
    assert_eq!(counter(&d.counters, CounterName::UnsyncedCal, None), 1);
    for st in Stage::ALL.into_iter().filter(|s| s.cross_domain()) {
        assert_eq!(hist_count(&d.hist, st), 0, "{}", st.name());
    }
}

#[test]
fn tel_015_stepped_endpoint() {
    let mut b = std_b();
    b.state = CLOCK_STEPPED;
    let cal = ClockCal::new(None, std_a(), Some(b), 1_000);
    assert!(!Telemetry::stages(&std_sample(), &cal).cal_ok);
    let w = std_window(std_a(), b, std_sample());
    assert_eq!(counter(&w.counters, CounterName::UnsyncedCal, None), 1);
    for st in [Stage::Ingest, Stage::Decide, Stage::Egress] {
        assert_eq!(hist_count(&w.hist, st), 1, "{}", st.name());
    }
    assert_eq!(hist_count(&w.hist, Stage::KernelRx), 0);
}

#[test]
fn tel_016_sw_anchor() {
    let mut s = std_sample();
    set_src(&mut s, 2, 2);
    s.hw_rx_ns = P0 + 405_000;
    s.sw_rx_ns = P0 + 405_000;
    // Distinct REALTIME and PHC columns: the SW anchor must use REALTIME.
    let mut a = std_a();
    let mut b = std_b();
    a.phc_ns += 37;
    b.phc_ns += 37;
    let cal = ClockCal::new(None, a, Some(b), 1_000);
    assert_eq!(cal.to_domain(T0, Domain::Realtime), P0);
    let st = Telemetry::stages(&s, &cal);
    assert_eq!(st.get(Stage::KernelRx), Some(7_000));
    assert_eq!(st.ts_src, TsSrc::Sw);
    let mut f = Feed::new(&cfg());
    f.tel.window(0);
    f.clock(&a);
    s.trigger_seq = f.rec(
        s.proc_tsc.0,
        hft_event::RecordFlags::SW_TS.0,
        &bbo(mhdr(s.hw_rx_ns, s.sw_rx_ns, s.rx_tsc.0, 0)),
    );
    f.sample(&s);
    f.clock(&b);
    let w = f.tel.window(10_000_000_000).unwrap();
    let k: Vec<_> = w
        .hist
        .iter()
        .filter(|(k, _)| k.stage == Stage::KernelRx)
        .collect();
    assert_eq!(k.len(), 1);
    assert_eq!((k[0].0.ts_src, k[0].1.max), (TsSrc::Sw, 7_000));
    assert!(w.hist.iter().all(|(k, _)| k.ts_src != TsSrc::Hw), "no HW series");
}

#[test]
fn tel_017_exchange_clock_trust() {
    // TEL-017.1
    let (mut a, mut b) = (std_a(), std_b());
    a.max_error_ns = 200_000;
    b.max_error_ns = 200_000;
    let w = std_window(a, b, std_sample());
    for st in [Stage::ExQueue, Stage::WireIn, Stage::FeedOneway] {
        assert_eq!(hist_count(&w.hist, st), 1, "TEL-017.1 {}", st.name());
    }
    assert_eq!(w.clock_trust, ClockTrust::Trusted, "TEL-017.1");
    // TEL-017.2
    a.max_error_ns = 200_001;
    let w = std_window(a, b, std_sample());
    for st in [Stage::ExQueue, Stage::WireIn, Stage::FeedOneway] {
        assert_eq!(hist_count(&w.hist, st), 0, "TEL-017.2 {}", st.name());
        assert_eq!(
            counter(&w.counters, CounterName::Unsynced, Some(st)),
            1,
            "TEL-017.2 {}",
            st.name()
        );
    }
    assert_eq!(w.clock_trust, ClockTrust::Untrusted, "TEL-017.2");
    assert_eq!(hist_count(&w.hist, Stage::KernelRx), 1, "TEL-017.2");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]
    #[test]
    fn tel_018_tsc_conversion_precision(
        hz in prop_oneof![Just(2_999_999_999u64), 100_000_000u64..=5_000_000_000u64],
        frac in 0.0f64..=1.0,
    ) {
        let max = 10 * hz;
        let d = ((max as f64) * frac) as u64;
        let mut a = std_a();
        a.tsc_hz_milli = hz * 1_000;
        let mut b = std_b();
        b.tsc = a.tsc + 2 * max;
        b.tsc_hz_milli = hz * 1_000;
        let cal = ClockCal::new(None, a, Some(b), 1_000);
        let mut s = std_sample();
        s.proc_tsc = Tsc(T0 + 1);
        s.decided_tsc = Tsc(T0 + 1 + d);
        let got = Telemetry::stages(&s, &cal).get(Stage::Decide).unwrap() as i128;
        let exact = i128::from(d) * 1_000_000_000 / i128::from(hz);
        let err = (got - exact).abs();
        // < 1 ns per ms of segment length (and exact below 1 ms).
        prop_assert!(err * 1_000_000 <= exact.max(0), "hz {hz} d {d}: {got} vs {exact}");
    }
}

#[test]
fn tel_019_queue_bound() {
    let mut f = Feed::new(&cfg());
    let rows: Vec<ClockStatusRow> = (0..10)
        .map(|i| row(T0 * (i + 1), P0 + i as i64 * 1_000_000_000))
        .collect();
    for r in &rows {
        f.clock(r);
    }
    let q: Vec<ClockStatusRow> = f.tel.cal_queue().rows().copied().collect();
    assert!(q.len() <= 4);
    assert_eq!(q, rows[rows.len() - q.len()..].to_vec());
}
