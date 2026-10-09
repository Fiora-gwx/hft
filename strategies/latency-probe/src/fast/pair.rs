//! pair probes and the pair/target phase machine (README "pair 探测").

use hft_strat_lib::{FireKind, Outcome};
use hft_strategy_sdk::prelude::*;
use hft_strategy_sdk::{
    Ctx, HedgeKind, HedgePlan, IntentClass, IntentOutcome, LegLink, LegLinkOpt, LinkRole, OnFail,
    OrderIntent, OrderUpdate, OrderUpdateKind, PxMode, ResidualPolicy, Views,
};

use super::{
    Cx, ExecKind, FLAT, G_FILLED, G_STUCK_RETRY, G_TICK, K_CLOSE, K_OPEN, L_INFLIGHT, L_REF, Out, PAIR_TICK,
    Phase, ProbeGroup, count_gated, count_reject, cross_px, flatten, leg_paused, min_qty, opposite, pair_lot,
    spread_ok, target,
};
use crate::desc;
use crate::params::{Mode, PAIR_BUY_A, PAIR_SELL_A};

/// FLAT at `now + pair_flat_ms`.
pub fn register_flat<V: Views>(g: &mut ProbeGroup, cx: &Cx<'_, V>, ctx: &mut Ctx<'_>) {
    g.flat_at_ens = cx.ens().saturating_add(i64::from(cx.f.pair_flat_ms) * 1_000_000);
    cx.timer(ctx, g.flat_at_ens, FLAT);
}

/// Side of leg A for the next opening (README "组"): a spot leg is bought; perp/perp alternates.
pub fn open_side_a<V: Views>(g: &ProbeGroup, cx: &Cx<'_, V>) -> Side {
    match cx.f.pair_side {
        PAIR_BUY_A => Side::Buy,
        PAIR_SELL_A => Side::Sell,
        _ if cx.spot(0) => Side::Buy,
        _ if cx.spot(1) => Side::Sell,
        _ if g.next_dir == 0 => Side::Buy,
        _ => Side::Sell,
    }
}

/// Pair quantity: both legs' minimum (each at its taking touch) on the pair lot.
pub fn pair_qty<V: Views>(cx: &Cx<'_, V>, side_a: Side) -> Option<Qty> {
    let (ha, hb) = (cx.v.inst(cx.inst(0)), cx.v.inst(cx.inst(1)));
    let step = pair_lot(ha.lot, hb.lot)?;
    let touch = |k: usize, s: Side| {
        let b = cx.v.bbo(cx.inst(k));
        if s == Side::Buy { b.ask_px } else { b.bid_px }
    };
    let qa = min_qty(ha, touch(0, side_a), cx.size_margin, step)?;
    let qb = min_qty(hb, touch(1, opposite(side_a)), cx.size_margin, step)?;
    Some(Qty(qa.0.max(qb.0)))
}

/// Link of a first leg (opening or closing) whose second leg is produced on B by hft-oms.
pub fn link<V: Views>(cx: &Cx<'_, V>, side_first: Side, closing: bool) -> LegLinkOpt {
    let c = cx.f.pair_cross;
    LegLinkOpt::some(LegLink {
        group: cx.gid(),
        role: LinkRole::First,
        _pad: [0; 3],
        tolerance: Rate(0),
        hedge: HedgePlan {
            kind: HedgeKind::OnFill,
            px_mode: PxMode::Cross,
            on_fail: OnFail::Retry,
            tif: Tif::Ioc,
            venue: cx.venue(1),
            account: cx.acct,
            side: opposite(side_first),
            reduce_only: u8::from(closing && !cx.spot(1)),
            inst: cx.inst(1),
            ratio_num: 1,
            ratio_den: 1,
            residual: ResidualPolicy::TrimExcess,
            _pad: [0; 3],
            px_param: c,
            max_slip: c,
            limit_px: Px(0),
        },
    })
}

