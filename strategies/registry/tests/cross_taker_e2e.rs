//! Real strategy + sequencer + SimExchange; no test strategy substitutes for CrossTaker.
#![cfg(feature = "cross-taker")]
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)]
use cross_taker::XtParams;
use hft_event::{Bbo, GwCmd, GwOp, MarketHdr, OrderDecision};
use hft_strategy_registry::Strategies;
use hft_strategy_sdk::testing::MockViews;
use hft_strategy_sdk::{
    params::{RawParams, encode_body},
    prelude::*,
};
use hft_testkit::dst::{
    SeqSut, SimMatcher,
    sut::{BootCtx, InputRec, Source, StepOut, Sut},
    venue_behavior::{Matcher, rep_input},
    world::{ACCT_A, G1, P1, S1, VP, WorldSpec},
};
use zerocopy::FromZeros;

fn world() -> WorldSpec {
    let mut w = WorldSpec::w1();
    w.accounts.truncate(1);
    w.groups.truncate(1);
    w.strategies.truncate(1);
    // Use the configured spot and USDT-perpetual venues, without venue branches in the strategy.
    let old_spot = w.insts.iter().find(|i| i.id == S1).unwrap().venue;
    let remap = |id: VenueId| {
        if id == VP {
            VenueId(1)
        } else if id == old_spot {
            VenueId(11)
        } else {
            id
        }
    };
    for v in &mut w.venues {
        v.id = remap(v.id);
    }
    w.venues.sort_by_key(|v| v.id.0);
    for i in &mut w.insts {
        i.venue = remap(i.venue);
    }
    for a in &mut w.accounts {
        for v in &mut a.venues {
            *v = remap(*v);
        }
        for (v, _, _) in &mut a.balances {
            *v = remap(*v);
        }
    }
    w.groups[0].first = P1;
    w.groups[0].second = S1;
    let raw = RawParams {
        defaults: vec![
            1,
            1_000_000,
            0,
            500_000_000,
            120_000_000,
            240_000_000,
            1_000,
            10,
            3,
            0,
        ],
        overrides: vec![],
    };
    w.strategies[0].kind_id = 3;
    w.strategies[0].params = encode_body(&XtParams::SCHEMA, 1, &raw).unwrap();
    w
}

struct Rig {
    sut: SeqSut<Strategies>,
    venues: Vec<(VenueId, SimMatcher)>,
    now: u64,
    sent: Vec<GwCmd>,
    update: u64,
}
impl Rig {
    fn new(w: &WorldSpec) -> Self {
        let sut = SeqSut::boot(&BootCtx {
            seed: 7,
            world: w,
            planted: &[],
            tsc_hz: 1_000_000_000,
        })
        .unwrap();
        Self {
            sut,
            venues: w
                .venues
                .iter()
                .map(|v| (v.id, SimMatcher::new(w, v.id, 7)))
                .collect(),
            now: 10_000_000,
            sent: vec![],
            update: 100,
        }
    }
    fn feed(&mut self, inst: InstId, bid: i64, ask: i64, depth: i64) {
        let venue = self.sut.world().inst(inst).unwrap().venue;
        self.venues.iter_mut().find(|v| v.0 == venue).unwrap().1.on_bbo(
            inst,
            (bid, depth),
            (ask, depth),
            self.now,
        );
        self.update += 1;
        let mut h = MarketHdr::new_zeroed();
        h.venue = venue;
        h.inst = inst;
        h.rx_tsc = self.now;
        let b = Bbo {
            h,
            bid_px: Px(bid),
            ask_px: Px(ask),
            bid_qty: Qty(depth),
            ask_qty: Qty(depth),
            update_id: self.update,
        };
        let mut out = StepOut::default();
        self.sut
            .process(self.now, self.now, &InputRec::of(&b, Source::Market), &mut out);
        self.frames(&out);
        self.pump();
    }
    fn frames(&mut self, out: &StepOut) {
        if self.sent.is_empty() && !out.frames.is_empty() {
            assert_eq!(
                out.frames.len(),
                2,
                "both legs leave in one event before any reply"
            );
            assert!(out.frames.iter().all(|f| f.cmd.op == GwOp::Place));
        }
        for f in &out.frames {
            self.sent.push(f.cmd);
            self.venues
                .iter_mut()
                .find(|v| v.0 == self.sut.world().inst(f.cmd.inst).unwrap().venue)
                .unwrap()
                .1
                .on_command(&f.cmd, self.now);
        }
    }
    fn pump(&mut self) {
        let mut quiet = 0;
        for _ in 0..100 {
            // SimExchange queues entry arrivals separately from next_due (reply deadlines).
            self.now += 2_000_000;
            let mut replies = vec![];
            for (id, venue) in &mut self.venues {
                let mut r = vec![];
                venue.drain_due(self.now, &mut r);
                replies.extend(r.into_iter().map(|r| (*id, r)));
            }
            quiet = if replies.is_empty() { quiet + 1 } else { 0 };
            if quiet >= 3 {
                return;
            }
            for (venue, r) in replies {
                let mut out = StepOut::default();
                self.sut.process(
                    self.now,
                    self.now,
                    &rep_input(&r.rep, Source::Venue(venue)),
                    &mut out,
                );
                self.frames(&out);
            }
        }
        panic!("matcher did not settle");
    }
    fn positions(&self) -> (i64, i64) {
        let e = self.sut.sequencer().engine();
        (e.ledger.pos(ACCT_A, S1).qty.0, e.ledger.pos(ACCT_A, P1).qty.0)
    }
}

