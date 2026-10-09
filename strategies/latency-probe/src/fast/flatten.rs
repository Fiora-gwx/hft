//! Residual flattening and the residual cap (README "quote 探测" step 6, "意图" 残余平仓).

use hft_strategy_sdk::prelude::*;
use hft_strategy_sdk::{Ctx, IntentClass, LegLinkOpt, OrderIntent, Views};

use super::{Cx, K_RESID, L_EXTERNAL, Out, Phase, ProbeGroup, cross_px, notional_ceil, opposite};
use crate::desc;
use crate::params::Mode;

/// Mid of leg `k` (floor); 0 without a quote.
pub fn mid<V: Views>(cx: &Cx<'_, V>, k: usize) -> Px {
    let b = cx.v.bbo(cx.inst(k));
    if b.bid_px.0 <= 0 || b.ask_px.0 <= 0 {
        return Px(0);
    }
    Px::mid_floor(b.bid_px, b.ask_px)
}

/// Residual notional (USDT 1e-6, ceiling): quote = Σ |net_i| × mid_i; pair/target =
/// |Σ (net_i + immediate in-flight_i)| × mid_A (same asset, legs offset).
pub fn residual_notional<V: Views>(cx: &Cx<'_, V>) -> i64 {
    let lv = cx.v.legs(cx.acct, cx.gid());
    let n = cx.cfg.n();
    match cx.mode {
        Mode::Quote => (0..n)
            .map(|k| {
                let net = lv.leg.get(k).map_or(Qty(0), |l| l.net);
                notional_ceil(net, mid(cx, k), cx.v.inst(cx.inst(k)))
            })
            .fold(0i64, i64::saturating_add),
        _ => {
            let s: i64 = lv.leg.iter().take(n).fold(0i64, |a, l| {
                a.saturating_add(l.net.0).saturating_add(l.inflight_immediate.0)
            });
            notional_ceil(Qty(s), mid(cx, 0), cx.v.inst(cx.inst(0)))
        }
    }
}

/// Σ |net_i| × mid_i and the leg with the largest of them (`FlattenIncomplete`).
pub fn gross<V: Views>(cx: &Cx<'_, V>) -> (i64, usize) {
    let mut sum = 0i64;
    let (mut best, mut ix) = (-1i64, 0usize);
    for k in 0..cx.cfg.n() {
        let x = notional_ceil(cx.net(k), mid(cx, k), cx.v.inst(cx.inst(k)));
        sum = sum.saturating_add(x);
        if x > best {
            best = x;
            ix = k;
        }
    }
    (sum, ix)
}

/// Residual flatten of leg `k`: opposite IOC of |net| rounded toward zero to the lot, priced like
/// the pair opening (`pair_cross`), `REDUCE_ONLY` on perps; at most one per leg per event.
pub fn leg<V: Views>(_g: &mut ProbeGroup, cx: &Cx<'_, V>, ctx: &mut Ctx<'_>, out: &mut Out, k: usize) {
    if out.flattened & (1 << k) != 0 {
        return;
    }
    let net = cx.net(k);
    let inst = cx.inst(k);
    let h = cx.v.inst(inst);
    let lot = h.lot.max(1);
    let qty = net.0.unsigned_abs() / lot.unsigned_abs() * lot.unsigned_abs();
    if qty == 0 {
        return;
    }
    let side = if net.0 > 0 { Side::Sell } else { Side::Buy };
    let px = cross_px(side, &cx.v.bbo(inst), cx.f.pair_cross, h.tick);
    let it = OrderIntent {
        venue: cx.venue(k),
        account: cx.acct,
        side,
        tif: Tif::Ioc,
        class: IntentClass::Flatten,
        flags: if cx.spot(k) { 0 } else { OrderIntent::REDUCE_ONLY },
        _pad: [0; 2],
        inst,
        group: cx.gid(),
        px,
        qty: Qty(qty.min(i64::MAX as u64) as i64),
        link: LegLinkOpt::NONE,
        tag: (K_RESID << 8) | k as u32,
        max_rest_ms: 0,
    };
    if ctx.place(it).is_ok() {
        out.flattened |= 1 << k;
    }
    let _ = opposite(side);
}

/// Residual flatten of every non-zero leg.
pub fn all_legs<V: Views>(g: &mut ProbeGroup, cx: &Cx<'_, V>, ctx: &mut Ctx<'_>, out: &mut Out) {
    for k in 0..cx.cfg.n() {
        leg(g, cx, ctx, out, k);
    }
}

/// Residual cap (step 6). Above `max_inv_notional`: flatten every non-zero leg; when not yet halted
/// also `request_halt(g, 2)`, `ProbeHalted{2, notional}` and the halted mirror; pair/target groups go
/// `Blocked`. Returns whether the cap was exceeded (the caller then skips its phase action).
pub fn cap_check<V: Views>(g: &mut ProbeGroup, cx: &Cx<'_, V>, ctx: &mut Ctx<'_>, out: &mut Out) -> bool {
    let n = residual_notional(cx);
    if n <= cx.f.max_inv_notional.0 {
        return false;
    }
    if cx.mode != Mode::Quote {
        g.phase = Phase::Blocked;
    }
    all_legs(g, cx, ctx, out);
    if !g.thr.halted() {
        let _ = ctx.request_halt(cx.gid(), 2);
        cx.note(ctx, desc::PROBE_HALTED, [2, n as u64, 0, 0, 0, 0, 0, 0]);
        g.thr.set_halted(true);
    }
    true
}

/// `External`: flatten every non-zero leg; quote legs pause until flat, pair/target groups block.
pub fn on_external<V: Views>(g: &mut ProbeGroup, cx: &Cx<'_, V>, ctx: &mut Ctx<'_>, out: &mut Out) {
    let _ = cap_check(g, cx, ctx, out);
    all_legs(g, cx, ctx, out);
    match cx.mode {
        Mode::Quote => {
            if !cx.flat() {
                for l in g.legs.iter_mut().take(cx.cfg.n()) {
                    l.flags |= L_EXTERNAL;
                }
            }
        }
        _ => g.phase = Phase::Blocked,
    }
}
