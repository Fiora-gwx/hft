//! Cash facts pass through the real OMS, hot ledger, cold reader and journal replay.
use crate::common::*;
use hft_event::{EventKind, OrderDecision, OrderEventKind, RecordHeader, RecordView};
use hft_ledger::{JournalLedger, Ledger};
use hft_refdata::CapFlags;
use hft_sequencer::{FatalReason, MemRecord, StepOutcome};
use hft_strategy_sdk::{EventRef, LegLinkOpt, SnapshotReader};
use hft_types::{Px, Qty, Side};

fn cash_boot() -> H {
    reset_script();
    let mut b = b0();
    let c = b.rd.caps.iter_mut().find(|c| c.venue == B).unwrap();
    c.flags = c.flags | CapFlags::PERP_CASH;
    let mut h = b.boot_self_contained();
    h.steady();
    h
}

fn wallet(l: &Ledger) -> i64 {
    l.venue_book(AC1, B)
        .cash
        .iter()
        .find(|c| c.asset == USDT)
        .unwrap()
        .total
        .0
}

fn cold_at(h: &H, start: &MemRecord) -> JournalLedger {
    let mut j = JournalLedger::from_snapshot(
        &mut SnapshotReader::new(&start.body[96..]),
        &h.rd,
        &h.setup.ledger,
        &h.setup.caps.to_ledger(),
    )
    .unwrap();
    for r in h.j().records().filter(|r| r.seq > start.seq) {
        j.apply(&RecordView {
            hdr: RecordHeader {
                len: (24 + r.body.len()) as u32,
                kind: r.kind.0,
                flags: r.flags.0,
                seq: r.seq,
                tsc: r.tsc,
            },
            body: &r.body,
        })
        .unwrap_or_else(|e| panic!("cold seq {}: {e:?}", r.seq));
    }
    j
}

// B0 uses USDT qty_exp=-8. These are the original five matching facts rescaled from -6.
const FACTS: [(Side, i64, i64, i64, i64); 5] = [
    (Side::Sell, 100_000, 6_001_890, 3_001_000, 0),
    (Side::Sell, 100_000, 6_002_440, 3_001_300, 0),
    (Side::Buy, 200_000, 6_002_750, 6_002_800, -1_170_000),
    (Side::Sell, 100_000, 6_002_740, 3_001_400, 0),
    (Side::Buy, 100_000, 6_002_750, 3_001_400, -10_000),
];

fn place_orders(h: &mut H) -> Vec<OrderDecision> {
    // The quote itself identifies the order; replay never relies on an unsaved closure counter.
    add_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            let ix = (hx.v.bbo(A1).bid_px.0 - px("50002").0) / 100;
            if let Some(&(side, qty, _, _, _)) = usize::try_from(ix).ok().and_then(|ix| FACTS.get(ix)) {
                let mut it = open_buy(hx, B1);
                it.side = side;
                it.qty = Qty(qty);
                it.px = if side == Side::Buy {
                    hx.v.bbo(B1).ask_px
                } else {
                    hx.v.bbo(B1).bid_px
                };
                let _ = ctx.place(it);
            }
        }
    });
    let mut ds = Vec::new();
    for ix in 0..5 {
        let mark = h.mark();
        h.quote(A1, &(50002 + ix).to_string(), &(50003 + ix).to_string());
        assert_eq!(h.run(), StepOutcome::Idle);
        let d: Vec<_> = decisions(&h.since(mark))
            .into_iter()
            .filter(|(_, d)| d.inst == B1 && d.account == AC1 && d.reject == 0)
            .map(|(_, d)| d)
            .collect();
        assert_eq!(d.len(), 1, "order {ix}");
        h.ack(&d[0]);
        ds.push(d[0]);
    }
    ds
}