#[test]
fn cross_taker_reverse_spread_never_opens_spot_short() {
    let mut r = Rig::new(&world());
    r.feed(S1, 606_000_000, 606_001_000, 200_000_000);
    r.feed(P1, 599_999_900, 600_000_000, 500_000_000);
    assert!(r.sent.is_empty());
    assert_eq!(r.positions(), (0, 0));
    assert!(r.sut.verify_journal().unwrap().is_some());
}

#[test]
fn cross_taker_trades_both_legs_obeys_caps_and_replays() {
    let w = world();
    let mut r = Rig::new(&w);
    r.feed(S1, 599_999_900, 600_000_000, 200_000_000);
    r.feed(P1, 606_000_000, 606_001_000, 500_000_000);
    let (a, b) = r.positions();
    let decisions: Vec<_> = r
        .sut
        .journal()
        .iter()
        .filter_map(|j| j.try_read::<OrderDecision>())
        .collect();
    let reports: Vec<_> = r
        .sut
        .journal()
        .iter()
        .filter_map(|j| j.try_read::<hft_event::OrderEvent>())
        .collect();
    assert!(
        a > 0,
        "no trade: {decisions:?}; reports={reports:?}; sent={:?}",
        r.sent
    );
    assert_eq!(a, -b, "both actual fills must hedge");
    assert_eq!(a, 100_000, "both-leg 120 quote cap rounded down to common lot");
    assert_eq!(r.sent.iter().filter(|c| c.op == GwOp::Place).count(), 2);
    assert_eq!(
        r.sut.sequencer().engine().oms.legs(ACCT_A, G1).view.naked_qty,
        Qty(0)
    );
    for _ in 0..5 {
        r.now += 20_000_000;
        r.feed(S1, 599_999_900, 600_000_000, 200_000_000);
    }
    assert_eq!(
        r.positions(),
        (300_000, -300_000),
        "per-symbol position cap on both legs"
    );
    let sent = r.sent.iter().filter(|c| c.op == GwOp::Place).count();
    assert_eq!(sent, 6, "no over-cap first leg leaves the sequencer");
    assert_eq!(r.sut.rebuilt_hashes(), Ok(Some(r.sut.hashes())));
    assert!(r.sut.verify_journal().unwrap().is_some());
    // Reverse the executable spread: inventory is sold, the short perp bought back.
    r.now += 20_000_000;
    r.feed(P1, 593_999_000, 594_000_000, 500_000_000);
    for _ in 0..4 {
        r.now += 20_000_000;
        r.feed(S1, 599_999_900, 600_000_000, 200_000_000);
    }
    assert_eq!(r.positions(), (0, 0), "round trip ends flat; no spot short");
    assert_eq!(r.sut.rebuilt_hashes(), Ok(Some(r.sut.hashes())));
}

