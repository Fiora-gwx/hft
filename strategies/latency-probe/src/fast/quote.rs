//! quote probes (README "quote 探测").

use hft_strat_lib::{FireKind, Outcome};
use hft_strategy_sdk::prelude::*;
use hft_strategy_sdk::{
    BboView, Ctx, IntentOutcome, LegLinkOpt, OrderIntent, OrderUpdate, OrderUpdateKind, Views,
};

use super::{
    Cx, K_PROBE, L_ACKED, L_CANCEL_SENT, L_INFLIGHT, L_REF, L_UNKNOWN, Out, ProbeGroup, count_gated,
    count_reject, flatten, leg_paused, min_qty, spread_ok,
};
use crate::desc;
use crate::params::{SIDE_ALTERNATE, SIDE_SELL};

/// Leg 0 of a cross-gw group has an acknowledged resting probe that is not being cancelled.
fn anchor_resting(g: &ProbeGroup) -> bool {
    g.legs.first().is_some_and(|l| {
        l.flags & (L_INFLIGHT | L_ACKED) == (L_INFLIGHT | L_ACKED) && l.flags & L_CANCEL_SENT == 0
    })
}

/// Probe price (README "意图"): buy `floor_to_tick(ask × (1 − far))`, sell
/// `ceil_to_tick(bid × (1 + far))`.
pub fn probe_px(side: Side, bid: Px, ask: Px, far: Rate, tick: i64) -> Px {
    let (b, s) = (Side::Buy.price_round(), Side::Sell.price_round());
    match side {
        Side::Buy => ask.mul_rate(Rate(-far.0), b).round_to_tick(tick, b),
        Side::Sell => bid.mul_rate(far, s).round_to_tick(tick, s),
    }
}

/// `Bbo` of quote leg `k` (step 1–2).
pub fn on_bbo<V: Views>(
    g: &mut ProbeGroup,
    cx: &Cx<'_, V>,
    ctx: &mut Ctx<'_>,
    out: &mut Out,
    k: usize,
    b: &BboView,
) {
    let f = cx.f;
    let ens = cx.ens();
    let Some(last) = g.legs.get(k).map(|l| l.last_probe_ens) else {
        return;
    };
    if !f.enabled || !cx.live() {
        return;
    }
    let period = i64::from(f.probe_period_ms) * 1_000_000;
    let burst = i64::from(f.burst_period_ms) * 1_000_000;
    let prev_rx = cx.last_rx;
    let due = ens >= last.saturating_add(period)
        || (burst != 0 && b.rx_tsc.0 != 0 && b.rx_tsc == prev_rx && ens >= last.saturating_add(burst));
    if !due {
        return;
    }
    // A probe whose ref never arrived (placed just before a restart and gated) is orphaned once
    // twice its rest time has passed: an accepted one would have been reported (Aged at the latest).
    if let Some(l) = g.legs.get_mut(k) {
        let orphan = i64::from(f.quote_rest_ms) * 2_000_000;
        if l.flags & (L_INFLIGHT | L_REF) == L_INFLIGHT && ens >= l.last_probe_ens.saturating_add(orphan) {
            l.flags &= !(L_INFLIGHT | L_ACKED | L_CANCEL_SENT | L_UNKNOWN);
        }
    }
    let busy = g.legs.get(k).is_none_or(|l| l.flags & L_INFLIGHT != 0);
    if busy || leg_paused(g, cx, k) {
        return;
    }
    if f.quote_xgw && k == 1 && !anchor_resting(g) {
        return;
    }
    if cx.v.risk(cx.acct).margin_level != hft_strategy_sdk::MarginLevel::None {
        return;
    }
    let inst = cx.inst(k);
    if !cx.v.quote_ok(cx.acct, inst) {
        g.skip_unhealthy = g.skip_unhealthy.saturating_add(1);
        return;
    }
    let q = cx.v.bbo(inst);
    if !spread_ok(&q, f.max_spread) {
        g.skip_spread = g.skip_spread.saturating_add(1);
        return;
    }
    if g.thr.can_fire(FireKind::Open, ens).is_err() {
        return;
    }
    if flatten::residual_notional(cx) > f.max_inv_notional.0 {
        return;
    }
    let spot = cx.spot(k);
    let sell = !spot
        && (f.quote_side == SIDE_SELL
            || (f.quote_side == SIDE_ALTERNATE && g.legs.get(k).is_some_and(|l| l.next_sell)));
    let h = cx.v.inst(inst);
    let side = if sell { Side::Sell } else { Side::Buy };
    let px = probe_px(side, q.bid_px, q.ask_px, f.far_bp, h.tick);
    let Some(l) = g.legs.get_mut(k) else { return };
    // The decision time counts even when the intent is rejected later.
    l.last_probe_ens = ens;
    if f.quote_side == SIDE_ALTERNATE && !spot {
        l.next_sell = !l.next_sell;
    }
    let Some(qty) = min_qty(h, px, cx.size_margin, h.lot) else {
        return;
    };
    if px.0 <= 0 {
        return;
    }
    let it = OrderIntent {
        venue: cx.venue(k),
        account: cx.acct,
        side,
        tif: Tif::PostOnly,
        class: hft_strategy_sdk::IntentClass::Open,
        flags: 0,
        _pad: [0; 2],
        inst,
        group: cx.gid(),
        px,
        qty,
        link: LegLinkOpt::NONE,
        tag: (K_PROBE << 8) | k as u32,
        max_rest_ms: f.quote_rest_ms,
    };
    if let Ok(hd) = ctx.place(it) {
        l.flags = (l.flags & !(L_REF | L_ACKED | L_CANCEL_SENT)) | L_INFLIGHT;
        out.push(cx.gix, K_PROBE as u8, k as u8, hd);
    }
}