#[test]
fn seq_411_cash_wallet_matches_cold_reader_and_full_window_replay() {
    for order in [[0, 1, 2, 3, 4], [0, 1, 3, 2, 4], [4, 2, 3, 1, 0]] {
        let mut h = cash_boot();
        let initial = wallet(&h.s.engine().ledger);
        let before_orders = h.keyframe();
        let ds = place_orders(&mut h);
        let after_orders = h.keyframe();
        let mut expected = initial;
        let mut net = 0;
        let mut partial = None;
        for (step, ix) in order.into_iter().enumerate() {
            let (side, qty, price, fee, amount) = FACTS[ix];
            let mut e = order_event(&ds[ix], OrderEventKind::Fill, h.epoch());
            e.px = Px(price);
            e.avg_px = e.px;
            e.cum_filled = Qty(qty);
            e.cash_qty = e.cum_filled;
            e.cash_sum = Qty(amount);
            e.fee_asset = USDT;
            // Deliver the fee separately for one order, including an explicit zero cash fact.
            e.fee = Qty(if ix == 3 { 0 } else { fee });
            h.push(h.rep_ring(AC1), &e);
            assert_eq!(h.run(), StepOutcome::Idle);
            expected += amount - e.fee.0;
            net += side.sign() * qty;
            assert_eq!(wallet(&h.s.engine().ledger), expected, "{order:?}, step {step}");
            assert_eq!(h.s.engine().ledger.pos(AC1, B1).qty.0, net);
            h.push(h.rep_ring(AC1), &e);
            assert_eq!(h.run(), StepOutcome::Idle);
            assert_eq!(wallet(&h.s.engine().ledger), expected);
            if ix == 3 {
                e.what = OrderEventKind::FeeOnly;
                e.fee = Qty(fee);
                h.push(h.rep_ring(AC1), &e);
                assert_eq!(h.run(), StepOutcome::Idle);
                expected -= fee;
                assert_eq!(wallet(&h.s.engine().ledger), expected);
            }
            if step == 2 {
                partial = Some(h.keyframe());
            }
        }
        h.keyframe();
        assert_eq!(expected, initial - 1_180_000 - 18_007_900);
        assert_eq!(h.s.engine().ledger.pos(AC1, B1).qty, Qty(0));
        for start in [&before_orders, &after_orders, partial.as_ref().unwrap()] {
            let cold = cold_at(&h, start);
            assert_eq!(wallet(cold.ledger()), expected);
            assert_eq!(cold.ledger().pos(AC1, B1), h.s.engine().ledger.pos(AC1, B1));
            assert!(h.verify_from(Some(start.seq)).is_empty(), "window {}", start.seq);
        }
        assert!(h.verify().is_empty(), "full replay {order:?}");
    }
}

#[test]
fn seq_412_bad_cash_stops_before_hedge_and_wallet_booking() {
    for row in 0..5 {
        let mut h = cash_boot();
        add_hook(|hx, ev, ctx| {
            if let EventRef::Bbo { inst, .. } = ev
                && *inst == A1
                && hx.s.strategy == S1
                && hx.v.bbo(A1).bid_px == px("50002")
            {
                let mut it = pair_first(hx, q("0.002"));
                it.inst = B1;
                it.venue = B;
                it.side = Side::Sell;
                it.px = hx.v.bbo(B1).bid_px;
                let mut link = *it.link.get().unwrap();
                link.hedge = onfill(AC1, A1, Side::Buy);
                it.link = LegLinkOpt::some(link);
                let _ = ctx.place(it);
            }
        });
        let m = h.mark();
        h.quote(A1, "50002", "50003");
        assert_eq!(h.run(), StepOutcome::Idle);
        let d = decisions(&h.since(m))
            .into_iter()
            .find(|(_, d)| d.inst == B1 && d.reject == 0)
            .unwrap()
            .1;
        h.ack(&d);
        let mut e = order_event(&d, OrderEventKind::PartialFill, h.epoch());
        e.cum_filled = q("0.001");
        e.cash_qty = e.cum_filled;
        if row == 1 {
            h.push(h.rep_ring(AC1), &e);
            assert_eq!(h.run(), StepOutcome::Idle);
        }
        let want = match row {
            0 => {
                e.cash_qty = Qty(0);
                FatalReason::OmsCash(hft_oms::CashError::Missing)
            }
            1 => {
                e.cash_sum = Qty(1);
                FatalReason::OmsCash(hft_oms::CashError::Conflict)
            }
            2 => {
                e.epoch += 1;
                e.detail = Side::Sell as u8;
                FatalReason::OmsCash(hft_oms::CashError::Untracked)
            }
            3 => {
                e.px = Px(0);
                e.avg_px = Px(0);
                FatalReason::Settlement(hft_ledger::SettlementError::InvalidPrice)
            }
            _ => FatalReason::Settlement(hft_ledger::SettlementError::MissingCash),
        };
        let before = wallet(&h.s.engine().ledger);
        let pos = *h.s.engine().ledger.pos(AC1, B1);
        let m = h.mark();
        h.trace_on();
        if row == 4 {
            let x = hft_event::ExternalFill {
                venue: B,
                account: AC1,
                side: Side::Sell,
                cause: hft_event::ExternalCause::Adl,
                inst: B1,
                qty: q("0.001"),
                px: d.px,
                fee: Qty(0),
                fee_asset: USDT,
                _pad: [0; 6],
                exch_order_id: 0,
                trade_id: 77,
                ex_ts_ns: 0,
                rx_tsc: hft_types::Tsc(0),
                fill_seq: 0,
                cash_qty: Qty(0),
                cash_amount: Qty(0),
            };
            h.push(h.rep_ring(AC1), &x);
        } else {
            h.push(h.rep_ring(AC1), &e);
        }
        assert_eq!(h.run(), StepOutcome::Fatal(want), "row {row}");
        assert_eq!(wallet(&h.s.engine().ledger), before);
        assert_eq!(*h.s.engine().ledger.pos(AC1, B1), pos);
        assert!(decisions(&h.since(m)).is_empty());
        assert!(!h.trace().contains(&"ledger.apply_fill"));
        assert!(!h.since(m).iter().any(|r| r.kind == EventKind::ORDER_DECISION));
    }
}

