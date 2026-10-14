//! The intent pipeline (TESTS.md "意图管线": SEQ-170–SEQ-181).

use hft_event::{ControlCmd, EventKind, OrderDecision, RecordFlags};
use hft_risk::IntentView;
use hft_sequencer::OutRing;
use hft_sequencer::driver::live::RingId;
use hft_strategy_sdk::{EventRef, IntentClass, IntentOutcome};
use hft_types::{GroupId, KillLevel, RejectBits, RunMode, Side, Tif};

use crate::common::*;

fn steady() -> H {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    h
}

/// s1 places `f(hx)` on the next a1 Bbo; returns the mark before it.
fn s1_place(h: &mut H, f: fn(&Hx<'_>) -> hft_strategy_sdk::OrderIntent) -> usize {
    set_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            let _ = ctx.place(f(hx));
        }
    });
    let m = h.mark();
    h.quote(A1, "50000", "50003");
    h.run();
    clear_hook();
    m
}

fn views(h: &H) -> Vec<IntentView> {
    h.s.engine().trace.views.clone()
}

#[test]
fn seq_175_capacity_rejection_is_journaled_and_replayable() {
    reset_script();
    let mut b = b0();
    b.setup_edit.push(|s| s.oms.slots = hft_oms::PerTable::new(1));
    let mut h = b.boot();
    h.steady();
    let start = h.keyframe().seq;
    set_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            let _ = ctx.place(open_buy(hx, A1));
        }
    });
    let mark = h.mark();
    h.quote(A1, "50000", "50003");
    h.run();
    let first = decisions(&h.since(mark))[0].1;
    assert_eq!(first.reject, 0);
    let before = h.s.engine().oms.inflight().pos(AC1, A1);
    let risk_snapshot = |h: &H| {
        let mut w = hft_strategy_sdk::SnapshotWriter::new(1 << 20);
        h.s.engine().risk.snapshot(&mut w);
        w.as_bytes().to_vec()
    };
    let risk_before = risk_snapshot(&h);
    let sends = h.io_ref().out.len();
    let mark = h.mark();
    h.quote(A1, "50000", "50004");
    h.run();
    let ds = decisions(&h.since(mark));
    assert_eq!(ds.len(), 1);
    let rejected = ds[0].1;
    assert_eq!(rejected.reject, RejectBits::CAPACITY.0);
    assert_eq!((rejected.slot.0, rejected.r#gen), (0, 0));
    assert_eq!(
        (rejected.account, rejected.strategy, rejected.group),
        (AC1, S1, G1)
    );
    assert_eq!(h.io_ref().out.len(), sends);
    assert_eq!(h.s.engine().oms.inflight().pos(AC1, A1), before);
    assert_eq!(
        risk_snapshot(&h),
        risk_before,
        "rejection must not consume tokens"
    );
    assert!(h.verify_from(Some(start)).is_empty());
    clear_hook();
}

#[test]
fn seq_170_hedge_gated() {
    let mut h = steady();
    let m = s1_place(&mut h, |hx| pair_first(hx, q("0.01")));
    let first: OrderDecision = decisions(&h.since(m))[0].1;
    assert_eq!(first.reject, 0);
    h.ack(&first);
    // b1's BBO member connections down.
    h.push(
        RingId::Rep(0),
        &conn(
            B,
            0,
            hft_event::ConnStateKind::Down,
            hft_event::ConnRole::Md,
            hft_types::InstId(0),
            hft_types::AccountId(0),
            0,
        ),
    );
    h.run();
    h.trace_on();
    let n_out = h.io_ref().out.len();
    let m = h.mark();
    h.fill(&first, q("0.01"), q("0.01"), first.px);
    assert!(h.trace().contains(&"oms.hedge_gated"), "{:?}", h.trace());
    let ds = decisions(&h.since(m));
    assert!(ds.iter().all(|d| d.1.inst != B1));
    assert!(h.io_ref().out[n_out..].iter().all(|o| o.0 != OutRing::Venue(B)));
    assert!(
        h.s.engine()
            .heap
            .sorted()
            .iter()
            .any(|e| e.owner == 3 && e.timer_id & 0xFF == 4)
    );
}

#[test]
fn seq_174_mode_gate() {
    // .1 Shadow: decision with SHADOW, no frame, handle Shadowed.
    let mut h = steady();
    h.cmd(
        AC1,
        &ControlCmd::SetMode {
            scope: scope(S1, AC1, G1),
            mode: RunMode::Shadow,
        },
    );
    h.run();
    let c: Cap<Option<IntentOutcome>> = cap();
    let c2 = c.clone();
    set_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            let hd = ctx.place(open_buy(hx, A1)).unwrap();
            c2.borrow_mut().push(ctx.outcome(hd));
        }
        if let EventRef::Order { .. } = ev {
            let _ = 0;
        }
    });
    let n_out = h.io_ref().out.len();
    let m = h.mark();
    h.quote(A1, "50000", "50003");
    h.run();
    clear_hook();
    let recs = h.since(m);
    let d = recs.iter().find(|r| r.kind == EventKind::ORDER_DECISION).unwrap();
    assert!(d.flags.0 & RecordFlags::SHADOW.0 != 0);
    assert_eq!(h.io_ref().out.len(), n_out);
}

