//! The processing order of one Input (TESTS.md "每条 Input 的处理顺序": SEQ-121–SEQ-169).

use hft_event::{ControlCmd, EventKind, OrderDecision, RecordFlags, Role};
use hft_sequencer::driver::live::RingId;
use hft_strategy_sdk::{EventRef, ExecSpec};
use hft_types::{GroupId, KillLevel, Side, Tsc};
use hft_types::{Px, Qty};

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
        let id = h.cmd_with(
            "cli:test",
            4,
            AC1,
            &ControlCmd::Kill {
                scope: scope(hft_types::StrategyId(0), AC1, GroupId(0)),
                level: KillLevel::Account,
                flatten: false,
                cancel_resting: true,
                hard: false,
            },
            |x| x.expires_tsc = Tsc(e),
        );
        let m = h.mark();
        h.run();
        let r = h.result_of(id).unwrap();
        if ok {
            assert_eq!(r.status, 0, "dt {dt}");
        } else {
            assert_eq!(r.status, 3);
            assert_eq!(h.s.engine().risk.mode.ctl_state_version(), v0);
            assert!(
                h.notes_since(m)
                    .iter()
                    .all(|n| n.code != hft_event::note_code(hft_event::NoteSource::Risk, 2))
            );
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
    let outs: Vec<_> = recs
        .iter()
        .skip_while(|r| r.seq != bbo.seq)
        .skip(1)
        .take_while(|r| r.role() != Role::Input)
        .filter(|r| r.role() == Role::Output)
        .collect();
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
    let end = r
        .iter()
        .skip_while(|x| x.kind != EventKind::BBO)
        .find(|x| x.flags.0 & RecordFlags::EVENT_END.0 != 0)
        .unwrap();
    // Every non-Meta record of the event up to `end`, none after it before the next Input.
    let after: Vec<_> = r
        .iter()
        .skip_while(|x| x.seq <= end.seq)
        .take_while(|x| x.role() != Role::Input)
        .collect();
    assert!(
        after.iter().all(|x| x.role() == Role::Meta),
        "{:?}",
        after.iter().map(|x| x.kind).collect::<Vec<_>>()
    );
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
        let mut s = intent(
            hx,
            B1,
            Side::Sell,
            hft_types::Tif::Ioc,
            hft_strategy_sdk::IntentClass::Open,
            hx.v.bbo(B1).bid_px,
            q("0.001"),
        );
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
    let dbg: Vec<_> = outs
        .iter()
        .map(|r| (r.kind, r.read::<hft_event::Note>().map(|n| (n.code, n.v))))
        .collect();
    assert_eq!(outs[0].kind, EventKind::ORDER_DECISION, "{dbg:?}");
    let sec = outs[0].read::<OrderDecision>().unwrap();
    assert_eq!(sec.inst, B1);
    assert_eq!(sec.side, Side::Sell);
    assert_eq!(sec.qty, q("0.010"));
    assert_eq!(sec.reject, 0, "{sec:?}");
    let pushed: Vec<_> = h.io_ref().out[n_out..]
        .iter()
        .filter(|o| o.0 == hft_sequencer::OutRing::Venue(B))
        .collect();
    assert_eq!(pushed.len(), 1);
}

#[test]
fn seq_131_update_routing() {
    let mut h = steady();
    let m = s1_on_bbo(&mut h, |hx| vec![open_buy(hx, A1)]);
    let d = decisions(&h.since(m))[0].1;
    clear_log();
    h.ack(&d);
    let got: Vec<Got> = log()
        .into_iter()
        .filter(|g| g.kind == "Order" && !g.model)
        .collect();
    assert!(!got.is_empty());
    assert!(got.iter().all(|g| g.s == S1));
}

#[test]
fn seq_134_delivery_order() {
    let mut h = steady();
    for s in [S1, S2, S9] {
        set_subs(
            s,
            vec![(A1, 0, hft_strategy_sdk::EvMask::BBO, hft_strategy_sdk::Part::Fast)],
        );
    }
    clear_log();
    h.quote(A1, "50000", "50003");
    h.run();
    let order: Vec<_> = log()
        .iter()
        .filter(|g| g.kind == "Bbo" && !g.model)
        .map(|g| g.s)
        .collect();
    assert_eq!(order, vec![S1, S2, S9]);
    clear_log();
    h.cmd(
        hft_types::AccountId(0),
        &ControlCmd::Kill {
            scope: scope(hft_types::StrategyId(0), hft_types::AccountId(0), GroupId(0)),
            level: KillLevel::Global,
            flatten: false,
            cancel_resting: true,
            hard: false,
        },
    );
    h.run();
    let order: Vec<_> = log()
        .iter()
        .filter(|g| g.kind == "Mode" && !g.model)
        .map(|g| g.s)
        .collect();
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
    assert!(
        h.s.engine()
            .heap
            .sorted()
            .iter()
            .any(|e| e.owner == 17 && e.timer_id == 5 && e.group == G1.0)
    );
    let notes = h.notes_since(m);
    assert!(
        notes
            .iter()
            .any(|n| n.source == hft_event::NoteSource::Strategy && n.strategy == S1)
    );
    assert!(
        notes
            .iter()
            .any(|n| n.code == hft_event::note_code(hft_event::NoteSource::Risk, 2))
    );
}

#[test]
fn seq_138_advance_only_when_pending() {
    let mut h = steady();
    let t = target(
        AC2,
        G2,
        &[(0, q("0.01")), (1, q("-0.01"))],
        ExecSpec::PairOnFill(pair0()),
    );
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

fn mhdr(i: hft_types::InstId, rx: u64) -> hft_event::MarketHdr {
    let mut h = <hft_event::MarketHdr as zerocopy::FromZeros>::new_zeroed();
    h.venue = inst_venue(i);
    h.inst = i;
    h.rx_tsc = rx;
    h
}

#[test]
fn seq_142_bbo_steps() {
    let mut h = steady();
    let c: Cap<hft_strategy_sdk::BboView> = cap();
    let c2 = c.clone();
    set_hook(move |hx, ev, _| {
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            c2.borrow_mut().push(*b);
        }
    });
    h.trace_on();
    h.quote(A1, "50000", "50006");
    h.run();
    clear_hook();
    let tr = h.trace();
    let p = |n: &str| tr.iter().position(|x| *x == n).unwrap_or(usize::MAX);
    assert!(p("top.apply") < p("liveness"));
    assert!(p("liveness") < p("ledger.on_ref_px"));
    assert!(p("ledger.on_ref_px") < p("exec.touch_inst"));
    assert!(p("exec.touch_inst") < p("deliver.market"));
    let e = h.s.engine();
    assert_eq!(c.borrow().last().copied(), Some(e.market.bbo_view(&e.books, A1)));
}

#[test]
fn seq_143_stale_bbo_no_ref_px() {
    let mut h = steady();
    h.quote(A1, "50000", "50006");
    h.run();
    let top0 = *h.s.engine().books.top(A1);
    h.trace_on();
    let rx = h.now() + 9_000;
    let b = bbo(A1, A, 0, "49000", "49001", 1, rx);
    h.push(RingId::Bbo(0), &b);
    h.run();
    assert!(!h.trace().contains(&"ledger.on_ref_px"));
    assert_eq!(*h.s.engine().books.top(A1), top0);
    assert_eq!(h.s.engine().market.conns[usize::from(A.0)][0].live_tsc, rx);
}

