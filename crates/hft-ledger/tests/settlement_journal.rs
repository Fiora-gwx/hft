//! Explicit execution cash through journal consumption and OMS recovery rows.
use crate::common::journal::*;
use crate::common::*;
use hft_event::{Body, OrderEvent, OrderEventKind, RecordFlags, decode_record, encode_fixed};
use hft_ledger::{JournalApplyError, JournalLedger, Ledger, SettlementError};
use hft_refdata::RefData;
use hft_strategy_sdk::{RestoreError, SnapshotReader, SnapshotWriter};
use hft_types::{InstId, Qty};
use zerocopy::{FromZeros, IntoBytes};

fn rd_cash() -> &'static RefData {
    rd_with(&RdOpts {
        p2_cash: true,
        ..RdOpts::default()
    })
}

fn cold(l: &Ledger, rd: &RefData) -> JournalLedger {
    JournalLedger::from_snapshot(&mut SnapshotReader::new(&snap(l)), rd, &cfg_lf(), &caps_lf()).unwrap()
}

fn apply<B: Body>(j: &mut JournalLedger, b: &B) -> Result<(), JournalApplyError> {
    let mut bytes = [0u8; 512];
    let n = encode_fixed(&mut bytes, 1, TSC, RecordFlags::EVENT_END, b) as usize;
    j.apply(&decode_record(&bytes[..n]).unwrap())
}

fn event(slot: u32, cum: i64, amount: i64) -> OrderEvent {
    let mut e = order_event(
        OrderEventKind::PartialFill,
        7,
        slot,
        1,
        A1,
        P2,
        BTC_P2,
        Qty(cum),
        px("60000"),
    );
    e.cash_qty = e.cum_filled;
    e.cash_sum = Qty(amount);
    e
}

fn one_order(rd: &RefData) -> JournalLedger {
    let mut j = cold(&booted_with(rd), rd);
    clock(&mut j, 0);
    feed(&mut j, &epoch_start(7), true);
    let mut d = decision_new(5, 1, A1, P2, BTC_P2, Buy, G1, btc("0.003"));
    d.px = px("60000");
    feed(&mut j, &d, true);
    j
}

#[test]
fn led_279_journal_cash_is_exact_across_report_delivery_orders() {
    // Five independently recorded executions in matching order. Fees are excluded from cash.
    let facts = [
        (Sell, 100_000, 600_189_000, 30_010, 0),
        (Sell, 100_000, 600_244_000, 30_013, 0),
        (Buy, 200_000, 600_275_000, 60_028, -11_700),
        (Sell, 100_000, 600_274_000, 30_014, 0),
        (Buy, 100_000, 600_275_000, 30_014, -100),
    ];
    let rd = rd_cash();
    let mut rows = 0;
    for encoded in 0..3125u32 {
        let mut n = encoded;
        let mut order = [0usize; 5];
        for x in &mut order {
            *x = (n % 5) as usize;
            n /= 5;
        }
        let mut sorted = order;
        sorted.sort();
        if sorted != [0, 1, 2, 3, 4] {
            continue;
        }
        rows += 1;
        let mut j = cold(&booted_with(rd), rd);
        clock(&mut j, 0);
        feed(&mut j, &epoch_start(7), true);
        for (i, &(side, qty, _, _, _)) in facts.iter().enumerate() {
            feed(
                &mut j,
                &decision_new(i as u32 + 1, 1, A1, P2, BTC_P2, side, G1, Qty(qty)),
                true,
            );
        }
        let mut expected = 50_000_000_000i64;
        let mut net = 0;
        for ix in order {
            let (side, qty, price, fee, cash_delta) = facts[ix];
            let mut e = event(ix as u32 + 1, qty, cash_delta);
            e.what = OrderEventKind::Fill;
            e.px = hft_types::Px(price);
            e.avg_px = e.px;
            e.fee = Qty(fee);
            e.fee_asset = USDT;
            apply(&mut j, &e).unwrap();
            expected += cash_delta - fee;
            net += side.sign() * qty;
            assert_eq!(cash(j.ledger(), A1, P2, USDT).total.0, expected, "{order:?}");
            assert_eq!(j.ledger().margin(A1, P2).wallet.0, expected);
            assert_eq!(j.ledger().pos(A1, BTC_P2).qty.0, net);
            let before = snap(j.ledger());
            apply(&mut j, &e).unwrap();
            assert_eq!(snap(j.ledger()), before);
        }
        assert_eq!(expected, 50_000_000_000 - 11_800 - 180_079);
    }
    assert_eq!(rows, 120);
}

