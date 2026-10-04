//! Property tests for parsing and formatting (TYP-045, TYP-047, TYP-048).

mod reference;

use hft_types::*;
use proptest::prelude::*;
use reference::{MODES, ref_parse};

fn digits(max: usize) -> impl Strategy<Value = String> {
    proptest::collection::vec(prop::sample::select(b"0123456789".to_vec()), 0..=max)
        .prop_map(|v| String::from_utf8(v).unwrap())
}

/// Sign, 0–25 integer digits, optional 0–25 fraction digits, optional exponent (0–999, three
/// sign spellings, `e`/`E`), at most 40 bytes; 10% random byte strings.
fn number() -> impl Strategy<Value = Vec<u8>> {
    let structured = (
        any::<bool>(),
        digits(25),
        proptest::option::of(digits(25)),
        proptest::option::of((
            prop::sample::select(vec!["e", "E"]),
            prop::sample::select(vec!["", "+", "-"]),
            0u32..=999,
        )),
    )
        .prop_map(|(neg, int, frac, exp)| {
            let mut s = String::new();
            if neg {
                s.push('-');
            }
            s.push_str(&int);
            if let Some(f) = frac {
                s.push('.');
                s.push_str(&f);
            }
            if let Some((e, sign, x)) = exp {
                s.push_str(e);
                s.push_str(sign);
                s.push_str(&x.to_string());
            }
            let mut b = s.into_bytes();
            b.truncate(40);
            b
        });
    prop_oneof![9 => structured, 1 => proptest::collection::vec(any::<u8>(), 0..=44)]
}

/// Values for the round trip: uniform, powers of ten and powers of ten ± 1, a third each.
fn raw_for_round_trip() -> impl Strategy<Value = i64> {
    prop_oneof![
        -MAX_RAW..=MAX_RAW,
        (0u32..=18, any::<bool>()).prop_map(|(k, neg)| if neg { -(10i64.pow(k)) } else { 10i64.pow(k) }),
        (0u32..=18, any::<bool>(), prop::sample::select(vec![-1i64, 1])).prop_map(|(k, neg, d)| {
            let v = 10i64.pow(k) + d;
            if neg { -v } else { v }
        }),
    ]
}

fn is_shortest_decimal(s: &str) -> bool {
    let body = s.strip_prefix('-').unwrap_or(s);
    let (int, frac) = match body.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (body, None),
    };
    let int_ok =
        int == "0" || (!int.is_empty() && !int.starts_with('0') && int.bytes().all(|b| b.is_ascii_digit()));
    let frac_ok = match frac {
        None => true,
        Some(f) => !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit()) && !f.ends_with('0'),
    };
    int_ok && frac_ok
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn typ_045_parse_matches_reference(s in number(), exp in -30i8..=18) {
        prop_assert_eq!(parse_exact(&s, exp), ref_parse(&s, exp, None), "exact {:?}", String::from_utf8_lossy(&s));
        for m in MODES {
            prop_assert_eq!(parse_round(&s, exp, m), ref_parse(&s, exp, Some(m)), "{:?} {:?}", m, String::from_utf8_lossy(&s));
        }
    }

    #[test]
    fn typ_047_write_parse_round_trip(x in raw_for_round_trip(), e in -30i8..=18) {
        let mut out = [0u8; 40];
        let n = write_fixed(x, e, &mut out);
        let s = std::str::from_utf8(&out[..n]).unwrap();
        prop_assert!(n <= 38, "{}", s);
        prop_assert!(is_shortest_decimal(s), "{}", s);
        if x == 0 {
            prop_assert_eq!(s, "0");
        }
        prop_assert_eq!(parse_exact(&out[..n], e), Ok(x), "{}", s);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(5_000))]

    #[test]
    fn typ_048_tick_multiple_decimals(
        (t, k) in (0u32..=12).prop_flat_map(|t| {
            let lim = MAX_RAW / 10i64.pow(t);
            (Just(t), -lim..=lim)
        }),
        e in -18i8..=6,
    ) {
        let raw = 10i64.pow(t) * k;
        let mut out = [0u8; 40];
        let n = write_fixed(raw, e, &mut out);
        let s = std::str::from_utf8(&out[..n]).unwrap();
        let decimals = s.split_once('.').map_or(0, |(_, f)| f.len()) as i32;
        prop_assert!(decimals <= 0.max(-(e as i32 + t as i32)), "{} t={} e={}", s, t, e);
    }
}
