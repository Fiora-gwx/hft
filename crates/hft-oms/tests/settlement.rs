use crate::common::*;
use hft_event::{OrderEvent, OrderEventKind};
use hft_oms::{CashError, Oms, OmsOut, OrderState, SECTION_OMS_LINKS, SECTION_ORDER_SLOTS};
use hft_refdata::RefData;
use hft_strategy_sdk::{OrderRef, SnapshotReader, SnapshotWriter};
use hft_types::{Qty, SlotId};

fn fixture() -> (H, &'static RefData) {
    let rd = build_rd(&RdOpts {
        vx_perp_cash: true,
        ..RdOpts::default()
    });
    let mut cfg = p0();
    cfg.slots.set(VX, A1, 1);
    (H::from_cfg(&cfg, rd), rd)
}

fn snap(o: &Oms) -> Vec<u8> {
    let mut w = SnapshotWriter::new(1 << 22);
    o.snapshot(&mut w);
    w.as_bytes().to_vec()
}

fn restore(bytes: &[u8], rd: &RefData) -> Oms {
    let mut cfg = p0();
    cfg.slots.set(VX, A1, 1);
    Oms::restore(&mut SnapshotReader::new(bytes), &cfg, rd).unwrap()
}

fn event(h: &H, order: OrderRef, units: i64, cash: i64) -> OrderEvent {
    let mut e = h.oe(order, OrderEventKind::PartialFill);
    e.cum_filled = Qty(units * 1_000_000);
    e.avg_px = p("60100.00");
    e.cash_qty = e.cum_filled;
    e.cash_sum = Qty(cash);
    e.exch_order_id = 900;
    e
}

fn original_external(eid: u64) -> hft_event::ExternalFill {
    hft_event::ExternalFill {
        venue: VX,
        account: A1,
        side: hft_types::Side::Sell,
        cause: hft_event::ExternalCause::Unknown,
        inst: Y,
        qty: q("0.01"),
        px: p("60000"),
        fee: Qty(0),
        fee_asset: hft_types::AssetId(0),
        _pad: [0; 6],
        exch_order_id: eid,
        trade_id: 1,
        ex_ts_ns: 0,
        rx_tsc: hft_types::Tsc(0),
        fill_seq: 0,
        cash_qty: q("0.01"),
        cash_amount: Qty(0),
    }
}

fn original_copy(h: &H, r: OrderRef, f: &hft_event::ExternalFill, stale: bool) -> OrderEvent {
    let mut e = h.oe(r, OrderEventKind::Fill);
    e.slot = if stale { SlotId(r.slot) } else { SlotId(0) };
    e.r#gen += 7;
    e.detail = f.side as u8;
    e.exch_order_id = f.exch_order_id;
    e.trade_id = f.trade_id;
    e.qty = f.qty;
    e.cum_filled = f.qty;
    e.px = f.px;
    e.avg_px = f.px;
    e.cash_qty = f.cash_qty;
    e.cash_sum = f.cash_amount;
    e.fee = f.fee;
    e.fee_asset = f.fee_asset;
    e
}

