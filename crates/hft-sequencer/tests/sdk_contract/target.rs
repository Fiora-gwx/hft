//! Target intents (SDK-158–SDK-163).

use hft_event::{ControlCmd, EventKind, OrderDecision};
use hft_exec::decision::action;
use hft_sequencer::driver::live::RingId;
use hft_strategy_sdk::{
    BrokenReason, EngineOrigin, EventRef, ExecSpec, ExecUpdate, ExecUpdateKind, FirstLeg, IntentClass,
    IntentHandle, IntentOutcome, MakerParams, StuckReason, TargetIntent, TargetOrigin, TargetReason,
    TargetReject, TargetState,
};
use hft_types::{GroupId, InstId, KillLevel, Px, Qty, Rate, Side, StrategyId, Tif};

use super::*;
use crate::common::*;

// ------------------------------------------------------------------------------- helpers

/// The base target T of SDK-158 (G1, spot +0.01, perp −0.01, PairOnFill, band 0.001).
pub(crate) fn t_base() -> TargetIntent {
    let mut t = target(
        AC1,
        GT1,
        &[(0, q("0.01")), (1, q("-0.01"))],
        ExecSpec::PairOnFill(pair0()),
    );
    t.band = Rate(1_000_000);
    t
}

/// s1 issues `f(ctx)` on the next a1 Bbo; the outcome of its handle is read during s1's next
/// call (a second a1 Bbo). Returns the outcome and the records of both Inputs.
pub(crate) fn issue_with(
    h: &mut H,
    f: impl Fn(&mut hft_strategy_sdk::Ctx<'_>) -> Option<IntentHandle> + 'static,
) -> (Option<IntentOutcome>, Vec<hft_sequencer::MemRecord>) {
    let hc: Cap<IntentHandle> = cap();
    let oc: Cap<Option<IntentOutcome>> = cap();
    let fired: Cap<bool> = cap();
    let trig = h.upd + 1;
    {
        let (hc, oc, fired) = (hc.clone(), oc.clone(), fired.clone());
        set_hook(move |hx, ev, ctx| {
            if hx.s.strategy != S1 {
                return;
            }
            if !fired.borrow().is_empty() {
                if oc.borrow().is_empty() {
                    let o = hc.borrow().last().and_then(|x| ctx.outcome(*x));
                    oc.borrow_mut().push(o);
                }
                return;
            }
            if let EventRef::Bbo { inst, b, .. } = ev
                && *inst == A1
                && b.update_id == trig
            {
                fired.borrow_mut().push(true);
                if let Some(x) = f(ctx) {
                    hc.borrow_mut().push(x);
                }
            }
        });
    }
    let m = h.mark();
    h.quote(A1, "50000", "50001");
    h.run();
    h.quote(A1, "50000", "50001");
    h.run();
    clear_hook();
    assert!(!fired.borrow().is_empty(), "trigger Bbo not delivered");
    let o = oc.borrow().first().copied().flatten();
    (o, h.since(m))
}

/// s1 sets `t`; see `issue_with`.
pub(crate) fn issue(h: &mut H, t: TargetIntent) -> (Option<IntentOutcome>, Vec<hft_sequencer::MemRecord>) {
    issue_with(h, move |ctx| ctx.target(t).ok())
}

/// The bid/ask of the steady quotes of `i`.
fn steady_px(i: InstId, side: Side) -> Px {
    let (bid, ask) = if i == B1 {
        ("50010", "50011")
    } else if i == A1 {
        ("50000", "50001")
    } else {
        ("3000", "3000.5")
    };
    if side == Side::Buy { px(ask) } else { px(bid) }
}

/// s1 places one IOC per `(inst, side, qty)` of G1 in a single call (at the touch, class Open),
/// then every order is acked and filled; returns the decisions.
pub(crate) fn open_legs(h: &mut H, g: GroupId, legs: &[(InstId, Side, &str)]) -> Vec<OrderDecision> {
    let v: Vec<(InstId, Side, Qty)> = legs.iter().map(|(i, s, x)| (*i, *s, q(x))).collect();
    let trig = h.upd + 1;
    let v2 = v.clone();
    set_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
            && b.update_id == trig
        {
            for (i, s, x) in v2.iter() {
                let p = touch(hx, *i, *s);
                let _ = ctx.place(order_in(hx, g, *i, *s, Tif::Ioc, IntentClass::Open, p, *x));
            }
        }
    });
    let m = h.mark();
    h.quote(A1, "50000", "50001");
    h.run();
    clear_hook();
    let ds: Vec<OrderDecision> = decs(&h.since(m))
        .into_iter()
        .filter(|d| d.strategy == S1 && d.slot.0 != 0)
        .collect();
    assert_eq!(ds.len(), v.len(), "{ds:?}");
    for d in &ds {
        h.ack(d);
    }
    for d in &ds {
        h.fill(d, d.qty, d.qty, d.px);
    }
    ds
}

