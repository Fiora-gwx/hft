//! Property tests for fixed-point arithmetic (TYP-004, TYP-012, TYP-020, TYP-024).

mod reference;

use hft_types::*;
use proptest::prelude::*;
use reference::{MODES, ref_div_round, ref_mul_qty};

/// Edge values {±MAX_RAW, ±(MAX_RAW − 1), 0, ±1} and uniform `[-MAX_RAW, MAX_RAW]`, half each.
fn raw_edge_or_uniform() -> impl Strategy<Value = i64> {
    prop_oneof![
        prop::sample::select(vec![MAX_RAW, -MAX_RAW, MAX_RAW - 1, -(MAX_RAW - 1), 0, 1, -1]),
        -MAX_RAW..=MAX_RAW,
    ]
}

/// Log-uniform magnitude (BTC/PEPE scales) with random sign.
fn log_uniform() -> impl Strategy<Value = i64> {
    (0u32..=18, 1i64..10, any::<bool>()).prop_map(|(k, m, neg)| {
        let v = (m as i128 * 10i128.pow(k)).min(MAX_RAW as i128) as i64;
        if neg { -v } else { v }
    })
}

fn mode() -> impl Strategy<Value = RoundMode> {
    prop::sample::select(MODES.to_vec())
}

/// Legal exponent pairs: both in [-18, 6] and sum ≤ -6.
fn legal_exp() -> impl Strategy<Value = AssetExp> {
    (-18i8..=6)
        .prop_flat_map(|pe| (Just(pe), -18i8..=(-6 - pe).min(6)))
        .prop_map(|(px_exp, qty_exp)| AssetExp { px_exp, qty_exp })
}

/// Step: powers of ten and uniform `[1, 10^12]`, half each.
fn step() -> impl Strategy<Value = i64> {
    prop_oneof![(0u32..=12).prop_map(|k| 10i64.pow(k)), 1i64..=1_000_000_000_000]
}

/// Uniform values and values within ±10^13 of ±MAX_RAW, half each.
fn raw_near_bounds() -> impl Strategy<Value = i64> {
    prop_oneof![
        -MAX_RAW..=MAX_RAW,
        (0i64..=10_000_000_000_000, any::<bool>()).prop_map(|(d, neg)| if neg {
            -MAX_RAW + d
        } else {
            MAX_RAW - d
        }),
    ]
}

/// Reference rounding to a step with the README saturation rule.
fn ref_round_step(x: i64, step: i64, m: RoundMode) -> i64 {
    let lim = (MAX_RAW / step) as i128 * step as i128;
    let v = ref_div_round(x as i128, step as i128, m) * step as i128;
    v.clamp(-lim, lim) as i64
}

fn check_step_rounding(x: i64, step: i64, m: RoundMode, got: i64) -> Result<(), TestCaseError> {
    prop_assert_eq!(got % step, 0);
    prop_assert!(got.abs() <= MAX_RAW);
    prop_assert_eq!(got, ref_round_step(x, step, m));
    let unsat = ref_div_round(x as i128, step as i128, m) * step as i128;
    if unsat == got as i128 {
        prop_assert!((got as i128 - x as i128).abs() < step as i128);
        match m {
            RoundMode::Floor => prop_assert!(got <= x),
            RoundMode::Ceil => prop_assert!(got >= x),
            _ => {}
        }
    }
    Ok(())
}

/// Smallest valid first-leg quantity by brute force over multiples of `lot_f` when the
/// multiple is small; otherwise verify validity and minimality via the prime factors of the
/// multiple (valid multiples are closed under gcd).
fn ref_pair_lot(lot_f: i64, lot_h: i64, num: u32, den: u32) -> i128 {
    let m = den as i128 * lot_h as i128;
    let valid = |k: i128| (lot_f as i128 * k * num as i128) % m == 0;
    for k in 1..=10_000i128 {
        if valid(k) {
            return lot_f as i128 * k;
        }
    }
    // k0 = m / gcd(m, lot_f × num) is valid; it is minimal iff k0 / p is invalid for every prime p | k0.
    let g = gcd(m, lot_f as i128 * num as i128);
    let k0 = m / g;
    assert!(valid(k0));
    let mut n = k0;
    let mut p = 2i128;
    while p * p <= n {
        if n % p == 0 {
            assert!(!valid(k0 / p), "k0 not minimal");
            while n % p == 0 {
                n /= p;
            }
        }
        p += 1;
    }
    if n > 1 {
        assert!(!valid(k0 / n), "k0 not minimal");
    }
    lot_f as i128 * k0
}

fn gcd(a: i128, b: i128) -> i128 {
    if b == 0 { a } else { gcd(b, a % b) }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn typ_004_add_sub_never_overflow(a in raw_edge_or_uniform(), b in raw_edge_or_uniform()) {
        prop_assert_eq!((Px(a) + Px(b)).0 as i128, a as i128 + b as i128);
        prop_assert_eq!((Px(a) - Px(b)).0 as i128, a as i128 - b as i128);
        prop_assert_eq!((Qty(a) + Qty(b)).0 as i128, a as i128 + b as i128);
        prop_assert_eq!((Qty(a) - Qty(b)).0 as i128, a as i128 - b as i128);
    }

    #[test]
    fn typ_012_mul_qty_matches_reference(
        px in prop_oneof![-MAX_RAW..=MAX_RAW, log_uniform()],
        q in prop_oneof![-MAX_RAW..=MAX_RAW, log_uniform()],
        e in legal_exp(),
        m in mode(),
    ) {
        prop_assert_eq!(Px(px).mul_qty(Qty(q), e, m), ref_mul_qty(px, q, e, m));
    }

    #[test]
    fn typ_020_rounding_invariants(
        tick in step(),
        px in raw_near_bounds(),
        m in mode(),
    ) {
        check_step_rounding(px, tick, m, Px(px).round_to_tick(tick, m).0)?;
        check_step_rounding(px, tick, m, Qty(px).round_to_lot(tick, m).0)?;
        let (a, r) = Qty(px).split_lot(tick);
        prop_assert_eq!(a.0 + r.0, px);
        prop_assert_eq!(a.0 % tick, 0);
        prop_assert!(r.0.abs() < tick);
        prop_assert!(r.0 == 0 || (r.0 < 0) == (px < 0));
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    #[test]
    fn typ_024_pair_lot_matches_brute_force(
        lot_f in 1i64..=1_000_000,
        lot_h in 1i64..=1_000_000,
        num in 1u32..=1000,
        den in 1u32..=1000,
    ) {
        let want = ref_pair_lot(lot_f, lot_h, num, den);
        let got = pair_lot(lot_f, lot_h, num, den);
        if want <= MAX_RAW as i128 {
            prop_assert_eq!(got, Some(Qty(want as i64)));
        } else {
            prop_assert_eq!(got, None);
        }
    }
}

#[test]
fn reference_pair_lot_self_check() {
    assert_eq!(ref_pair_lot(4, 6, 3, 2), 4);
    assert_eq!(
        ref_pair_lot(999_983, 999_979, 997, 991),
        pair_lot(999_983, 999_979, 997, 991).unwrap().0 as i128
    );
}
