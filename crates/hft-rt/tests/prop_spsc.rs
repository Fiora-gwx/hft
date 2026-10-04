//! Hints never exceed the truth; contents match a bounded VecDeque (RT-004).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![cfg(not(loom))]

use std::collections::VecDeque;

use hft_rt::{Backing, spsc};
use proptest::prelude::*;

#[derive(Clone, Debug)]
enum Op {
    Push,
    Pop,
    WriteBatch(usize),
    ReadBatch(usize),
    Hints,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        Just(Op::Push),
        Just(Op::Pop),
        (0usize..80).prop_map(Op::WriteBatch),
        (0usize..80).prop_map(Op::ReadBatch),
        Just(Op::Hints),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]
    #[test]
    fn rt_004_hints_never_exceed_truth(
        cap in prop::sample::select(vec![2usize, 4, 8, 64]),
        ops in prop::collection::vec(op(), 1..2000),
    ) {
        let (mut p, mut c) = spsc::<u64>(cap, Backing::Heap).unwrap();
        let mut model: VecDeque<u64> = VecDeque::new();
        let mut next = 0u64;
        for o in ops {
            match o {
                Op::Push => {
                    let r = p.try_push(next);
                    if model.len() < cap {
                        prop_assert_eq!(r, Ok(()));
                        model.push_back(next);
                    } else {
                        prop_assert_eq!(r, Err(next));
                    }
                    next += 1;
                }
                Op::Pop => prop_assert_eq!(c.try_pop(), model.pop_front()),
                Op::WriteBatch(k) => {
                    let mut b = p.write_batch(k);
                    for _ in 0..k {
                        if !b.push(next) {
                            break;
                        }
                        model.push_back(next);
                        next += 1;
                    }
                    b.commit();
                    prop_assert!(model.len() <= cap);
                }
                Op::ReadBatch(k) => {
                    let b = c.read_batch(k);
                    let (a, z) = b.slices();
                    let got: Vec<u64> = a.iter().chain(z.iter()).copied().collect();
                    let want: Vec<u64> = model.iter().take(k).copied().collect();
                    prop_assert_eq!(&got, &want);
                    b.release();
                    for _ in 0..got.len() {
                        model.pop_front();
                    }
                }
                Op::Hints => {}
            }
            prop_assert!(p.free_hint() <= cap - model.len());
            prop_assert!(c.len_hint() <= model.len());
        }
    }
}