#[test]
fn oms_477_original_receipt_proves_only_the_same_first_execution_after_restore() {
    for stale in [false, true] {
        for positive_id in [false, true] {
            for cash in [-11_700, 0, 11_800] {
                for fee in [-17, 0, 17] {
                    let (mut h, rd) = fixture();
                    let r = h.place_ok(&sell_y(), 0);
                    let mut f = original_external(9900);
                    f.trade_id = u64::from(positive_id);
                    f.fill_seq = if positive_id { 0 } else { f.qty.0 as u64 };
                    f.cash_amount = Qty(cash);
                    f.fee = Qty(fee);
                    f.fee_asset = USDT;
                    assert_eq!(h.o.external_cash_guard(&f), Ok(false));
                    h.o.remember_external_cash(&f, true).unwrap();
                    h.o.external_adjust(
                        A1,
                        VX,
                        g1(),
                        Y,
                        Qty(-f.qty.0),
                        f.exch_order_id,
                        hft_event::ExternalCause::Unknown,
                        &mut h.out,
                    );
                    let e = original_copy(&h, r, &f, stale);
                    let before = snap(&h.o);
                    assert_eq!(
                        SnapshotReader::new(&before)
                            .find_section(SECTION_OMS_LINKS)
                            .unwrap()
                            .unwrap()
                            .version,
                        4
                    );
                    for _ in 0..2 {
                        h.o = restore(&snap(&h.o), rd);
                        h.ev(&e, 5);
                        assert_eq!(h.out.cash_error, None);
                        assert!(
                            h.out.fills.is_empty() && h.out.foreign_fills.is_empty() && h.out.fees.is_empty()
                        );
                        let path = if stale {
                            hft_oms::notes::code_of(hft_oms::notes::STALE_GEN_FILL)
                        } else {
                            hft_oms::notes::code_of(hft_oms::notes::FOREIGN_FILL_VIA_ORDER_EVENT)
                        };
                        let routed: Vec<_> = h.out.notes.iter().filter(|n| n.code == path).collect();
                        assert_eq!(routed.len(), 1);
                        assert_eq!(routed[0].v[2], 0);
                        assert!(h.out.notes.iter().all(|n| {
                            ![
                                hft_oms::notes::code_of(hft_oms::notes::FOREIGN_FILL),
                                hft_oms::notes::code_of(hft_oms::notes::EXT_DEDUP),
                            ]
                            .contains(&n.code)
                        }));
                        assert_eq!(snap(&h.o), before);
                        assert_eq!(h.o.external_cash_guard(&f), Ok(true));
                    }
                }
            }
        }
    }
}

#[test]
fn oms_478_original_receipt_never_authorizes_wrong_or_missing_facts() {
    for row in 0..13 {
        let (mut h, rd) = fixture();
        let r = h.place_ok(&sell_y(), 0);
        let mut f = original_external(9900);
        f.fee_asset = USDT;
        f.fee = Qty(17);
        f.cash_amount = Qty(-11_700);
        h.o.remember_external_cash(&f, row != 11).unwrap();
        let mut e = original_copy(&h, r, &f, true);
        let want = match row {
            0 => {
                e.trade_id += 1;
                CashError::ExternalOverlap
            }
            1 => {
                e.qty.0 += 1;
                CashError::ExternalOverlap
            }
            2 => {
                e.cum_filled.0 += 1;
                e.cash_qty = e.cum_filled;
                CashError::ExternalOverlap
            }
            3 => {
                e.cash_qty = Qty(0);
                CashError::Missing
            }
            4 => {
                e.cash_sum.0 += 1;
                CashError::Conflict
            }
            5 => {
                e.fee.0 += 1;
                CashError::Conflict
            }
            6 => {
                e.fee_asset = BTC;
                CashError::Conflict
            }
            7 => {
                e.detail = 1;
                CashError::Identity
            }
            8 => {
                e.avg_px.0 += 1;
                CashError::Conflict
            }
            9 => {
                e.what = OrderEventKind::QueryResult;
                CashError::Untracked
            }
            10 => {
                e.r#gen = r.r#gen;
                CashError::ExternalOverlap
            }
            11 => CashError::ExternalOverlap,
            _ => {
                let mut later = f;
                later.trade_id += 1;
                h.o.remember_external_cash(&later, true).unwrap();
                // Restoring and replaying the first raw receipt must not recreate lost proof.
                h.o = restore(&snap(&h.o), rd);
                h.o.remember_external_cash(&f, false).unwrap();
                CashError::ExternalOverlap
            }
        };
        h.o = restore(&snap(&h.o), rd);
        let before = snap(&h.o);
        h.ev(&e, 4);
        assert_eq!(h.out.cash_error, Some(want), "row {row}");
        assert_eq!(snap(&h.o), before, "row {row}");
        assert!(h.out.fills.is_empty() && h.out.foreign_fills.is_empty() && h.out.fees.is_empty());
    }
    let (mut h, rd) = fixture();
    let f = original_external(9900);
    h.o.remember_external_cash(&f, true).unwrap();
    h.o = restore(&snap(&h.o), rd);
    let before = snap(&h.o);
    let mut conflict = f;
    conflict.cash_amount.0 += 1;
    assert_eq!(h.o.external_cash_guard(&conflict), Err(CashError::Conflict));
    assert_eq!(
        h.o.remember_external_cash(&conflict, false),
        Err(CashError::Conflict)
    );
    assert_eq!(snap(&h.o), before);
}