#[test]
fn seq_147_market_last_updates() {
    let mut h = steady();
    set_subs(
        S2,
        vec![(
            B1,
            0,
            hft_strategy_sdk::EvMask(0xFF),
            hft_strategy_sdk::Part::Fast,
        )],
    );
    let mut p = Spb::s2(2);
    p.values[F_THR] = 9;
    let id = h.set_params(&p);
    h.run();
    assert_eq!(h.result_of(id).map(|r| r.status), Some(0));
    clear_log();
    let mut t = trade(B1, 1);
    t.px = px("50010");
    t.qty = q("0.5");
    t.h.ex_ts_ns = 123;
    h.push(RingId::Md(1), &t);
    let ix = hft_event::IndexPrice {
        h: mhdr(B1, 1),
        px: px("50005"),
    };
    h.push(RingId::Md(1), &ix);
    let lq = hft_event::Liquidation {
        h: mhdr(B1, 1),
        px: px("50000"),
        qty: q("1"),
        side: Side::Buy,
        _pad: [0; 7],
    };
    h.push(RingId::Md(1), &lq);
    h.run();
    let row = *h.s.engine().market.row(B1).unwrap();
    assert_eq!(row.trade_px, px("50010"));
    assert_eq!(row.trade_qty, q("0.5"));
    assert_eq!(row.index_px, px("50005"));
    let kinds: Vec<&str> = log()
        .iter()
        .filter(|g| g.s == S2 && !g.model)
        .map(|g| g.kind)
        .collect();
    assert!(
        kinds.contains(&"Trade") && kinds.contains(&"Index") && kinds.contains(&"Liquidation"),
        "{kinds:?}"
    );
    // .4 interval 0: unchanged, no ledger call.
    h.trace_on();
    let f = hft_event::Funding {
        h: mhdr(B1, 2),
        rate: r("0.0001"),
        next_funding_ns: UTC0 + 7 * 3_600_000_000_000,
        interval_s: 0,
        _pad: 0,
    };
    h.push(RingId::Md(1), &f);
    h.run();
    let row2 = *h.s.engine().market.row(B1).unwrap();
    assert_eq!(row2.funding_rate, r("0.0001"));
    assert_eq!(row2.interval_s, row.interval_s);
    assert!(!h.trace().contains(&"ledger.on_funding_interval"));
}

#[test]
fn seq_148_funding_interval_change() {
    let mut h = steady();
    let fund = |iv: u32, rx: u64| hft_event::Funding {
        h: mhdr(B1, rx),
        rate: r("0.0001"),
        next_funding_ns: UTC0 + 3_600_000_000_000,
        interval_s: iv,
        _pad: 0,
    };
    h.push(RingId::Md(1), &fund(28_800, 1));
    h.run();
    h.trace_on();
    h.push(RingId::Md(1), &fund(3_600, 2));
    h.run();
    let row = *h.s.engine().market.row(B1).unwrap();
    assert_eq!((row.prev_interval_s, row.interval_s), (28_800, 3_600));
    assert_eq!(h.s.engine().trace.count("ledger.on_funding_interval"), 1);
    h.trace_on();
    h.push(RingId::Md(1), &fund(3_600, 3));
    h.run();
    let row = *h.s.engine().market.row(B1).unwrap();
    assert_eq!((row.prev_interval_s, row.interval_s), (28_800, 3_600));
    assert_eq!(h.s.engine().trace.count("ledger.on_funding_interval"), 1);
    h.trace_on();
    h.push(RingId::Md(1), &fund(0, 4_000));
    h.run();
    assert_eq!(h.s.engine().trace.count("ledger.on_funding_interval"), 0);
    assert_eq!(h.s.engine().market.funding[B1.0 as usize].upd_tsc, Tsc(4_000));
}

pub fn ext(
    a: hft_types::AccountId,
    i: hft_types::InstId,
    side: Side,
    qty: &str,
    cause: hft_event::ExternalCause,
    tid: u64,
) -> hft_event::ExternalFill {
    hft_event::ExternalFill {
        venue: inst_venue(i),
        account: a,
        side,
        cause,
        inst: i,
        qty: q(qty),
        px: px("50000"),
        fee: hft_types::Qty(0),
        fee_asset: hft_types::AssetId(0),
        _pad: [0; 6],
        exch_order_id: 0,
        trade_id: tid,
        ex_ts_ns: 0,
        rx_tsc: Tsc(0),
        fill_seq: 0,
        cash_qty: hft_types::Qty(0),
        cash_amount: hft_types::Qty(0),
    }
}

#[test]
fn seq_152_external_fill_unattributed() {
    let mut h = steady();
    clear_log();
    h.trace_on();
    h.push(
        RingId::Rep(0),
        &ext(AC1, A2, Side::Sell, "0.5", hft_event::ExternalCause::Manual, 501),
    );
    h.run();
    assert_eq!(h.s.engine().trace.count("exec.on_external"), 1);
    let ex: Vec<Got> = log()
        .into_iter()
        .filter(|g| g.kind == "External" && !g.model)
        .collect();
    assert_eq!(ex.len(), 1);
    assert_eq!(ex[0].s, S1);
    assert_eq!(ex[0].gix, hft_strategy_sdk::GIX_NONE);
}

#[test]
fn seq_153_external_fill_duplicate() {
    let mut h = steady();
    h.push(
        RingId::Rep(0),
        &ext(AC1, A2, Side::Sell, "0.5", hft_event::ExternalCause::Manual, 77),
    );
    h.run();
    clear_log();
    h.trace_on();
    let m = h.mark();
    h.push(
        RingId::Rep(0),
        &ext(AC1, A2, Side::Sell, "0.5", hft_event::ExternalCause::Manual, 77),
    );
    h.run();
    assert_eq!(h.s.engine().trace.count("exec.on_external"), 0);
    assert_eq!(h.s.engine().trace.count("oms.external_adjust"), 0);
    assert!(log().iter().all(|g| g.kind != "External"));
    assert!(h.since(m).iter().all(|r| r.role() != Role::Output));
}

#[test]
fn seq_157_account_config_report() {
    reset_script();
    let mut h = b0().boot();
    h.step();
    h.trace_on();
    let l = hft_event::LeverageItem {
        inst: B1,
        _pad: 0,
        leverage: r("10"),
    };
    let hd = cfg_hdr(AC1, B, 1);
    let mut body = zerocopy::IntoBytes::as_bytes(&hd).to_vec();
    body.extend_from_slice(zerocopy::IntoBytes::as_bytes(&l));
    h.push_bytes(RingId::Rep(0), EventKind::ACCOUNT_CONFIG_REPORT, body);
    h.run();
    let tr = h.trace();
    let a = tr.iter().position(|x| *x == "risk.on_account_cfg").unwrap();
    let b = tr.iter().position(|x| *x == "risk.apply").unwrap();
    assert!(a < b);
    use hft_strategy_sdk::Views;
    assert_eq!(h.s.engine().seq_views().leverage(AC1, B1), Some(1000));
}

#[test]
fn seq_159_balance_position_reports() {
    let mut h = steady();
    h.trace_on();
    let br = hft_event::BalanceReport {
        venue: A,
        account: AC1,
        asset: USDT,
        _pad: 0,
        free: q("100000"),
        locked: hft_types::Qty(0),
        ex_ts_ns: 0,
        rx_tsc: Tsc(0),
    };
    h.push(RingId::Rep(0), &br);
    h.run();
    let ev =
        h.s.engine()
            .trace
            .ev
            .iter()
            .find(|e| e.what == "ledger.on_balance")
            .copied()
            .unwrap();
    assert_eq!(ev.b, 1);
    let pr = |qty: &str| hft_event::PositionReport {
        venue: B,
        account: AC1,
        _pad: 0,
        inst: B1,
        qty: q(qty),
        entry_px: hft_types::Px(0),
        upnl: hft_types::Notional(0),
        liq_px: hft_types::Px(0),
        ex_ts_ns: 0,
        rx_tsc: Tsc(0),
    };
    let m = h.mark();
    h.push(RingId::Rep(0), &pr("0"));
    h.run();
    assert!(h.since(m).iter().all(|r| r.role() != Role::Output));
    let m = h.mark();
    h.push(RingId::Rep(0), &pr("-0.012"));
    h.run();
    assert_eq!(h.notes_since(m).len(), 1);
    assert_eq!(h.s.engine().ledger.pos(AC1, B1).qty.0, 0);
}

#[test]
fn seq_130_net_adjust_same_event() {
    // .4 a perp sell with a quote fee: the ledger gets the absolute net and returns no adjust.
    let mut h = steady();
    h.trace_on();
    let d = h.open_position(AC1, B1, Side::Sell, "0.001");
    assert_eq!(d.side, Side::Sell);
    assert!(!h.trace().contains(&"oms.net_adjust"), "{:?}", h.trace());
    let l = h.s.engine().oms.legs(AC1, G1);
    assert_eq!(l.view.leg[1].net, hft_types::Qty(-q("0.001").0));
    assert_eq!(
        h.s.engine().ledger.pos(AC1, B1).qty,
        hft_types::Qty(-q("0.001").0)
    );
    // .3 a FeeOnly in the quote asset: no net adjustment.
    h.trace_on();
    let mut ev = order_event(&d, hft_event::OrderEventKind::FeeOnly, h.epoch());
    ev.qty = hft_types::Qty(0);
    ev.cum_filled = d.qty;
    ev.fee = hft_types::Qty(1_000);
    ev.fee_asset = USDT;
    let r = h.rep_ring(AC1);
    h.push(r, &ev);
    h.run();
    assert!(!h.trace().contains(&"oms.net_adjust"), "{:?}", h.trace());
    assert!(!h.trace().contains(&"exec.on_leg_adjust"), "{:?}", h.trace());
}

