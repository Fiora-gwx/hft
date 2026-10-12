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

fn reconcile_input(h: &mut H, receipt: &hft_event::ExternalFill, rest: Qty) -> Vec<u8> {
    use zerocopy::IntoBytes;
    // A complete prior report after UserStream Up supplies the established stability gate.
    let view_seq = h.j().records().last().unwrap().seq;
    h.push(
        hft_sequencer::driver::live::RingId::Ctl,
        &recon_summary(AC1, B, 801, view_seq),
    );
    assert_eq!(h.run(), StepOutcome::Idle);
    assert!(h.s.engine().risk.auto_external_ok(AC1, B, 0));
    let mut summary = recon_summary(AC1, B, 802, h.j().records().last().unwrap().seq);
    summary.count = 2;
    let mut p = summary;
    p.idx = 1;
    p.item = 1;
    p.inst = B1;
    p.engine_qty = h.s.engine().ledger.pos(AC1, B1).qty;
    p.exch_qty = rest;
    p.sflags = hft_event::ReconcileReport::AUTO_EXTERNAL;
    let hdr = hft_event::ReconcileBatchHdr {
        report_id: 802,
        n_reports: 2,
        n_fills: 1,
        venue: B,
        account: AC1,
        _pad: 0,
    };
    [
        hdr.as_bytes(),
        summary.as_bytes(),
        p.as_bytes(),
        receipt.as_bytes(),
    ]
    .concat()
}

fn reconcile_receipt(side: Side, id: u64, qty: i64, cash: i64) -> hft_event::ExternalFill {
    hft_event::ExternalFill {
        venue: B,
        account: AC1,
        inst: B1,
        side,
        cause: hft_event::ExternalCause::Adl,
        qty: Qty(qty),
        px: Px(6_002_750),
        fee: Qty(0),
        fee_asset: USDT,
        _pad: [0; 6],
        exch_order_id: 80_000 + id,
        trade_id: id,
        ex_ts_ns: 0,
        rx_tsc: hft_types::Tsc(0),
        fill_seq: 0,
        cash_qty: Qty(qty),
        cash_amount: Qty(cash),
    }
}

#[test]
fn seq_418_bad_receipt_batch_cannot_partially_book() {
    use hft_sequencer::driver::live::RingId;
    use zerocopy::IntoBytes;
    for mutation in 0..6 {
        let mut h = cash_boot();
        let open = reconcile_receipt(Side::Sell, 40, 300_000, 0);
        h.push(h.rep_ring(AC1), &open);
        assert_eq!(h.run(), StepOutcome::Idle);
        let first = reconcile_receipt(Side::Buy, 41, 100_000, -111);
        let mut last = reconcile_receipt(Side::Buy, 42, 100_000, -222);
        match mutation {
            0 => last.cash_qty = Qty(0),
            1 => {
                last.fee_asset = hft_types::ASSET_NONE;
                last.fee = Qty(1);
            }
            2 => last.trade_id = first.trade_id,
            3 => {
                last.qty.0 += 1;
                last.cash_qty = last.qty;
            }
            4 => last.side = Side::Sell,
            _ => last.trade_id = open.trade_id,
        }
        let mut body = reconcile_input(&mut h, &first, Qty(-100_000));
        body[10..12].copy_from_slice(&2u16.to_le_bytes());
        body.extend_from_slice(last.as_bytes());
        let wallet_before = wallet(&h.s.engine().ledger);
        let pos_before = *h.s.engine().ledger.pos(AC1, B1);
        let seen_before = h.s.engine().ledger.ext_seen(AC1, B).to_vec();
        let mark = h.mark();
        h.push_bytes(RingId::Ctl, EventKind::RECONCILE_BATCH, body);
        assert!(matches!(h.run(), StepOutcome::Fatal(FatalReason::Settlement(_))));
        assert_eq!(wallet(&h.s.engine().ledger), wallet_before, "row {mutation}");
        assert_eq!(*h.s.engine().ledger.pos(AC1, B1), pos_before);
        assert_eq!(h.s.engine().ledger.auto_held(AC1, B1, Side::Buy), Qty(0));
        assert_eq!(h.s.engine().ledger.ext_seen(AC1, B), seen_before);
        assert!(!h.since(mark).iter().any(|r| {
            r.read::<hft_event::Note>()
                .is_some_and(|n| n.code == hft_ledger::notes::code::EXTERNAL_FILL)
        }));
    }
}

