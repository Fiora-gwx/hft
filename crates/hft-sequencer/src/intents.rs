//! The intent pipeline (README "意图管线") for every source — strategy `place`, hft-exec orders,
//! OMS hedge requests and smoke orders — plus cancel/amend and the fixed-order drain of `OmsOut`
//! (README "process" 3).

use hft_event::{GwCmd, NoteLevel, OrderAction, OrderDecision, OrderFlags, RecordFlags};
use hft_exec::{ExecOrder, ExecOrigin, LinkBrokenCause, LinkNotice, LinkPhase, PlaceResult};
use hft_oms::{
    ApprovedIntent, HedgeGate, HedgePx, HedgeRequest, OmsOut, OmsTimerKind, OrderState, link_valid,
    price_hedge,
};
use hft_refdata::InstKind;
use hft_risk::mode::{CellKey, Effective, ModeInput};
use hft_risk::{Classification, IntentDraft, IntentView, PosFacts, Risk};
use hft_strategy_sdk::ctx::CancelReq;
use hft_strategy_sdk::{HedgeKind, IntentClass, IntentOutcome, LinkRole, OrderIntent, OrderRef, StrategySet};
use hft_types::{
    AccountId, CapSource, GroupId, InstId, Px, Qty, RejectBits, Side, SlotId, StrategyId, Tsc, VenueId,
};

use crate::Driver;
use crate::engine::{Emit, Engine};
use crate::notify::Notif;
use crate::strategies::{Instances, full_views, gix_for};
use crate::types::{FatalReason, OutRing};
use crate::views::{RiskAdapter, SeqViews, lev_x100, quote_rate};

type R<T> = Result<T, FatalReason>;

/// Where an intent comes from.
#[derive(Copy, Clone, Debug)]
pub(crate) enum Src {
    Strategy { reduce_only_ctx: bool },
    Exec { o: ExecOrder },
    Hedge { req: HedgeRequest },
    Smoke { ttl_ms: u32 },
}

/// The outcome of the pipeline.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum PipeOut {
    Placed(OrderRef),
    Rejected(RejectBits),
    Shadowed,
    /// Hedge gated (price), nothing written.
    Gated,
}

fn views_noexec(e: &Engine) -> SeqViews<'_> {
    let mut v = full_views(e);
    v.exec = None;
    v
}

fn risk_origin(s: &Src) -> hft_risk::Origin {
    match s {
        Src::Strategy { .. } => hft_risk::Origin::Strategy,
        Src::Exec { .. } => hft_risk::Origin::Exec,
        Src::Hedge { .. } => hft_risk::Origin::Oms,
        Src::Smoke { .. } => hft_risk::Origin::Command,
    }
}

fn oms_origin(s: &Src) -> hft_oms::Origin {
    match s {
        Src::Strategy { .. } => hft_oms::Origin::Strategy,
        Src::Exec { .. } => hft_oms::Origin::Exec,
        Src::Hedge { .. } => hft_oms::Origin::Oms,
        Src::Smoke { .. } => hft_oms::Origin::Command,
    }
}

/// A decision that is only recorded (rejected or shadowed): no slot, no tokens, no in-flight.
#[allow(clippy::too_many_arguments)]
fn record_only(
    e: &mut Engine,
    strategy: StrategyId,
    it: &OrderIntent,
    reject: RejectBits,
    shadow: bool,
    oflags: u16,
    reason: u32,
    intent_no: u32,
) {
    let d = OrderDecision {
        trigger_seq: e.cur_seq,
        decided_tsc: Tsc(0),
        px: it.px,
        qty: it.qty,
        group: it.group,
        inst: it.inst,
        slot: SlotId(0),
        r#gen: 0,
        target_slot: SlotId(0),
        reject: reject.0,
        strategy,
        venue: it.venue,
        account: it.account,
        action: OrderAction::New,
        side: it.side,
        tif: it.tif,
        oflags,
        reason,
        intent_no,
        req_px: it.px,
        link_role: it.link.get().map_or(0, |l| l.role as u8),
        _pad: [0; 7],
    };
    let flags = if shadow {
        RecordFlags::SHADOW
    } else {
        RecordFlags(0)
    };
    e.trace.rec(
        if shadow {
            "decision.shadow"
        } else {
            "decision.reject"
        },
        u64::from(reject.0),
        0,
    );
    e.out.push(flags, &d);
}