fn receipt_snapshot(bytes: &[u8], mode: u8) -> Vec<u8> {
    let reader = SnapshotReader::new(bytes);
    let mut w = SnapshotWriter::new(1 << 22);
    for id in [
        SECTION_ORDER_SLOTS,
        hft_oms::SECTION_OMS_COUNTERS,
        SECTION_OMS_LINKS,
    ] {
        let s = reader.find_section(id).unwrap().unwrap();
        w.begin_section(
            id,
            if id == SECTION_OMS_LINKS && matches!(mode, 11 | 13) {
                3
            } else {
                s.version
            },
        );
        if id != SECTION_OMS_LINKS {
            w.put_bytes(s.body);
            w.add_rows(s.count);
        } else {
            let mut tail = s.body;
            for _ in 0..s.count {
                let len = u32::from_le_bytes(tail[4..8].try_into().unwrap()) as usize;
                let mut row = tail[..8 + len].to_vec();
                tail = &tail[8 + len..];
                let kind = row[0];
                if (kind == 6 && mode == 10) || (kind == 7 && mode == 13) {
                    continue;
                }
                if kind == 7 {
                    let b = &mut row[8..];
                    match mode {
                        0 => b[3] = 1,
                        1 => b[2] = 3,
                        2 => b[4..8].copy_from_slice(&u32::MAX.to_le_bytes()),
                        3 => b[8] ^= 1,
                        4 => b[32..40].fill(0),
                        5 => b[40..48].fill(0),
                        6 => b[24] = 1,
                        7 => b[66] = 1,
                        8 => b[64..66].fill(0xFF),
                        _ => {}
                    }
                }
                if kind == 6 && mode == 12 {
                    row[10] = 1;
                    row[24..32].copy_from_slice(&1i64.to_le_bytes());
                }
                w.put_bytes(&row);
                w.add_rows(1);
                if kind == 7 && mode == 9 {
                    w.put_bytes(&row);
                    w.add_rows(1);
                }
            }
        }
        w.end_section();
    }
    w.as_bytes().to_vec()
}

#[test]
fn oms_479_receipt_restore_rejects_corruption_and_legacy_has_no_proof() {
    let (mut h, rd) = fixture();
    let r = h.place_ok(&sell_y(), 0);
    let f = original_external(9900);
    let e = original_copy(&h, r, &f, true);
    h.o.remember_external_cash(&f, true).unwrap();
    let good = snap(&h.o);
    let mut cfg = p0();
    cfg.slots.set(VX, A1, 1);
    for mode in 0..13 {
        let bytes = receipt_snapshot(&good, mode);
        assert!(
            Oms::restore(&mut SnapshotReader::new(&bytes), &cfg, rd).is_err(),
            "mode {mode}"
        );
    }
    let old = receipt_snapshot(&good, 13);
    h.o = restore(&old, rd);
    assert_eq!(h.o.external_cash_guard(&f), Ok(false));
    h.o.remember_external_cash(&f, false).unwrap();
    h.ev(&e, 3);
    assert_eq!(h.out.cash_error, Some(CashError::ExternalOverlap));
    assert!(h.out.foreign_fills.is_empty());
}