#[test]
fn seq_425_terminal_reports_and_base_fees_refresh_exposure_before_snapshot_replay() {
    use hft_event::OrderEventKind;

    for late_fee in [false, true] {
        reset_script();
        let mut b = b0();
        // Isolate the exposure gate from automatic residual trimming. The risk gate
        // remains 100 USDT; the executor treats this fixture's residuals as dust.
        b.setup_edit.push(|s| s.exec.default.dust_notional = n("1000"));
        let mut h = b.boot();
        h.steady();
        let d = h.order_at_touch(AC1, A1, Side::Buy, if late_fee { "0.004" } else { "0.010" });
        let filled = if late_fee { d.qty } else { q("0.001") };
        h.fill(&d, filled, filled, d.px);
        let before = h.keyframe().seq;
        h.trace_on();
        let what = if late_fee {
            OrderEventKind::FeeOnly
        } else {
            OrderEventKind::Expired
        };
        let mut report = order_event(&d, what, h.epoch());
        report.qty = Qty(0);
        report.cum_filled = filled;
        report.avg_px = d.px;
        if late_fee {
            report.fee = q("0.003");
            report.fee_asset = BTC;
        }
        h.push(h.rep_ring(AC1), &report);
        h.run();
        assert!(
            h.trace().contains(&"risk.on_legs"),
            "late_fee={late_fee}: {:?}",
            h.trace()
        );
        let legs = h.s.engine().oms.legs(AC1, G1);
        assert_eq!(legs.view.leg[0].net, q("0.001"));
        assert_eq!(legs.view.leg[0].inflight_immediate, Qty(0));
        let after = h.keyframe().seq;
        // Filled exposure is now about 50 USDT, below the fixture's 100 USDT gate.
        // Before the report it was about 500 (including pending) or 200 (before fee).
        h.order_at_touch(AC1, A1, Side::Buy, "0.001");
        assert!(h.verify().is_empty(), "full replay, late_fee={late_fee}");
        for at in [before, after] {
            assert!(
                h.verify_from(Some(at)).is_empty(),
                "late_fee={late_fee}, snapshot={at}"
            );
        }
    }
}

fn rref(d: &OrderDecision) -> hft_strategy_sdk::OrderRef {
    hft_strategy_sdk::OrderRef {
        venue: d.venue,
        account: d.account,
        _pad: [0; 2],
        slot: d.slot.0,
        r#gen: d.r#gen,
    }
}

/// Two resting GTC buys of s1 on a1 (acked).
fn two_resting(h: &mut H) -> (OrderDecision, OrderDecision) {
    let m = s1_on_bbo(h, |hx| {
        let b = hx.v.bbo(A1);
        let p = Px(b.bid_px.0 - 100);
        vec![
            intent(
                hx,
                A1,
                Side::Buy,
                hft_types::Tif::Gtc,
                hft_strategy_sdk::IntentClass::Open,
                p,
                q("0.001"),
            ),
            intent(
                hx,
                A1,
                Side::Buy,
                hft_types::Tif::Gtc,
                hft_strategy_sdk::IntentClass::Open,
                p,
                q("0.001"),
            ),
        ]
    });
    let ds = decisions(&h.since(m));
    assert_eq!(ds.len(), 2, "{ds:?}");
    h.ack(&ds[0].1);
    h.ack(&ds[1].1);
    (ds[0].1, ds[1].1)
}

#[test]
fn seq_127_trig_stamps_passed() {
    reset_script();
    let mut b0 = b0();
    for c in b0.rd.caps.iter_mut() {
        c.flags = c.flags | hft_refdata::CapFlags::AMEND;
    }
    let mut h = b0.boot();
    h.steady();
    let (h1, h2) = two_resting(&mut h);
    let (r1, r2) = (rref(&h1), rref(&h2));
    set_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            let _ = ctx.place(open_buy(hx, A1));
            let _ = ctx.cancel(r1);
            let _ = ctx.amend(r2, px("49990"), q("0.001"));
        }
    });
    let mut b = bbo(A1, A, 0, "50000", "50001", 9_000, 15);
    b.h.ex_ts_ns = 11;
    b.h.ex_send_ns = 12;
    b.h.hw_rx_ns = 13;
    b.h.sw_rx_ns = 14;
    b.h.rx_tsc = 15;
    let m = h.mark();
    h.push(RingId::Bbo(0), &b);
    h.run();
    clear_hook();
    let rec = h
        .since(m)
        .into_iter()
        .find(|r| r.kind == EventKind::BBO)
        .expect("bbo");
    let new = decisions(&h.since(m))
        .into_iter()
        .find(|d| d.1.action == hft_event::OrderAction::New)
        .expect("new")
        .1;
    let e = h.s.engine();
    for (r, op) in [
        (rref(&new), hft_oms::SlotStamps::OP_NEW),
        (r1, hft_oms::SlotStamps::OP_CANCEL),
        (r2, hft_oms::SlotStamps::OP_AMEND),
    ] {
        let st = e.oms.slot_stamps(r).copied().expect("stamps");
        assert_eq!(st.op, op);
        assert_eq!(
            (
                st.trig_ex_ts_ns,
                st.trig_ex_send_ns,
                st.trig_hw_rx_ns,
                st.trig_sw_rx_ns,
                st.trig_rx_tsc
            ),
            (11, 12, 13, 14, 15)
        );
        assert_eq!(st.trig_proc_tsc, rec.tsc);
        assert_eq!(st.trigger_seq, rec.seq);
        assert_eq!(st.trig_kind, EventKind::BBO.0);
    }
}

#[test]
fn seq_128_oms_out_drain_order() {
    // A linked first-leg fill on s1's g1: second leg, fill, fee, update, link, decision, timer, note.
    let mut h = steady();
    let m = s1_on_bbo(&mut h, |hx| vec![pair_first(hx, q("0.001"))]);
    let d = decisions(&h.since(m))
        .into_iter()
        .find(|x| x.1.inst == A1)
        .expect("first leg")
        .1;
    h.ack(&d);
    h.trace_on();
    let mut ev = order_event(&d, hft_event::OrderEventKind::Fill, h.epoch());
    ev.qty = d.qty;
    ev.cum_filled = d.qty;
    ev.px = d.px;
    ev.avg_px = d.px;
    ev.fee = Qty(1_000);
    ev.fee_asset = USDT;
    let r = h.rep_ring(AC1);
    h.push(r, &ev);
    h.run();
    let t = h.trace();
    // The hedge placement writes its own OMS note first; the drain's notes are the last ones.
    let pos = |n: &str| {
        if n == "note.oms" {
            t.iter().rposition(|x| *x == n)
        } else {
            t.iter().position(|x| *x == n)
        }
    };
    let order = [
        "hedge",
        "ledger.apply_fill",
        "ledger.apply_fee",
        "queue.order",
        "exec.on_link",
        "note.oms",
        "risk.on_pnl",
        "risk.on_legs",
    ];
    let idx: Vec<Option<usize>> = order.iter().map(|n| pos(n)).collect();
    for (k, w) in idx.windows(2).enumerate() {
        if let (Some(a), Some(b)) = (w[0], w[1]) {
            assert!(a < b, "{} before {}: {t:?}", order[k], order[k + 1]);
        }
    }
    assert!(idx[0].is_some() && idx[1].is_some() && idx[6].is_some(), "{t:?}");
    // risk.on_pnl after every OMS note of the drain.
    let last_note = t.iter().rposition(|x| *x == "note.oms").unwrap_or(0);
    assert!(
        idx[6].unwrap_or(0) > last_note
            || t.iter().position(|x| *x == "risk.on_pnl").unwrap_or(0)
                > t.iter().position(|x| *x == "exec.on_link").unwrap_or(0),
        "{t:?}"
    );
}

