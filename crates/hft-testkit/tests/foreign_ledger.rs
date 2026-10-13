//! The defensive cash scenario must perform real executions and reject incomplete evidence.
use hft_event::{
    EventKind, ExternalCause, ExternalFill, Note, NoteSource, OrderAction, OrderDecision, OrderEvent,
};
use hft_testkit::dst::seq::matcher::SimMatcher;
use hft_testkit::dst::venue_behavior::{Matcher, MockVenue, MockVenueCfg};
use hft_testkit::dst::world::{ACCT_A, P1, VP, WorldSpec};
use hft_testkit::dst::{Fault, InvariantSet, RerunPolicy, run_record};
use hft_testkit::scenarios::{DstEngine, foreign_ledger, suites};
use hft_types::{Qty, Side};
use zerocopy::{IntoBytes, TryFromBytes};

const MS: u64 = 1_000_000;

#[test]
fn tk_144_foreign_order_delivery_retains_original_settlement_and_identity() {
    for buy in [false, true] {
        for positive_trade_id in [false, true] {
            for raw_first in [false, true] {
                let mut cfg = MockVenueCfg::w1(VP);
                cfg.bbo_ping_ms = 0;
                cfg.latency_ns = (MS, MS);
                let mut v = MockVenue::<SimMatcher>::new(cfg, &WorldSpec::w1(), 1);
                v.tick(0);
                // Bootstrap account reports have their own response latency. Finish
                // their delivery before isolating the foreign-order delivery window.
                assert!(
                    v.tick(2 * MS)
                        .iter()
                        .all(|(_, i)| !matches!(i.kind, EventKind::ORDER_EVENT | EventKind::EXTERNAL_FILL))
                );
                assert!(
                    !v.truth(3 * MS)
                        .iter()
                        .find(|t| t.account == ACCT_A)
                        .unwrap()
                        .undelivered
                );
                let open = if buy { Side::Sell } else { Side::Buy };
                v.m.on_bbo(P1, (600_189_000, 1_000_000), (600_190_000, 1_000_000), 10 * MS);
                v.m.external(ACCT_A, P1, open, 200_000, "seed", false, 10 * MS);
                v.m.on_bbo(P1, (600_275_000, 1_000_000), (600_276_000, 1_000_000), 20 * MS);
                let rows = v.apply_fault(
                    &Fault::ForeignOrderFill {
                        account: ACCT_A,
                        inst: P1,
                        qty: 100_000,
                        buy,
                        positive_trade_id,
                        unresolved: true,
                        raw_first,
                    },
                    20 * MS,
                );
                assert!(v.fault_error().is_none());
                assert_eq!(rows.len(), 1 + usize::from(raw_first));
                let receipt = v.m.external_history().last().unwrap().fill;
                assert_eq!(receipt.cause, ExternalCause::Unknown);
                assert_eq!(receipt.exch_order_id, u64::MAX - 2);
                assert_eq!(receipt.trade_id, if positive_trade_id { 2 } else { 0 });
                assert_eq!(receipt.fill_seq, if positive_trade_id { 0 } else { 100_000 });
                assert_eq!(receipt.cash_qty, receipt.qty);
                assert_eq!(receipt.cash_amount, Qty(if buy { -8_600 } else { 8_600 }));
                let last = rows.last().unwrap();
                let order = OrderEvent::try_read_from_bytes(&last.1.body).unwrap();
                assert_eq!(order.exch_order_id, receipt.exch_order_id);
                assert_eq!(order.trade_id, receipt.trade_id);
                assert_eq!(order.cash_sum, receipt.cash_amount);
                assert_eq!(order.cum_filled, receipt.qty);
                assert_eq!(order.slot.0, 0);
                if raw_first {
                    assert!(rows[0].0 < last.0);
                    let raw = ExternalFill::try_read_from_bytes(&rows[0].1.body).unwrap();
                    assert_eq!(raw.as_bytes(), receipt.as_bytes());
                }
                assert!(
                    v.truth(last.0 - 1)
                        .iter()
                        .find(|t| t.account == ACCT_A)
                        .unwrap()
                        .undelivered
                );
                let after = v.m.truth(ACCT_A);
                assert!(
                    v.tick(last.0 + MS)
                        .iter()
                        .all(|(_, i)| !matches!(i.kind, EventKind::ORDER_EVENT | EventKind::EXTERNAL_FILL))
                );
                let delivered = v.m.truth(ACCT_A);
                // The matching clock advances while replies drain. Compare all financial
                // truth, not the clock-bearing state hash, to detect a second settlement.
                assert_eq!(delivered.positions, after.positions);
                assert_eq!(delivered.inventory, after.inventory);
                assert_eq!(delivered.balances, after.balances);
                assert_eq!(v.m.external_history().len(), 2);
                assert!(
                    !v.truth(last.0 + MS)
                        .iter()
                        .find(|t| t.account == ACCT_A)
                        .unwrap()
                        .undelivered
                );
            }
        }
    }
    for stream_down in [false, true] {
        let mut cfg = MockVenueCfg::w1(VP);
        cfg.bbo_ping_ms = 0;
        let mut v = MockVenue::<SimMatcher>::new(cfg, &WorldSpec::w1(), 1);
        if stream_down {
            v.apply_fault(
                &Fault::ConnDown {
                    venue: VP,
                    conn: hft_testkit::dst::ConnKind::UserStream,
                    down_ms: 10,
                    resync: false,
                },
                0,
            );
        }
        let before = v.m.state_hash();
        assert!(
            v.apply_fault(
                &Fault::ForeignOrderFill {
                    account: ACCT_A,
                    inst: P1,
                    qty: 1,
                    buy: true,
                    positive_trade_id: true,
                    unresolved: stream_down,
                    raw_first: true,
                },
                MS
            )
            .is_empty()
        );
        assert!(v.fault_error().is_some());
        assert_eq!(v.m.state_hash(), before);
        assert!(v.m.external_history().is_empty());
    }
}

