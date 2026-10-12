//! External order reports use original exchange settlement facts and exact event boundaries.
use hft_event::{
    Bbo, ExternalCause, ExternalFill, Note, NoteSource, OrderEvent, OrderEventKind, RecordFlags,
};
use hft_types::{Side, SlotId};
use zerocopy::IntoBytes;

use super::ledger::{LOT, SECOND, assert_final, assert_trade_window, command, configure_base, require};
use crate::dst::sut::ledger_input;
use crate::dst::venue_behavior::EXTERNAL_ORDER_ID_BASE;
use crate::dst::world::{ACCT_A, P1, VP};
use crate::dst::{DstConfig, Fault, InvariantSet, MarketSource, RunRecord, Scenario, SeedSet};

fn count(seed: u64) -> usize {
    1 + (crate::seed::derive(seed >> 2, "led-080/count") % 3) as usize
}

fn lots(seed: u64) -> i64 {
    2 * (1 + (crate::seed::derive(seed >> 2, "led-080/size") % 3) as i64)
}

fn plan(seed: u64) -> Vec<(u64, Fault)> {
    let n = count(seed);
    (0..=n)
        .map(|ix| {
            let unresolved = ix == n;
            let at = if unresolved { 25 } else { 12 + 3 * ix as u64 } * SECOND
                + crate::seed::derive(seed >> 2, &format!("led-080/when/{ix}")) % SECOND;
            (
                at,
                Fault::ForeignOrderFill {
                    account: ACCT_A,
                    inst: P1,
                    qty: if unresolved { n as i64 * LOT } else { LOT },
                    buy: (seed & 4 == 0) != unresolved,
                    positive_trade_id: (seed & 1 != 0) != (ix % 2 != 0),
                    unresolved,
                    raw_first: (seed & 2 != 0) != (ix % 2 != 0),
                },
            )
        })
        .collect()
}

fn configure(c: &mut DstConfig) {
    configure_base(c);
    for (id, seconds, quantity) in [(1, 1, Some(lots(c.seed))), (2, 4, None)] {
        let at = seconds * SECOND;
        c.confirmed_commands.push((at, command(c, at, id, quantity)));
    }
    // All external opens precede the price change; the final opposite execution realizes
    // nonzero cash. Prices are case inputs, never reconstructed from either wallet.
    let tick = c
        .world
        .insts
        .iter()
        .find(|i| i.id == P1)
        .expect("W1 perpetual")
        .tick;
    let shift = (5 + (crate::seed::derive(c.seed >> 2, "led-080/price") % 5) as i64) * tick;
    if let MarketSource::Recorded(inputs) = &mut c.market {
        for (at, input) in inputs {
            if *at >= 22 * SECOND
                && let Some(mut b) = input.read::<Bbo>()
                && b.h.inst == P1
            {
                b.bid_px.0 += shift;
                b.ask_px.0 += shift;
                input.body = b.as_bytes().to_vec();
            }
        }
    }
    c.scripted = plan(c.seed);
}

fn notes(r: &RunRecord, seq: u64) -> Result<Vec<Note>, String> {
    let start = r
        .journal
        .iter()
        .position(|j| j.seq == seq)
        .ok_or("missing event Input")?;
    let end = r.journal[start..]
        .iter()
        .position(|j| j.flags.contains(RecordFlags::EVENT_END))
        .ok_or("missing completed event")?
        + start;
    Ok(r.journal[start..=end]
        .iter()
        .filter_map(|j| j.try_read::<Note>())
        .filter(|n| n.source == NoteSource::Oms)
        .collect())
}