/// Net quantity of G-leg `i` (LegTracker).
pub(crate) fn net(h: &H, g: GroupId, i: usize) -> Qty {
    h.s.engine().oms.legs(AC1, g).view.leg[i].net
}

/// The target view of (acct-1, g).
pub(crate) fn tview(h: &H, g: GroupId) -> Option<hft_strategy_sdk::TargetView> {
    h.s.engine()
        .exec
        .cell(AC1, g)
        .filter(|c| c.has_target)
        .map(|c| c.view)
}

/// Captures every `Exec` update s1's fast part receives from now on.
pub(crate) fn capture_exec() -> Cap<(u16, ExecUpdate)> {
    let c: Cap<(u16, ExecUpdate)> = cap();
    let c2 = c.clone();
    add_hook(move |hx, ev, _ctx| {
        if let EventRef::Exec { gix, u } = ev
            && hx.s.strategy == S1
        {
            c2.borrow_mut().push((*gix, **u));
        }
    });
    c
}

/// Fresh a1/b1 quotes after `ms` of engine time.
pub(crate) fn tick(h: &mut H, ms: u64) {
    h.adv(ms * MS);
    h.quote(A1, "50000", "50001");
    h.quote(B1, "50010", "50011");
    h.run();
}

/// s1 places a pair first leg a1 buy `first` (OnFill b1 sell) in G1; the first leg fills in full,
/// the hedge fills `hedge` (and the rest expires when it is less than its quantity).
pub(crate) fn pair_g1(h: &mut H, first: &str, hedge: &str) {
    let fq = q(first);
    let trig = h.upd + 1;
    set_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
            && b.update_id == trig
        {
            let _ = ctx.place(pair_first(hx, fq));
        }
    });
    let m = h.mark();
    h.quote(A1, "50000", "50001");
    h.run();
    clear_hook();
    let d1 = decs(&h.since(m))
        .into_iter()
        .find(|d| d.inst == A1 && d.slot.0 != 0)
        .expect("first leg");
    h.ack(&d1);
    let m = h.mark();
    h.fill(&d1, d1.qty, d1.qty, d1.px);
    let d2 = decs(&h.since(m))
        .into_iter()
        .find(|d| d.inst == B1 && d.link_role == 2 && d.slot.0 != 0)
        .expect("second leg");
    h.ack(&d2);
    let hq = q(hedge);
    h.fill(&d2, hq, hq, steady_px(B1, Side::Sell));
    if hq.0 < d2.qty.0 {
        let mut ev = order_event(&d2, hft_event::OrderEventKind::Expired, h.epoch());
        ev.cum_filled = hq;
        ev.qty = Qty(0);
        h.push(RingId::Rep(0), &ev);
        h.run();
    }
}

/// G1 spot +0.02, perp −0.01 with the link Broken (`Deadline`): the hedge of a 0.02 first leg
/// fills 0.01 and expires; the hedge deadline passes.
pub(crate) fn broken_g1(h: &mut H) {
    pair_g1(h, "0.02", "0.01");
    tick(h, 3_100);
    assert!(
        matches!(tview_any(h, GT1), Some(TargetState::Broken(_))),
        "{:?}",
        tview_any(h, GT1)
    );
    assert_eq!(net(h, GT1, 0), q("0.02"));
    assert_eq!(net(h, GT1, 1), q("-0.01"));
}

/// The state of the exec cell of (acct-1, g) whether or not it has a target.
pub(crate) fn tview_any(h: &H, g: GroupId) -> Option<TargetState> {
    h.s.engine().exec.cell(AC1, g).map(|c| c.view.state)
}

/// A MarginReport of (acct-1, B) with one item (b1, `liq`).
fn margin_l3(h: &mut H, liq: &str) {
    let mut hd = hft_event::MarginReportHdr::new_zeroed();
    hd.venue = B;
    hd.account = AC1;
    hd.source = 1;
    hd.n = 1;
    hd.margin_balance = n("100000");
    hd.maint_margin = n("10");
    hd.avail_margin = n("99000");
    hd.rx_tsc = hft_types::Tsc(h.now());
    let it = hft_event::MarginItem {
        inst: B1,
        adl_quantile: 255,
        _pad: [0; 3],
        liq_px: px(liq),
    };
    let mut body = zerocopy::IntoBytes::as_bytes(&hd).to_vec();
    body.extend_from_slice(zerocopy::IntoBytes::as_bytes(&it));
    h.push_bytes(RingId::Rep(0), EventKind::MARGIN_REPORT, body);
    h.run();
}