fn first_leg<V: Views>(cx: &Cx<'_, V>, side: Side, qty: Qty, closing: bool) -> OrderIntent {
    let inst = cx.inst(0);
    let h = cx.v.inst(inst);
    OrderIntent {
        venue: cx.venue(0),
        account: cx.acct,
        side,
        tif: Tif::Ioc,
        class: if closing {
            IntentClass::Reduce
        } else {
            IntentClass::Open
        },
        flags: if closing && !cx.spot(0) {
            OrderIntent::REDUCE_ONLY
        } else {
            0
        },
        _pad: [0; 2],
        inst,
        group: cx.gid(),
        px: cross_px(side, &cx.v.bbo(inst), cx.f.pair_cross, h.tick),
        qty,
        link: link(cx, side, closing),
        tag: (if closing { K_CLOSE } else { K_OPEN }) << 8,
        max_rest_ms: 0,
    }
}

/// Close link by the actual A net quantity (rounded toward zero to A's lot).
pub fn close_link<V: Views>(g: &mut ProbeGroup, cx: &Cx<'_, V>, ctx: &mut Ctx<'_>, out: &mut Out) {
    let net = cx.net(0);
    let lot = cx.lot(0).max(1);
    let qty = net.0 / lot * lot;
    if qty == 0 {
        flatten::all_legs(g, cx, ctx, out);
        return;
    }
    let side = if qty > 0 { Side::Sell } else { Side::Buy };
    let _ = ctx.place(first_leg(cx, side, Qty(qty.abs()), true));
}

/// Step 2: the A leg's `Bbo` while `Due`.
pub fn on_a_bbo<V: Views>(g: &mut ProbeGroup, cx: &Cx<'_, V>, ctx: &mut Ctx<'_>, out: &mut Out) {
    if g.phase != Phase::Due || !cx.f.enabled || !cx.live() {
        return;
    }
    if cx.v.risk(cx.acct).margin_level != hft_strategy_sdk::MarginLevel::None {
        return;
    }
    if leg_paused(g, cx, 0) || leg_paused(g, cx, 1) {
        return;
    }
    if !(cx.v.quote_ok(cx.acct, cx.inst(0)) && cx.v.quote_ok(cx.acct, cx.inst(1))) {
        g.skip_unhealthy = g.skip_unhealthy.saturating_add(1);
        return;
    }
    let max = cx.f.max_spread;
    if !(spread_ok(&cx.v.bbo(cx.inst(0)), max) && spread_ok(&cx.v.bbo(cx.inst(1)), max)) {
        g.skip_spread = g.skip_spread.saturating_add(1);
        return;
    }
    if !cx.flat() || g.thr.can_fire(FireKind::Open, cx.ens()).is_err() {
        return;
    }
    let side = open_side_a(g, cx);
    let Some(q) = pair_qty(cx, side) else { return };
    let period = match cx.mode {
        Mode::Target => cx.f.target_period_ms,
        _ => cx.f.pair_period_ms,
    };
    let ok = match cx.mode {
        Mode::Target => target::open(g, cx, ctx, out, side, q),
        _ => match ctx.place(first_leg(cx, side, q, false)) {
            Ok(h) => {
                out.push(cx.gix, K_OPEN as u8, 0, h);
                if let Some(l) = g.legs.first_mut() {
                    l.flags = (l.flags & !L_REF) | L_INFLIGHT;
                }
                true
            }
            Err(_) => false,
        },
    };
    if !ok {
        return;
    }
    if !cx.spot(0) && !cx.spot(1) {
        g.next_dir ^= 1;
    }
    g.thr.on_fire(FireKind::Open, cx.ens(), &cx.throttle_params());
    g.next_pair_ens = cx.ens().saturating_add(i64::from(period) * 1_000_000);
    g.flags &= !G_FILLED;
    g.phase = Phase::Opening;
}

