//! Parameter body codec: `n_fields_at`, `field_ix`, `encode_body`, `decode_body` (SDK-090,
//! SDK-091, SDK-094–SDK-097, SDK-101).
#![allow(clippy::type_complexity)] // table-driven rows

mod common;

use std::collections::BTreeMap;

use common::*;
use hft_strategy_sdk::params::*;
use proptest::prelude::*;

#[test]
fn sdk_090_n_fields_at() {
    for (row, ver, want) in [(1, 0, Some(0)), (2, 1, Some(5)), (3, 2, Some(6)), (4, 3, None)] {
        assert_eq!(n_fields_at(&TP, ver), want, "SDK-090.{row}");
    }
}

#[test]
fn sdk_091_field_ix() {
    for (row, name, want) in [
        (1, "open_th", Some(0)),
        (2, "mode", Some(5)),
        (3, "Open_th", None),
        (4, "", None),
    ] {
        assert_eq!(field_ix(&TP, name), want, "SDK-091.{row}");
    }
}

fn base_raw() -> RawParams {
    raw(
        vec![5_000_000, 20_000, 100_000, 0, 1, 0],
        &[(2, 0, 7_000_000), (0, 0, 6_000_000), (0, 0, 6_000_000)],
    )
}

#[test]
fn sdk_094_encode_body_layout() {
    let b = encode_body(&TP, 3, &base_raw()).unwrap();
    assert_eq!(b.len(), 88);
    assert_eq!(b[0..8], [6, 0, 0, 0, 2, 0, 0, 0]);
    for (i, v) in [5_000_000i64, 20_000, 100_000, 0, 1, 0].iter().enumerate() {
        assert_eq!(b[8 + 8 * i..16 + 8 * i], v.to_le_bytes(), "default {i}");
    }
    let ov = |g: u16, f: u16, v: i64| {
        let mut e = Vec::new();
        e.extend_from_slice(&g.to_le_bytes());
        e.extend_from_slice(&f.to_le_bytes());
        e.extend_from_slice(&[0; 4]);
        e.extend_from_slice(&v.to_le_bytes());
        e
    };
    assert_eq!(b[56..72], ov(0, 0, 6_000_000)[..]);
    assert_eq!(b[72..88], ov(2, 0, 7_000_000)[..]);
}

#[test]
fn sdk_095_encode_body_errors() {
    type Edit = fn(&mut RawParams);
    let rows: [(u32, Edit, Result<(), ParamError>); 9] = [
        (1, |r| r.defaults[0] = 100_000_000, Ok(())),
        (
            2,
            |r| r.defaults[0] = 100_000_001,
            Err(ParamError::OutOfRange {
                field_ix: 0,
                group_ix: 0xFFFF,
            }),
        ),
        (
            3,
            |r| r.defaults[2] = 9_999,
            Err(ParamError::OutOfRange {
                field_ix: 2,
                group_ix: 0xFFFF,
            }),
        ),
        (4, |r| r.defaults[5] = 3, Err(ParamError::BadEnum { field_ix: 5 })),
        (
            5,
            |r| r.overrides.push(ParamOverride::new(1, 1, 30_000)),
            Err(ParamError::NotPerGroup { field_ix: 1 }),
        ),
        (
            6,
            |r| r.overrides.push(ParamOverride::new(3, 0, 1)),
            Err(ParamError::GroupIxOutOfRange { group_ix: 3 }),
        ),
        (
            7,
            |r| r.overrides.push(ParamOverride::new(1, 0, 100_000_001)),
            Err(ParamError::OutOfRange {
                field_ix: 0,
                group_ix: 1,
            }),
        ),
        (
            8,
            |r| {
                r.overrides = vec![
                    ParamOverride::new(0, 0, 6_000_000),
                    ParamOverride::new(0, 0, 6_500_000),
                ]
            },
            Err(ParamError::OverrideOrder { ix: 1 }),
        ),
        (9, |r| r.defaults.truncate(5), Err(ParamError::BodyLen)),
    ];
    for (row, edit, want) in rows {
        let mut r = base_raw();
        edit(&mut r);
        assert_eq!(encode_body(&TP, 3, &r).map(|_| ()), want, "SDK-095.{row}");
    }
}