#[test]
fn led_280_journal_query_cash_and_late_fee_have_separate_watermarks() {
    let rd = rd_cash();
    let mut j = one_order(rd);
    let mut e = event(5, 100_000, -11700);
    e.what = OrderEventKind::QueryResult;
    e.detail = 2;
    // Missing cumulative price uses the logged order price, as OMS QueryResult does.
    e.avg_px = hft_types::Px(0);
    e.px = hft_types::Px(0);
    apply(&mut j, &e).unwrap();
    let mut fill = e;
    fill.what = OrderEventKind::PartialFill;
    fill.fee = Qty(123);
    fill.fee_asset = USDT;
    apply(&mut j, &fill).unwrap();
    apply(&mut j, &fill).unwrap();
    e = event(5, 200_000, -11800);
    apply(&mut j, &e).unwrap();
    apply(&mut j, &fill).unwrap(); // older cash fact cannot rewind the watermark
    e.what = OrderEventKind::FeeOnly;
    e.fee = Qty(7);
    e.fee_asset = USDT;
    apply(&mut j, &e).unwrap();
    assert_eq!(
        cash(j.ledger(), A1, P2, USDT).total.0,
        50_000_000_000 - 11800 - 123 - 7
    );
    assert_eq!(j.ledger().pos(A1, BTC_P2).qty.0, 200_000);
}

fn recovered_bytes(l: &Ledger, tomb: bool, legacy: bool) -> Vec<u8> {
    let mut w = SnapshotWriter::new(64 << 20);
    l.snapshot(&mut w);
    if tomb {
        w.begin_section(12, if legacy { 1 } else { 2 });
        w.put_u8(4);
        w.put_bytes(&[0; 3]);
        w.put_u32(if legacy { 64 } else { 80 });
        let mut row = vec![0u8; if legacy { 64 } else { 80 }];
        row[0] = P2.0;
        row[1] = A1.0;
        row[4..8].copy_from_slice(&5u32.to_le_bytes());
        row[8..12].copy_from_slice(&1u32.to_le_bytes());
        row[12..14].copy_from_slice(&7u16.to_le_bytes());
        row[16..20].copy_from_slice(&BTC_P2.0.to_le_bytes());
        row[20..24].copy_from_slice(&G1.0.to_le_bytes());
        row[24] = Buy as u8;
        row[32..40].copy_from_slice(&100_000i64.to_le_bytes());
        if !legacy {
            row[64..72].copy_from_slice(&100_000i64.to_le_bytes());
            row[72..80].copy_from_slice(&(-11700i64).to_le_bytes());
        }
        w.put_bytes(&row);
    } else {
        w.begin_section(2, if legacy { 2 } else { 3 });
        let mut row = [0u8; 192];
        row[0] = 2; // Live
        row[1] = Buy as u8;
        row[6] = P2.0;
        row[7] = A1.0;
        row[8..12].copy_from_slice(&1u32.to_le_bytes());
        row[12..16].copy_from_slice(&5u32.to_le_bytes());
        row[16..20].copy_from_slice(&BTC_P2.0.to_le_bytes());
        row[22..24].copy_from_slice(&7u16.to_le_bytes());
        row[24..28].copy_from_slice(&G1.0.to_le_bytes());
        row[32..40].copy_from_slice(&300_000i64.to_le_bytes());
        row[40..48].copy_from_slice(&100_000i64.to_le_bytes());
        row[48..56].copy_from_slice(&px("60000").0.to_le_bytes());
        let notional = i128::from(px("60000").0) * 100_000;
        row[64..80].copy_from_slice(&notional.to_le_bytes());
        row[160..168].copy_from_slice(&100_000i64.to_le_bytes());
        if !legacy {
            row[168..176].copy_from_slice(&100_000i64.to_le_bytes());
            row[176..184].copy_from_slice(&(-11700i64).to_le_bytes());
        }
        w.put_bytes(&row);
    }
    w.add_rows(1);
    w.end_section();
    assert!(w.status().is_ok());
    w.as_bytes().to_vec()
}

