//! TYP-049 observed Binance/Bitget subset; six-product capture completeness remains pending.
use hft_types::{FxErr, RoundMode, parse_exact, parse_round};

#[test]
fn typ_049_venue_decimal_samples() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/decimal");
    for product in [
        "binance-spot",
        "binance-usdm",
        "bitget-spot",
        "bitget-usdt-futures",
    ] {
        let text = std::fs::read_to_string(root.join(format!("{product}.tsv"))).unwrap();
        let mut rows = 0;
        for (index, line) in text.lines().enumerate() {
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            let columns: Vec<_> = line.split('\t').collect();
            assert_eq!(columns.len(), 4, "{product}:{}", index + 1);
            let exponent: i8 = columns[1].parse().unwrap();
            let expected = match columns[2] {
                "PRECISION" => Err(FxErr::PRECISION),
                "OVERFLOW" => Err(FxErr::OVERFLOW),
                raw => Ok(raw.parse::<i64>().unwrap()),
            };
            assert_eq!(
                parse_exact(columns[0].as_bytes(), exponent),
                expected,
                "{product}:{} exact",
                index + 1
            );
            assert_eq!(
                parse_round(columns[0].as_bytes(), exponent, RoundMode::HalfEven),
                Ok(columns[3].parse().unwrap()),
                "{product}:{} round",
                index + 1
            );
            rows += 1;
        }
        assert!(rows > 0, "empty observed subset: {product}");
    }
}
