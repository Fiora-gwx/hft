//! BOOK-062: after each non-duplicate TopN snapshot the book equals that snapshot.
mod common;

use common::{lv, t};
use hft_book::{BookParams, L2Book, Outcome};
use hft_types::Side;
use proptest::prelude::*;

type Side20 = Vec<(i64, i64)>;

/// Unique offsets 1..=3W from the mid, qty 0–5 (≈ 1/6 zero), best first.
fn side_levels(mid: i64, sign: i64, raw: &[(i64, i64)]) -> Side20 {
    let mut offs: Vec<(i64, i64)> = raw.to_vec();
    offs.sort_by_key(|x| x.0);
    offs.dedup_by_key(|x| x.0);
    offs.iter().map(|&(o, q)| ((mid + sign * o) * 10, q)).collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn book_062_topn_equals_last_snapshot(
        snaps in prop::collection::vec(
            (
                -40i64..=40,
                prop::collection::vec((1i64..=192, 0i64..=5), 0..=20),
                prop::collection::vec((1i64..=192, 0i64..=5), 0..=20),
                0u32..100,
            ),
            1..=300,
        ),
    ) {
        let p = BookParams { tick: 10, window: 64, ovf_cap: 64, crossed_tolerance: 0 };
        let mut b = L2Book::new(&p);
        let (mut mid, mut next_id) = (100_000i64, 10u64);
        for (dm, rb, ra, r) in &snaps {
            mid += dm;
            // snap_id: 5 % zero, 10 % repeat or step back, otherwise increasing.
            let id = if *r < 5 {
                0
            } else if *r < 15 {
                next_id.saturating_sub(u64::from(*r % 3))
            } else {
                next_id += 1;
                next_id
            };
            let bids = side_levels(mid, -1, rb);
            let asks = side_levels(mid, 1, ra);
            let before = (b.state_hash(), b.last_id());
            let dup = id != 0 && id <= b.last_id();
            let o = t(&mut b, id, &bids, &asks);
            if dup {
                prop_assert_eq!(o, Outcome::DUPLICATE);
                prop_assert_eq!((b.state_hash(), b.last_id()), before);
                continue;
            }
            prop_assert!(o.contains(Outcome::APPLIED));
            let want = |v: &Side20| v.iter().copied().filter(|l| l.1 != 0).collect::<Vec<_>>();
            let got = |s: Side| b.iter(s).map(|l| (l.px.0, l.qty.0)).collect::<Vec<_>>();
            prop_assert_eq!(got(Side::Buy), want(&bids));
            prop_assert_eq!(got(Side::Sell), want(&asks));
            prop_assert_eq!(b.best_bid(), want(&bids).first().map(|&(p, q)| lv(p, q)));
            prop_assert_eq!(b.best_ask(), want(&asks).first().map(|&(p, q)| lv(p, q)));
            prop_assert_eq!(b.last_id(), id);
        }
    }
}
