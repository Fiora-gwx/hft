//! Property test for U256 ↔ Qty (TYP-072).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod reference;

use hft_types::*;
use proptest::prelude::*;
use reference::{MODES, digits_to_limbs, ref_qty_from_u256};

/// (token_decimals, qty_exp) with `qty_exp ∈ [-18, 6]` and `d = td + qty_exp` in `[lo, hi]`.
fn exps(lo: i32, hi: i32) -> impl Strategy<Value = (u8, i8)> {
    (lo..=hi)
        .prop_flat_map(|d| {
            let qmin = (-18).max(d - 255);
            let qmax = 6.min(d);
            (Just(d), qmin..=qmax)
        })
        .prop_map(|(d, qe)| ((d - qe) as u8, qe as i8))
}

/// Four uniform limbs, or `10^k + delta` (k ∈ 0..=76, |delta| ≤ 1000), half each.
fn u256() -> impl Strategy<Value = [u64; 4]> {
    prop_oneof![
        any::<[u64; 4]>(),
        (0usize..=76, -1000i64..=1000).prop_map(|(k, delta)| {
            let mut d = vec![1u8];
            d.extend(std::iter::repeat_n(0, k));
            let base = digits_to_limbs(&d).unwrap();
            add_signed(base, delta)
        }),
    ]
}

fn add_signed(v: [u64; 4], delta: i64) -> [u64; 4] {
    let mut out = v;
    if delta >= 0 {
        let mut carry = delta as u128;
        for l in out.iter_mut() {
            let s = *l as u128 + carry;
            *l = s as u64;
            carry = s >> 64;
        }
    } else {
        let mut borrow = delta.unsigned_abs() as u128;
        for l in out.iter_mut() {
            let (r, b) = (*l as u128).overflowing_sub(borrow);
            *l = r as u64;
            borrow = if b { 1 } else { 0 };
            if !b {
                break;
            }
        }
        if borrow != 0 {
            return v;
        }
    }
    out
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(5_000))]

    #[test]
    fn typ_072_u256_round_trip(
        q in 0i64..=MAX_RAW,
        (td, qe) in exps(0, 36),
        v in u256(),
        (td2, qe2) in exps(-18, 36),
    ) {
        let limbs = qty_to_u256(Qty(q), td, qe).unwrap();
        for m in MODES {
            prop_assert_eq!(qty_from_u256(&limbs, td, qe, m), Ok(Qty(q)));
        }
        let mut res = Vec::new();
        for m in MODES {
            let got = qty_from_u256(&v, td2, qe2, m);
            prop_assert_eq!(got.map(|x| x.0), ref_qty_from_u256(&v, td2, qe2, m), "{:?}", m);
            res.push(got);
        }
        if let (Ok(f), Ok(c)) = (res[0], res[1]) {
            prop_assert!(c.0 - f.0 == 0 || c.0 - f.0 == 1);
        }
    }
}