/// G1 spot +0.01 / perp −0.01, then MarginStress L3 on B (engine target set).
fn l3_rig() -> H {
    let mut h = rig();
    open_legs(&mut h, GT1, &[(A1, Side::Buy, "0.01"), (B1, Side::Sell, "0.01")]);
    h.mark_md(B1, "50010");
    margin_l3(&mut h, "52000");
    let c = h.s.engine().exec.cell(AC1, GT1).expect("cell");
    assert!(
        matches!(c.view.origin, TargetOrigin::Engine(EngineOrigin::MarginStress)),
        "{:?}",
        c.view
    );
    h
}

use zerocopy::FromZeros;

fn rejected(o: Option<IntentOutcome>) -> Option<TargetReject> {
    match o {
        Some(IntentOutcome::TargetRejected(r)) => Some(r),
        _ => None,
    }
}

// ------------------------------------------------------------------------------- SDK-158

/// Rig with s1's G1 on three legs (a1, b1, a3).
fn rig_three_legs() -> H {
    let mut b = b_gt3();
    let g = group(GT1, BTC, &[leg(A1, A), leg(B1, B), leg(A3, A)]);
    b.params = vec![
        Spb::new(S1, AC1, TAKER, 1, vec![g, gspec_eth(GT2), gspec_fill(GT3)]),
        Spb::s2(1),
        Spb::s9(1),
    ];
    let mut h = b.boot();
    // a3 has no quote in `steady`; the FEEDS gate needs one on every leg.
    h.quote(A3, "50000", "50001");
    h.steady();
    clear_log();
    h
}

/// Rig with a1 lot 0.004 and b1 lot 0.01.
fn rig_lots() -> H {
    let mut b = b_gt3();
    for s in b.rd.insts.iter_mut() {
        if s.id == A1 {
            s.lot = 400_000;
            s.min_qty = Qty(400_000);
            s.lot_str = "0.004".into();
            s.min_qty_str = "0.004".into();
        }
        if s.id == B1 {
            s.lot = 1_000_000;
            s.min_qty = Qty(1_000_000);
            s.lot_str = "0.01".into();
            s.min_qty_str = "0.01".into();
        }
    }
    let mut h = b.boot();
    h.steady();
    clear_log();
    h
}

