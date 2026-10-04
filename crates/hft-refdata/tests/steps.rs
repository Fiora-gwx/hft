//! Load step 7: tick / lot / min_qty re-parsed from the exchange text (REF-028).

mod common;

use common::*;
use hft_refdata::InstSpec;
use hft_refdata::LoadErrCode::{AssetExpTooCoarse, TickMismatch};
use hft_types::Qty;

fn inst1(f: impl FnOnce(&mut InstSpec)) -> Vec<u8> {
    let mut s = f0_spec();
    f(s.inst_mut(1));
    s.build()
}

#[test]
fn ref_028_tick_lot_recheck() {
    let tm = e(TickMismatch, 3, 0);
    let coarse = e(AssetExpTooCoarse, 3, 0);
    ok(&inst1(|_| {})); // REF-028.1
    assert_eq!(errs(&inst1(|i| i.tick = 9_999_999)), vec![tm], "REF-028.2");
    assert_eq!(
        errs(&inst1(|i| i.tick_str = "0.000000001".into())),
        vec![coarse],
        "REF-028.3"
    );
    let b = inst1(|i| {
        i.tick_str = "0".into();
        i.tick = 0;
    });
    assert_eq!(errs(&b), vec![tm], "REF-028.4");
    let b = inst1(|i| {
        i.tick_str = "-0.10".into();
        i.tick = -10_000_000;
    });
    assert_eq!(errs(&b), vec![tm], "REF-028.5");
    assert_eq!(errs(&inst1(|i| i.tick_str = "abc".into())), vec![tm], "REF-028.6");
    assert_eq!(errs(&inst1(|i| i.lot = 100_001)), vec![tm, tm], "REF-028.7");
    assert_eq!(
        errs(&inst1(|i| i.lot_str = "0.0000000001".into())),
        vec![coarse],
        "REF-028.8"
    );
    let b = inst1(|i| {
        i.lot_str = "0".into();
        i.lot = 0;
    });
    let r = load(&b).expect_err("REF-028.9");
    assert_eq!(r.code(), TickMismatch, "REF-028.9");
    assert!(r.errors.iter().all(|x| *x == tm), "REF-028.9: {:?}", r.errors);
    let b = inst1(|i| {
        i.min_qty_str = "0.003".into();
        i.min_qty = Qty(300_000);
    });
    ok(&b); // REF-028.10
    let b = inst1(|i| {
        i.min_qty_str = "0.0035".into();
        i.min_qty = Qty(350_000);
    });
    assert_eq!(errs(&b), vec![tm], "REF-028.11");
    let b = inst1(|i| {
        i.min_qty_str = "0.003".into();
        i.min_qty = Qty(300_001);
    });
    assert_eq!(errs(&b), vec![tm, tm], "REF-028.12");
    let b = inst1(|i| {
        i.min_qty_str = "0".into();
        i.min_qty = Qty(0);
    });
    assert_eq!(errs(&b), vec![tm], "REF-028.13");
    assert_eq!(
        errs(&inst1(|i| i.min_qty_str = "0.0000000001".into())),
        vec![coarse],
        "REF-028.14"
    );

    let mut s = f0_spec();
    let i3 = s.inst_mut(3);
    i3.qty_unit_shift = -1;
    i3.lot_str = "1".into();
    i3.lot = 50_000_000;
    i3.min_qty_str = "1".into();
    i3.min_qty = Qty(50_000_000);
    assert_eq!(
        errs(&s.build()),
        vec![e(TickMismatch, 3, 2), e(TickMismatch, 3, 2)],
        "REF-028.15"
    );
}
