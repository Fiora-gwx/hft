//! Cumulative-average spot fills whose rounded increment price loses cash precision.
use super::*;
use hft_event::{BalanceReport, OrderEvent, OrderEventKind};

pub(super) fn exercise<S: StrategySet>(
    s: &mut Sequencer<LiveDriver<S, Io<S>>, S>,
    world: &WorldSpec,
    side: Side,
) {
    prepare_smoke(s, world);
    let i = &world.insts[0];
    assert!(!i.perp);
    let a = world.accounts[0].id;
    let vi = world.venues.iter().position(|v| v.id == i.venue).unwrap() as u8;
    const INITIAL: i64 = 100_000_000_000;
    for (asset, amount) in [(i.quote, INITIAL), (i.base, 100_000_000)] {
        let mut balance = BalanceReport::new_zeroed();
        balance.venue = i.venue;
        balance.account = a;
        balance.asset = asset;
        balance.free = Qty(amount);
        input(s, (2, vi), EventKind::BALANCE_REPORT, balance.as_bytes().to_vec());
    }
    settle(s);
    command(
        s,
        800,
        &ControlCmd::SmokeOrder {
            account: a,
            venue: i.venue,
            _pad: 0,
            inst: i.id,
            side,
            _pad2: [0; 3],
            ttl_ms: 30_000,
        },
    );
    settle(s);
    let slot_id = s.engine().oms.table_view(i.venue, a).unwrap().smoke_slot;
    assert_ne!(
        slot_id,
        0,
        "smoke rejected: {:?}",
        s.driver().io.writer.last_result
    );
    let slot = *s.engine().oms.raw_slot(i.venue, a, slot_id).unwrap().0;
    assert_eq!(slot.qty, Qty(1_000_000));
    command(s, 801, &ControlCmd::SnapshotNow);
    settle(s);
    let first_px = slot.px.0 - side.sign() * 20_000;
    let final_avg = slot.px.0 - side.sign() * 10_000;
    for (ix, (what, qty, cum, avg)) in [
        (OrderEventKind::PartialFill, 400_000, 400_000, first_px),
        (OrderEventKind::Fill, 600_000, 1_000_000, final_avg),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(qty % i.lot, 0);
        let e = OrderEvent {
            venue: i.venue,
            account: a,
            what,
            detail: 0,
            slot: hft_types::SlotId(slot_id),
            r#gen: slot.r#gen,
            inst: i.id,
            px: Px(avg),
            qty: Qty(qty),
            cum_filled: Qty(cum),
            fee: Qty(3),
            fee_asset: i.quote,
            epoch: (s.engine().oms.engine_epoch() & 0x7fff) as u16,
            reject_code: 0,
            exch_order_id: 80_000,
            ex_ts_ns: 0,
            hw_rx_ns: 0,
            rx_tsc: 0,
            avg_px: Px(avg),
            cash_qty: Qty(0),
            cash_sum: Qty(0),
            trade_id: ix as u64 + 1,
        };
        input(s, (2, vi), EventKind::ORDER_EVENT, e.as_bytes().to_vec());
        settle(s);
        // W1: px_exp=-4, base qty_exp=-8, quote qty_exp=-6. Both cumulative
        // amounts are integral quote units, so this oracle needs no rounded price.
        let notional = i128::from(avg) * i128::from(cum);
        assert_eq!(notional % 1_000_000, 0);
        let expected = INITIAL - side.sign() * (notional / 1_000_000) as i64 - 3 * (ix as i64 + 1);
        let wallet = s
            .engine()
            .ledger
            .venue_book(a, i.venue)
            .cash
            .iter()
            .find(|c| c.asset == i.quote)
            .unwrap();
        assert_eq!(wallet.total, Qty(expected), "side={side:?}, increment={ix}");
        assert_eq!(
            s.engine().oms.raw_slot(i.venue, a, slot_id).unwrap().0.filled,
            Qty(cum)
        );
        command(s, 802 + ix as u64, &ControlCmd::SnapshotNow);
        settle(s);
    }
}
