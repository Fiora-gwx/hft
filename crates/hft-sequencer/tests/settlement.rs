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

fn receipt_boot() -> H {
    reset_script();
    let mut b = b0();
    let c = b.rd.caps.iter_mut().find(|c| c.venue == B).unwrap();
    c.flags = c.flags | CapFlags::PERP_CASH;
    b.setup_edit.push(|s| s.ledger.ext_dedup_cap = 2);
    let mut h = b.boot_self_contained();
    h.steady();
    h
}

fn first_execution(positive: bool) -> hft_event::ExternalFill {
    let mut raw = reconcile_receipt(Side::Sell, 40, 100_000, -11_700);
    raw.cause = hft_event::ExternalCause::Unknown;
    raw.trade_id = if positive { 40 } else { 0 };
    raw.fill_seq = if positive { 0 } else { raw.qty.0 as u64 };
    raw.fee = Qty(17);
    raw
}

fn duplicate_report(slot: u32, raw: &hft_event::ExternalFill) -> hft_event::OrderEvent {
    let mut e = foreign_event(slot, raw.qty.0, raw.cash_amount.0);
    e.what = OrderEventKind::Fill;
    e.exch_order_id = raw.exch_order_id;
    e.trade_id = raw.trade_id;
    e.qty = raw.qty;
    e.px = raw.px;
    e.avg_px = raw.px;
    e.fee = raw.fee;
    e.fee_asset = raw.fee_asset;
    e
}

#[test]
fn seq_419_original_duplicate_proof_survives_fifo_cold_restore_and_replay() {
    for slot in [0, 700] {
        for positive in [false, true] {
            let mut h = receipt_boot();
            let initial = wallet(&h.s.engine().ledger);
            let start = h.keyframe();
            let raw = first_execution(positive);
            h.push(h.rep_ring(AC1), &raw);
            assert_eq!(h.run(), StepOutcome::Idle);
            assert_eq!(wallet(&h.s.engine().ledger), initial - 11_717);
            let first = h.keyframe();
            // Evict the raw execution's ordinary ledger key while retaining its OMS proof.
            for id in 50..53 {
                let mut other = reconcile_receipt(Side::Buy, id, 1, 1);
                other.exch_order_id = 0;
                h.push(h.rep_ring(AC1), &other);
                assert_eq!(h.run(), StepOutcome::Idle);
            }
            assert_eq!(h.s.engine().ledger.ext_seen(AC1, B).len(), 2);
            let evicted = h.keyframe();
            let e = duplicate_report(slot, &raw);
            for _ in 0..2 {
                let mark = h.mark();
                h.push(h.rep_ring(AC1), &e);
                h.push(h.rep_ring(AC1), &raw);
                assert_eq!(h.run(), StepOutcome::Idle);
                assert_eq!(wallet(&h.s.engine().ledger), initial - 11_714);
                assert_eq!(h.s.engine().ledger.pos(AC1, B1).qty, Qty(-99_997));
                assert!(decisions(&h.since(mark)).is_empty());
                let path = if slot == 0 {
                    hft_oms::notes::code_of(hft_oms::notes::FOREIGN_FILL_VIA_ORDER_EVENT)
                } else {
                    hft_oms::notes::code_of(hft_oms::notes::STALE_GEN_FILL)
                };
                let routed: Vec<_> = h
                    .since(mark)
                    .iter()
                    .filter_map(|r| r.read::<hft_event::Note>())
                    .filter(|n| n.source == hft_event::NoteSource::Oms && n.code == path)
                    .collect();
                assert_eq!(routed.len(), 1);
                assert_eq!(routed[0].v[2], 0);
                assert!(
                    !h.since(mark)
                        .iter()
                        .any(|r| r.read::<hft_event::Note>().is_some_and(|n| {
                            matches!(
                                n.code,
                                hft_ledger::notes::code::OMS_FOREIGN_FILL
                                    | hft_ledger::notes::code::EXTERNAL_FILL
                            )
                        }))
                );
                for from in [&start, &first, &evicted] {
                    let cold = cold_at(&h, from);
                    let mut a = hft_strategy_sdk::SnapshotWriter::new(1 << 22);
                    let mut b = hft_strategy_sdk::SnapshotWriter::new(1 << 22);
                    cold.ledger().snapshot(&mut a);
                    h.s.engine().ledger.snapshot(&mut b);
                    assert_eq!(a.as_bytes(), b.as_bytes());
                }
            }
            h.keyframe();
            assert!(h.verify().is_empty());
            for from in [&start, &first, &evicted] {
                assert!(h.verify_from(Some(from.seq)).is_empty(), "window {}", from.seq);
            }
        }
    }
}