#[test]
fn cross_taker_link_respects_engine_hedge_limit_before_first_leg() {
    let mut w = world();
    w.risk.max_hedge_notional = 50_000_000;
    let mut r = Rig::new(&w);
    r.feed(S1, 599_999_900, 600_000_000, 200_000_000);
    r.feed(P1, 606_000_000, 606_001_000, 500_000_000);
    assert!(r.sent.is_empty(), "unaffordable hedge must prevent the first leg");
    assert_eq!(r.positions(), (0, 0));
    assert!(
        r.sut
            .journal()
            .iter()
            .filter_map(|j| j.try_read::<OrderDecision>())
            .any(|d| d.reject & RejectBits::HEDGE_LIMIT.0 != 0)
    );
    assert!(r.sut.verify_journal().unwrap().is_some());
}

#[test]
fn cross_taker_dst_reverses_rejected_pair_and_is_deterministic() {
    use hft_testkit::dst::{
        DstConfig, Fault, MarketGen, MarketSource, MockVenueCfg, RerunPolicy, run_record,
    };
    for seed in [7, 19, 41] {
        let mut cfg = DstConfig::new(seed, "cross-taker");
        cfg.world = world();
        let perp = cfg.world.insts.iter_mut().find(|i| i.id == P1).unwrap();
        perp.bid = 606_000_000;
        perp.ask = 606_001_000;
        cfg.venues = cfg.world.venues.iter().map(|v| MockVenueCfg::w1(v.id)).collect();
        cfg.steps = 500;
        cfg.horizon_ns = 5_000_000_000;
        cfg.market = MarketSource::Generated(MarketGen {
            tick_ms: 50,
            walk_centi_bp: 0,
        });
        cfg.actors.ctl = false;
        cfg.actors.advisor = false;
        cfg.scripted
            .push((1, Fault::SecondLegReject { venue: VenueId(1) }));
        cfg.rerun = RerunPolicy::Always;
        cfg.rebuild_every = 50;
        let r = run_record::<SeqSut<Strategies>>(&cfg);
        assert!(r.outcome.is_pass(), "seed {seed}: {:?}", r.outcome.failure());
        assert!(r.journal_verification.is_some());
        assert!(
            r.compared[6] > 0 && r.compared[9] > 0,
            "replay and rerun must execute"
        );
        assert!(
            r.matched_tail
                .iter()
                .any(|(_, e)| e.inst == S1 && e.cum_filled.0 > 0)
        );
        assert!(
            r.matched_tail
                .iter()
                .any(|(_, e)| e.inst == P1 && e.cum_filled.0 > 0)
        );
        let reversed = r
            .frames
            .iter()
            .filter(|(_, f)| f.cmd.inst == S1 && f.cmd.op == GwOp::Place && f.cmd.side == Side::Sell)
            .count();
        assert!(reversed > 0, "the rejected pair must unwind the filled spot leg");
    }
}

fn mock() -> (
    cross_taker::CrossTaker,
    hft_strategy_sdk::testing::MockViews,
    GroupTable,
    XtParams,
) {
    use hft_strategy_sdk::testing::RefFixture;
    use hft_testkit::dst::seq::build;
    let w = world();
    let rd = build::load_refdata(&build::refdata_blob(&w, false).unwrap()).unwrap();
    let leg = |i| LegDef {
        inst: i,
        venue: rd.inst(i).venue,
        kind: LegDef::CEX,
        _pad: 0,
        route: 0,
        _pad2: 0,
    };
    let g = GroupSpec {
        id: G1,
        asset: rd.inst(S1).asset,
        n_legs: 2,
        open_allowed: 1,
        legs: [leg(P1), leg(S1), LegDef::new_zeroed(), LegDef::new_zeroed()],
        _rsv: 0,
    };
    let groups = GroupTable::new(&[g], 4);
    let p = XtParams::decode(&w.strategies[0].params, 2, &groups).unwrap();
    let strategy = cross_taker::CrossTaker::new(
        StrategyInstance {
            strategy: StrategyId(1),
            account: ACCT_A,
            kind_id: 3,
        },
        &p,
        &groups,
        &RefView::new(&rd, ACCT_A),
    )
    .unwrap();
    let mut v = MockViews::new(RefFixture::new(rd, ACCT_A));
    v.at(Now::new(
        Tsc(10_000_000),
        20,
        1,
        true,
        EngineClockAnchor {
            base_engine_ns: 0,
            base_tsc: Tsc(0),
            tsc_hz_milli: 1_000_000_000_000,
            utc_tsc: Tsc(0),
            utc_ns: hft_testkit::dst::world::UTC0,
        },
    ));
    for (i, bid, ask) in [(S1, 599_999_900, 600_000_000), (P1, 606_000_000, 606_001_000)] {
        let venue = Views::inst(&v, i).venue;
        MockViews::bbo(
            &mut v,
            i,
            (Px(bid), Qty(200_000_000)),
            (Px(ask), Qty(200_000_000)),
        );
        v.conn(i, Tsc(10_000_000), true, false);
        MockViews::venue_ready(&mut v, ACCT_A, venue, true);
    }
    MockViews::risk(
        &mut v,
        ACCT_A,
        RiskStateView {
            conn_liveness_ms: 1000,
            ..RiskStateView::EMPTY
        },
    );
    MockViews::mode(&mut v, ACCT_A, G1, RunMode::Live);
    (strategy, v, groups, p)
}