fn oflags_of(it: &OrderIntent, c: &Classification, extra: u16) -> u16 {
    let mut f = extra;
    // `REDUCE_ONLY` is hft-risk `classify()`'s result alone (hft-oms "分类"): it already folds
    // in the intent's own reduce-only request and the venue's `VenueCaps.reduce_only`, so a spot
    // venue never receives the flag (a `Reverse` on spot is a plain IOC).
    if c.reduce_only {
        f |= OrderFlags::REDUCE_ONLY.0;
    }
    if c.risk_reducing {
        f |= OrderFlags::RISK_REDUCING.0;
    }
    if it.tif == hft_types::Tif::PostOnly {
        f |= OrderFlags::POST_ONLY.0;
    }
    if it.link.get().is_some() {
        f |= OrderFlags::LINKED.0;
    }
    f
}

/// The pipeline (README "意图管线" 0–8).
pub(crate) fn pipeline<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    strategy: StrategyId,
    it0: &OrderIntent,
    src: Src,
    intent_no: u32,
) -> R<PipeOut> {
    let mut it = *it0;
    let mut hedge_px = HedgePx::default();
    let mut hedge_cap = Qty(0);
    let mut reason = 0u32;
    let mut extra_flags = 0u16;
    let mut smoke_ttl = 0u32;
    // 0. Link structure (strategy place with a LegLink).
    if let Src::Strategy { .. } = src
        && let Some(l) = it.link.get()
        && !link_valid(l)
    {
        #[cfg(feature = "testkit")]
        let audit_facts = crate::audit::capture(e, &it, src, it.class);
        record_only(e, strategy, &it, RejectBits::INVALID, false, 0, 0, intent_no);
        #[cfg(feature = "testkit")]
        e.audit.record((e.out.recs.len() - 1) as u32, audit_facts);
        return Ok(PipeOut::Rejected(RejectBits::INVALID));
    }
    // 1. Hedge pricing.
    if let Src::Hedge { req } = src {
        let Some((_, plan)) = e.oms.link_plan(req.link_id).copied() else {
            return Ok(PipeOut::Gated);
        };
        let lim = e.risk.hedge_px_limits(req.account, req.venue, req.inst);
        let bbo = e.market.bbo_view(&e.books, req.inst);
        // Planted: the quote's last change stands in for the connection's liveness evidence.
        #[cfg(feature = "planted")]
        let bbo = if crate::planted::on(crate::planted::HEDGE_PX_BOOK_AGE) {
            hft_strategy_sdk::BboView {
                conn_live_tsc: bbo.rx_tsc,
                ..bbo
            }
        } else {
            bbo
        };
        let tick = e.rd.inst(req.inst).tick;
        match price_hedge(&req, &plan, &bbo, e.now.tsc, &lim, tick, e.now.tsc_per_ms) {
            Ok(hp) => {
                hedge_px = hp;
                hedge_cap = req.hedge_cap_qty;
                let class = if req.reduce_only {
                    IntentClass::Reduce
                } else {
                    IntentClass::Hedge
                };
                let ai = ApprovedIntent::hedge(&req, &plan, hp, class);
                it = ai.intent;
            }
            Err(err) => {
                e.trace.rec("oms.hedge_gated", u64::from(req.link_id), 0);
                let mut o = e.take_oms();
                e.oms.hedge_gated(
                    req.link_id,
                    HedgeGate::Px(err),
                    Tsc(0),
                    e.now.tsc,
                    e.cur_seq,
                    &mut o,
                );
                drain_oms(e, inst, drv, &mut o)?;
                e.put_oms(o);
                return Ok(PipeOut::Gated);
            }
        }
    }
    if let Src::Exec { o } = src {
        hedge_cap = o.hedge_cap_qty;
        reason = u32::from(o.kind.code());
    }
    if let Src::Smoke { ttl_ms } = src {
        extra_flags |= OrderFlags::SMOKE.0;
        smoke_ttl = ttl_ms;
    }
    let account = it.account;
    let venue = it.venue;
    let inst_id = it.inst;
    // 3. classify (needed by the mode gate for Exec Target increase orders).
    let pos = e.ledger.pos(account, inst_id).qty;
    let fl = e.oms.inflight().pos(account, inst_id);
    let facts = PosFacts {
        pos,
        pending_buy: fl.open_buy,
        pending_sell: fl.open_sell,
    };
    let link_role = it.link.get().map(|l| l.role);
    let simul = it
        .link
        .get()
        .is_some_and(|l| l.hedge.kind == HedgeKind::Simultaneous);
    let draft = IntentDraft {
        account,
        inst: inst_id,
        side: it.side,
        qty: it.qty,
        tif: it.tif,
        class: it.class,
        link_role,
        simultaneous: simul,
        hedge_reduce_only: it.reduce_only(),
        flags: u16::from(it.flags),
    };
    let class = Risk::classify(&draft, risk_origin(&src), &facts, e.rd.caps(venue));
    #[cfg(feature = "testkit")]
    let audit_facts = crate::audit::capture(e, &it, src, class.class);
    let oflags = oflags_of(&it, &class, extra_flags);
    // 2. Mode gate: strategy place and exec Target-origin increase orders.
    let gated = match src {
        Src::Strategy { .. } => true,
        Src::Exec { o } => o.origin == ExecOrigin::Target && class.class == IntentClass::Open,
        _ => false,
    };
    if gated {
        let cell = CellKey {
            strategy: e.rd.account(account).bound_instance,
            account,
            group: it.group,
        };
        let eff = e.risk.mode.effective(cell);
        e.trace.rec("mode_gate", 0, 0);
        match eff {
            Effective::Shadow => {
                record_only(e, strategy, &it, RejectBits(0), true, oflags, reason, intent_no);
                #[cfg(feature = "testkit")]
                e.audit.record((e.out.recs.len() - 1) as u32, audit_facts);
                return Ok(PipeOut::Shadowed);
            }
            Effective::Live if class.class == IntentClass::Open && !open_allowed(inst, account, it.group) => {
                record_only(
                    e,
                    strategy,
                    &it,
                    RejectBits::MODE_SHADOW,
                    false,
                    oflags,
                    reason,
                    intent_no,
                );
                #[cfg(feature = "testkit")]
                e.audit.record((e.out.recs.len() - 1) as u32, audit_facts);
                return Ok(PipeOut::Shadowed);
            }
            _ => {}
        }
    }
    if let Src::Strategy {
        reduce_only_ctx: true,
    } = src
        && class.class == IntentClass::Open
    {
        record_only(
            e,
            strategy,
            &it,
            RejectBits::CLOSE_ONLY,
            false,
            oflags,
            reason,
            intent_no,
        );
        #[cfg(feature = "testkit")]
        e.audit.record((e.out.recs.len() - 1) as u32, audit_facts);
        return Ok(PipeOut::Rejected(RejectBits::CLOSE_ONLY));
    }
    // 4. IntentView.
    let qr = quote_rate(&e.rd, &e.ledger, inst_id);
    let exp = e.rd.asset(e.rd.inst(inst_id).asset).exp;
    let mut iv = IntentView {
        venue,
        account,
        side: it.side,
        class: class.class,
        inst: inst_id,
        group: it.group,
        strategy,
        origin: risk_origin(&src),
        link_role,
        tif: it.tif,
        flags: oflags,
        is_swap: false,
        px: it.px,
        qty: it.qty,
        notional: Risk::notional(it.qty, it.px, exp, qr),
        link_venue: VenueId(0),
        link_account: AccountId(0),
        link_side: Side::Buy,
        link_inst: InstId(0),
        link_qty: Qty(0),
        link_notional: hft_types::Notional(0),
        hedge_cap_qty: hedge_cap,
    };
    let mut hedge_ref_px = Px(0);
    if let Some(l) = it.link.get()
        && l.role == LinkRole::First
        && class.class == IntentClass::Open
    {
        let p = l.hedge;
        iv.link_venue = p.venue;
        iv.link_account = p.account;
        iv.link_inst = p.inst;
        iv.link_side = p.side;
        let b = e.market.bbo_view(&e.books, p.inst);
        let rr = e.risk.view_for(p.account, p.inst, &e.now).rr_max_cross;
        let lexp = e.rd.asset(e.rd.inst(p.inst).asset).exp;
        (iv.link_qty, iv.link_notional) = crate::views::link_projection(
            it.qty,
            p.ratio_num,
            p.ratio_den,
            b.bid_px,
            b.ask_px,
            rr,
            lexp,
            quote_rate(&e.rd, &e.ledger, p.inst),
        );
        let lim = e.risk.hedge_px_limits(p.account, p.venue, p.inst);
        let silence = u64::from(lim.conn_liveness_ms).saturating_mul(e.now.tsc_per_ms);
        if b.quote_live(e.now.tsc, silence) {
            hedge_ref_px = match p.side {
                Side::Buy => b.ask_px,
                Side::Sell => b.bid_px,
            };
        }
    }
    // The peer of a Simultaneous second leg (README "意图管线" 4); OnFill hedges have none.
    if let Src::Hedge { req } = src
        && req.role == LinkRole::Second
        && e.oms
            .link_plan(req.link_id)
            .is_some_and(|(_, p)| p.kind == hft_strategy_sdk::HedgeKind::Simultaneous)
    {
        iv.link_venue = req.peer_venue;
        iv.link_account = req.peer_account;
        // The peer is the first leg: its inst is the one whose quote health #5 checks
        // (`LINK_STALE`); without it the check reads the empty quote of inst 0.
        if let Some(first) = e.oms.link(req.link_id).map(|l| l.first)
            && let Some(s) = e.oms.slot(first)
        {
            iv.link_inst = s.inst;
            iv.link_side = s.side;
        }
    }
    // 5. evaluate.
    if e.trace.on {
        e.trace.views.push(iv);
    }
    let res = {
        let v = RiskAdapter { v: views_noexec(e) };
        e.risk.evaluate(&iv, &v, &e.now)
    };
    e.trace.rec("risk.evaluate", u64::from(account.0), 0);
    let adm = match res {
        Ok(a) => a,
        Err(bits) => {
            record_only(e, strategy, &it, bits, false, oflags, reason, intent_no);
            #[cfg(feature = "testkit")]
            e.audit.record((e.out.recs.len() - 1) as u32, audit_facts);
            if let Src::Hedge { req } = src {
                if bits.0 & RejectBits::HEDGE_LIMIT.0 != 0 {
                    e.trace.rec("oms.hedge_vetoed", u64::from(req.link_id), 0);
                    let mut o = e.take_oms();
                    e.oms.hedge_vetoed(req.link_id, e.now.tsc, e.cur_seq, &mut o);
                    drain_oms(e, inst, drv, &mut o)?;
                    e.put_oms(o);
                    crate::mode_glue::broken(e, inst, drv, req.account, req.group)?;
                } else {
                    let retry_at = if bits.0 & RejectBits::RATE_BACKOFF.0 != 0 {
                        e.now.tsc_of(e.risk.backoff_until(req.venue, req.account))
                    } else {
                        Tsc(0)
                    };
                    e.trace.rec("oms.hedge_gated", u64::from(req.link_id), retry_at.0);
                    let mut o = e.take_oms();
                    e.oms.hedge_gated(
                        req.link_id,
                        HedgeGate::Risk(bits),
                        retry_at,
                        e.now.tsc,
                        e.cur_seq,
                        &mut o,
                    );
                    drain_oms(e, inst, drv, &mut o)?;
                    e.put_oms(o);
                }
            }
            return Ok(PipeOut::Rejected(bits));
        }
    };
    // 7. commit → place.
    e.risk.commit(&adm, &e.now);
    let lev = if e.rd.inst(inst_id).kind == InstKind::Perp && class.class == IntentClass::Open {
        e.risk
            .leverage(account, venue, inst_id)
            .map_or(0, |r| u16::try_from(lev_x100(r)).unwrap_or(u16::MAX))
    } else {
        0
    };
    let mut ai = ApprovedIntent::new(it, oms_origin(&src), strategy, class.class);
    if let Src::Hedge { req } = src {
        let plan = e.oms.link_plan(req.link_id).map(|x| x.1);
        if let Some(plan) = plan {
            ai = ApprovedIntent::hedge(&req, &plan, hedge_px, class.class);
        }
    }
    ai.risk_reducing = class.risk_reducing;
    ai.reduce_only = class.reduce_only;
    if !class.reduce_only {
        ai.intent.flags &= !OrderIntent::REDUCE_ONLY;
    }
    ai.lev_x100 = lev;
    ai.smoke_ttl_ms = smoke_ttl;
    ai.hedge_px = hedge_px;
    ai.hedge_ref_px = hedge_ref_px;
    ai.trig = e.trig;
    ai.reason = reason;
    ai.intent_no = intent_no;
    if extra_flags & OrderFlags::SMOKE.0 != 0 {
        ai.intent.flags |= 0;
    }
    e.trace.rec("oms.place", u64::from(inst_id.0), u64::from(lev));
    let mut o = e.take_oms();
    #[cfg(feature = "testkit")]
    let place_facts = crate::audit::capture_place(e, &ai);
    let r = e.oms.place(&ai, e.now.tsc, e.cur_seq, &mut o);
    #[cfg(feature = "testkit")]
    if let (Some(facts), Err(error)) = (place_facts, r) {
        e.audit.place_failed(facts, error);
    }
    #[cfg(feature = "testkit")]
    for d in o.decisions.as_slice() {
        e.audit.pending(*d, audit_facts);
    }
    drain_oms(e, inst, drv, &mut o)?;
    e.put_oms(o);
    match r {
        Ok(h) => {
            e.out.stamped.push(h);
            Ok(PipeOut::Placed(h))
        }
        Err(_) => Ok(PipeOut::Rejected(RejectBits::LOCAL_SEND_FAIL)),
    }
}