#[test]
fn seq_420_unproven_receipt_overlap_stops_without_wallet_or_hedge_change() {
    for row in 0..8 {
        let mut h = receipt_boot();
        let raw = first_execution(true);
        h.push(h.rep_ring(AC1), &raw);
        assert_eq!(h.run(), StepOutcome::Idle);
        let mut e = duplicate_report(700, &raw);
        let want = match row {
            0 => {
                e.trade_id += 1;
                hft_oms::CashError::ExternalOverlap
            }
            1 => {
                e.qty.0 += 1;
                hft_oms::CashError::ExternalOverlap
            }
            2 => {
                e.cash_sum.0 += 1;
                hft_oms::CashError::Conflict
            }
            3 => {
                e.fee.0 += 1;
                hft_oms::CashError::Conflict
            }
            4 => {
                e.detail = Side::Buy as u8;
                hft_oms::CashError::Identity
            }
            5 => {
                e.cash_qty = Qty(0);
                hft_oms::CashError::Missing
            }
            6 => {
                let mut later = raw;
                later.trade_id += 1;
                h.push(h.rep_ring(AC1), &later);
                assert_eq!(h.run(), StepOutcome::Idle);
                hft_oms::CashError::ExternalOverlap
            }
            _ => {
                // A replay carrying the same trade key under another order is not a
                // newly booked execution and must not manufacture a second proof.
                let mut alias = raw;
                alias.exch_order_id += 1;
                let prior = wallet(&h.s.engine().ledger);
                h.push(h.rep_ring(AC1), &alias);
                assert_eq!(h.run(), StepOutcome::Idle);
                assert_eq!(wallet(&h.s.engine().ledger), prior);
                e.exch_order_id = alias.exch_order_id;
                hft_oms::CashError::ExternalOverlap
            }
        };
        let before = wallet(&h.s.engine().ledger);
        let pos = *h.s.engine().ledger.pos(AC1, B1);
        let mark = h.mark();
        h.push(h.rep_ring(AC1), &e);
        assert_eq!(
            h.run(),
            StepOutcome::Fatal(FatalReason::OmsCash(want)),
            "row {row}"
        );
        assert_eq!(wallet(&h.s.engine().ledger), before);
        assert_eq!(*h.s.engine().ledger.pos(AC1, B1), pos);
        assert!(decisions(&h.since(mark)).is_empty());
    }
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
        let start = h.keyframe();
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
        let seen_before = h.s.engine().ledger.ext_seen(AC1, B);
        let mark = h.mark();
        h.push_bytes(RingId::Ctl, EventKind::RECONCILE_BATCH, body);
        // Untrusted warm-plane evidence is not applied; the engine keeps running.
        assert_eq!(h.run(), StepOutcome::Idle, "row {mutation}");
        let out = h.since(mark);
        let rejected = seq_notes(&out, hft_sequencer::codes::note::RECONCILE_REJECTED);
        assert_eq!(rejected.len(), 1, "row {mutation}");
        assert_eq!(rejected[0].level, hft_event::NoteLevel::P0);
        assert_eq!(rejected[0].v[2], 3, "row {mutation}: ledger preflight");
        let cash = seq_notes(&out, hft_sequencer::codes::note::RECONCILE_CASH);
        assert_eq!(cash.len(), 1, "row {mutation}");
        assert_eq!(cash[0].v[4], 0, "row {mutation}: applied");
        assert_eq!(wallet(&h.s.engine().ledger), wallet_before, "row {mutation}");
        assert_eq!(*h.s.engine().ledger.pos(AC1, B1), pos_before);
        assert_eq!(h.s.engine().ledger.auto_held(AC1, B1, Side::Buy), Qty(0));
        assert_eq!(h.s.engine().ledger.ext_seen(AC1, B), seen_before);
        assert!(!h.since(mark).iter().any(|r| {
            r.read::<hft_event::Note>()
                .is_some_and(|n| n.code == hft_ledger::notes::code::EXTERNAL_FILL)
        }));
        h.keyframe();
        assert_eq!(
            wallet(cold_at(&h, &start).ledger()),
            wallet_before,
            "row {mutation}"
        );
        assert!(h.verify().is_empty(), "row {mutation}");
        assert!(h.verify_from(Some(start.seq)).is_empty(), "row {mutation}");
    }
}

