//! LED-179: write-offs and adoptions interleaved keep the base-asset invariant and the position
//! source rules.

use crate::common;

use common::opgen::*;
use common::*;
use hft_ledger::Ledger;
use proptest::prelude::*;

fn adjust_op() -> impl Strategy<Value = Op> {
    let px = (3_000i64..=9_000).prop_map(|k| k * 100_000);
    prop_oneof![
        4 => (any::<bool>(), any::<bool>(), 1i64..=1_000, px.clone(), prop::bool::weighted(0.2))
            .prop_map(|(perp, buy, lots, px, g4)| Op::Fill { perp, buy, lots, px, fee: 1, late: false, g4 }),
        // Position adoption within ±1.0 BTC (lots of 0.001).
        1 => (-1_000i64..=1_000, px.clone()).prop_map(|(lots, px)| Op::AdoptPos { lots, px }),
        1 => (any::<bool>(), 1i64..=500, px).prop_map(|(group, lots, px)| Op::AdoptBal { group, lots, px }),
        2 => (any::<bool>(), 0i64..=100).prop_map(|(perp, pct)| Op::WriteOff { perp, pct }),
    ]
}

fn totals(l: &Ledger) -> Vec<i64> {
    [S1, P2]
        .iter()
        .flat_map(|v| {
            l.venue_book(A1, *v)
                .cash
                .iter()
                .map(|e| e.total.0)
                .collect::<Vec<_>>()
        })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(cases(2_000)))]

    #[test]
    fn led_179_adjust_writeoff_invariants(
        fee_ix in 0usize..4,
        ops in prop::collection::vec(adjust_op(), 0..=100),
    ) {
        let mut r = Run::new(fee_ix, 0, &cfg_lf());
        for (k, op) in ops.iter().enumerate() {
            let pos0 = [*r.l.pos(A1, BTC_S1), *r.l.pos(A1, BTC_P2)];
            let tot0 = totals(&r.l);
            let leg0 = [r.l.leg_net(A1, G1, 0), r.l.leg_net(A1, G1, 1)];
            let ung0 = [r.l.ungrouped_qty(A1, BTC_S1), r.l.ungrouped_qty(A1, BTC_P2)];
            let un0 = cash(&r.l, A1, S1, BTC).unassigned;
            let inv0 = r.l.inventory(A1, S1, BTC, G1);
            r.step(op);
            if let Err(e) = base_invariant(&r.l, S1) {
                prop_assert!(false, "step {k} {op:?}: {e}");
            }
            if let Op::WriteOff { perp, pct } = *op {
                let li = usize::from(perp);
                let q = leg0[li].0 * pct / 100;
                prop_assert_eq!([*r.l.pos(A1, BTC_S1), *r.l.pos(A1, BTC_P2)], pos0, "step {} WriteOff moved InstPos", k);
                prop_assert_eq!(totals(&r.l), tot0, "step {} WriteOff moved cash", k);
                prop_assert_eq!(r.l.leg_net(A1, G1, li as u8).0, leg0[li].0 - q, "step {} leg", k);
                let ung = [r.l.ungrouped_qty(A1, BTC_S1), r.l.ungrouped_qty(A1, BTC_P2)];
                prop_assert_eq!(ung[li].0, ung0[li].0 + q, "step {} account-level", k);
                if !perp {
                    // A positive write-off moves at most what the group holds.
                    let moved = if q > 0 { q.min(inv0.0) } else { q };
                    prop_assert_eq!(cash(&r.l, A1, S1, BTC).unassigned.0, un0.0 + moved, "step {} unassigned", k);
                    prop_assert_eq!(r.l.inventory(A1, S1, BTC, G1).0, inv0.0 - moved, "step {} inventory", k);
                }
            }
        }
    }
}
