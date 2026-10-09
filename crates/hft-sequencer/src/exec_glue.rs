//! Executor glue (README "目标意图与执行层", "process" 6): target hand-over, `advance`, and the
//! fixed-order drain of `ExecOut` after every call that takes an `out`.

use hft_event::RecordFlags;
use hft_exec::{ExecCmdErr, ExecOut};
use hft_strategy_sdk::{IntentOutcome, StrategySet, TargetIntent};
use hft_types::{AccountId, GroupId, InstId, Rate, StrategyId, VenueId};

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
    let decisions: Vec<hft_event::ExecDecision> = out.decisions.as_slice().to_vec();
    let cancel_resting: Vec<hft_exec::CancelResting> = out.cancel_resting.as_slice().to_vec();
    let orders: Vec<hft_exec::ExecOrder> = out.orders.as_slice().to_vec();
    let cancels: Vec<hft_strategy_sdk::OrderRef> = out.cancels.as_slice().to_vec();
    let broken: Vec<hft_exec::BrokenReq> = out.broken.as_slice().to_vec();
    let timers: Vec<hft_exec::ExecTimerReq> = out.timers.as_slice().to_vec();
    let notify: Vec<hft_exec::ExecNotify> = out.notify.as_slice().to_vec();
    let notes: Vec<hft_event::Note> = out.notes.as_slice().to_vec();
    out.clear();
    for d in decisions.iter() {
        let flags = if d.eflags & hft_exec::decision::eflags::SHADOW != 0 {
            RecordFlags::SHADOW
        } else {
            RecordFlags(0)
        };
        e.trace
            .rec("exec_decision", u64::from(d.action), u64::from(d.group.0));
        e.out.push(flags, d);
    }
    for c in cancel_resting.iter() {
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
    for o in orders.iter() {
        e.trace.rec("exec.order", u64::from(o.id.packed()), 0);
        let r = crate::intents::place_exec(e, inst, drv, o)?;
        e.trace.rec("exec.on_place_result", u64::from(o.id.packed()), 0);
        let mut xo = e.take_exec();
        e.exec.on_place_result(o.id, r, e.now, &mut xo);
        drain_out(e, inst, drv, &mut xo)?;
        e.put_exec(xo);
    }
    for c in cancels.iter() {
        e.trace.rec("oms.cancel", u64::from(c.slot), 1);
        let mut o = e.take_oms();
        let trig = e.trig;
        if e.oms.cancel(*c, &trig, e.now.tsc, e.cur_seq, &mut o).is_ok() {
            e.out.stamped.push(*c);
        }
        crate::intents::drain_oms(e, inst, drv, &mut o)?;
        e.put_oms(o);
    }
    for b in broken.iter() {
        e.trace
            .rec("exec.broken", u64::from(b.account.0), u64::from(b.group.0));
        crate::mode_glue::broken(e, inst, drv, b.account, b.group)?;
    }
    for t in timers.iter() {
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
    for n in notify.iter() {
        if let Some(ix) = inst.ix(n.owner) {
            let gix = inst
                .slots
                .get(ix)
                .and_then(|s| s.gix_of(n.u.group))
                .unwrap_or(hft_strategy_sdk::GIX_NONE);
            e.trace.rec("queue.exec", u64::from(n.owner.0), 0);
            e.notify.push(ix as u16, Notif::Exec { gix, u: n.u });
        }
    }
    for n in notes.iter() {
        e.out.push(RecordFlags(0), n);
    }
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

#[allow(dead_code)]
fn _inst_unused(_i: InstId) {}