#[test]
fn oms_476_reconcile_cash_capacity_counts_distinct_order_identities() {
    use hft_event::{ReconcileBatchHdr, ReconcileBatchView, ReconcileReport};
    use zerocopy::{FromZeros, IntoBytes};
    // Provenance is only retained for a strict-cash venue (OMS-483).
    let mut cfg = p0();
    cfg.foreign_fill_track.set(VX, A1, 2);
    let rd = build_rd(&RdOpts {
        vx_perp_cash: true,
        ..RdOpts::default()
    });
    let mut h = H::from_cfg(&cfg, rd);
    // An order identity without single-execution proof (as before on the non-cash fixture).
    h.o.remember_external_cash(&original_external(70), false).unwrap();
    let mut summary = ReconcileReport::new_zeroed();
    summary.report_id = 7;
    summary.venue = VX;
    summary.account = A1;
    summary.item = 5;
    summary.count = 2;
    let mut p = summary;
    p.item = 1;
    p.idx = 1;
    p.inst = Y;
    p.sflags = ReconcileReport::AUTO_EXTERNAL;
    let hdr = ReconcileBatchHdr {
        report_id: 7,
        n_reports: 2,
        n_fills: 2,
        venue: VX,
        account: A1,
        _pad: 0,
    };
    let first = original_external(71);
    let before = snap(&h.o);
    for (eid, expected) in [(71, Ok(())), (72, Err(CashError::Capacity)), (70, Ok(()))] {
        let second = original_external(eid);
        let body = [
            hdr.as_bytes(),
            summary.as_bytes(),
            p.as_bytes(),
            first.as_bytes(),
            second.as_bytes(),
        ]
        .concat();
        assert_eq!(
            h.o.reconcile_cash_guard(&ReconcileBatchView::read(&body).unwrap()),
            expected
        );
        assert_eq!(snap(&h.o), before);
    }
}

#[test]
fn oms_467_cash_cumulative_queries_and_permutations() {
    let permutations = [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]];
    for perm in permutations {
        let (mut h, _) = fixture();
        let mut it = sell_y();
        it.intent.qty = q("0.03");
        let r = h.place_ok(&it, 0);
        let mut events = [
            event(&h, r, 1, 0),
            event(&h, r, 2, -11700),
            event(&h, r, 3, -11800),
        ];
        events[1].what = OrderEventKind::QueryResult;
        events[1].detail = 2; // PartiallyFilled
        events[2].what = OrderEventKind::Fill;
        let mut cash = 0;
        let mut qty = 0;
        for (i, k) in perm.into_iter().enumerate() {
            h.ev(&events[k], i as u64 + 1);
            assert_eq!(h.out.cash_error, None, "{perm:?}");
            for f in h.out.fills.iter() {
                cash += f.cash.unwrap().0;
                qty += f.qty.0;
            }
        }
        assert_eq!((cash, qty), (-11800, 3_000_000), "{perm:?}");
        assert_eq!((h.ext(r).cash_qty, h.ext(r).cash_sum), (q("0.03"), Qty(-11800)));
        h.ev(&events[2], 10);
        assert!(h.out.fills.is_empty());
        assert_eq!(h.out.cash_error, None);
        // FeeOnly is independent of the cumulative cash watermark.
        let mut fee = events[2];
        fee.what = OrderEventKind::FeeOnly;
        fee.fee = Qty(123);
        fee.fee_asset = USDT;
        h.ev(&fee, 11);
        assert_eq!(h.out.fees.len(), 1);
        assert!(h.out.fills.is_empty());
        assert_eq!(h.ext(r).cash_sum, Qty(-11800));
    }
}

#[test]
fn oms_468_cash_bad_facts_leave_slot_unchanged() {
    let (mut h, rd) = fixture();
    let mut it = sell_y();
    it.intent.qty = q("0.03");
    let r = h.place_ok(&it, 0);
    let first = event(&h, r, 1, -11700);
    h.ev(&first, 1);
    let before = snap(&h.o);
    let next = event(&h, r, 2, -11800);
    let mut rows = Vec::new();
    let mut e = next;
    e.cash_qty = Qty(0);
    e.cash_sum = Qty(0);
    rows.push((e, CashError::Missing));
    let mut e = next;
    e.cash_qty = q("0.01");
    rows.push((e, CashError::Missing));
    let mut e = next;
    e.cash_qty = q("0.04");
    rows.push((e, CashError::Coverage));
    let mut e = next;
    e.cash_qty = Qty(-1);
    rows.push((e, CashError::Coverage));
    let mut e = next;
    e.cash_qty = Qty(0);
    rows.push((e, CashError::Coverage));
    let mut e = first;
    e.cash_sum = Qty(-11701);
    e.fee = Qty(900);
    e.fee_asset = USDT;
    rows.push((e, CashError::Conflict));
    let mut e = next;
    e.cash_sum = Qty(i64::MAX);
    rows.push((e, CashError::Overflow));
    let mut e = next;
    e.inst = Z;
    rows.push((e, CashError::Identity));
    let mut e = next;
    e.epoch += 1;
    rows.push((e, CashError::Untracked));
    for (i, (ev, want)) in rows.into_iter().enumerate() {
        h.o = restore(&before, rd);
        h.ev(&ev, 2);
        assert_eq!(h.out.cash_error, Some(want), "row {i}");
        assert_eq!(snap(&h.o), before, "row {i}");
        assert!(h.out.fills.is_empty() && h.out.fees.is_empty() && h.out.hedges.is_empty());
        assert!(!h.out.is_empty());
        h.out.clear();
        assert!(h.out.is_empty());
    }
}