#[test]
fn seq_416_reconciliation_receipts_rebuild_wallet_and_pending_confirmation() {
    use hft_sequencer::driver::live::RingId;
    for side in [Side::Buy, Side::Sell] {
        for remaining in [0, 100_000] {
            let mut h = cash_boot();
            let initial = wallet(&h.s.engine().ledger);
            let start = h.keyframe();
            let open = reconcile_receipt(side, 40, 200_000, 0);
            h.push(h.rep_ring(AC1), &open);
            assert_eq!(h.run(), StepOutcome::Idle);
            let mut receipt = reconcile_receipt(side.opposite(), 41, 200_000 - remaining, -1_170_003);
            receipt.fee = Qty(397);
            let body = reconcile_input(&mut h, &receipt, Qty(side.sign() * remaining));
            h.push_bytes(RingId::Ctl, EventKind::RECONCILE_BATCH, body.clone());
            assert_eq!(h.run(), StepOutcome::Idle);
            assert_eq!(wallet(&h.s.engine().ledger), initial - 1_170_400);
            assert_eq!(h.s.engine().ledger.pos(AC1, B1).qty, Qty(side.sign() * remaining));
            assert_eq!(h.s.engine().ledger.auto_held(AC1, B1, receipt.side), receipt.qty);
            let pending = h.keyframe();
            assert_eq!(wallet(cold_at(&h, &start).ledger()), initial - 1_170_400);
            h.push(h.rep_ring(AC1), &receipt);
            assert_eq!(h.run(), StepOutcome::Idle);
            assert_eq!(h.s.engine().ledger.auto_held(AC1, B1, receipt.side), Qty(0));
            h.push(h.rep_ring(AC1), &receipt);
            assert_eq!(h.run(), StepOutcome::Idle);
            assert_eq!(wallet(&h.s.engine().ledger), initial - 1_170_400);
            h.keyframe();
            for snapshot in [&start, &pending] {
                let cold = cold_at(&h, snapshot);
                assert_eq!(wallet(cold.ledger()), initial - 1_170_400);
                assert_eq!(cold.ledger().auto_held(AC1, B1, receipt.side), Qty(0));
                assert!(h.verify_from(Some(snapshot.seq)).is_empty());
            }
            assert!(h.verify().is_empty());
            // A retried, now stale batch is graded but must not book again.
            h.push_bytes(RingId::Ctl, EventKind::RECONCILE_BATCH, body);
            assert_eq!(h.run(), StepOutcome::Idle);
            assert_eq!(wallet(cold_at(&h, &start).ledger()), initial - 1_170_400);
            assert!(h.verify().is_empty());
        }
    }
}

