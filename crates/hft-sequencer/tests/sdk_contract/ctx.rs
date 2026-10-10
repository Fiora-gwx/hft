//! Ctx and order intents, engine side (SDK-116–SDK-118, SDK-147–SDK-149, SDK-151–SDK-153).

use std::cell::Cell;
use std::rc::Rc;

use hft_event::{ConnRole, ConnStateKind, ControlCmd, EventKind, NoteLevel, NoteSource, OrderAction};
use hft_sequencer::driver::live::RingId;
use hft_strategy_sdk::{EventRef, ExecSpec, IntentClass, IntentHandle, IntentOutcome, OrderRef, TimerId};
use hft_types::{
    AccountId, CapSource, GroupId, InstId, KillLevel, RejectBits, RunMode, Side, StrategyId, Tif, Tsc,
};
use zerocopy::FromBytes;

use super::views::{in_event, oref};
use super::*;
use crate::common::*;

// ------------------------------------------------------------------------------- notes

#[test]
fn sdk_116_strategy_note_record() {
    let mut h = rig();
    set_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { inst, gix, .. } = ev
            && *inst == A2
            && *gix == 1
            && hx.s.strategy == S1
        {
            ctx.note(1, NoteLevel::Info, GT2, A1, [5, 0, 0, 0, 0, 0, 0, 0]);
        }
    });
    let m = h.mark();
    h.quote(A2, "3000", "3000.5");
    h.run();
    clear_hook();
    let ns: Vec<_> = h
        .notes_since(m)
        .into_iter()
        .filter(|n| n.source == NoteSource::Strategy)
        .collect();
    assert_eq!(ns.len(), 1, "{ns:?}");
    let n = ns[0];
    assert_eq!(n.code, 0x0200_0001);
    assert_eq!(n.level, NoteLevel::Info);
    assert_eq!(n.strategy, S1);
    assert_eq!(n.group, GT2);
    assert_eq!(n.inst, A1);
    assert_eq!(n.v, [5, 0, 0, 0, 0, 0, 0, 0]);
}

// ------------------------------------------------------------------------------- StrategyCmd

/// `StrategyCmd{s1, acct-1, group = GT1}` with a `CmdView` payload (ver 1, `op`, `vals`).
fn strategy_cmd(reduce_only: bool, op: u8, vals: &[i64]) -> ControlCmd<'static> {
    let mut payload = [0u8; 256];
    payload[0] = 1;
    payload[1] = op;
    payload[2] = vals.len() as u8;
    for (k, v) in vals.iter().enumerate() {
        payload[8 + 8 * k..16 + 8 * k].copy_from_slice(&v.to_le_bytes());
    }
    ControlCmd::StrategyCmd {
        instance: S1,
        reduce_only: u8::from(reduce_only),
        len: (8 + 8 * vals.len()) as u16,
        group: GT1,
        payload,
    }
}