#[test]
fn sdk_158_target_rejects() {
    use TargetReject::*;
    let maker = |t: &mut TargetIntent| {
        t.exec = ExecSpec::MakerFirst(MakerParams {
            offset: r("0.0001"),
            requote: r("0.0001"),
            max_rest_ms: 1_000,
            _pad0: 0,
            chunk_notional: n("100"),
            hedge: pair0(),
        });
    };
    type Edit = fn(&mut TargetIntent);
    let simple: Vec<(u32, Edit, Option<TargetReject>)> = vec![
        (1, |_| {}, None),
        (2, |t| t.account = AC2, Some(NotOwner)),
        (3, |t| t.group = GroupId(0x0100_0009), Some(NotOwner)),
        (4, |t| t.legs[1].leg_ix = 2, Some(BadLegs)),
        (5, |t| t.legs[1].leg_ix = 0, Some(BadLegs)),
        (6, |t| t.legs[0].qty = q("-0.01"), Some(SpotShort)),
        (7, |t| t.legs[0].qty = Qty(0), None),
        (
            9,
            |t| {
                let mut p = pair0();
                p.first_leg = FirstLeg::Fixed;
                p.first_leg_ix = 5;
                t.exec = ExecSpec::PairOnFill(p);
            },
            Some(SpecInvalid),
        ),
        (10, maker, Some(AlgoUnsupported)),
    ];
    for (row, edit, want) in simple {
        let mut h = rig();
        let mut t = t_base();
        edit(&mut t);
        let (o, recs) = issue(&mut h, t);
        match want {
            None => {
                assert_eq!(o, Some(IntentOutcome::TargetAccepted), "SDK-158.{row}");
                if row == 1 {
                    assert!(
                        execs(&recs).iter().any(|d| d.action == action::TARGET_SET),
                        "SDK-158.1 TargetSet"
                    );
                }
            }
            Some(w) => assert_eq!(rejected(o), Some(w), "SDK-158.{row}: {o:?}"),
        }
    }
    // .8 n_legs = 3 on a three-leg group, PairOnFill.
    let mut h = rig_three_legs();
    let mut t = t_base();
    t.n_legs = 3;
    t.legs[2] = hft_strategy_sdk::LegTarget::new(2, Qty(0));
    let (o, _) = issue(&mut h, t);
    assert_eq!(rejected(o), Some(TooManyLegs), "SDK-158.8: {o:?}");
    // .11 G1 Broken.
    let mut h = rig();
    broken_g1(&mut h);
    let (o, _) = issue(&mut h, t_base());
    assert_eq!(rejected(o), Some(CellBroken), "SDK-158.11: {o:?}");
    // .12 / .13 MarginStress L3 forced reduction (engine target spot 0.008, perp −0.008).
    let mut h = l3_rig();
    let (o, _) = issue(&mut h, t_base());
    assert_eq!(rejected(o), Some(ForcedReduceActive), "SDK-158.12: {o:?}");
    let mut h = l3_rig();
    let mut t = t_base();
    t.legs[0].qty = q("0.005");
    t.legs[1].qty = q("-0.005");
    let (o, _) = issue(&mut h, t);
    assert_eq!(o, Some(IntentOutcome::TargetAccepted), "SDK-158.13");
    // .14 Kill{Account, flatten}, no Resume; T increases.
    let mut h = rig();
    let id = h.cmd_confirmed(
        AC1,
        &ControlCmd::Kill {
            scope: scope(StrategyId(0), AC1, GroupId(0)),
            level: KillLevel::Account,
            flatten: true,
            cancel_resting: true,
            hard: false,
        },
    );
    h.run();
    assert_eq!(h.res(id).0, 0, "Kill");
    let (o, _) = issue(&mut h, t_base());
    assert_eq!(rejected(o), Some(FlattenLocked), "SDK-158.14: {o:?}");
    // .15 lots 0.004 / 0.01 (pair lot 0.02 ≈ 1 000 USDT) > max_chunk_notional 500.
    let small = |t: &mut TargetIntent| {
        let mut p = pair0();
        p.slice.max_chunk_notional = n("500");
        t.exec = ExecSpec::PairOnFill(p);
        t.legs[0].qty = q("0.04");
        t.legs[1].qty = q("-0.04");
    };
    let mut h = rig_lots();
    let mut t = t_base();
    small(&mut t);
    let (o, _) = issue(&mut h, t);
    assert_eq!(rejected(o), Some(LotMismatch), "SDK-158.15: {o:?}");
    // .16 the same with reason Exit, reducing (G1 spot +0.04 / perp −0.04 → 0.02 / −0.02).
    let mut h = rig_lots();
    open_legs(&mut h, GT1, &[(A1, Side::Buy, "0.04"), (B1, Side::Sell, "0.04")]);
    let mut t = t_base();
    small(&mut t);
    t.reason = TargetReason::Exit;
    t.legs[0].qty = q("0.02");
    t.legs[1].qty = q("-0.02");
    let (o, _) = issue(&mut h, t);
    assert_eq!(o, Some(IntentOutcome::TargetAccepted), "SDK-158.16");
}

// ------------------------------------------------------------------------------- SDK-159

/// Rejects every new s1 order decision until G1's target is `Stuck(SliceFails)`.
fn stuck_slice_fails(h: &mut H, from: usize) {
    let mut m = from;
    for _ in 0..40 {
        tick(h, 400);
        let ds: Vec<OrderDecision> = decs(&h.since(m))
            .into_iter()
            .filter(|d| d.account == AC1 && d.slot.0 != 0 && d.action == hft_event::OrderAction::New)
            .collect();
        m = h.mark();
        for d in &ds {
            h.reject(d);
        }
        if tview(h, GT1).is_some_and(|v| v.state == TargetState::Stuck(StuckReason::SliceFails)) {
            return;
        }
    }
    panic!("no SliceFails: {:?}", tview(h, GT1));
}

fn rig_stuck() -> H {
    let mut h = rig();
    let m = h.mark();
    let (o, _) = issue(&mut h, t_base());
    assert_eq!(o, Some(IntentOutcome::TargetAccepted));
    stuck_slice_fails(&mut h, m);
    h
}

#[test]
fn sdk_159_identical_target_not_reset() {
    let stuck = TargetState::Stuck(StuckReason::SliceFails);
    // Step 1: the same T again.
    let mut h = rig_stuck();
    let (o, recs) = issue(&mut h, t_base());
    assert_eq!(o, Some(IntentOutcome::TargetAccepted));
    assert!(
        execs(&recs).iter().all(|d| d.action != action::TARGET_REPLACED),
        "{:?}",
        execs(&recs)
    );
    assert_eq!(tview(&h, GT1).map(|v| v.state), Some(stuck));
    // Step 2: tag 0x11, the rest equal.
    let mut t = t_base();
    t.tag = 0x11;
    let (o, recs) = issue(&mut h, t);
    assert_eq!(o, Some(IntentOutcome::TargetAccepted));
    assert!(execs(&recs).iter().any(|d| d.action == action::TARGET_REPLACED));
    assert_eq!(tview(&h, GT1).map(|v| v.state), Some(TargetState::Working));
    // Step 3 (another run): clear_target then target in one call.
    let mut h = rig_stuck();
    let (_, recs) = issue_with(&mut h, |ctx| {
        let _ = ctx.clear_target(AC1, GT1);
        ctx.target(t_base()).ok()
    });
    let acts: Vec<u8> = execs(&recs)
        .iter()
        .map(|d| d.action)
        .filter(|a| {
            [
                action::TARGET_CLEARED,
                action::TARGET_SET,
                action::TARGET_REPLACED,
            ]
            .contains(a)
        })
        .collect();
    assert_eq!(acts, vec![action::TARGET_CLEARED, action::TARGET_SET]);
    assert_eq!(tview(&h, GT1).map(|v| v.state), Some(TargetState::Working));
}

