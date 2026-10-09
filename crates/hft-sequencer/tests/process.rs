//! The processing order of one Input (TESTS.md "每条 Input 的处理顺序": SEQ-121–SEQ-169).

use hft_event::{ControlCmd, EventKind, OrderDecision, RecordFlags, Role};
use hft_sequencer::driver::live::RingId;
use hft_strategy_sdk::{EventRef, ExecSpec};
use hft_types::{GroupId, KillLevel, Side, Tsc};

use crate::common::*;

fn steady() -> H {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    h
}

/// s1 places `f(hx)` on the next a1 Bbo.
fn s1_on_bbo(h: &mut H, f: fn(&Hx<'_>) -> Vec<hft_strategy_sdk::OrderIntent>) -> usize {
    set_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            for it in f(hx) {
                let _ = ctx.place(it);
            }
        }
    });
    let m = h.mark();
    h.quote(A1, "50000", "50003");
    h.run();
    clear_hook();
    m
}

#[test]
fn seq_121_append_before_effect() {
    let mut h = steady();
    h.io().j.faults.force_seq = Some(1_000);
    h.trace_on();
    let c: Cap<u64> = cap();
    let c2 = c.clone();
    set_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            c2.borrow_mut().push(hx.v.now().seq);
            let _ = ctx.place(open_buy(hx, A1));
        }
    });
    let m = h.mark();
    h.quote(A1, "50000", "50003");
    h.step();
    clear_hook();
    let tr = h.trace();
    let a = tr.iter().position(|x| *x == "append.input").unwrap();
    let t = tr.iter().position(|x| *x == "top.apply").unwrap();
    assert!(a < t);
    assert_eq!(c.borrow().last().copied(), Some(1_000));
    let d = decisions(&h.since(m));
    assert_eq!(d[0].1.trigger_seq, 1_000);
}

#[test]
fn seq_122_cmd_expiry() {
    for (dt, ok) in [(-1i64, true), (0, true), (1, false)] {
        let mut h = steady();
        let e = h.now() + 1_000;
        h.at((e as i64 + dt) as u64);
        let v0 = h.s.engine().risk.mode.ctl_state_version();
        let id = h.cmd_with("cli:test", 4, AC1, &ControlCmd::Kill {
            scope: scope(hft_types::StrategyId(0), AC1, GroupId(0)),
            level: KillLevel::Account,
            flatten: false,
            cancel_resting: true,
            hard: false,
        }, |x| x.expires_tsc = Tsc(e));
        let m = h.mark();
        h.run();
        let r = h.result_of(id).unwrap();
        if ok {
            assert_eq!(r.status, 0, "dt {dt}");
        } else {
            assert_eq!(r.status, 3);
            assert_eq!(h.s.engine().risk.mode.ctl_state_version(), v0);
            assert!(h.notes_since(m).iter().all(|n| n.code != hft_event::note_code(hft_event::NoteSource::Risk, 2)));
            assert!(!h.s.engine().cmd_dedup_has(id));
        }
    }
}

#[test]
fn seq_123_output_tsc_and_trigger_seq() {
    let mut h = steady();
    h.at(4_000_000);
    let m = s1_on_bbo(&mut h, |hx| vec![open_buy(hx, A1)]);
    let recs = h.since(m);
    let bbo = recs.iter().find(|r| r.kind == EventKind::BBO).unwrap();
    let outs: Vec<_> = recs.iter().skip_while(|r| r.seq != bbo.seq).skip(1).take_while(|r| r.role() != Role::Input).filter(|r| r.role() == Role::Output).collect();
    assert!(!outs.is_empty());
    assert!(outs.iter().all(|r| r.tsc == 4_000_000));
    let d = decisions(&recs);
    assert_eq!(d[0].1.trigger_seq, bbo.seq);
}