/// Engine result of the first leg placed in the previous call: a gate reject or Shadow ends the
/// opening without counting `PairNoFill`.
pub fn on_open_outcome<V: Views>(g: &mut ProbeGroup, cx: &Cx<'_, V>, o: IntentOutcome) {
    match o {
        IntentOutcome::Placed(r) => {
            if let Some(l) = g.legs.first_mut() {
                l.rest_slot = r.slot;
                l.rest_gen = r.r#gen;
                l.flags |= L_REF;
            }
        }
        IntentOutcome::Rejected(_) | IntentOutcome::Shadowed => {
            if let IntentOutcome::Rejected(bits) = o {
                count_gated(g, bits);
            }
            let _ = g.thr.on_outcome(Outcome::Gated, cx.ens(), &cx.throttle_params());
            if let Some(l) = g.legs.first_mut() {
                l.flags &= !(L_INFLIGHT | L_REF);
            }
            if g.phase == Phase::Opening {
                g.phase = Phase::Idle;
            }
        }
        _ => {}
    }
}

/// Order updates of a pair (or target) group: the first leg's and the closing first leg's.
pub fn on_order<V: Views>(
    g: &mut ProbeGroup,
    cx: &Cx<'_, V>,
    ctx: &mut Ctx<'_>,
    out: &mut Out,
    u: &OrderUpdate,
) {
    let kind = u.tag >> 8;
    let tp = cx.throttle_params();
    let ens = cx.ens();
    let fill = matches!(u.kind, OrderUpdateKind::Filled { .. });
    if fill && flatten::cap_check(g, cx, ctx, out) {
        return;
    }
    let terminal = match u.kind {
        OrderUpdateKind::Filled { done, .. } => done,
        OrderUpdateKind::Cancelled | OrderUpdateKind::Aged | OrderUpdateKind::Rejected { .. } => true,
        _ => false,
    };
    if fill && kind == K_OPEN {
        g.flags |= G_FILLED;
    }
    if !terminal {
        return;
    }
    match (kind, g.phase) {
        (K_OPEN, Phase::Opening) => {
            if let Some(l) = g.legs.first_mut() {
                l.flags &= !(L_INFLIGHT | L_REF);
            }
            if g.flags & G_FILLED != 0 {
                let _ = g.thr.on_outcome(Outcome::Filled, ens, &tp);
                g.phase = Phase::Holding;
                register_flat(g, cx, ctx);
                return;
            }
            match u.kind {
                OrderUpdateKind::Rejected { bits, venue_code } if bits.0 == 0 => {
                    count_reject(g, cx, ctx, bits, venue_code);
                }
                OrderUpdateKind::Rejected { bits, .. } => {
                    count_gated(g, bits);
                    let _ = g.thr.on_outcome(Outcome::Gated, ens, &tp);
                }
                _ => {
                    g.nofill = g.nofill.saturating_add(1);
                    let _ = g.thr.on_outcome(Outcome::NoFill, ens, &tp);
                }
            }
            g.phase = Phase::Idle;
        }
        (K_CLOSE, Phase::Flattening) => register_flat(g, cx, ctx),
        _ => {}
    }
}

/// `Exec` of a pair group (only Broken and its release).
pub fn on_exec<V: Views>(g: &mut ProbeGroup, cx: &Cx<'_, V>, ctx: &mut Ctx<'_>, out: &mut Out, k: ExecKind) {
    match k {
        ExecKind::Broken(_) => on_broken(g, cx, ctx, out),
        ExecKind::Done {
            cleared_by_engine: true,
        } if g.phase == Phase::Blocked && !cx.flat() => {
            close_link(g, cx, ctx, out);
            g.phase = Phase::Flattening;
            g.attempt = 1;
        }
        _ => {}
    }
}

/// `Exec{Broken}` / `Risk{HedgeBroken}`: no opening, no new target, no flattening.
pub fn on_broken<V: Views>(g: &mut ProbeGroup, cx: &Cx<'_, V>, _ctx: &mut Ctx<'_>, _out: &mut Out) {
    if cx.mode == Mode::Quote {
        return;
    }
    if g.phase == Phase::Opening {
        let _ = g.thr.on_outcome(Outcome::Gated, cx.ens(), &cx.throttle_params());
    }
    g.flags &= !G_STUCK_RETRY;
    g.phase = Phase::Blocked;
}