/// `(report_id, count, received)` of the in-flight groups, read from a fresh keyframe.
fn pending_recon(h: &mut H) -> Vec<(u64, u16, u16)> {
    let k = h.keyframe();
    hft_sequencer::snapshot::read_seq_sections(&k.body[96..])
        .unwrap()
        .recon
        .iter()
        .map(|(g, _)| (g.report_id, g.count, g.received))
        .collect()
}

fn seq_notes(recs: &[MemRecord], n: u32) -> Vec<hft_event::Note> {
    let code = hft_sequencer::codes::seq_note(n);
    recs.iter()
        .filter_map(|r| r.read::<hft_event::Note>())
        .filter(|x| x.code == code)
        .collect()
}

#[test]
fn seq_426_undecodable_or_in_flight_batch_is_not_applied_and_does_not_stop() {
    use hft_sequencer::driver::live::RingId;
    for row in 0..2u64 {
        let mut h = cash_boot();
        let start = h.keyframe();
        let open = reconcile_receipt(Side::Sell, 40, 300_000, 0);
        h.push(h.rep_ring(AC1), &open);
        assert_eq!(h.run(), StepOutcome::Idle);
        let receipt = reconcile_receipt(Side::Buy, 41, 100_000, -111);
        let mut body = reconcile_input(&mut h, &receipt, Qty(-200_000));
        if row == 0 {
            // SEQ-426.1: a truncated body (header still readable).
            body.truncate(body.len() - 1);
        } else {
            // SEQ-426.2: the same report_id is still being collected as plain reports.
            let mut part = recon_summary(AC1, B, 802, h.j().records().last().unwrap().seq);
            part.count = 3;
            h.push(hft_sequencer::driver::live::RingId::Ctl, &part);
            assert_eq!(h.run(), StepOutcome::Idle);
            assert_eq!(pending_recon(&mut h), vec![(802, 3, 1)]);
        }
        let wallet_before = wallet(&h.s.engine().ledger);
        let pos_before = *h.s.engine().ledger.pos(AC1, B1);
        let mark = h.mark();
        h.push_bytes(RingId::Ctl, EventKind::RECONCILE_BATCH, body);
        assert_eq!(h.run(), StepOutcome::Idle, "SEQ-426.{}", row + 1);
        let out = h.since(mark);
        let rejected = seq_notes(&out, hft_sequencer::codes::note::RECONCILE_REJECTED);
        assert_eq!(rejected.len(), 1, "SEQ-426.{}", row + 1);
        assert_eq!(rejected[0].v[0], 802);
        assert_eq!(rejected[0].v[2], row + 1);
        let cash = seq_notes(&out, hft_sequencer::codes::note::RECONCILE_CASH);
        if row == 0 {
            assert!(cash.is_empty());
        } else {
            assert_eq!(cash.len(), 1);
            assert_eq!(cash[0].v[4], 0);
            // The in-flight group keeps its own count.
            assert_eq!(pending_recon(&mut h), vec![(802, 3, 1)]);
        }
        assert_eq!(wallet(&h.s.engine().ledger), wallet_before);
        assert_eq!(*h.s.engine().ledger.pos(AC1, B1), pos_before);
        h.keyframe();
        assert_eq!(wallet(cold_at(&h, &start).ledger()), wallet_before);
        assert!(h.verify().is_empty(), "SEQ-426.{}", row + 1);
        assert!(h.verify_from(Some(start.seq)).is_empty(), "SEQ-426.{}", row + 1);
    }
}