/// SDK-117/118: GT1 holds b1 short 0.02; on `Manual` s1 places h1 (a1 buy Open IOC) and h2 (b1
/// buy 0.01 Reduce IOC); the next event reads both outcomes. Returns (h1 decision, h2 decision,
/// outcomes, new GwCmds, the command's seq).
fn manual_run(
    reduce_only: bool,
    op: u8,
    vals: &[i64],
) -> (
    hft_event::OrderDecision,
    hft_event::OrderDecision,
    (Option<IntentOutcome>, Option<IntentOutcome>),
    usize,
    u64,
) {
    // GT1 is one-legged (b1 short only): the exposure gate is off so that h1 meets only the
    // reduce-only rule.
    let mut h = super::views::rig_risk(super::views::no_naked_gate);
    open_in(&mut h, S1, GT1, B1, Side::Sell, "0.02");
    let hs: Rc<Cell<Option<(IntentHandle, IntentHandle)>>> = Rc::new(Cell::new(None));
    let hs2 = hs.clone();
    set_hook(move |hx, ev, ctx| {
        if let EventRef::Manual { .. } = ev
            && hx.s.strategy == S1
        {
            let a = touch(hx, A1, Side::Buy);
            let h1 = ctx.place(order_in(
                hx,
                GT1,
                A1,
                Side::Buy,
                Tif::Ioc,
                IntentClass::Open,
                a,
                q("0.001"),
            ));
            let b = touch(hx, B1, Side::Buy);
            let h2 = ctx.place(order_in(
                hx,
                GT1,
                B1,
                Side::Buy,
                Tif::Ioc,
                IntentClass::Reduce,
                b,
                q("0.01"),
            ));
            if let (Ok(h1), Ok(h2)) = (h1, h2) {
                hs2.set(Some((h1, h2)));
            }
        }
    });
    let m = h.mark();
    let out0 = h.io_ref().out.len();
    let id = h.cmd_confirmed(AC1, &strategy_cmd(reduce_only, op, vals));
    h.run();
    clear_hook();
    assert_eq!(h.res(id).0, 0, "StrategyCmd applied");
    let recs = h.since(m);
    let cmd_seq = recs
        .iter()
        .find(|r| r.kind == EventKind::CONTROL_CMD)
        .map(|r| r.seq)
        .expect("command record");
    let ds: Vec<_> = decs(&recs).into_iter().filter(|d| d.strategy == S1).collect();
    assert_eq!(ds.len(), 2, "{ds:?}");
    let new_out = h.io_ref().out.len() - out0;
    let (h1, h2) = hs.get().expect("handles");
    // The next event reads both outcomes.
    let oc: Cap<(Option<IntentOutcome>, Option<IntentOutcome>)> = cap();
    let oc2 = oc.clone();
    set_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            oc2.borrow_mut().push((ctx.outcome(h1), ctx.outcome(h2)));
        }
    });
    h.quote(A1, "50000", "50001");
    h.run();
    clear_hook();
    let o = oc.borrow().first().copied().expect("outcomes");
    (ds[0], ds[1], o, new_out, cmd_seq)
}

#[test]
fn sdk_117_manual_reduce_only_rejects_open() {
    let (d1, d2, (o1, o2), new_out, cmd_seq) = manual_run(true, 2, &[]);
    assert_eq!((d1.inst, d2.inst), (A1, B1));
    assert_eq!(o1, Some(IntentOutcome::Rejected(RejectBits::CLOSE_ONLY)));
    assert!(matches!(o2, Some(IntentOutcome::Placed(_))), "{o2:?}");
    assert_eq!(d1.reject, RejectBits::CLOSE_ONLY.0);
    assert_eq!(d1.slot.0, 0);
    assert_eq!(d1.trigger_seq, cmd_seq);
    assert_eq!(d2.reject, 0, "{d2:?}");
    assert_eq!(new_out, 1, "only h2 leaves on a ring");
}

#[test]
fn sdk_118_manual_not_reduce_only_normal() {
    let (d1, d2, (o1, o2), new_out, _) = manual_run(false, 1, &[1]);
    assert_eq!((d1.reject, d2.reject), (0, 0), "{d1:?} {d2:?}");
    assert!(matches!(o1, Some(IntentOutcome::Placed(_))), "{o1:?}");
    assert!(matches!(o2, Some(IntentOutcome::Placed(_))), "{o2:?}");
    assert_eq!(new_out, 2);
}

// ------------------------------------------------------------------------------- Ctx capacity and order