#[test]
fn seq_413_external_cash_dedup_cold_snapshots_and_full_window_replay() {
    for order in [[0, 1, 2, 3, 4], [0, 1, 3, 2, 4], [4, 2, 3, 1, 0]] {
        let mut h = cash_boot();
        let initial = wallet(&h.s.engine().ledger);
        let before = h.keyframe();
        let mut expected = initial;
        let mut net = 0;
        let mut partial = None;
        for (step, ix) in order.into_iter().enumerate() {
            let (side, qty, price, fee, amount) = FACTS[ix];
            let e = hft_event::ExternalFill {
                venue: B,
                account: AC1,
                side,
                cause: hft_event::ExternalCause::Manual,
                inst: B1,
                qty: Qty(qty),
                px: Px(price),
                fee: Qty(fee),
                fee_asset: USDT,
                _pad: [0; 6],
                exch_order_id: 0,
                trade_id: ix as u64 + 70,
                ex_ts_ns: 0,
                rx_tsc: hft_types::Tsc(0),
                fill_seq: 0,
                cash_qty: Qty(qty),
                cash_amount: Qty(amount),
            };
            h.push(h.rep_ring(AC1), &e);
            assert_eq!(h.run(), StepOutcome::Idle);
            expected += amount - fee;
            net += side.sign() * qty;
            assert_eq!(wallet(&h.s.engine().ledger), expected, "{order:?} step {step}");
            assert_eq!(h.s.engine().ledger.pos(AC1, B1).qty.0, net);
            if step == 2 {
                // The following duplicate must be rejected by keys restored from this frame.
                partial = Some(h.keyframe());
            }
            let m = h.mark();
            h.push(h.rep_ring(AC1), &e);
            assert_eq!(h.run(), StepOutcome::Idle);
            assert_eq!(wallet(&h.s.engine().ledger), expected);
            assert_eq!(h.s.engine().ledger.pos(AC1, B1).qty.0, net);
            assert!(decisions(&h.since(m)).is_empty());
        }
        h.keyframe();
        assert_eq!(expected, initial - 1_180_000 - 18_007_900);
        assert_eq!(net, 0);
        for start in [&before, partial.as_ref().unwrap()] {
            let cold = cold_at(&h, start);
            assert_eq!(wallet(cold.ledger()), expected);
            assert_eq!(cold.ledger().pos(AC1, B1), h.s.engine().ledger.pos(AC1, B1));
            assert_eq!(
                cold.ledger().ext_seen(AC1, B),
                h.s.engine().ledger.ext_seen(AC1, B)
            );
            assert!(h.verify_from(Some(start.seq)).is_empty(), "window {}", start.seq);
        }
        assert!(h.verify().is_empty(), "full replay {order:?}");
    }
}