#[test]
fn seq_427_receipt_confirmation_ignores_timestamps_and_mismatch_does_not_stop() {
    use hft_sequencer::driver::live::RingId;
    for row in 0..2 {
        let mut h = cash_boot();
        let start = h.keyframe();
        let open = reconcile_receipt(Side::Sell, 40, 200_000, 0);
        h.push(h.rep_ring(AC1), &open);
        assert_eq!(h.run(), StepOutcome::Idle);
        let receipt = reconcile_receipt(Side::Buy, 41, 100_000, -1_170_003);
        let body = reconcile_input(&mut h, &receipt, Qty(-100_000));
        h.push_bytes(RingId::Ctl, EventKind::RECONCILE_BATCH, body);
        assert_eq!(h.run(), StepOutcome::Idle);
        assert_eq!(h.s.engine().ledger.auto_held(AC1, B1, Side::Buy), receipt.qty);
        let wallet_before = wallet(&h.s.engine().ledger);
        let pending = h.keyframe();
        // The WS push of the same execution, with other delivery timestamps.
        let mut push = receipt;
        push.ex_ts_ns = 1_700_000_000_000;
        push.rx_tsc = hft_types::Tsc(42);
        if row == 1 {
            push.cash_amount.0 -= 1;
        }
        let mark = h.mark();
        h.push(h.rep_ring(AC1), &push);
        assert_eq!(h.run(), StepOutcome::Idle, "SEQ-427.{}", row + 1);
        let mismatch: Vec<_> = h
            .since(mark)
            .iter()
            .filter_map(|r| r.read::<hft_event::Note>())
            .filter(|n| n.code == hft_ledger::notes::code::RECEIPT_MISMATCH)
            .collect();
        assert_eq!(wallet(&h.s.engine().ledger), wallet_before);
        if row == 0 {
            assert!(mismatch.is_empty());
            assert_eq!(h.s.engine().ledger.auto_held(AC1, B1, Side::Buy), Qty(0));
        } else {
            assert_eq!(mismatch.len(), 1);
            assert_eq!(mismatch[0].v[5], u64::from(hft_ledger::receipt_field::CASH));
            assert_eq!(h.s.engine().ledger.auto_held(AC1, B1, Side::Buy), receipt.qty);
        }
        h.keyframe();
        for from in [&start, &pending] {
            assert_eq!(wallet(cold_at(&h, from).ledger()), wallet_before);
            assert!(h.verify_from(Some(from.seq)).is_empty(), "SEQ-427.{}", row + 1);
        }
        assert!(h.verify().is_empty());
    }
}

