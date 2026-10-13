//! Default stale-generation faults must create independent, exactly-once executions.
use hft_event::{EventKind, GwCmd, GwOp, Note, NoteSource, OrderEvent, OrderEventKind, RecordFlags};
use hft_testkit::dst::seq::matcher::SimMatcher;
use hft_testkit::dst::venue_behavior::{EXTERNAL_HISTORY_CAP, Matcher, MockVenue, MockVenueCfg, ToyMatcher};
use hft_testkit::dst::world::{ACCT_A, BTC, P1, S1, USDT, VP, VS, WorldSpec};
use hft_testkit::dst::{
    ConnKind, DstConfig, Fault, FaultRates, InvariantSet, RerunPolicy, RunRecord, run_record,
};
use hft_testkit::scenarios::{DstEngine, foreign_ledger, suites};
use hft_types::{InstId, Px, Qty, Side, SlotId, Tif, Tsc, VenueId};
use zerocopy::TryFromBytes;

const MS: u64 = 1_000_000;
const SECOND: u64 = 1_000 * MS;

pub(super) fn without_stale(cfg: &DstConfig) -> DstConfig {
    let mut cfg = cfg.clone();
    let mut plan = cfg.fault_plan();
    for (_, fault) in &mut plan.items {
        if let Fault::StaleGenFill { count, .. } = fault {
            *count = 0;
        }
    }
    cfg.plan = Some(plan);
    cfg
}

fn venue<M: Matcher>(world: &WorldSpec, id: VenueId, seed: u64) -> MockVenue<M> {
    let mut cfg = MockVenueCfg::w1(id);
    cfg.rate_per_s = 0;
    cfg.bbo_ping_ms = 0;
    cfg.latency_ns = (0, 0);
    MockVenue::new(cfg, world, seed)
}

fn place(inst: InstId, qty: i64) -> GwCmd {
    GwCmd {
        decision_seq: 1,
        decided_tsc: Tsc(0),
        px: Px(500_000_000),
        qty: Qty(qty),
        inst,
        slot: SlotId(1),
        r#gen: 3,
        target_slot: SlotId(0),
        op: GwOp::Place,
        account: ACCT_A,
        side: Side::Buy,
        tif: Tif::Gtc,
        oflags: 0,
        _pad: [0; 2],
        arg: 7,
        arg2: 0,
    }
}

fn ack(v: &mut MockVenue<SimMatcher>, inst: InstId, lot: i64) -> u64 {
    let rows = v.on_frame(&place(inst, lot), 10 * MS);
    rows.iter()
        .filter(|(_, input)| input.kind == EventKind::ORDER_EVENT)
        .find_map(|(at, input)| {
            OrderEvent::try_read_from_bytes(&input.body)
                .ok()
                .filter(|e| e.what == OrderEventKind::Ack)
                .map(|_| *at)
        })
        .expect("actual matcher Ack")
}

fn amount<K: Copy + PartialEq>(rows: &[(K, i128)], key: K) -> i128 {
    rows.iter().find(|(k, _)| *k == key).map_or(0, |(_, v)| *v)
}

