//! Node tests of `hft-feature` (TESTS.md FEA-*): fixed input → fixed output.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code

use hft_feature::*;
use hft_strategy_sdk::prelude::*;

#[test]
fn fea_001_fixed_point_math() {
    // .1 floor square root.
    assert_eq!(
        (isqrt(0), isqrt(1), isqrt(99), isqrt(100), isqrt(1 << 100)),
        (0, 1, 9, 10, 1 << 50),
        "FEA-001.1"
    );
    // .2 logarithms (exact values of the series).
    assert_eq!(ln_rate(1_000_000_000), 0, "FEA-001.2");
    assert_eq!(ln_rate(2_000_000_000), 693_147_181, "FEA-001.2");
    assert_eq!(ln_rate(10_000_000_000), 2_302_585_091, "FEA-001.2");
    assert_eq!(ln_rate(31_000_000_000), 3_433_987_198, "FEA-001.2");
    assert_eq!(ln_rate(999_999_999), 0, "FEA-001.2 below 1");
    assert_eq!(ln_ratio(10_001, 10_000), 99_994, "FEA-001.2");
    assert_eq!(ln_ratio(10_000, 10_001), -99_994, "FEA-001.2");
    assert_eq!(ln_ratio(0, 10_000), 0, "FEA-001.2");
    assert_eq!(atanh2(-300_000_000), -619_039_204, "FEA-001.2");
    // .3 imbalance: truncated toward zero, 0 without size.
    assert_eq!(imbalance(3, 1), 500_000_000, "FEA-001.3");
    assert_eq!(imbalance(1, 2), -333_333_333, "FEA-001.3");
    assert_eq!(imbalance(0, 0), 0, "FEA-001.3");
    // .4 interpolated quantiles ×10.
    assert_eq!(
        quantile_x10(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 9),
        Some(91),
        "FEA-001.4"
    );
    assert_eq!(quantile_x10(&[1, 2, 3, 4], 5), Some(25), "FEA-001.4");
    assert_eq!(quantile_x10(&[7], 9), Some(70), "FEA-001.4");
    assert_eq!(quantile_x10(&[], 5), None, "FEA-001.4");
}

#[test]
fn fea_002_normal_cdf() {
    let near = |a: i64, b: i64, tol: i64| (a - b).abs() <= tol;
    for (z, want) in [
        (0, 500_000_000),
        (1_000_000_000, 841_344_746),
        (-1_000_000_000, 158_655_254),
        (1_960_000_000, 975_002_105),
        (9_000_000_000, 999_999_713),
    ] {
        assert!(near(cdf(z), want, 20_000), "FEA-002.1 {z} {}", cdf(z));
    }
    // Table points exactly.
    assert_eq!(cdf(1_000_000_000 / 64), 506_233_219, "FEA-002.1");
    for p in (1..100).map(|k| k * 10_000_000) {
        assert!(near(cdf(inv_cdf(p)), p, 1_000_000), "FEA-002.2 {p}");
    }
    assert_eq!(inv_cdf(0), inv_cdf(1_000_000), "FEA-002.2 clamp");
    assert_eq!(inv_cdf(-5), -inv_cdf(1_000_000_005), "FEA-002.2 symmetric");
}

#[test]
fn fea_003_realized_vol() {
    let mut v = RealizedVol::new(4);
    for (s, m) in [
        (0, 2_000_000),
        (1, 2_000_200),
        (2, 2_000_000),
        (3, 2_000_200),
        (4, 2_000_000),
    ] {
        v.on_mid(m, s);
    }
    // .1 four returns of ±1e-4: sigma ≈ 1e-4.
    assert_eq!(v.samples(), 4, "FEA-003.1");
    assert_eq!(v.sigma(4), Some(99_995), "FEA-003.1");
    assert_eq!(v.sigma(5), None, "FEA-003.1 below min samples");
    assert_eq!(v.recent(2), (-99_990 + 100_000, 199_990), "FEA-003.1");
    // .2 same second: no sample; a later second evicts the oldest (ring of 4).
    v.on_mid(2_000_200, 4);
    assert_eq!(v.samples(), 4, "FEA-003.2");
    v.on_mid(2_000_200, 6);
    assert_eq!((v.samples(), v.recent(1)), (4, (100_000, 100_000)), "FEA-003.2");
    // .3 non-positive mids and a ret above 100 % clamp.
    let mut w = RealizedVol::new(2);
    w.on_mid(0, 0);
    assert_eq!(w.samples(), 0, "FEA-003.3");
    w.on_mid(10, 1);
    w.on_mid(1_000, 2);
    assert_eq!(w.recent(1), (ONE, ONE), "FEA-003.3 clamp");
    // .4 export: sigma with one sample, sample count.
    let mut b = [0i64; 2];
    let mut row = FeatureRow::new(&mut b);
    v.export(&mut row);
    assert_eq!(b, [v.sigma(1).unwrap(), 4], "FEA-003.4");
    assert_eq!(
        <RealizedVol as Feature>::COLUMNS
            .iter()
            .map(|c| c.name)
            .collect::<Vec<_>>(),
        ["sigma", "samples"],
        "FEA-003.4"
    );
}