/// Engine result of a probe placed in the previous call.
pub fn on_outcome<V: Views>(g: &mut ProbeGroup, cx: &Cx<'_, V>, leg: u8, o: IntentOutcome) {
    let Some(l) = g.legs.get_mut(usize::from(leg)) else {
        return;
    };
    match o {
        IntentOutcome::Placed(r) => {
            if l.flags & L_INFLIGHT != 0 && l.flags & L_REF == 0 {
                l.rest_slot = r.slot;
                l.rest_gen = r.r#gen;
                l.flags |= L_REF;
            }
        }
        IntentOutcome::Rejected(bits) => {
            l.flags &= !(L_INFLIGHT | L_REF | L_ACKED | L_CANCEL_SENT);
            count_gated(g, bits);
            let _ = g.thr.on_outcome(Outcome::Gated, cx.ens(), &cx.throttle_params());
        }
        IntentOutcome::Shadowed => {
            l.flags &= !(L_INFLIGHT | L_REF | L_ACKED | L_CANCEL_SENT);
            let _ = g.thr.on_outcome(Outcome::Gated, cx.ens(), &cx.throttle_params());
        }
        _ => {}
    }
}

/// Order update of a quote group.
pub fn on_order<V: Views>(
    g: &mut ProbeGroup,
    cx: &Cx<'_, V>,
    ctx: &mut Ctx<'_>,
    out: &mut Out,
    u: &OrderUpdate,
) {
    let kind = u.tag >> 8;
    let k = (u.tag & 0xFF) as usize;
    if kind != K_PROBE || k >= cx.cfg.n() {
        return;
    }
    let tp = cx.throttle_params();
    let ens = cx.ens();
    let xgw = cx.f.quote_xgw;
    let Some(l) = g.legs.get_mut(k) else { return };
    if l.flags & L_INFLIGHT != 0 && l.flags & L_REF == 0 {
        l.rest_slot = u.r.slot;
        l.rest_gen = u.r.r#gen;
        l.flags |= L_REF;
    }
    let clear =
        |l: &mut super::LegState| l.flags &= !(L_INFLIGHT | L_REF | L_ACKED | L_CANCEL_SENT | L_UNKNOWN);
    match u.kind {
        OrderUpdateKind::Acked => {
            l.flags |= L_ACKED;
            if xgw && k == 0 {
                return;
            }
            if l.flags & L_CANCEL_SENT == 0 {
                let _ = ctx.cancel(u.r);
                l.flags |= L_CANCEL_SENT;
            }
            if xgw
                && k == 1
                && anchor_resting(g)
                && let Some(a) = g.legs.first_mut()
            {
                let r = cx.order_ref(0, a);
                let _ = ctx.cancel(r);
                a.flags |= L_CANCEL_SENT;
            }
        }
        OrderUpdateKind::Cancelled | OrderUpdateKind::Aged => {
            clear(l);
            let _ = g.thr.on_outcome(Outcome::NoFill, ens, &tp);
        }
        OrderUpdateKind::Rejected { bits, venue_code } => {
            clear(l);
            if bits.0 == 0 {
                count_reject(g, cx, ctx, bits, venue_code);
            } else {
                count_gated(g, bits);
                let _ = g.thr.on_outcome(Outcome::Gated, ens, &tp);
            }
        }
        OrderUpdateKind::Unknown => l.flags |= L_UNKNOWN,
        OrderUpdateKind::Filled { d, px, done, .. } => {
            if done {
                clear(l);
            }
            cx.note(
                ctx,
                desc::PROBE_FILLED,
                [k as u64, d.0.unsigned_abs(), px.0 as u64, 0, 0, 0, 0, 0],
            );
            let _ = g.thr.on_outcome(Outcome::Filled, ens, &tp);
            if !flatten::cap_check(g, cx, ctx, out) {
                flatten::leg(g, cx, ctx, out, k);
            }
        }
        _ => {}
    }
}
