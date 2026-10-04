//! BOOK-061: two merged copies of one delta stream give the single-path book.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
mod common;

use common::{book_s0, delta};
use hft_book::{BookParams, L2Book, Outcome};
use hft_event::L2Delta;
use hft_types::Side;
use proptest::prelude::*;

/// Messages as delta lists, chained from `last_id = 100` (S0).
fn messages(raw: &[Vec<(bool, i64, i64)>]) -> Vec<Vec<L2Delta>> {
    let mut id = 100u64;
    raw.iter()
        .map(|lv| {
            let n = lv.len();
            let m = lv
                .iter()
                .enumerate()
                .map(|(k, &(ask, off, q))| {
                    let (side, px) = if ask {
                        (Side::Sell, 10010 + 10 * off)
                    } else {
                        (Side::Buy, 10000 - 10 * off)
                    };
                    delta(id, id + 1, side, px, q, k as u16, k + 1 == n)
                })
                .collect();
            id += 1;
            m
        })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn book_061_dual_path_dedup(
        raw in prop::collection::vec(prop::collection::vec((any::<bool>(), 0i64..200, 0i64..=5), 1..=10), 1..=500),
        pick in prop::collection::vec(any::<bool>(), 1000),
    ) {
        let p = BookParams { tick: 10, window: 64, ovf_cap: 4, crossed_tolerance: u16::MAX };
        let msgs = messages(&raw);
        let mut single = book_s0(&p);
        for d in msgs.iter().flatten() {
            single.apply_delta(d);
        }
        let mut dual: L2Book = book_s0(&p);
        let (mut ia, mut ib, mut dups, mut k) = (0usize, 0usize, 0usize, 0usize);
        while ia < msgs.len() || ib < msgs.len() {
            let take_a = ib >= msgs.len() || (ia < msgs.len() && pick[k % pick.len()]);
            k += 1;
            let m = if take_a { ia += 1; &msgs[ia - 1] } else { ib += 1; &msgs[ib - 1] };
            for d in m {
                if dual.apply_delta(d) == Outcome::DUPLICATE {
                    dups += 1;
                }
            }
        }
        prop_assert_eq!(dual.state_hash(), single.state_hash());
        prop_assert_eq!(dups, raw.iter().map(Vec::len).sum::<usize>());
    }
}