#[test]
fn fea_004_bbo_top() {
    let mut t = BboTop::default();
    // .1 one-sided quotes change nothing.
    assert!(!t.on_quote(0, 5, 101, 5), "FEA-004.1");
    assert!(!t.on_quote(99, 5, 0, 5), "FEA-004.1");
    assert_eq!(t, BboTop::default(), "FEA-004.1");
    // .2 two-sided: 2 × mid and the quantity imbalance.
    assert!(t.on_quote(99, 30, 101, 10), "FEA-004.2");
    assert_eq!((t.mid_x2, t.imb), (200, 500_000_000), "FEA-004.2");
    // .3 zero sizes keep the previous imbalance; the mid moves.
    t.on_event((100, 0, 104, 0));
    assert_eq!((t.mid_x2, t.imb), (204, 500_000_000), "FEA-004.3");
}

#[test]
fn fea_005_second_history() {
    let mut h = SecHist::<8>::default();
    h.put(100, 1);
    h.put(100, 2);
    h.put(101, 3);
    // .1 last write of a second wins; a missing second falls back to t − 1, t − 2.
    assert_eq!(h.at(100), Some(2), "FEA-005.1");
    assert_eq!(h.at(101), Some(3), "FEA-005.1");
    assert_eq!(h.at(103), Some(3), "FEA-005.1 two back");
    assert_eq!(h.at(104), None, "FEA-005.1 three back");
    assert_eq!(h.at(99), None, "FEA-005.1");
    // .2 the ring of 8 seconds overwrites the slot of second 100 at 108.
    h.on_event((108, 9));
    assert_eq!((h.at(100), h.at(108)), (None, Some(9)), "FEA-005.2");
}

#[test]
fn fea_006_signed_flow() {
    let (x, o, other) = (InstId(3), InstId(4), InstId(1));
    let mut f = SignedFlow::<4>::default();
    f.push(10, x, 50);
    f.push(11, o, 80);
    f.push(12, x, -60);
    f.push(13, other, 1_000);
    // .1 toward x: +50 − 80 − 60; `other` does not count.
    assert_eq!(f.net(x, o, 0), -90, "FEA-006.1");
    assert_eq!(f.net(x, o, 12), -60, "FEA-006.1 window");
    assert_eq!(f.net(o, x, 0), 90, "FEA-006.1 mirror");
    // .2 the fifth print overwrites the oldest.
    f.on_event((14, x, 7));
    assert_eq!(f.net(x, o, 0), -133, "FEA-006.2");
}

/// A book fixture for `OwnExcludedSide`.
struct Book {
    bids: Vec<Level>,
    asks: Vec<Level>,
}

impl BookRead for Book {
    fn status(&self) -> BookStatus {
        BookStatus::Live
    }
    fn upd_tsc(&self) -> Tsc {
        Tsc(0)
    }
    fn best(&self, s: Side) -> Option<Level> {
        match s {
            Side::Buy => self.bids.first().copied(),
            Side::Sell => self.asks.first().copied(),
        }
    }
    fn levels(&self, s: Side, out: &mut [Level]) -> usize {
        let src = if s == Side::Buy { &self.bids } else { &self.asks };
        let n = src.len().min(out.len());
        out[..n].copy_from_slice(&src[..n]);
        n
    }
}

fn lv(px: i64, qty: i64) -> Level {
    Level {
        px: Px(px),
        qty: Qty(qty),
    }
}

