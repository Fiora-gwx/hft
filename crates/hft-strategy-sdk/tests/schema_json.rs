//! Canonical schema JSON, `schema_digest` and `OwnedSchema` (SDK-106–SDK-109).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::common;

use std::path::PathBuf;

use common::*;
use hft_strategy_sdk::params::*;
use hft_strategy_sdk::*;
use proptest::prelude::*;
use proptest::strategy::Strategy;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/params/sdk-tp")
        .join(name)
}

fn golden() -> String {
    std::fs::read_to_string(fixture("schema.golden.json")).unwrap()
}

/// `schema_digest` recorded beside the golden (`schema.golden.meta.toml`).
fn golden_digest() -> u64 {
    let meta: toml::Table = std::fs::read_to_string(fixture("schema.golden.meta.toml"))
        .unwrap()
        .parse()
        .unwrap();
    u64::from_str_radix(meta["schema_digest"].as_str().unwrap(), 16).unwrap()
}

#[test]
fn sdk_106_schema_json_golden() {
    let got = schema_json(&EchoProbe::DESC, &ModelLayoutDesc::EMPTY, 2);
    assert_eq!(got, golden().trim_end());
    // `sections` lists the Model layout's tags and names (ProbeModel).
    let with_model = schema_json(&EchoProbe::DESC, &PROBE_LAYOUT, 2);
    assert!(with_model.ends_with(r#","sections":[{"tag":1,"name":"ticks"},{"tag":2,"name":"samples"}]}"#));
    assert_eq!(
        with_model.replace(r#"{"tag":1,"name":"ticks"},{"tag":2,"name":"samples"}"#, ""),
        got
    );
}

/// A copy of `EchoProbe::DESC` with its own `TP` field array, edited by `f`.
fn variant(f: impl FnOnce(&mut StrategyDesc, &mut Vec<FieldDesc>)) -> &'static StrategyDesc {
    let mut d = EchoProbe::DESC;
    let mut fields = TP_FIELDS.to_vec();
    f(&mut d, &mut fields);
    let schema = Box::leak(Box::new(ParamSchemaDesc {
        id: TP.id,
        version: TP.version,
        fields: Box::leak(fields.into_boxed_slice()),
    }));
    d.schema = schema;
    Box::leak(Box::new(d))
}

#[test]
fn sdk_107_digest_scope() {
    let d0 = schema_digest(&EchoProbe::DESC, 2);
    assert_eq!(d0, golden_digest(), "SDK-107.1");
    type Edit = fn(&mut StrategyDesc, &mut Vec<FieldDesc>);
    let same: [(u32, Edit); 7] = [
        (2, |_, f| f[0].doc = "另一段说明"),
        (3, |_, f| f[2].structural = false),
        (4, |_, f| f[4].manual_only = false),
        (5, |_, f| f[0].relax = Relax::Up),
        (6, |_, f| f[0].minor_change = MinorChange::Any),
        (7, |d, _| d.non_prod_only = true),
        (8, |d, _| {
            d.notes = ECHO_NOTES.split_at(2).0;
            d.report = &[];
            d.cmds = ECHO_CMDS.split_at(1).0;
        }),
    ];
    for (row, edit) in same {
        assert_eq!(schema_digest(variant(edit), 2), d0, "SDK-107.{row}");
    }
    let differ: [(u32, Edit); 5] = [
        (9, |_, f| f[0].max = 100_000_001),
        (10, |_, f| f[1].default = 20_001),
        (11, |_, f| {
            f[3].per_group = false;
            f[3].unit = Unit::Notional;
        }),
        (12, |_, f| f[1].name = "cool_ms"),
        (13, |_, f| f[5].unit = Unit::Enum(&["a", "b", "z"])),
    ];
    for (row, edit) in differ {
        assert_ne!(schema_digest(variant(edit), 2), d0, "SDK-107.{row}");
    }
    assert_ne!(schema_digest(&EchoProbe::DESC, 1), d0, "SDK-107.14");
    assert_ne!(
        schema_digest(variant(|d, _| d.kind_id = 0xFF09), 2),
        d0,
        "SDK-107.15"
    );
}

fn arb_raw() -> impl Strategy<Value = (Vec<i64>, Vec<(u16, u16, i64)>)> {
    let defaults: Vec<_> = TP_FIELDS.iter().map(|f| f.min..=f.max).collect();
    let per_group: Vec<u16> = (0..6).filter(|&i| TP_FIELDS[usize::from(i)].per_group).collect();
    let ov = prop::collection::vec((0u16..3, 0..per_group.len(), any::<u64>()), 0..64).prop_map(move |v| {
        let mut out: Vec<(u16, u16, i64)> = Vec::new();
        for (g, fi, pick) in v {
            let f = per_group[fi];
            if out.iter().any(|o| (o.0, o.1) == (g, f)) {
                continue;
            }
            let d = &TP_FIELDS[usize::from(f)];
            out.push((g, f, d.min + (pick % ((d.max - d.min) as u64 + 1)) as i64));
        }
        out
    });
    (defaults, ov)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1_000))]

    #[test]
    fn sdk_108_owned_schema_equivalent((defaults, ov) in arb_raw()) {
        let owned = OwnedSchema::from_json(&schema_json(&EchoProbe::DESC, &ModelLayoutDesc::EMPTY, 2)).unwrap();
        let r = raw(defaults, &ov);
        let a = encode_body(&TP, 3, &r);
        let b = encode_body(&owned, 3, &r);
        prop_assert_eq!(&a, &b);
        let body = a.unwrap();
        let al = aligned(&body);
        let bytes = bytes_of(&al, body.len());
        prop_assert_eq!(decode_body(&TP, 2, 3, bytes), decode_body(&owned, 2, 3, bytes));
        prop_assert_eq!(owned.schema_digest(2), schema_digest(&EchoProbe::DESC, 2));
        prop_assert_eq!(owned.schema_digest(1), schema_digest(&EchoProbe::DESC, 1));
    }
}