#[test]
fn seq_417_reconciliation_cash_requires_all_raw_facts_and_output_evidence() {
    use hft_event::Note;
    use hft_sequencer::driver::live::RingId;
    use zerocopy::IntoBytes;
    let mut h = cash_boot();
    let start = h.keyframe();
    let open = reconcile_receipt(Side::Sell, 40, 200_000, 0);
    h.push(h.rep_ring(AC1), &open);
    h.run();
    let receipt = reconcile_receipt(Side::Buy, 41, 100_000, -1_170_003);
    let body = reconcile_input(&mut h, &receipt, Qty(-100_000));
    h.push_bytes(RingId::Ctl, EventKind::RECONCILE_BATCH, body);
    assert_eq!(h.run(), StepOutcome::Idle);
    h.keyframe();
    let code = hft_sequencer::codes::seq_note(hft_sequencer::codes::note::RECONCILE_CASH);
    for mutation in 0..5 {
        let mut bad = h.j().clone();
        let row = bad
            .segments
            .iter_mut()
            .flat_map(|s| &mut s.records)
            .find(|r| {
                if mutation == 4 {
                    r.kind == EventKind::RECONCILE_BATCH
                } else {
                    r.read::<Note>().is_some_and(|n| {
                        if mutation < 2 {
                            n.code == code
                        } else {
                            n.code == hft_ledger::notes::code::EXTERNAL_FILL
                                && n.v[0] >> 24 & hft_ledger::aflags::AUTO_RECON != 0
                        }
                    })
                }
            })
            .unwrap();
        if mutation == 4 {
            // Raw fee changed while all generated outputs and snapshot claims stay fixed.
            let at = 16 + 2 * 104 + 24;
            row.body[at] ^= 1;
        } else if mutation % 2 == 0 {
            row.kind = EventKind::LATENCY_SAMPLE;
        } else {
            let mut n = row.read::<Note>().unwrap();
            n.v[3] += 1;
            row.body.copy_from_slice(n.as_bytes());
        }
        assert!(
            !verify_journal::<TestSet>(&bad, &h.setup, Some(start.seq)).is_empty(),
            "mutation {mutation}"
        );
        let mut cold = JournalLedger::from_snapshot(
            &mut SnapshotReader::new(&start.body[96..]),
            &h.rd,
            &h.setup.ledger,
            &h.setup.caps.to_ledger(),
        )
        .unwrap();
        let result = bad.records().filter(|r| r.seq > start.seq).try_for_each(|r| {
            cold.apply(&RecordView {
                hdr: RecordHeader {
                    len: (24 + r.body.len()) as u32,
                    kind: r.kind.0,
                    flags: r.flags.0,
                    seq: r.seq,
                    tsc: r.tsc,
                },
                body: &r.body,
            })
        });
        if mutation < 4 {
            assert!(result.is_err(), "cold mutation {mutation}");
        } else {
            assert_ne!(wallet(cold.ledger()), wallet(&h.s.engine().ledger));
        }
    }
}

fn foreign_event(slot: u32, cum: i64, cash: i64) -> hft_event::OrderEvent {
    hft_event::OrderEvent {
        venue: B,
        account: AC1,
        what: OrderEventKind::PartialFill,
        detail: Side::Sell as u8,
        slot: hft_types::SlotId(slot),
        r#gen: 17,
        inst: B1,
        px: Px(6_000_000),
        qty: Qty(100_000),
        cum_filled: Qty(cum),
        fee: Qty(0),
        fee_asset: USDT,
        epoch: 1,
        reject_code: 0,
        exch_order_id: 99_001,
        ex_ts_ns: 0,
        hw_rx_ns: 0,
        rx_tsc: 0,
        avg_px: Px(6_000_000),
        cash_qty: Qty(cum),
        cash_sum: Qty(cash),
    }
}