#[test]
fn fea_007_own_excluded_side() {
    let b = Book {
        bids: vec![lv(453, 30), lv(450, 120), lv(447, 40), lv(440, 60), lv(420, 200)],
        asks: vec![lv(470, 100), lv(475, 20), lv(491, 5)],
    };
    // .1 our 30 at 0.453 is the whole level: the best bid as seen is 0.450.
    let s = OwnExcludedSide::<64>::read(&b, Side::Buy, Some((453, 30)));
    assert_eq!(
        (s.best(), s.top_size(), s.depth(10, 5)),
        (Some(450), 120, 380),
        "FEA-007.1"
    );
    // .2 part of a level: the rest stays; without own order nothing is excluded.
    let s = OwnExcludedSide::<64>::read(&b, Side::Buy, Some((450, 20)));
    assert_eq!(
        (s.best(), s.top_size(), s.size_at(450)),
        (Some(453), 30, 100),
        "FEA-007.2"
    );
    // Depth counts bucket prices only: 450, 440, 430, 420, 410 (0.453 is off the cent grid).
    let s = OwnExcludedSide::<64>::read(&b, Side::Buy, None);
    assert_eq!(s.depth(10, 5), 380, "FEA-007.2");
    // .3 asks: buckets upwards from the bucket the best ask is in (470, 480, 490; 0.475 and 0.491
    // are off the grid).
    let a = OwnExcludedSide::<64>::read(&b, Side::Sell, None);
    assert_eq!(
        (a.best(), a.depth(10, 3), a.depth(5, 2)),
        (Some(470), 100, 120),
        "FEA-007.3"
    );
    // .4 only L levels are read.
    let s = OwnExcludedSide::<2>::read(&b, Side::Buy, None);
    assert_eq!(s.depth(10, 5), 120, "FEA-007.4");
}

#[test]
fn fea_008_lag_quantiles() {
    let s = 1_000_000_000i64;
    let mut g = LagQuantiles::new();
    let mut b = [0i64; 3];
    g.export(&mut FeatureRow::new(&mut b));
    assert_eq!(b, [NA, NA, NA], "FEA-008.1 no minute yet");
    for k in 0..10 {
        g.on_event(LagInput::Sample {
            rx_ns: 60 * s + k * 1_000_000,
            lag_ns: (10 + k) * 1_000_000,
        });
    }
    g.push(61 * s, 30_000_000);
    g.push(62 * s + 5, 3_000_000);
    g.push(62 * s + 9, 1_000_000);
    g.push(61 * s + 1, 99_000_000);
    assert_eq!(g.late, 1, "FEA-008.2 closed second");
    g.on_event(LagInput::Tick(120 * s + 1));
    assert_eq!(
        (g.done_min, g.done_p90, g.done_p50),
        (1, 1_810_000_000, 1_450_000_000),
        "FEA-008.3"
    );
    assert_eq!(g.gates(130 * s, 18_100_000, 0), (true, false), "FEA-008.4");
    assert_eq!(g.gates(130 * s, 18_100_001, 0), (false, false), "FEA-008.4");
    assert_eq!(g.gates(130 * s, 0, 14_499_999), (false, true), "FEA-008.4");
    assert_eq!(g.gates(180 * s, 1, 1), (false, false), "FEA-008.4 stale minute");
    g.export(&mut FeatureRow::new(&mut b));
    assert_eq!(b, [1, 1_810_000_000, 1_450_000_000], "FEA-008.5");
}

#[test]
fn fea_009_zscore_sum() {
    let f = [Some(3_000_000_000), None, Some(-1_000_000_000)];
    let mu = [1_000_000_000, 0, 0];
    let sd = [1_000_000_000, 1, 2_000_000_000];
    // (3 − 1)/1 − 1·(−1)/2 = 2.5; √4 → 1.25 (the missing feature adds 0).
    assert_eq!(
        zsum(&f, &mu, &sd, &[1, 1, -1], 2_000_000_000),
        1_250_000_000,
        "FEA-009.1"
    );
    // .2 sd ≤ 0 skips the feature; √N ≤ 0 gives 0.
    assert_eq!(
        zsum(&f, &mu, &[0, 1, 2_000_000_000], &[1, 1, -1], 2_000_000_000),
        250_000_000,
        "FEA-009.2"
    );
    assert_eq!(zsum(&f, &mu, &sd, &[1, 1, -1], 0), 0, "FEA-009.2");
}

#[test]
fn fea_010_feature_row_and_events() {
    // .1 values beyond the storage are counted, not written; `put_opt(None)` writes NA.
    let mut b = [0i64; 2];
    let mut row = FeatureRow::new(&mut b);
    assert!(row.is_empty(), "FEA-010.1");
    row.put(5);
    row.put_opt(None);
    row.put(9);
    assert_eq!((row.len(), b), (3, [5, NA]), "FEA-010.1");
    // .2 event masks.
    assert!(
        Events::BBO.union(Events::TRADE).contains(Events::TRADE),
        "FEA-010.2"
    );
    assert!(!Events::BBO.contains(Events::BOOK), "FEA-010.2");
    assert_eq!(<SecHist<8> as Feature>::EVENTS, Events::ANY, "FEA-010.2");
    assert_eq!(<SignedFlow<8> as Feature>::EVENTS, Events::TRADE, "FEA-010.2");
    assert_eq!(<BboTop as Feature>::EVENTS, Events::BBO, "FEA-010.2");
}