#[test]
fn sdk_147_ctx_full_engine_side() {
    let mut h = rig();
    let res: Cap<Result<IntentHandle, hft_strategy_sdk::CtxFull>> = cap();
    let r2 = res.clone();
    set_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
            && b.ask_px == px("50004")
        {
            for k in 0..17u32 {
                let mut it = order_in(
                    hx,
                    GT1,
                    A1,
                    Side::Buy,
                    Tif::Ioc,
                    IntentClass::Open,
                    b.ask_px,
                    q("0.001"),
                );
                it.tag = 100 + k;
                r2.borrow_mut().push(ctx.place(it));
            }
        }
    });
    let m = h.mark();
    h.quote(A1, "50000", "50004");
    h.run();
    clear_hook();
    let rs = res.borrow().clone();
    assert_eq!(rs.len(), 17);
    assert!(rs[..16].iter().all(|r| r.is_ok()));
    assert_eq!(rs[16], Err(hft_strategy_sdk::CtxFull));
    let recs = h.since(m);
    let bbo_seq = recs
        .iter()
        .find(|r| r.kind == EventKind::BBO)
        .map(|r| r.seq)
        .expect("bbo");
    let ds: Vec<_> = decs(&recs)
        .into_iter()
        .filter(|d| d.strategy == S1 && d.action == OrderAction::New && d.trigger_seq == bbo_seq)
        .collect();
    assert_eq!(ds.len(), 16, "{ds:?}");
    let nos: Vec<u32> = ds.iter().map(|d| d.intent_no).collect();
    assert_eq!(nos, (0..16).collect::<Vec<u32>>());
    assert!(ds.iter().all(|d| d.reject == 0), "{ds:?}");
    // The tags of the placed orders are the first 16.
    let refs: Vec<OrderRef> = ds.iter().map(oref).collect();
    let tags = in_event(&mut h, move |hx| {
        refs.iter()
            .map(|r| hx.v.order(*r).map_or(0, |o| o.tag))
            .collect::<Vec<u32>>()
    });
    assert_eq!(tags, (100..116).collect::<Vec<u32>>());
}

/// s1 sets `t` on the next a1 Bbo with ask `ask`.
fn s1_target(h: &mut H, ask: &str, t: hft_strategy_sdk::TargetIntent) {
    let a = px(ask);
    set_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
            && b.ask_px == a
        {
            let _ = ctx.target(t);
        }
    });
    h.quote(A1, "50000", ask);
    h.run();
    clear_hook();
}

fn tgt(qty: &str, tag: u32) -> hft_strategy_sdk::TargetIntent {
    let mut t = target(AC1, GT2, &[(0, q(qty))], ExecSpec::TakerSlice(slice0()));
    t.tag = tag;
    t
}

const TARGET_SET: u8 = 1;
const TARGET_REPLACED: u8 = 2;
const TARGET_CLEARED: u8 = 3;