// ------------------------------------------------------------------------------- SDK-160

#[test]
fn sdk_160_engine_target_cancels_strategy_resting() {
    let mut h = rig();
    open_legs(&mut h, GT1, &[(A1, Side::Buy, "0.01"), (B1, Side::Sell, "0.01")]);
    // Resting on b1: a reduce-only GTC buy 0.01 (s1) and an Open GTC sell 0.001 (s2).
    let trig = h.upd + 1;
    set_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
            && b.update_id == trig
        {
            let mut ro = order_in(
                hx,
                GT1,
                B1,
                Side::Buy,
                Tif::Gtc,
                IntentClass::Reduce,
                px("49500"),
                q("0.01"),
            );
            ro.flags = hft_strategy_sdk::OrderIntent::REDUCE_ONLY;
            let _ = ctx.place(ro);
            let _ = ctx.place(order_in(
                hx,
                GT1,
                B1,
                Side::Sell,
                Tif::Gtc,
                IntentClass::Open,
                px("50500"),
                q("0.001"),
            ));
        }
    });
    let m = h.mark();
    h.quote(A1, "50000", "50001");
    h.run();
    clear_hook();
    let rest: Vec<OrderDecision> = decs(&h.since(m)).into_iter().filter(|d| d.slot.0 != 0).collect();
    assert_eq!(rest.len(), 2, "{rest:?}");
    for d in &rest {
        h.ack(d);
    }
    let (s1, s2) = (rest[0].slot, rest[1].slot);
    h.mark_md(B1, "50010");
    clear_log();
    let m = h.mark();
    margin_l3(&mut h, "52000");
    let ds = decisions(&h.since(m));
    let cancels: Vec<&(u64, OrderDecision)> = ds
        .iter()
        .filter(|d| d.1.action == hft_event::OrderAction::Cancel)
        .collect();
    let mut tgt: Vec<u32> = cancels.iter().map(|d| d.1.target_slot.0).collect();
    tgt.sort_unstable();
    let mut want = vec![s1.0, s2.0];
    want.sort_unstable();
    assert_eq!(tgt, want, "{ds:?}");
    assert!(
        cancels
            .iter()
            .all(|c| c.1.trigger_seq == cancels[0].1.trigger_seq)
    );
    let first_new = ds
        .iter()
        .find(|d| d.1.action == hft_event::OrderAction::New && d.1.slot.0 != 0)
        .expect("engine reduce order");
    assert!(cancels.iter().all(|c| c.0 < first_new.0), "{ds:?}");
    assert_eq!(first_new.1.trigger_seq, cancels[0].1.trigger_seq);
    for d in &rest {
        let ev = order_event(d, hft_event::OrderEventKind::CancelAck, h.epoch());
        h.push(RingId::Rep(0), &ev);
        h.run();
    }
    let n = fast(S1)
        .iter()
        .filter(|g| g.kind == "Order" && g.text.contains("Cancelled"))
        .count();
    assert_eq!(n, 2, "{:?}", fast(S1));
}

// ------------------------------------------------------------------------------- SDK-161

