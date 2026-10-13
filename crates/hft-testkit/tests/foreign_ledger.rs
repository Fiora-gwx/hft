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
                // BBO/position changes also schedule margin reports. Drain their actual
                // response deadlines, then observe strictly after the final delivery.
                let mut delivered_at = last.0 + MS;
                for _ in 0..4 {
                    let drained = v.tick(delivered_at);
                    assert!(drained.iter().all(|(_, i)| {
                        !matches!(i.kind, EventKind::ORDER_EVENT | EventKind::EXTERNAL_FILL)
                    }));
                    delivered_at = delivered_at.max(drained.iter().map(|(at, _)| *at).max().unwrap_or(0));
                    let Some(next) = v.m.next_due() else { break };
                    delivered_at = delivered_at.max(next);
                }
                assert!(v.m.next_due().is_none(), "non-trading reports did not drain");
                let delivered = v.m.truth(ACCT_A);
                // The matching clock advances while replies drain. Compare all financial
                // truth, not the clock-bearing state hash, to detect a second settlement.
                assert_eq!(delivered.positions, after.positions);
                assert_eq!(delivered.inventory, after.inventory);
                assert_eq!(delivered.balances, after.balances);
                assert_eq!(v.m.external_history().len(), 2);
                assert!(
                    !v.truth(delivered_at + 1)
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
        assert!(r.outcome.is_pass(), "seed {seed}: {}", crate::seq_sut::show(&r));
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
        assert!(
            off.outcome.is_pass(),
            "seed {seed}: {}",
            crate::seq_sut::show(&off)
        );
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

#[test]
fn tk_147_external_dedup_observation_keeps_scope_and_key_domain() {
    use hft_testkit::dst::invariants::{InvId, InvState, StepView, checker};
    use hft_testkit::dst::obs::{Evidence, EvidenceKind, ExternalKey, Obs, StepInfo};
    use hft_testkit::dst::seq::observe::Before;
    use hft_testkit::dst::sut::{BootCtx, InputRec, JRec, Source, Sut};
    use hft_testkit::dst::world::{ACCT_B, VS};
    use hft_types::{InstId, Tsc};
    use zerocopy::FromZeros;

    fn check(state: &mut InvState, evidence: &[Evidence]) -> Result<(), String> {
        let info = StepInfo {
            evidence: evidence.to_vec(),
            ..StepInfo::default()
        };
        let obs = Obs::default();
        (checker(InvId::I19).unwrap().step)(
            state,
            &StepView {
                step: 1,
                now_ns: 1,
                records: &[],
                delivered: None,
                frames: &[],
                info: &info,
                before: &obs,
                after: &obs,
                truth: &[],
                rebuild: None,
            },
        )
    }

    let world = WorldSpec::w1();
    let engine = DstEngine::boot(&BootCtx {
        seed: 1,
        world: &world,
        planted: &[],
        tsc_hz: 1_000_000_000,
    })
    .unwrap();
    let mut fill = ExternalFill::new_zeroed();
    fill.account = ACCT_A;
    fill.venue = VP;
    fill.inst = P1;
    fill.exch_order_id = 900;
    fill.trade_id = 100;
    fill.fill_seq = 100;
    let positive = ExternalKey::Trade(100);
    let zero = ExternalKey::OrderSeq {
        exch_order_id: 900,
        fill_seq: 100,
    };
    let key = |x: &ExternalFill| Before::take(&engine, &InputRec::of(x, Source::Venue(VP))).ext_key;
    assert_eq!(key(&fill), Some(positive));
    // With a positive trade id, neither the order id nor a cumulative field may
    // create a fresh identity for a repeated execution.
    fill.exch_order_id = 901;
    fill.fill_seq = 101;
    assert_eq!(key(&fill), Some(positive));
    fill.exch_order_id = 900;
    fill.fill_seq = 100;
    fill.trade_id = 0;
    assert_eq!(key(&fill), Some(zero));
    for (order, cumulative) in [(0, 100), (900, 0)] {
        let mut bad = fill;
        bad.exch_order_id = order;
        bad.fill_seq = cumulative;
        assert_eq!(key(&bad), None);
    }
    let mut wrong_kind = InputRec::of(&fill, Source::Venue(VP));
    wrong_kind.kind = EventKind::ORDER_EVENT;
    assert_eq!(Before::take(&engine, &wrong_kind).ext_key, None);
    let mut slot = hft_sequencer::types::InputSlot::with_capacity(size_of::<ExternalFill>());
    slot.set(hft_event::RecordFlags(0), Tsc(1), &fill);
    assert_eq!(
        Before::input(engine.sequencer().engine(), &world, &slot).ext_key,
        Some(zero)
    );
    slot.kind = EventKind::ORDER_EVENT;
    assert_eq!(
        Before::input(engine.sequencer().engine(), &world, &slot).ext_key,
        None
    );

    let base = Evidence {
        kind: EvidenceKind::External,
        account: ACCT_A,
        venue: VP,
        inst: P1,
        delta: 100,
        ext_key: Some(positive),
    };
    for domain in [positive, zero] {
        let first = Evidence {
            ext_key: Some(domain),
            ..base
        };
        for second in [
            Evidence {
                account: ACCT_B,
                ..first
            },
            Evidence { venue: VS, ..first },
            Evidence {
                inst: InstId(P1.0 + 1),
                ..first
            },
        ] {
            let mut state = InvState::default();
            check(&mut state, &[first]).unwrap();
            check(&mut state, &[second]).unwrap();
            assert!(check(&mut state, &[second]).unwrap_err().contains("booked twice"));
        }
        assert!(check(&mut InvState::default(), &[first, first]).is_err());
        let mut state = InvState::default();
        check(&mut state, &[first]).unwrap();
        // Direction/amount changes cannot disguise a second booking of the same key.
        assert!(check(&mut state, &[Evidence { delta: -1, ..first }]).is_err());
    }
    let distinct = [
        positive,
        zero,
        ExternalKey::Trade(101),
        ExternalKey::OrderSeq {
            exch_order_id: 901,
            fill_seq: 100,
        },
        ExternalKey::OrderSeq {
            exch_order_id: 900,
            fill_seq: 101,
        },
    ];
    let rows: Vec<_> = distinct
        .into_iter()
        .map(|k| Evidence {
            ext_key: Some(k),
            ..base
        })
        .collect();
    check(&mut InvState::default(), &rows).unwrap();
    assert!(
        check(
            &mut InvState::default(),
            &[Evidence {
                ext_key: None,
                ..base
            }]
        )
        .unwrap_err()
        .contains("lacks its dedup identity")
    );

    // I2 must not excuse an unbooked input using another order's or domain's
    // retained key. Exercise the same scoped identity in the read-only dedup view.
    let record = JRec {
        seq: 1,
        tsc: 1,
        kind: EventKind::EXTERNAL_FILL,
        flags: hft_event::RecordFlags(0),
        body: fill.as_bytes().to_vec(),
    };
    for (account, venue, inst, retained, accepted) in [
        (ACCT_A, VP, P1, zero, true),
        (ACCT_B, VP, P1, zero, false),
        (ACCT_A, VS, P1, zero, false),
        (ACCT_A, VP, InstId(P1.0 + 1), zero, false),
        (ACCT_A, VP, P1, positive, false),
        (
            ACCT_A,
            VP,
            P1,
            ExternalKey::OrderSeq {
                exch_order_id: 901,
                fill_seq: 100,
            },
            false,
        ),
        (
            ACCT_A,
            VP,
            P1,
            ExternalKey::OrderSeq {
                exch_order_id: 900,
                fill_seq: 101,
            },
            false,
        ),
    ] {
        let mut obs = Obs::default();
        obs.ext_seen.push((account, venue, inst, retained));
        let info = StepInfo::default();
        let result = (checker(InvId::I2).unwrap().step)(
            &mut InvState::default(),
            &StepView {
                step: 1,
                now_ns: 1,
                records: std::slice::from_ref(&record),
                delivered: None,
                frames: &[],
                info: &info,
                before: &obs,
                after: &obs,
                truth: &[],
                rebuild: None,
            },
        );
        assert_eq!(
            result.is_ok(),
            accepted,
            "retained {:?}: {result:?}",
            obs.ext_seen
        );
    }
}
