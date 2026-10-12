//! TK-104: FeeOnly must have an observable ledger effect, including tombs and unknown orders.

use hft_event::{EventKind, OrderEvent, OrderEventKind, RecordFlags};
use hft_testkit::dst::invariants::{InvId, InvState, StepView, checker};
use hft_testkit::dst::obs::{
    AssetRow, InstRow, NoteInfo, Obs, PosRow, SlotRow, SlotState, StepInfo, TombRow,
};
use hft_testkit::dst::sut::JRec;
use hft_testkit::dst::world::{ACCT_A, BTC, G1, S1, USDT, VS};
use hft_types::{Px, Qty, Side, SlotId};
use zerocopy::IntoBytes;

#[test]
fn tk_104_fee_only_checker() {
    for path in ["current", "tomb", "stale", "wrong-epoch", "unresolved"] {
        for base in [false, true] {
            for fee in [17, -7, 0] {
                let slot = SlotRow {
                    venue: VS,
                    account: ACCT_A,
                    slot: 1,
                    r#gen: 3,
                    epoch: 2,
                    state: SlotState::Filled,
                    group: G1,
                    inst: S1,
                    side: Side::Buy,
                    qty: 1_000,
                    filled: 1_000,
                    immediate: true,
                    risk_reducing: false,
                    terminal_ens: 1,
                };
                let mut before = Obs::default();
                before.insts.push(InstRow {
                    inst: S1,
                    venue: VS,
                    asset: BTC,
                    perp: false,
                    lot: 1_000,
                    min_qty: 1_000,
                });
                before.positions.push(PosRow {
                    account: ACCT_A,
                    venue: VS,
                    inst: S1,
                    asset: BTC,
                    qty: 1_000,
                });
                before.inventory.push(AssetRow {
                    account: ACCT_A,
                    venue: VS,
                    asset: BTC,
                    qty: 1_000,
                });
                before.balances.push(AssetRow {
                    account: ACCT_A,
                    venue: VS,
                    asset: USDT,
                    qty: 100_000,
                });
                if path == "current" {
                    before.slots.push(slot);
                } else if matches!(path, "tomb" | "wrong-epoch") {
                    before.tombs.push(TombRow {
                        venue: VS,
                        account: ACCT_A,
                        slot: 1,
                        r#gen: 3,
                        epoch: 2,
                        inst: S1,
                        group: G1,
                        filled: 1_000,
                    });
                }
                let matched = matches!(path, "current" | "tomb");
                let e = OrderEvent {
                    venue: VS,
                    account: ACCT_A,
                    inst: S1,
                    what: OrderEventKind::FeeOnly,
                    detail: 0,
                    slot: if path == "unresolved" {
                        SlotId::UNRESOLVED
                    } else {
                        SlotId(1)
                    },
                    r#gen: 3,
                    epoch: if path == "wrong-epoch" { 9 } else { 2 },
                    px: Px(0),
                    qty: Qty(0),
                    cum_filled: Qty(1_000),
                    fee: Qty(fee),
                    fee_asset: if base { BTC } else { USDT },
                    reject_code: 0,
                    exch_order_id: 1,
                    ex_ts_ns: 0,
                    hw_rx_ns: 0,
                    rx_tsc: 0,
                    avg_px: Px(0),
                    cash_qty: hft_types::Qty(0),
                    cash_sum: hft_types::Qty(0),
                };
                let recs = [JRec {
                    seq: 1,
                    tsc: 1,
                    kind: EventKind::ORDER_EVENT,
                    flags: RecordFlags(0),
                    body: e.as_bytes().to_vec(),
                }];
                let mut info = StepInfo::default();
                if !matched {
                    info.notes.push(NoteInfo {
                        seq: 1,
                        name: "OmsFeeUnattributed",
                        account: ACCT_A,
                        group: hft_types::GroupId(0),
                        inst: S1,
                        v: [0; 8],
                    });
                }
                let check = |after: &Obs, info: &StepInfo| {
                    (checker(InvId::I2).unwrap().step)(
                        &mut InvState::default(),
                        &StepView {
                            step: 1,
                            now_ns: 1,
                            records: &recs,
                            delivered: Some((EventKind::ORDER_EVENT, e.as_bytes())),
                            frames: &[],
                            info,
                            before: &before,
                            after,
                            truth: &[],
                            rebuild: None,
                        },
                    )
                };
                let mut after = before.clone();
                if base {
                    after.positions[0].qty -= i128::from(fee);
                    after.inventory[0].qty -= i128::from(fee);
                } else {
                    after.balances[0].qty -= i128::from(fee);
                }
                assert!(check(&after, &info).is_ok(), "{path} base={base} fee={fee}");
                if fee != 0 {
                    assert!(check(&before, &info).is_err(), "missing fee accepted: {path}");
                    let mut doubled = after.clone();
                    if base {
                        doubled.positions[0].qty -= i128::from(fee);
                        doubled.inventory[0].qty -= i128::from(fee);
                    } else {
                        doubled.balances[0].qty -= i128::from(fee);
                    }
                    assert!(check(&doubled, &info).is_err(), "double fee accepted: {path}");
                }
                if !matched {
                    assert!(
                        check(&after, &StepInfo::default()).is_err(),
                        "missing alarm: {path}"
                    );
                } else if path == "current" {
                    after.slots[0].filled += 1;
                    assert!(check(&after, &info).is_err(), "FeeOnly changed cumulative fill");
                }
            }
        }
    }
}