#[test]
fn sdk_161_broken_auto_fix_progress_done() {
    let mut b = b_gt3();
    b.setup_edit.push(|s| s.exec.default.broken_auto_fix_ms = 30_000);
    let mut h = b.boot();
    h.steady();
    clear_log();
    let ex = capture_exec();
    broken_g1(&mut h);
    assert!(
        ex.borrow()
            .iter()
            .any(|(g, u)| *g == 0 && u.kind == ExecUpdateKind::Broken(BrokenReason::LinkBroken)),
        "{:?}",
        ex.borrow()
    );
    let t0 = h.now();
    let autofix = u32::from(hft_exec::ExecOrderKind::AutoFix.code());
    let mut m = h.mark();
    let mut found = None;
    for _ in 0..35 {
        tick(&mut h, 1_000);
        let d = decs(&h.since(m))
            .into_iter()
            .find(|d| d.action == hft_event::OrderAction::New && d.reason == autofix);
        m = h.mark();
        if let Some(d) = d {
            found = Some((h.now() - t0, d));
            break;
        }
    }
    let (at, d) = found.expect("auto-fix order");
    assert!((30 * SEC..31 * SEC).contains(&at), "at {at}");
    assert_eq!((d.inst, d.side, d.qty), (A1, Side::Sell, q("0.01")), "{d:?}");
    assert_eq!(d.reject, 0);
    ex.borrow_mut().clear();
    let m = h.mark();
    h.ack(&d);
    h.fill(&d, d.qty, d.qty, d.px);
    let ups: Vec<ExecUpdate> = ex.borrow().iter().map(|x| x.1).collect();
    let p = ups
        .iter()
        .position(|u| {
            u.kind
                == ExecUpdateKind::Progress {
                    leg_ix: 0,
                    d: Qty(-1_000_000),
                }
                && u.origin == TargetOrigin::Engine(EngineOrigin::AutoFix)
        })
        .unwrap_or_else(|| panic!("{ups:?}"));
    let done = ups
        .iter()
        .position(|u| {
            u.kind
                == ExecUpdateKind::Done {
                    cleared_by_engine: true,
                }
        })
        .unwrap_or_else(|| panic!("{ups:?}"));
    assert!(p < done);
    assert_eq!(net(&h, GT1, 0), q("0.01"));
    assert_eq!(net(&h, GT1, 1), q("-0.01"));
    let code = hft_exec::decision::note_code(hft_exec::decision::note::EXEC_AUTO_FIX_DONE);
    assert!(h.notes_since(m).iter().any(|n| n.code == code));
}

// ------------------------------------------------------------------------------- SDK-162

/// Reads a view through s1's hook on the next a1 Bbo.
pub(crate) fn read_view<T: 'static>(h: &mut H, f: impl Fn(&Hx<'_>) -> T + 'static) -> T {
    let c: Cap<T> = cap();
    let c2 = c.clone();
    let trig = h.upd + 1;
    set_hook(move |hx, ev, _ctx| {
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
            && b.update_id == trig
        {
            c2.borrow_mut().push(f(hx));
        }
    });
    h.quote(A1, "50000", "50001");
    h.run();
    clear_hook();
    c.borrow_mut().pop().expect("view read")
}

#[test]
fn sdk_162_write_off_changes_legs_without_fill() {
    // a1 minimum order 0.001: the 0.0005 excess cannot be traded but is above the dust line.
    let mut b = b_gt3();
    for s in b.rd.insts.iter_mut() {
        if s.id == A1 {
            s.min_qty = q("0.001");
            s.min_qty_str = "0.001".into();
        }
    }
    let mut h = b.boot();
    h.steady();
    clear_log();
    // The hedge of a 0.0105 first leg is 0.01 (b1 lot 0.001); the 0.0005 excess is above the
    // repair threshold but below a1's minimum order, so repair runs out (`FixExhausted`).
    pair_g1(&mut h, "0.0105", "0.01");
    for _ in 0..12 {
        tick(&mut h, 1_000);
    }
    assert!(
        matches!(tview_any(&h, GT1), Some(TargetState::Broken(_))),
        "{:?}",
        tview_any(&h, GT1)
    );
    let snap = |hx: &Hx<'_>| {
        (
            hx.v.group_pos(AC1, GT1).legs[0].qty,
            hx.v.legs(AC1, GT1),
            hx.v.inventory(AC1, A, BTC).unassigned,
        )
    };
    let (q0, _, un0) = read_view(&mut h, snap);
    let ex = capture_exec();
    let m = h.mark();
    let id = h.cmd_confirmed(
        AC1,
        &ControlCmd::WriteOffResidual {
            account: AC1,
            _pad: [0; 3],
            group: GT1,
        },
    );
    h.run();
    assert_eq!(h.res(id).0, 0, "{:?}", h.res(id));
    let recs = h.since(m);
    assert!(decisions(&recs).is_empty(), "{:?}", decisions(&recs));
    assert!(
        ex.borrow().iter().any(|(g, u)| *g == 0
            && u.kind
                == ExecUpdateKind::Done {
                    cleared_by_engine: true
                }),
        "{:?}",
        ex.borrow()
    );
    let (q1, lv, un1) = read_view(&mut h, snap);
    assert_eq!(q0.0 - q1.0, 50_000);
    assert_eq!(lv.leg[0].residual, Qty(0));
    assert_eq!(lv.leg[1].residual, Qty(0));
    assert_eq!(un1.0 - un0.0, 50_000);
}

// ------------------------------------------------------------------------------- SDK-163

