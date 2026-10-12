//! Perpetual cash facts are additive even when the fill delivery order changes (LED-275–278).
use crate::common::*;
use hft_ledger::{FillDelta, PerpCashSettlement, SettlementError};
use hft_types::{AccountId, InstId, Px, Qty};

/// Raw executions from the fixed cash-order example. Cash excludes the fee. It is derived
/// from generation order in TK-123, never from this test's ledger balance or cost.
fn executions(mirror: bool) -> [(FillDelta, PerpCashSettlement); 5] {
    [
        (Sell, 100_000, 600_189_000, 30_010, 0),
        (Sell, 100_000, 600_244_000, 30_013, 0),
        (Buy, 200_000, 600_275_000, 60_028, -11_700),
        (Sell, 100_000, 600_274_000, 30_014, 0),
        (Buy, 100_000, 600_275_000, 30_014, -100),
    ]
    .map(|(side, qty, px, fee, cash)| {
        let side = if mirror {
            if side == Buy { Sell } else { Buy }
        } else {
            side
        };
        let mut fill = f(side, BTC_P2, "0.001", "60000", "0", USDT, G1, 1);
        fill.qty = Qty(qty);
        fill.net_qty = Qty(qty);
        fill.px = Px(px);
        fill.fee = fee;
        (
            fill,
            PerpCashSettlement {
                asset: USDT,
                amount: Qty(if mirror { -cash } else { cash }),
            },
        )
    })
}

fn permutations() -> Vec<[usize; 5]> {
    let mut result = Vec::new();
    for a in 0..5 {
        for b in 0..5 {
            for c in 0..5 {
                for d in 0..5 {
                    for e in 0..5 {
                        let row = [a, b, c, d, e];
                        let mut sorted = row;
                        sorted.sort();
                        if sorted == [0, 1, 2, 3, 4] {
                            result.push(row);
                        }
                    }
                }
            }
        }
    }
    assert_eq!(result.len(), 120);
    result
}

#[test]
fn led_275_settlement_cash_is_exact_in_every_delivery_order() {
    for mirror in [false, true] {
        let fills = executions(mirror);
        for order in permutations() {
            let mut ledger = booted();
            let mut expected = 50_000_000_000i64;
            let mut quantity = 0i64;
            for (step, ix) in order.into_iter().enumerate() {
                let (fill, settlement) = fills[ix];
                let applied = ledger
                    .apply_settled_fill(&fill, settlement, s(step as i64 + 1))
                    .unwrap();
                expected += settlement.amount.0 - fill.fee;
                quantity += fill.side.sign() * fill.qty.0;
                assert_eq!(
                    cash(&ledger, A1, P2, USDT).total.0,
                    expected,
                    "{order:?} at {step}"
                );
                assert_eq!(ledger.margin(A1, P2).wallet.0, expected);
                assert_eq!(ledger.pos(A1, BTC_P2).qty.0, quantity);
                assert_eq!(applied.pnl.d.fee_cex.0, -fill.fee);
                assert_eq!(applied.net_adjust, None);
            }
            let realized = if mirror { 11_800 } else { -11_800 };
            assert_eq!(expected, 50_000_000_000 + realized - 180_079);
            assert_eq!(ledger.pos(A1, BTC_P2).qty, Qty(0));
            assert_eq!(ledger.pos(A1, BTC_P2).cost, 0);
        }
    }
}

