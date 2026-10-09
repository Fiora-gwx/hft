//! BOOK-048: the same messages fed as `L2Batch` and as `L2Delta` levels give the same book.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
use crate::common;
use crate::seqgen;

use common::{batch_hdr, delta, lv, snap};
use hft_book::{BookParams, BookState, L2Book, Outcome};
use hft_event::{L2SnapHdr, Level};
use hft_types::Side;
use proptest::prelude::*;
use seqgen::{Concrete, Stream, TICK};

const UNION: Outcome = Outcome(Outcome::CROSSED.0 | Outcome::RECENTERED.0 | Outcome::TRUNCATED.0);

fn tops(b: &L2Book) -> (Option<Level>, Option<Level>) {
    (b.best_bid(), b.best_ask())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn book_048_batch_equals_deltas(
        window in prop::sample::select(vec![64u32, 256]),
        ovf_cap in prop::sample::select(vec![1u16, 4, 64]),
        tol in prop::sample::select(vec![1u16, u16::MAX]),
        ops in prop::collection::vec(seqgen::op(20, 5, 0), 1..=200),
    ) {
        let p = BookParams { tick: TICK, window, ovf_cap, crossed_tolerance: tol };
        let mut a = L2Book::new(&p);
        let mut st = Stream::new(window);
        let Concrete::Snap { id, bids, asks } =
            st.snap(&[(1, 5), (2, 5), (4, 3)], &[(1, 5), (2, 5), (4, 3)]) else { unreachable!() };
        snap(&mut a, L2SnapHdr::EXCHANGE, id, &bids, &asks);
        let mut b = L2Book::new(&p);
        snap(&mut b, L2SnapHdr::EXCHANGE, id, &bids, &asks);
        let mut broken = false;
        for op in &ops {
            let Concrete::Msg { prev, last, lv: v } = st.next(op, 3) else { continue };
            let nb = v.iter().filter(|l| l.0 == 0).count();
            let levels: Vec<Level> = v.iter().map(|&(_, px, q)| lv(px, q)).collect();
            let illegal = v.iter().any(|&(_, px, q)| px <= 0 || q < 0 || px % TICK != 0);
            let before = tops(&a);
            let oa = a.apply_batch(&batch_hdr(prev, last, nb, v.len() - nb), &levels);
            let mut ob = Vec::with_capacity(v.len());
            for (k, &(s, px, q)) in v.iter().enumerate() {
                let side = if s == 0 { Side::Buy } else { Side::Sell };
                ob.push(b.apply_delta(&delta(prev, last, side, px, q, k as u16, k + 1 == v.len())));
            }
            if broken {
                prop_assert_eq!(oa, Outcome::NOT_LIVE);
                prop_assert!(ob.iter().all(|o| *o == Outcome::NOT_LIVE));
                continue;
            }
            if illegal {
                // Batch path: nothing written; delta path: Stale at the illegal level.
                prop_assert_eq!(oa, Outcome::INVALID | Outcome::WENT_STALE);
                prop_assert_eq!(tops(&a), before);
                prop_assert!(ob.contains(&(Outcome::INVALID | Outcome::WENT_STALE)));
                prop_assert_eq!(a.state(), BookState::Stale);
                prop_assert_eq!(b.state(), BookState::Stale);
                broken = true;
                continue;
            }
            let union = ob.iter().fold(Outcome::NONE, |u, o| u | *o);
            prop_assert_eq!(oa & UNION, union & UNION);
            prop_assert_eq!(oa.contains(Outcome::BID_TOP), before.0 != a.best_bid());
            prop_assert_eq!(oa.contains(Outcome::ASK_TOP), before.1 != a.best_ask());
            prop_assert_eq!(a.state_hash(), b.state_hash());
            prop_assert_eq!(a.last_id(), b.last_id());
            prop_assert_eq!(a.state(), b.state());
            prop_assert_eq!(a.meta(), b.meta());
            if a.state() != BookState::Live {
                broken = true;
            }
        }
    }
}