#[test]
fn seq_124_event_end_placement() {
    let mut h = steady();
    // .1 a Trade alone.
    let m = h.mark();
    h.push(RingId::Md(0), &trade(A1, 1));
    h.run();
    let r = h.since(m);
    let t = r.iter().find(|x| x.kind == EventKind::TRADE).unwrap();
    assert!(t.flags.0 & RecordFlags::EVENT_END.0 != 0);
    // .4 keyframe: the StateSnapshot carries EVENT_END.
    let kf = h.keyframe();
    assert!(kf.flags.0 & RecordFlags::EVENT_END.0 != 0);
    // .2 Bbo + OrderDecision: the decision carries it.
    let m = s1_on_bbo(&mut h, |hx| vec![open_buy(hx, A1)]);
    let r = h.since(m);
    let end = r.iter().skip_while(|x| x.kind != EventKind::BBO).find(|x| x.flags.0 & RecordFlags::EVENT_END.0 != 0).unwrap();
    // Every non-Meta record of the event up to `end`, none after it before the next Input.
    let after: Vec<_> = r.iter().skip_while(|x| x.seq <= end.seq).take_while(|x| x.role() != Role::Input).collect();
    assert!(after.iter().all(|x| x.role() == Role::Meta), "{:?}", after.iter().map(|x| x.kind).collect::<Vec<_>>());
    assert_ne!(end.role(), Role::Meta);
    assert_ne!(end.kind, EventKind::BBO);
}

#[test]
fn seq_125_publish_before_push() {
    let mut h = steady();
    h.io().push_committed.clear();
    let m = s1_on_bbo(&mut h, |hx| {
        let mut b = open_buy(hx, A1);
        b.qty = q("0.001");
        let mut s = intent(hx, B1, Side::Sell, hft_types::Tif::Ioc, hft_strategy_sdk::IntentClass::Open, hx.v.bbo(B1).bid_px, q("0.001"));
        s.group = G1;
        vec![b, s]
    });
    let recs = h.since(m);
    let ds = decisions(&recs);
    assert_eq!(ds.len(), 2);
    let last = ds[1].0;
    // committed_len covers the second decision at both pushes.
    let mut len = 0u64;
    for r in h.j().records() {
        len += 24 + r.body.len().div_ceil(8) as u64 * 8;
        if r.seq == last {
            break;
        }
    }
    let pc = h.io_ref().push_committed.clone();
    assert_eq!(pc.len(), 2);
    assert!(pc.iter().all(|c| *c >= len));
    let out = &h.io_ref().out;
    let n = out.len();
    assert_eq!(out[n - 2].1.slot, ds[0].1.slot);
    assert_eq!(out[n - 1].1.slot, ds[1].1.slot);
}

#[test]
fn seq_126_decided_tsc_all_ops() {
    let mut h = steady();
    h.io().measure = Some(8_888);
    let m = s1_on_bbo(&mut h, |hx| vec![open_buy(hx, A1)]);
    let d = decisions(&h.since(m));
    assert_eq!(d[0].1.decided_tsc, Tsc(8_888));
    let out = h.io_ref().out.last().unwrap().1;
    assert_eq!(out.decided_tsc, Tsc(8_888));
    let st = h.s.engine().oms.slot_stamps(hft_strategy_sdk::OrderRef {
        venue: d[0].1.venue,
        account: d[0].1.account,
        _pad: [0; 2],
        slot: d[0].1.slot.0,
        r#gen: d[0].1.r#gen,
    });
    assert_eq!(st.map(|s| s.decided_tsc), Some(8_888));
}

#[test]
fn seq_129_hedge_fast_path_first() {
    let mut h = steady();
    let m = s1_on_bbo(&mut h, |hx| vec![pair_first(hx, q("0.01"))]);
    let d = decisions(&h.since(m));
    let first: OrderDecision = d[0].1;
    eprintln!("first {first:?}");
    h.ack(&first);
    let n_out = h.io_ref().out.len();
    let m = h.mark();
    h.fill(&first, q("0.01"), q("0.01"), first.px);
    let recs = h.since(m);
    let outs: Vec<_> = recs.iter().filter(|r| r.role() == Role::Output).collect();
    let dbg: Vec<_> = outs.iter().map(|r| (r.kind, r.read::<hft_event::Note>().map(|n| (n.code, n.v)))).collect();
    assert_eq!(outs[0].kind, EventKind::ORDER_DECISION, "{dbg:?}");
    let sec = outs[0].read::<OrderDecision>().unwrap();
    assert_eq!(sec.inst, B1);
    assert_eq!(sec.side, Side::Sell);
    assert_eq!(sec.qty, q("0.010"));
    assert_eq!(sec.reject, 0, "{sec:?}");
    let pushed: Vec<_> = h.io_ref().out[n_out..].iter().filter(|o| o.0 == hft_sequencer::OutRing::Venue(B)).collect();
    assert_eq!(pushed.len(), 1);
}