fn open_allowed<S: StrategySet>(inst: &Instances<S>, account: AccountId, g: GroupId) -> bool {
    inst.spec(account, g).is_none_or(|sp| sp.open_allowed != 0)
}

/// Strategy `place`.
pub(crate) fn place_strategy<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    strategy: StrategyId,
    it: &OrderIntent,
    ro: bool,
    intent_no: u32,
) -> R<IntentOutcome> {
    let r = pipeline(
        e,
        inst,
        drv,
        strategy,
        it,
        Src::Strategy { reduce_only_ctx: ro },
        intent_no,
    )?;
    Ok(match r {
        PipeOut::Placed(h) => IntentOutcome::Placed(h),
        PipeOut::Rejected(b) => IntentOutcome::Rejected(b),
        PipeOut::Shadowed => IntentOutcome::Shadowed,
        PipeOut::Gated => IntentOutcome::Rejected(RejectBits::INVALID),
    })
}

/// hft-exec order; the result goes back through `on_place_result`.
pub(crate) fn place_exec<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    o: &ExecOrder,
) -> R<PlaceResult> {
    #[cfg(feature = "testing")]
    let o = &{
        let mut x = *o;
        if x.kind.is_hedge()
            && let Some(cap) = e.test_exec_hedge_cap.take()
        {
            x.hedge_cap_qty = cap;
        }
        x
    };
    let strategy = e.rd.account(o.intent.account).bound_instance;
    let r = pipeline(e, inst, drv, strategy, &o.intent, Src::Exec { o: *o }, 0)?;
    let pr = match r {
        PipeOut::Placed(h) => PlaceResult::Placed(h),
        PipeOut::Rejected(b) => PlaceResult::Rejected(b),
        PipeOut::Shadowed | PipeOut::Gated => PlaceResult::Shadowed,
    };
    // `HEDGE_LIMIT` is terminal; the caller sets Broken after `on_place_result` (README "意图管线" 6).
    Ok(pr)
}

