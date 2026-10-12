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
    h.ev(&e, 5);
    assert_eq!(h.out.cash_error, Some(CashError::ExternalOverlap));
    assert_eq!(snap(&h.o), before);
}
