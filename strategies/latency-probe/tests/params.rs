//! Parameter schema and `check` (TESTS.md LPR-004–LPR-007).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::type_complexity
)] // allow-list: test code

mod common;

use common::*;
use hft_strategy_sdk::params::{
    GROUP_IX_NONE, MinorChange, RawParams, Relax, Unit, encode_body, schema_json,
};
use hft_strategy_sdk::{ModelLayoutDesc, ParamError, ParamSet, RefView, Strategy};
use hft_types::GroupId;
use latency_probe::{LatencyProbe, ProbeParams};

#[test]
fn lpr_004_schema_defaults() {
    let s = ProbeParams::SCHEMA;
    assert_eq!((s.id, s.version, s.fields.len()), (5, 1, 17));
    use Relax::{Down, None as N, Up};
    let rows: &[(&str, &str, Unit, i64, bool, bool, Relax)] = &[
        ("LPR-004.1", "enabled", Unit::Bool, 0, false, true, Up),
        (
            "LPR-004.2",
            "mode",
            Unit::Enum(&["quote", "pair", "target"]),
            0,
            true,
            true,
            N,
        ),
        (
            "LPR-004.3",
            "probe_period_ms",
            Unit::DurationMs,
            1_000,
            true,
            false,
            Down,
        ),
        ("LPR-004.4", "far_bp", Unit::Rate, 30_000_000, true, false, Down),
        (
            "LPR-004.5",
            "quote_side",
            Unit::Enum(&["buy", "sell", "alternate"]),
            0,
            true,
            false,
            N,
        ),
        (
            "LPR-004.6",
            "quote_rest_ms",
            Unit::DurationMs,
            2_000,
            true,
            false,
            N,
        ),
        ("LPR-004.7", "quote_xgw", Unit::Bool, 0, true, false, N),
        (
            "LPR-004.8",
            "burst_period_ms",
            Unit::DurationMs,
            0,
            true,
            false,
            N,
        ),
        (
            "LPR-004.9",
            "pair_period_ms",
            Unit::DurationMs,
            10_000,
            true,
            false,
            Down,
        ),
        (
            "LPR-004.10",
            "target_period_ms",
            Unit::DurationMs,
            60_000,
            true,
            false,
            Down,
        ),
        (
            "LPR-004.11",
            "pair_flat_ms",
            Unit::DurationMs,
            5_000,
            true,
            false,
            N,
        ),
        ("LPR-004.12", "pair_cross", Unit::Rate, 5_000_000, true, false, Up),
        (
            "LPR-004.13",
            "pair_side",
            Unit::Enum(&["auto", "buy_a", "sell_a"]),
            0,
            true,
            false,
            N,
        ),
        (
            "LPR-004.14",
            "size_margin",
            Unit::Rate,
            100_000_000,
            false,
            false,
            N,
        ),
        (
            "LPR-004.15",
            "max_spread",
            Unit::Rate,
            10_000_000,
            true,
            false,
            Up,
        ),
        (
            "LPR-004.16",
            "max_inv_notional",
            Unit::Notional,
            100_000_000,
            true,
            false,
            Up,
        ),
        ("LPR-004.17", "max_consec_fail", Unit::Count, 5, false, false, Up),
    ];
    for (ix, &(row, name, unit, def, pg, mo, relax)) in rows.iter().enumerate() {
        let f = &s.fields[ix];
        assert_eq!(
            (
                f.name,
                f.unit,
                f.default,
                f.per_group,
                f.manual_only,
                f.relax,
                f.since
            ),
            (name, unit, def, pg, mo, relax, 1),
            "{row}"
        );
    }
    for f in s.fields {
        assert!(
            !f.structural && f.minor_change == MinorChange::None,
            "LPR-004.18 {}",
            f.name
        );
    }
    let js = schema_json(&LatencyProbe::DESC, &ModelLayoutDesc::EMPTY, 1);
    assert_eq!(js.matches("\"minor_change\":\"none\"").count(), 17, "LPR-004.18");
}

#[test]
fn lpr_005_ranges() {
    let rows: &[(&str, u16, [i64; 4])] = &[
        ("LPR-005.1", 2, [99, 100, 600_000, 600_001]),
        ("LPR-005.2", 3, [4_999_999, 5_000_000, 100_000_000, 100_000_001]),
        ("LPR-005.3", 5, [99, 100, 600_000, 600_001]),
        ("LPR-005.4", 7, [-1, 0, 600_000, 600_001]),
        ("LPR-005.5", 8, [9_999, 10_000, 3_600_000, 3_600_001]),
        ("LPR-005.6", 9, [9_999, 10_000, 3_600_000, 3_600_001]),
        ("LPR-005.7", 10, [999, 1_000, 600_000, 600_001]),
        ("LPR-005.8", 11, [99_999, 100_000, 50_000_000, 50_000_001]),
        ("LPR-005.9", 13, [-1, 0, 1_000_000_000, 1_000_000_001]),
        ("LPR-005.10", 14, [99_999, 100_000, 100_000_000, 100_000_001]),
        (
            "LPR-005.11",
            15,
            [999_999, 1_000_000, 10_000_000_000, 10_000_000_001],
        ),
        ("LPR-005.12", 16, [0, 1, 100, 101]),
    ];
    let enc = |ix: u16, v: i64| {
        let mut d = pp0();
        d[usize::from(ix)] = v;
        encode_body(
            &ProbeParams::SCHEMA,
            0,
            &RawParams {
                defaults: d,
                overrides: vec![],
            },
        )
        .map(|_| ())
    };
    for &(row, ix, vals) in rows {
        let oor = Err(ParamError::OutOfRange {
            field_ix: ix,
            group_ix: GROUP_IX_NONE,
        });
        assert_eq!(enc(ix, vals[0]), oor, "{row} lower − 1");
        assert_eq!(enc(ix, vals[1]), Ok(()), "{row} lower");
        assert_eq!(enc(ix, vals[2]), Ok(()), "{row} upper");
        assert_eq!(enc(ix, vals[3]), oor, "{row} upper + 1");
    }
    assert_eq!(enc(1, 3), Err(ParamError::BadEnum { field_ix: 1 }), "LPR-005.13");
}

