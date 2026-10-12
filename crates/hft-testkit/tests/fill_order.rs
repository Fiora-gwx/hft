//! Raw generation-order evidence for the INV-16 cash-order counterexample.
use hft_event::{OrderAction, OrderDecision, OrderEvent};
use hft_testkit::dst::invariants::{InvId, InvariantSet};
use hft_testkit::dst::{DstOutcome, FailKind, RerunPolicy, SeqSut, run_record};
use hft_testkit::strategies::TestStrategies;
use hft_types::Side;

#[test]
fn tk_123_raw_matching_order_survives_report_jitter() {
    let suite = hft_testkit::scenarios::suites()
        .into_iter()
        .find(|s| s.name == "modes")
        .unwrap();
    let mut c = suite.config(0x7dfe_e420_7b81_bfbd);
    c.steps = 316;
    c.rerun = RerunPolicy::Never;
    let r = run_record::<SeqSut<TestStrategies>>(&c);
    let DstOutcome::Fail(f) = &r.outcome else {
        panic!("the strict INV-16 counterexample was not reproduced");
    };
    assert_eq!(f.invariant, FailKind::Inv(InvId::I16));
    assert_eq!(f.step, 315);
    assert!(f.message.contains("balance 49999683576 != truth 49999683575"));
    let selected = |e: &OrderEvent| e.account.0 == 1 && e.venue.0 == 5 && (5..=7).contains(&e.slot.0);
    let generated: Vec<_> = r.matched_tail.iter().filter(|(_, e)| selected(e)).collect();
    assert_eq!(generated.len(), 3);
    assert!(generated.windows(2).all(|w| w[0].0 < w[1].0));
    assert_eq!(
        generated.iter().map(|(_, e)| e.slot.0).collect::<Vec<_>>(),
        [5, 6, 7]
    );
    let delivered: Vec<_> = r
        .journal
        .iter()
        .filter_map(|j| j.try_read::<OrderEvent>().map(|e| (j.seq, e)))
        .filter(|(_, e)| selected(e) && e.qty.0 > 0)
        .collect();
    assert_eq!(
        delivered
            .iter()
            .map(|(seq, e)| (*seq, e.slot.0))
            .collect::<Vec<_>>(),
        [(544, 6), (555, 5), (597, 7)]
    );
    for ((_, e), (slot, side, qty, px)) in generated.iter().zip([
        (5, Side::Buy, 200_000, 600_275_000),
        (6, Side::Sell, 100_000, 600_274_000),
        (7, Side::Buy, 100_000, 600_275_000),
    ]) {
        let decision = r
            .journal
            .iter()
            .filter_map(|j| j.try_read::<OrderDecision>())
            .find(|d| {
                d.account == e.account
                    && d.venue == e.venue
                    && d.slot == e.slot
                    && d.r#gen == e.r#gen
                    && d.action == OrderAction::New
            })
            .unwrap();
        assert_eq!((e.slot.0, decision.side, e.qty.0, e.px.0), (slot, side, qty, px));
        assert!(delivered.iter().any(|(_, d)| d.slot == e.slot
            && d.r#gen == e.r#gen
            && d.epoch == e.epoch
            && d.cum_filled == e.cum_filled));
    }
    c.invariants = InvariantSet::none();
    let unchecked = run_record::<SeqSut<TestStrategies>>(&c);
    assert_eq!(r.steps_run, unchecked.steps_run);
    assert_eq!(r.journal.len(), unchecked.journal.len());
    for (a, b) in r.journal.iter().zip(&unchecked.journal) {
        assert!(a == b, "journal differs at seq {}", a.seq);
    }
    assert_eq!(r.trail, unchecked.trail);
    assert_eq!(r.matched_tail, unchecked.matched_tail);
}
