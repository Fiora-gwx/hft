//! Raw generation-order evidence for the INV-16 cash-order counterexample.
use std::collections::BTreeSet;

use hft_event::{GwOp, OrderAction, OrderDecision, OrderEvent, OrderEventKind};
use hft_testkit::dst::invariants::{InvId, InvariantSet};
use hft_testkit::dst::{DstOutcome, FailKind, RerunPolicy, RunRecord, SeqSut, run_record};
use hft_testkit::strategies::TestStrategies;
use hft_types::Side;

/// Independent signed-basis calculation for this fixed example. No ledger / OMS / Sim
/// arithmetic or observed position/cost/balance enters the expected value.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
struct Cash {
    qty: i128,
    basis: i128,
    wallet: i128,
    fees: i128,
}

impl Cash {
    fn trade(&mut self, side: Side, qty: i64, px: i64, fee: i64, scale: i128) {
        let sign = if side == Side::Buy { 1 } else { -1 };
        let mut remaining = i128::from(qty);
        if self.qty != 0 && self.qty.signum() != sign {
            let closed = remaining.min(self.qty.abs());
            let released = self.basis * closed / self.qty.abs();
            let realized = self.qty.signum() * i128::from(px) * closed - released;
            self.wallet += realized / scale;
            self.basis -= released;
            self.qty += sign * closed;
            remaining -= closed;
        }
        self.qty += sign * remaining;
        self.basis += sign * i128::from(px) * remaining;
        self.fees += i128::from(fee);
    }
    fn balance(self, initial: i128) -> i128 {
        initial + self.wallet - self.fees
    }
}

fn key(e: &OrderEvent) -> (u8, u8, u16, u32, u32, u32, i64) {
    (
        e.account.0,
        e.venue.0,
        e.epoch,
        e.slot.0,
        e.r#gen,
        e.inst.0,
        e.cum_filled.0,
    )
}

/// Deliberately limited to the full fills in this example. Other report forms must not
/// silently inherit this calculation as their acceptance oracle.
fn cash_of(r: &RunRecord, fills: &[OrderEvent]) -> Result<Cash, &'static str> {
    let mut cash = Cash::default();
    let mut seen = BTreeSet::new();
    for e in fills {
        if e.what != OrderEventKind::Fill
            || e.qty.0 <= 0
            || e.cum_filled != e.qty
            || e.avg_px != e.px
            || e.fee_asset.0 != 2
            || e.fee.0 < 0
        {
            return Err("unsupported evidence in fixed cash-order example");
        }
        if !seen.insert(key(e)) {
            return Err("duplicate fill evidence");
        }
        let matches: Vec<_> = r
            .frames
            .iter()
            .map(|(_, f)| f.cmd)
            .filter(|c| {
                c.op == GwOp::Place
                    && c.account == e.account
                    && c.inst == e.inst
                    && c.slot == e.slot
                    && c.r#gen == e.r#gen
                    && c.arg as u16 == e.epoch
            })
            .collect();
        if matches.len() != 1 {
            return Err("missing or ambiguous complete order identity");
        }
        let cmd = matches[0];
        if !r.journal.iter().any(|j| {
            j.seq == cmd.decision_seq
                && j.try_read::<OrderDecision>().is_some_and(|d| {
                    d.action == OrderAction::New
                        && d.reject == 0
                        && d.account == e.account
                        && d.venue == e.venue
                        && d.inst == e.inst
                        && d.slot == e.slot
                        && d.r#gen == e.r#gen
                        && d.side == cmd.side
                        && d.qty == cmd.qty
                })
        }) {
            return Err("order has no committed decision");
        }
        // W1 BTC px_exp=-4, qty_exp=-8, USDT qty_exp=-6; one raw cash unit is 10^6 products.
        cash.trade(cmd.side, e.qty.0, e.px.0, e.fee.0, 1_000_000);
    }
    Ok(cash)
}