#[test]
fn sdk_148_ops_in_call_order() {
    // Step 1: cancel, place, clear_target, target.
    {
        let mut h = rig();
        set_hook(|hx, ev, ctx| {
            if let EventRef::Bbo { inst, b, .. } = ev
                && *inst == A1
                && hx.s.strategy == S1
                && b.ask_px == px("50002")
            {
                let _ = ctx.place(order_in(
                    hx,
                    GT1,
                    A1,
                    Side::Buy,
                    Tif::Gtc,
                    IntentClass::Open,
                    px("49900"),
                    q("0.001"),
                ));
            }
        });
        let m = h.mark();
        h.quote(A1, "50000", "50002");
        h.run();
        clear_hook();
        let gtc = decs(&h.since(m))
            .into_iter()
            .find(|d| d.strategy == S1 && d.tif == Tif::Gtc)
            .expect("gtc");
        h.ack(&gtc);
        s1_target(&mut h, "50003", tgt("0.01", 1));
        let r = oref(&gtc);
        let t1 = tgt("0.02", 2);
        set_hook(move |hx, ev, ctx| {
            if let EventRef::Bbo { inst, b, .. } = ev
                && *inst == A1
                && hx.s.strategy == S1
                && b.ask_px == px("50004")
            {
                let _ = ctx.cancel(r);
                let _ = ctx.place(order_in(
                    hx,
                    GT1,
                    A1,
                    Side::Buy,
                    Tif::Ioc,
                    IntentClass::Open,
                    b.ask_px,
                    q("0.001"),
                ));
                let _ = ctx.clear_target(AC1, GT2);
                let _ = ctx.target(t1);
            }
        });
        let m = h.mark();
        h.quote(A1, "50000", "50004");
        h.run();
        clear_hook();
        let recs = h.since(m);
        let pos = |f: &dyn Fn(&hft_sequencer::MemRecord) -> bool| recs.iter().position(f);
        let rd = |x: &hft_sequencer::MemRecord| {
            (x.kind == EventKind::ORDER_DECISION)
                .then(|| hft_sequencer::types::read_body::<hft_event::OrderDecision>(&x.body))
                .flatten()
        };
        let xd = |x: &hft_sequencer::MemRecord| {
            (x.kind == EventKind::EXEC_DECISION)
                .then(|| x.read::<hft_event::ExecDecision>())
                .flatten()
        };
        let p_cancel =
            pos(&|x| rd(x).is_some_and(|d| d.action == OrderAction::Cancel && d.target_slot == gtc.slot))
                .expect("cancel");
        let p_new = pos(&|x| {
            rd(x).is_some_and(|d| {
                d.action == OrderAction::New && d.strategy == S1 && d.inst == A1 && d.tif == Tif::Ioc
            })
        })
        .expect("new");
        let p_clear =
            pos(&|x| xd(x).is_some_and(|e| e.action == TARGET_CLEARED && e.group == GT2)).expect("cleared");
        let p_set = pos(&|x| xd(x).is_some_and(|e| e.action == TARGET_SET && e.group == GT2 && e.tag == 2))
            .expect("set");
        assert!(
            p_cancel < p_new && p_new < p_clear && p_clear < p_set,
            "{p_cancel} {p_new} {p_clear} {p_set}"
        );
        let tv = in_event(&mut h, |hx| hx.v.target(AC1, GT2)).expect("target");
        assert_eq!(tv.intent, t1);
    }
    // Step 2: target, then clear_target.
    {
        let mut h = rig();
        s1_target(&mut h, "50003", tgt("0.01", 1));
        let t1 = tgt("0.02", 2);
        set_hook(move |hx, ev, ctx| {
            if let EventRef::Bbo { inst, b, .. } = ev
                && *inst == A1
                && hx.s.strategy == S1
                && b.ask_px == px("50004")
            {
                let _ = ctx.target(t1);
                let _ = ctx.clear_target(AC1, GT2);
            }
        });
        let m = h.mark();
        h.quote(A1, "50000", "50004");
        h.run();
        clear_hook();
        let xs: Vec<u8> = execs(&h.since(m))
            .into_iter()
            .filter(|e| e.group == GT2 && matches!(e.action, TARGET_SET | TARGET_REPLACED | TARGET_CLEARED))
            .map(|e| e.action)
            .collect();
        assert_eq!(xs, vec![TARGET_REPLACED, TARGET_CLEARED]);
        assert_eq!(in_event(&mut h, |hx| hx.v.target(AC1, GT2)), None);
    }
}

#[test]
fn sdk_149_outcome_previous_call_only() {
    let mut h = rig();
    let seen: Cap<(u32, Option<IntentHandle>, Option<IntentOutcome>)> = cap();
    let s2 = seen.clone();
    let hd: Rc<Cell<Option<IntentHandle>>> = Rc::new(Cell::new(None));
    set_hook(move |hx, ev, ctx| {
        let EventRef::Bbo { inst, b, .. } = ev else { return };
        if *inst != A1 || hx.s.strategy != S1 {
            return;
        }
        if b.ask_px == px("50011") {
            let it = order_in(
                hx,
                GT1,
                A1,
                Side::Buy,
                Tif::Ioc,
                IntentClass::Open,
                b.ask_px,
                q("0.001"),
            );
            let x = ctx.place(it).ok();
            hd.set(x);
            s2.borrow_mut().push((1, x, None));
        } else if b.ask_px == px("50012") || b.ask_px == px("50013") {
            let k = if b.ask_px == px("50012") { 2 } else { 3 };
            let x = hd.get();
            s2.borrow_mut().push((k, x, x.and_then(|x| ctx.outcome(x))));
        }
    });
    let m = h.mark();
    h.quote(A1, "50000", "50011");
    h.run();
    let d = decs(&h.since(m))
        .into_iter()
        .find(|d| d.strategy == S1 && d.inst == A1)
        .expect("decision");
    h.quote(A1, "50000", "50012");
    h.run();
    h.quote(A1, "50000", "50013");
    h.run();
    clear_hook();
    let s = seen.borrow().clone();
    assert_eq!(s.len(), 3, "{s:?}");
    assert_eq!(s[0].1, Some(IntentHandle(0)));
    assert_eq!(s[1].2, Some(IntentOutcome::Placed(oref(&d))));
    assert_eq!(s[2].2, None);
}

