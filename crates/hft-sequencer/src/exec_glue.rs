//! Executor glue (README "目标意图与执行层", "process" 6): target hand-over, `advance`, and the
//! fixed-order drain of `ExecOut` after every call that takes an `out`.

use hft_event::RecordFlags;
use hft_exec::{ExecCmdErr, ExecOut};
use hft_strategy_sdk::{IntentOutcome, StrategySet, TargetIntent};
use hft_types::{AccountId, GroupId, Rate, StrategyId, VenueId};

use crate::Driver;
use crate::engine::Engine;
use crate::notify::Notif;
use crate::strategies::Instances;
use crate::types::FatalReason;

type R<T> = Result<T, FatalReason>;

/// Drains an `ExecOut` (README "process" 6): decisions → cancel_resting → orders (pipeline +
/// `on_place_result`) → cancels → broken → timers → notify → notes.
pub(crate) fn drain_out<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    out: &mut ExecOut,
) -> R<()> {
    if out.is_empty() {
        return Ok(());
    }
    // `out` is checked out of the engine's pool. Nested calls use other pooled buffers, so
    // processing this one in place preserves the fixed drain order without heap copies.
    for d in out.decisions.iter() {
        let flags = if d.eflags & hft_exec::decision::eflags::SHADOW != 0 {
            RecordFlags::SHADOW
        } else {
            RecordFlags(0)
        };
        e.trace
            .rec("exec_decision", u64::from(d.action), u64::from(d.group.0));
        e.out.push(flags, d);
    }
    for c in out.cancel_resting.iter() {
        e.trace.rec(
            "oms.cancel_resting_on",
            u64::from(c.account.0),
            u64::from(c.inst.0),
        );
        let mut o = e.take_oms();
        e.oms
            .cancel_resting_on(c.account, c.inst, e.now.tsc, e.cur_seq, &mut o);
        crate::intents::drain_oms(e, inst, drv, &mut o)?;
        e.put_oms(o);
    }
    for o in out.orders.iter() {
        e.trace.rec("exec.order", u64::from(o.id.packed()), 0);
        let r = crate::intents::place_exec(e, inst, drv, o)?;
        e.trace.rec("exec.on_place_result", u64::from(o.id.packed()), 0);
        let mut xo = e.take_exec();
        e.exec.on_place_result(o.id, r, e.now, &mut xo);
        drain_out(e, inst, drv, &mut xo)?;
        e.put_exec(xo);
    }
    for c in out.cancels.iter() {
        e.trace.rec("oms.cancel", u64::from(c.slot), 1);
        let mut o = e.take_oms();
        let trig = e.trig;
        if e.oms.cancel(*c, &trig, e.now.tsc, e.cur_seq, &mut o).is_ok() {
            e.out.stamped.push(*c);
        }
        crate::intents::drain_oms(e, inst, drv, &mut o)?;
        e.put_oms(o);
    }
    for b in out.broken.iter() {
        e.trace
            .rec("exec.broken", u64::from(b.account.0), u64::from(b.group.0));
        crate::mode_glue::broken(e, inst, drv, b.account, b.group)?;
    }
    for t in out.timers.iter() {
        e.heap.remove_key(5, t.timer_id, t.group.0);
        if !t.cancel {
            e.heap
                .push(crate::timers::TimerEntry {
                    deadline: t.at.0,
                    owner: 5,
                    timer_id: t.timer_id,
                    group: t.group.0,
                })
                .map_err(|_| FatalReason::TimerOverflow)?;
        }
    }
    for n in out.notify.iter() {
        if let Some(ix) = inst.ix(n.owner) {
            let gix = inst
                .slots
                .get(ix)
                .and_then(|s| s.gix_of(n.u.group))
                .unwrap_or(hft_strategy_sdk::GIX_NONE);
            e.trace.rec("queue.exec", u64::from(n.owner.0), 0);
            // `ExecUpdate.inflight` = the legs' immediate in-flight (OMS `LegTrack`) after this
            // Input's orders were placed.
            let mut u = n.u;
            let lv = &e.oms.legs(n.account, n.u.group).view;
            for (d, l) in u.inflight.iter_mut().zip(lv.leg.iter()) {
                *d = l.inflight_immediate;
            }
            e.notify.push(ix as u16, Notif::Exec { gix, u });
        }
    }
    for n in out.notes.iter() {
        e.out.push(RecordFlags(0), n);
    }
    out.clear();
    Ok(())
}

/// `exec.set_target` with `owner` = the issuing instance; the outcome is `TargetAccepted`,
/// `TargetShadowed` (the decision carries SHADOW) or `TargetRejected`.
pub(crate) fn set_target<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    owner: StrategyId,
    t: &TargetIntent,
    ro: bool,
) -> R<IntentOutcome> {
    let mut xo = e.take_exec();
    let r = {
        let v = crate::noexec_views!(e);
        e.exec.set_target(owner, t, ro, &v, e.now, &mut xo)
    };
    // Live with `open_allowed = false`: an accepted target that increases a leg is recorded like
    // one set in Shadow (hft-exec "运行模式、Drain 与 Unknown"); the executor does not see
    // `open_allowed`, so the flag goes on its TargetSet/TargetReplaced record here.
    if r.is_ok() && closed_increase(e, inst, t) {
        for d in xo.decisions.as_mut_slice() {
            if d.action == hft_exec::decision::action::TARGET_SET
                || d.action == hft_exec::decision::action::TARGET_REPLACED
            {
                d.eflags |= hft_exec::decision::eflags::SHADOW;
            }
        }
    }
    let shadow = xo
        .decisions
        .iter()
        .any(|d| d.eflags & hft_exec::decision::eflags::SHADOW != 0);
    drain_out(e, inst, drv, &mut xo)?;
    e.put_exec(xo);
    Ok(match r {
        Ok(()) if shadow => IntentOutcome::TargetShadowed,
        Ok(()) => IntentOutcome::TargetAccepted,
        Err(x) => IntentOutcome::TargetRejected(x),
    })
}

