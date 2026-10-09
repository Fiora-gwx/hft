//! target probes (README "target 探测").

use hft_strat_lib::Outcome;
use hft_strategy_sdk::prelude::*;
use hft_strategy_sdk::{
    Ctx, ExecSpec, FirstLeg, IntentOutcome, LegTarget, PairParams, PxMode, ResidualPolicy, StuckReason,
    TakerSliceParams, TargetIntent, TargetReason, TargetReject, Urgency, Views,
};

use super::{
    Cx, ExecKind, G_STUCK_RETRY, K_T_EXIT, K_T_OPEN, Out, Phase, ProbeGroup, notional_ceil, pair, pair_lot,
};
use crate::desc;

fn spec<V: Views>(g: &ProbeGroup, cx: &Cx<'_, V>) -> ExecSpec {
    let c = cx.f.pair_cross;
    ExecSpec::PairOnFill(PairParams {
        slice: TakerSliceParams {
            depth_frac: Rate(1_000_000_000),
            depth_levels: 1,
            _pad0: [0; 7],
            max_cross: c,
            max_chunk_notional: Notional(g.tgt_chunk),
            min_chunk_notional: Notional(0),
            cooldown_ms: 0,
            _pad1: 0,
        },
        first_leg: FirstLeg::Fixed,
        first_leg_ix: 0,
        hedge_px_mode: PxMode::Cross,
        _pad0: [0; 5],
        hedge_cross: c,
        hedge_max_slip: c,
        ratio_num: 1,
        ratio_den: 1,
        fix_tolerance: Rate(0),
        residual: ResidualPolicy::TrimExcess,
        _pad1: [0; 7],
    })
}

/// Target of the group: leg 0 = `q0`, leg 1 = `−q0`.
fn intent<V: Views>(g: &ProbeGroup, cx: &Cx<'_, V>, q0: i64, reason: TargetReason) -> TargetIntent {
    let q = g.tgt_q.unsigned_abs();
    let pl = pair_lot(cx.lot(0), cx.lot(1)).unwrap_or(1);
    // Half a pair lot relative to q, rounded up.
    let band = if q == 0 {
        0
    } else {
        let n = u128::from(pl.unsigned_abs()) * 1_000_000_000;
        let d = 2 * u128::from(q);
        (n.div_ceil(d)).min(i64::MAX as u128) as i64
    };
    TargetIntent {
        account: cx.acct,
        _pad0: [0; 3],
        group: cx.gid(),
        legs: [
            LegTarget::new(0, Qty(q0)),
            LegTarget::new(1, Qty(q0.saturating_neg())),
            LegTarget::ZERO,
            LegTarget::ZERO,
        ],
        n_legs: 2,
        reason,
        urgency: Urgency::Normal,
        _pad1: [0; 5],
        band: Rate(band),
        exec: spec(g, cx),
        tag: u32::from(if reason == TargetReason::Exit {
            g.retry_tag
        } else {
            0
        }),
        _pad2: 0,
    }
}

/// Opening target (`Due` + A `Bbo`): `q` for the side of leg A.
pub fn open<V: Views>(
    g: &mut ProbeGroup,
    cx: &Cx<'_, V>,
    ctx: &mut Ctx<'_>,
    out: &mut Out,
    side: Side,
    q: Qty,
) -> bool {
    let b = cx.v.bbo(cx.inst(0));
    let touch = if side == Side::Buy { b.ask_px } else { b.bid_px };
    g.tgt_q = if side == Side::Buy { q.0 } else { -q.0 };
    g.tgt_chunk = notional_ceil(q, touch, cx.v.inst(cx.inst(0))).saturating_mul(2);
    g.retry_tag = 0;
    let t = intent(g, cx, g.tgt_q, TargetReason::Rebalance);
    match ctx.target(t) {
        Ok(h) => {
            out.push(cx.gix, K_T_OPEN, 0, h);
            true
        }
        Err(_) => false,
    }
}

/// 0/0 exit target with the opening's exec and `tag` = the resend count.
pub fn exit<V: Views>(g: &mut ProbeGroup, cx: &Cx<'_, V>, ctx: &mut Ctx<'_>, out: &mut Out) {
    let t = intent(g, cx, 0, TargetReason::Exit);
    if let Ok(h) = ctx.target(t) {
        out.push(cx.gix, K_T_EXIT, 0, h);
    }
}

/// Result of a target set in the previous call.
pub fn on_outcome<V: Views>(
    g: &mut ProbeGroup,
    cx: &Cx<'_, V>,
    ctx: &mut Ctx<'_>,
    _out: &mut Out,
    kind: u8,
    o: IntentOutcome,
) {
    if let IntentOutcome::TargetRejected(r) = o {
        cx.note(
            ctx,
            desc::TARGET_REJECTED,
            [u64::from(r.code()), 0, 0, 0, 0, 0, 0, 0],
        );
        let _ = g.thr.on_outcome(Outcome::Gated, cx.ens(), &cx.throttle_params());
        if kind == K_T_OPEN && g.phase == Phase::Opening {
            if r == TargetReject::LotMismatch {
                g.phase = Phase::Blocked;
                g.flags |= super::G_STICKY;
            } else {
                g.phase = Phase::Idle;
            }
        }
    }
}

/// `Exec` of a target group.
pub fn on_exec<V: Views>(g: &mut ProbeGroup, cx: &Cx<'_, V>, ctx: &mut Ctx<'_>, out: &mut Out, k: ExecKind) {
    let tp = cx.throttle_params();
    match k {
        ExecKind::Done { cleared_by_engine } => match g.phase {
            Phase::Opening => {
                let _ = g.thr.on_outcome(Outcome::Filled, cx.ens(), &tp);
                g.phase = Phase::Holding;
                pair::register_flat(g, cx, ctx);
            }
            Phase::Flattening => {
                let _ = ctx.clear_target(cx.acct, cx.gid());
                g.flags &= !G_STUCK_RETRY;
                g.phase = Phase::Idle;
            }
            Phase::Blocked if cleared_by_engine && !cx.flat() => {
                g.retry_tag = 0;
                exit(g, cx, ctx, out);
                g.phase = Phase::Flattening;
                g.attempt = 1;
            }
            _ => {}
        },
        ExecKind::Stuck(r) => match g.phase {
            Phase::Opening => {
                cx.note(
                    ctx,
                    desc::TARGET_STUCK,
                    [u64::from(r.code()), 1, 0, 0, 0, 0, 0, 0],
                );
                let _ = g.thr.on_outcome(Outcome::NoFill, cx.ens(), &tp);
                g.retry_tag = 0;
                exit(g, cx, ctx, out);
                g.phase = Phase::Flattening;
                g.attempt = 1;
            }
            Phase::Flattening => {
                cx.note(
                    ctx,
                    desc::TARGET_STUCK,
                    [u64::from(r.code()), 2, 0, 0, 0, 0, 0, 0],
                );
                if matches!(r, StuckReason::Shadow | StuckReason::Gated(_)) {
                    return;
                }
                if g.attempt <= 1 {
                    g.flags |= G_STUCK_RETRY;
                    pair::register_flat(g, cx, ctx);
                } else {
                    let _ = ctx.clear_target(cx.acct, cx.gid());
                    g.flags &= !G_STUCK_RETRY;
                    pair::incomplete(g, cx, ctx);
                }
            }
            _ => {}
        },
        ExecKind::Broken(_) => pair::on_broken(g, cx, ctx, out),
        _ => {}
    }
}