#[test]
fn led_281_journal_restores_cash_for_live_and_recycled_orders() {
    let rd = rd_cash();
    let mut j = one_order(rd);
    apply(&mut j, &event(5, 100_000, -11700)).unwrap();
    for tomb in [false, true] {
        for legacy in [false, true] {
            let bytes = recovered_bytes(j.ledger(), tomb, legacy);
            let restored =
                JournalLedger::from_snapshot(&mut SnapshotReader::new(&bytes), rd, &cfg_lf(), &caps_lf());
            if legacy {
                assert!(matches!(restored, Err(RestoreError::BadValue)));
                // Old snapshots remain readable when the venue does not require cash facts.
                assert!(
                    JournalLedger::from_snapshot(
                        &mut SnapshotReader::new(&bytes),
                        rd_lf(),
                        &cfg_lf(),
                        &caps_lf()
                    )
                    .is_ok()
                );
                continue;
            }
            let mut resumed = restored.unwrap();
            apply(&mut resumed, &event(5, 100_000, -11700)).unwrap();
            assert_eq!(
                cash(resumed.ledger(), A1, P2, USDT).total.0,
                50_000_000_000 - 11700
            );
            feed(&mut resumed, &epoch_start(8), true);
            feed(
                &mut resumed,
                &decision_new(5, 2, A1, P2, BTC_P2, Sell, G1, Qty(100_000)),
                true,
            );
            let late = event(5, 200_000, -11800);
            apply(&mut resumed, &late).unwrap();
            apply(&mut resumed, &late).unwrap();
            assert_eq!(
                cash(resumed.ledger(), A1, P2, USDT).total.0,
                50_000_000_000 - 11800
            );
            assert_eq!(resumed.ledger().pos(A1, BTC_P2).qty.0, 200_000);
        }
    }
}

#[test]
fn led_282_journal_rejects_unprovable_cash_and_stops_consumption() {
    let rd = rd_cash();
    for row in 0..9 {
        let mut j = one_order(rd);
        let first = event(5, 100_000, -11700);
        apply(&mut j, &first).unwrap();
        let mut e = event(5, 200_000, -11800);
        let want = match row {
            0 => {
                e.cash_qty = Qty(0);
                e.cash_sum = Qty(0);
                JournalApplyError::MissingCash
            }
            1 => {
                e.cash_qty = Qty(100_000);
                JournalApplyError::MissingCash
            }
            2 => {
                e.cash_qty = Qty(300_000);
                JournalApplyError::CashCoverage
            }
            3 => {
                e = first;
                e.cash_sum.0 -= 1;
                JournalApplyError::CashConflict
            }
            4 => {
                e.cash_sum = Qty(i64::MAX);
                JournalApplyError::CashOverflow
            }
            5 => {
                e.epoch += 1;
                JournalApplyError::Unmapped
            }
            6 => {
                e.inst = ETH_P2;
                JournalApplyError::Identity
            }
            7 => {
                e.inst = InstId(u32::MAX);
                JournalApplyError::Identity
            }
            _ => {
                e.avg_px = hft_types::Px(-1);
                JournalApplyError::Settlement(SettlementError::InvalidPrice)
            }
        };
        let before = snap(j.ledger());
        assert_eq!(apply(&mut j, &e), Err(want), "row {row}");
        assert_eq!(snap(j.ledger()), before);
        assert_eq!(apply(&mut j, &event(5, 200_000, -11800)), Err(want));
        assert_eq!(snap(j.ledger()), before);
    }
    let mut j = one_order(rd);
    let mut x = ext(hft_event::ExternalCause::Adl, Sell, BTC_P2, "0.001", "60000");
    x.cash_qty = Qty(0);
    assert_eq!(
        apply(&mut j, &x),
        Err(JournalApplyError::Settlement(SettlementError::MissingCash))
    );
}