#[test]
fn tk_145_foreign_case_requires_every_execution_and_exact_ledger_boundary() {
    let case = foreign_ledger::led_080_foreign_order_cold_hot_equal();
    for seed in 0..8 {
        let mut cfg = case.config(&suites(), seed).unwrap();
        cfg.rerun = RerunPolicy::Never;
        let r = case.run_config::<DstEngine>(&cfg);
        assert!(r.outcome.is_pass(), "seed {seed}: {:?}", r.outcome);
        for change in 0..9 {
            let mut bad = r.clone();
            match change {
                0 => bad.ledger_checks.clear(),
                1 => {
                    bad.external_receipts.pop();
                }
                2 => bad.external_receipts[0].fill.cash_amount.0 += 1,
                3 => bad.journal.retain(|j| j.kind != EventKind::EXEC_DECISION),
                4 => {
                    let before = bad.journal.len();
                    let paths = [
                        hft_oms::notes::code_of(hft_oms::notes::STALE_GEN_FILL),
                        hft_oms::notes::code_of(hft_oms::notes::FOREIGN_FILL_VIA_ORDER_EVENT),
                    ];
                    bad.journal.retain(|j| {
                        !j.try_read::<Note>()
                            .is_some_and(|n| n.source == NoteSource::Oms && paths.contains(&n.code))
                    });
                    assert!(bad.journal.len() < before, "no path evidence removed");
                }
                5 => bad.truth[0].undelivered = true,
                6 => bad.obs.balances[0].qty += 1,
                7 => {
                    bad.ledger_checks.push(bad.ledger_checks[0]);
                }
                _ => {
                    let mut extra = bad
                        .journal
                        .iter()
                        .find(|j| {
                            j.try_read::<OrderDecision>()
                                .is_some_and(|d| d.action == OrderAction::New && d.reject == 0)
                        })
                        .unwrap()
                        .clone();
                    extra.seq = bad.journal.last().unwrap().seq + 1;
                    extra.tsc = 8_000 * MS;
                    bad.journal.push(extra);
                }
            }
            assert!(
                foreign_ledger::assert_record(&bad).is_err(),
                "seed {seed}, mutation {change}"
            );
        }
        cfg.invariants = InvariantSet::none();
        let off = run_record::<DstEngine>(&cfg);
        assert!(off.outcome.is_pass(), "seed {seed}: {:?}", off.outcome);
        assert_eq!(r.journal, off.journal);
        assert_eq!(r.frames, off.frames);
        assert_eq!(r.deliveries, off.deliveries);
        assert_eq!(r.trail.to_bytes(), off.trail.to_bytes());
        assert_eq!(r.truth, off.truth);
    }
    let mut short = case.config(&suites(), 0).unwrap();
    short.steps = 1;
    assert!(!case.run_config::<DstEngine>(&short).outcome.is_pass());
}
