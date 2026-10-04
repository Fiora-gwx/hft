//! Text ↔ raw value per unit (SDK-092, SDK-093).
#![allow(clippy::type_complexity)] // table-driven rows

mod common;

use common::*;
use hft_strategy_sdk::params::*;
use hft_types::AssetExp;
use proptest::prelude::*;

fn tp(name: &str) -> FieldRef<'static> {
    TP.field(field_ix(&TP, name).unwrap()).unwrap()
}

fn aux(name: &str) -> FieldRef<'static> {
    AUX.field(field_ix(&AUX, name).unwrap()).unwrap()
}

#[test]
fn sdk_092_parse_value() {
    let bt = |ix| Err(ParamError::BadText { field_ix: ix });
    let rows: [(
        u32,
        FieldRef<'static>,
        &str,
        Option<AssetExp>,
        Result<i64, ParamError>,
    ); 19] = [
        (1, tp("open_th"), "0.5%", None, Ok(5_000_000)),
        (2, tp("open_th"), "50bp", None, Ok(5_000_000)),
        (3, tp("open_th"), "0.005", None, Ok(5_000_000)),
        (4, tp("open_th"), "-0.5%", None, Ok(-5_000_000)),
        (5, tp("open_th"), "0.0000000001", None, bt(0)),
        (6, tp("open_th"), "abc", None, bt(0)),
        (7, tp("cooldown_ms"), "5m", None, Ok(300_000)),
        (8, tp("cooldown_ms"), "30s", None, Ok(30_000)),
        (9, tp("cooldown_ms"), "2h", None, Ok(7_200_000)),
        (10, tp("cooldown_ms"), "1500", None, Ok(1_500)),
        (11, aux("n"), "1500", None, Ok(1_500)),
        (12, aux("cap"), "1500", None, Ok(1_500_000_000)),
        (13, tp("enabled"), "true", None, Ok(1)),
        (14, tp("enabled"), "false", None, Ok(0)),
        (15, tp("mode"), "b", None, Ok(1)),
        (
            16,
            tp("mode"),
            "d",
            None,
            Err(ParamError::BadEnum { field_ix: 5 }),
        ),
        (17, tp("max_qty"), "0.5", Some(BTC), Ok(50_000_000)),
        (18, tp("max_qty"), "0.5", None, bt(3)),
        (19, tp("max_qty"), "0.000000001", Some(BTC), bt(3)),
    ];
    for (row, f, text, exp, want) in rows {
        assert_eq!(parse_value(f, text, exp), want, "SDK-092.{row}");
    }
}

/// Every field of `TP` and the auxiliary schema.
fn all_fields() -> Vec<FieldRef<'static>> {
    let mut v: Vec<FieldRef<'static>> = (0..TP.n_fields()).map(|i| TP.field(i).unwrap()).collect();
    v.extend((0..AUX.n_fields()).map(|i| AUX.field(i).unwrap()));
    v
}

fn arb_case() -> impl Strategy<Value = (usize, i64, i8)> {
    let n = all_fields().len();
    (0..n, any::<u64>(), 0u8..4, -10i8..=0).prop_map(move |(fi, pick, edge, qexp)| {
        let f = all_fields()[fi];
        let x = match edge {
            0 => f.min,
            1 => f.max,
            2 if f.min <= 0 && 0 <= f.max => 0,
            _ => f.min + (pick % ((f.max - f.min) as u64 + 1)) as i64,
        };
        (fi, x, qexp)
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn sdk_093_format_parse_roundtrip((fi, x, qexp) in arb_case()) {
        let f = all_fields()[fi];
        let exp = Some(AssetExp { px_exp: -2, qty_exp: qexp });
        let text = format_value(f, x, exp);
        prop_assert_eq!(parse_value(f, &text, exp), Ok(x), "field {} text {:?}", f.name, text);
    }
}