#[test]
fn led_286_journal_external_cash_identity_and_missing_facts_stop_consumption() {
    let rd = rd_cash();
    let mut good = ext(hft_event::ExternalCause::Adl, Sell, BTC_P2, "0.001", "60000");
    good.trade_id = 90;
    good.cash_qty = good.qty;
    for row in 0..5 {
        let mut j = cold(&booted_with(rd), rd);
        let mut e = good;
        let want = match row {
            0 => {
                e.inst = InstId(u32::MAX);
                SettlementError::UnknownInstrument
            }
            1 => {
                e.venue = S1;
                SettlementError::VenueMismatch
            }
            2 => {
                e.account = hft_types::AccountId(0);
                SettlementError::AccountVenue
            }
            3 => {
                e.cash_qty = Qty(0);
                SettlementError::MissingCash
            }
            _ => {
                e.cash_qty.0 += 1;
                SettlementError::CashCoverage
            }
        };
        let before = snap(j.ledger());
        let error = Err(JournalApplyError::Settlement(want));
        assert_eq!(apply(&mut j, &e), error, "row {row}");
        assert_eq!(snap(j.ledger()), before);
        assert_eq!(apply(&mut j, &good), error);
        assert_eq!(snap(j.ledger()), before);
    }
}

#[test]
fn led_287_cold_clock_accepts_only_completely_zero_unstarted_anchor() {
    let rd = rd_cash();
    for row in 0..4 {
        let l = booted_with(rd);
        let mut w = SnapshotWriter::new(1 << 20);
        l.snapshot(&mut w);
        w.begin_section(25, 1);
        let mut anchor = [0u8; 32];
        match row {
            1 => anchor[0..8].copy_from_slice(&1i64.to_le_bytes()),
            2 => anchor[8..16].copy_from_slice(&1u64.to_le_bytes()),
            3 => anchor[24..32].copy_from_slice(&1u64.to_le_bytes()),
            _ => {}
        }
        w.put_bytes(&anchor);
        w.add_rows(1);
        w.end_section();
        let r =
            JournalLedger::from_snapshot(&mut SnapshotReader::new(w.as_bytes()), rd, &cfg_lf(), &caps_lf());
        if row == 0 {
            let mut j = r.unwrap();
            // EngineClock arrives after the boot frame and supplies the first valid anchor.
            clock(&mut j, s(10));
            let e = bal(A1, P2, USDT, usdt("50000"), Qty(0), s(10));
            feed(&mut j, &e, true);
            let mut expected = l;
            expected.on_balance(&e, false, s(10));
            assert_eq!(snap(j.ledger()), snap(&expected));
        } else {
            assert!(matches!(r, Err(RestoreError::BadValue)), "row {row}");
        }
    }
}