#[test]
fn seq_149_mark_price_to_margin() {
    let mut h = steady();
    h.open_position(AC1, B1, Side::Sell, "0.010");
    h.adv(7_000_000);
    h.trace_on();
    clear_log();
    h.mark_md(B1, "50100");
    let t = h.trace();
    let p = |n: &str| t.iter().position(|x| *x == n).unwrap_or(usize::MAX);
    assert!(p("ledger.on_mark") < p("risk.on_margin"), "{t:?}");
    assert!(p("risk.on_margin") < p("exec.touch_inst"), "{t:?}");
    let row = *h.s.engine().market.row(B1).expect("row");
    assert_eq!(row.mark_px, px("50100"));
    assert_eq!(row.mark_ens, h.s.engine().now.engine_ns);
    assert!(log().iter().any(|g| g.s == S2 && g.kind == "Mark") || !log().iter().any(|g| g.kind == "Mark"));
}

#[test]
fn seq_150_external_fill_attributed() {
    let mut h = steady();
    h.open_position(AC1, B1, Side::Sell, "0.010");
    h.trace_on();
    clear_log();
    let m = h.mark();
    let x = hft_event::ExternalFill {
        venue: B,
        account: AC1,
        side: Side::Buy,
        cause: hft_event::ExternalCause::Liquidation,
        inst: B1,
        qty: q("0.004"),
        px: px("50000"),
        fee: Qty(0),
        fee_asset: hft_types::AssetId(0),
        _pad: [0; 6],
        exch_order_id: 0,
        trade_id: 77,
        ex_ts_ns: 0,
        rx_tsc: Tsc(h.now()),
        fill_seq: 0,
        cash_qty: hft_types::Qty(0),
        cash_amount: hft_types::Qty(0),
    };
    h.push(RingId::Rep(0), &x);
    h.run();
    let t = h.trace();
    let p = |n: &str| t.iter().position(|x| *x == n).unwrap_or(usize::MAX);
    assert!(p("ledger.apply_external") < p("oms.external_adjust"), "{t:?}");
    assert!(p("oms.external_adjust") < p("exec.on_external"), "{t:?}");
    assert!(p("exec.on_external") < p("risk.on_pnl"), "{t:?}");
    assert!(
        p("risk.on_pnl") <= p("risk.on_legs") || p("risk.on_legs") == usize::MAX,
        "{t:?}"
    );
    assert_eq!(t.iter().filter(|x| **x == "oms.external_adjust").count(), 1);
    assert_eq!(t.iter().filter(|x| **x == "exec.on_external").count(), 1);
    let got: Vec<Got> = log()
        .into_iter()
        .filter(|g| g.kind == "External" && !g.model)
        .collect();
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0].gix, 0);
    let notes = h.notes_since(m);
    assert!(
        notes
            .iter()
            .any(|n| n.code == hft_ledger::notes::code::EXTERNAL_FILL && n.level == hft_event::NoteLevel::P0),
        "{notes:?}"
    );
}

#[test]
fn seq_156_margin_report() {
    let mut h = steady();
    h.open_position(AC1, B1, Side::Sell, "0.010");
    // .1 ledger.on_margin before risk.on_margin.
    h.trace_on();
    h.margin_report(AC1, "100000", "0", "100000");
    h.run();
    let t = h.trace();
    let p = |n: &str| t.iter().position(|x| *x == n).unwrap_or(usize::MAX);
    assert!(p("ledger.on_margin") < p("risk.on_margin"), "{t:?}");
    // .2 a ratio above the L1 threshold: MarginStress cap and its CapSet note.
    let m = h.mark();
    let mut hd = <hft_event::MarginReportHdr as zerocopy::FromZeros>::new_zeroed();
    hd.venue = B;
    hd.account = AC1;
    hd.source = 1;
    hd.margin_balance = n("100000");
    hd.maint_margin = n("90000");
    hd.avail_margin = n("10000");
    hd.margin_ratio = r("0.90");
    hd.rx_tsc = Tsc(h.now());
    h.push_bytes(
        RingId::Rep(0),
        EventKind::MARGIN_REPORT,
        zerocopy::IntoBytes::as_bytes(&hd).to_vec(),
    );
    h.run();
    let notes = h.notes_since(m);
    let cap_set = hft_risk::NOTE_CODES
        .iter()
        .find(|x| x.2 == "CapSet")
        .map(|x| ((x.0 as u32) << 24) | x.1)
        .unwrap();
    assert!(notes.iter().any(|n| n.code == cap_set), "{notes:?}");
}

#[test]
fn seq_372_foreign_fill_increments_not_held() {
    let mut h = steady();
    let ff = hft_oms::notes::code_of(hft_oms::notes::FOREIGN_FILL);
    let dedup = hft_oms::notes::code_of(hft_oms::notes::EXT_DEDUP);
    let mut booked = Vec::new();
    for cum in ["0.002", "0.005"] {
        let ev = hft_event::OrderEvent {
            venue: B,
            account: AC1,
            what: hft_event::OrderEventKind::Fill,
            detail: Side::Buy as u8,
            slot: hft_types::SlotId(0),
            r#gen: 0,
            inst: B1,
            px: px("50010"),
            qty: q(cum),
            cum_filled: q(cum),
            fee: Qty(0),
            fee_asset: USDT,
            epoch: h.epoch(),
            reject_code: 0,
            exch_order_id: 9,
            ex_ts_ns: 0,
            hw_rx_ns: 0,
            rx_tsc: 0,
            avg_px: px("50010"),
            cash_qty: hft_types::Qty(0),
            cash_sum: hft_types::Qty(0),
            trade_id: 0,
        };
        let m = h.mark();
        h.push(RingId::Rep(0), &ev);
        h.run();
        let notes = h.notes_since(m);
        assert!(
            !notes.iter().any(|n| n.code == dedup),
            "no OmsExtDedup: {notes:?}"
        );
        let n: Vec<_> = notes.iter().filter(|n| n.code == ff).collect();
        assert_eq!(n.len(), 1, "one OmsForeignFill per report");
        booked.push(n[0].v[1] as i64);
    }
    assert_eq!(booked, vec![q("0.002").0, q("0.003").0]);
    assert_eq!(h.s.engine().ledger.pos(AC1, B1).qty, q("0.005"));
}