/// An OMS hedge request (second leg, retry, reverse).
pub(crate) fn place_hedge<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    req: &HedgeRequest,
) -> R<PipeOut> {
    let it = OrderIntent {
        venue: req.venue,
        account: req.account,
        side: req.side,
        tif: hft_types::Tif::Ioc,
        class: IntentClass::Hedge,
        flags: 0,
        _pad: [0; 2],
        inst: req.inst,
        group: req.group,
        px: Px(0),
        qty: req.qty,
        link: hft_strategy_sdk::LegLinkOpt::NONE,
        tag: 0,
        max_rest_ms: 0,
    };
    e.trace.rec("hedge", u64::from(req.link_id), 0);
    pipeline(e, inst, drv, req.strategy, &it, Src::Hedge { req: *req }, 0)
}

/// Cancel or amend from a strategy `Ctx` (trigger stamps of this Input).
pub(crate) fn cancel<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    c: &CancelReq,
) -> R<()> {
    let mut o = e.take_oms();
    let trig = e.trig;
    let r = match c.amend {
        None => {
            e.trace.rec("oms.cancel", u64::from(c.r.slot), 0);
            e.oms.cancel(c.r, &trig, e.now.tsc, e.cur_seq, &mut o)
        }
        Some((px, qty)) => {
            e.trace.rec("oms.amend", u64::from(c.r.slot), 0);
            e.oms.amend(c.r, px, qty, &trig, e.now.tsc, e.cur_seq, &mut o)
        }
    };
    if r.is_ok() {
        e.out.stamped.push(c.r);
    }
    drain_oms(e, inst, drv, &mut o)?;
    e.put_oms(o);
    Ok(())
}