#[test]
fn tk_123_raw_matching_order_survives_report_jitter() {
    let suite = hft_testkit::scenarios::suites()
        .into_iter()
        .find(|s| s.name == "modes")
        .unwrap();
    let mut c = suite.config(0x7dfe_e420_7b81_bfbd);
    // Keep the old-record counterexample explicit even when the standard world supplies cash.
    c.world
        .venues
        .iter_mut()
        .find(|v| v.id == hft_testkit::dst::world::VP)
        .unwrap()
        .perp_cash = false;
    c.plan = Some(c.fault_plan());
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
        .filter(|(_, e)| selected(e) && matches!(e.what, OrderEventKind::Fill | OrderEventKind::PartialFill))
        .collect();
    assert_eq!(
        delivered
            .iter()
            .map(|(seq, e)| (
                crate::seq_sut::seq_before_balance_checks(&r.journal, *seq),
                e.slot.0
            ))
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
    assert_eq!(r.truth, unchecked.truth);

    let relevant = |e: &OrderEvent| e.account.0 == 1 && e.venue.0 == 5 && e.inst.0 == 2;
    let source: Vec<_> = r.matched_tail.iter().map(|(_, e)| *e).filter(relevant).collect();
    let delivery: Vec<_> = r
        .journal
        .iter()
        .filter_map(|j| j.try_read::<OrderEvent>())
        .filter(|e| relevant(e) && matches!(e.what, OrderEventKind::Fill | OrderEventKind::PartialFill))
        .collect();
    assert_eq!(
        source.iter().map(key).collect::<BTreeSet<_>>(),
        delivery.iter().map(key).collect::<BTreeSet<_>>()
    );
    let a = cash_of(&r, &source).unwrap();
    let b = cash_of(&r, &delivery).unwrap();
    assert_eq!((a.qty, a.basis, b.qty, b.basis), (0, 0, 0, 0));
    assert_eq!(a.fees, b.fees);
    let initial = c
        .world
        .accounts
        .iter()
        .find(|a| a.id.0 == 1)
        .unwrap()
        .balances
        .iter()
        .find(|(v, a, _)| v.0 == 5 && a.0 == 2)
        .unwrap()
        .2;
    let source_balance = r
        .truth
        .iter()
        .find(|t| t.account.0 == 1 && t.venue.0 == 5)
        .unwrap()
        .balances
        .iter()
        .find(|(a, _)| a.0 == 2)
        .unwrap()
        .1;
    let ledger_balance = r
        .obs
        .balances
        .iter()
        .find(|b| b.account.0 == 1 && b.venue.0 == 5 && b.asset.0 == 2)
        .unwrap()
        .qty;
    assert_eq!(a.balance(initial), source_balance);
    assert_eq!(b.balance(initial), ledger_balance);
    assert_eq!(b.balance(initial) - a.balance(initial), 1);
    for adjustment in [-1, 1] {
        assert_ne!(ledger_balance + adjustment, b.balance(initial));
        let mut bad = delivery.clone();
        bad[0].fee.0 += adjustment as i64;
        assert_ne!(cash_of(&r, &bad).unwrap().balance(initial), ledger_balance);
    }
    let mut bad = source.clone();
    bad[0].epoch += 1;
    assert!(cash_of(&r, &bad).is_err());
    let mut bad = source.clone();
    bad.push(source[0]);
    assert!(cash_of(&r, &bad).is_err());
    let mut bad = source.clone();
    let fifth = bad.iter().position(|e| e.slot.0 == 5).unwrap();
    let sixth = bad.iter().position(|e| e.slot.0 == 6).unwrap();
    bad.swap(fifth, sixth);
    assert_ne!(cash_of(&r, &bad).unwrap().balance(initial), source_balance);
}

#[test]
fn tk_124_cash_order_math_keeps_open_cost_and_signed_rounding() {
    let opening = [
        (Side::Sell, 100_000, 600_189_000),
        (Side::Sell, 100_000, 600_244_000),
    ];
    let fills = [
        (Side::Buy, 200_000, 600_275_000),
        (Side::Sell, 100_000, 600_274_000),
        (Side::Buy, 100_000, 600_275_000),
    ];
    let fold = |indices: &[usize], mirror: bool| {
        let mut c = Cash::default();
        for (side, q, p) in opening.into_iter().chain(indices.iter().map(|i| fills[*i])) {
            let side = if mirror {
                if side == Side::Buy { Side::Sell } else { Side::Buy }
            } else {
                side
            };
            c.trade(side, q, p, 0, 1_000_000);
        }
        c
    };
    for mirror in [false, true] {
        let sign = if mirror { -1 } else { 1 };
        let a = fold(&[0, 1], mirror);
        let b = fold(&[1, 0], mirror);
        assert_eq!(
            (a.qty, a.basis, a.wallet),
            (-100_000 * sign, -60_027_400_000_000 * sign, -11_700 * sign)
        );
        assert_eq!(
            (b.qty, b.basis, b.wallet),
            (-100_000 * sign, -60_023_566_666_667 * sign, -7_866 * sign)
        );
        let a = fold(&[0, 1, 2], mirror);
        let b = fold(&[1, 0, 2], mirror);
        assert_eq!((a.qty, a.basis, a.wallet), (0, 0, -11_800 * sign));
        assert_eq!((b.qty, b.basis, b.wallet), (0, 0, -11_799 * sign));
    }
    let mut c = Cash::default();
    c.trade(Side::Buy, 2, 7, 0, 1);
    c.trade(Side::Sell, 3, 9, 0, 1);
    assert_eq!((c.qty, c.basis, c.wallet), (-1, -9, 4));
    c.trade(Side::Buy, 1, 8, 0, 1);
    assert_eq!((c.qty, c.basis, c.wallet), (0, 0, 5));
}

#[test]
fn tk_127_explicit_cash_closes_the_original_wallet_difference() {
    use hft_refdata::CapFlags;
    use hft_testkit::dst::seq::build;
    use hft_testkit::dst::world::{VP, VS};

    let suite = hft_testkit::scenarios::suites()
        .into_iter()
        .find(|s| s.name == "modes")
        .unwrap();
    let mut c = suite.config(0x7dfe_e420_7b81_bfbd);
    // Freeze the original full-length plan before shortening the diagnostic prefix.
    c.plan = Some(c.fault_plan());
    assert!(c.world.venues.iter().find(|v| v.id == VP).unwrap().perp_cash);
    for all_sim in [false, true] {
        let rd = build::load_refdata(&build::refdata_blob(&c.world, all_sim).unwrap()).unwrap();
        assert!(rd.caps(VP).flags.contains(CapFlags::PERP_CASH));
        assert!(!rd.caps(VS).flags.contains(CapFlags::PERP_CASH));
    }
    let mut bad = c.world.clone();
    bad.venues.iter_mut().find(|v| v.id == VS).unwrap().perp_cash = true;
    assert!(build::refdata_blob(&bad, false).is_err());
    assert!(build::refdata_blob(&bad, true).is_err());

    c.steps = 316;
    c.rerun = RerunPolicy::Always;
    let r = run_record::<SeqSut<TestStrategies>>(&c);
    assert!(r.outcome.is_pass(), "prefix: {}", crate::seq_sut::show(&r));
    assert_eq!(r.steps_run, 316);
    assert!(!r.trail.points.is_empty());
    let relevant = |e: &OrderEvent| e.account.0 == 1 && e.venue.0 == 5 && e.inst.0 == 2;
    let source: Vec<_> = r.matched_tail.iter().map(|(_, e)| *e).filter(relevant).collect();
    let delivery: Vec<_> = r
        .journal
        .iter()
        .filter_map(|j| j.try_read::<OrderEvent>())
        .filter(|e| relevant(e) && matches!(e.what, OrderEventKind::Fill | OrderEventKind::PartialFill))
        .collect();
    assert!(source.len() >= 5);
    assert_eq!(
        source.iter().map(key).collect::<BTreeSet<_>>(),
        delivery.iter().map(key).collect()
    );
    let mut previous_cash = 0;
    for (ix, e) in source.iter().enumerate() {
        let independent = cash_of(&r, &source[..=ix]).unwrap();
        assert_eq!(e.cash_qty, e.cum_filled);
        assert_eq!(i128::from(e.cash_sum.0), independent.wallet - previous_cash);
        previous_cash = independent.wallet;
    }
    let generated = cash_of(&r, &source).unwrap();
    let arrived = cash_of(&r, &delivery).unwrap();
    assert_eq!(
        arrived.wallet - generated.wallet,
        1,
        "the original delivery reordering must remain exercised"
    );
    let expected = generated.balance(50_000_000_000);
    assert_eq!(expected, 49_999_683_575);
    let truth = r
        .truth
        .iter()
        .find(|t| t.account.0 == 1 && t.venue == VP)
        .unwrap();
    assert_eq!(truth.balances.iter().find(|(a, _)| a.0 == 2).unwrap().1, expected);
    let balance = r
        .obs
        .balances
        .iter()
        .find(|b| b.account.0 == 1 && b.venue == VP && b.asset.0 == 2)
        .unwrap();
    assert_eq!(balance.qty, expected);
    assert!(truth.positions.is_empty());
    assert_cold_money(&c, &r);

    c.invariants = InvariantSet::none();
    let unchecked = run_record::<SeqSut<TestStrategies>>(&c);
    assert!(unchecked.outcome.is_pass());
    assert_eq!(r.journal.len(), unchecked.journal.len());
    for (a, b) in r.journal.iter().zip(&unchecked.journal) {
        assert!(a == b, "cash checker changes journal seq {}", a.seq);
    }
    assert_eq!(r.trail, unchecked.trail);
    assert_eq!(r.matched_tail, unchecked.matched_tail);
    assert_eq!(r.truth, unchecked.truth);

    c.invariants = InvariantSet::all();
    c.steps = 10_000;
    let full = run_record::<SeqSut<TestStrategies>>(&c);
    assert!(full.outcome.is_pass(), "full: {}", crate::seq_sut::show(&full));
    assert_eq!(full.steps_run, 10_000);
    assert!(!full.trail.points.is_empty());
    assert!(full.snapshot_checks > 0);
    assert_cold_money(&c, &full);
}

/// Replay the actual journal through the independent cold consumer from several saved windows.
fn assert_cold_money(c: &hft_testkit::dst::DstConfig, r: &RunRecord) {
    use hft_event::{EventKind, RecordHeader, RecordView};
    use hft_ledger::JournalLedger;
    use hft_strategy_sdk::SnapshotReader;
    use hft_testkit::dst::seq::build;

    let rd = build::load_refdata(&build::refdata_blob(&c.world, false).unwrap()).unwrap();
    let setup = build::engine_setup(&c.world, c.tsc_hz, 1);
    let last_fill = r
        .journal
        .iter()
        .rev()
        .find(|j| j.try_read::<OrderEvent>().is_some_and(|e| e.cum_filled.0 > 0))
        .unwrap()
        .seq;
    let snapshots: Vec<_> = r
        .journal
        .iter()
        .filter(|j| j.kind == EventKind::STATE_SNAPSHOT && j.seq < last_fill)
        .collect();
    assert!(!snapshots.is_empty());
    let windows: BTreeSet<_> = [0, snapshots.len() / 2, snapshots.len() - 1]
        .into_iter()
        .collect();
    for ix in windows {
        let start = snapshots[ix];
        let mut cold = JournalLedger::from_snapshot(
            &mut SnapshotReader::new(&start.body[96..]),
            &rd,
            &setup.ledger,
            &setup.caps.to_ledger(),
        )
        .unwrap();
        for j in r.journal.iter().filter(|j| j.seq > start.seq) {
            cold.apply(&RecordView {
                hdr: RecordHeader {
                    len: (24 + j.body.len()) as u32,
                    kind: j.kind.0,
                    flags: j.flags.0,
                    seq: j.seq,
                    tsc: j.tsc,
                },
                body: &j.body,
            })
            .unwrap_or_else(|e| panic!("cold window {} seq {}: {e:?}", start.seq, j.seq));
        }
        let ledger = cold.ledger();
        for b in &r.obs.balances {
            let actual = ledger
                .venue_book(b.account, b.venue)
                .cash
                .iter()
                .find(|cash| cash.asset == b.asset)
                .map_or(0, |cash| i128::from(cash.total.0));
            assert_eq!(actual, b.qty, "cold window {}, balance {b:?}", start.seq);
        }
        for p in &r.obs.positions {
            assert_eq!(
                i128::from(ledger.pos(p.account, p.inst).qty.0),
                p.qty,
                "cold window {}, position {p:?}",
                start.seq
            );
        }
        for i in &r.obs.inventory {
            assert_eq!(
                i128::from(ledger.inventory_view(i.account, i.venue, i.asset).total.0),
                i.qty,
                "cold window {}, inventory {i:?}",
                start.seq
            );
        }
    }
}
