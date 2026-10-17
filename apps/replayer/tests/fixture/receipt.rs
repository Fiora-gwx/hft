//! Real files for a retained original receipt, FIFO wrap and cumulative duplicates.
use super::*;
use hft_event::{BalanceReport, ExternalCause, ExternalFill, OrderEvent, OrderEventKind};

pub(super) fn exercise<S: StrategySet>(
    s: &mut Sequencer<LiveDriver<S, Io<S>>, S>,
    world: &WorldSpec,
    positive: bool,
) {
    let i = world.insts.iter().find(|i| i.perp).unwrap();
    let a = world.accounts[0].id;
    assert!(
        s.engine()
            .rd
            .caps(i.venue)
            .flags
            .contains(hft_refdata::CapFlags::PERP_CASH)
    );
    let vi = world.venues.iter().position(|v| v.id == i.venue).unwrap() as u8;
    let mut balance = BalanceReport::new_zeroed();
    balance.venue = i.venue;
    balance.account = a;
    balance.asset = i.quote;
    balance.free = Qty(100_000_000_000);
    input(s, (2, vi), EventKind::BALANCE_REPORT, balance.as_bytes().to_vec());
    settle(s);
    command(s, 900, &ControlCmd::SnapshotNow);
    settle(s);
    let raw = ExternalFill {
        venue: i.venue,
        account: a,
        inst: i.id,
        side: Side::Sell,
        cause: ExternalCause::Unknown,
        qty: Qty(100_000),
        px: Px(i.bid),
        fee: Qty(17),
        fee_asset: i.quote,
        _pad: [0; 6],
        exch_order_id: 99_000,
        trade_id: if positive { 40 } else { 0 },
        ex_ts_ns: 0,
        rx_tsc: Tsc(0),
        fill_seq: if positive { 0 } else { 100_000 },
        cash_qty: Qty(100_000),
        cash_amount: Qty(-11_700),
    };
    input(s, (2, vi), EventKind::EXTERNAL_FILL, raw.as_bytes().to_vec());
    settle(s);
    command(s, 901, &ControlCmd::SnapshotNow);
    settle(s);
    for id in 50..53 {
        let mut other = raw;
        other.exch_order_id = 0;
        other.trade_id = id;
        other.fill_seq = 0;
        other.cause = ExternalCause::Adl;
        other.side = Side::Buy;
        other.qty = Qty(1);
        other.cash_qty = other.qty;
        other.cash_amount = Qty(1);
        other.fee = Qty(0);
        input(s, (2, vi), EventKind::EXTERNAL_FILL, other.as_bytes().to_vec());
        settle(s);
    }
    assert_eq!(s.engine().ledger.ext_seen(a, i.venue).len(), 2);
    command(s, 902, &ControlCmd::SnapshotNow);
    settle(s);
    for slot in [0, 700] {
        let e = OrderEvent {
            venue: i.venue,
            account: a,
            what: OrderEventKind::Fill,
            detail: Side::Sell as u8,
            slot: hft_types::SlotId(slot),
            r#gen: 17,
            inst: i.id,
            px: raw.px,
            qty: raw.qty,
            cum_filled: raw.qty,
            fee: raw.fee,
            fee_asset: raw.fee_asset,
            epoch: (s.engine().oms.engine_epoch() & 0x7fff) as u16,
            reject_code: 0,
            exch_order_id: raw.exch_order_id,
            ex_ts_ns: 0,
            hw_rx_ns: 0,
            rx_tsc: 0,
            avg_px: raw.px,
            cash_qty: raw.qty,
            cash_sum: raw.cash_amount,
            trade_id: raw.trade_id,
            fill_cash_qty: hft_types::Qty(0),
            fill_cash_amount: hft_types::Qty(0),
        };
        input(s, (2, vi), EventKind::ORDER_EVENT, e.as_bytes().to_vec());
        input(s, (2, vi), EventKind::EXTERNAL_FILL, raw.as_bytes().to_vec());
        settle(s);
        let wallet = s
            .engine()
            .ledger
            .venue_book(a, i.venue)
            .cash
            .iter()
            .find(|c| c.asset == i.quote)
            .unwrap();
        assert_eq!(wallet.total, Qty(100_000_000_000 - 11_714));
        assert_eq!(s.engine().ledger.pos(a, i.id).qty, Qty(-99_997));
    }
    command(s, 903, &ControlCmd::SnapshotNow);
    settle(s);
}
