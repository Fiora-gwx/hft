//! refdata built from the configuration (README "refdata 指数").

use hft_refdata::{
    AccountSpec, AccountVenueSpec, AssetClass, AssetSpec, Exchange, FeePair, FeeSpec, InstKind, InstSpec,
    InstStatus, RateLimit, RdbBuilder, RefData, StaleGuardKind, TsUnit, VenueSpec,
};
use hft_types::{AccountId, AssetExp, AssetId, InstId, Notional, Px, Qty, Rate, VenueId};
use hft_venue_proto::VenueProtocol;
use venue_binance::{Spot, Usdm};

use crate::config::{CfgErr, Config, Dec, Product};

/// The probe's account (Live, non-production).
pub const ACCOUNT: AccountId = AccountId(1);
/// Quote asset of every inst.
pub const QUOTE: &str = "USDT";

/// One inst of the generated refdata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RdInst {
    pub id: InstId,
    pub symbol: String,
    /// Probed (`[[insts]]`) or ballast only.
    pub probed: bool,
}

/// The refdata blob, the loaded refdata and the inst list.
#[derive(Debug)]
pub struct Built {
    pub blob: Vec<u8>,
    pub rd: &'static RefData,
    pub insts: Vec<RdInst>,
}

pub fn venue(p: Product) -> VenueId {
    match p {
        Product::Usdm => VenueId::BINANCE_USDM,
        Product::Spot => VenueId::BINANCE_SPOT,
    }
}

/// `(px_exp, qty_exp)` of a base asset from its tick and lot.
pub fn exps(tick: &Dec, lot: &Dec) -> (i8, i8) {
    let px = -(tick.decimals as i8);
    let qty = (-8i8).min(-(lot.decimals as i8));
    (px, qty)
}

fn bad(m: String) -> CfgErr {
    CfgErr::Invalid(m)
}