/// Unchecked test encoder: header, defaults and overrides exactly as given.
fn raw_body(defaults: &[i64], overrides: &[(u16, u16, i64)]) -> Vec<u64> {
    let mut b = Vec::new();
    b.extend_from_slice(&(defaults.len() as u16).to_le_bytes());
    b.extend_from_slice(&[0, 0]);
    b.extend_from_slice(&(overrides.len() as u32).to_le_bytes());
    for d in defaults {
        b.extend_from_slice(&d.to_le_bytes());
    }
    for &(g, f, v) in overrides {
        b.extend_from_slice(&g.to_le_bytes());
        b.extend_from_slice(&f.to_le_bytes());
        b.extend_from_slice(&[0; 4]);
        b.extend_from_slice(&v.to_le_bytes());
    }
    aligned(&b)
}

fn decode(ver: u16, body: &[u64]) -> Result<RawParams, ParamError> {
    decode_body(&TP, ver, 3, zerocopy::IntoBytes::as_bytes(body))
}

#[test]
fn sdk_096_decode_old_version_fills_default() {
    let body = raw_body(&[6_000_000, 25_000, 100_000, 0, 0], &[]);
    let r = decode(1, &body).unwrap();
    assert_eq!(r.defaults, vec![6_000_000, 25_000, 100_000, 0, 0, 0]);
    assert!(r.overrides.is_empty());
}

#[test]
fn sdk_097_decode_body_errors() {
    let d6 = [5_000_000, 20_000, 100_000, 0, 1, 0];
    let d5 = [5_000_000, 20_000, 100_000, 0, 1];
    let rows: [(u32, Vec<u64>, u16, ParamError); 7] = [
        (
            1,
            raw_body(&d6, &[]),
            3,
            ParamError::SchemaTooNew { have: 2, got: 3 },
        ),
        (2, raw_body(&d5, &[]), 2, ParamError::BodyLen),
        (3, raw_body(&d6, &[]), 1, ParamError::BodyLen),
        (
            4,
            raw_body(&[-1, 20_000, 100_000, 0, 1, 0], &[]),
            2,
            ParamError::OutOfRange {
                field_ix: 0,
                group_ix: 0xFFFF,
            },
        ),
        (
            5,
            raw_body(&d6, &[(0, 4, 0)]),
            2,
            ParamError::NotPerGroup { field_ix: 4 },
        ),
        (
            6,
            raw_body(&d6, &[(2, 2, 1_000_001)]),
            2,
            ParamError::OutOfRange {
                field_ix: 2,
                group_ix: 2,
            },
        ),
        (
            7,
            raw_body(&[5_000_000, 20_000, 100_000, 0, 1, 7], &[]),
            2,
            ParamError::BadEnum { field_ix: 5 },
        ),
    ];
    for (row, body, ver, want) in rows {
        assert_eq!(decode(ver, &body), Err(want), "SDK-097.{row}");
    }
}

/// A random schema (owned names/enum lists leaked for the `'static` descriptor).
#[derive(Debug, Clone)]
struct RandSchema {
    fields: Vec<(Unit, i64, i64, i64, bool)>,
}

impl RandSchema {
    fn desc(&self) -> ParamSchemaDesc {
        let fields: Vec<FieldDesc> = self
            .fields
            .iter()
            .enumerate()
            .map(|(i, &(unit, min, max, default, per_group))| FieldDesc {
                name: Box::leak(format!("f{i}").into_boxed_str()),
                unit,
                min,
                max,
                default,
                per_group: per_group || matches!(unit, Unit::Qty | Unit::Px),
                structural: false,
                manual_only: false,
                relax: Relax::None,
                minor_change: MinorChange::None,
                since: 1,
                doc: "",
            })
            .collect();
        ParamSchemaDesc {
            id: 1,
            version: 1,
            fields: Box::leak(fields.into_boxed_slice()),
        }
    }
}