/// The `GwCmd` projection of an `OrderDecision`.
fn gw_of(d: &OrderDecision) -> GwCmd {
    let op = match d.action {
        OrderAction::New => hft_event::GwOp::Place,
        OrderAction::Cancel => hft_event::GwOp::Cancel,
        OrderAction::Amend => hft_event::GwOp::Amend,
        OrderAction::CancelAll => hft_event::GwOp::CancelAll,
        OrderAction::Query => hft_event::GwOp::Query,
    };
    GwCmd {
        decision_seq: 0,
        decided_tsc: Tsc(0),
        px: d.px,
        qty: d.qty,
        inst: d.inst,
        slot: d.slot,
        r#gen: d.r#gen,
        target_slot: d.target_slot,
        op,
        account: d.account,
        side: d.side,
        tif: d.tif,
        oflags: d.oflags,
        _pad: [0; 2],
        arg: 0,
        arg2: 0,
    }
}

/// The out ring of an account's orders.
pub(crate) fn ring_of(e: &Engine, a: AccountId, v: VenueId) -> OutRing {
    if e.is_sim(a) {
        OutRing::Sim
    } else {
        OutRing::Venue(v)
    }
}

fn link_phase(s: hft_oms::LinkState) -> LinkPhase {
    match s {
        hft_oms::LinkState::Hedging => LinkPhase::Hedging,
        hft_oms::LinkState::Done => LinkPhase::Done,
        hft_oms::LinkState::Broken => LinkPhase::Broken,
        _ => LinkPhase::Open,
    }
}

