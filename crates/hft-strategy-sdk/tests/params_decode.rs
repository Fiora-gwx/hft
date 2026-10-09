//! `ParamSet::decode`/`validate` of a `define_params!` type (SDK-059, SDK-102, SDK-103).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod common;

use std::panic::{AssertUnwindSafe, catch_unwind};

use common::*;
use hft_event::GroupSpec;
use hft_strategy_sdk::params::encode_body;
use hft_strategy_sdk::*;
use hft_types::{GroupId, InstId, VenueId};
use proptest::prelude::{ProptestConfig, any};
use proptest::{prop_assert, prop_oneof, proptest};

const OPEN_TH: u16 = 0;
const ENABLED: usize = 4;

fn body(n_groups: u16, defaults: Vec<i64>, overrides: &[(u16, u16, i64)]) -> Vec<u64> {
    aligned(&encode_body(&TP, n_groups, &raw(defaults, overrides)).unwrap())
}

fn groups(n: u32) -> GroupTable {
    let specs: Vec<_> = (1..=n).map(|k| gspec(GroupId(0x0300_0000 + k))).collect();
    GroupTable::new(&specs, 8)
}

#[test]
fn sdk_059_too_many_groups() {
    for (row, n, ok) in [("SDK-059.1", 7u32, true), ("SDK-059.2", 8, true), ("SDK-059.3", 9, false)] {
        let t = groups(n);
        assert_eq!(t.len(), n as u16, "{row}: the table keeps every spec");
        let b = body(0, tp_defaults(), &[]);
        let r = TPParams::decode(bytes_of(&b, b.len() * 8), 2, &t);
        if ok {
            assert!(r.is_ok(), "{row}: {:?}", r.err());
        } else {
            assert_eq!(r.err(), Some(ParamError::TooManyGroups), "{row}");
        }
    }
}

fn validate(t: &GroupTable, overrides: &[(u16, u16, i64)], enabled: i64) -> Result<(), ParamError> {
    let rd = gt3_fixture();
    let mut d = tp_defaults();
    d[ENABLED] = enabled;
    let b = body(t.len(), d, overrides);
    let p = TPParams::decode(bytes_of(&b, b.len() * 8), 2, t)?;
    p.validate(t, &rd.view())
}

#[test]
fn sdk_103_validate_shape_and_cross_field() {
    let one_leg = spec(G2, BTC_ID, &[(I1, V1)]);
    let g3_spot_leg1 = spec(G3, BTC_ID, &[(I1, V1), (I4, V4)]);
    let g1_i5 = spec(G1, BTC_ID, &[(I1, V1), (I5, V2)]);
    let rows: [(&str, GroupTable, &[(u16, u16, i64)], i64, Result<(), ParamError>); 6] = [
        ("SDK-103.1", gt3(), &[], 1, Ok(())),
        ("SDK-103.2", table(&[gspec(G1), one_leg, gspec(G3)]), &[], 1, Err(ParamError::GroupShape { gix: 1, code: 1 })),
        ("SDK-103.3", table(&[gspec(G1), gspec(G2), g3_spot_leg1]), &[], 1, Err(ParamError::GroupShape { gix: 2, code: 2 })),
        ("SDK-103.4", table(&[g1_i5, gspec(G2), gspec(G3)]), &[], 1, Err(ParamError::GroupShape { gix: 0, code: 3 })),
        ("SDK-103.5", gt3(), &[(1, OPEN_TH, 0)], 1, Err(ParamError::Invalid { field_ix: 0, code: 1 })),
        ("SDK-103.6", gt3(), &[(1, OPEN_TH, 0)], 0, Ok(())),
    ];
    for (row, t, ov, enabled, want) in rows {
        assert_eq!(validate(&t, ov, enabled), want, "{row}");
    }
}

/// A group whose legs use arbitrary (possibly unknown) insts.
fn rand_spec(k: u32, insts: (u32, u32), n_legs: u8) -> GroupSpec {
    let legs = [(InstId(insts.0), VenueId(2)), (InstId(insts.1), VenueId(5))];
    let mut g = spec(GroupId(0x0300_0000 + k), BTC_ID, &legs[..usize::from(n_legs.clamp(1, 2))]);
    g.n_legs = n_legs;
    g
}

fn arb_body() -> impl proptest::strategy::Strategy<Value = Vec<u8>> {
    use proptest::strategy::Strategy as _;
    let valid = encode_body(&TP, 3, &raw(tp_defaults(), &[(0, 0, 6_000_000), (2, 3, 50_000_000)])).unwrap();
    prop_oneof![
        proptest::collection::vec(any::<u8>(), 0..=8_192),
        proptest::collection::vec((any::<usize>(), any::<u8>()), 1..=8).prop_map(move |flips| {
            let mut b = valid.clone();
            for (at, x) in flips {
                let i = at % b.len();
                b[i] ^= x | 1;
            }
            b
        }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(5_000))]

    #[test]
    fn sdk_102_decode_validate_never_panic(
        b in arb_body(),
        ver in 0u16..=5,
        specs in proptest::collection::vec((0u32..200, 0u32..200, 0u8..=4), 0..=9),
    ) {
        let specs: Vec<GroupSpec> =
            specs.iter().enumerate().map(|(k, &(a, c, n))| rand_spec(k as u32 + 1, (a, c), n)).collect();
        let t = GroupTable::new(&specs, 8);
        let rd = gt3_fixture();
        let r = catch_unwind(AssertUnwindSafe(|| {
            let al = aligned(&b);
            let body = bytes_of(&al, b.len());
            match TPParams::decode(body, ver, &t) {
                Ok(p) => p.validate(&t, &rd.view()).err(),
                Err(e) => Some(e),
            }
        }));
        prop_assert!(r.is_ok(), "decode/validate panicked");
    }
}
