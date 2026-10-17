//! `price_hedge` (README "对冲定价") and `link_valid` (strategy link structure check).

use hft_strategy_sdk::{BboView, HedgeKind, HedgePlan, LegLink, LinkRole, OnFail, PxMode};
use hft_types::{HedgePxLimits, Px, RoundMode, Side, Tif, Tsc, cap_cross_px};

use crate::types::{HedgePx, HedgePxErr, HedgeRequest};

/// Hedge-leg limit price. `tsc_per_ms` converts `lim.conn_liveness_ms` to this run's TSC.
pub fn price_hedge(
    req: &HedgeRequest,
    plan: &HedgePlan,
    bbo: &BboView,
    now: Tsc,
    lim: &HedgePxLimits,
    tick: i64,
    tsc_per_ms: u64,
) -> Result<HedgePx, HedgePxErr> {
    let reverse = req.role == LinkRole::Reverse
        || (req.role == LinkRole::SecondRetry
            && plan.kind == HedgeKind::Simultaneous
            && plan.reduce_only != 0);
    let side = req.side;
    let silence = u64::from(lim.conn_liveness_ms).saturating_mul(tsc_per_ms);
    let live = bbo.quote_live(now, silence) && bbo.bid_px.0 > 0 && bbo.ask_px.0 > 0;
    if !live {
        if !reverse && plan.limit_px.0 != 0 {
            return Ok(HedgePx {
                px: plan.limit_px,
                capped: false,
                slipped: false,
                touch: Px(0),
                cap: Px(0),
            });
        }
        return Err(HedgePxErr::StaleQuote);
    }
    let mid = Px::mid_floor(bbo.bid_px, bbo.ask_px);
    let rr = lim.rr_max_cross;
    let cap = match side {
        Side::Buy => mid
            .mul_rate(rr, RoundMode::Floor)
            .round_to_tick(tick, RoundMode::Floor),
        Side::Sell => mid
            .mul_rate(hft_types::Rate(rr.0.saturating_neg()), RoundMode::Ceil)
            .round_to_tick(tick, RoundMode::Ceil),
    };
    let touch = match side {
        Side::Buy => bbo.ask_px,
        Side::Sell => bbo.bid_px,
    };
    let beyond = match side {
        Side::Buy => touch.0 > cap.0,
        Side::Sell => touch.0 < cap.0,
    };
    if beyond {
        return Err(HedgePxErr::BeyondCross);
    }
    let mode = if reverse { PxMode::Cross } else { plan.px_mode };
    let ref_px = if reverse { Px(0) } else { req.ref_px };
    let slip = ref_px.0 > 0 && {
        let dev = i128::from((touch.0 - ref_px.0).abs()) * 1_000_000_000;
        dev > i128::from(plan.max_slip.0) * i128::from(ref_px.0)
    };
    let clamp = |p: Px| match side {
        Side::Buy => Px(p.0.min(cap.0)),
        Side::Sell => Px(p.0.max(cap.0)),
    };
    let (px, slipped) = match mode {
        PxMode::Limit => (clamp(plan.limit_px), false),
        PxMode::Cross | PxMode::Mid if slip => (cap, true),
        PxMode::Cross => (cap_cross_px(side, touch, mid, plan.px_param, lim, tick), false),
        PxMode::Mid => (cap_cross_px(side, mid, mid, plan.px_param, lim, tick), false),
    };
    Ok(HedgePx {
        px,
        capped: px == cap,
        slipped,
        touch,
        cap,
    })
}

/// Structure check of a strategy's link (README "策略链接的结构校验").
pub fn link_valid(l: &LegLink) -> bool {
    let h = &l.hedge;
    if l.role != LinkRole::First || h.ratio_num == 0 || h.ratio_den == 0 {
        return false;
    }
    match h.kind {
        HedgeKind::OnFill => {
            matches!(h.on_fail, OnFail::Retry | OnFail::Reverse | OnFail::Hold) && h.tif == Tif::Ioc
        }
        HedgeKind::Simultaneous => {
            matches!(h.on_fail, OnFail::Reverse | OnFail::Hold) && matches!(h.tif, Tif::Ioc | Tif::Fok)
        }
        HedgeKind::None => false,
    }
}
