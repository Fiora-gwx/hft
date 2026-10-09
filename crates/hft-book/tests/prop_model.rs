//! BOOK-060: random snapshots and messages against the `BTreeMap` reference book.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
use crate::common;
use crate::model;
use crate::seqgen;

use common::{batch_hdr, lv, snap_hdr};
use hft_book::{BookParams, L2Book, Outcome};
use hft_event::{L2SnapHdr, Level};
use hft_types::Side;
use model::{ASK, BID, Model};
use proptest::prelude::*;
use seqgen::{Concrete, Op, Stream, TICK};

pub fn apply_book(b: &mut L2Book, c: &Concrete) -> Outcome {
    match c {
        Concrete::Msg { prev, last, lv: v } => {
            let h = batch_hdr(*prev, *last, c.n_bids(), v.len() - c.n_bids());
            let l: Vec<Level> = v.iter().map(|&(_, p, q)| lv(p, q)).collect();
            b.apply_batch(&h, &l)
        }
        Concrete::Snap { id, bids, asks } => {
            let h = snap_hdr(L2SnapHdr::EXCHANGE, *id, bids.len(), asks.len());
            let l: Vec<Level> = bids.iter().chain(asks).map(|&(p, q)| lv(p, q)).collect();
            b.apply_snapshot(&h, &l)
        }
    }
}

pub fn apply_model(m: &mut Model, c: &Concrete) -> Outcome {
    match c {
        Concrete::Msg { prev, last, lv } => m.apply_msg(*prev, *last, lv),
        Concrete::Snap { id, bids, asks } => m.apply_snapshot(1, *id, bids, asks),
    }
}

fn top50(b: &L2Book, s: Side) -> Vec<(i64, i64)> {
    b.iter(s).take(50).map(|l| (l.px.0, l.qty.0)).collect()
}

fn same(b: &L2Book, m: &Model) -> Result<(), TestCaseError> {
    let lvl = |x: Option<Level>| x.map(|l| (l.px.0, l.qty.0));
    let mb = m.best(BID).map(|(i, q)| (i * TICK, q));
    let ma = m.best(ASK).map(|(i, q)| (i * TICK, q));
    prop_assert_eq!(lvl(b.best_bid()), mb);
    prop_assert_eq!(lvl(b.best_ask()), ma);
    let rb: Vec<_> = m.levels(BID).into_iter().take(50).collect();
    let ra: Vec<_> = m.levels(ASK).into_iter().take(50).collect();
    prop_assert_eq!(top50(b, Side::Buy), rb);
    prop_assert_eq!(top50(b, Side::Sell), ra);
    prop_assert_eq!(b.last_id(), m.last_id);
    prop_assert_eq!(b.state() as u8, m.state);
    prop_assert_eq!(b.meta().truncated, m.truncated);
    prop_assert_eq!(b.meta().base, m.base);
    Ok(())
}

fn params() -> impl Strategy<Value = BookParams> {
    (
        prop::sample::select(vec![64u32, 128, 2048]),
        prop::sample::select(vec![1u16, 4, 64]),
        prop::sample::select(vec![0u16, 3, u16::MAX]),
    )
        .prop_map(|(window, ovf_cap, crossed_tolerance)| BookParams {
            tick: TICK,
            window,
            ovf_cap,
            crossed_tolerance,
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn book_060_matches_btreemap_model(
        p in params(),
        first in (prop::collection::vec((any::<u32>(), 1i64..=5), 1..=20),
                  prop::collection::vec((any::<u32>(), 1i64..=5), 1..=20)),
        ops in prop::collection::vec(seqgen::op(20, 0, 10), 1..2000),
    ) {
        let mut b = L2Book::new(&p);
        let mut m = Model::new(&p);
        let mut st = Stream::new(p.window);
        let c = st.snap(&first.0, &first.1);
        prop_assert_eq!(apply_book(&mut b, &c), apply_model(&mut m, &c));
        same(&b, &m)?;
        for (k, op) in ops.iter().enumerate() {
            // Every 16th message leaves crossing levels in place (crossed / Stale coverage).
            let keep = k % 16 == 15;
            let c = st.next_del(op, 4, |mid| if keep { (vec![], vec![]) } else { m.crossing(mid) });
            let (ob, om) = (apply_book(&mut b, &c), apply_model(&mut m, &c));
            prop_assert_eq!(ob, om, "step {}: {:?}", k, c);
            same(&b, &m)?;
        }
        let _ = Op::Snap { bids: vec![], asks: vec![] };
    }
}