#[test]
fn seq_382_ledger_notes_stay_in_originating_event_across_snapshots() {
    use hft_event::{Note, OrderEvent, OrderEventKind};
    use zerocopy::IntoBytes;

    for late_fee in [false, true] {
        let mut h = steady();
        let opening = h.open_position(AC1, A1, Side::Buy, "0.001");
        let before = h.keyframe().seq;
        let mut mark = h.mark();
        let closing = h.order_at_touch(AC1, A1, Side::Sell, if late_fee { "0.0009" } else { "0.001" });
        let mut fill = order_event(&closing, OrderEventKind::Fill, h.epoch());
        fill.cum_filled = closing.qty;
        // Asset quantity exponents come from refdata, independently of Notional's -6.
        fill.fee = Qty(hft_types::parse_exact(
            if late_fee { b"0" } else { b"0.01" },
            h.rd.asset(USDT).exp.qty_exp,
        )
        .unwrap());
        fill.fee_asset = USDT;
        h.push(h.rep_ring(AC1), &fill);
        h.run();
        let expected_kind = if late_fee {
            assert!(
                h.notes_since(mark)
                    .iter()
                    .all(|n| n.code != hft_ledger::notes::code::ROUND_CLOSED)
            );
            mark = h.mark();
            let mut fee = order_event(&opening, OrderEventKind::FeeOnly, h.epoch());
            fee.qty = Qty(0);
            fee.cum_filled = opening.qty;
            fee.fee = q("0.00001");
            fee.fee_asset = BTC;
            h.push(h.rep_ring(AC1), &fee);
            h.run();
            OrderEventKind::FeeOnly
        } else {
            OrderEventKind::Fill
        };
        let records = h.since(mark);
        let closed: Vec<_> = records
            .iter()
            .enumerate()
            .filter_map(|(ix, r)| {
                if r.kind != EventKind::NOTE {
                    return None;
                }
                let n = r.read::<Note>()?;
                (n.code == hft_ledger::notes::code::ROUND_CLOSED).then_some((ix, r, n))
            })
            .collect();
        assert_eq!(closed.len(), 1, "late_fee={late_fee}: {records:?}");
        let (note_ix, record, note) = &closed[0];
        assert_eq!((note.group, note.inst), (G1, A1));
        assert_eq!(note.v[2], q("0.001").0 as u64);
        assert_eq!(note.v[4], q(if late_fee { "0.0009" } else { "0.001" }).0 as u64);
        let expected_net = n(if late_fee { "-0.500905" } else { "-0.011" });
        assert_eq!(note.v[7] as i64, expected_net.0, "round includes its ending fee");
        assert_eq!(
            h.s.engine().ledger.group(AC1, G1).round.pnl,
            hft_ledger::PnlBreakdown::zero()
        );
        let input_ix = records[..*note_ix]
            .iter()
            .rposition(|r| r.role() == Role::Input)
            .unwrap();
        let input = &records[input_ix];
        assert_eq!(input.kind, EventKind::ORDER_EVENT);
        assert_eq!(input.read::<OrderEvent>().unwrap().what, expected_kind);
        assert!(
            records[input_ix..*note_ix]
                .iter()
                .all(|r| r.flags.0 & RecordFlags::EVENT_END.0 == 0)
        );
        assert_eq!(record.tsc, input.tsc);
        assert!(h.s.engine().ledger.notes().is_empty());

        let after = h.keyframe().seq;
        let mark = h.mark();
        h.push(RingId::Ctl, &recon_summary(AC1, A, 90_000, after));
        h.run();
        assert!(
            h.notes_since(mark)
                .iter()
                .all(|n| n.code != hft_ledger::notes::code::ROUND_CLOSED)
        );
        for at in [before, after] {
            assert!(
                h.verify_from(Some(at)).is_empty(),
                "late_fee={late_fee}, snapshot={at}"
            );
        }

        // An unchanged final ledger hash cannot excuse missing, duplicated or corrupt Outputs.
        for mutation in 0..3 {
            let mut bad = h.j().clone();
            let segment = bad
                .segments
                .iter_mut()
                .find(|s| s.records.iter().any(|r| r.seq == record.seq))
                .unwrap();
            let ix = segment.records.iter().position(|r| r.seq == record.seq).unwrap();
            match mutation {
                0 => {
                    segment.records.remove(ix);
                }
                1 => {
                    segment.records.insert(ix, (*record).clone());
                }
                _ => {
                    let mut altered = *note;
                    altered.v[2] += 1;
                    segment.records[ix].body = altered.as_bytes().to_vec();
                }
            }
            assert!(
                !verify_journal::<TestSet>(&bad, &h.setup, Some(before)).is_empty(),
                "mutation={mutation}"
            );
        }
    }
}

#[test]
fn seq_383_price_round_notes_are_journaled_before_snapshots() {
    use hft_event::{L2SnapHdr, Level, Note};
    use zerocopy::{FromZeros, IntoBytes};

    fn topn(h: &mut H, bid: &str, ask: &str, id: u64) {
        let mut hd = L2SnapHdr::new_zeroed();
        hd.h.venue = A;
        hd.h.inst = A1;
        hd.h.rx_tsc = h.now();
        hd.snap_id = id;
        hd.origin = L2SnapHdr::TOP_N;
        hd.n_bids = 1;
        hd.n_asks = 1;
        let mut body = hd.as_bytes().to_vec();
        for price in [bid, ask] {
            body.extend_from_slice(
                Level {
                    px: px(price),
                    qty: q("1"),
                }
                .as_bytes(),
            );
        }
        h.push_bytes(RingId::Md(0), EventKind::L2_SNAPSHOT, body);
        h.run();
    }

    for case in 0..3 {
        reset_script();
        let mut b = b0();
        b.setup_edit.push(|s| s.ledger.round_dust_notional = n("50"));
        if case == 2 {
            b.rd.insts.iter_mut().find(|i| i.id == A1).unwrap().l2_depth = 5;
        }
        let mut h = b.boot();
        if case == 2 {
            topn(&mut h, "50000", "50001", 1);
        }
        h.steady();
        let inst = if case == 1 { B1 } else { A1 };
        h.open_position(AC1, inst, Side::Buy, "0.001");
        if case == 1 {
            h.mark_md(inst, "50010");
        }
        assert_eq!(h.s.engine().ledger.group(AC1, G1).round.round_id, 1);
        let before = h.keyframe().seq;
        let m = h.mark();
        let expected_kind = match case {
            0 => {
                h.quote(A1, "49999", "50000");
                h.run();
                EventKind::BBO
            }
            1 => {
                h.mark_md(inst, "49999");
                EventKind::MARK_PRICE
            }
            _ => {
                topn(&mut h, "49999", "50000", 2);
                EventKind::L2_SNAPSHOT
            }
        };
        let records = h.since(m);
        assert!(records.iter().all(|r| r.kind != EventKind::ORDER_DECISION));
        let notes: Vec<_> = records
            .iter()
            .enumerate()
            .filter_map(|(ix, r)| {
                if r.kind != EventKind::NOTE {
                    return None;
                }
                let note = r.read::<Note>()?;
                (note.code == hft_ledger::notes::code::ROUND_CLOSED).then_some((ix, r, note))
            })
            .collect();
        assert_eq!(notes.len(), 1, "price case={case}");
        let (ix, record, note) = &notes[0];
        assert_eq!(
            (note.group, note.inst, note.v[2], note.v[4]),
            (G1, inst, q("0.001").0 as u64, 0)
        );
        let input_ix = records[..*ix]
            .iter()
            .rposition(|r| r.role() == Role::Input)
            .unwrap();
        assert_eq!(records[input_ix].kind, expected_kind);
        assert_eq!(record.tsc, records[input_ix].tsc);
        assert!(
            records[input_ix..*ix]
                .iter()
                .all(|r| r.flags.0 & RecordFlags::EVENT_END.0 == 0)
        );
        assert!(h.s.engine().ledger.notes().is_empty());
        assert_eq!(h.s.engine().ledger.pos(AC1, inst).qty, q("0.001"));
        assert_eq!(
            h.s.engine().ledger.group(AC1, G1).round.pnl,
            hft_ledger::PnlBreakdown::zero()
        );
        let after = h.keyframe().seq;
        let m = h.mark();
        h.push(
            RingId::Ctl,
            &recon_summary(AC1, inst_venue(inst), 90_100 + case, after),
        );
        h.run();
        assert!(
            h.notes_since(m)
                .iter()
                .all(|n| n.code != hft_ledger::notes::code::ROUND_CLOSED)
        );
        for at in [before, after] {
            assert!(
                h.verify_from(Some(at)).is_empty(),
                "price case={case}, snapshot={at}"
            );
        }
    }
}

// ===================================================================================================
// SEQ-341: Broken auto-clear conditions, L3 on a Broken group (helpers: tests/common/risk_kit.rs).
// ===================================================================================================

mod broken_clear {
    use hft_event::{ControlCmd, OrderDecision, OrderFlags};
    use hft_sequencer::codes::status;
    use hft_strategy_sdk::LinkRole;
    use hft_types::{CapSource, Side};

    use crate::common::risk_kit::*;
    use crate::common::*;

    fn steady() -> H {
        reset_script();
        let mut h = b0().boot();
        h.steady();
        h
    }

    /// g2 Broken by an exhausted link (a1 +0.01 filled, every b1 hedge expired): `Broken{2, g2}`
    /// and `HedgeFailure{2}`.
    fn broken_link() -> H {
        let mut h = steady();
        break_link(&mut h, S2, q("0.01"));
        assert!(cap_on(&h, CapSource::BROKEN, AC2, Some(G2)));
        assert!(cap_on(&h, CapSource::HEDGE_FAILURE, AC2, None));
        h
    }

    fn hedge_fix(h: &mut H) -> OrderDecision {
        let m = h.mark();
        let r = run_cmd(
            h,
            AC2,
            &ControlCmd::HedgeFix {
                scope: scope(S2, AC2, G2),
            },
        );
        assert_eq!(r.0, status::APPLIED, "{r:?}");
        decs_since(h, m)
            .into_iter()
            .find(|d| d.inst == A1 && d.account == AC2)
            .expect("HedgeFix order")
    }

    fn broken(h: &H) -> bool {
        cap_on(h, CapSource::BROKEN, AC2, Some(G2))
    }

    fn cleared_broken(h: &H, m: usize) -> bool {
        cap_notes(h, m, false)
            .iter()
            .any(|c| c.source == CapSource::BROKEN && c.scope.account == AC2 && c.scope.group == G2)
    }