fn check(groups: &[G]) -> Result<(), ParamError> {
    let rd = rl1();
    let p = params(&[], groups);
    p.validate(&table(groups), &RefView::new(&rd, T1))
}

fn q(id: u32, insts: &[hft_types::InstId]) -> G {
    g(GroupId(0x0500_0100 + id), insts, QUOTE)
}

#[test]
fn lpr_006_check_group_shape() {
    let shape = |gix, code| Err(ParamError::GroupShape { gix, code });
    let rows: Vec<(&str, Vec<G>, Result<(), ParamError>)> = vec![
        ("LPR-006.1", vec![gq()], Ok(())),
        ("LPR-006.2", vec![q(1, &[])], shape(0, 1)),
        ("LPR-006.3", vec![q(1, &[I1, I2, I3, I5, I4])], shape(0, 1)),
        ("LPR-006.4", vec![q(1, &[I1, I2, I3, I5])], Ok(())),
        ("LPR-006.5", vec![g(GPS, &[I2], PAIR)], shape(0, 1)),
        ("LPR-006.6", vec![g(GTS, &[I1, I2, I3], TARGET)], shape(0, 1)),
        ("LPR-006.7", vec![g(GPS, &[I1, I4], PAIR)], shape(0, 2)),
        ("LPR-006.8", vec![g(GTS, &[I4, I1], TARGET)], shape(0, 2)),
        ("LPR-006.9", vec![q(1, &[I2]).with(f::QUOTE_XGW, 1)], shape(0, 3)),
        (
            "LPR-006.10",
            vec![q(1, &[I2, I5]).with(f::QUOTE_XGW, 1)],
            shape(0, 3),
        ),
        ("LPR-006.11", vec![gx()], Ok(())),
        ("LPR-006.12", vec![gps().with(f::QUOTE_XGW, 1)], Ok(())),
        ("LPR-006.13", vec![gq(), q(2, &[])], shape(1, 1)),
    ];
    for (row, groups, want) in rows {
        assert_eq!(check(&groups), want, "{row}");
    }
}

#[test]
fn lpr_007_check_fields() {
    let inv = |field_ix| Err(ParamError::Invalid { field_ix, code: 1 });
    let rows: Vec<(&str, Vec<G>, Result<(), ParamError>)> = vec![
        ("LPR-007.1", vec![q(1, &[I1]).with(f::QUOTE_SIDE, 1)], inv(4)),
        ("LPR-007.2", vec![q(1, &[I1]).with(f::QUOTE_SIDE, 2)], Ok(())),
        ("LPR-007.3", vec![q(2, &[I2]).with(f::QUOTE_SIDE, 1)], Ok(())),
        ("LPR-007.4", vec![gps().with(f::PAIR_SIDE, 2)], inv(12)),
        ("LPR-007.5", vec![gps().with(f::PAIR_SIDE, 1)], Ok(())),
        ("LPR-007.6", vec![gsp().with(f::PAIR_SIDE, 1)], inv(12)),
        ("LPR-007.7", vec![gsp().with(f::PAIR_SIDE, 2)], Ok(())),
        ("LPR-007.8", vec![gpp().with(f::PAIR_SIDE, 2)], Ok(())),
        (
            "LPR-007.9",
            vec![
                gq().with(f::BURST_PERIOD_MS, 1_001)
                    .with(f::PROBE_PERIOD_MS, 1_000),
            ],
            inv(7),
        ),
        (
            "LPR-007.10",
            vec![
                gq().with(f::BURST_PERIOD_MS, 1_000)
                    .with(f::PROBE_PERIOD_MS, 1_000),
            ],
            Ok(()),
        ),
        ("LPR-007.11", vec![gq().with(f::BURST_PERIOD_MS, 999)], Ok(())),
        (
            "LPR-007.12",
            vec![q(3, &[]).with(f::QUOTE_SIDE, 1)],
            Err(ParamError::GroupShape { gix: 0, code: 1 }),
        ),
    ];
    for (row, groups, want) in rows {
        assert_eq!(check(&groups), want, "{row}");
    }
}