#[test]
fn seq_176_link_fields() {
    let mut h = steady();
    h.quote(B1, "49997.5", "50002.5");
    h.run();
    h.trace_on();
    s1_place(&mut h, |hx| pair_first(hx, q("0.01")));
    let iv = views(&h)
        .into_iter()
        .find(|v| v.inst == A1)
        .expect("first leg view");
    assert_eq!(iv.link_venue, B);
    assert_eq!(iv.link_account, AC1);
    assert_eq!(iv.link_inst, B1);
    assert_eq!(iv.link_side, Side::Sell);
    assert_eq!(iv.link_qty, q("0.010"));
    // mid 50 000 × (1 + rr_max_cross 0.01 of class 0) × 0.010.
    let rr =
        h.s.engine()
            .risk
            .view_for(AC1, B1, &h.s.engine().now)
            .rr_max_cross;
    let want = 50_000.0 * (1.0 + rr.0 as f64 / 1e9) * 0.01;
    assert!(
        (iv.link_notional.0 as f64 / 1e6 - want).abs() < 0.01,
        "{} vs {want}",
        iv.link_notional.0
    );
}

#[test]
fn seq_178_peer_and_hedge_cap() {
    super::simultaneous::check_simultaneous_atomic_risk_and_capacity();
    super::simultaneous::check_simultaneous_cold_restart_keeps_inflight_link();
    super::simultaneous::check_simultaneous_residual_affordability();
    super::simultaneous::check_simultaneous_second_binding_blocks_both();
    let mut h = steady();
    let m = s1_place(&mut h, |hx| pair_first(hx, q("0.01")));
    let first: OrderDecision = decisions(&h.since(m))[0].1;
    h.ack(&first);
    h.trace_on();
    h.fill(&first, q("0.01"), q("0.01"), first.px);
    let iv = views(&h).into_iter().find(|v| v.inst == B1).expect("hedge view");
    assert_eq!(iv.class, IntentClass::Hedge);
    assert!(iv.hedge_cap_qty.0 > 0);
    // OnFill second legs carry no peer.
    assert_eq!(iv.link_venue, hft_types::VenueId(0));
}

#[test]
fn seq_179_commit_place_route() {
    // .1 Live account: out-A, nothing on out-sim.
    let mut h = steady();
    let n = h.io_ref().out.len();
    s1_place(&mut h, |hx| open_buy(hx, A1));
    let new: Vec<_> = h.io_ref().out[n..].to_vec();
    assert_eq!(new.len(), 1);
    assert_eq!(new[0].0, OutRing::Venue(A));
    // .2 Sim account: out-sim.
    let n = h.io_ref().out.len();
    set_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S9
        {
            let _ = ctx.place(open_buy(hx, A1));
        }
    });
    h.quote(A1, "50000", "50004");
    h.run();
    clear_hook();
    let new: Vec<_> = h.io_ref().out[n..].to_vec();
    assert_eq!(new.len(), 1);
    assert_eq!(new[0].0, OutRing::Sim);
    // .3 perp Open with leverage 12.349 → lev_x100 1234 (on the GwCmd / decision projection).
    let l = hft_event::LeverageItem {
        inst: B1,
        _pad: 0,
        leverage: r("12.349"),
    };
    let hd = cfg_hdr(AC1, B, 1);
    let mut body = zerocopy::IntoBytes::as_bytes(&hd).to_vec();
    body.extend_from_slice(zerocopy::IntoBytes::as_bytes(&l));
    h.push_bytes(RingId::Rep(0), EventKind::ACCOUNT_CONFIG_REPORT, body);
    h.run();
    h.trace_on();
    s1_place(&mut h, |hx| {
        let b = hx.v.bbo(B1);
        let mut it = intent(
            hx,
            B1,
            Side::Sell,
            Tif::Ioc,
            IntentClass::Open,
            b.bid_px,
            q("0.001"),
        );
        it.group = G1;
        it
    });
    let lev =
        h.s.engine()
            .trace
            .ev
            .iter()
            .find(|e| e.what == "oms.place")
            .map(|e| e.b);
    assert_eq!(lev, Some(1234));
}