#[test]
fn seq_428_report_without_inst_does_not_stop_a_strict_cash_engine() {
    let mut h = cash_boot();
    let initial = wallet(&h.s.engine().ledger);
    let start = h.keyframe();
    let ds = place_orders(&mut h);
    // SEQ-428.1: a WS API error reply carries no inst (Binance `blank`).
    let mut e = order_event(&ds[0], OrderEventKind::Reject, h.epoch());
    e.inst = hft_types::INST_NONE;
    h.push(h.rep_ring(AC1), &e);
    assert_eq!(h.run(), StepOutcome::Idle, "SEQ-428.1");
    // SEQ-428.2: a fill without inst is booked on the slot's inst with its cash facts.
    let (_, qty, price, fee, amount) = FACTS[0];
    let mut e = order_event(&ds[0], OrderEventKind::Fill, h.epoch());
    e.inst = hft_types::INST_NONE;
    e.px = Px(price);
    e.avg_px = e.px;
    e.cum_filled = Qty(qty);
    e.cash_qty = e.cum_filled;
    e.cash_sum = Qty(amount);
    e.fee = Qty(fee);
    e.fee_asset = USDT;
    h.push(h.rep_ring(AC1), &e);
    assert_eq!(h.run(), StepOutcome::Idle, "SEQ-428.2");
    assert_eq!(wallet(&h.s.engine().ledger), initial + amount - fee);
    assert_eq!(h.s.engine().ledger.pos(AC1, B1).qty, Qty(-qty));
    h.keyframe();
    let cold = cold_at(&h, &start);
    assert_eq!(wallet(cold.ledger()), initial + amount - fee);
    assert_eq!(cold.ledger().pos(AC1, B1), h.s.engine().ledger.pos(AC1, B1));
    assert!(h.verify().is_empty());
    assert!(h.verify_from(Some(start.seq)).is_empty());
}

#[test]
fn seq_429_latest_fill_seq_survives_snapshot_recovery() {
    use hft_sequencer::driver::live::RingId;
    reset_script();
    let mut h = b0().boot_self_contained();
    h.steady();
    add_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
            && hx.v.bbo(A1).bid_px == px("50002")
        {
            let mut it = open_buy(hx, B1);
            it.side = Side::Sell;
            it.qty = Qty(100_000);
            it.px = hx.v.bbo(B1).bid_px;
            let _ = ctx.place(it);
        }
    });
    let start = h.keyframe();
    // An established, clean prior report enables AUTO_EXTERNAL.
    let seen = h.j().records().last().unwrap().seq;
    h.push(RingId::Ctl, &recon_summary(AC1, B, 801, seen));
    assert_eq!(h.run(), StepOutcome::Idle);
    assert!(h.s.engine().risk.auto_external_ok(AC1, B, 0));
    let mark = h.mark();
    h.quote(A1, "50002", "50003");
    assert_eq!(h.run(), StepOutcome::Idle);
    let d: Vec<_> = decisions(&h.since(mark))
        .into_iter()
        .filter(|(_, d)| d.inst == B1 && d.account == AC1 && d.reject == 0)
        .map(|(_, d)| d)
        .collect();
    assert_eq!(d.len(), 1);
    h.ack(&d[0]);
    assert_eq!(h.run(), StepOutcome::Idle);
    // The exchange view is taken here, before the fill.
    let view_seq = h.j().records().last().unwrap().seq;
    let mut e = order_event(&d[0], OrderEventKind::Fill, h.epoch());
    e.cum_filled = Qty(100_000);
    h.push(h.rep_ring(AC1), &e);
    assert_eq!(h.run(), StepOutcome::Idle);
    let pos = h.s.engine().ledger.pos(AC1, B1).qty;
    assert_eq!(pos, Qty(-100_000));
    let after_fill = h.keyframe();
    let secs = hft_sequencer::snapshot::read_seq_sections(&after_fill.body[96..]).unwrap();
    assert_eq!(secs.fill_seq.len(), 1, "SEQ-429.1");
    assert_eq!((secs.fill_seq[0].account, secs.fill_seq[0].inst), (AC1, B1));
    assert!(secs.fill_seq[0].seq > view_seq);
    let before = hft_sequencer::snapshot::read_seq_sections(&start.body[96..]).unwrap();
    assert!(before.fill_seq.is_empty(), "SEQ-429.2");
    // A report viewed before that fill must not AUTO_EXTERNAL it away.
    let mut summary = recon_summary(AC1, B, 802, view_seq);
    summary.count = 2;
    let mut p = summary;
    p.idx = 1;
    p.item = 1;
    p.inst = B1;
    p.engine_qty = pos;
    p.exch_qty = Qty(-50_000);
    p.sflags = hft_event::ReconcileReport::AUTO_EXTERNAL;
    h.push(RingId::Ctl, &summary);
    h.push(RingId::Ctl, &p);
    assert_eq!(h.run(), StepOutcome::Idle);
    assert_eq!(h.s.engine().ledger.pos(AC1, B1).qty, pos, "SEQ-429.3");
    // Control: the same report viewed after the fill is applied.
    let now = h.j().records().last().unwrap().seq;
    for r in [&mut summary, &mut p] {
        r.report_id = 803;
        r.view_seq = now;
    }
    h.push(RingId::Ctl, &summary);
    h.push(RingId::Ctl, &p);
    assert_eq!(h.run(), StepOutcome::Idle);
    assert_eq!(h.s.engine().ledger.pos(AC1, B1).qty, Qty(-50_000), "SEQ-429.4");
    h.keyframe();
    assert!(h.verify().is_empty());
    // Recovery from either keyframe makes the same decision as the continuous run.
    for from in [&start, &after_fill] {
        assert!(
            h.verify_from(Some(from.seq)).is_empty(),
            "SEQ-429.3 from {}",
            from.seq
        );
    }
}