/// Builds the refdata of `cfg` (dense inst ids: probed insts first, then ballast symbols).
pub fn build(cfg: &Config) -> Result<Built, CfgErr> {
    let v = venue(cfg.product);
    let mut b = RdbBuilder::new(1);
    b.created_utc_ns(1_759_276_800_000_000_000);
    let mut assets: Vec<(String, AssetId, i8, i8)> = Vec::new();
    let quote = AssetId(1);
    b.asset(AssetSpec {
        id: quote,
        name: QUOTE.into(),
        class: AssetClass::Stable,
        exp: AssetExp {
            px_exp: -6,
            qty_exp: -8,
        },
        px_max: Px(1_000_000_000_000_000),
        qty_max: Qty(1_000_000_000_000_000_000),
        dust: Qty(1),
    });
    assets.push((QUOTE.to_string(), quote, -6, -8));
    let ballast_tick = Dec::parse("0.0001").ok_or_else(|| bad("tick".into()))?;
    let ballast_lot = Dec::parse("0.001").ok_or_else(|| bad("lot".into()))?;
    // (symbol, base, tick, lot, min_qty, min_notional, probed)
    let mut rows: Vec<(String, String, Dec, Dec, Dec, Dec, bool)> = cfg
        .insts
        .iter()
        .map(|i| {
            (
                i.symbol.clone(),
                i.base.clone(),
                i.tick.clone(),
                i.lot.clone(),
                i.min_qty.clone(),
                i.min_notional.clone(),
                true,
            )
        })
        .collect();
    for s in &cfg.ballast_symbols {
        if rows.iter().any(|r| r.0 == *s) {
            continue;
        }
        let base = s.strip_suffix(QUOTE).unwrap_or(s).to_string();
        rows.push((
            s.clone(),
            base,
            ballast_tick.clone(),
            ballast_lot.clone(),
            ballast_lot.clone(),
            Dec::parse("5").ok_or_else(|| bad("5".into()))?,
            false,
        ));
    }
    for r in &rows {
        if assets.iter().any(|a| a.0 == r.1) {
            continue;
        }
        let (px, qty) = exps(&r.2, &r.3);
        let id = AssetId(u16::try_from(assets.len() + 1).map_err(|_| bad("too many assets".into()))?);
        b.asset(AssetSpec {
            id,
            name: r.1.clone().into(),
            class: AssetClass::Crypto,
            exp: AssetExp {
                px_exp: px,
                qty_exp: qty,
            },
            px_max: Px(1_000_000_000_000_000_000),
            qty_max: Qty(1_000_000_000_000_000_000),
            dust: Qty(1),
        });
        assets.push((r.1.clone(), id, px, qty));
    }
    // The spot fee rule names BNB (`ReceivedAssetUnlessDiscount{BNB}`): it must be an asset.
    if !assets.iter().any(|a| a.0 == "BNB") {
        let id = AssetId(u16::try_from(assets.len() + 1).map_err(|_| bad("too many assets".into()))?);
        b.asset(AssetSpec {
            id,
            name: "BNB".into(),
            class: AssetClass::Crypto,
            exp: AssetExp {
                px_exp: -3,
                qty_exp: -8,
            },
            px_max: Px(1_000_000_000_000_000_000),
            qty_max: Qty(1_000_000_000_000_000_000),
            dust: Qty(1),
        });
        assets.push(("BNB".to_string(), id, -3, -8));
    }
    let (product, ws_in) = match cfg.product {
        Product::Usdm => (InstKind::Perp, 10),
        Product::Spot => (InstKind::Spot, 5),
    };
    b.venue(VenueSpec {
        id: v,
        exchange: Exchange::Binance,
        product,
        ws_max_streams_per_conn: 1024,
        ws_max_in_msgs_per_s: ws_in,
        ws_max_conn_age_s: 86_400,
        ws_new_conn_per_5min_ip: 300,
        app_ping_interval_s: 0,
        ts_unit: TsUnit::Ms,
        client_id_max_len: 36,
        client_id_prefix: "".into(),
        max_batch: 5,
        max_inflight_amend: 1,
        crossed_tolerance_default: 0,
        stale_guard: StaleGuardKind::RecvWindow,
    });
    b.fee(FeeSpec {
        venue: v,
        tier: 0,
        fee: FeePair {
            maker: Rate(200_000),
            taker: Rate(500_000),
        },
    });
    let asset_of = |n: &str| assets.iter().find(|a| a.0 == n).map(|a| a.1);
    let (caps, rules) = match cfg.product {
        Product::Usdm => (Usdm::default_caps(asset_of), Usdm::rate_rules()),
        Product::Spot => (Spot::default_caps(asset_of), Spot::rate_rules()),
    };
    b.caps(caps);
    for r in rules {
        b.limit(
            v,
            RateLimit {
                scope: r.scope,
                bucket: r.bucket,
                limit: r.limit,
                window_ms: r.window_ms,
                default_cost: 1,
            },
        );
    }
    let mut insts = Vec::with_capacity(rows.len());
    for (k, r) in rows.iter().enumerate() {
        let id = InstId(u32::try_from(k + 1).map_err(|_| bad("too many insts".into()))?);
        let a = assets
            .iter()
            .find(|a| a.0 == r.1)
            .ok_or_else(|| bad(format!("asset {}", r.1)))?;
        let (px, qty) = (a.2, a.3);
        let need = |d: &Dec, e: i8, what: &str| {
            d.raw(e)
                .filter(|x| *x > 0)
                .ok_or_else(|| bad(format!("{}: {what} {} not representable at 10^{e}", r.0, d.text)))
        };
        let tick = need(&r.2, px, "tick")?;
        let lot = need(&r.3, qty, "lot")?;
        let min_qty = need(&r.4, qty, "min_qty")?;
        let min_notional = need(&r.5, -6, "min_notional")?;
        b.inst(InstSpec {
            id,
            venue: v,
            symbol: r.0.clone(),
            status: InstStatus::Trading,
            kind: product,
            asset: a.1,
            quote,
            tick,
            lot,
            min_qty: Qty(min_qty),
            max_qty: Qty(1_000_000_000_000_000_000),
            min_notional: Notional(min_notional),
            book_window: 64,
            l2_depth: 20,
            crossed_tolerance: 0,
            tick_str: r.2.text.clone(),
            lot_str: r.3.text.clone(),
            min_qty_str: r.4.text.clone(),
            funding_interval_s: if product == InstKind::Perp { 28_800 } else { 0 },
            max_leverage: if product == InstKind::Perp { 20 } else { 0 },
            ..InstSpec::default()
        });
        insts.push(RdInst {
            id,
            symbol: r.0.clone(),
            probed: r.6,
        });
    }
    b.account(AccountSpec {
        id: ACCOUNT,
        label: "latency-slice".into(),
        venues: 1u64 << v.0,
        is_prod: false,
        ..AccountSpec::default()
    });
    b.account_venue(AccountVenueSpec {
        account: ACCOUNT,
        venue: v,
        uid: "latency-slice".into(),
    });
    let blob = b.finish();
    let rd = RefData::load(&blob, None).map_err(|e| bad(format!("refdata rejected: {e:?}")))?;
    Ok(Built {
        blob,
        rd: Box::leak(Box::new(rd)),
        insts,
    })
}