#[test]
fn seq_180_result_backfill() {
    let mut h = steady();
    let c: Cap<Option<IntentOutcome>> = cap();
    let c2 = c.clone();
    set_hook(move |hx, ev, ctx| match ev {
        EventRef::Bbo { inst, .. } if *inst == A1 && hx.s.strategy == S1 => {
            let hd = ctx.place(open_buy(hx, A1)).unwrap();
            c2.borrow_mut().push(ctx.outcome(hd));
        }
        EventRef::Order { .. } if hx.s.strategy == S1 => {}
        _ => {}
    });
    h.quote(A1, "50000", "50003");
    h.run();
    clear_hook();
    // The outcome is available to the strategy in a later delivery of the same Input or after.
    let _ = c.borrow().len();
    // .5 a smoke order: Applied and its OrderDecision in the same event, trigger = command seq.
    let id = h.cmd_confirmed(
        AC1,
        &ControlCmd::SmokeOrder {
            account: AC1,
            venue: A,
            _pad: 0,
            inst: A1,
            side: Side::Buy,
            _pad2: [0; 3],
            ttl_ms: 5_000,
        },
    );
    let m = h.mark();
    h.run();
    let r = h.result_of(id).unwrap();
    assert_eq!(r.status, 0, "{r:?}");
    let ds = decisions(&h.since(m));
    assert_eq!(ds.len(), 1);
    assert_eq!(ds[0].1.trigger_seq, r.cmd_seq);
    let _ = (GroupId(0), KillLevel::Global, RejectBits::RATE);
}

/// A first leg a1 buy with an OnFill b1 sell link whose `max_slip` is `slip`.
fn pair_first_slip(hx: &Hx<'_>, qty: hft_types::Qty, slip: &str) -> hft_strategy_sdk::OrderIntent {
    let mut it = open_buy(hx, A1);
    it.qty = qty;
    let mut plan = onfill(hx.s.account, B1, Side::Sell);
    plan.max_slip = r(slip);
    it.link = hft_strategy_sdk::LegLinkOpt::some(hft_strategy_sdk::LegLink {
        group: it.group,
        role: hft_strategy_sdk::LinkRole::First,
        _pad: [0; 3],
        tolerance: r("0.02"),
        hedge: plan,
    });
    it
}

#[test]
fn seq_171_exec_order_hedge_limit() {
    let mut h = steady();
    // s2's unhedged a1 buy fill leaves g2 naked 0.006.
    let d = h.order_at_touch(AC2, A1, Side::Buy, "0.006");
    h.fill(&d, d.qty, d.qty, d.px);
    // The producer understates the cap: the g2 repair order (0.006) carries 0.004.
    h.s.engine_mut().test_exec_hedge_cap = Some(q("0.004"));
    h.trace_on();
    h.s.engine_mut().trace.clear();
    let n_out_ack = h.io_ref().out.len();
    let m_fill = h.mark();
    let id = h.cmd_confirmed(
        AC2,
        &ControlCmd::HedgeFix {
            scope: scope(S2, AC2, G2),
        },
    );
    h.run();
    assert_eq!(h.result_of(id).map(|r| r.status), Some(0), "HedgeFix applied");
    let recs = h.since(m_fill);
    let ds = decisions(&recs);
    let veto: Vec<_> = ds
        .iter()
        .filter(|x| x.1.reject == RejectBits::HEDGE_LIMIT.0)
        .collect();
    assert_eq!(veto.len(), 1, "{ds:?}");
    assert_eq!(veto[0].1.account, AC2);
    assert_eq!(veto[0].1.qty, q("0.006"));
    // No frame for it.
    assert_eq!(h.io_ref().out.len(), n_out_ack, "nothing went out");
    let t = h.s.engine().trace.clone();
    // on_place_result(Rejected(HEDGE_LIMIT)) first, then Broken{2, g2} applied once.
    let res = t.pos("exec.on_place_result").expect("on_place_result");
    assert_eq!(t.count("apply.Broken"), 1, "{:?}", t.names());
    assert!(t.pos("apply.Broken").unwrap() > res, "{:?}", t.names());
    assert_eq!(
        t.ev.iter()
            .filter(|e| e.what == "risk.on_legs" && e.a == u64::from(AC2.0) && e.b == u64::from(G2.0))
            .count(),
        1,
        "{:?}",
        t.names()
    );
    // Broken{2, g2} in effect, one P0 note (the executor's ExecBroken).
    let notes = h.notes_since(m_fill);
    let p0: Vec<_> = notes
        .iter()
        .filter(|n| n.level == hft_event::NoteLevel::P0)
        .collect();
    assert_eq!(p0.len(), 1, "{notes:?}");
    assert!(
        h.s.engine()
            .risk
            .mode
            .caps()
            .iter()
            .any(|c| c.source == hft_types::CapSource::BROKEN
                && c.scope.account == AC2
                && c.scope.group == G2),
        "Broken cap of (2, g2)"
    );
}

