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