#[test]
fn seq_416_reconciliation_receipts_rebuild_wallet_and_pending_confirmation() {
    use hft_sequencer::driver::live::RingId;
    use zerocopy::IntoBytes;
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
            let receipts = if remaining == 0 {
                let mut first = reconcile_receipt(side.opposite(), 41, 100_000, -600_001);
                first.fee = Qty(198);
                let mut last = reconcile_receipt(side.opposite(), 42, 100_000, -570_002);
                last.fee = Qty(199);
                vec![first, last]
            } else {
                vec![receipt]
            };
            let mut body = reconcile_input(&mut h, &receipts[0], Qty(side.sign() * remaining));
            body[10..12].copy_from_slice(&(receipts.len() as u16).to_le_bytes());
            for raw in receipts.iter().skip(1) {
                body.extend_from_slice(raw.as_bytes());
            }
            h.push_bytes(RingId::Ctl, EventKind::RECONCILE_BATCH, body.clone());
            assert_eq!(h.run(), StepOutcome::Idle);
            assert_eq!(wallet(&h.s.engine().ledger), initial - 1_170_400);
            assert_eq!(h.s.engine().ledger.pos(AC1, B1).qty, Qty(side.sign() * remaining));
            assert_eq!(h.s.engine().ledger.auto_held(AC1, B1, receipt.side), receipt.qty);
            let pending = h.keyframe();
            assert_eq!(wallet(cold_at(&h, &start).ledger()), initial - 1_170_400);
            let mut held = receipt.qty.0;
            // Confirmation may reverse execution order; each key consumes only its own receipt.
            for raw in receipts.iter().rev() {
                held -= raw.qty.0;
                for _ in 0..2 {
                    h.push(h.rep_ring(AC1), raw);
                    assert_eq!(h.run(), StepOutcome::Idle);
                    assert_eq!(h.s.engine().ledger.auto_held(AC1, B1, receipt.side), Qty(held));
                    assert_eq!(wallet(&h.s.engine().ledger), initial - 1_170_400);
                }
            }
            assert_eq!(h.s.engine().ledger.auto_held(AC1, B1, receipt.side), Qty(0));
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
        trade_id: 0,
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
#[test]
fn seq_414_bootstrap_balances_are_snapshot_baselines_not_pending_notes() {
    use hft_event::{BalanceReport, Note};
    use hft_sequencer::driver::live::RingId;
    use hft_types::Tsc;
    reset_script();
    let balances: Vec<_> = [A, B]
        .into_iter()
        .map(|venue| BalanceReport {
            venue,
            account: AC1,
            asset: USDT,
            _pad: 0,
            free: q("5000"),
            locked: Qty(0),
            ex_ts_ns: UTC0,
            rx_tsc: Tsc(T0),
        })
        .collect();
    let mut h = b0().boot_self_contained_accounts(&balances);
    let start = h.snapshots()[0].clone();
    for b in &balances {
        h.push(RingId::Rep(0), b);
        h.run();
    }
    let init_count = h
        .j()
        .records()
        .filter_map(|r| r.read::<Note>())
        .filter(|n| n.code == hft_ledger::notes::code::BALANCE_INIT)
        .count();
    assert_eq!(init_count, 0);
    let cold = cold_at(&h, &start);
    for venue in [A, B] {
        assert_eq!(
            cold.ledger().venue_book(AC1, venue).cash,
            h.s.engine().ledger.venue_book(AC1, venue).cash
        );
    }
    assert!(h.verify().is_empty());
    assert!(h.verify_from(Some(start.seq)).is_empty());
}

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

fn adopt_cmd(report_id: u64) -> hft_event::ControlCmd<'static> {
    hft_event::ControlCmd::AdoptExchange {
        account: AC1,
        venue: B,
        inst: B1,
        report_id,
        item: 0,
        _pad: 0,
        asset: hft_types::AssetId(0),
        group: hft_types::GROUP_NONE,
        entry_px: Px(0),
    }
}

