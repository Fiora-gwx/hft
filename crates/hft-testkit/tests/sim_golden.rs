#[path = "../../hft-sim/tests/common/mod.rs"]
mod common;
use common::*;
use hft_event::{Bbo, GwCmd, GwOp, OrderEvent, OrderEventKind};
use hft_sim::{MarketEventRef, SimExchange, SimInput};
use hft_testkit::{SimStep, run_sim_case};
use hft_types::{AssetId, Qty, SlotId, Tif, Tsc};

#[test]
fn tk_009_sim_case_reports_first_field_difference() {
    let input: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/sim/tk-009-ioc-cross/input.json")).unwrap();
    let number = |key: &str| input[key].as_i64().unwrap();
    let t = number("at_ns") as u64;
    let bbo = Bbo {
        h: mhdr(S1, BTC_S1, t as i64),
        bid_px: px("59999"),
        ask_px: hft_types::Px(number("px")),
        bid_qty: q("1"),
        ask_qty: Qty(number("depth")),
        update_id: 1,
    };
    let cmd = GwCmd {
        decision_seq: 1,
        decided_tsc: Tsc(t),
        px: bbo.ask_px,
        qty: Qty(number("qty")),
        inst: BTC_S1,
        slot: SlotId(1),
        r#gen: 1,
        target_slot: SlotId(0),
        account: A9,
        op: GwOp::Place,
        side: Buy,
        tif: Tif::Ioc,
        oflags: 0,
        arg: 0,
        arg2: 0,
        _pad: [0; 2],
    };
    let header = rhdr(t);
    let steps = [
        SimStep::Record(&header, SimInput::Market(MarketEventRef::Bbo(&bbo))),
        SimStep::Command(Tsc(t), &cmd),
        SimStep::Advance(Tsc(t + 10_000_000)),
    ];
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/sim/tk-009-ioc-cross/expected.json"
    ))
    .unwrap();
    let events: Vec<_> = expected
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let n = |key: &str| row[key].as_i64().unwrap();
            OrderEvent {
                venue: S1,
                account: A9,
                what: if row["what"] == "Ack" {
                    OrderEventKind::Ack
                } else {
                    OrderEventKind::Fill
                },
                detail: 0,
                slot: SlotId(1),
                r#gen: 1,
                inst: BTC_S1,
                px: hft_types::Px(n("px")),
                qty: Qty(n("qty")),
                cum_filled: Qty(n("cum")),
                fee: Qty(n("fee")),
                fee_asset: AssetId(n("fee_asset") as u16),
                epoch: 0,
                reject_code: 0,
                exch_order_id: 1,
                ex_ts_ns: t as i64,
                hw_rx_ns: 0,
                rx_tsc: n("due_tsc") as u64,
                avg_px: hft_types::Px(n("avg")),
                cash_qty: Qty(0),
                cash_sum: Qty(0),
                trade_id: 0,
            }
        })
        .collect();
    let fresh = || SimExchange::new(&setup_sim(), &rd_sim()).unwrap();
    assert_eq!(run_sim_case(&mut fresh(), &steps, &events), Ok(()), "TK-009.1");
    let mut wrong = events.clone();
    wrong[1].qty.0 += 1000; // rd_sim BTC/S1 lot = 0.00001 at qty_exp -8.
    let diff = run_sim_case(&mut fresh(), &steps, &wrong).unwrap_err();
    assert_eq!((diff.event, diff.field.as_str()), (1, "qty"), "TK-009.2");
    assert_ne!(diff.expected, diff.actual);
    assert_eq!(
        run_sim_case(&mut fresh(), &steps[..2], &events)
            .unwrap_err()
            .field,
        "reply_count"
    );
}