#[test]
fn sdk_163_manual_external_no_default() {
    let mut h = rig();
    open_legs(&mut h, GT1, &[(A1, Side::Buy, "0.01"), (B1, Side::Sell, "0.01")]);
    let got: Cap<(u16, hft_strategy_sdk::ExternalFillView)> = cap();
    let g2 = got.clone();
    add_hook(move |hx, ev, _ctx| {
        if let EventRef::External { gix, f } = ev
            && hx.s.strategy == S1
        {
            g2.borrow_mut().push((*gix, **f));
        }
    });
    let m = h.mark();
    let x = hft_event::ExternalFill {
        venue: A,
        account: AC1,
        side: Side::Sell,
        cause: hft_event::ExternalCause::Manual,
        inst: A1,
        qty: q("0.005"),
        px: px("50000"),
        fee: Qty(0),
        fee_asset: hft_types::AssetId(0),
        _pad: [0; 6],
        exch_order_id: 0,
        trade_id: 9_001,
        ex_ts_ns: 0,
        rx_tsc: hft_types::Tsc(h.now()),
        fill_seq: 0,
        cash_qty: hft_types::Qty(0),
        cash_amount: hft_types::Qty(0),
    };
    h.push(RingId::Rep(0), &x);
    h.run();
    for _ in 0..60 {
        tick(&mut h, 1_000);
    }
    let g = got.borrow().clone();
    assert_eq!(g.len(), 1, "{g:?}");
    assert_eq!(g[0].0, 0);
    assert_eq!(g[0].1.cause, hft_event::ExternalCause::Manual);
    assert_eq!(g[0].1.deadline_tsc, hft_types::Tsc(0));
    let ext = u32::from(hft_exec::ExecOrderKind::ExternalDefault.code());
    let recs = h.since(m);
    assert!(
        decisions(&recs).iter().all(|d| d.1.reason != ext),
        "{:?}",
        decisions(&recs)
    );
    assert!(execs(&recs).iter().all(|d| d.action != action::EXTERNAL_DEFAULT));
}

#[test]
fn seq_374_manual_hold_pair_tail_verify() {
    let mut h = rig();
    open_legs(&mut h, GT1, &[(A1, Side::Buy, "0.01"), (B1, Side::Sell, "0.01")]);
    let (outcome, _) = issue(&mut h, t_base());
    assert_eq!(outcome, Some(IntentOutcome::TargetAccepted));
    clear_hook();
    h.keyframe();
    let snapshot = h
        .j()
        .records()
        .filter(|r| r.kind == EventKind::STATE_SNAPSHOT)
        .last()
        .unwrap()
        .seq;
    let m = h.mark();
    for (trade_id, cause) in [
        (9_101, hft_event::ExternalCause::Manual),
        (9_102, hft_event::ExternalCause::Adl),
    ] {
        h.push(
            RingId::Rep(0),
            &hft_event::ExternalFill {
                venue: A,
                account: AC1,
                side: Side::Sell,
                cause,
                inst: A1,
                qty: q("0.002"),
                px: px("50000"),
                fee: Qty(0),
                fee_asset: hft_types::AssetId(0),
                _pad: [0; 6],
                exch_order_id: 0,
                trade_id,
                ex_ts_ns: 0,
                rx_tsc: hft_types::Tsc(h.now()),
                fill_seq: 0,
                cash_qty: hft_types::Qty(0),
                cash_amount: hft_types::Qty(0),
            },
        );
        h.run();
        for _ in 0..4 {
            tick(&mut h, 100);
        }
        let c = h.s.engine().exec.cell(AC1, GT1).unwrap();
        assert_ne!(c.hold & hft_exec::MANUAL_HOLD, 0);
    }
    let ds = decs(&h.since(m));
    assert!(
        ds.iter()
            .all(|d| d.action != hft_event::OrderAction::New || d.reject != 0),
        "manual hold emitted orders: {ds:?}"
    );
    let mismatch = h.verify_from(Some(snapshot));
    assert!(mismatch.is_empty(), "manual hold replay: {mismatch:?}");
}