    #[test]
    fn seq_341_broken_auto_clear_conditions() {
        // .1 the bounded auto-close sells the excess a1 at t_b + broken_auto_fix_ms; its fill
        // takes the filled naked to 0 with the group quiet → AutoClear{Broken}, HedgeFailure kept.
        let mut h = broken_link();
        let t_b = h.now();
        let m = h.mark();
        alive_to(&mut h, t_b + 121 * SEC, 500 * MS);
        let af: Vec<_> = decs_since(&h, m)
            .into_iter()
            .filter(|d| d.reason == REASON_AUTO_FIX)
            .collect();
        assert_eq!(af.len(), 1, "{af:?}");
        let x = af[0];
        assert_eq!(
            (x.inst, x.side, x.qty, x.reject),
            (A1, Side::Sell, q("0.01"), 0),
            "{x:?}"
        );
        assert!(is_role(&x, LinkRole::HedgeFix));
        h.ack(&x);
        assert!(broken(&h), "SEQ-341.3: in flight, not cleared");
        let m = h.mark();
        h.fill(&x, x.qty, x.qty, x.px);
        assert!(
            !broken(&h) && cleared_broken(&h, m),
            "SEQ-341.1 AutoClear{{Broken}}"
        );
        assert!(
            cap_on(&h, CapSource::HEDGE_FAILURE, AC2, None),
            "SEQ-341.1 HedgeFailure kept"
        );

        // .2 HedgeFix to the dust line, group quiet → AutoClear; .3 before its fill → kept.
        let mut h = broken_link();
        let x = hedge_fix(&mut h);
        assert_eq!(
            (x.side, x.qty, x.reason),
            (Side::Sell, q("0.01"), REASON_CMD_HEDGE_FIX)
        );
        h.ack(&x);
        assert!(broken(&h), "SEQ-341.3");
        let m = h.mark();
        h.fill(&x, x.qty, x.qty, x.px);
        assert!(!broken(&h) && cleared_broken(&h, m), "SEQ-341.2");
        assert!(
            cap_on(&h, CapSource::HEDGE_FAILURE, AC2, None),
            "SEQ-341.2 HedgeFailure kept"
        );

        // .4 the closing IOC ends with zero fill → still Broken.
        let mut h = broken_link();
        let x = hedge_fix(&mut h);
        h.ack(&x);
        let m = h.mark();
        expire_order(&mut h, &x);
        assert!(broken(&h) && !cleared_broken(&h, m), "SEQ-341.4");

        // .5 L3 ForceDeleverage on a Broken g2 holding a1 +0.010 / b1 −0.005: a reduce-only perp
        // buy goes out (perp first).
        let mut h = steady();
        broken_by_release(
            &mut h,
            S2,
            G2,
            &[(A1, Side::Buy, "0.010"), (B1, Side::Sell, "0.005")],
        );
        h.mark_md(B1, "50000");
        let n = h.io_ref().out.len();
        let m = h.mark();
        // liq_px 54 500 for the short b1 at mark 50 000: liq_dist 0.09 ≤ liq_dist_force 0.10.
        margin_with(&mut h, AC2, "0.01", Some("54500"));
        let fs: Vec<_> = exec_decs_since(&h, m)
            .into_iter()
            .filter(|x| x.action == hft_exec::decision::action::FORCE_STEP)
            .collect();
        assert!(!fs.is_empty(), "ForceStep");
        let ds = decs_since(&h, m);
        let p = ds
            .iter()
            .find(|d| d.inst == B1 && d.account == AC2)
            .unwrap_or_else(|| panic!("perp reduction: {ds:?}"));
        assert_eq!((p.side, p.reject), (Side::Buy, 0), "{p:?}");
        assert!(p.oflags & OrderFlags::REDUCE_ONLY.0 != 0);
        assert!(ds.iter().all(|d| d.inst != A1), "perp first: {ds:?}");
        assert!(
            h.io_ref().out[n..]
                .iter()
                .any(|f| f.1.inst == B1 && f.1.account == AC2),
            "SEQ-341.5 out"
        );
        assert!(broken(&h));
    }
}

// ------------------------------------------------------------------------------- ExecOut drain

/// The trace split per Input (each chunk starts at its `append.input`).
fn per_input(h: &H) -> Vec<Vec<hft_sequencer::trace::TraceEv>> {
    let mut v: Vec<Vec<hft_sequencer::trace::TraceEv>> = Vec::new();
    for e in h.s.engine().trace.ev.iter() {
        if e.what == "append.input" || v.is_empty() {
            v.push(Vec::new());
        }
        if let Some(c) = v.last_mut() {
            c.push(*e);
        }
    }
    v
}

fn pos_of(c: &[hft_sequencer::trace::TraceEv], what: &str) -> Option<usize> {
    c.iter().position(|e| e.what == what)
}

/// An `ExecOut` with one output of every kind (account 2, g2).
fn one_of_each(h: &H) -> hft_exec::ExecOut {
    use zerocopy::FromZeros;
    let mut xo = hft_exec::ExecOut::new();
    let mut d = hft_event::ExecDecision::new_zeroed();
    d.group = G2;
    d.owner = S2;
    d.account = AC2;
    d.action = hft_exec::decision::action::TARGET_SET;
    d.leg_ix = 0xFF;
    d.first_leg_ix = 0xFF;
    xo.decisions.push(d);
    xo.cancel_resting.push(hft_exec::CancelResting {
        account: AC2,
        inst: B1,
    });
    let b = h.s.engine().market.bbo_view(&h.s.engine().books, A1);
    let it = hft_strategy_sdk::OrderIntent {
        venue: A,
        account: AC2,
        side: Side::Buy,
        tif: hft_types::Tif::Ioc,
        class: hft_strategy_sdk::IntentClass::Open,
        flags: 0,
        _pad: [0; 2],
        inst: A1,
        group: G2,
        px: b.ask_px,
        qty: q("0.001"),
        link: hft_strategy_sdk::LegLinkOpt::NONE,
        tag: 0,
        max_rest_ms: 0,
    };
    xo.orders.push(hft_exec::ExecOrder {
        id: hft_exec::ExecOrderId { cix: 0, n: 1 },
        intent: it,
        origin: hft_exec::ExecOrigin::Engine,
        kind: hft_exec::ExecOrderKind::Slice,
        reason: hft_strategy_sdk::TargetReason::Rebalance,
        hedge_cap_qty: Qty(0),
    });
    xo.cancels.push(hft_strategy_sdk::OrderRef::new(B, AC2, 15, 0));
    xo.broken.push(hft_exec::BrokenReq {
        account: AC2,
        group: G2,
    });
    xo.timers.push(hft_exec::ExecTimerReq {
        cancel: false,
        timer_id: hft_exec::ExecTimerReq::id(hft_exec::ExecTimerKind::Wake, AC2),
        group: G2,
        at: Tsc(h.now() + 60 * SEC),
    });
    xo.notify.push(hft_exec::ExecNotify {
        account: AC2,
        owner: S2,
        u: hft_strategy_sdk::ExecUpdate {
            group: G2,
            tag: 0,
            origin: hft_strategy_sdk::TargetOrigin::Strategy,
            kind: hft_strategy_sdk::ExecUpdateKind::Done {
                cleared_by_engine: false,
            },
            target: [Qty(0); 4],
            filled: [Qty(0); 4],
            inflight: [Qty(0); 4],
        },
    });
    xo.notes.push(hft_event::Note {
        code: hft_event::note_code(hft_event::NoteSource::Exec, 7),
        level: hft_event::NoteLevel::Info,
        source: hft_event::NoteSource::Exec,
        strategy: S2,
        _pad: 0,
        group: G2,
        inst: hft_types::InstId(0),
        v: [0x137, 0, 0, 0, 0, 0, 0, 0],
    });
    xo
}