#[test]
fn cross_taker_guards_age_connections_modes_unknown_and_cooldown() {
    use hft_strategy_sdk::testing::CtxHarness;
    let (mut s, mut v, _, _) = mock();
    let mut ctx = CtxHarness::new(&cross_taker::CrossTaker::DESC);
    let fire =
        |s: &mut cross_taker::CrossTaker, v: &hft_strategy_sdk::testing::MockViews, ctx: &mut CtxHarness| {
            ctx.run(|ctx| {
                s.on_event(
                    &EventRef::Bbo {
                        inst: P1,
                        gix: 0,
                        b: Views::bbo(v, P1),
                    },
                    v,
                    ctx,
                )
            })
        };
    let profitable = Views::bbo(&v, P1);
    let mut before = SnapshotWriter::new(1024);
    s.snapshot(&mut before);
    for (bid, ask) in [(599_999_900, 600_000_000), (600_000_000, 600_000_100)] {
        MockViews::bbo(
            &mut v,
            P1,
            (Px(bid), profitable.bid_qty),
            (Px(ask), profitable.ask_qty),
        );
        assert!(fire(&mut s, &v, &mut ctx).places.is_empty());
        let mut after = SnapshotWriter::new(1024);
        s.snapshot(&mut after);
        assert_eq!(after.as_bytes(), before.as_bytes(), "miss changed strategy state");
    }
    MockViews::bbo(
        &mut v,
        P1,
        (profitable.bid_px, profitable.bid_qty),
        (profitable.ask_px, profitable.ask_qty),
    );
    v.bbo_rx(S1, Tsc(0), 0, RxTsSrc::None);
    assert!(fire(&mut s, &v, &mut ctx).places.is_empty());
    v.bbo_rx(S1, Tsc(10_000_000), 0, RxTsSrc::None);
    v.at_tsc(Tsc(1_010_000_000));
    for i in [S1, P1] {
        v.conn(i, Tsc(1_010_000_000), true, false);
    }
    assert!(
        fire(&mut s, &v, &mut ctx).places.is_empty(),
        "age equality rejects despite healthy pongs"
    );
    for i in [S1, P1] {
        v.bbo_rx(i, Tsc(1_010_000_000), 0, RxTsSrc::None);
    }
    v.conn(S1, Tsc(1_010_000_000), false, false);
    assert!(fire(&mut s, &v, &mut ctx).places.is_empty());
    v.conn(S1, Tsc(1_010_000_000), true, false);
    MockViews::mode(&mut v, ACCT_A, G1, RunMode::CloseOnly);
    assert!(fire(&mut s, &v, &mut ctx).places.is_empty());
    MockViews::mode(&mut v, ACCT_A, G1, RunMode::Live);
    MockViews::legs(
        &mut v,
        ACCT_A,
        G1,
        LegView {
            has_unknown: true,
            ..LegView::EMPTY
        },
    );
    assert!(fire(&mut s, &v, &mut ctx).places.is_empty());
    MockViews::legs(&mut v, ACCT_A, G1, LegView::EMPTY);
    let r = fire(&mut s, &v, &mut ctx);
    assert_eq!(r.places.len(), 1);
    let it = r.places[0];
    assert_eq!(it.tif, Tif::Ioc);
    assert_eq!(it.px, Px(600_000_000));
    let hedge = it.link.l.hedge;
    assert_eq!(hedge.kind, HedgeKind::Simultaneous);
    assert_eq!(hedge.on_fail, OnFail::Reverse);
    assert_eq!(hedge.px_mode, PxMode::Limit);
    assert_eq!(hedge.tif, Tif::Ioc);
    assert_eq!(hedge.limit_px, Px(606_000_000));
    assert!(fire(&mut s, &v, &mut ctx).places.is_empty());
    let mut bytes = SnapshotWriter::new(1024);
    s.snapshot(&mut bytes);
    let (mut restored, _, _, _) = mock();
    restored
        .restore(&mut SnapshotReader::with_desc_version(bytes.as_bytes(), 1))
        .unwrap();
    v.restart(Tsc(100), 1000); // engine_ns survives a reset of the hardware counter
    for i in [S1, P1] {
        v.bbo_rx(i, Tsc(100), 0, RxTsSrc::None);
        v.conn(i, Tsc(100), true, false);
    }
    assert!(
        fire(&mut restored, &v, &mut ctx).places.is_empty(),
        "restored cooldown"
    );
}