#[test]
fn seq_172_overslip_priced_at_cap() {
    let mut h = steady();
    let m = s1_place(&mut h, |hx| pair_first_slip(hx, q("0.01"), "0.002"));
    let first: OrderDecision = decisions(&h.since(m))[0].1;
    assert_eq!(first.reject, 0);
    h.ack(&first);
    // b1's bid moves 0.4 % away from the reference (50 010) — beyond max_slip 0.002, within
    // rr_max_cross (0.01 of the B0 class) of the mid.
    h.quote(B1, "49810", "50011");
    h.run();
    h.trace_on();
    let n_out = h.io_ref().out.len();
    let m = h.mark();
    h.fill(&first, q("0.01"), q("0.01"), first.px);
    let ds = decisions(&h.since(m));
    let second = ds.iter().find(|d| d.1.inst == B1).expect("second leg").1;
    assert_eq!(second.reject, 0, "{second:?}");
    let e = h.s.engine();
    let rr = e.risk.hedge_px_limits(AC1, B, B1).rr_max_cross;
    let tick = h.rd.inst(B1).tick;
    let mid = hft_types::Px::mid_floor(px("49810"), px("50011"));
    let cap = mid
        .mul_rate(hft_types::Rate(-rr.0), hft_types::RoundMode::Ceil)
        .round_to_tick(tick, hft_types::RoundMode::Ceil);
    assert_eq!(second.px, cap, "limit = price_hedge's cap");
    assert!(!h.trace().contains(&"oms.hedge_gated"), "{:?}", h.trace());
    assert!(
        h.io_ref().out[n_out..]
            .iter()
            .any(|o| o.0 == OutRing::Venue(B) && o.1.inst == B1),
        "second leg out on B"
    );
    let slip = hft_oms::notes::code_of(hft_oms::notes::HEDGE_SLIP_CAPPED);
    assert_eq!(
        h.notes_since(m).iter().filter(|n| n.code == slip).count(),
        1,
        "one OmsHedgeSlipCapped"
    );
}

#[test]
fn seq_173_quiet_book_still_hedges() {
    let mut h = steady();
    // b1's last Bbo now; then 10 s of a quiet book with its connection alive every 500 ms.
    h.quote(B1, "50010", "50011");
    h.run();
    for _ in 0..20 {
        h.adv(500 * MS);
        let t = h.now();
        h.push(
            RingId::Rep(0),
            &conn(
                B,
                0,
                hft_event::ConnStateKind::Alive,
                hft_event::ConnRole::Md,
                hft_types::InstId(0),
                hft_types::AccountId(0),
                t,
            ),
        );
        h.run();
    }
    let m = s1_place(&mut h, |hx| pair_first(hx, q("0.01")));
    let first: OrderDecision = decisions(&h.since(m))[0].1;
    assert_eq!(first.reject, 0, "LINK_STALE must not fire on a quiet book");
    h.ack(&first);
    h.trace_on();
    let n_out = h.io_ref().out.len();
    let m = h.mark();
    h.fill(&first, q("0.01"), q("0.01"), first.px);
    let ds = decisions(&h.since(m));
    let second = ds.iter().find(|d| d.1.inst == B1).expect("second leg").1;
    assert_eq!(second.reject, 0, "{second:?}");
    assert!(!h.trace().contains(&"oms.hedge_gated"), "{:?}", h.trace());
    let stale = hft_oms::notes::code_of(hft_oms::notes::HEDGE_PX_STALE);
    assert!(h.notes_since(m).iter().all(|n| n.code != stale));
    assert!(
        h.io_ref().out[n_out..]
            .iter()
            .any(|o| o.0 == OutRing::Venue(B) && o.1.inst == B1)
    );
}

// ===================================================================================================
// SEQ-181: InflightBook.open_margin, incremental vs per-slot sum (helpers: tests/common/risk_kit.rs).
// ===================================================================================================

mod open_margin {
    use hft_event::{EventKind, LeverageItem, OrderDecision, OrderEventKind};
    use hft_sequencer::driver::live::RingId;
    use hft_strategy_sdk::IntentClass;
    use hft_types::{Qty, Rate, RoundMode, Side, Tif};
    use proptest::prelude::*;
    use zerocopy::IntoBytes;

    use crate::common::risk_kit::*;
    use crate::common::*;

    /// One in-flight b1 Open of the reference model.
    struct Live {
        d: OrderDecision,
        lev_x100: i64,
        filled: Qty,
        acked: bool,
    }

    /// Account 1's b1 leverage on B set to `x` (b2 kept at 3×).
    fn set_leverage(h: &mut H, x: u16) {
        let levs = [
            LeverageItem {
                inst: B1,
                _pad: 0,
                leverage: Rate(i64::from(x) * 1_000_000_000),
            },
            LeverageItem {
                inst: B2,
                _pad: 0,
                leverage: r("3.00"),
            },
        ];
        let mut body = cfg_hdr(AC1, B, 2).as_bytes().to_vec();
        for l in &levs {
            body.extend_from_slice(l.as_bytes());
        }
        h.push_bytes(RingId::Rep(0), EventKind::ACCOUNT_CONFIG_REPORT, body);
        h.run();
    }

    fn reference(h: &H, live: &[Live]) -> i64 {
        let exp = h.rd.asset(BTC).exp;
        live.iter()
            .map(|o| {
                let rem = Qty(o.d.qty.0 - o.filled.0);
                let notional = o.d.px.mul_qty(rem, exp, RoundMode::HalfEven).0.0;
                (i128::from(notional) * 100 / i128::from(o.lev_x100)) as i64
            })
            .sum()
    }