/// The cell of `t` is Live with `open_allowed = false` and `t` moves some leg away from its
/// current net (LegTracker) or flips its sign.
fn closed_increase<S: StrategySet>(e: &Engine, inst: &Instances<S>, t: &TargetIntent) -> bool {
    if inst
        .spec(t.account, t.group)
        .is_none_or(|sp| sp.open_allowed != 0)
    {
        return false;
    }
    let cell = hft_risk::mode::CellKey {
        strategy: e.rd.account(t.account).bound_instance,
        account: t.account,
        group: t.group,
    };
    if e.risk.mode.effective(cell) != hft_risk::mode::Effective::Live {
        return false;
    }
    let lv = &e.oms.legs(t.account, t.group).view;
    t.legs().iter().any(|l| {
        let pos = lv.leg.get(usize::from(l.leg_ix)).map_or(0, |x| x.net.0);
        let q = l.qty.0;
        !(q == 0 || (q.signum() == pos.signum() && q.unsigned_abs() <= pos.unsigned_abs()))
    })
}

pub(crate) fn clear_target<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    owner: StrategyId,
    a: AccountId,
    g: GroupId,
) -> R<()> {
    let mut xo = e.take_exec();
    e.exec.clear_target(owner, a, g, e.now, &mut xo);
    drain_out(e, inst, drv, &mut xo)?;
    e.put_exec(xo);
    Ok(())
}

pub(crate) fn note_action<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    a: AccountId,
    g: GroupId,
) -> R<()> {
    let mut xo = e.take_exec();
    e.exec.note_strategy_action(a, g, e.now, &mut xo);
    drain_out(e, inst, drv, &mut xo)?;
    e.put_exec(xo);
    Ok(())
}

/// `exec.advance` when something is pending (README "process" 6).
pub(crate) fn advance<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
) -> R<()> {
    if !e.exec.has_pending() {
        return Ok(());
    }
    e.gpos.refresh(&e.ledger);
    e.trace.rec("exec.advance", 0, 0);
    let mut xo = e.take_exec();
    {
        let v = crate::noexec_views!(e);
        e.exec.advance(&v, e.now, &mut xo);
    }
    drain_out(e, inst, drv, &mut xo)?;
    e.put_exec(xo);
    Ok(())
}

/// `exec.on_timer`.
pub(crate) fn on_timer<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    token: u64,
) -> R<()> {
    e.trace.rec("exec.on_timer", token, 0);
    let mut xo = e.take_exec();
    e.exec.on_timer(token, e.now, &mut xo);
    drain_out(e, inst, drv, &mut xo)?;
    e.put_exec(xo);
    Ok(())
}

/// `exec.force_deleverage` (MarginOut.force).
pub(crate) fn force<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    a: AccountId,
    v: VenueId,
    frac: Rate,
) -> R<()> {
    e.trace
        .rec("exec.force_deleverage", u64::from(a.0), u64::from(v.0));
    let mut xo = e.take_exec();
    e.exec.force_deleverage(a, v, frac, e.now, &mut xo);
    drain_out(e, inst, drv, &mut xo)?;
    e.put_exec(xo);
    Ok(())
}

/// `exec.engine_flatten` (`Kill{Account, flatten}`).
pub(crate) fn flatten<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    a: AccountId,
) -> R<()> {
    e.trace.rec("exec.engine_flatten", u64::from(a.0), 0);
    let mut xo = e.take_exec();
    e.exec.engine_flatten(a, e.now, &mut xo);
    drain_out(e, inst, drv, &mut xo)?;
    e.put_exec(xo);
    Ok(())
}

/// `exec.hedge_fix_cmd`.
pub(crate) fn hedge_fix<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    a: AccountId,
    g: GroupId,
) -> R<Result<(), ExecCmdErr>> {
    e.gpos.refresh(&e.ledger);
    e.trace.rec("exec.hedge_fix_cmd", u64::from(a.0), u64::from(g.0));
    let mut xo = e.take_exec();
    let r = {
        let v = crate::noexec_views!(e);
        e.exec.hedge_fix_cmd(a, g, &v, e.now, &mut xo)
    };
    drain_out(e, inst, drv, &mut xo)?;
    e.put_exec(xo);
    Ok(r)
}

/// `exec.on_external` for one part of an external fill; returns the deadline.
pub(crate) fn on_external<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    a: AccountId,
    f: &hft_strategy_sdk::ExternalFillView,
) -> R<hft_types::Tsc> {
    e.gpos.refresh(&e.ledger);
    e.trace
        .rec("exec.on_external", u64::from(a.0), u64::from(f.group.0));
    let mut xo = e.take_exec();
    let d = {
        let v = crate::noexec_views!(e);
        e.exec.on_external(a, f, &v, e.now, &mut xo)
    };
    drain_out(e, inst, drv, &mut xo)?;
    e.put_exec(xo);
    Ok(d)
}

/// `exec.on_mode` for one effective-mode change.
pub(crate) fn on_mode<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    a: AccountId,
    g: GroupId,
    old: hft_types::RunMode,
    new: hft_types::RunMode,
) -> R<()> {
    e.trace.rec("exec.on_mode", u64::from(a.0), u64::from(g.0));
    let mut xo = e.take_exec();
    e.exec.on_mode(a, g, old, new, e.now, &mut xo);
    drain_out(e, inst, drv, &mut xo)?;
    e.put_exec(xo);
    Ok(())
}