#[test]
fn seq_415_foreign_cash_survives_checkpoints_cold_consumption_and_replay() {
    use hft_event::Note;
    use zerocopy::IntoBytes;
    for slot in [0, 700] {
        let mut h = cash_boot();
        let initial = wallet(&h.s.engine().ledger);
        let start = h.keyframe();
        let mut mid = None;
        for (i, cash) in [0, -1_170_000, -1_180_000].into_iter().enumerate() {
            let e = foreign_event(slot, (i as i64 + 1) * 100_000, cash);
            let mark = h.mark();
            h.push(h.rep_ring(AC1), &e);
            assert_eq!(h.run(), StepOutcome::Idle);
            let n = h
                .since(mark)
                .iter()
                .filter_map(|r| r.read::<Note>())
                .filter(|n| n.code == hft_ledger::notes::code::OMS_FOREIGN_FILL)
                .count();
            assert_eq!(n, 1);
            assert_eq!(wallet(&h.s.engine().ledger), initial + cash);
            let cold = cold_at(&h, &start);
            assert_eq!(wallet(cold.ledger()), initial + cash);
            assert_eq!(cold.ledger().pos(AC1, B1), h.s.engine().ledger.pos(AC1, B1));
            h.push(h.rep_ring(AC1), &e);
            assert_eq!(h.run(), StepOutcome::Idle);
            if i == 0 {
                mid = Some(h.keyframe());
            }
        }
        h.keyframe();
        let mid = mid.unwrap();
        assert_eq!(wallet(cold_at(&h, &mid).ledger()), initial - 1_180_000);
        assert!(h.verify().is_empty());
        assert!(h.verify_from(Some(start.seq)).is_empty());
        assert!(h.verify_from(Some(mid.seq)).is_empty());
        for hide in [false, true] {
            let mut bad = h.j().clone();
            let row = bad
                .segments
                .iter_mut()
                .flat_map(|s| &mut s.records)
                .find(|r| {
                    r.read::<Note>()
                        .is_some_and(|n| n.code == hft_ledger::notes::code::OMS_FOREIGN_FILL)
                })
                .unwrap();
            if hide {
                row.kind = EventKind::LATENCY_SAMPLE;
            } else {
                let mut note = row.read::<Note>().unwrap();
                note.v[1] += 1;
                row.body.copy_from_slice(note.as_bytes());
            }
            assert!(!verify_journal::<TestSet>(&bad, &h.setup, Some(start.seq)).is_empty());
        }
    }
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

#[test]
fn seq_414_balance_check_gate_matches_cold_view_and_full_window_replay() {
    use hft_event::{BalanceReport, Note};
    use hft_ledger::{notes::code, vflags};
    use zerocopy::IntoBytes;
    let mut h = cash_boot();
    let start = h.keyframe();
    let total = wallet(&h.s.engine().ledger);
    let mut last = None;
    for free in [total - 100_000_000_000, total] {
        let m = h.mark();
        let b = BalanceReport {
            venue: B,
            account: AC1,
            asset: USDT,
            _pad: 0,
            free: Qty(free),
            locked: Qty(0),
            ex_ts_ns: h.now() as i64,
            rx_tsc: hft_types::Tsc(h.now()),
        };
        h.push(h.rep_ring(AC1), &b);
        assert_eq!(h.run(), StepOutcome::Idle);
        let notes: Vec<Note> = h
            .since(m)
            .iter()
            .filter_map(|r| r.read::<Note>())
            .filter(|n| n.code == code::BALANCE_CHECK)
            .collect();
        assert_eq!(notes.len(), 1);
        let hot = h.s.engine().ledger.venue_book(AC1, B);
        assert_eq!(hot.vflags & vflags::DRIFT != 0, free != total);
        let cold = cold_at(&h, &start);
        let got = cold.ledger().venue_book(AC1, B);
        assert_eq!(got.summary, hot.summary);
        assert_eq!(got.vflags, hot.vflags);
        assert_eq!(got.cash, hot.cash);
        assert_eq!(wallet(cold.ledger()), total);
        last = Some(h.keyframe());
    }
    assert!(h.verify().is_empty());
    assert!(h.verify_from(Some(start.seq)).is_empty());
    assert!(h.verify_from(Some(last.unwrap().seq)).is_empty());
    for hide in [false, true] {
        let mut bad = h.j().clone();
        let r = bad
            .segments
            .iter_mut()
            .flat_map(|s| &mut s.records)
            .find(|r| r.read::<Note>().is_some_and(|n| n.code == code::BALANCE_CHECK))
            .unwrap();
        if hide {
            // Preserve sequence numbers while withholding this business Output from replay.
            r.kind = EventKind::LATENCY_SAMPLE;
        } else {
            let mut n = r.read::<Note>().unwrap();
            n.v[2] ^= 1;
            r.body.copy_from_slice(n.as_bytes());
        }
        assert!(
            !verify_journal::<TestSet>(&bad, &h.setup, Some(start.seq)).is_empty(),
            "missing or corrupted BalanceCheck passed replay"
        );
    }
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