/// `PAIR_TICK` and `FLAT` (phase transition table).
pub fn on_timer<V: Views>(g: &mut ProbeGroup, cx: &Cx<'_, V>, ctx: &mut Ctx<'_>, out: &mut Out, tag: u32) {
    if cx.mode == Mode::Quote {
        return;
    }
    match tag {
        PAIR_TICK => {
            g.flags &= !G_TICK;
            if g.phase != Phase::Idle {
                return;
            }
            if cx.mode == Mode::Target && cx.v.target(cx.acct, cx.gid()).is_some() {
                g.next_pair_ens = cx
                    .ens()
                    .saturating_add(i64::from(cx.f.target_period_ms) * 1_000_000);
                return;
            }
            g.phase = Phase::Due;
        }
        FLAT => on_flat(g, cx, ctx, out),
        _ => {}
    }
}

fn on_flat<V: Views>(g: &mut ProbeGroup, cx: &Cx<'_, V>, ctx: &mut Ctx<'_>, out: &mut Out) {
    if !matches!(g.phase, Phase::Holding | Phase::Flattening) {
        return;
    }
    if flatten::cap_check(g, cx, ctx, out) {
        return;
    }
    let target = cx.mode == Mode::Target;
    if cx.flat() {
        if target && g.phase == Phase::Flattening {
            let _ = ctx.clear_target(cx.acct, cx.gid());
        }
        g.flags &= !G_STUCK_RETRY;
        g.phase = Phase::Idle;
        return;
    }
    let a = cx.net(0).0.unsigned_abs() >= cx.lot(0).max(1).unsigned_abs();
    let b = cx.net(1).0.unsigned_abs() >= cx.lot(1).max(1).unsigned_abs();
    match (g.phase, target) {
        (Phase::Holding, true) => {
            target::exit(g, cx, ctx, out);
            g.phase = Phase::Flattening;
            g.attempt = 1;
        }
        (Phase::Holding, false) => {
            if a && b {
                close_link(g, cx, ctx, out);
                g.phase = Phase::Flattening;
                g.attempt = 1;
            } else {
                // Second leg missing (cell possibly Broken): wait for the engine.
                register_flat(g, cx, ctx);
            }
        }
        (_, true) => {
            if g.flags & G_STUCK_RETRY != 0 {
                g.flags &= !G_STUCK_RETRY;
                g.retry_tag = g.retry_tag.wrapping_add(1);
                target::exit(g, cx, ctx, out);
                g.attempt = 2;
            }
        }
        (_, false) => {
            if g.attempt <= 1 {
                close_link(g, cx, ctx, out);
                g.attempt = 2;
            } else {
                incomplete(g, cx, ctx);
            }
        }
    }
}

/// `FlattenIncomplete` and `Blocked`.
pub fn incomplete<V: Views>(g: &mut ProbeGroup, cx: &Cx<'_, V>, ctx: &mut Ctx<'_>) {
    let (n, leg) = flatten::gross(cx);
    cx.note(
        ctx,
        desc::FLATTEN_INCOMPLETE,
        [n as u64, leg as u64, 0, 0, 0, 0, 0, 0],
    );
    g.phase = Phase::Blocked;
}

/// First event after a restore (README "状态"): target groups clear their target; `Blocked` stays;
/// other phases continue by the actual net quantities (flat → Idle, else Holding with FLAT).
pub fn after_restore<V: Views>(g: &mut ProbeGroup, cx: &Cx<'_, V>, ctx: &mut Ctx<'_>) {
    if cx.mode == Mode::Target && g.phase != Phase::Idle {
        let _ = ctx.clear_target(cx.acct, cx.gid());
    }
    if g.phase == Phase::Opening {
        let _ = g.thr.on_outcome(Outcome::Gated, cx.ens(), &cx.throttle_params());
    }
    g.flags &= !G_STUCK_RETRY;
    match g.phase {
        Phase::Blocked | Phase::Idle => {}
        _ if cx.flat() => g.phase = Phase::Idle,
        _ => {
            g.phase = Phase::Holding;
            register_flat(g, cx, ctx);
        }
    }
}
