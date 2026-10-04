//! TYP-050: any bytes + one exponent byte → `parse_exact` and `parse_round` (four modes),
//! differential against the reference parser in `tests/reference.rs`.
#![no_main]

#[path = "../../tests/reference.rs"]
mod reference;

use hft_types::{MAX_RAW, parse_exact, parse_round};
use libfuzzer_sys::fuzz_target;
use reference::{MODES, ref_parse};

fn typ_050_fuzz_parse(data: &[u8]) {
    let Some((&e, s)) = data.split_first() else { return };
    let exp = e as i8;
    let got = parse_exact(s, exp);
    assert_eq!(got, ref_parse(s, exp, None), "exact {s:?} exp {exp}");
    if let Ok(v) = got {
        assert!(v.unsigned_abs() <= MAX_RAW as u64);
    }
    for m in MODES {
        let got = parse_round(s, exp, m);
        assert_eq!(got, ref_parse(s, exp, Some(m)), "{m:?} {s:?} exp {exp}");
        if let Ok(v) = got {
            assert!(v.unsigned_abs() <= MAX_RAW as u64);
        }
    }
}

fuzz_target!(|data: &[u8]| typ_050_fuzz_parse(data));
