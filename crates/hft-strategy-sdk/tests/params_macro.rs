//! `define_params!` expansion (SDK-089, SDK-104).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::common;

use common::*;
use hft_strategy_sdk::params::encode_body;
use hft_strategy_sdk::*;
use hft_types::{Qty, Rate};

const OPEN_TH: u16 = 0;
const PCT_BIN: u16 = 2;

fn p(defaults: &[(u16, i64)], overrides: &[(u16, u16, i64)]) -> TPParams {
    let mut d = tp_defaults();
    for &(f, v) in defaults {
        d[usize::from(f)] = v;
    }
    TPParams::from_raw(&raw(d, overrides), 3)
}

#[test]
fn sdk_089_structural_changed() {
    let old = p(&[], &[(1, PCT_BIN, 200_000)]);
    let t = gt3();
    let gix = |g| t.gix_of(g).unwrap();
    let ch = |new: &TPParams, g| new.structural_changed(&old, gix(g), gix(g));
    let over_300 = p(&[], &[(1, PCT_BIN, 300_000)]);
    let def_150 = p(&[(PCT_BIN, 150_000)], &[(1, PCT_BIN, 200_000)]);
    assert!(!ch(&p(&[], &[(1, PCT_BIN, 200_000)]), G2), "SDK-089.1");
    assert!(ch(&over_300, G2), "SDK-089.2");
    assert!(!ch(&over_300, G1), "SDK-089.3");
    assert!(ch(&def_150, G1), "SDK-089.4");
    assert!(!ch(&def_150, G2), "SDK-089.5");
    assert!(ch(&p(&[], &[]), G2), "SDK-089.6");
    assert!(
        !ch(&p(&[], &[(0, PCT_BIN, 100_000), (1, PCT_BIN, 200_000)]), G1),
        "SDK-089.7"
    );
    assert!(
        !ch(&p(&[(OPEN_TH, 7_000_000)], &[(1, PCT_BIN, 200_000)]), G1),
        "SDK-089.8"
    );
    // GroupId alignment: G2 moved from gix 1 to gix 0 keeps its effective value.
    let moved = TPParams::from_raw(&raw(tp_defaults(), &[(0, PCT_BIN, 200_000)]), 3);
    assert!(
        !moved.structural_changed(&old, 1, 0),
        "aligned by GroupId, not gix"
    );
}

#[test]
fn sdk_104_at_applies_overrides() {
    let b = aligned(
        &encode_body(
            &TP,
            3,
            &raw(tp_defaults(), &[(0, 0, 6_000_000), (2, 3, 50_000_000)]),
        )
        .unwrap(),
    );
    let p = TPParams::decode(bytes_of(&b, b.len() * 8), 2, &gt3()).unwrap();
    assert_eq!(p.defaults().open_th, Rate(5_000_000));
    assert_eq!(p.at(0).open_th, Rate(6_000_000));
    assert_eq!(p.at(1).open_th, Rate(5_000_000));
    assert_eq!(p.at(2).max_qty, Qty(50_000_000));
    assert_eq!(p.at(1).max_qty, Qty(0));
    assert_eq!(p.at(0).mode, 0, "\"a\"");
    assert_eq!(p.n_groups(), 3);
}