#[test]
fn sdk_109_owned_schema_rejects() {
    let g = golden();
    let g = g.trim_end();
    assert!(OwnedSchema::from_json(g).is_ok(), "SDK-109.1");
    assert!(OwnedSchema::from_json(&g[..g.len() / 2]).is_err(), "SDK-109.2");
    let first_min = r#""min":0,"max":100000000,"default":5000000"#;
    assert!(g.contains(first_min));
    let rows: [(u32, String); 3] = [
        (
            3,
            g.replacen(first_min, r#""max":100000000,"default":5000000"#, 1),
        ),
        (4, g.replacen(r#""unit":"rate""#, r#""unit":"Foo""#, 1)),
        (
            5,
            g.replacen(first_min, r#""min":10,"max":100000000,"default":9"#, 1),
        ),
    ];
    for (row, text) in rows {
        assert_ne!(text, g, "SDK-109.{row} edit applied");
        assert!(OwnedSchema::from_json(&text).is_err(), "SDK-109.{row}");
    }
}

#[test]
fn sdk_107_production_minimum_roundtrip_and_digest() {
    let d = variant(|_, f| {
        f[0].min = -100_000_000;
        f[0].prod_min = Some(0);
    });
    let json = schema_json(d, &ModelLayoutDesc::EMPTY, 2);
    let owned = OwnedSchema::from_json(&json).unwrap();
    assert_eq!(owned.field(0).unwrap().prod_min, Some(0));
    assert_eq!(owned.schema_digest(2), schema_digest(d, 2));
    let without = variant(|_, f| {
        f[0].min = -100_000_000;
    });
    assert_ne!(schema_digest(d, 2), schema_digest(without, 2));
    let mut values: Vec<_> = d.schema.fields.iter().map(|f| f.default).collect();
    values[0] = -1;
    assert!(check_production_values(&owned, &values, false, GROUP_IX_NONE).is_ok());
    assert_eq!(
        check_production_values(&owned, &values, true, 2),
        Err(ParamError::OutOfRange {
            field_ix: 0,
            group_ix: 2
        })
    );
    values[0] = 0;
    assert!(check_production_values(&owned, &values, true, 2).is_ok());
    assert!(OwnedSchema::from_json(&json.replace("\"prod_min\":0", "\"prod_min\":false")).is_err());
}

#[test]
fn sdk_105_bound_only_schema_revision_preserves_field_prefix() {
    hft_strategy_sdk::define_params! {
        pub struct BoundsOnly(schema_id = 65400, version = 3, fields = BoundsFields) {
            threshold: Rate [min = -100_000_000, max = 100_000_000, prod_min = 0, default = 0, per_group],
        }
    }
    let groups = hft_strategy_sdk::GroupTable::new(&[], 0);
    let body = encode_body(
        &BoundsOnly::SCHEMA,
        0,
        &RawParams {
            defaults: vec![-3_000_000],
            overrides: vec![],
        },
    )
    .unwrap();
    for version in [1, 2, 3] {
        assert_eq!(
            BoundsOnly::decode(&body, version, &groups)
                .unwrap()
                .defaults()
                .threshold
                .0,
            -3_000_000
        );
    }
}