const ENUM3: &[&str] = &["p", "q", "r"];

fn arb_field() -> impl Strategy<Value = (Unit, i64, i64, i64, bool)> {
    let unit = prop_oneof![
        Just(Unit::Rate),
        Just(Unit::Notional),
        Just(Unit::Qty),
        Just(Unit::Px),
        Just(Unit::DurationMs),
        Just(Unit::Count),
        Just(Unit::Bool),
        Just(Unit::Enum(ENUM3)),
    ];
    (
        unit,
        -1_000_000i64..1_000_000,
        0i64..2_000_000,
        any::<u64>(),
        prop::bool::weighted(0.3),
    )
        .prop_map(|(unit, lo, span, pick, pg)| {
            let (min, max) = match unit {
                Unit::Bool => (0, 1),
                Unit::Enum(n) => (0, n.len() as i64 - 1),
                _ => (lo, lo + span),
            };
            let default = min + (pick % ((max - min) as u64 + 1)) as i64;
            (unit, min, max, default, pg)
        })
}

fn arb_case() -> impl Strategy<Value = (RandSchema, u16, Vec<i64>, Vec<(u16, u16, i64)>)> {
    (prop::collection::vec(arb_field(), 1..=64), 0u16..=64).prop_flat_map(|(fields, n_groups)| {
        let s = RandSchema { fields };
        let pg: Vec<u16> = s
            .fields
            .iter()
            .enumerate()
            .filter(|(_, f)| f.4 || matches!(f.0, Unit::Qty | Unit::Px))
            .map(|(i, _)| i as u16)
            .collect();
        let defaults: Vec<_> = s.fields.iter().map(|&(_, min, max, _, _)| min..=max).collect();
        let n_ov = if pg.is_empty() || n_groups == 0 {
            0..=0
        } else {
            0..=4096usize
        };
        let ranges: Vec<(i64, i64)> = s.fields.iter().map(|f| (f.1, f.2)).collect();
        let ov = prop::collection::vec(
            (
                0..n_groups.max(1),
                0..pg.len().max(1),
                any::<u64>(),
                prop::bool::weighted(0.1),
            ),
            n_ov,
        )
        .prop_map(move |v| {
            let mut out: Vec<(u16, u16, i64)> = Vec::new();
            for (g, fi, pick, dup) in v {
                if dup && let Some(&last) = out.last() {
                    out.push(last);
                    continue;
                }
                let f = pg[fi];
                let (min, max) = ranges[usize::from(f)];
                out.push((g, f, min + (pick % ((max - min) as u64 + 1)) as i64));
            }
            out
        });
        (Just(s), Just(n_groups), defaults, ov)
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    #[test]
    fn sdk_101_encode_decode_roundtrip((s, n_groups, defaults, ov) in arb_case()) {
        let desc = s.desc();
        // Reference: last value per key wins only when values agree; keep the first value and
        // turn conflicting keys into identical duplicates so the input is legal.
        let mut map: BTreeMap<(u16, u16), i64> = BTreeMap::new();
        let mut legal = Vec::new();
        for (g, f, v) in ov {
            let v = *map.entry((g, f)).or_insert(v);
            legal.push((g, f, v));
        }
        let r = raw(defaults.clone(), &legal);
        let body = encode_body(&desc, n_groups, &r).unwrap();
        let al = aligned(&body);
        let bytes = bytes_of(&al, body.len());
        let v = body_view(bytes, n_groups).unwrap();
        prop_assert_eq!(v.defaults.len(), defaults.len());
        let back = decode_body(&desc, 1, n_groups, bytes).unwrap();
        prop_assert_eq!(&back.defaults, &defaults);
        let want: Vec<(u16, u16, i64)> = map.into_iter().map(|((g, f), v)| (g, f, v)).collect();
        let got: Vec<(u16, u16, i64)> = back.overrides.iter().map(|o| (o.group_ix, o.field_ix, o.value)).collect();
        prop_assert_eq!(got, want);
    }
}