#[test]
fn led_288_recovery_rebuilds_read_views_without_replacing_booked_facts() {
    let rd = rd_cash();
    let mut before = one_order(rd);
    clock(&mut before, s(10));
    let e = event(5, 100_000, 0);
    apply(&mut before, &e).unwrap();
    let report = bal(A1, P2, USDT, usdt("49000"), Qty(0), s(20));
    before.ledger_mut().on_balance(&report, true, s(20));
    let original = before.ledger().venue_book(A1, P2);
    assert_ne!(original.vflags & hft_ledger::vflags::DRIFT, 0);
    assert_eq!(original.summary.as_of_ens, s(20));
    let durable = snap(before.ledger());

    for purpose in [1, 2, 3, 5, 4] {
        let mut cold = before.clone();
        let mut expected = if purpose == 4 {
            restore_with(&durable, rd).unwrap()
        } else {
            before.ledger().clone()
        };
        // Supply a different ledger snapshot: the consumer must not overwrite its own facts
        // with the checkpoint being audited. Only the recovery purpose controls view lifetime.
        let hdr = hft_event::SnapshotHdr {
            purpose,
            ..hft_event::SnapshotHdr::new_zeroed()
        };
        let mut body = hdr.as_bytes().to_vec();
        body.extend(snap(&booted_with(rd)));
        let record = hft_event::RecordView {
            hdr: hft_event::RecordHeader {
                len: (24 + body.len()) as u32,
                kind: hft_event::EventKind::STATE_SNAPSHOT.0,
                flags: RecordFlags::EVENT_END.0,
                seq: 17,
                tsc: TSC,
            },
            body: &body,
        };
        let mut checkpoint_body = body.clone();
        checkpoint_body[0] = 2;
        let checkpoint = hft_event::RecordView {
            body: &checkpoint_body,
            ..record
        };
        cold.apply(&checkpoint).unwrap();
        for _ in 0..2 {
            cold.apply(&record).unwrap();
            assert_eq!(
                snap(cold.ledger()),
                durable,
                "purpose {purpose}: durable facts changed"
            );
            for &(account, venue, _) in rd.account_venues() {
                let a = cold.ledger().venue_book(account, venue);
                let b = expected.venue_book(account, venue);
                assert_eq!(a.cash, b.cash);
                assert_eq!(a.inv, b.inv);
                assert_eq!(a.margin, b.margin);
                assert_eq!(a.summary, b.summary, "purpose {purpose}");
                assert_eq!(a.vflags, b.vflags);
            }
        }
        // The following clock supplies the new run's time; the existing order watermark still
        // rejects a duplicate fill, and the next cumulative increment is booked exactly once.
        clock(&mut cold, s(30));
        expected.on_run_start(s(30));
        assert_eq!(
            cold.ledger().venue_book(A1, P2).summary,
            expected.venue_book(A1, P2).summary
        );
        apply(&mut cold, &e).unwrap();
        assert_eq!(snap(cold.ledger()), durable);
        apply(&mut cold, &event(5, 200_000, 0)).unwrap();
        assert_eq!(cold.ledger().pos(A1, BTC_P2).qty, Qty(200_000));
        assert_eq!(
            cash(cold.ledger(), A1, P2, USDT).total,
            cash(before.ledger(), A1, P2, USDT).total
        );
    }

    // A real recovery replays the checkpoint's tail. A later clock is an easy nonzero
    // counterexample to resetting all caches at the Recovery record itself.
    let mut cold = before.clone();
    let mut hdr = hft_event::SnapshotHdr::new_zeroed();
    hdr.purpose = 2;
    let record = hft_event::RecordView {
        hdr: hft_event::RecordHeader {
            len: (24 + size_of::<hft_event::SnapshotHdr>()) as u32,
            kind: hft_event::EventKind::STATE_SNAPSHOT.0,
            flags: RecordFlags::EVENT_END.0,
            seq: 18,
            tsc: TSC,
        },
        body: hdr.as_bytes(),
    };
    cold.apply(&record).unwrap();
    clock(&mut cold, s(30));
    let mut recovered = restore_with(&durable, rd).unwrap();
    recovered.on_run_start(s(30));
    let mut recovery_header = hdr;
    recovery_header.purpose = 4;
    cold.apply(&hft_event::RecordView {
        body: recovery_header.as_bytes(),
        ..record
    })
    .unwrap();
    assert_eq!(snap(cold.ledger()), durable);
    assert_eq!(
        cold.ledger().venue_book(A1, P2).summary,
        recovered.venue_book(A1, P2).summary
    );
}