#[test]
fn seq_137_exec_out_drain_order() {
    let mut h = steady();
    let xo = one_of_each(&h);
    h.s.engine_mut().test_exec_inject = Some(xo);
    h.trace_on();
    clear_log();
    let m = h.mark();
    h.quote(A1, "50000", "50003");
    h.run();
    let c = per_input(&h)
        .into_iter()
        .find(|c| pos_of(c, "exec.advance").is_some())
        .expect("the advance Input");
    let names: Vec<&str> = c.iter().map(|e| e.what).collect();
    let at = |w: &str| pos_of(&c, w).unwrap_or_else(|| panic!("{w} in {names:?}"));
    let order = [
        at("exec_decision"),
        at("oms.cancel_resting_on"),
        at("exec.order"),
        at("exec.on_place_result"),
        c.iter()
            .position(|e| e.what == "oms.cancel" && e.b == 1)
            .expect("exec cancel"),
        at("exec.broken"),
        at("queue.exec"),
    ];
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?} {names:?}");
    // The order went through the pipeline between the two.
    let place = at("risk.evaluate");
    assert!(order[2] < place && place < order[3]);
    assert!(
        h.s.engine()
            .heap
            .sorted()
            .iter()
            .any(|t| t.owner == 5 && t.group == G2.0)
    );
    // Records: the ExecDecision first, the exec Note after the Broken CapSet Note.
    let recs = h.since(m);
    let xd = recs
        .iter()
        .position(|r| r.kind == EventKind::EXEC_DECISION)
        .expect("ExecDecision");
    let od = recs
        .iter()
        .position(|r| r.kind == EventKind::ORDER_DECISION)
        .expect("OrderDecision");
    let capset = recs
        .iter()
        .position(|r| {
            r.read::<hft_event::Note>()
                .is_some_and(|n| n.source == hft_event::NoteSource::Risk && n.code & 0xFFFF == 2)
        })
        .expect("CapSet{Broken}");
    let xn = recs
        .iter()
        .position(|r| r.read::<hft_event::Note>().is_some_and(|n| n.v[0] == 0x137))
        .expect("exec note");
    assert!(xd < od && od < capset && capset < xn, "{xd} {od} {capset} {xn}");
    h.run();
    assert!(log().iter().any(|g| g.s == S2 && g.kind == "Exec"));
}

/// Inputs whose trace has `apply.Broken` for (a, g): the number of `risk.on_legs(a, g)` after it.
fn on_legs_after_broken(h: &H, a: hft_types::AccountId, g: GroupId) -> Vec<usize> {
    let is = |e: &hft_sequencer::trace::TraceEv, w: &str| {
        e.what == w && e.a == u64::from(a.0) && e.b == u64::from(g.0)
    };
    per_input(h)
        .iter()
        .filter_map(|c| {
            let p = c.iter().position(|e| is(e, "apply.Broken"))?;
            Some(c[p..].iter().filter(|e| is(e, "risk.on_legs")).count())
        })
        .collect()
}

#[test]
fn seq_139_on_legs_after_broken() {
    use crate::scenarios as sc;
    use hft_types::{AccountId, CapSource};
    // .1 link Broken{Deadline}: the second leg is gated beyond the cross until the deadline.
    let mut h = sc::steady();
    let m = sc::place_on_a1(&mut h, &[S1], sc::link_open_slip);
    let first = sc::decs(&h, m)[0];
    h.ack(&first);
    h.quote(B1, "48000", "52000");
    h.run();
    let t_fill = h.now();
    h.fill(&first, first.qty, first.qty, first.px);
    h.trace_on();
    sc::run_to(&mut h, t_fill + 3_500 * MS);
    assert_eq!(
        sc::last_link(&h, AC1).map(|l| l.state),
        Some(hft_oms::LinkState::Broken)
    );
    assert_eq!(on_legs_after_broken(&h, AC1, G1), vec![1], ".1");

    // .2 the second leg is vetoed by HEDGE_LIMIT.
    let mut h = sc::steady();
    let first = sc::open_first(&mut h);
    h.quote(B1, "50010000", "50011000");
    h.run();
    h.trace_on();
    h.fill(&first, first.qty, first.qty, first.px);
    assert_eq!(on_legs_after_broken(&h, AC1, G1), vec![1], ".2");

    // .3 the executor's ExecOut.broken.
    let mut h = sc::steady();
    let mut xo = hft_exec::ExecOut::new();
    xo.broken.push(hft_exec::BrokenReq {
        account: AC1,
        group: G1,
    });
    h.s.engine_mut().test_exec_inject = Some(xo);
    h.trace_on();
    h.quote(A1, "50000", "50003");
    h.run();
    assert_eq!(on_legs_after_broken(&h, AC1, G1), vec![1], ".3");

    // .4 resolve_broken: a one-leg a1 fill lands during a HARD Kill; Resume{Kill} judges it.
    let mut h = sc::steady();
    let m = sc::place_on_a1(&mut h, &[S1], sc::a1_buy_open);
    let d = sc::decs(&h, m)[0];
    h.ack(&d);
    let kill = ControlCmd::Kill {
        scope: scope(hft_types::StrategyId(0), AC1, GroupId(0)),
        level: KillLevel::Account,
        flatten: false,
        cancel_resting: false,
        hard: true,
    };
    let id = h.cmd_confirmed(AC1, &kill);
    h.run();
    assert_eq!(h.res(id).0, 0, "{:?}", h.res(id));
    h.fill(&d, d.qty, d.qty, d.px);
    assert!(!sc::has_cap(&h, CapSource::BROKEN, AC1, Some(G1)));
    // Resume{Kill} needs a clean reconciliation of both venues seen after the Kill.
    let after = h.j().records().last().map_or(0, |r| r.seq);
    for (k, v) in [A, B].into_iter().enumerate() {
        h.push(RingId::Rep(0), &recon_summary(AC1, v, 91_000 + k as u64, after));
    }
    h.run();
    h.trace_on();
    let id = h.cmd_confirmed(
        AC1,
        &ControlCmd::Resume {
            scope: scope(hft_types::StrategyId(0), AC1, GroupId(0)),
            source: CapSource::KILL,
        },
    );
    h.run();
    assert_eq!(h.res(id).0, 0, "{:?}", h.res(id));
    assert!(sc::has_cap(&h, CapSource::BROKEN, AC1, Some(G1)), ".4 Broken");
    assert_eq!(on_legs_after_broken(&h, AC1, G1), vec![1], ".4");
    let _ = AccountId(0);

    // .5 a fill only (no Broken): on_legs once, with the LegTracker change.
    let mut h = sc::steady();
    let m = sc::place_on_a1(&mut h, &[S1], sc::a1_buy_open);
    let d = sc::decs(&h, m)[0];
    h.ack(&d);
    h.trace_on();
    h.fill(&d, d.qty, d.qty, d.px);
    let c = per_input(&h)
        .into_iter()
        .find(|c| pos_of(c, "ledger.apply_fill").is_some())
        .expect("fill Input");
    assert_eq!(c.iter().filter(|e| e.what == "risk.on_legs").count(), 1, ".5");
    assert!(pos_of(&c, "apply.Broken").is_none());
}

// ------------------------------------------------------------------------------- SEQ-132

/// s2's g2: first leg b1 sell 0.003 with an OnFill a1 buy; every second leg expires until the
/// link is Broken (b1 −0.003 left). Returns the harness at the Broken Input.
fn g2_broken_short_b1() -> H {
    use crate::scenarios as sc;
    fn first(hx: &Hx<'_>) -> hft_strategy_sdk::OrderIntent {
        let b = hx.v.bbo(B1);
        let mut it = intent(
            hx,
            B1,
            Side::Sell,
            hft_types::Tif::Ioc,
            hft_strategy_sdk::IntentClass::Open,
            b.bid_px,
            q("0.003"),
        );
        it.group = G2;
        it.link = hft_strategy_sdk::LegLinkOpt::some(hft_strategy_sdk::LegLink {
            group: G2,
            role: hft_strategy_sdk::LinkRole::First,
            _pad: [0; 3],
            tolerance: r("0.02"),
            hedge: onfill(hx.s.account, A1, Side::Buy),
        });
        it
    }
    let mut h = sc::steady();
    let m = sc::place_on_a1(&mut h, &[S2], first);
    let f = sc::decs(&h, m)
        .into_iter()
        .find(|d| d.inst == B1)
        .expect("first leg");
    assert_eq!(f.reject, 0, "{f:?}");
    h.ack(&f);
    let m0 = h.mark();
    h.fill(&f, f.qty, f.qty, f.px);
    for _ in 0..8 {
        if sc::has_cap(&h, hft_types::CapSource::BROKEN, AC2, Some(G2)) {
            return h;
        }
        let open = sc::decs(&h, m0)
            .into_iter()
            .rev()
            .find(|d| d.inst == A1 && d.reject == 0)
            .expect("hedge leg");
        h.ack(&open);
        sc::expire(&mut h, &open);
        if sc::has_cap(&h, hft_types::CapSource::BROKEN, AC2, Some(G2)) {
            return h;
        }
        let due = sc::hedge_retry_timers(&h).first().map(|t| t.0).expect("retry");
        sc::keep_alive_to(&mut h, due);
    }
    panic!("g2 not Broken");
}