#[test]
fn cross_taker_halts_on_repeated_hedge_failures_and_keeps_latch() {
    use hft_strategy_sdk::testing::CtxHarness;
    let (mut s, mut v, groups, p) = mock();
    let mut ctx = CtxHarness::new(&cross_taker::CrossTaker::DESC);
    let ev = EventRef::Bbo {
        inst: P1,
        gix: 0,
        b: Views::bbo(&v, P1),
    };
    for slot in 0..3 {
        MockViews::legs(&mut v, ACCT_A, G1, LegView::EMPTY);
        v.at_tsc(Tsc(10_000_000 + u64::from(slot) * 20_000_000));
        let fired = ctx.run(|c| s.on_event(&ev, &v, c));
        assert_eq!(fired.places.len(), 1);
        let first = fired.places[0].venue;
        let second = fired.places[0].link.l.hedge.venue;
        MockViews::legs(
            &mut v,
            ACCT_A,
            G1,
            LegView {
                open_links: 1,
                ..LegView::EMPTY
            },
        );
        let u = OrderUpdate {
            r: OrderRef::new(if slot % 2 == 0 { first } else { second }, ACCT_A, slot, 1),
            group: G1,
            tag: if slot % 2 == 0 { 1 } else { 0 },
            kind: OrderUpdateKind::Rejected {
                bits: RejectBits(0),
                venue_code: 1,
            },
        };
        let r = ctx.run(|c| s.on_event(&EventRef::Order { gix: 0, u: &u }, &v, c));
        assert_eq!(r.halts.len(), usize::from(slot == 2));
        assert!(
            ctx.run(|c| s.on_event(&EventRef::Order { gix: 0, u: &u }, &v, c))
                .halts
                .is_empty()
        );
        let other = OrderUpdate {
            r: OrderRef::new(if slot % 2 == 0 { second } else { first }, ACCT_A, slot, 1),
            tag: if slot % 2 == 0 { 0 } else { 1 },
            ..u
        };
        let duplicate_pair = ctx.run(|c| s.on_event(&EventRef::Order { gix: 0, u: &other }, &v, c));
        assert!(duplicate_pair.halts.is_empty());
        assert!(
            duplicate_pair.notes.is_empty(),
            "a pair counts once, including its other leg"
        );
    }
    s.on_params(&p, &groups, &v.refview()).unwrap(); // parameter replacement cannot erase the latch
    let mut bytes = SnapshotWriter::new(1024);
    s.snapshot(&mut bytes);
    let (mut restored, _, _, _) = mock();
    restored
        .restore(&mut SnapshotReader::with_desc_version(bytes.as_bytes(), 1))
        .unwrap();
    MockViews::legs(&mut v, ACCT_A, G1, LegView::EMPTY);
    v.at_tsc(Tsc(70_000_000));
    assert!(ctx.run(|c| restored.on_event(&ev, &v, c)).places.is_empty());
    restored.on_event(
        &EventRef::Mode {
            gix: 0,
            old: RunMode::CloseOnly,
            new: RunMode::Live,
        },
        &v,
        &mut CtxBufs::new().ctx(),
    );
    assert_eq!(
        ctx.run(|c| restored.on_event(&ev, &v, c)).places.len(),
        1,
        "safe explicit mode resume"
    );
}