#[test]
fn led_289_balance_gate_recomputes_drift_from_same_event_facts() {
    use hft_ledger::{BalanceVerdict, FeeDelta, vflags};
    let rd = rd_cash();
    let base = booted_with(rd);
    let mut hot = base.clone();
    let mut j = cold(&base, rd);
    clock(&mut j, s(20));
    hot.on_run_start(s(20));
    let total = cash(&base, A1, P2, USDT).total;
    for (free, quiet, drift) in [
        (Qty(total.0 - usdt("1000").0), true, true),
        (total, false, true),
        (total, true, false),
    ] {
        let b = bal(A1, P2, USDT, free, Qty(0), s(20));
        hot.clear_notes();
        hot.on_balance(&b, quiet, s(20));
        let evidence = hot
            .notes()
            .iter()
            .find(|n| n.code == hft_ledger::notes::code::BALANCE_CHECK)
            .copied();
        assert_eq!(evidence.is_some(), quiet);
        feed(&mut j, &b, !quiet);
        if let Some(n) = evidence {
            apply(&mut j, &n).unwrap();
        }
        let a = j.ledger().venue_book(A1, P2);
        let e = hot.venue_book(A1, P2);
        assert_eq!(a.summary, e.summary);
        assert_eq!(a.vflags, e.vflags);
        assert_eq!(a.vflags & vflags::DRIFT != 0, drift);
        assert_eq!(a.cash, e.cash);
        assert_eq!(cash(j.ledger(), A1, P2, USDT).total, total);
    }

    let b = bal(A1, P2, USDT, total, Qty(0), s(20));
    hot.clear_notes();
    assert_eq!(hot.on_balance(&b, true, s(20)), BalanceVerdict::Ok);
    let note = *hot
        .notes()
        .iter()
        .find(|n| n.code == hft_ledger::notes::code::BALANCE_CHECK)
        .unwrap();
    for field in 0..12 {
        let mut bad = cold(&base, rd);
        clock(&mut bad, s(20));
        feed(&mut bad, &b, field == 11);
        let mut n = note;
        match field {
            0..=7 => n.v[field] ^= 1,
            8 => n.group.0 = 1,
            9 => n.inst.0 = 1,
            10 => n.strategy.0 = 1,
            _ => {}
        }
        let prior = snap(bad.ledger());
        assert_eq!(
            apply(&mut bad, &n),
            Err(JournalApplyError::BalanceEvidence),
            "field {field}"
        );
        assert_eq!(snap(bad.ledger()), prior);
        assert_eq!(apply(&mut bad, &note), Err(JournalApplyError::BalanceEvidence));
    }
    let mut duplicate = cold(&base, rd);
    clock(&mut duplicate, s(20));
    feed(&mut duplicate, &b, false);
    apply(&mut duplicate, &note).unwrap();
    assert_eq!(
        apply(&mut duplicate, &note),
        Err(JournalApplyError::BalanceEvidence)
    );

    // The note contains the gate and the raw report, never the hot result. A distinct local
    // wallet must produce its own Drift verdict even when the producer's verdict was Ok.
    let mut different = base.clone();
    different.apply_fee(
        &FeeDelta {
            account: A1,
            venue: P2,
            inst: BTC_P2,
            group: G1,
            side: Buy,
            fee: usdt("10").0,
            fee_asset: USDT,
            seq: 1,
        },
        s(10),
    );
    let mut independent = cold(&different, rd);
    clock(&mut independent, s(20));
    feed(&mut independent, &b, false);
    apply(&mut independent, &note).unwrap();
    assert_ne!(independent.ledger().venue_book(A1, P2).vflags & vflags::DRIFT, 0);
    assert_eq!(
        cash(independent.ledger(), A1, P2, USDT).total,
        cash(&different, A1, P2, USDT).total
    );
}