/// Plain report group `report_id`: Summary + one B1 Position row (no AUTO_EXTERNAL).
fn position_report(h: &mut H, report_id: u64, view_seq: u64, exch: i64) {
    use hft_sequencer::driver::live::RingId;
    let mut summary = recon_summary(AC1, B, report_id, view_seq);
    summary.count = 2;
    let mut p = summary;
    p.idx = 1;
    p.item = 1;
    p.inst = B1;
    p.engine_qty = h.s.engine().ledger.pos(AC1, B1).qty;
    p.exch_qty = Qty(exch);
    h.push(RingId::Ctl, &summary);
    h.push(RingId::Ctl, &p);
    assert_eq!(h.run(), StepOutcome::Idle);
}

fn adopt_result(h: &mut H, report_id: u64) -> (u8, u32) {
    let id = h.cmd_confirmed(AC1, &adopt_cmd(report_id));
    assert_eq!(h.run(), StepOutcome::Idle);
    let r = h.result_of(id).unwrap();
    (r.status, r.detail)
}

#[test]
fn seq_430_adopt_stale_guard_and_retained_views_survive_recovery() {
    use hft_sequencer::codes::{detail, status};
    use hft_sequencer::driver::live::RingId;
    reset_script();
    let mut h = b0().boot_self_contained();
    h.steady();
    add_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
            && hx.v.bbo(A1).bid_px == px("50002")
        {
            let mut it = open_buy(hx, B1);
            it.side = Side::Sell;
            it.qty = Qty(100_000);
            it.px = hx.v.bbo(B1).bid_px;
            let _ = ctx.place(it);
        }
    });
    let start = h.keyframe();
    let mark = h.mark();
    h.quote(A1, "50002", "50003");
    assert_eq!(h.run(), StepOutcome::Idle);
    let d: Vec<_> = decisions(&h.since(mark))
        .into_iter()
        .filter(|(_, d)| d.inst == B1 && d.account == AC1 && d.reject == 0)
        .map(|(_, d)| d)
        .collect();
    assert_eq!(d.len(), 1);
    h.ack(&d[0]);
    assert_eq!(h.run(), StepOutcome::Idle);
    let before_fill = h.j().records().last().unwrap().seq;
    let mut e = order_event(&d[0], OrderEventKind::Fill, h.epoch());
    e.cum_filled = Qty(100_000);
    h.push(h.rep_ring(AC1), &e);
    assert_eq!(h.run(), StepOutcome::Idle);
    let now = h.j().records().last().unwrap().seq;
    // 802 was viewed before the fill, 803 after it; both see 0.0005 BTC less short.
    position_report(&mut h, 802, before_fill, -50_000);
    position_report(&mut h, 803, now, -50_000);
    let k = h.keyframe();
    let secs = hft_sequencer::snapshot::read_seq_sections(&k.body[96..]).unwrap();
    let ids: Vec<u64> = secs.recon_views.iter().map(|(v, _)| v.report_id).collect();
    // Startup reconciliation groups precede them, oldest first.
    assert_eq!(ids[ids.len() - 2..], [802, 803], "SEQ-430.1");
    assert!(
        !hft_sequencer::snapshot::read_seq_sections(&start.body[96..])
            .unwrap()
            .recon_views
            .iter()
            .any(|(v, _)| matches!(v.report_id, 802 | 803))
    );
    // .2 the stale report is refused, the fresh one applies its retained diff.
    assert_eq!(
        adopt_result(&mut h, 802),
        (status::PRECONDITION_FAILED, u32::from(detail::STALE_REPORT)),
        "SEQ-430.2"
    );
    assert_eq!(h.s.engine().ledger.pos(AC1, B1).qty, Qty(-100_000));
    assert_eq!(adopt_result(&mut h, 803), (status::APPLIED, 0), "SEQ-430.3");
    assert_eq!(h.s.engine().ledger.pos(AC1, B1).qty, Qty(-50_000));
    // .4 an evicted report (oldest of more than RECON_VIEW_REPORTS) cannot be adopted.
    let seen = h.j().records().last().unwrap().seq;
    position_report(&mut h, 804, seen, -40_000);
    for id in 0..hft_sequencer::RECON_VIEW_REPORTS as u64 {
        h.push(RingId::Ctl, &recon_summary(AC1, B, 900 + id, seen));
        assert_eq!(h.run(), StepOutcome::Idle);
    }
    assert_eq!(
        adopt_result(&mut h, 804),
        (status::PRECONDITION_FAILED, u32::from(detail::STALE_REPORT)),
        "SEQ-430.4"
    );
    assert_eq!(h.s.engine().ledger.pos(AC1, B1).qty, Qty(-50_000));
    h.keyframe();
    assert!(h.verify().is_empty());
    for from in [&start, &k] {
        assert!(
            h.verify_from(Some(from.seq)).is_empty(),
            "SEQ-430.3 from {}",
            from.seq
        );
    }
}

