//! BOOK-079: restore + keyframe gives a book indistinguishable from the continuously running one.
mod common;
#[path = "seqgen.rs"]
mod seqgen;

use common::{aligned_mem, batch_hdr, lv, refdata, snap_hdr};
use hft_book::{BookSet, BookState, L2Book, Outcome};
use hft_event::{L2SnapHdr, Level};
use hft_strategy_sdk::{SnapshotReader, SnapshotWriter};
use hft_types::{InstId, Side};
use proptest::prelude::*;
use seqgen::{Concrete, Stream, TICK};

const I: InstId = InstId(1);

fn apply(b: &mut L2Book, c: &Concrete) -> Outcome {
    match c {
        Concrete::Msg { prev, last, lv: v } => {
            let nb = c.n_bids();
            let l: Vec<Level> = v.iter().map(|&(_, p, q)| lv(p, q)).collect();
            b.apply_batch(&batch_hdr(*prev, *last, nb, v.len() - nb), &l)
        }
        Concrete::Snap { id, bids, asks } => {
            let h = snap_hdr(L2SnapHdr::EXCHANGE, *id, bids.len(), asks.len());
            let l: Vec<Level> = bids.iter().chain(asks).map(|&(p, q)| lv(p, q)).collect();
            b.apply_snapshot(&h, &l)
        }
    }
}

/// Bid prices at or above `mid` and ask prices at or below it.
fn crossing(b: &L2Book, mid: i64) -> (Vec<i64>, Vec<i64>) {
    let px = mid * TICK;
    let bids = b
        .iter(Side::Buy)
        .map(|l| l.px.0)
        .take_while(|p| *p >= px)
        .collect();
    let asks = b
        .iter(Side::Sell)
        .map(|l| l.px.0)
        .take_while(|p| *p <= px)
        .collect();
    (bids, asks)
}

fn l2(s: &mut BookSet) -> &mut L2Book {
    s.l2_mut(I).unwrap_or_else(|| panic!("no L2 book"))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn book_079_restore_plus_keyframe_is_seamless(
        window in prop::sample::select(vec![64u32, 128]),
        tol in prop::sample::select(vec![0u16, 3, u16::MAX]),
        first in (prop::collection::vec((any::<u32>(), 1i64..=5), 1..=20),
                  prop::collection::vec((any::<u32>(), 1i64..=5), 1..=20)),
        prefix in prop::collection::vec(seqgen::op(20, 0, 10), 0..=500),
        suffix in prop::collection::vec(seqgen::op(20, 0, 10), 0..=500),
    ) {
        let rd = refdata(1, TICK, window, tol);
        let ids = [I];
        let mk = || match BookSet::new_in(rd, &ids, aligned_mem(BookSet::bytes_needed(rd, &ids))) {
            Ok(s) => s,
            Err(e) => panic!("{e:?}"),
        };
        let mut a = mk();
        let mut st = Stream::new(window);
        // No ±L jumps in the prefix: they push whole books into the overflow area (truncation).
        st.jumps = false;
        let c = st.snap(&first.0, &first.1);
        apply(l2(&mut a), &c);
        for op in &prefix {
            let c = st.next_del(op, 1, |mid| a.l2(I).map_or((vec![], vec![]), |x| crossing(x, mid)));
            apply(l2(&mut a), &c);
        }
        let ab = l2(&mut a);
        // Keyframes are exported for Live books only; the property needs no truncation.
        if ab.state() != BookState::Live || ab.meta().truncated != 0 {
            return Ok(());
        }
        let mut out = vec![lv(0, 0); 4096];
        let (nb, na, tr) = ab.export(&mut out);
        prop_assert!(!tr);
        let last = ab.last_id();
        let mut w = SnapshotWriter::new(1 << 16);
        a.snapshot(&mut w);
        let mut b = mk();
        prop_assert_eq!(b.restore(&mut SnapshotReader::new(w.as_bytes())), Ok(()));
        let n = usize::from(nb + na);
        let h = snap_hdr(L2SnapHdr::KEYFRAME, last, usize::from(nb), usize::from(na));
        prop_assert!(l2(&mut b).apply_snapshot(&h, &out[..n]).contains(Outcome::APPLIED));
        prop_assert_eq!(l2(&mut b).state_hash(), l2(&mut a).state_hash());
        prop_assert_eq!(l2(&mut b).meta(), l2(&mut a).meta());
        st.jumps = true;
        for op in &suffix {
            let c = st.next_del(op, 1, |mid| a.l2(I).map_or((vec![], vec![]), |x| crossing(x, mid)));
            let oa = apply(l2(&mut a), &c);
            let ob = apply(l2(&mut b), &c);
            prop_assert_eq!(oa, ob);
            prop_assert_eq!(l2(&mut b).state_hash(), l2(&mut a).state_hash());
            prop_assert_eq!(l2(&mut b).meta(), l2(&mut a).meta());
        }
    }
}
