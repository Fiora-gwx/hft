//! INV-16 compares the union of sparse keys, including a zero exchange-side quantity.
use hft_testkit::dst::invariants::{EndView, InvId, InvState, StepView, checker};
use hft_testkit::dst::obs::{AssetRow, InstRow, Obs, ObservationRequirements, PosRow, StepInfo, TruthRow};
use hft_testkit::dst::world::{ACCT_A, ACCT_B, BTC, P1, USDT, VP};

fn passes(o: &Obs, truth: &[TruthRow]) -> bool {
    let info = StepInfo::default();
    let c = checker(InvId::I16).unwrap();
    let step = StepView {
        step: 1,
        now_ns: 1,
        records: &[],
        delivered: None,
        frames: &[],
        info: &info,
        before: o,
        after: o,
        truth,
        rebuild: None,
    };
    let end = EndView {
        now_ns: 1,
        obs: o,
        truth,
    };
    let a = (c.step)(&mut InvState::default(), &step);
    let b = c.end.unwrap()(&mut InvState::default(), &end);
    assert_eq!(
        a, b,
        "step and final check must have the same completeness requirements"
    );
    a.is_ok()
}

#[test]
fn tk_125_truth_comparison_includes_zero_and_missing_rows() {
    let pos = PosRow {
        account: ACCT_A,
        venue: VP,
        inst: P1,
        asset: BTC,
        qty: 0,
    };
    let asset = AssetRow {
        account: ACCT_A,
        venue: VP,
        asset: BTC,
        qty: 0,
    };
    let mut o = Obs::default();
    o.insts.push(InstRow {
        inst: P1,
        venue: VP,
        asset: BTC,
        perp: true,
        lot: 1,
        min_qty: 1,
    });
    o.balances.push(AssetRow {
        asset: USDT,
        qty: 100,
        ..asset
    });
    let t = TruthRow {
        account: ACCT_A,
        venue: VP,
        positions: vec![],
        inventory: vec![],
        balances: vec![(USDT, 100)],
        undelivered: false,
        excluded: vec![],
    };
    assert!(passes(&o, std::slice::from_ref(&t)));
    // An explicit zero is the same as an absent sparse entry, in either direction.
    let mut zeros = o.clone();
    zeros.positions.push(pos);
    zeros.inventory.push(asset);
    zeros.balances.push(asset);
    assert!(passes(&zeros, std::slice::from_ref(&t)));
    let mut zeros = t.clone();
    zeros.positions.push((P1, 0));
    zeros.inventory.push((BTC, 0));
    zeros.balances.push((BTC, 0));
    assert!(passes(&o, &[zeros]));
    for q in [-1, 1] {
        for row in 0..6 {
            let mut a = o.clone();
            let mut b = t.clone();
            match row {
                0 => a.positions.push(PosRow { qty: q, ..pos }),
                1 => a.inventory.push(AssetRow { qty: q, ..asset }),
                2 => a.balances.push(AssetRow { qty: q, ..asset }),
                3 => b.positions.push((P1, q)),
                4 => b.inventory.push((BTC, q)),
                5 => b.balances.push((BTC, q)),
                _ => unreachable!(),
            }
            assert!(
                !passes(&a, std::slice::from_ref(&b)),
                "sparse counterexample {row}/{q}"
            );
            b.undelivered = true;
            assert!(passes(&a, &[b]), "quiescence boundary changed");
        }
    }
    for row in 0..11 {
        let mut a = o.clone();
        let mut b = vec![t.clone()];
        match row {
            0 => a.balances.push(a.balances[0]),
            1 => b[0].balances.push((USDT, 100)),
            2 => a.positions.extend([pos, pos]),
            3 => b[0].positions.extend([(P1, 0); 2]),
            4 => a.inventory.extend([asset, asset]),
            5 => b[0].inventory.extend([(BTC, 0); 2]),
            6 => b.clear(),
            7 => b.push(t.clone()),
            8 => {
                a.required = Some(ObservationRequirements {
                    venue_accounts: vec![(VP, ACCT_A), (VP, ACCT_B)],
                    ..Default::default()
                })
            }
            9 => {
                a.insts.clear();
                a.positions.push(pos);
            }
            10 => {
                a.required = Some(ObservationRequirements {
                    instruments: vec![(ACCT_B, P1)],
                    ..Default::default()
                })
            }
            _ => unreachable!(),
        }
        assert!(!passes(&a, &b), "incomplete / duplicate counterexample {row}");
    }
    // The instrument's spot position is ownership, not total exchange inventory.
    let mut spot = o.clone();
    spot.insts[0].perp = false;
    spot.positions.push(PosRow { qty: 7, ..pos });
    assert!(passes(&spot, std::slice::from_ref(&t)));
    let mut excluded = t;
    excluded.excluded.push(P1.0);
    let mut held = o;
    held.positions.push(PosRow { qty: 1, ..pos });
    assert!(passes(&held, &[excluded]));
}
