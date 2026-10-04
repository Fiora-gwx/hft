//! Property test for `cap_cross_px` (TYP-122).

use hft_types::*;
use proptest::prelude::*;

const ONE: i128 = 1_000_000_000;

/// Exact reference: tighter bound (numerator over 1e9), rounded once to `tick` by the side.
fn reference(side: Side, touch: i64, mid: i64, mc: i64, rr: i64, tick: i64) -> (i128, i64) {
    let (t, m, mc, rr) = (touch as i128, mid as i128, mc as i128, rr as i128);
    let bound = match side {
        Side::Buy => (t * (ONE + mc)).min(m * (ONE + rr)),
        Side::Sell => (t * (ONE - mc)).max(m * (ONE - rr)),
    };
    let den = ONE * tick as i128;
    let k = match side {
        Side::Buy => bound.div_euclid(den),
        Side::Sell => -(-bound).div_euclid(den),
    };
    let v = k * tick as i128;
    (bound, if v <= 0 { 0 } else { v.min(MAX_RAW as i128) as i64 })
}

fn inputs() -> impl Strategy<Value = (i64, i64)> {
    (1i64..=1_000_000_000_000).prop_flat_map(|t| {
        let lo = (t - t / 5).max(1);
        let hi = t + t / 5;
        (Just(t), lo..=hi)
    })
}

fn tick() -> impl Strategy<Value = i64> {
    prop_oneof![(0u32..=6).prop_map(|k| 10i64.pow(k)), 1i64..=1_000_000]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn typ_122_cap_cross_matches_reference(
        (touch, mid) in inputs(),
        mc in 0i64..=100_000_000,
        rr in 0i64..=100_000_000,
        tick in tick(),
        side in prop::sample::select(vec![Side::Buy, Side::Sell]),
    ) {
        let lim = HedgePxLimits { conn_liveness_ms: 1000, rr_max_cross: Rate(rr) };
        let got = cap_cross_px(side, Px(touch), Px(mid), Rate(mc), &lim, tick);
        let (bound, want) = reference(side, touch, mid, mc, rr, tick);
        prop_assert_eq!(got, Px(want));
        if got.0 > 0 {
            prop_assert_eq!(got.0 % tick, 0);
            let g = got.0 as i128 * ONE;
            let step = tick as i128 * ONE;
            match side {
                Side::Buy => prop_assert!(g <= bound && bound - g < step),
                Side::Sell => prop_assert!(g >= bound && g - bound < step),
            }
        }
    }
}