fn link_cause(b: hft_oms::LinkBroken) -> LinkBrokenCause {
    match b {
        hft_oms::LinkBroken::RetryExhausted => LinkBrokenCause::RetryExhausted,
        hft_oms::LinkBroken::Deadline => LinkBrokenCause::Deadline,
        hft_oms::LinkBroken::HedgeLimit => LinkBrokenCause::HedgeLimit,
        hft_oms::LinkBroken::Overfill => LinkBrokenCause::Overfill,
        hft_oms::LinkBroken::ReverseFailed => LinkBrokenCause::ReverseFailed,
        hft_oms::LinkBroken::ResumeTimeout => LinkBrokenCause::ResumeTimeout,
        _ => LinkBrokenCause::None,
    }
}

/// Drains an `OmsOut` in the fixed order (README "process" 3): hedges → fills → fees → updates →
/// links → foreign fills → decisions → timers → notes, latency; then the P0 `OmsUnknown` caps.
pub(crate) fn drain_oms<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    out: &mut OmsOut,
) -> R<()> {
    if let Some(error) = out.cash_error {
        return Err(FatalReason::OmsCash(error));
    }
    // OMS has already matched the account/venue/instrument and positive net quantity.
    // Reject an unusable increment price before a linked hedge can enter the pipeline.
    if out.fills.iter().any(|f| {
        f.px.0 <= 0
            && (f.cash.is_some() || (e.rd.inst(f.inst).kind == InstKind::Spot && f.notional.is_some()))
    }) {
        return Err(FatalReason::Settlement(hft_ledger::SettlementError::InvalidPrice));
    }
    if out
        .fills
        .iter()
        .any(|f| e.rd.inst(f.inst).kind == InstKind::Spot && f.notional.is_some_and(|n| n <= 0))
    {
        return Err(FatalReason::Settlement(
            hft_ledger::SettlementError::InvalidNotional,
        ));
    }
    // `out` is checked out of the engine's pool (or owned by the caller), never part of `e`:
    // nested calls take other pooled buffers, so it is drained in place in the fixed order
    // without heap copies (hot-path rule M1; the fill → second leg path is `seq/fill_to_hedge`).
    // Hedges (fast path first, after the facts have been validated).
    for h in out.hedges.iter() {
        place_hedge(e, inst, drv, h)?;
    }
    // PnL of the fills and fees goes to risk after the whole output is drained. A stack buffer
    // sized by the output's own capacities (fills + fees) holds every delta.
    let mut pnls = PnlBuf::new();
    // Fills → ledger.
    for f in out.fills.iter() {
        let fd = hft_ledger::FillDelta {
            account: f.account,
            venue: f.venue,
            side: f.side,
            class: f.class,
            flags: f.flags,
            leg_ix: f.leg_ix,
            _pad: [0; 2],
            inst: f.inst,
            group: f.group,
            qty: f.qty,
            // OMS `FillOut.net_qty` is signed (buy +); the ledger takes the absolute net.
            net_qty: Qty(f.net_qty.0.saturating_mul(f.side.sign())),
            px: f.px,
            fee: f.fee,
            fee_asset: f.fee_asset,
            _pad2: 0,
            slot: f.slot.slot,
            r#gen: f.slot.r#gen,
            _pad3: 0,
            seq: f.seq,
        };
        // Seq of the latest fill per (account, inst): `AdoptExchange`'s `StaleReport` check and
        // the `AUTO_EXTERNAL` precondition read it.
        // Fixed-size table: no allocation on the fill path; refdata bounds every fill key.
        let stored = e.cmd.fill_seq.set(f.account, f.inst, e.cur_seq);
        debug_assert!(stored, "fill outside refdata dimensions");
        let before = e.ledger.pos(f.account, f.inst).qty.0 != 0;
        let r = if let Some(amount) = f.cash {
            let cash = hft_ledger::PerpCashSettlement {
                asset: e.rd.inst(f.inst).quote,
                amount,
            };
            e.ledger
                .apply_settled_fill(&fd, cash, e.now.engine_ns)
                .map_err(FatalReason::Settlement)?
        } else if let Some(notional) = f.notional
            && e.rd.inst(f.inst).kind == InstKind::Spot
        {
            e.ledger
                .apply_spot_fill(&fd, notional, e.now.engine_ns)
                .map_err(FatalReason::Settlement)?
        } else {
            e.ledger.apply_fill(&fd, e.now.engine_ns)
        };
        e.trace
            .rec("ledger.apply_fill", u64::from(f.account.0), u64::from(f.inst.0));
        if !pnls.push(r.pnl) {
            // Only an over-capacity test output gets here: book it now rather than lose it.
            crate::process::pnl(e, inst, drv, r.pnl)?;
        }
        if let Some(n) = r.net_adjust {
            e.trace
                .rec("oms.net_adjust", u64::from(n.account.0), u64::from(n.group.0));
            e.oms.net_adjust(n.account, n.group, n.inst, n.delta);
            e.exec.on_leg_adjust(n.account, n.group);
        }
        let after = e.ledger.pos(f.account, f.inst).qty.0 != 0;
        if before != after {
            e.pos_touched.push(f.inst);
        }
        crate::process::touch_legs(e, f.account, f.group);
    }
    // Fees → ledger.
    for f in out.fees.iter() {
        let side = if f.side == 2 { Side::Sell } else { Side::Buy };
        let fd = hft_ledger::FeeDelta {
            account: f.account,
            venue: f.venue,
            inst: f.inst,
            group: f.group,
            side,
            fee: f.fee,
            fee_asset: f.fee_asset,
            seq: e.cur_seq,
        };
        let r = e.ledger.apply_fee(&fd, e.now.engine_ns);
        e.trace.rec("ledger.apply_fee", u64::from(f.account.0), 0);
        if !pnls.push(r.pnl) {
            // Only an over-capacity test output gets here: book it now rather than lose it.
            crate::process::pnl(e, inst, drv, r.pnl)?;
        }
        if let Some(n) = r.net_adjust {
            e.trace
                .rec("oms.net_adjust", u64::from(n.account.0), u64::from(n.group.0));
            e.oms.net_adjust(n.account, n.group, n.inst, n.delta);
            e.exec.on_leg_adjust(n.account, n.group);
            crate::process::touch_legs(e, n.account, n.group);
        }
    }
    // Updates → exec (EXEC slots), nobody (SMOKE), else the owner strategy.
    for u in out.updates.iter() {
        let origin = e.oms.slot_ext(u.r).map_or(1, |x| x.origin);
        let smoke = e
            .oms
            .slot(u.r)
            .is_some_and(|s| s.oflags & OrderFlags::SMOKE.0 != 0);
        if smoke {
            continue;
        }
        if origin == hft_oms::Origin::Exec as u8 {
            e.trace.rec("exec.on_order_update", u64::from(u.r.slot), 0);
            let mut xo = e.take_exec();
            {
                let v = crate::noexec_views!(e);
                e.exec.on_order_update(u, &v, e.now, &mut xo);
            }
            crate::exec_glue::drain_out(e, inst, drv, &mut xo)?;
            e.put_exec(xo);
        } else {
            let strategy = e.oms.slot(u.r).map_or(StrategyId(0), |s| s.strategy);
            let strategy = if strategy.0 == 0 {
                e.rd.account(u.r.account).bound_instance
            } else {
                strategy
            };
            if let Some(ix) = inst.ix(strategy) {
                let gix = inst
                    .slots
                    .get(ix)
                    .and_then(|s| s.gix_of(u.group))
                    .unwrap_or(hft_strategy_sdk::GIX_NONE);
                e.trace
                    .rec("queue.order", u64::from(strategy.0), u64::from(u.r.slot));
                e.notify.push(ix as u16, Notif::Order { gix, u: *u });
            }
        }
        if let Some(s) = e.oms.slot(u.r)
            && s.state == OrderState::Live
        {
            crate::mode_glue::cancel_obligation_on_live(e, inst, drv, u.r.account, u.r.venue)?;
        }
    }
    // Links → exec.on_link.
    for l in out.links.iter() {
        let n = LinkNotice {
            link_id: l.link_id,
            account: l.account,
            group: l.group,
            first: l.first,
            state: link_phase(l.state),
            broken: link_cause(l.broken_reason),
            flags: l.flags,
            first_net: l.first_net,
            hedge_filled: l.hedge_filled,
            owed: l.owed,
            tolerance: l.tolerance,
            residual: l.residual_policy,
        };
        e.trace.rec("exec.on_link", u64::from(l.link_id), 0);
        let mut xo = e.take_exec();
        e.exec.on_link(&n, e.now, &mut xo);
        crate::exec_glue::drain_out(e, inst, drv, &mut xo)?;
        e.put_exec(xo);
        if l.state == hft_oms::LinkState::Broken {
            crate::mode_glue::broken(e, inst, drv, l.account, l.group)?;
        }
    }
    // OMS cash is an increment of the original Input's cumulative facts. The cold consumer
    // independently rebuilds that increment and verifies the same-event OMS Note.
    for f in out.foreign_fills.iter() {
        let ef = foreign_to_external(f);
        crate::process::external_fill(e, inst, drv, &ef, false, true)?;
    }
    // Decisions → Output + out ring.
    for d0 in out.decisions.iter() {
        // Every Output carries the seq of the Input that produced it (README "process" 9).
        let mut d1 = *d0;
        if d1.trigger_seq == 0 {
            d1.trigger_seq = e.cur_seq;
        }
        let d = &d1;
        let ix = e.out.push(RecordFlags(0), d);
        #[cfg(feature = "testkit")]
        e.audit.bind(d, ix);
        e.trace.rec("decision", u64::from(d.slot.0), d.action as u64);
        let ring = ring_of(e, d.account, d.venue);
        e.out.emits.push(Emit::Gw {
            ring,
            cmd: gw_of(d),
            rec: Some(ix),
        });
    }
    // Timers.
    for t in out.timers.iter() {
        let code = match t.kind {
            OmsTimerKind::UnknownQuery => 3u32,
            OmsTimerKind::HedgeRetry => 4,
            OmsTimerKind::SmokeTtl => 5,
        };
        let tid = code | (t.id << 8);
        e.heap.remove_key(3, tid, 0);
        if !t.cancel {
            e.heap
                .push(crate::timers::TimerEntry {
                    deadline: t.deadline_tsc.0,
                    owner: 3,
                    timer_id: tid,
                    group: 0,
                })
                .map_err(|_| FatalReason::TimerOverflow)?;
        }
        e.trace.rec("heap.oms", u64::from(tid), 0);
    }
    // Notes and latency samples; P0 OmsUnknown → UnknownTimeout cap.
    let unknown_code = hft_oms::notes::code_of(hft_oms::notes::UNKNOWN);
    for n in out.notes.iter() {
        e.out.push(RecordFlags(0), n);
        e.trace.rec("note.oms", u64::from(n.code), 0);
    }
    for l in out.latency.as_slice().iter() {
        e.out.push(RecordFlags(0), l);
    }
    // Ledger Notes are Outputs of this Input, not snapshot state. Do not leave them for a
    // later reconciliation: restoring a snapshot in between would lose those Outputs.
    crate::process::ledger_notes(e);
    for d in pnls.iter() {
        crate::process::pnl(e, inst, drv, *d)?;
    }
    for n in out.notes.iter() {
        if n.code == unknown_code && n.level == NoteLevel::P0 {
            let v = VenueId((n.v[0] & 0xFF) as u8);
            let a = AccountId(((n.v[0] >> 8) & 0xFF) as u8);
            let rec = crate::mode_glue::auto_rec(
                CapSource::UNKNOWN_TIMEOUT,
                hft_risk::account_venue_scope(a, v),
                e.cur_seq,
                0,
            );
            crate::mode_glue::apply_mode(e, inst, drv, ModeInput::Auto(rec))?;
        }
    }
    out.clear();
    Ok(())
}