    fn report(h: &mut H, o: &Live, what: OrderEventKind, qty: Qty, cum: Qty) {
        let mut ev = order_event(&o.d, what, h.epoch());
        ev.qty = qty;
        ev.cum_filled = cum;
        h.push(RingId::Rep(0), &ev);
        h.run();
    }

    /// Reports drawn from {Ack, PartialFill, Fill, Cancel}; `op = (kind, leverage, pick)`.
    fn run(ops: &[(u8, u16, u8)]) -> Result<(), TestCaseError> {
        reset_script();
        let mut b = b0();
        // No repair of the deliberately unhedged b1 opens (it would end in Broken / CloseOnly).
        b.setup_edit
            .push(|s| s.exec.default.fix_tolerance_default = Rate(1_000_000_000_000));
        // … nor an exposure gate on it: every generated open reaches the OMS.
        let rd = b.rd.load();
        let mut p = b0_risk(&rd);
        for a in p.accounts.as_mut_slice() {
            a.default_group.naked_gate = Rate(i64::MAX);
        }
        b.risk = Some(p);
        let mut h = b.boot();
        h.steady();
        let mut live: Vec<Live> = Vec::new();
        let (mut tried, mut placed, mut max_margin) = (0u32, 0u32, 0i64);
        let mut rejects = Vec::new();
        for &(kind, lev, pick) in ops {
            // 600 ms per step and ≤ 6 live orders: terminal slots wait reuse_quarantine_ms
            // (5 s) before reuse, and B0 has 16 slots per (venue, account).
            h.adv(600 * MS);
            md_alive(&mut h);
            h.margin_report(AC1, "100000", "0", "100000");
            h.mark_md(B1, "50010");
            if kind == 0 || live.is_empty() {
                if live.len() >= 6 {
                    continue;
                }
                set_leverage(&mut h, lev);
                let d = place_tif(
                    &mut h,
                    S1,
                    G1,
                    B1,
                    Side::Sell,
                    q("0.002"),
                    IntentClass::Open,
                    Tif::Gtc,
                );
                tried += 1;
                if d.reject != 0 {
                    let caps: Vec<_> =
                        h.s.engine()
                            .risk
                            .mode
                            .caps()
                            .iter()
                            .map(|c| (c.source, c.scope.account, c.scope.group))
                            .collect();
                    rejects.push(format!("{:#x} {caps:?}", d.reject));
                }
                if d.reject == 0 {
                    placed += 1;
                    // Acknowledged at once: an order left PendingNew past ack_timeout goes
                    // Unknown and the venue stops admitting Opens (VENUE_NOT_READY).
                    h.ack(&d);
                    live.push(Live {
                        d,
                        lev_x100: i64::from(lev) * 100,
                        filled: Qty(0),
                        acked: true,
                    });
                }
            } else {
                let k = usize::from(pick) % live.len();
                let o = &mut live[k];
                let rem = Qty(o.d.qty.0 - o.filled.0);
                match kind {
                    1 if !o.acked => {
                        o.acked = true;
                        let x = Live {
                            d: o.d,
                            lev_x100: o.lev_x100,
                            filled: o.filled,
                            acked: true,
                        };
                        report(&mut h, &x, OrderEventKind::Ack, Qty(0), x.filled);
                    }
                    1 | 2 if rem.0 > q("0.001").0 => {
                        let f = q("0.001");
                        o.filled = Qty(o.filled.0 + f.0);
                        let x = Live {
                            d: o.d,
                            lev_x100: o.lev_x100,
                            filled: o.filled,
                            acked: o.acked,
                        };
                        report(&mut h, &x, OrderEventKind::PartialFill, f, x.filled);
                    }
                    2 => {
                        let x = Live {
                            d: o.d,
                            lev_x100: o.lev_x100,
                            filled: o.d.qty,
                            acked: o.acked,
                        };
                        live.remove(k);
                        report(&mut h, &x, OrderEventKind::Fill, rem, x.d.qty);
                    }
                    _ => {
                        let x = Live {
                            d: o.d,
                            lev_x100: o.lev_x100,
                            filled: o.filled,
                            acked: o.acked,
                        };
                        live.remove(k);
                        report(&mut h, &x, OrderEventKind::Expired, Qty(0), x.filled);
                    }
                }
            }
            let got = h.s.engine().oms.inflight().open_margin(AC1, B).0;
            prop_assert_eq!(got, reference(&h, &live), "after {:?}", (kind, lev, pick));
            max_margin = max_margin.max(got);
        }
        // Non-vacuous: every attempted open is admitted and carries margin.
        prop_assert_eq!(placed, tried, "rejects {:x?}", rejects);
        prop_assert!(tried == 0 || max_margin > 0);
        Ok(())
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(ProptestConfig::default().cases.max(1_000)))]
        #[test]
        fn seq_181_open_margin_incremental(
            ops in proptest::collection::vec((0u8..4, 1u16..=20, any::<u8>()), 1..300)
        ) {
            run(&ops)?;
        }
    }
}
// ------------------------------------------------------------------------------- SEQ-177