#[test]
fn oms_469_cash_tomb_reuse_and_restore() {
    let (mut h, rd) = fixture();
    let mut it = sell_y();
    it.intent.qty = q("0.02");
    let r = h.place_ok(&it, 0);
    let mut first = event(&h, r, 1, -11700);
    h.ev(&first, 1);
    h.o = restore(&snap(&h.o), rd);
    h.ev(&first, 2);
    assert!(h.out.fills.is_empty());
    assert_eq!(h.out.cash_error, None);
    first.what = OrderEventKind::CancelAck;
    h.ev(&first, 3);
    h.sweep(5004);
    let next = h.place_ok(&sell_y(), 5005);
    assert_eq!(next.slot, r.slot);
    assert_ne!(next.r#gen, r.r#gen);
    let tb = h.o.tomb(VX, A1, r.slot, r.r#gen).unwrap();
    assert_eq!((tb.cash_qty, tb.cash_sum), (q("0.01"), Qty(-11700)));
    let saved = snap(&h.o);
    let mut resumed = H {
        o: restore(&saved, rd),
        out: OmsOut::new(),
        seq: h.seq,
    };
    assert_eq!(snap(&resumed.o), saved);
    let mut late = first;
    late.what = OrderEventKind::PartialFill;
    late.cum_filled = q("0.02");
    late.cash_qty = late.cum_filled;
    late.cash_sum = Qty(-11800);
    for state in [&mut h, &mut resumed] {
        state.ev(&late, 5006);
        assert_eq!(state.out.cash_error, None);
        assert_eq!(state.out.fills.len(), 1);
        assert_eq!(state.out.fills[0].cash, Some(Qty(-100)));
        assert_eq!(state.ext(next).cash_qty, Qty(0));
        assert_eq!(state.slot(next).filled, Qty(0));
        assert!(state.out.hedges.is_empty() && state.out.decisions.is_empty());
    }
    assert_eq!(snap(&h.o), snap(&resumed.o));
    resumed.o = restore(&snap(&resumed.o), rd);
    resumed.ev(&late, 5007);
    assert_eq!(resumed.out.cash_error, None);
    assert!(resumed.out.fills.is_empty());
    late.cash_sum = Qty(-11801);
    let before = snap(&resumed.o);
    resumed.ev(&late, 5008);
    assert_eq!(resumed.out.cash_error, Some(CashError::Conflict));
    assert_eq!(snap(&resumed.o), before);
}

/// Rewrite only the known OMS rows as old versions, keeping all other sections intact.
fn legacy(bytes: &[u8], slots_version: u16) -> Vec<u8> {
    let mut w = SnapshotWriter::new(1 << 22);
    let r = SnapshotReader::new(bytes);
    for id in [
        SECTION_ORDER_SLOTS,
        hft_oms::SECTION_OMS_COUNTERS,
        SECTION_OMS_LINKS,
    ] {
        let sec = r.find_section(id).unwrap().unwrap();
        let ver = match id {
            SECTION_ORDER_SLOTS => slots_version,
            SECTION_OMS_LINKS => 1,
            _ => sec.version,
        };
        w.begin_section(id, ver);
        if id == SECTION_ORDER_SLOTS {
            for row in sec.body.chunks_exact(192).take(sec.count as usize) {
                let mut b = row.to_vec();
                b[168..184].fill(0);
                if slots_version == 1 {
                    b.truncate(160);
                }
                w.put_bytes(&b);
                w.add_rows(1);
            }
        } else if id == SECTION_OMS_LINKS {
            let mut rest = sec.body;
            for _ in 0..sec.count {
                let mut hd = rest[..8].to_vec();
                let len = u32::from_le_bytes(hd[4..8].try_into().unwrap()) as usize;
                let body = &rest[8..8 + len];
                if hd[0] == 4 {
                    hd[4..8].copy_from_slice(&64u32.to_le_bytes());
                    w.put_bytes(&hd);
                    w.put_bytes(&body[..64]);
                } else {
                    w.put_bytes(&hd);
                    w.put_bytes(body);
                }
                w.add_rows(1);
                rest = &rest[8 + len..];
            }
        } else {
            w.put_bytes(sec.body);
            for _ in 0..sec.count {
                w.add_rows(1);
            }
        }
        w.end_section();
    }
    w.as_bytes().to_vec()
}

#[test]
fn oms_470_cash_legacy_snapshots_and_unproven_paths() {
    for recycled in [false, true] {
        let (mut h, strict_rd) = fixture();
        let r = h.place_ok(&sell_y(), 0);
        let first = event(&h, r, 1, 0);
        h.ev(&first, 1);
        assert_eq!(h.out.fills[0].cash, Some(Qty(0)));
        if recycled {
            h.sweep(5002);
        }
        let bytes = snap(&h.o);
        for version in [1, 2] {
            let old = legacy(&bytes, version);
            let mut cfg = p0();
            cfg.slots.set(VX, A1, 1);
            assert!(Oms::restore(&mut SnapshotReader::new(&old), &cfg, strict_rd).is_err());
            let legacy_rd = rd();
            assert!(Oms::restore(&mut SnapshotReader::new(&old), &cfg, legacy_rd).is_ok());
        }
    }
    let (mut h, _) = fixture();
    let r = h.place_ok(&sell_y(), 0);
    h.what(r, OrderEventKind::OutcomeUnknown, 1);
    let before = snap(&h.o);
    h.recon(&recon(r, 4, 3, 1, q("0.01")), 2);
    assert_eq!(h.out.cash_error, Some(CashError::Missing));
    assert_eq!(snap(&h.o), before);
    assert_eq!(h.state(r), OrderState::Unknown);
    let mut e = event(&h, r, 1, 10);
    e.slot = SlotId(0);
    e.exch_order_id = 0;
    h.ev(&e, 3);
    assert_eq!(h.out.cash_error, Some(CashError::Untracked));
    assert!(h.out.foreign_fills.is_empty());
    h.ack(r, 900, 4);
    h.out.clear();
    h.o.external_adjust(
        A1,
        VX,
        g1(),
        Y,
        q("0.01"),
        900,
        hft_event::ExternalCause::Unknown,
        &mut h.out,
    );
    let before = snap(&h.o);
    e.slot = SlotId(r.slot);
    e.exch_order_id = 900;
    h.ev(&e, 5);
    assert_eq!(h.out.cash_error, Some(CashError::ExternalOverlap));
    assert_eq!(snap(&h.o), before);
}

#[test]
fn oms_471_foreign_cash_cursors_survive_restore_and_reordered_reports() {
    for stale in [false, true] {
        for perm in [[0, 1, 2], [2, 0, 1], [1, 2, 0]] {
            let (mut h, rd) = fixture();
            let r = h.place_ok(&sell_y(), 0);
            let mut saved = Vec::new();
            let mut cash = 0;
            let mut qty = 0;
            for (step, i) in perm.into_iter().enumerate() {
                let mut e = event(&h, r, i + 1, [0, -11700, -11800][i as usize]);
                e.exch_order_id = 9900;
                e.detail = 2;
                e.slot = if stale { SlotId(r.slot) } else { SlotId(0) };
                e.r#gen += 7;
                h.ev(&e, step as u64 + 1);
                assert_eq!(h.out.cash_error, None);
                for f in h.out.foreign_fills.as_slice() {
                    cash += f.cash.unwrap().0;
                    qty += f.qty.0;
                }
                saved.push(e);
                let bytes = snap(&h.o);
                h.o = restore(&bytes, rd);
                assert_eq!(snap(&h.o), bytes);
            }
            assert_eq!((qty, cash), (3_000_000, -11800));
            for e in saved {
                h.ev(&e, 9);
                assert_eq!(h.out.cash_error, None);
                assert!(h.out.foreign_fills.is_empty());
                let path = if stale {
                    hft_oms::notes::code_of(hft_oms::notes::STALE_GEN_FILL)
                } else {
                    hft_oms::notes::code_of(hft_oms::notes::FOREIGN_FILL_VIA_ORDER_EVENT)
                };
                let routed: Vec<_> = h.out.notes.iter().filter(|n| n.code == path).collect();
                assert_eq!(routed.len(), 1);
                assert_eq!(routed[0].v[2], 0);
            }
        }
    }
}

#[test]
fn oms_472_foreign_cash_bad_facts_and_cross_path_aliases_are_atomic() {
    for row in 0..12 {
        let (mut h, rd) = fixture();
        let r = h.place_ok(&sell_y(), 0);
        let mut first = event(&h, r, 1, -11700);
        first.slot = SlotId(0);
        first.detail = 2;
        first.exch_order_id = 9900;
        h.ev(&first, 1);
        assert_eq!(h.out.cash_error, None);
        h.o = restore(&snap(&h.o), rd);
        let mut e = first;
        e.cum_filled.0 *= 2;
        e.cash_qty = e.cum_filled;
        e.cash_sum = Qty(-11800);
        let want = match row {
            0 => {
                e.cash_qty = Qty(0);
                e.cash_sum = Qty(0);
                CashError::Missing
            }
            1 => {
                e.cash_qty.0 += 1;
                CashError::Coverage
            }
            2 => {
                e = first;
                e.cash_sum.0 += 1;
                CashError::Conflict
            }
            3 => {
                e.cash_sum = Qty(i64::MAX);
                CashError::Overflow
            }
            4 => {
                e.detail = 1;
                CashError::Identity
            }
            5 => {
                e.exch_order_id = 0;
                CashError::Untracked
            }
            6 => {
                e.avg_px = hft_types::Px(0);
                e.px = e.avg_px;
                CashError::Identity
            }
            7 => {
                e.what = OrderEventKind::QueryResult;
                CashError::Untracked
            }
            8 => {
                e.slot = SlotId(r.slot);
                e.r#gen = r.r#gen;
                CashError::ExternalOverlap
            }
            9 | 10 => {
                e.exch_order_id = 9901;
                let ext = original_external(e.exch_order_id);
                h.o.external_cash_guard(&ext).unwrap();
                h.o.remember_external_cash(&ext, true).unwrap();
                h.o = restore(&snap(&h.o), rd);
                if row == 10 {
                    e.slot = SlotId(r.slot);
                    e.r#gen = r.r#gen;
                }
                CashError::ExternalOverlap
            }
            _ => {
                e.what = OrderEventKind::FeeOnly;
                e.fee = Qty(1);
                CashError::Untracked
            }
        };
        let before = snap(&h.o);
        h.ev(&e, 3);
        assert_eq!(h.out.cash_error, Some(want), "row {row}");
        assert_eq!(snap(&h.o), before, "row {row}");
        assert!(h.out.foreign_fills.is_empty());
        assert!(h.out.fills.is_empty());
        assert!(h.out.fees.is_empty());
    }
}

#[test]
fn oms_473_foreign_cash_capacity_never_evicts_a_booked_identity() {
    let rd = build_rd(&RdOpts {
        vx_perp_cash: true,
        ..RdOpts::default()
    });
    let mut cfg = p0();
    cfg.slots.set(VX, A1, 1);
    cfg.foreign_fill_track.set(VX, A1, 1);
    let mut h = H::from_cfg(&cfg, rd);
    let r = h.place_ok(&sell_y(), 0);
    let mut e = event(&h, r, 1, 0);
    e.slot = SlotId(0);
    e.detail = 2;
    e.exch_order_id = 9900;
    h.ev(&e, 1);
    assert_eq!(h.out.cash_error, None);
    let before = snap(&h.o);
    e.exch_order_id += 1;
    h.ev(&e, 2);
    assert_eq!(h.out.cash_error, Some(CashError::Capacity));
    assert_eq!(snap(&h.o), before);
    e.exch_order_id -= 1;
    h.ev(&e, 3);
    assert_eq!(h.out.cash_error, None);
    assert!(h.out.foreign_fills.is_empty());
    let mut ext = original_external(9900);
    assert_eq!(h.o.external_cash_guard(&ext), Err(CashError::ExternalOverlap));
    ext.exch_order_id += 1;
    assert_eq!(h.o.external_cash_guard(&ext), Err(CashError::Capacity));
}

#[test]
fn oms_475_sim_reset_clears_only_that_accounts_foreign_cash() {
    let rd = build_rd(&RdOpts {
        vx_perp_cash: true,
        s1_perp: true,
        ..RdOpts::default()
    });
    let mut h = H::from_cfg(&p0(), rd);
    assert!(h.o.table_view(VX, S1).is_some());
    let mut raw = original_external(9901);
    h.o.remember_external_cash(&raw, true).unwrap();
    raw.account = S1;
    h.o.remember_external_cash(&raw, true).unwrap();
    assert_eq!(h.o.external_cash_guard(&raw), Ok(true));
    let r = h.place_ok(&sell_y(), 0);
    let mut e = event(&h, r, 1, -11700);
    e.slot = SlotId(0);
    e.detail = 2;
    e.exch_order_id = 9900;
    h.ev(&e, 1);
    assert_eq!(h.out.cash_error, None);
    assert_eq!(h.out.foreign_fills.len(), 1);
    e.account = S1;
    h.ev(&e, 2);
    assert_eq!(h.out.cash_error, None);
    assert_eq!(h.out.foreign_fills.len(), 1);
    h.o.sim_reset(S1).unwrap();
    assert_eq!(h.o.external_cash_guard(&raw), Ok(false));
    raw.account = A1;
    assert_eq!(h.o.external_cash_guard(&raw), Ok(true));
    assert!(h.o.has_foreign_cash(VX, A1, Y, e.exch_order_id));
    assert!(!h.o.has_foreign_cash(VX, S1, Y, e.exch_order_id));
    h.ev(&e, 3);
    assert_eq!(h.out.cash_error, None);
    assert_eq!(h.out.foreign_fills[0].cash, Some(Qty(-11700)));
    e.account = A1;
    h.ev(&e, 4);
    assert!(h.out.foreign_fills.is_empty());
}

#[test]
fn oms_483_external_cash_guard_rejects_rows_the_restore_would_refuse() {
    let (mut h, rd) = fixture();
    let before = snap(&h.o);
    let good = original_external(9901);
    assert_eq!(h.o.external_cash_guard(&good), Ok(false), "OMS-483.1");
    // .2 a spot inst of a non-cash venue reported under the strict-cash venue.
    let mut e = good;
    e.inst = X;
    // .3 the strict-cash inst reported under another venue.
    let mut f = good;
    f.venue = VS;
    for (row, x) in [("OMS-483.2", e), ("OMS-483.3", f)] {
        assert_eq!(h.o.external_cash_guard(&x), Err(CashError::Identity), "{row}");
        assert_eq!(
            h.o.remember_external_cash(&x, true),
            Err(CashError::Identity),
            "{row}"
        );
        assert_eq!(snap(&h.o), before, "{row}");
        restore(&snap(&h.o), rd);
    }
    // The accepted identity still restores.
    h.o.remember_external_cash(&good, true).unwrap();
    let after = snap(&h.o);
    assert_ne!(after, before);
    assert_eq!(snap(&restore(&after, rd)), after, "OMS-483.1");
}
