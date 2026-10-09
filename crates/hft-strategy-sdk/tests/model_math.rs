//! `model_math` (SDK-176, SDK-177).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use hft_strategy_sdk::model_math::{exp, ln, log10, powf, to_notional, to_rate};

#[test]
fn sdk_176_to_fixed() {
    let rows: [(&str, Option<i64>, Option<i64>); 13] = [
        ("SDK-176.1", to_rate(0.0005), Some(500_000)),
        ("SDK-176.2", to_rate(2.5e-9), Some(2)),
        ("SDK-176.3", to_rate(3.5e-9), Some(4)),
        ("SDK-176.4", to_rate(-2.5e-9), Some(-2)),
        ("SDK-176.5", to_rate(5e-10), Some(0)),
        ("SDK-176.6", to_rate(1e-10), Some(0)),
        ("SDK-176.7", to_notional(2.5e-6), Some(2)),
        ("SDK-176.8", to_notional(1234.5678905), Some(1_234_567_890)),
        ("SDK-176.9", to_rate(1e10), Some(i64::MAX)),
        ("SDK-176.10", to_rate(-1e10), Some(i64::MIN)),
        ("SDK-176.11", to_rate(f64::NAN), None),
        ("SDK-176.12", to_rate(f64::INFINITY), None),
        ("SDK-176.13", to_notional(f64::NEG_INFINITY), None),
    ];
    for (row, got, want) in rows {
        assert_eq!(got, want, "{row}");
    }
}

/// Bit patterns from the first x86_64 run (checked: ln 2, √2 and log10(1e-9) = −9 are the
/// correctly rounded values; libm's exp(1) is 1 ulp above e's correctly rounded 0x4005BF0A8B145769,
/// which is fine: the requirement is equality across platforms, not correct rounding).
#[test]
fn sdk_177_libm_bitwise() {
    let rows: [(&str, f64, u64); 10] = [
        ("SDK-177.1 ln(2)", ln(2.0), 0x3FE6_2E42_FEFA_39EF),
        ("SDK-177.1 ln(1e-300)", ln(1e-300), 0xC085_9634_47F8_7FB5),
        ("SDK-177.1 ln(1)", ln(1.0), 0x0000_0000_0000_0000),
        ("SDK-177.2 exp(1)", exp(1.0), 0x4005_BF0A_8B14_576A),
        ("SDK-177.2 exp(-745)", exp(-745.0), 0x0000_0000_0000_0001),
        ("SDK-177.2 exp(710)", exp(710.0), 0x7FF0_0000_0000_0000),
        (
            "SDK-177.3 powf(1.0001, 1e4)",
            powf(1.0001, 10_000.0),
            0x4005_BEC3_4AAB_BFD3,
        ),
        ("SDK-177.3 powf(2, 0.5)", powf(2.0, 0.5), 0x3FF6_A09E_667F_3BCD),
        ("SDK-177.4 log10(1e-9)", log10(1e-9), 0xC022_0000_0000_0000),
        ("SDK-177.4 log10(3)", log10(3.0), 0x3FDE_8927_964F_D5FD),
    ];
    for (row, x, bits) in rows {
        assert_eq!(x.to_bits(), bits, "{row}: got 0x{:016X}", x.to_bits());
    }
    assert_eq!(ln(1.0), 0.0);
    assert_eq!(exp(710.0), f64::INFINITY);
}
