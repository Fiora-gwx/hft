//! Window roll and daily accumulation (TEL-020, TEL-021).

mod common;

use common::*;
use hft_telemetry::Stage;
use hft_types::Tsc;

#[test]
fn tel_020_window_alignment() {
    let mut f = Feed::new(&cfg());
    f.std_with(std_sample());
    // TEL-020.1
    assert!(f.tel.window(1_700_000_003_000_000_000).is_none(), "TEL-020.1");
    // TEL-020.2
    assert!(f.tel.window(1_700_000_009_999_999_999).is_none(), "TEL-020.2");
    // TEL-020.3
    let w = f.tel.window(1_700_000_010_000_000_000).expect("TEL-020.3");
    assert_eq!(
        (w.start_ns, w.end_ns),
        (1_700_000_000_000_000_000, 1_700_000_010_000_000_000),
        "TEL-020.3"
    );
    assert!(!w.hist.is_empty(), "TEL-020.3: the sample is in the first window");
    // TEL-020.4
    assert!(f.tel.window(1_700_000_010_000_000_000).is_none(), "TEL-020.4");
    let next = f.tel.window(1_700_000_020_000_000_000).unwrap();
    assert_eq!(next.start_ns, 1_700_000_010_000_000_000, "TEL-020.4");
    assert!(
        next.hist.is_empty(),
        "TEL-020.4: the new window's histograms are empty"
    );
}

#[test]
fn tel_021_daily_accumulates() {
    let mut f = Feed::new(&cfg());
    let w0 = 1_700_000_000_000_000_000i64;
    f.tel.window(w0);
    let mut tsc = T0;
    for win in 1..=3i64 {
        // Each window: calibration rows around 10 tick_to_trade samples.
        f.clock(&row(tsc, P0 + (tsc - T0) as i64));
        for i in 0..10u64 {
            let base = tsc + 1_000_000 + i * 100_000;
            let mut s = std_sample();
            s.hw_rx_ns = P0 + (base - T0) as i64 - 12_000;
            s.sw_rx_ns = s.hw_rx_ns + 5_000;
            s.ack_hw_rx_ns = s.hw_rx_ns + 1_000_000;
            s.rx_tsc = Tsc(base);
            s.proc_tsc = Tsc(base + 3_000);
            s.decided_tsc = Tsc(base + 8_000);
            s.sent_tsc = Tsc(base + 16_000);
            s.trigger_seq = f.rec(
                base + 3_000,
                hft_event::RecordFlags::HW_TS.0,
                &bbo(mhdr(s.hw_rx_ns, s.sw_rx_ns, base, 0)),
            );
            f.sample(&s);
        }
        tsc += 2_000_000_000;
        f.clock(&row(tsc, P0 + (tsc - T0) as i64));
        let w = f.tel.window(w0 + win * 10_000_000_000).unwrap();
        let n: u64 = w
            .hist
            .iter()
            .filter(|(k, _)| k.stage == Stage::TickToTrade)
            .map(|(_, q)| q.count)
            .sum();
        assert_eq!(n, 10, "window {win}");
    }
    let d = f.tel.day_close(19_675);
    let n: u64 = d
        .hist
        .iter()
        .filter(|(k, _)| k.stage == Stage::TickToTrade)
        .map(|(_, q)| q.count)
        .sum();
    assert_eq!(n, 30);
    assert_eq!(d.day, 19_675);
}