#[test]
fn seq_431_recon_view_table_overflow_is_defined_and_bounded() {
    use zerocopy::FromZeros;
    let mut t = hft_sequencer::ReconViews::default();
    let row = |report_id: u64, inst: u32| {
        let mut r = hft_event::ReconcileReport::new_zeroed();
        r.report_id = report_id;
        r.account = AC1;
        r.venue = B;
        r.item = 1;
        r.inst = hft_types::InstId(inst);
        r.view_seq = 7;
        r.engine_qty = Qty(10);
        r.exch_qty = Qty(4);
        r
    };
    // .1 an older report is evicted to make room for the current one's rows.
    assert!(!t.record(&row(1, 1), true));
    for i in 0..hft_sequencer::RECON_VIEW_DIFFS as u32 {
        assert!(!t.record(&row(2, i + 1), true), "SEQ-431.1 row {i}");
    }
    assert!(t.view(1).is_none(), "SEQ-431.1");
    assert_eq!(t.diff(2, AC1, hft_types::InstId(5)), Qty(-6));
    // .2 one report alone exceeding the capacity overflows once; its diffs are dropped.
    assert!(t.record(&row(2, u32::MAX), true), "SEQ-431.2");
    assert!(!t.record(&row(2, u32::MAX - 1), true));
    let v = t.view(2).unwrap();
    assert!(v.overflow, "SEQ-431.2");
    assert_eq!(t.diff(2, AC1, hft_types::InstId(5)), Qty(0));
    // .3 more than RECON_VIEW_REPORTS reports keep only the newest.
    for id in 10..10 + hft_sequencer::RECON_VIEW_REPORTS as u64 + 3 {
        t.record(&row(id, 1), false);
    }
    assert_eq!(t.views().count(), hft_sequencer::RECON_VIEW_REPORTS, "SEQ-431.3");
    assert!(t.view(2).is_none() && t.view(12).is_none() && t.view(13).is_some());
}