#[test]
fn tk_160_default_stale_fills_preserve_original_settlement_and_scoped_identity() {
    for (venue_id, inst) in [(VS, S1), (VP, P1)] {
        let row = if inst == S1 { "TK-160.1" } else { "TK-160.2" };
        for seed in 0..4 {
            let mut world = WorldSpec::w1();
            world.accounts[0].balances.push((VS, BTC, 100_000_000));
            let spec = world.insts.iter().find(|i| i.id == inst).unwrap();
            let lot = spec.lot;
            let buy = seed & 1 == 0;
            let side = if buy { Side::Buy } else { Side::Sell };
            let mut v = venue::<SimMatcher>(&world, venue_id, seed);
            let due = ack(&mut v, inst, lot);
            v.tick(due);
            if inst == P1 {
                v.m.on_bbo(inst, (600_189_000, 10 * lot), (600_190_000, 10 * lot), 20 * MS);
                v.m.external(
                    ACCT_A,
                    inst,
                    if buy { Side::Sell } else { Side::Buy },
                    2 * lot,
                    "seed",
                    false,
                    20 * MS,
                );
            }
            v.m.on_bbo(inst, (600_275_000, 10 * lot), (600_276_000, 10 * lot), 30 * MS);
            let before = v.m.truth(ACCT_A);
            let rows = v.apply_fault(
                &Fault::StaleGenFill {
                    venue: venue_id,
                    count: 1,
                },
                30 * MS,
            );
            assert!(v.fault_error().is_none(), "{row} venue={venue_id:?} seed={seed}");
            let orders: Vec<_> = rows
                .iter()
                .filter(|(_, input)| input.kind == EventKind::ORDER_EVENT)
                .map(|(_, input)| OrderEvent::try_read_from_bytes(&input.body).unwrap())
                .collect();
            assert_eq!(orders.len(), 1);
            let order = orders[0];
            assert_eq!(
                (order.what, order.detail, order.venue),
                (OrderEventKind::Fill, side as u8, venue_id)
            );
            let fill = v.m.external_history().last().unwrap().fill;
            assert_eq!(fill.side, side);
            assert_eq!(fill.qty, Qty(lot));
            assert_eq!(fill.px, Px(600_275_000));
            assert_eq!(fill.fee, Qty(0));
            assert_eq!(fill.trade_id > 0, seed & 2 == 0);
            assert_eq!(fill.fill_seq, if seed & 2 == 0 { 0 } else { lot as u64 });
            assert_eq!(
                (order.account, order.inst, order.epoch, order.slot, order.r#gen),
                (ACCT_A, inst, 7, SlotId(1), 3 ^ (1 << 31))
            );
            assert_eq!(
                (
                    order.exch_order_id,
                    order.trade_id,
                    order.px,
                    order.qty,
                    order.cum_filled
                ),
                (fill.exch_order_id, fill.trade_id, fill.px, fill.qty, fill.qty)
            );
            assert_eq!(
                (order.cash_qty, order.cash_sum),
                (fill.cash_qty, fill.cash_amount)
            );
            let after = v.m.truth(ACCT_A);
            let sign = if buy { 1 } else { -1 };
            let cash = if inst == P1 {
                -sign * 8_600
            } else {
                -sign * i128::from(lot) * 600_275_000 / 1_000_000
            };
            assert_eq!(
                amount(&after.balances, USDT),
                amount(&before.balances, USDT) + cash
            );
            if inst == P1 {
                assert_eq!(fill.cash_amount.0 as i128, cash);
                assert_eq!(fill.cash_qty.0, lot);
                assert_eq!(
                    amount(&after.positions, inst),
                    amount(&before.positions, inst) + sign * i128::from(lot)
                );
            } else {
                assert_eq!(fill.cash_qty, Qty(0));
                assert_eq!(
                    amount(&after.inventory, BTC),
                    amount(&before.inventory, BTC) + sign * i128::from(lot)
                );
            }
            let proof = v.stale_gen_report();
            assert_eq!(
                (proof.requested, proof.pending, proof.executions.len()),
                (1, 0, 1)
            );
            assert_eq!(proof.executions[0].ack_ns, due);
            assert_eq!(proof.executions[0].order_id, fill.exch_order_id);
            let count = v.m.external_history().len();
            v.tick(50 * MS);
            let delivered = v.m.truth(ACCT_A);
            assert_eq!(delivered.balances, after.balances);
            assert_eq!(delivered.positions, after.positions);
            assert_eq!(delivered.inventory, after.inventory);
            assert_eq!(v.m.external_history().len(), count);
            assert_eq!(v.stale_gen_report(), proof);
        }
    }
}

#[test]
fn tk_161_stale_queue_waits_for_ack_and_stream_and_fails_before_invalid_execution() {
    let world = WorldSpec::w1();
    let mut v = venue::<SimMatcher>(&world, VP, 0);
    assert!(
        v.apply_fault(&Fault::StaleGenFill { venue: VP, count: 1 }, 0)
            .is_empty()
    );
    assert_eq!(v.stale_gen_report().pending, 1);
    assert!(v.m.external_history().is_empty());
    let at = ack(&mut v, P1, 100_000);
    v.tick(at - 1);
    assert!(v.m.external_history().is_empty(), "future Ack cannot qualify");
    v.apply_fault(
        &Fault::ConnDown {
            venue: VP,
            conn: ConnKind::UserStream,
            down_ms: 10,
            resync: false,
        },
        at - 1,
    );
    let up = at - 1 + 10 * MS;
    v.tick(up - 1);
    assert!(
        v.m.external_history().is_empty(),
        "disconnected stream cannot qualify"
    );
    assert_eq!(v.next_wake(up - 1), up);
    let rows = v.tick(up);
    assert_eq!(
        rows.iter()
            .filter(|(_, i)| i.kind == EventKind::ORDER_EVENT)
            .filter_map(|(_, i)| OrderEvent::try_read_from_bytes(&i.body).ok())
            .filter(|e| e.r#gen == 3 ^ (1 << 31))
            .count(),
        1
    );
    assert_eq!(v.stale_gen_report().pending, 0);
    assert_eq!(v.stale_gen_report().executions[0].matched_ns, up);

    for clean in [false, true] {
        let mut fresh = venue::<ToyMatcher>(&world, VP, 0);
        fresh.clean = clean;
        let before = fresh.m.state_hash();
        fresh.apply_fault(
            &Fault::StaleGenFill {
                venue: VP,
                count: if clean { 1 } else { 0 },
            },
            0,
        );
        assert_eq!(fresh.stale_gen_report().requested, 0);
        assert_eq!(fresh.m.state_hash(), before);
    }
    let mut full = venue::<ToyMatcher>(&world, VP, 0);
    full.apply_fault(
        &Fault::StaleGenFill {
            venue: VP,
            count: EXTERNAL_HISTORY_CAP as u32,
        },
        0,
    );
    assert_eq!(full.stale_gen_report().pending, EXTERNAL_HISTORY_CAP);
    let before = full.m.state_hash();
    full.apply_fault(&Fault::StaleGenFill { venue: VP, count: 1 }, 1);
    assert!(full.fault_error().unwrap().contains("capacity"));
    assert_eq!(full.stale_gen_report().pending, EXTERNAL_HISTORY_CAP);
    assert_eq!(full.m.state_hash(), before);
    let mut bad = world.clone();
    bad.insts.iter_mut().find(|i| i.id == P1).unwrap().lot = 0;
    let mut invalid = venue::<ToyMatcher>(&bad, VP, 0);
    let before = invalid.m.state_hash();
    invalid.apply_fault(&Fault::StaleGenFill { venue: VP, count: 1 }, 0);
    assert!(invalid.fault_error().unwrap().contains("positive"));
    assert_eq!(invalid.m.state_hash(), before);
    let before = v.m.state_hash();
    v.apply_fault(&Fault::StaleGenFill { venue: VP, count: 1 }, u64::MAX - 1);
    assert!(v.fault_error().unwrap().contains("virtual time"));
    assert_eq!(v.m.state_hash(), before);

    // The request was valid when armed; only its later execution discovers a missing quote.
    let mut delayed = venue::<SimMatcher>(&world, VP, 0);
    delayed.apply_fault(&Fault::StaleGenFill { venue: VP, count: 1 }, 0);
    let due = ack(&mut delayed, P1, 100_000);
    delayed.m.on_bbo(P1, (0, 0), (0, 0), due);
    let before = delayed.m.truth(ACCT_A);
    delayed.tick(due);
    assert!(delayed.fault_error().unwrap().contains("external order"));
    assert!(delayed.m.external_history().is_empty());
    assert_eq!(delayed.stale_gen_report().pending, 1);
    let after = delayed.m.truth(ACCT_A);
    assert_eq!(before.positions, after.positions);
    assert_eq!(before.balances, after.balances);
}

fn validate(r: &RunRecord) -> Result<(), &'static str> {
    let req = |ok, why| if ok { Ok(()) } else { Err(why) };
    req(r.stale_gen.len() == 2, "missing venue observation")?;
    req(r.external_receipts.len() == 4, "missing or extra originals")?;
    for report in &r.stale_gen {
        req(
            report.requested == 2 && report.pending == 0 && report.executions.len() == 2,
            "missing actual executions",
        )?;
        for trace in &report.executions {
            req(
                trace.ack_ns <= trace.matched_ns && trace.requested_ns <= trace.matched_ns,
                "future prerequisite",
            )?;
            let receipt = r
                .external_receipts
                .iter()
                .find(|r| r.fill.venue == report.venue && r.fill.exch_order_id == trace.order_id)
                .ok_or("missing original")?;
            let f = receipt.fill;
            req(
                r.journal
                    .iter()
                    .filter_map(|j| j.try_read::<OrderEvent>())
                    .any(|e| {
                        e.what == OrderEventKind::Ack
                            && e.venue == report.venue
                            && e.account == trace.account
                            && (e.inst, e.epoch, e.slot, e.r#gen, e.exch_order_id)
                                == (
                                    trace.inst,
                                    trace.epoch,
                                    trace.slot,
                                    trace.ack_gen,
                                    trace.ack_order_id,
                                )
                    }),
                "missing original Ack",
            )?;
            let orders: Vec<_> = r
                .journal
                .iter()
                .filter_map(|j| j.try_read::<OrderEvent>().map(|e| (j.seq, e)))
                .filter(|(_, e)| e.venue == report.venue && e.exch_order_id == trace.order_id)
                .collect();
            req(orders.len() == 1, "missing or repeated report")?;
            let (seq, order) = orders[0];
            req(
                (order.epoch, order.slot, order.r#gen)
                    == (trace.epoch, trace.slot, trace.ack_gen ^ (1 << 31)),
                "wrong stale identity",
            )?;
            req(
                (
                    order.account,
                    order.inst,
                    order.qty,
                    order.cum_filled,
                    order.px,
                    order.cash_qty,
                    order.cash_sum,
                    order.fee,
                    order.trade_id,
                ) == (
                    f.account,
                    f.inst,
                    f.qty,
                    f.qty,
                    f.px,
                    f.cash_qty,
                    f.cash_amount,
                    f.fee,
                    f.trade_id,
                ),
                "changed original facts",
            )?;
            req(r.ledger_checks.contains(&seq), "missing exact ledger comparison")?;
            let begin = r.journal.iter().position(|j| j.seq == seq).unwrap();
            let end = begin
                + r.journal[begin..]
                    .iter()
                    .position(|j| j.flags.contains(RecordFlags::EVENT_END))
                    .ok_or("incomplete event")?;
            let notes: Vec<_> = r.journal[begin..=end]
                .iter()
                .filter_map(|j| j.try_read::<Note>())
                .filter(|n| n.source == NoteSource::Oms)
                .collect();
            req(
                notes.iter().any(|n| {
                    n.code == hft_oms::notes::code_of(hft_oms::notes::STALE_GEN_FILL) && n.v[2] == 1
                }),
                "missing booked stale note",
            )?;
        }
    }
    req(
        r.truth.iter().all(|t| !t.undelivered && t.excluded.is_empty()),
        "truth still skipped",
    )?;
    Ok(())
}

#[test]
fn tk_162_real_stale_fills_keep_strict_wallets_replay_and_checkers_equal() {
    for seed in [0, 2] {
        let mut cfg = foreign_ledger::led_080_foreign_order_cold_hot_equal()
            .config(&suites(), seed)
            .unwrap();
        cfg.rerun = RerunPolicy::Never;
        cfg.scripted = [(12, VS), (14, VP), (16, VS), (18, VP)]
            .into_iter()
            .map(|(s, venue)| (s * SECOND, Fault::StaleGenFill { venue, count: 1 }))
            .collect();
        let on = run_record::<DstEngine>(&cfg);
        assert!(on.outcome.is_pass(), "{}", crate::seq_sut::show(&on));
        validate(&on).unwrap();
        assert!(on.journal_verification.is_some());
        for damage in 0..4 {
            let mut bad = on.clone();
            match damage {
                0 => bad.stale_gen.clear(),
                1 => {
                    bad.external_receipts.pop();
                }
                2 => bad.stale_gen[0].executions[0].ack_gen ^= 1,
                _ => {
                    let f = bad.external_receipts.last_mut().unwrap();
                    f.fill.cash_amount.0 += 1;
                }
            }
            assert!(validate(&bad).is_err(), "unchecked evidence damage {damage}");
        }
        cfg.invariants = InvariantSet::none();
        let off = run_record::<DstEngine>(&cfg);
        assert!(off.outcome.is_pass());
        assert!(on.journal == off.journal);
        assert_eq!(on.frames, off.frames);
        assert_eq!(on.deliveries, off.deliveries);
        assert_eq!(on.trail.to_bytes(), off.trail.to_bytes());
        assert_eq!(on.truth, off.truth);
        assert_eq!(on.stale_gen, off.stale_gen);
    }
}

#[test]
fn tk_164_framework_model_books_default_stale_fills_as_external_evidence() {
    let mut cfg = DstConfig::new(0, "core");
    cfg.steps = 2_500;
    cfg.faults = FaultRates::off();
    cfg.scripted = vec![
        (SECOND, Fault::StaleGenFill { venue: VS, count: 2 }),
        (SECOND, Fault::StaleGenFill { venue: VP, count: 2 }),
    ];
    let r = run_record::<hft_testkit::dst::toy::ToyEngine>(&cfg);
    assert!(
        r.outcome.is_pass(),
        "{:?}",
        r.outcome.failure().map(|f| (&f.invariant, &f.message))
    );
    assert_eq!(r.stale_gen.iter().map(|v| v.executions.len()).sum::<usize>(), 4);
}

#[test]
fn tk_165_stale_reports_share_transport_faults_without_repeating_settlement() {
    let world = WorldSpec::w1();
    let mut v = venue::<SimMatcher>(&world, VP, 0);
    let ack_due = ack(&mut v, P1, 100_000);
    v.tick(ack_due);
    v.m.on_bbo(P1, (600_189_000, 1_000_000), (600_190_000, 1_000_000), 20 * MS);
    v.m.external(ACCT_A, P1, Side::Sell, 200_000, "seed", false, 20 * MS);
    // Drain the seed receipt before arming delivery faults.
    v.tick(25 * MS);
    v.m.on_bbo(P1, (600_275_000, 1_000_000), (600_276_000, 1_000_000), 30 * MS);
    for fault in [
        Fault::GrayConn {
            venue: VP,
            dur_ms: 1_000,
            factor: 100,
        },
        Fault::ReportDup {
            venue: VP,
            dur_ms: 1_000,
        },
        Fault::NoClientIdFill {
            venue: VP,
            within_hold: false,
        },
    ] {
        assert!(v.apply_fault(&fault, 30 * MS).is_empty());
    }
    let originals = v.m.external_history().len();
    let before = v.m.truth(ACCT_A);
    let rows = v.apply_fault(&Fault::StaleGenFill { venue: VP, count: 1 }, 30 * MS);
    assert!(v.fault_error().is_none());
    assert_eq!(v.m.external_history().len(), originals + 1);
    let fill = v.m.external_history().last().unwrap().fill;
    assert_eq!(fill.cash_amount, Qty(-8_600));
    let orders: Vec<_> = rows
        .iter()
        .filter(|(_, input)| input.kind == EventKind::ORDER_EVENT)
        .map(|(at, input)| (*at, OrderEvent::try_read_from_bytes(&input.body).unwrap()))
        .collect();
    assert_eq!(orders.len(), 3);
    for (_, order) in &orders {
        assert_eq!(
            (
                order.exch_order_id,
                order.qty,
                order.cum_filled,
                order.px,
                order.cash_qty,
                order.cash_sum,
                order.fee,
                order.trade_id
            ),
            (
                fill.exch_order_id,
                fill.qty,
                fill.qty,
                fill.px,
                fill.cash_qty,
                fill.cash_amount,
                fill.fee,
                fill.trade_id
            )
        );
    }
    let unresolved: Vec<_> = orders
        .iter()
        .filter(|(_, e)| e.slot == SlotId::UNRESOLVED)
        .collect();
    assert_eq!(unresolved.len(), 1);
    assert_eq!(
        (unresolved[0].0, unresolved[0].1.epoch, unresolved[0].1.r#gen),
        (230 * MS, 0, 0)
    );
    let mut stale: Vec<_> = rows
        .iter()
        .filter(|(_, input)| input.kind == EventKind::ORDER_EVENT)
        .filter(|(_, input)| OrderEvent::try_read_from_bytes(&input.body).unwrap().r#gen == 3 ^ (1 << 31))
        .collect();
    stale.sort_by_key(|(at, _)| *at);
    assert_eq!(stale.len(), 2);
    assert_eq!((stale[0].0, stale[1].0), (230 * MS, 230 * MS + 1));
    assert_eq!(stale[0].1.body, stale[1].1.body);
    let report = v.stale_gen_report();
    assert_eq!(
        (report.requested, report.pending, report.executions.len()),
        (1, 0, 1)
    );
    assert_eq!(report.executions[0].delivered_ns, 230 * MS + 1);
    let settled = v.m.truth(ACCT_A);
    assert_eq!(
        amount(&settled.balances, USDT),
        amount(&before.balances, USDT) - 8_600
    );
    v.tick(300 * MS);
    let after = v.m.truth(ACCT_A);
    assert_eq!(after.positions, settled.positions);
    assert_eq!(after.balances, settled.balances);
    assert_eq!(v.m.external_history().len(), originals + 1);
}