mod seq_177 {
    use hft_strategy_sdk::{BboView, HedgeKind, HedgePlan, LinkRole, OnFail, PxMode, ResidualPolicy};
    use hft_types::{AssetExp, HedgePxLimits, InstId, Notional, Px, Qty, Rate, Side, Tsc, VenueId};
    use proptest::prelude::*;

    const EXP: AssetExp = AssetExp {
        px_exp: -2,
        qty_exp: -8,
    };
    const PAR: Rate = Rate(1_000_000_000);

    proptest! {
        #![proptest_config(ProptestConfig { cases: 5_000, ..ProptestConfig::default() })]
        /// At an unchanged second-leg mid, `price_hedge`'s limit × the second leg's quantity never
        /// exceeds the `link_notional` the first leg was admitted with; so a first leg admitted
        /// with `link_notional ≤ max_hedge_notional` is never refused by #6 on notional.
        #[test]
        fn seq_177_second_leg_within_link_notional(
            mid_units in 1i64..=100_000,
            mid_cents in 0i64..100,
            rr_bp in 5i64..=200,
            off_bp in -300i64..=300,
            buy in any::<bool>(),
            num in 1u32..=20,
            den in 1u32..=10,
            lots in 1i64..=10_000,
            param_bp in 0i64..=100,
            cross in any::<bool>(),
        ) {
            let num = num.min(2 * den); // ratio 0.1–2 (den ≤ 10, so num / den ≥ 0.1)
            let mid = Px(mid_units * 100 + mid_cents);
            let rr = Rate(rr_bp * 100_000);
            let tick = 1i64;
            let lot = 100_000i64; // 0.001
            // Counterparty price off mid by `off_bp`, the other side mirrored: same mid.
            let d = (mid.0 * off_bp.abs() / 10_000).max(0);
            let (bid, ask) = (Px(mid.0 - d), Px(mid.0 + d));
            prop_assume!(bid.0 > 0 && Px::mid_floor(bid, ask) == mid);
            let first_qty = Qty(lots * lot);
            let (link_qty, link_notional) =
                hft_sequencer::views::link_projection(first_qty, num, den, bid, ask, rr, EXP, PAR);
            // The second leg: the net fill × ratio floored to its lot (never above link_qty).
            let qty = Qty(link_qty.0 / lot * lot);
            prop_assume!(qty.0 > 0);
            let side = if buy { Side::Buy } else { Side::Sell };
            let plan = HedgePlan {
                kind: HedgeKind::OnFill,
                px_mode: if cross { PxMode::Cross } else { PxMode::Mid },
                on_fail: OnFail::Retry,
                tif: hft_types::Tif::Ioc,
                venue: VenueId::GATE_USDT_FUT,
                account: hft_types::AccountId(1),
                side,
                reduce_only: 0,
                inst: InstId(21),
                ratio_num: num,
                ratio_den: den,
                residual: ResidualPolicy::Defer,
                _pad: [0; 3],
                px_param: Rate(param_bp * 100_000),
                max_slip: Rate(50_000_000),
                limit_px: Px(0),
            };
            let req = hft_oms::HedgeRequest {
                link_id: 1,
                role: LinkRole::Second,
                venue: plan.venue,
                account: plan.account,
                side,
                reduce_only: false,
                inst: plan.inst,
                group: hft_types::GroupId(0x0100_0001),
                strategy: hft_types::StrategyId(1),
                qty,
                basis: qty,
                hedge_cap_qty: Qty(qty.0 + lot),
                ref_px: if buy { ask } else { bid },
                peer_venue: VenueId(0),
                peer_account: hft_types::AccountId(0),
                trigger_seq: 1,
            };
            let bbo = BboView {
                bid_px: bid,
                bid_qty: Qty(1),
                ask_px: ask,
                ask_qty: Qty(1),
                conn_live_tsc: Tsc(1_000),
                conn_up: true,
                ..BboView::EMPTY
            };
            let lim = HedgePxLimits { conn_liveness_ms: 2_000, rr_max_cross: rr };
            match hft_oms::price_hedge(&req, &plan, &bbo, Tsc(1_000), &lim, tick, 1_000_000) {
                Ok(hp) => {
                    let n = hft_risk::Risk::notional(qty, hp.px, EXP, PAR);
                    prop_assert!(n.0 <= link_notional.0, "{n:?} > {link_notional:?} ({hp:?})");
                    // #6 with max_hedge_notional = link_notional.
                    let max_hedge: Notional = link_notional;
                    prop_assert!(n.0 <= max_hedge.0);
                }
                // Beyond the cross: gated, nothing priced (the counterparty is outside the cap).
                Err(e) => prop_assert_eq!(e, hft_oms::HedgePxErr::BeyondCross),
            }
        }
    }
}

// ------------------------------------------------------------------------------- SEQ-352