// ------------------------------------------------------------------------------- gates

#[derive(Copy, Clone, Debug)]
enum Cond {
    Healthy,
    Silent2001,
    Quiet30sAlive,
    NoMargin,
    Killed,
}

/// s1 places the a1 buy Open IOC 0.01 with an OnFill b1 sell link (G1) after `cond`; returns the
/// outcome read in the next event.
fn link_first(cond: Cond) -> Option<IntentOutcome> {
    let mut h = rig();
    let t0 = h.now();
    match cond {
        Cond::Healthy => {}
        Cond::Silent2001 => h.at(t0 + 2_001 * MS),
        Cond::Quiet30sAlive => {
            for k in 1..=60u64 {
                h.at(t0 + k * 500 * MS);
                let t = h.now();
                h.push(
                    RingId::Rep(0),
                    &conn(
                        B,
                        0,
                        ConnStateKind::Alive,
                        ConnRole::Md,
                        InstId(0),
                        AccountId(0),
                        t,
                    ),
                );
                if k % 10 == 0 {
                    h.margin_report(AC1, "100000", "0", "100000");
                }
                h.run();
            }
        }
        Cond::NoMargin => {
            h.margin_report(AC1, "100000", "0", "0");
            h.run();
        }
        Cond::Killed => {
            let id = h.cmd_confirmed(
                AC1,
                &ControlCmd::Kill {
                    scope: scope(StrategyId(0), AC1, GroupId(0)),
                    level: KillLevel::Account,
                    flatten: false,
                    cancel_resting: true,
                    hard: false,
                },
            );
            h.run();
            assert_eq!(h.res(id).0, 0);
        }
    }
    let hd: Rc<Cell<Option<IntentHandle>>> = Rc::new(Cell::new(None));
    let oc: Cap<Option<IntentOutcome>> = cap();
    let (hd2, oc2) = (hd.clone(), oc.clone());
    set_hook(move |hx, ev, ctx| {
        let EventRef::Bbo { inst, b, .. } = ev else { return };
        if *inst != A1 || hx.s.strategy != S1 {
            return;
        }
        if b.ask_px == px("50021") {
            hd2.set(ctx.place(pair_first(hx, q("0.01"))).ok());
        } else if b.ask_px == px("50022") {
            oc2.borrow_mut().push(hd2.get().and_then(|x| ctx.outcome(x)));
        }
    });
    h.quote(A1, "50000", "50021");
    h.run();
    h.quote(A1, "50000", "50022");
    h.run();
    clear_hook();
    let first = oc.borrow().first().copied();
    first.flatten()
}

#[test]
fn sdk_151_gate_bits_link_checks() {
    let placed = |o: Option<IntentOutcome>| matches!(o, Some(IntentOutcome::Placed(_)));
    let o = link_first(Cond::Healthy);
    assert!(placed(o), "SDK-151.1 {o:?}");
    let o = link_first(Cond::Silent2001);
    assert_eq!(
        o,
        Some(IntentOutcome::Rejected(RejectBits::LINK_STALE)),
        "SDK-151.2"
    );
    let o = link_first(Cond::Quiet30sAlive);
    assert!(placed(o), "SDK-151.3 {o:?}");
    let o = link_first(Cond::NoMargin);
    assert_eq!(
        o,
        Some(IntentOutcome::Rejected(RejectBits::LINK_FUNDS)),
        "SDK-151.4"
    );
    let o = link_first(Cond::Killed);
    assert!(
        matches!(o, Some(IntentOutcome::Rejected(b)) if b.0 & RejectBits::KILLED.0 != 0),
        "SDK-151.5 {o:?}"
    );
}

// ------------------------------------------------------------------------------- halt and timers

