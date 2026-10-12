//! The complete DST loop must release committed gaps with checkers both on and off.
use hft_event::{Bbo, EventKind, ExecDecision, ExternalFill, MarketHdr, ReconcileBatchView, ReconcileReport};
use hft_strategy_sdk::params::{RawParams, SchemaView, encode_body};
use hft_testkit::dst::faults::{ConnKind, Fault};
use hft_testkit::dst::sut::{InputRec, Source};
use hft_testkit::dst::world::{ACCT_A, P1, S1, VP};
use hft_testkit::dst::{DstConfig, InvariantSet, MarketSource, RerunPolicy, run_record};
use hft_testkit::scenarios::DstEngine;
use hft_testkit::strategies::TargetParams;
use hft_types::{Px, Qty};
use zerocopy::FromZeros;

fn config(full: bool, replay: bool) -> DstConfig {
    let mut c = DstConfig::new(1, "core");
    c.steps = 10_000;
    // The last quotes precede the 90 s periodic reports. Allow their real delivery before
    // checking final truth; stopping on that timer would leave a report genuinely in flight.
    c.horizon_ns = 90_500_000_000;
    c.rerun = RerunPolicy::Never;
    c.world.accounts.truncate(1);
    c.world.groups.truncate(1);
    c.world.strategies.truncate(1);
    // A real order leaves a tombstone. Wait for its configured expiry before the two rounds;
    // the default one-hour retention is exercised separately below and must block AUTO.
    c.world.oms_tombstone_keep_ms = Some(10_000);
    let first = c.world.insts.iter_mut().find(|i| i.id == S1).unwrap();
    first.lot = 200_000;
    first.min_qty = first.lot;
    let mut defaults: Vec<_> = (0..TargetParams::SCHEMA.n_fields())
        .map(|i| TargetParams::SCHEMA.field(i).unwrap().default)
        .collect();
    let pair = (0..TargetParams::SCHEMA.n_fields())
        .find(|i| TargetParams::SCHEMA.field(*i).unwrap().name == "t_pair_ms")
        .unwrap();
    defaults[pair as usize] = 1_000;
    c.world.strategies[0].kind_id = 0xFF02;
    c.world.strategies[0].params = encode_body(
        &TargetParams::SCHEMA,
        1,
        &RawParams {
            defaults,
            overrides: vec![],
        },
    )
    .unwrap();
    let mut quotes = Vec::new();
    for tick in 1..=899 {
        let ns = tick * 100_000_000;
        for i in &c.world.insts {
            let mut h = MarketHdr::new_zeroed();
            h.inst = i.id;
            h.venue = i.venue;
            h.ex_ts_ns = ns as i64;
            let b = Bbo {
                h,
                bid_px: Px(i.bid),
                bid_qty: Qty(i.depth),
                ask_px: Px(i.ask),
                ask_qty: Qty(i.depth),
                update_id: tick,
            };
            quotes.push((ns, InputRec::of(&b, Source::Market)));
        }
    }
    c.market = MarketSource::Recorded(quotes);
    c.scripted = vec![
        (
            10_000_000_000,
            Fault::ConnDown {
                venue: VP,
                conn: ConnKind::UserStream,
                down_ms: 5_000,
                resync: false,
            },
        ),
        (
            11_000_000_000,
            Fault::ReconciledAdl {
                account: ACCT_A,
                inst: P1,
                full,
                replay,
            },
        ),
    ];
    c
}

#[test]
fn tk_140_committed_gap_delivery_is_independent_of_checkers() {
    let mut retained = config(false, false);
    retained.world.oms_tombstone_keep_ms = None;
    let blocked = run_record::<DstEngine>(&retained);
    assert!(blocked.outcome.is_pass(), "{:?}", blocked.outcome);
    assert!(
        blocked
            .journal
            .iter()
            .all(|r| r.kind != EventKind::RECONCILE_BATCH)
    );
    assert!(
        blocked
            .journal
            .iter()
            .filter_map(|r| r.read::<ReconcileReport>())
            .any(|r| { r.account == ACCT_A && r.venue == VP && r.item == 5 && r.skipped_inflight > 0 })
    );
    assert!(
        blocked
            .truth
            .iter()
            .any(|t| t.account == ACCT_A && t.venue == VP && t.undelivered)
    );
    let mut failed = Vec::new();
    for full in [false, true] {
        for replay in [false, true] {
            let result = std::panic::catch_unwind(|| {
                let mut c = config(full, replay);
                let on = run_record::<DstEngine>(&c);
                assert!(
                    on.outcome.is_pass(),
                    "full={full} replay={replay}: {:?}",
                    on.outcome
                );
                let pair = on
                    .journal
                    .iter()
                    .filter_map(|r| r.read::<ExecDecision>())
                    .any(|d| d.account == ACCT_A && d.algo == 2 && d.qty.0 > 0);
                assert!(pair, "PairOnFill must actually place orders");
                let batches: Vec<_> = on
                    .journal
                    .iter()
                    .filter(|r| r.kind == EventKind::RECONCILE_BATCH)
                    .collect();
                assert_eq!(
                    batches.len(),
                    1,
                    "full={full} replay={replay} steps={} end={:?}: the real actor must actually book the gap; reports={:?}",
                    on.steps_run,
                    on.journal.last().map(|r| r.tsc),
                    on.journal
                        .iter()
                        .filter_map(|r| r.read::<ReconcileReport>())
                        .collect::<Vec<_>>()
                );
                assert!(on.ledger_checks.contains(&batches[0].seq));
                let b = ReconcileBatchView::read(&batches[0].body).unwrap();
                assert_eq!(b.hdr.n_fills, 1);
                let original = b.fills().next().unwrap();
                assert_eq!(original.qty.0, if full { 200_000 } else { 100_000 });
                let confirms: Vec<_> = on
                    .journal
                    .iter()
                    .filter_map(|r| r.try_read::<ExternalFill>().map(|e| (r.seq, e)))
                    .collect();
                assert_eq!(confirms.len(), usize::from(replay));
                for (seq, e) in confirms {
                    assert!(seq > batches[0].seq);
                    assert!(on.ledger_checks.contains(&seq));
                    assert_eq!(
                        (e.trade_id, e.cash_qty, e.cash_amount),
                        (original.trade_id, original.cash_qty, original.cash_amount)
                    );
                }
                assert!(
                    on.truth.iter().all(|t| !t.undelivered && t.excluded.is_empty()),
                    "full={full} replay={replay}: {:?}",
                    on.truth
                );
                assert!(on.obs.slots.iter().all(|s| s.state.is_terminal()));
                c.invariants = InvariantSet::none();
                let off = run_record::<DstEngine>(&c);
                assert!(off.outcome.is_pass(), "{:?}", off.outcome);
                assert_eq!(on.journal, off.journal);
                assert_eq!(on.frames, off.frames);
                assert_eq!(on.deliveries, off.deliveries);
                assert_eq!(on.trail.to_bytes(), off.trail.to_bytes());
                assert_eq!(on.truth, off.truth);
            });
            if result.is_err() {
                failed.push((full, replay));
            }
        }
    }
    assert!(failed.is_empty(), "failed (full, replay) rows: {failed:?}");
}
