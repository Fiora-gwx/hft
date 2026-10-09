//! LED-231: snapshot round trip, and a restored ledger behaves like the original afterwards.

use crate::common;

use common::opgen::*;
use common::*;
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(cases(1_000)))]

    #[test]
    fn led_231_snapshot_roundtrip(
        (fee_ix, fund_ix, ops) in case(true, 300),
        at in 0usize..=300,
    ) {
        let mut a = Run::new(fee_ix, fund_ix, &cfg_lf());
        let at = at.min(ops.len());
        for op in &ops[..at] {
            a.step(op);
        }
        let bytes = snap(&a.l);
        let restored = restore_with(&bytes, rd_combo(fee_ix, fund_ix));
        prop_assert!(restored.is_ok(), "restore: {:?}", restored.as_ref().err());
        let mut b = a.clone();
        b.l = restored.expect("checked");
        prop_assert_eq!(&snap(&b.l), &bytes, "restore → snapshot differs");
        for (k, op) in ops[at..].iter().enumerate() {
            let ca = a.step(op);
            let cb = b.step(op);
            prop_assert_eq!(&ca, &cb, "step {}: calls differ", at + k);
            prop_assert_eq!(snap(&a.l), snap(&b.l), "step {} {:?}: snapshots differ", at + k, op);
        }
    }
}