#[test]
fn led_276_zero_is_known_and_local_cost_is_not_cash() {
    let rows = executions(false);
    let mut settled = booted();
    let mut local = booted();
    for (step, ix) in [0, 1, 3, 2, 4].into_iter().enumerate() {
        let (fill, settlement) = rows[ix];
        let a = settled
            .apply_settled_fill(&fill, settlement, s(step as i64 + 1))
            .unwrap();
        let b = local.apply_fill(&fill, s(step as i64 + 1));
        // The cash-only API does not rewrite the analytical average-cost books or fees.
        assert_eq!(a, b);
        assert_eq!(settled.pos(A1, BTC_P2), local.pos(A1, BTC_P2));
        assert_eq!(settled.group(A1, G1).pnl, local.group(A1, G1).pnl);
        if step == 3 {
            assert_eq!(settled.pos(A1, BTC_P2).qty.0, -100_000);
            assert_eq!(
                cash(&settled, A1, P2, USDT).total.0,
                50_000_000_000 - 11_700 - 150_065
            );
            assert_eq!(
                cash(&local, A1, P2, USDT).total.0,
                50_000_000_000 - 7_866 - 150_065
            );
        }
    }
    assert_eq!(
        cash(&local, A1, P2, USDT).total.0 - cash(&settled, A1, P2, USDT).total.0,
        1
    );

    // This close has non-zero local realized PnL, but a supplied zero must stay zero.
    let mut l = booted();
    let buy = f(Buy, BTC_P2, "1", "60000", "0", USDT, G1, 1);
    let sell = f(Sell, BTC_P2, "1", "61000", "0", USDT, G1, 1);
    for (step, fill) in [buy, sell].iter().enumerate() {
        l.apply_settled_fill(
            fill,
            PerpCashSettlement {
                asset: USDT,
                amount: Qty(0),
            },
            s(step as i64 + 1),
        )
        .unwrap();
    }
    assert_eq!(cash(&l, A1, P2, USDT).total, usdt("50000"));
    assert_eq!(l.group(A1, G1).pnl.price_realized, n("1000"));
}

#[test]
fn led_277_settlement_validation_is_atomic() {
    let (good, cash) = executions(false)[0];
    for row in 0..9 {
        let mut f = good;
        let mut c = cash;
        let expected = match row {
            0 => {
                f.inst = InstId(u32::MAX);
                SettlementError::UnknownInstrument
            }
            1 => {
                f.inst = BTC_S1;
                f.venue = S1;
                SettlementError::NotPerpetual
            }
            2 => {
                f.venue = S1;
                SettlementError::VenueMismatch
            }
            3 => {
                f.account = AccountId(0);
                SettlementError::AccountVenue
            }
            4 => {
                c.asset = BTC;
                SettlementError::WrongAsset
            }
            5 => {
                f.qty = Qty(0);
                SettlementError::InvalidQuantity
            }
            6 => {
                f.qty = Qty(-1);
                SettlementError::InvalidQuantity
            }
            7 => {
                f.net_qty.0 += 1;
                SettlementError::InvalidQuantity
            }
            _ => {
                f.px = Px(0);
                SettlementError::InvalidPrice
            }
        };
        let mut l = booted();
        let before = snap(&l);
        assert_eq!(l.apply_settled_fill(&f, c, s(1)), Err(expected));
        assert_eq!(snap(&l), before, "validation row {row} changed the ledger");
    }
}

#[test]
fn led_278_settlement_snapshot_resume_and_late_fee() {
    let rows = executions(false);
    let order = [0, 1, 3, 2, 4];
    for cut in 0..=5 {
        let mut continuous = booted();
        let mut resumed = booted();
        for (step, ix) in order.into_iter().enumerate() {
            if step == cut {
                resumed = restore(&snap(&resumed)).unwrap();
            }
            let (mut fill, settled) = rows[ix];
            fill.fee = 0;
            for l in [&mut continuous, &mut resumed] {
                l.apply_settled_fill(&fill, settled, s(step as i64 + 1)).unwrap();
            }
            assert_eq!(snap(&resumed), snap(&continuous));
        }
        if cut == 5 {
            resumed = restore(&snap(&resumed)).unwrap();
        }
        for (step, (fill, _)) in rows.iter().enumerate() {
            let mut fee = fee_delta(fill.side, BTC_P2, G1, "0", USDT);
            fee.fee = fill.fee;
            for l in [&mut continuous, &mut resumed] {
                l.apply_fee(&fee, s(step as i64 + 6));
            }
        }
        assert_eq!(snap(&resumed), snap(&continuous));
        assert_eq!(
            cash(&resumed, A1, P2, USDT).total.0,
            50_000_000_000 - 11_800 - 180_079
        );
    }
}
