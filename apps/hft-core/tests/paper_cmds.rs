//! CORE-427: `out-sim` commands that project no order decision leave the ring without waiting for
//! an order (`paper::pop_unsequenced`). CORE-428: journalled L2 records reach the simulated
//! exchange's queue model (`paper::l2`).
mod support;
use hft_core::paper::pop_unsequenced;
use hft_event::{GwCmd, GwOp};
use hft_sim::{MarketEventRef, SimInput};
use hft_types::{AccountId, InstId, Px, VenueId};

fn settle(end_s: u32) -> GwCmd {
    GwCmd::sim_settle(AccountId(1), VenueId::POLYMARKET_CLOB, InstId(3), Px(1000), end_s)
}

fn order(decision_seq: u64) -> GwCmd {
    GwCmd {
        decision_seq,
        op: GwOp::Place,
        ..settle(0)
    }
}

#[test]
fn core_427_unsequenced_commands_do_not_wait_for_an_order() {
    let (mut tx, mut rx) = hft_core::runtime::ring::<GwCmd>(16).unwrap();
    // .1 empty ring.
    assert!(pop_unsequenced(&mut rx).is_none(), "CORE-427.1");
    // .2 two settles queue up: both leave in order.
    tx.try_push(settle(100)).unwrap();
    tx.try_push(settle(200)).unwrap();
    let a = pop_unsequenced(&mut rx).map(|c| c.arg2);
    let b = pop_unsequenced(&mut rx).map(|c| c.arg2);
    assert_eq!((a, b), (Some(100), Some(200)), "CORE-427.2");
    // .3 an order command at the head stays (its decision barrier), and so does what follows it.
    tx.try_push(order(7)).unwrap();
    tx.try_push(settle(300)).unwrap();
    assert!(pop_unsequenced(&mut rx).is_none(), "CORE-427.3");
    assert_eq!(rx.try_pop().map(|c| c.decision_seq), Some(7), "CORE-427.3");
    assert_eq!(pop_unsequenced(&mut rx).map(|c| c.arg2), Some(300), "CORE-427.3");
}

#[test]
fn core_428_paper_forwards_l2_to_the_queue_model() {
    use hft_event::{EventKind, L2SnapHdr, Level, MarketHdr, OrderEventKind, RecordHeader, RepMsg, Trade};
    use hft_types::{ConnId, Qty, Side, SlotId, Tif, Tsc};
    use zerocopy::IntoBytes;
    let d = tempfile::tempdir().unwrap();
    let path = support::write(d.path(), &d.path().join("hot"), None);
    let mut l = hft_core::config::load(&path, true).unwrap();
    let paper = l.cfg.sim.as_mut().unwrap();
    paper.maker_fill = hft_core::config::MakerFillKind::Queue;
    let rd = hft_refdata::RefData::load(l.rd.blob(), None).unwrap();
    // 1 GHz: 1 TSC = 1 ns.
    let mut ex = hft_core::paper::exchange(paper, &rd, 1_000_000_000_000).unwrap();
    let (a, i, v) = (AccountId(1), InstId(1), VenueId::BINANCE_SPOT);
    let ms = |x: u64| x * 1_000_000;
    let rec = |kind: EventKind, t: u64| RecordHeader {
        len: 0,
        kind: kind.0,
        flags: 0,
        seq: t,
        tsc: t,
    };
    let mh = |t: u64| MarketHdr {
        venue: v,
        conn: ConnId(0),
        _pad: 0,
        inst: i,
        ex_ts_ns: t as i64,
        ex_send_ns: 0,
        hw_rx_ns: 0,
        rx_tsc: 0,
        sw_rx_ns: 0,
        rx_empty_gap_ns: 0,
        _rsv: 0,
    };
    // Journal body of a TopN snapshot: bid 59 000 showing 0.4 BTC, ask 60 000 (px −4, qty −8).
    let hdr = L2SnapHdr {
        h: mh(ms(100)),
        snap_id: 1,
        origin: L2SnapHdr::TOP_N,
        _pad: [0; 3],
        n_bids: 1,
        n_asks: 1,
    };
    let mut body = hdr.as_bytes().to_vec();
    for l in [
        Level {
            px: Px(590_000_000),
            qty: Qty(40_000_000),
        },
        Level {
            px: Px(600_000_000),
            qty: Qty(100_000_000),
        },
    ] {
        body.extend_from_slice(l.as_bytes());
    }
    let mut levels = Vec::new();
    hft_core::paper::l2(
        &mut ex,
        &mut levels,
        &rec(EventKind::L2_SNAPSHOT, ms(100)),
        EventKind::L2_SNAPSHOT,
        &body,
    );
    let place = GwCmd {
        decision_seq: 1,
        decided_tsc: Tsc(ms(100)),
        px: Px(590_000_000),
        qty: Qty(1_000_000),
        inst: i,
        slot: SlotId(1),
        r#gen: 1,
        target_slot: SlotId(1),
        op: GwOp::Place,
        account: a,
        side: Side::Buy,
        tif: Tif::Gtc,
        oflags: 0,
        _pad: [0; 2],
        arg: 0,
        arg2: 0,
    };
    ex.on_command(&place, Tsc(ms(100)));
    let mut filled = Vec::new();
    let step = |ex: &mut hft_sim::SimExchange, t: u64, filled: &mut Vec<i64>| {
        ex.advance(Tsc(t));
        ex.drain_due(Tsc(t), |r| {
            if let RepMsg::OrderEvent(e, _) = r.msg
                && matches!(e.what, OrderEventKind::PartialFill | OrderEventKind::Fill)
            {
                filled.push(e.qty.0);
            }
        });
    };
    step(&mut ex, ms(110), &mut filled);
    let trade = |t: u64, qty: i64| Trade {
        h: mh(t),
        px: Px(590_000_000),
        qty: Qty(qty),
        trade_id: 1,
        taker_side: Side::Sell,
        _pad: [0; 7],
    };
    // .1 0.3 BTC sold at the price: still 0.1 ahead of the order.
    let t = trade(ms(200), 30_000_000);
    ex.on_record(
        &rec(EventKind::TRADE, ms(200)),
        SimInput::Market(MarketEventRef::Trade(&t)),
    );
    step(&mut ex, ms(210), &mut filled);
    assert!(filled.is_empty(), "CORE-428.1 {filled:?}");
    // .2 another 0.2: the order's 0.01 fills.
    let t = trade(ms(300), 20_000_000);
    ex.on_record(
        &rec(EventKind::TRADE, ms(300)),
        SimInput::Market(MarketEventRef::Trade(&t)),
    );
    step(&mut ex, ms(310), &mut filled);
    assert_eq!(filled, vec![1_000_000], "CORE-428.2");
}
