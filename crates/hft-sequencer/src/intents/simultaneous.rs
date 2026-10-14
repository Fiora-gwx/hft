//! Atomic local admission of both legs. The output event is published only after both slots exist.

use super::*;

pub(super) struct Prepared {
    pub ai: ApprovedIntent,
    pub view: IntentView,
    pub admission: Option<hft_risk::Admission>,
}

pub(super) fn prepare<S: StrategySet>(
    e: &Engine,
    inst: &Instances<S>,
    strategy: StrategyId,
    first: &OrderIntent,
    fv: &IntentView,
    src: Src,
) -> Result<Prepared, PipeOut> {
    let bad = || PipeOut::Rejected(RejectBits::INVALID);
    let link = first.link.get().ok_or_else(bad)?;
    let plan = &link.hedge;
    if !link_valid(link) || first.venue == plan.venue || first.inst == plan.inst {
        return Err(bad());
    }
    let qty = e.oms.simultaneous_preview(first).map_err(|err| {
        PipeOut::Rejected(match err {
            hft_oms::PlaceError::NoTable => RejectBits::INVALID,
            _ => RejectBits::CAPACITY,
        })
    })?;
    if qty.0 < e.rd.inst(plan.inst).min_qty.0.max(1) {
        return Err(bad());
    }
    let req = HedgeRequest {
        link_id: 0,
        role: LinkRole::Second,
        venue: plan.venue,
        account: plan.account,
        side: plan.side,
        reduce_only: plan.reduce_only != 0,
        inst: plan.inst,
        group: first.group,
        strategy,
        qty,
        basis: qty,
        hedge_cap_qty: qty,
        ref_px: Px(0),
        peer_venue: first.venue,
        peer_account: first.account,
        trigger_seq: e.cur_seq,
    };
    let hp = price_hedge(
        &req,
        plan,
        &e.market.bbo_view(&e.books, plan.inst),
        e.now.tsc,
        &e.risk.hedge_px_limits(plan.account, plan.venue, plan.inst),
        e.rd.inst(plan.inst).tick,
        e.now.tsc_per_ms,
    )
    .map_err(|_| PipeOut::Rejected(RejectBits::PX_BAND))?;
    let mut ai = ApprovedIntent::hedge(&req, plan, hp, IntentClass::Open);
    let fl = e.oms.inflight().pos(plan.account, plan.inst);
    let facts = PosFacts {
        pos: e.ledger.pos(plan.account, plan.inst).qty,
        pending_buy: fl.open_buy,
        pending_sell: fl.open_sell,
    };
    let draft = IntentDraft {
        account: plan.account,
        inst: plan.inst,
        side: plan.side,
        qty,
        tif: plan.tif,
        class: IntentClass::Open,
        link_role: Some(LinkRole::Second),
        simultaneous: true,
        hedge_reduce_only: req.reduce_only,
        flags: u16::from(ai.intent.flags),
    };
    let class = Risk::classify(&draft, hft_risk::Origin::Oms, &facts, e.rd.caps(plan.venue));
    let gated = matches!(src, Src::Strategy { .. })
        || matches!(src, Src::Exec { o } if o.origin == ExecOrigin::Target && class.class == IntentClass::Open);
    if gated {
        let cell = CellKey {
            strategy: e.rd.account(plan.account).bound_instance,
            account: plan.account,
            group: first.group,
        };
        let eff = e.risk.mode.effective(cell);
        if eff == Effective::Shadow
            || (eff == Effective::Live
                && class.class == IntentClass::Open
                && !open_allowed(inst, plan.account, first.group))
        {
            return Err(PipeOut::Shadowed);
        }
    }
    if matches!(
        src,
        Src::Strategy {
            reduce_only_ctx: true
        }
    ) && class.class == IntentClass::Open
    {
        return Err(PipeOut::Rejected(RejectBits::CLOSE_ONLY));
    }
    ai.class = class.class;
    ai.intent.class = class.class;
    ai.risk_reducing = class.risk_reducing;
    ai.reduce_only = class.reduce_only;
    if !class.reduce_only {
        ai.intent.flags &= !OrderIntent::REDUCE_ONLY;
    }
    ai.trig = e.trig;
    if e.rd.inst(plan.inst).kind == InstKind::Perp && class.class == IntentClass::Open {
        ai.lev_x100 = e
            .risk
            .leverage(plan.account, plan.venue, plan.inst)
            .map_or(0, |r| u16::try_from(lev_x100(r)).unwrap_or(u16::MAX));
    }
    let notional = Risk::notional(
        qty,
        hp.px,
        e.rd.asset(e.rd.inst(plan.inst).asset).exp,
        quote_rate(&e.rd, &e.ledger, plan.inst),
    );
    let view = IntentView {
        venue: plan.venue,
        account: plan.account,
        side: plan.side,
        class: class.class,
        inst: plan.inst,
        group: first.group,
        strategy,
        origin: hft_risk::Origin::Oms,
        link_role: Some(LinkRole::Second),
        tif: plan.tif,
        flags: oflags_of(&ai.intent, &class, 0),
        is_swap: false,
        px: hp.px,
        qty,
        notional,
        link_venue: first.venue,
        link_account: first.account,
        link_side: first.side,
        link_inst: first.inst,
        link_qty: first.qty,
        link_notional: fv.notional,
        hedge_cap_qty: qty,
    };
    Ok(Prepared {
        ai,
        view,
        admission: None,
    })
}

