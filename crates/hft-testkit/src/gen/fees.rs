//! `fills_with_fee_asset` and `funding_flows` (README "2. proptest 生成器").

use proptest::prelude::*;

/// Fee asset rule of a fill (`VenueCaps.fee_asset_rule` shapes) plus the late-fee variant.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum FeeRule {
    /// Fee charged in the base asset.
    Base,
    /// Fee charged in the quote asset.
    Quote,
    /// A fill without fee, followed by a fee-only report (quote fee).
    LateQuote,
}

/// Which asset a fee is in.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum FeeAsset {
    Base,
    Quote,
}

/// One generated report: a fill (`qty > 0`) or a fee-only report (`qty = 0`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct FeeFill {
    pub qty: i64,
    pub px: i64,
    pub fee: i64,
    pub fee_asset: FeeAsset,
}

/// The reports of one fill of `gross` (base raw) at `px` (raw) with fee rate `ppm` (10^-6),
/// notional = `gross × px / notional_div` (raw quote).
pub fn fill_with_fee(rule: FeeRule, gross: i64, px: i64, ppm: i64, notional_div: i64) -> Vec<FeeFill> {
    let quote_fee = (i128::from(gross) * i128::from(px) / i128::from(notional_div.max(1)) * i128::from(ppm)
        / 1_000_000) as i64;
    match rule {
        FeeRule::Base => vec![FeeFill {
            qty: gross,
            px,
            fee: (i128::from(gross) * i128::from(ppm) / 1_000_000) as i64,
            fee_asset: FeeAsset::Base,
        }],
        FeeRule::Quote => vec![FeeFill {
            qty: gross,
            px,
            fee: quote_fee,
            fee_asset: FeeAsset::Quote,
        }],
        FeeRule::LateQuote => vec![
            FeeFill {
                qty: gross,
                px,
                fee: 0,
                fee_asset: FeeAsset::Quote,
            },
            FeeFill {
                qty: 0,
                px,
                fee: quote_fee,
                fee_asset: FeeAsset::Quote,
            },
        ],
    }
}

/// Fills under `rule`: gross 1–1000 lots of `lot`, px around `px0`, fee rate `ppm`.
pub fn fills_with_fee_asset(
    rule: FeeRule,
    lot: i64,
    px0: i64,
    ppm: i64,
) -> impl Strategy<Value = Vec<FeeFill>> {
    (1i64..=1_000, -1_000i64..=1_000)
        .prop_map(move |(n, d)| fill_with_fee(rule, n * lot, px0 + d, ppm, 1_000_000))
}

/// Pairing shape of one funding item.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum FundingShape {
    Paired,
    AmountDiff,
    ExchangeOnly,
    LocalOnly,
    /// Settled within `funding_recon_lag_ms` of the report (not compared yet).
    WithinLag,
}

/// One exchange funding item and its local booking.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct FundingPair {
    pub shape: FundingShape,
    pub settle_ms: i64,
    pub exchange: Option<i64>,
    pub local: Option<i64>,
}

/// 1–50 funding items against a report at `report_ms`, with `lag_ms` = `funding_recon_lag_ms`.
pub fn funding_flows(report_ms: i64, lag_ms: i64) -> impl Strategy<Value = Vec<FundingPair>> {
    proptest::collection::vec(
        (0u8..5, -1_000_000i64..=1_000_000, 1i64..=1_000, 0i64..28_800_000),
        1..=50,
    )
    .prop_map(move |v| {
        v.into_iter()
            .map(|(s, amt, d, back)| {
                let shape = [
                    FundingShape::Paired,
                    FundingShape::AmountDiff,
                    FundingShape::ExchangeOnly,
                    FundingShape::LocalOnly,
                    FundingShape::WithinLag,
                ][usize::from(s)];
                let settle_ms = match shape {
                    FundingShape::WithinLag => report_ms - (back % lag_ms.max(1)),
                    _ => report_ms - lag_ms - 1 - back,
                };
                let (exchange, local) = match shape {
                    FundingShape::Paired | FundingShape::WithinLag => (Some(amt), Some(amt)),
                    FundingShape::AmountDiff => (Some(amt), Some(amt + d)),
                    FundingShape::ExchangeOnly => (Some(amt), None),
                    FundingShape::LocalOnly => (None, Some(amt)),
                };
                FundingPair {
                    shape,
                    settle_ms,
                    exchange,
                    local,
                }
            })
            .collect()
    })
}