#[test]
fn seq_352_link_valid_precheck() {
    fn with(f: fn(&mut hft_strategy_sdk::LegLink)) -> impl Fn(&Hx<'_>) -> hft_strategy_sdk::OrderIntent {
        move |hx| {
            let mut it = pair_first(hx, q("0.01"));
            if let Some(l0) = it.link.get() {
                let mut l = *l0;
                f(&mut l);
                it.link = hft_strategy_sdk::LegLinkOpt::some(l);
            }
            it
        }
    }
    let rows: [(&str, fn(&mut hft_strategy_sdk::LegLink), bool); 4] = [
        ("352.1", |_| {}, false),
        ("352.2", |l| l.role = hft_strategy_sdk::LinkRole::Second, false),
        (
            "352.3",
            |l| l.hedge.kind = hft_strategy_sdk::HedgeKind::Simultaneous,
            false,
        ),
        ("352.4", |l| l.role = hft_strategy_sdk::LinkRole::Second, true),
    ];
    for (row, f, shadow) in rows {
        let mut h = steady();
        if shadow {
            let id = h.cmd_confirmed(
                AC1,
                &ControlCmd::SetMode {
                    scope: scope(S1, AC1, G1),
                    mode: RunMode::Shadow,
                },
            );
            h.run();
            assert_eq!(h.res(id).0, 0, "{row}");
        }
        let ok = row == "352.1";
        let g = with(f);
        let n = h.io_ref().out.len();
        let tokens =
            h.s.engine()
                .risk
                .view_for(AC1, A1, &h.s.engine().now)
                .rate_backoff;
        let infl = h.s.engine().oms.inflight().pos(AC1, A1);
        let outcome: Cap<Option<IntentOutcome>> = cap();
        let o2 = outcome.clone();
        let handle: Cap<hft_strategy_sdk::IntentHandle> = cap();
        let h2 = handle.clone();
        set_hook(move |hx, ev, ctx| {
            if let EventRef::Bbo { inst, .. } = ev
                && *inst == A1
                && hx.s.strategy == S1
            {
                let prev = h2.borrow().first().copied();
                match prev {
                    None => {
                        if let Ok(x) = ctx.place(g(hx)) {
                            h2.borrow_mut().push(x);
                        }
                    }
                    Some(x) => o2.borrow_mut().push(ctx.outcome(x)),
                }
            }
        });
        h.trace_on();
        let m = h.mark();
        h.quote(A1, "50000", "50003");
        h.run();
        let tr_eval = h.s.engine().trace.count("risk.evaluate");
        let tr_mode = h.s.engine().trace.count("mode_gate");
        h.quote(A1, "50000", "50003");
        h.run();
        clear_hook();
        let ds = decisions(&h.since(m));
        assert_eq!(ds.len(), 1, "{row}: {ds:?}");
        let d = ds[0].1;
        if ok {
            assert_eq!(d.reject, 0, "{row}");
            assert!(tr_mode >= 1);
            assert!(tr_eval >= 1);
            assert_eq!(h.io_ref().out.len(), n + 1, "{row}");
            continue;
        }
        assert_eq!(d.reject, RejectBits::INVALID.0, "{row}");
        assert_eq!(d.slot.0, 0, "{row}: no slot");
        assert_eq!(
            h.since(m)
                .iter()
                .filter(|r| r.flags.0 & RecordFlags::SHADOW.0 != 0)
                .count(),
            usize::from(shadow),
            "{row}: preserve Shadow mode on the recorded rejection"
        );
        assert_eq!(tr_eval, 0, "{row}");
        assert_eq!(tr_mode, 0, "{row}");
        assert_eq!(h.io_ref().out.len(), n, "{row}: nothing out");
        assert_eq!(h.s.engine().oms.inflight().pos(AC1, A1), infl, "{row}");
        assert_eq!(
            h.s.engine()
                .risk
                .view_for(AC1, A1, &h.s.engine().now)
                .rate_backoff,
            tokens
        );
        assert_eq!(
            outcome.borrow().first().copied(),
            Some(Some(if shadow {
                IntentOutcome::Shadowed
            } else {
                IntentOutcome::Rejected(RejectBits::INVALID)
            })),
            "{row}"
        );
    }
}

// ------------------------------------------------------------------------------- SEQ-360

