//! LED-240–LED-243, LED-245: ledger-wide invariants over random operation sequences.

use crate::common;

use common::opgen::*;
use common::*;
use hft_ledger::{InstPos, InvEntry, Ledger};
use hft_types::InstId;
use proptest::prelude::*;

/// Σ PnL booked on `A1` (every column of the account's day book, unrealized included) plus the
/// funding residual (wallet funding not attributed to a group).
fn pnl_total(l: &Ledger) -> i128 {
    let a = l.account(A1);
    i128::from(a.day.net().0) + i128::from(a.funding_residual.0)
}

/// Test-side value of `A1`'s holdings at constant prices (BTC 60 000, BNB 600, USDC 1, mark
/// 60 000), in USDT raw, computed in i128 from the ledger's quantities.
fn value_const(l: &Ledger) -> i128 {
    let price = |a: hft_types::AssetId| -> i128 {
        match a {
            BTC => 600_000_000,
            BNB => 6_000_000,
            _ => 10_000,
        }
    };
    let mut v = 0i128;
    for e in l.venue_book(A1, S1).cash.iter() {
        // qty (1e-6 or 1e-8) × px (1e-4) → USDT raw (1e-6).
        let scale = if e.asset == USDT || e.asset == USDC {
            10_000
        } else {
            1_000_000
        };
        v += i128::from(e.total.0) * price(e.asset) / scale;
    }
    v += i128::from(cash(l, A1, P2, USDT).total.0);
    for inst in [BTC_P2, ETH_P2] {
        let p = l.pos(A1, inst);
        let m: i128 = if inst == BTC_P2 { 600_000_000 } else { 30_000_000 };
        let value = i128::from(p.qty.0) * m;
        let u = if p.qty.0 >= 0 {
            value - p.cost
        } else {
            value + p.cost
        };
        v += u / 1_000_000;
    }
    v
}

fn equity_all(l: &Ledger) -> i128 {
    [S1, P2]
        .iter()
        .map(|v| i128::from(l.venue_book(A1, *v).summary.equity.0))
        .sum()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(cases(2_000)))]

    #[test]
    fn led_240_pnl_equals_cashflow((fee_ix, fund_ix, ops) in case(false, 300)) {
        let mut r = Run::new(fee_ix, fund_ix, &cfg_prop());
        let (v0, p0) = (value_const(&r.l), pnl_total(&r.l));
        let mut injected = 0i128;
        for op in &ops {
            let (vb, pb) = (value_const(&r.l), pnl_total(&r.l));
            let calls = r.step(op);
            if calls.contains(&Call::AdoptBal) {
                // Balance adoption brings assets in from outside: the part not booked as PnL.
                injected += (value_const(&r.l) - vb) - (pnl_total(&r.l) - pb);
            }
        }
        let dv = value_const(&r.l) - v0 - injected - r.residual;
        let dp = pnl_total(&r.l) - p0;
        let tol = 8 * (ops.len() as i128 + 1);
        prop_assert!((dv - dp).abs() <= tol, "value change {dv} vs PnL {dp} (tol {tol})");
    }

    #[test]
    fn led_241_base_asset_invariant((fee_ix, fund_ix, ops) in case(true, 300)) {
        let mut r = Run::new(fee_ix, fund_ix, &cfg_lf());
        for (k, op) in ops.iter().enumerate() {
            let g4_before = r.l.inventory(A1, S1, BNB, G4);
            r.step(op);
            if let Err(e) = base_invariant(&r.l, S1) {
                prop_assert!(false, "step {k} {op:?}: {e}");
            }
            if let Op::Fill { fee: 3, g4: false, perp: false, .. } = op {
                prop_assert!(r.l.inventory(A1, S1, BNB, G4).0 >= g4_before.0, "step {k}: BNB fee touched G4");
            }
        }
    }

    #[test]
    fn led_242_cash_conservation((fee_ix, fund_ix, ops) in case(true, 300)) {
        let mut r = Run::new(fee_ix, fund_ix, &cfg_prop());
        for (k, op) in ops.iter().enumerate() {
            let (e0, p0, r0) = (equity_all(&r.l), pnl_total(&r.l), r.residual);
            let calls = r.step(op);
            if r.approx || calls.iter().any(|c| matches!(c, Call::Price | Call::AdoptBal)) {
                continue;
            }
            let (de, dp) = (equity_all(&r.l) - e0 - (r.residual - r0), pnl_total(&r.l) - p0);
            prop_assert!((de - dp).abs() <= 16, "step {k} {op:?}: equity {de} vs PnL {dp}");
        }
    }

    #[test]
    fn led_243_position_change_sources((fee_ix, fund_ix, ops) in case(true, 300)) {
        let mut r = Run::new(fee_ix, fund_ix, &cfg_lf());
        let state = |l: &Ledger| -> (Vec<InstPos>, Vec<InvEntry>) {
            let pos = [BTC_S1, BNB_S1, BTC_P2, ETH_P2].iter().map(|i: &InstId| *l.pos(A1, *i)).collect();
            (pos, l.venue_book(A1, S1).inv.iter().copied().collect())
        };
        for (k, op) in ops.iter().enumerate() {
            let (p0, i0) = state(&r.l);
            let calls = r.step(op);
            let (p1, i1) = state(&r.l);
            let moves_pos = calls.iter().any(|c| matches!(c, Call::Fill | Call::Fee | Call::ExtSome | Call::AdoptPos | Call::AdoptBal));
            if moves_pos {
                continue;
            }
            prop_assert_eq!(&p0, &p1, "step {} {:?}: InstPos changed", k, op);
            if !calls.contains(&Call::WriteOff) && !calls.contains(&Call::Baseline) {
                prop_assert_eq!(&i0, &i1, "step {} {:?}: inventory changed", k, op);
            }
        }
    }

    #[test]
    fn led_245_replays_idempotent(
        (fee_ix, fund_ix, ops) in case(true, 200),
        dups in prop::collection::vec((0usize..200, 1usize..=3, 0usize..60), 0..20),
    ) {
        let mut plain = Run::new(fee_ix, fund_ix, &cfg_lf());
        let mut with = Run::new(fee_ix, fund_ix, &cfg_lf());
        let kind = KINDS[fund_ix];
        // Insertions after position `at + dist`: copies of op `at` (externals, and pushes on
        // BalanceOnlyPush venues).
        let mut extra: Vec<Vec<Op>> = vec![Vec::new(); ops.len() + 1];
        for &(at, n, dist) in &dups {
            let Some(op) = ops.get(at) else { continue };
            let replayable = match op {
                Op::Ext { .. } => true,
                Op::Fund { push: true, .. } => kind == hft_refdata::FundingSettleKind::BalanceOnlyPush,
                _ => false,
            };
            if !replayable {
                continue;
            }
            let slot = (at + dist).min(ops.len() - 1);
            for _ in 0..n {
                extra[slot].push(op.clone());
            }
        }
        for (k, op) in ops.iter().enumerate() {
            let t = s(2 + k as i64);
            plain.step_at(op, t);
            with.step_at(op, t);
            for d in &extra[k] {
                with.step_at(d, t);
            }
        }
        prop_assert_eq!(snap(&plain.l), snap(&with.l));
    }
}