/// PnL deltas of one `OmsOut` drain: at most one per fill and fee (`OmsOut::new` capacities).
struct PnlBuf {
    v: [Option<hft_ledger::PnlDelta>; PNL_CAP],
    n: usize,
}

const PNL_CAP: usize = 16 + 8;

impl PnlBuf {
    fn new() -> PnlBuf {
        PnlBuf {
            v: [None; PNL_CAP],
            n: 0,
        }
    }
    /// `false` (nothing stored) when full.
    fn push(&mut self, d: hft_ledger::PnlDelta) -> bool {
        match self.v.get_mut(self.n) {
            Some(x) => {
                *x = Some(d);
                self.n += 1;
                true
            }
            None => false,
        }
    }
    fn iter(&self) -> impl Iterator<Item = &hft_ledger::PnlDelta> {
        self.v.iter().take(self.n).flatten()
    }
}

fn foreign_to_external(f: &hft_oms::ForeignFill) -> hft_event::ExternalFill {
    hft_event::ExternalFill {
        venue: f.venue,
        account: f.account,
        side: f.side,
        cause: hft_event::ExternalCause::Unknown,
        inst: f.inst,
        qty: f.qty,
        px: f.px,
        fee: Qty(f.fee),
        fee_asset: f.fee_asset,
        _pad: [0; 6],
        exch_order_id: f.exch_order_id,
        trade_id: f.trade_id,
        ex_ts_ns: 0,
        rx_tsc: Tsc(0),
        fill_seq: if f.trade_id == 0 { f.cum_filled.0 as u64 } else { 0 },
        cash_qty: if f.cash.is_some() { f.qty } else { Qty(0) },
        cash_amount: f.cash.unwrap_or(Qty(0)),
    }
}

/// Unused helpers kept for the pipeline's callers.
#[allow(dead_code)]
pub(crate) fn cell_of(e: &Engine, a: AccountId, g: GroupId) -> CellKey {
    CellKey {
        strategy: e.rd.account(a).bound_instance,
        account: a,
        group: g,
    }
}

#[allow(dead_code)]
fn _gix<S: StrategySet>(inst: &Instances<S>, a: AccountId, g: GroupId) -> u16 {
    gix_for(inst, a, g)
}
