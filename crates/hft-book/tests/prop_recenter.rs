//! BOOK-056: a mid drifting monotonically by 10 × W ticks matches the reference book.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
mod common;
#[path = "model.rs"]
mod model;

use common::{batch_hdr, lv, snap};
use hft_book::{BookParams, L2Book, Outcome};
use hft_event::{L2SnapHdr, Level};
use hft_types::Side;
use model::{ASK, BID, Model};
use proptest::prelude::*;

const TICK: i64 = 10;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn book_056_monotonic_drift_matches_model(
        window in prop::sample::select(vec![64u32, 2048]),
        up in any::<bool>(),
        // Per step: (move 1–5, levels changed 1–10 as (is_ask, offset 1–20, qty 1–5)).
        steps in prop::collection::vec(
            (1i64..=5, prop::collection::vec((any::<bool>(), 1i64..=20, 1i64..=5), 1..=10)),
            1..=64,
        ),
    ) {
        let p = BookParams { tick: TICK, window, ovf_cap: 64, crossed_tolerance: 0 };
        let w = i64::from(window);
        let mut b = L2Book::new(&p);
        let mut m = Model::new(&p);
        let mut mid: i64 = 1_000_000;
        let bids: Vec<(i64, i64)> = (1..=3).map(|k| ((mid - k) * TICK, 1)).collect();
        let asks: Vec<(i64, i64)> = (1..=3).map(|k| ((mid + k) * TICK, 1)).collect();
        prop_assert_eq!(
            snap(&mut b, L2SnapHdr::EXCHANGE, 100, &bids, &asks),
            m.apply_snapshot(1, 100, &bids, &asks)
        );
        let mut id = 100u64;
        let (mut moved, mut recentered, mut k) = (0i64, 0u32, 0usize);
        while moved < 10 * w {
            let (dm, changes) = &steps[k % steps.len()];
            k += 1;
            mid += if up { *dm } else { -*dm };
            moved += dm;
            // Keep three levels per side next to the mid, delete what would cross it,
            // add the step's random changes behind the inner levels and trim far levels.
            let mut v: Vec<(usize, i64, i64)> = Vec::new();
            for s in [BID, ASK] {
                let sign = if s == BID { -1 } else { 1 };
                for o in 1..=3 {
                    v.push((s, (mid + sign * o) * TICK, 1));
                }
                for &(ask, off, q) in changes {
                    if usize::from(ask) == s {
                        v.push((s, (mid + sign * (3 + off)) * TICK, q));
                    }
                }
            }
            let (db, da) = m.crossing(mid);
            v.extend(db.into_iter().map(|px| (BID, px, 0)));
            v.extend(da.into_iter().map(|px| (ASK, px, 0)));
            for s in [BID, ASK] {
                let lv = m.levels(s);
                for &(px, _) in lv.iter().skip(40) {
                    v.push((s, px, 0));
                }
            }
            v.sort_by_key(|l| l.0);
            let nb = v.iter().filter(|l| l.0 == BID).count();
            let levels: Vec<Level> = v.iter().map(|&(_, px, q)| lv(px, q)).collect();
            let ob = b.apply_batch(&batch_hdr(id, id + 1, nb, v.len() - nb), &levels);
            let om = m.apply_msg(id, id + 1, &v);
            id += 1;
            prop_assert_eq!(ob, om);
            if ob.contains(Outcome::RECENTERED) {
                recentered += 1;
            }
            if m.truncated == 0 {
                prop_assert!(!ob.contains(Outcome::TRUNCATED));
            }
            let top = |s: Side| b.iter(s).take(50).map(|l| (l.px.0, l.qty.0)).collect::<Vec<_>>();
            prop_assert_eq!(top(Side::Buy), m.levels(BID).into_iter().take(50).collect::<Vec<_>>());
            prop_assert_eq!(top(Side::Sell), m.levels(ASK).into_iter().take(50).collect::<Vec<_>>());
            prop_assert_eq!(b.last_id(), m.last_id);
        }
        prop_assert!(recentered >= 9, "recentered {}", recentered);
    }
}