/// Prove every requested execution occurred, its delivery identity took the intended OMS
/// path, and the exact original cash was booked once on both independently driven ledgers.
pub fn assert_record(r: &RunRecord) -> Result<(), String> {
    let expected = plan(r.seed);
    require(
        r.end_ns >= 90 * SECOND,
        "foreign scenario ended before the delivery tail",
    )?;
    require(r.snapshot_checks > 0, "no independent snapshot checks")?;
    assert_trade_window(r, SECOND, 4 * SECOND, Side::Buy, lots(r.seed))?;
    assert_trade_window(r, 4 * SECOND, 7 * SECOND, Side::Sell, lots(r.seed))?;
    require(
        r.external_receipts.len() == expected.len(),
        "missing or extra original executions",
    )?;
    let orders: Vec<_> = r
        .journal
        .iter()
        .filter_map(|j| j.try_read::<OrderEvent>().map(|e| (j.seq, e)))
        .filter(|(_, e)| e.exch_order_id >= EXTERNAL_ORDER_ID_BASE)
        .collect();
    require(
        orders.len() == expected.len(),
        "missing or repeated defensive order report",
    )?;
    let raw: Vec<_> = r
        .journal
        .iter()
        .filter_map(|j| j.try_read::<ExternalFill>().map(|e| (j.seq, e)))
        .collect();
    let raw_count = expected
        .iter()
        .filter(|(_, f)| matches!(f, Fault::ForeignOrderFill { raw_first: true, .. }))
        .count();
    require(raw.len() == raw_count, "wrong raw-first execution count")?;
    for ((at, fault), receipt) in expected.iter().zip(&r.external_receipts) {
        let Fault::ForeignOrderFill {
            qty,
            buy,
            positive_trade_id,
            unresolved,
            raw_first,
            ..
        } = *fault
        else {
            return Err("invalid foreign scenario plan".into());
        };
        let f = receipt.fill;
        require(
            receipt.client_id == "foreign-order"
                && f.account == ACCT_A
                && f.venue == VP
                && f.inst == P1
                && f.cause == ExternalCause::Unknown
                && f.qty.0 == qty
                && f.side == if buy { Side::Buy } else { Side::Sell }
                && f.ex_ts_ns == *at as i64
                && f.exch_order_id >= EXTERNAL_ORDER_ID_BASE
                && (f.trade_id > 0) == positive_trade_id
                && f.fill_seq == if positive_trade_id { 0 } else { qty as u64 }
                && f.cash_qty == f.qty,
            "original receipt does not match configured execution",
        )?;
        let copies: Vec<_> = orders
            .iter()
            .filter(|(_, e)| e.exch_order_id == f.exch_order_id)
            .collect();
        require(copies.len() == 1, "execution has no unique cumulative report")?;
        let (seq, e) = copies[0];
        require(
            e.what == OrderEventKind::Fill
                && e.account == f.account
                && e.venue == f.venue
                && e.inst == f.inst
                && e.detail == f.side as u8
                && e.qty == f.qty
                && e.cum_filled == f.qty
                && e.px == f.px
                && e.avg_px == f.px
                && e.cash_qty == f.cash_qty
                && e.cash_sum == f.cash_amount
                && e.fee == f.fee
                && e.fee_asset == f.fee_asset
                && e.trade_id == f.trade_id
                && e.ex_ts_ns == f.ex_ts_ns
                && (e.slot == SlotId::UNRESOLVED) == unresolved,
            "cumulative report differs from original execution facts",
        )?;
        let prior: Vec<_> = raw
            .iter()
            .filter(|(_, x)| x.exch_order_id == f.exch_order_id)
            .collect();
        require(
            prior.len() == usize::from(raw_first),
            "wrong original delivery family",
        )?;
        if let Some((raw_seq, original)) = prior.first() {
            let mut delivered = *original;
            // The gw adds receive time; every exchange-originated field remains unchanged.
            delivered.rx_tsc = f.rx_tsc;
            require(
                *raw_seq < *seq && delivered.as_bytes() == f.as_bytes(),
                "raw delivery is not the earlier original",
            )?;
        }
        let event_notes = notes(r, *seq)?;
        let path = if unresolved {
            hft_oms::notes::FOREIGN_FILL_VIA_ORDER_EVENT.n
        } else {
            hft_oms::notes::STALE_GEN_FILL.n
        };
        let paths: Vec<_> = event_notes.iter().filter(|n| n.code == path).collect();
        require(paths.len() == 1, "missing intended defensive OMS path")?;
        if unresolved {
            require(
                paths[0].v[1] == f.exch_order_id && paths[0].v[2] == if raw_first { 0 } else { qty as u64 },
                "unresolved path booked the wrong quantity",
            )?;
        } else {
            require(
                paths[0].v[1] == u64::from(e.slot.0)
                    && paths[0].v[2] == u64::from(!raw_first)
                    && paths[0].v[3] == u64::from(e.r#gen)
                    && paths[0].v[4] == u64::from(e.epoch)
                    && paths[0].v[5] == f.exch_order_id,
                "stale path booked wrong identity or duplicate",
            )?;
        }
        require(
            event_notes
                .iter()
                .filter(|n| n.code == hft_oms::notes::FOREIGN_FILL.n)
                .count()
                == usize::from(!raw_first),
            "foreign booking is missing or duplicated",
        )?;
    }
    require(
        r.external_receipts
            .last()
            .is_some_and(|r| r.fill.cash_amount.0 != 0),
        "closing execution did not realize cash",
    )?;
    let expected_checks: Vec<_> = r
        .journal
        .iter()
        .filter(|j| ledger_input(j.kind))
        .map(|j| j.seq)
        .collect();
    require(
        r.ledger_checks == expected_checks,
        "missing, repeated or reordered ledger event checks",
    )?;
    assert_final(r)
}

pub fn led_080_foreign_order_cold_hot_equal() -> Scenario {
    Scenario::new(
        "LED-080",
        "defensive order fills preserve exact original settlement",
    )
    .suite("core")
    .seeds(SeedSet::Standard)
    .steps(crate::seed::DST_DEFAULT_STEPS)
    .invariants(InvariantSet::all())
    .configure(configure)
    .assert(assert_record)
}