#[test]
fn seq_132_on_risk_after_updates() {
    use crate::scenarios as sc;
    let mut h = g2_broken_short_b1();
    // The bounded auto-fix starts broken_auto_fix_ms later with one b1 buy 0.003 slice.
    let t = h.now();
    let m = h.mark();
    let mut slice = None;
    let mut until = t;
    while slice.is_none() && until < t + 125 * SEC {
        until += SEC;
        sc::keep_alive_to(&mut h, until);
        h.quote(B1, "50010", "50011");
        h.run();
        slice = sc::decs(&h, m)
            .into_iter()
            .find(|d| d.inst == B1 && d.reject == 0 && d.account == AC2);
    }
    let slice = slice.expect("auto-fix slice");
    assert_eq!((slice.side, slice.qty), (Side::Buy, q("0.003")));
    h.ack(&slice);
    h.trace_on();
    let m = h.mark();
    h.fill(&slice, slice.qty, slice.qty, slice.px);
    let tr = h.trace();
    let at = |w: &str| {
        tr.iter()
            .position(|x| *x == w)
            .unwrap_or_else(|| panic!("{w}: {tr:?}"))
    };
    let upd = at("exec.on_order_update");
    let legs = tr
        .iter()
        .enumerate()
        .skip(upd)
        .find(|(_, x)| **x == "risk.on_legs")
        .map(|(i, _)| i)
        .expect("on_legs after the update");
    let apply = tr
        .iter()
        .enumerate()
        .skip(legs)
        .find(|(_, x)| **x == "risk.apply")
        .map(|(i, _)| i)
        .expect("apply(AutoClear)");
    let on_risk = tr
        .iter()
        .enumerate()
        .skip(apply)
        .find(|(_, x)| **x == "exec.on_risk")
        .map(|(i, _)| i)
        .expect("exec.on_risk(CapCleared)");
    assert!(upd < legs && legs < apply && apply < on_risk);
    let recs = h.since(m);
    assert!(
        recs.iter()
            .any(|r| r.read::<hft_event::ExecDecision>().is_some_and(|x| x.action
                == hft_exec::decision::action::AUTO_FIX
                && x.detail
                    == hft_exec::pack(
                        hft_exec::decision::table::AUTO_FIX,
                        hft_exec::decision::autofix::DONE,
                        0
                    ))),
        "ExecDecision{{AutoFix/Done}}"
    );
    assert!(
        h.notes_since(m)
            .iter()
            .any(|n| n.source == hft_event::NoteSource::Exec
                && n.code & 0xFFFF == hft_exec::decision::note::EXEC_AUTO_FIX_DONE)
    );
    assert!(
        h.notes_since(m)
            .iter()
            .any(|n| n.source == hft_event::NoteSource::Risk && n.code & 0xFFFF == 3),
        "CapCleared{{Broken}}"
    );
    assert!(!sc::has_cap(&h, hft_types::CapSource::BROKEN, AC2, Some(G2)));
    assert!(sc::has_cap(&h, hft_types::CapSource::HEDGE_FAILURE, AC2, None));
}

// ------------------------------------------------------------------------------- SEQ-136

mod seq_136 {
    use proptest::prelude::*;

    use super::*;
    use crate::scenarios as sc;

    fn a1_sell(hx: &Hx<'_>) -> hft_strategy_sdk::OrderIntent {
        let b = hx.v.bbo(A1);
        intent(
            hx,
            A1,
            Side::Sell,
            hft_types::Tif::Ioc,
            hft_strategy_sdk::IntentClass::Reduce,
            b.bid_px,
            q("0.001"),
        )
    }

    /// One random Input (or a few) of the generator.
    fn op(h: &mut H, k: u8, x: u32) {
        match k {
            0 => {
                let (i, base) = match x % 4 {
                    0 => (A1, 50_000),
                    1 => (B1, 50_010),
                    2 => (A2, 3_000),
                    _ => (B2, 3_000),
                };
                let bid = base + i64::from(x % 7) - 3;
                h.quote(i, &bid.to_string(), &(bid + 1).to_string());
                h.run();
            }
            1 => {
                let f: fn(&Hx<'_>) -> hft_strategy_sdk::OrderIntent = match x % 4 {
                    0 => sc::a1_buy_open,
                    1 => sc::b1_sell_open,
                    2 => a1_sell,
                    _ => sc::link_open,
                };
                sc::place_on_a1(h, &[S1], f);
            }
            2 => {
                // Answer one open order: ack, fill, partial fill, expire or reject.
                let open: Vec<hft_event::OrderDecision> = sc::decs(h, 0)
                    .into_iter()
                    .filter(|d| {
                        d.reject == 0
                            && d.action == hft_event::OrderAction::New
                            && sc::slot_state(h, d).is_some_and(|s| {
                                matches!(s, hft_oms::OrderState::PendingNew | hft_oms::OrderState::Live)
                            })
                    })
                    .collect();
                if open.is_empty() {
                    return;
                }
                let d = open[(x as usize / 8) % open.len()];
                match x % 5 {
                    0 => h.ack(&d),
                    1 => h.fill(&d, d.qty, d.qty, d.px),
                    2 => {
                        let half = Qty(d.qty.0 / 2);
                        h.fill(&d, half, half, d.px)
                    }
                    3 => sc::expire(h, &d),
                    _ => h.reject(&d),
                }
            }
            3 => {
                let qty = Qty(i64::from(x % 5) * 1_000_000);
                let t = target(
                    AC2,
                    G2,
                    &[(0, qty), (1, Qty(-qty.0))],
                    ExecSpec::PairOnFill(pair0()),
                );
                h.set_target_s2(t);
            }
            4 => {
                let (i, side) = if x.is_multiple_of(2) {
                    (A1, Side::Sell)
                } else {
                    (B1, Side::Buy)
                };
                h.push(
                    RingId::Rep(0),
                    &ext(
                        AC1,
                        i,
                        side,
                        "0.001",
                        hft_event::ExternalCause::Unknown,
                        9_000 + u64::from(x),
                    ),
                );
                h.run();
            }
            5 => {
                let liq = if x.is_multiple_of(2) { "52000" } else { "80000" };
                margin_item(h, liq);
            }
            6 => {
                let c = if x.is_multiple_of(2) {
                    ControlCmd::Kill {
                        scope: scope(hft_types::StrategyId(0), AC1, GroupId(0)),
                        level: KillLevel::Account,
                        flatten: true,
                        cancel_resting: true,
                        hard: false,
                    }
                } else {
                    ControlCmd::Resume {
                        scope: scope(hft_types::StrategyId(0), AC1, GroupId(0)),
                        source: hft_types::CapSource::KILL,
                    }
                };
                h.cmd_confirmed(AC1, &c);
                h.run();
            }
            _ => {
                let t = h.now() + u64::from(x % 1_500) * MS;
                sc::keep_alive_to(h, t);
            }
        }
    }

    /// A MarginReport of (acct-1, B) with one b1 item at `liq`.
    fn margin_item(h: &mut H, liq: &str) {
        use zerocopy::FromZeros;
        let mut hd = hft_event::MarginReportHdr::new_zeroed();
        hd.venue = B;
        hd.account = AC1;
        hd.source = 1;
        hd.n = 1;
        hd.margin_balance = n("100000");
        hd.maint_margin = n("10");
        hd.avail_margin = n("99000");
        hd.rx_tsc = Tsc(h.now());
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

    proptest! {
        #![proptest_config(ProptestConfig { cases: 300, ..ProptestConfig::default() })]
        /// Every executor call drains its `ExecOut` before the buffer goes back
        /// (`Engine::put_exec` debug assertion) under targets, fills, external fills, L3
        /// margin reports and flattening kills.
        #[test]
        fn seq_136_exec_out_drained_after_each_call(
            ops in proptest::collection::vec((0u8..8, any::<u32>()), 100..400)
        ) {
            let mut h = sc::steady();
            h.mark_md(B1, "50010");
            for (k, x) in ops {
                op(&mut h, k, x);
            }
            prop_assert!(h.s.engine().exec.cell(AC2, G2).is_some());
        }
    }
}