#[test]
fn seq_397_rejected_force_backoff_replays_from_both_sides() {
    let mut b = b_gt3();
    b.rd.limits.retain(|(venue, _)| *venue != B);
    b.rd.limits.extend(order_limits(B, 1, 100));
    let mut risk = b0_risk(&b.rd.load());
    for venue in risk.venues.as_mut_slice() {
        if venue.venue == B {
            venue.rr_reserve = Rate(0);
        }
    }
    b.risk = Some(risk);
    let rd = b.rd.load();
    let setup = b.setup(&rd);
    let mut configs = b.configs(&rd);
    configs.engine_blob = setup.encode_blob(1, hft_types::HostClass::Dev2, UTC0);
    configs.risk_blob = configs.risk.encode_blob().unwrap();
    configs.risk_sha256 = sha(&configs.risk_blob);
    let heartbeat = hb();
    let driver = hft_sequencer::driver::live::LiveDriver::new(TestIo::<TestSet>::new(setup.clock), b.live);
    let s = hft_sequencer::Sequencer::boot(
        hft_sequencer::BootSource::Empty { configs },
        &setup,
        driver,
        heartbeat,
    )
    .unwrap();
    let mut h = H {
        s,
        rd,
        setup,
        cmd_no: 0,
        hb: heartbeat,
        upd: 1,
    };
    h.steady();
    open_legs(&mut h, GT1, &[(A1, Side::Buy, "0.01"), (B1, Side::Sell, "0.01")]);
    let before = h.keyframe().seq;
    let m = h.mark();
    let mut external =
        crate::process::ext(AC1, B1, Side::Sell, "0.005", hft_event::ExternalCause::Adl, 9_397);
    external.rx_tsc = hft_types::Tsc(h.now());
    h.push(RingId::Rep(0), &external);
    h.run();
    h.cmd_confirmed(
        AC1,
        &ControlCmd::Kill {
            scope: scope(StrategyId(0), AC1, GroupId(0)),
            level: KillLevel::Account,
            flatten: true,
            cancel_resting: true,
            hard: false,
        },
    );
    h.run();
    let during = h.keyframe().seq;
    let cell = h.s.engine().exec.cell(AC1, GT1).unwrap();
    assert_eq!(cell.gated_bits, hft_types::RejectBits::RATE);
    assert!(cell.wake_at.0 > h.now());
    assert_eq!(cell.fix.first_tsc, hft_types::Tsc(0));
    // The initial position consumed the sole venue token; the next wake is still rate-gated.
    tick(&mut h, 500);
    h.keyframe();
    let cell = h.s.engine().exec.cell(AC1, GT1).unwrap();
    assert_eq!(cell.fix.first_tsc, hft_types::Tsc(0));
    assert!(!matches!(cell.view.state, TargetState::Broken(_)));
    let rate_rejects = decs(&h.since(m))
        .into_iter()
        .filter(|d| d.account == AC1 && d.inst == B1 && d.reject & hft_types::RejectBits::RATE.0 != 0)
        .count();
    assert!(
        rate_rejects >= 2,
        "initial force and wake retry must both reach the rate gate"
    );
    for snapshot in [before, during] {
        let log = hft_sequencer::driver::replay::Log::from_segments(&h.j().segments);
        let setup = hft_sequencer::EngineSetup::from_snapshot(&log, snapshot).unwrap();
        let mismatches = verify_journal::<TestSet>(h.j(), &setup, Some(snapshot));
        assert!(
            mismatches.is_empty(),
            "backoff replay at {snapshot}: {mismatches:?}"
        );
    }
}

#[test]
fn seq_375_late_fill_resume_verify() {
    let mut h = rig();
    let first_epoch = h.epoch();
    let trig = h.upd + 1;
    set_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
            && b.update_id == trig
        {
            let _ = ctx.place(pair_first(hx, q("0.001")));
        }
    });
    let m = h.mark();
    h.quote(A1, "50000", "50001");
    h.run();
    clear_hook();
    let first = decs(&h.since(m))
        .into_iter()
        .find(|d| d.action == hft_event::OrderAction::New && d.reject == 0)
        .unwrap();
    h.keyframe();
    let mut h = h
        .crash_restart(
            plan(hft_sequencer::SnapshotSel::Latest, true, 2, 0),
            Rerun::default(),
        )
        .unwrap();
    h.bring_up(false);
    let (slot, _, _) =
        h.s.engine()
            .oms
            .raw_slot(first.venue, first.account, first.slot.0)
            .unwrap();
    assert_eq!(slot.state, hft_oms::OrderState::Unknown);
    let link = h.s.engine().oms.link(slot.link_id).unwrap();
    assert_ne!(link.flags & hft_oms::link_flags::RESUME_HOLD, 0);
    let snapshot = h.keyframe();
    let m = h.mark();
    let mut fill = order_event(&first, hft_event::OrderEventKind::Fill, first_epoch);
    fill.cum_filled = first.qty;
    h.push(h.rep_ring(first.account), &fill);
    h.run();
    let orders: Vec<_> = decs(&h.since(m))
        .into_iter()
        .filter(|d| d.action == hft_event::OrderAction::New && d.reject == 0 && d.group == GT1)
        .collect();
    assert_eq!(orders.len(), 1, "one first fill produced {orders:?}");
    assert_eq!(orders[0].qty, first.qty);
    assert_eq!(orders[0].link_role, hft_strategy_sdk::LinkRole::Second as u8);
    let mismatch = h.verify_from(Some(snapshot.seq));
    assert!(mismatch.is_empty(), "resume replay: {mismatch:?}");
}
