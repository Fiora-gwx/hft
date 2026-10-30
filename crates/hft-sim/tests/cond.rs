//! The latency-condition tracker (SIM-240).

use hft_event::{Bbo, EventKind, MarketHdr};
use hft_sim::{CondCfg, CondTracker};
use hft_types::{InstId, Px, Tsc, VenueId};
use zerocopy::{FromZeros, IntoBytes};

const HZ_MILLI: u64 = 1_000_000_000_000; // 1 GHz TSC: 1 tick = 1 ns
const S: i64 = 1_000_000_000;

fn l2(venue: u8, rx_ns: i64, lag_ns: i64) -> Vec<u8> {
    let mut h = MarketHdr::new_zeroed();
    h.venue = VenueId(venue);
    h.sw_rx_ns = rx_ns;
    h.ex_ts_ns = rx_ns - lag_ns;
    h.as_bytes().to_vec()
}

fn bbo(venue: u8, inst: u32, bid: i64, ask: i64) -> Vec<u8> {
    let mut b = Bbo::new_zeroed();
    b.h.venue = VenueId(venue);
    b.h.inst = InstId(inst);
    b.bid_px = Px(bid);
    b.ask_px = Px(ask);
    b.as_bytes().to_vec()
}

/// Ten samples per second of `lag_ns` on venue 1 over UTC minute `m`.
fn minute(t: &mut CondTracker, m: i64, lag_ns: i64) {
    for s in 0..60 {
        for k in 0..10 {
            let rx = (m * 60 + s) * S + k * 100_000_000;
            t.on_market(EventKind::L2_BATCH, &l2(1, rx, lag_ns), 0);
        }
    }
}

#[test]
fn sim_240_condition_buckets() {
    let cfg = CondCfg {
        load_thr_ns: vec![1_000_000, 5_000_000],
        window_s: 300,
        phase_s: 5,
    };
    // Venue 1 tracked; venue 2 carries the reference quote.
    let mut t = CondTracker::new(cfg, 1 << 1, HZ_MILLI);
    // .1 load: 1 until a minute completes; then the number of thresholds below the minute median
    // of per-second p90, from the next minute on.
    assert_eq!(t.condition(Tsc(0), 0).load, 1, "SIM-240.1");
    minute(&mut t, 0, 10_000_000);
    assert_eq!(t.condition(Tsc(0), 0).load, 1, "SIM-240.1 minute 0 not complete");
    minute(&mut t, 1, 500_000);
    assert_eq!(t.condition(Tsc(0), 0).load, 2, "SIM-240.1 minute 0 = 10 ms");
    minute(&mut t, 2, 3_000_000);
    assert_eq!(t.condition(Tsc(0), 0).load, 0, "SIM-240.1 minute 1 = 0.5 ms");
    // Market data of an untracked venue is not a lag sample.
    t.on_market(EventKind::L2_BATCH, &l2(2, 200 * S, 50_000_000), 0);
    assert_eq!(t.condition(Tsc(0), 0).load, 0, "SIM-240.1");
    // .2 move: the reference mid over the last 1 s (bid + ask 2 000 000; 0.25 bp = 50).
    let base = 10_000_000_000u64;
    t.on_market(EventKind::BBO, &bbo(2, 9, 999_000, 1_001_000), base);
    t.on_market(EventKind::BBO, &bbo(2, 9, 999_020, 1_001_020), base + 500_000_000);
    assert_eq!(t.condition(Tsc(base + 600_000_000), 0).mv, 0, "SIM-240.2 0.2 bp");
    t.on_market(EventKind::BBO, &bbo(2, 9, 999_080, 1_001_080), base + 700_000_000);
    assert_eq!(t.condition(Tsc(base + 800_000_000), 0).mv, 1, "SIM-240.2 0.8 bp");
    t.on_market(EventKind::BBO, &bbo(2, 9, 999_200, 1_001_200), base + 900_000_000);
    assert_eq!(t.condition(Tsc(base + 950_000_000), 0).mv, 2, "SIM-240.2 2 bp");
    // Another inst on the reference venue is not the reference (the first one seen is).
    t.on_market(EventKind::BBO, &bbo(2, 8, 1, 3), base + 960_000_000);
    assert_eq!(t.condition(Tsc(base + 970_000_000), 0).mv, 2, "SIM-240.2");
    // Older than 1 s before the decision: compared from the oldest quote within 1 s.
    assert_eq!(
        t.condition(Tsc(base + 1_850_000_000), 0).mv,
        0,
        "SIM-240.2 last quote only"
    );
    // .3 phase: 0 within the first 5 s of each 300 s window, else 1.
    assert_eq!(t.condition(Tsc(0), 300 * S + 4 * S).phase, 0, "SIM-240.3");
    assert_eq!(t.condition(Tsc(0), 300 * S + 5 * S).phase, 1, "SIM-240.3");
}