#[test]
fn sdk_152_request_halt_cap() {
    let mut h = rig();
    set_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
            && b.ask_px == px("50005")
        {
            let _ = ctx.request_halt(GT2, 42);
        }
    });
    let m = h.mark();
    h.quote(A1, "50000", "50005");
    h.run();
    clear_hook();
    let modes = |from: usize| -> Vec<Got> {
        log()
            .into_iter()
            .skip(from)
            .filter(|g| g.kind == "Mode" && g.s == S1 && !g.model)
            .collect()
    };
    let ms = modes(0);
    assert_eq!(ms.len(), 1, "{ms:?}");
    assert_eq!(ms[0].gix, 1);
    assert!(
        ms[0].text.contains("old: Live") && ms[0].text.contains("new: CloseOnly"),
        "{}",
        ms[0].text
    );
    let md = in_event(&mut h, |hx| {
        (hx.v.mode(AC1, GT1), hx.v.mode(AC1, GT2), hx.v.mode(AC1, GT3))
    });
    assert_eq!(md, (RunMode::Live, RunMode::CloseOnly, RunMode::Live));
    // The Halted record carries the reason (CapSet note: v[0..6] = the 48-byte CapRecord).
    let cap_set = hft_risk::NOTE_CODES
        .iter()
        .find(|x| x.2 == "CapSet")
        .map(|x| ((x.0 as u32) << 24) | x.1)
        .expect("CapSet code");
    let rec = h
        .notes_since(m)
        .into_iter()
        .filter(|n| n.code == cap_set)
        .filter_map(|n| {
            let mut b = [0u8; 48];
            for (k, w) in n.v[..6].iter().enumerate() {
                b[8 * k..8 * k + 8].copy_from_slice(&w.to_le_bytes());
            }
            hft_event::CapRecord::read_from_bytes(&b).ok()
        })
        .find(|c| c.source == CapSource::HALTED)
        .expect("Halted CapSet");
    assert_eq!(rec.params[0], 42);
    assert_eq!(
        (rec.scope.strategy, rec.scope.account, rec.scope.group),
        (S1, AC1, GT2)
    );
    // Step 2: SetMode Live does not lift the cap.
    let n0 = log().len();
    let id = h.cmd_confirmed(
        AC1,
        &ControlCmd::SetMode {
            scope: scope(S1, AC1, GT2),
            mode: RunMode::Live,
        },
    );
    h.run();
    let _ = h.res(id);
    assert_eq!(in_event(&mut h, |hx| hx.v.mode(AC1, GT2)), RunMode::CloseOnly);
    assert!(modes(n0).is_empty(), "{:?}", modes(n0));
    // Step 3: Resume{Halted}.
    let n1 = log().len();
    let id = h.cmd_confirmed(
        AC1,
        &ControlCmd::Resume {
            scope: scope(S1, AC1, GT2),
            source: CapSource::HALTED,
        },
    );
    h.run();
    assert_eq!(h.res(id).0, 0);
    let ms = modes(n1);
    assert_eq!(ms.len(), 1, "{ms:?}");
    assert!(
        ms[0].text.contains("old: CloseOnly") && ms[0].text.contains("new: Live"),
        "{}",
        ms[0].text
    );
    assert_eq!(in_event(&mut h, |hx| hx.v.mode(AC1, GT2)), RunMode::Live);
}

#[test]
fn sdk_153_cancel_timer() {
    let mut h = rig();
    let id: Rc<Cell<Option<TimerId>>> = Rc::new(Cell::new(None));
    let id2 = id.clone();
    set_hook(move |hx, ev, ctx| {
        let EventRef::Bbo { inst, b, .. } = ev else { return };
        if *inst != A1 || hx.s.strategy != S1 {
            return;
        }
        if b.ask_px == px("50031") {
            let now = hx.v.now();
            id2.set(ctx.timer_at(Tsc(now.tsc.0 + 5_000 * MS), 9, GT1).ok());
        } else if b.ask_px == px("50032")
            && let Some(t) = id2.get()
        {
            ctx.cancel_timer(t);
        }
    });
    h.quote(A1, "50000", "50031");
    h.run();
    assert!(id.get().is_some());
    h.quote(A1, "50000", "50032");
    h.run();
    clear_hook();
    clear_log();
    let t0 = h.now();
    for k in 1..=100u64 {
        h.at(t0 + k * 100 * MS);
        h.run();
    }
    assert!(
        !fast(S1)
            .iter()
            .any(|g| g.kind == "Timer" && g.text.contains("tag: 9")),
        "{:?}",
        fast(S1)
    );
}