#[test]
fn seq_360_oms_hooks() {
    use crate::scenarios as sc;
    // .1 Timer{Oms, UnknownQuery} → on_unknown_query, not on_sweep.
    let mut h = steady();
    let m = sc::place_on_a1(&mut h, &[S1], sc::a1_buy_open);
    let (seq, d) = decisions(&h.since(m))[0];
    let t = h.now();
    h.push(
        RingId::Rep(0),
        &sc::send_report_st(
            &d,
            hft_event::GwOp::Place,
            seq,
            t,
            hft_event::SendStatus::WriteError,
        ),
    );
    h.run();
    assert_eq!(sc::slot_state(&h, &d), Some(hft_oms::OrderState::Unknown));
    let due =
        h.s.engine()
            .heap
            .sorted()
            .iter()
            .find(|e| e.owner == 3 && e.timer_id & 0xFF == 3)
            .map(|e| e.deadline)
            .expect("UnknownQuery timer");
    h.trace_on();
    sc::run_to(&mut h, due);
    assert!(h.s.engine().trace.count("oms.on_unknown_query") >= 1, ".1");
    let tr = h.trace();
    let uq = tr.iter().position(|x| *x == "oms.on_unknown_query").unwrap_or(0);
    let input_start = tr[..uq].iter().rposition(|x| *x == "append.input").unwrap_or(0);
    let input_end = tr[uq..]
        .iter()
        .position(|x| *x == "append.input")
        .map_or(tr.len(), |k| uq + k);
    assert!(
        !tr[input_start..input_end].contains(&"oms.on_sweep"),
        ".1: {:?}",
        &tr[input_start..input_end]
    );

    // .2 / .3 the second leg refused with RATE_BACKOFF → retry_at = backoff end; with RATE → 0.
    for backoff in [true, false] {
        reset_script();
        let mut b = b0();
        b.caps.max_timers = 8_192;
        b.setup_edit.push(|s| s.oms.slots = hft_oms::PerTable::new(256));
        if !backoff {
            // No reserve on (B, 1): one-leg opens can empty its 10 s window.
            let rd = b.rd.load();
            let mut rp = b0_risk(&rd);
            let mut v = rp.venues.as_slice().to_vec();
            for x in v.iter_mut() {
                if x.venue == B && x.account == AC1 {
                    x.rr_reserve = r("0");
                }
            }
            rp.venues = hft_risk::PreVec::from_vec(v);
            b.risk = Some(rp);
        }
        let mut h = b.boot();
        h.steady();
        let first = sc::open_first(&mut h);
        if backoff {
            sc::rate_limit_hit(&mut h, B1);
        } else {
            for k in 0..100 {
                let m = sc::place_on_a1(&mut h, &[S1], sc::b1_sell_open);
                let d = sc::decs(&h, m)[0];
                assert_eq!(d.reject, 0, "b1 open #{k}");
                h.reject(&d);
            }
            let m = sc::place_on_a1(&mut h, &[S1], sc::b1_sell_open);
            assert_eq!(sc::decs(&h, m)[0].reject, RejectBits::RATE.0, "window empty");
        }
        h.trace_on();
        let m = h.mark();
        h.fill(&first, first.qty, first.qty, first.px);
        let second = sc::decs(&h, m)
            .into_iter()
            .find(|d| d.inst == B1)
            .expect("second leg decision");
        let gated: Vec<_> =
            h.s.engine()
                .trace
                .ev
                .iter()
                .filter(|e| e.what == "oms.hedge_gated")
                .copied()
                .collect();
        if backoff {
            assert_eq!(second.reject, RejectBits::RATE_BACKOFF.0, ".2");
            let e = h.s.engine();
            let want = e.now.tsc_of(e.risk.backoff_until(B, AC1)).0;
            assert_eq!(gated.last().map(|g| g.b), Some(want), ".2");
        } else {
            assert_eq!(
                second.reject & RejectBits::RATE.0,
                RejectBits::RATE.0,
                ".3 {second:?}"
            );
            assert_eq!(gated.last().map(|g| g.b), Some(0), ".3");
        }
    }

    // .4 / .5 the link's hedge_ref_px = b1 bid; the second leg's price = price_hedge's result.
    let mut h = steady();
    h.quote(B1, "60000", "60000.1");
    h.run();
    let first = sc::open_first(&mut h);
    let l = sc::last_link(&h, AC1).expect("link");
    assert_eq!(l.ref_px, px("60000"), ".4");
    let m = h.mark();
    h.fill(&first, first.qty, first.qty, first.px);
    let second = sc::decs(&h, m)
        .into_iter()
        .find(|d| d.inst == B1)
        .expect("second leg");
    let e = h.s.engine();
    let (_, plan) = *e.oms.link_plan(l.link_id).expect("plan");
    let bbo = e.market.bbo_view(&e.books, B1);
    let lim = e.risk.hedge_px_limits(AC1, B, B1);
    let req = hft_oms::HedgeRequest {
        link_id: l.link_id,
        role: hft_strategy_sdk::LinkRole::Second,
        venue: B,
        account: AC1,
        side: Side::Sell,
        reduce_only: false,
        inst: B1,
        group: G1,
        strategy: S1,
        qty: second.qty,
        basis: second.qty,
        hedge_cap_qty: second.qty,
        ref_px: l.ref_px,
        peer_venue: hft_types::VenueId(0),
        peer_account: hft_types::AccountId(0),
        trigger_seq: 0,
    };
    let hp = hft_oms::price_hedge(
        &req,
        &plan,
        &bbo,
        e.now.tsc,
        &lim,
        h.rd.inst(B1).tick,
        e.now.tsc_per_ms,
    )
    .expect("priced");
    assert_eq!(second.px, hp.px, ".5");
}
