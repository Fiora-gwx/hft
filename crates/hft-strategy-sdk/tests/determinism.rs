//! Two runs on the same input give identical output (SDK-179).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod common;

use std::collections::HashMap;
use std::hash::{BuildHasher, RandomState};

use common::*;
use hft_strategy_sdk::testing::{CtxHarness, ModelHarness, Recorded};
use hft_strategy_sdk::*;
use hft_types::{GroupId, Tsc};
use proptest::prelude::{ProptestConfig, TestCaseError};
use proptest::{prop_assert, proptest};

type Out = (Vec<Recorded>, Vec<Recorded>, Vec<Vec<(GroupId, u16, Vec<u8>)>>);

/// One step: variant, gix, value, time advance (ms).
type Step = (u8, u16, i64, u32);

fn run(n: u16, steps: &[Step]) -> Out {
    let specs: Vec<_> = (0..n).map(|g| gspec(gid(g))).collect();
    let t = GroupTable::new(&specs, 8);
    let mut v = clocked_views();
    let mut e = EchoProbe::new(s3(0xFF01), &tp(n, &[]), &t, &v.refview()).unwrap();
    let mut p = TargetProbe::new(s3(0xFF02), &tp2(n, &[]), &t, &v.refview()).unwrap();
    let mut m = ProbeModel::new(s3(0xFF02), &tp2(n, &[]), &t, &v.refview());
    let (mut he, mut hp, mut hm) = (
        CtxHarness::new(&EchoProbe::DESC),
        CtxHarness::new(&TargetProbe::DESC),
        ModelHarness::new(),
    );
    let mut out: Out = (Vec::new(), Vec::new(), Vec::new());
    for &(k, g, x, dt) in steps {
        let now = Views::now(&v);
        v.at_tsc(Tsc(now.tsc.0 + u64::from(dt) * 1_000_000));
        let ev = if k == u8::MAX {
            hft_strategy_sdk::testing::ev::timer(MODEL_TICK, hft_types::GROUP_NONE, GIX_NONE)
        } else {
            variant(k, g % n, x, Views::now(&v))
        };
        out.0.push(fast(&mut he, &mut e, &ev, &v));
        out.1.push(fast(&mut hp, &mut p, &ev, &v));
        out.2.push(model(&mut hm, &mut m, &ev, &v));
    }
    out
}

/// Perturbs the process's hashing state between runs: new `RandomState` keys and a populated map.
fn reseed(salt: i64) -> u64 {
    let s = RandomState::new();
    let mut h: HashMap<i64, i64> = HashMap::with_hasher(RandomState::new());
    for i in 0..64 {
        h.insert(i ^ salt, i);
    }
    s.hash_one(salt) ^ h.len() as u64
}

fn check(n: u16, steps: &[Step]) -> Result<(), TestCaseError> {
    let a = run(n, steps);
    let _ = reseed(steps.len() as i64);
    let b = run(n, steps);
    prop_assert!(a == b, "two runs differ");
    Ok(())
}

fn step() -> impl proptest::strategy::Strategy<Value = Step> {
    use proptest::strategy::Strategy as _;
    proptest::prop_oneof![
        9 => (0u8..N_VARIANTS, 0u16..8, -100_000i64..100_000, 0u32..2_000),
        1 => (0u32..2_000).prop_map(|dt| (u8::MAX, 0, 0, dt)),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn sdk_179_two_runs_identical(n in 1u16..=8, steps in proptest::collection::vec(step(), 0..=2_000)) {
        check(n, &steps)?;
    }
}