pub(super) fn place<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    mut p: Prepared,
    first_out: &mut OmsOut,
) -> R<()> {
    let req = *first_out
        .hedges
        .as_slice()
        .first()
        .ok_or(FatalReason::PairAdmission)?;
    if first_out.hedges.len() != 1 || req.qty != p.ai.intent.qty {
        return Err(FatalReason::PairAdmission);
    }
    first_out.hedges.clear();
    p.ai.link_id = req.link_id;
    let admission = p.admission.ok_or(FatalReason::PairAdmission)?;
    let mut out = e.take_oms();
    #[cfg(feature = "testkit")]
    let facts = crate::audit::capture(e, &p.ai.intent, Src::Hedge { req }, p.ai.class);
    let h = e
        .oms
        .place(&p.ai, e.now.tsc, e.cur_seq, &mut out)
        .map_err(|_| FatalReason::PairAdmission)?;
    e.risk.commit(&admission, &e.now);
    #[cfg(feature = "testkit")]
    for d in out.decisions.as_slice() {
        e.audit.pending(*d, facts);
    }
    e.out.stamped.push(h);
    drain_oms(e, inst, drv, &mut out)?;
    e.put_oms(out);
    Ok(())
}

/// Replace the two staged frames with a single paired emission, preserving their decisions.
pub(super) fn bundle(e: &mut Engine, first: OrderRef) -> R<()> {
    let link = e.oms.slot(first).ok_or(FatalReason::PairAdmission)?.link_id;
    let mut found = [None; 2];
    for (ix, em) in e.out.emits.iter().enumerate() {
        if let Emit::Gw {
            ring,
            cmd,
            rec: Some(rec),
        } = em
        {
            let venue = match ring {
                OutRing::Venue(v) => *v,
                OutRing::Sim => {
                    // The slot's venue is recovered from the linked pair's instrument.
                    e.rd.inst(cmd.inst).venue
                }
            };
            let h = OrderRef::new(venue, cmd.account, cmd.slot.0, cmd.r#gen);
            if e.oms.slot(h).is_some_and(|s| s.link_id == link) {
                let k = usize::from(h != first);
                found[k] = Some((ix, *ring, *cmd, *rec));
            }
        }
    }
    let [Some((a, ar, ac, ai)), Some((b, br, bc, bi))] = found else {
        return Err(FatalReason::PairAdmission);
    };
    e.out.emits.remove(a.max(b));
    e.out.emits.remove(a.min(b));
    e.out.emits.push(Emit::Pair {
        rings: [ar, br],
        cmds: [ac, bc],
        recs: [ai, bi],
    });
    Ok(())
}