#[test]
fn seq_131_update_routing() {
    let mut h = steady();
    let m = s1_on_bbo(&mut h, |hx| vec![open_buy(hx, A1)]);
    let d = decisions(&h.since(m))[0].1;
    clear_log();
    h.ack(&d);
    let got: Vec<Got> = log().into_iter().filter(|g| g.kind == "Order" && !g.model).collect();
    assert!(!got.is_empty());
    assert!(got.iter().all(|g| g.s == S1));
}

#[test]
fn seq_134_delivery_order() {
    let mut h = steady();
    for s in [S1, S2, S9] {
        set_subs(s, vec![(A1, 0, hft_strategy_sdk::EvMask::BBO, hft_strategy_sdk::Part::Fast)]);
    }
    clear_log();
    h.quote(A1, "50000", "50003");
    h.run();
    let order: Vec<_> = log().iter().filter(|g| g.kind == "Bbo" && !g.model).map(|g| g.s).collect();
    assert_eq!(order, vec![S1, S2, S9]);
    clear_log();
    h.cmd(hft_types::AccountId(0), &ControlCmd::Kill {
        scope: scope(hft_types::StrategyId(0), hft_types::AccountId(0), GroupId(0)),
        level: KillLevel::Global,
        flatten: false,
        cancel_resting: true,
        hard: false,
    });
    h.run();
    let order: Vec<_> = log().iter().filter(|g| g.kind == "Mode" && !g.model).map(|g| g.s).collect();
    assert_eq!(order, vec![S1, S2, S9]);
}

#[test]
fn seq_135_ctx_application() {
    let mut h = steady();
    h.trace_on();
    let m = h.mark();
    set_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            let _ = ctx.place(open_buy(hx, A1));
            let _ = ctx.timer_at(Tsc(hx.v.now().tsc.0 + 1_000_000_000), 5, G1);
            ctx.note(1, hft_event::NoteLevel::Info, G1, A1, [7, 0, 0, 0, 0, 0, 0, 0]);
            let _ = ctx.request_halt(G1, 3);
        }
    });
    h.quote(A1, "50000", "50003");
    h.run();
    clear_hook();
    let tr = h.trace();
    assert!(tr.contains(&"ctx.place"));
    assert!(tr.contains(&"heap.push"));
    assert!(tr.contains(&"ctx.request_halt"));
    assert!(h.s.engine().heap.sorted().iter().any(|e| e.owner == 17 && e.timer_id == 5 && e.group == G1.0));
    let notes = h.notes_since(m);
    assert!(notes.iter().any(|n| n.source == hft_event::NoteSource::Strategy && n.strategy == S1));
    assert!(notes.iter().any(|n| n.code == hft_event::note_code(hft_event::NoteSource::Risk, 2)));
}

#[test]
fn seq_138_advance_only_when_pending() {
    let mut h = steady();
    let t = target(AC2, G2, &[(0, q("0.01")), (1, q("-0.01"))], ExecSpec::PairOnFill(pair0()));
    h.set_target_s2(t);
    // advance runs exactly when a cell is pending after the Input's own effects.
    for k in 0..6u64 {
        let pending_before = h.s.engine().exec.has_pending();
        h.trace_on();
        match k % 3 {
            0 => h.quote(A2, "3000", if k % 2 == 0 { "3001" } else { "3002" }),
            1 => h.push(RingId::Md(0), &trade(A1, 77 + k)),
            _ => h.quote(A1, "50000", if k % 2 == 0 { "50004" } else { "50005" }),
        }
        h.step();
        let n = h.s.engine().trace.count("exec.advance");
        assert!(n <= 1);
        if k % 3 != 2 && !pending_before {
            assert_eq!(n, 0, "k {k}");
        }
    }
}

#[test]
fn seq_140_leg_summary_projection() {
    let mut h = steady();
    let m = s1_on_bbo(&mut h, |hx| vec![open_buy(hx, A1)]);
    let d = decisions(&h.since(m))[0].1;
    // In flight: not quiescent.
    let e = h.s.engine();
    let ls = hft_sequencer::views::leg_summary(&e.oms, &e.market, &e.rd, AC1, G1, Some(&gspec(G1)));
    assert!(!ls.quiescent);
    h.ack(&d);
    h.fill(&d, d.qty, d.qty, d.px);
    let e = h.s.engine();
    let ls = hft_sequencer::views::leg_summary(&e.oms, &e.market, &e.rd, AC1, G1, Some(&gspec(G1)));
    assert!(ls.quiescent);
    assert!(ls.naked_filled_notional.0 > 0);
}
