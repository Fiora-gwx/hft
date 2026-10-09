//! Warmup injection (SDK-180, SDK-183, SDK-210).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::common;

use common::*;
use hft_strategy_sdk::testing::{MockViews, ModelHarness, ev};
use hft_strategy_sdk::*;
use hft_types::{GROUP_NONE, GroupId, Px, Qty, Tsc};
use proptest::prelude::{ProptestConfig, TestCaseError};
use proptest::{prop_assert_eq, proptest};

fn tick() -> ev::Ev {
    ev::timer(MODEL_TICK, GROUP_NONE, GIX_NONE)
}

/// 64 one-second samples ending 10 s before UTC0.
fn samples64() -> Vec<(i64, i64)> {
    (0..64)
        .map(|k| (UTC0 - (74 - k) * 1_000_000_000, 6_500_000 + 11 * k))
        .collect()
}

#[test]
fn sdk_183_checkpoint_keeps_warmup() {
    let mut v = clocked_views();
    let mut h = ModelHarness::new();
    let mut a = ProbeModel::new(s3(0xFF02), &tp2(3, &[]), &gt3(), &v.refview());
    model(
        &mut h,
        &mut a,
        &ev::warmup(1, 0, true, &warm_chunk(G1, &samples64())),
        &v,
    );
    let c = h.snapshot_all(&a, 65_536);
    let mut b = ProbeModel::new(s3(0xFF02), &tp2(3, &[]), &gt3(), &v.refview());
    h.restore_all(&mut b, &c, true).unwrap();
    assert_eq!(b.groups[0].s.len(), 64);
    v.at_tsc(Tsc(1_000_000_000));
    let oa = model(&mut h, &mut a, &tick(), &v);
    let ob = model(&mut h, &mut b, &tick(), &v);
    assert_eq!(oa[0].0, G1);
    assert_eq!(oa[0], ob[0], "G1's Advice equal byte for byte");
}

#[test]
fn sdk_210_unknown_warmup_version_ignored() {
    let v = clocked_views();
    let mut h = ModelHarness::new();
    let mut m = ProbeModel::new(s3(0xFF02), &tp2(3, &[]), &gt3(), &v.refview());
    let before = h.snapshot_all(&m, 65_536);
    let mut p = warm_chunk(G1, &samples64());
    p[0] = 2;
    model(&mut h, &mut m, &ev::warmup(1, 0, true, &p), &v);
    assert!(m.groups[0].s.is_empty());
    assert_eq!(h.snapshot_all(&m, 65_536), before);
}

/// Live Bbo at second `sec` after UTC0 (TSC = sec × 10^9 in `clocked_views`) for `gix`.
type Live = (u64, u16, i64);

/// Warmup chunk: group index, samples.
type Chunk = (u16, Vec<(i64, i64)>);

/// Path section, raw-sample section and one tick's Advice.
type Final = (Vec<u8>, Vec<u8>, Vec<(GroupId, u16, Vec<u8>)>);

/// Group count, chunks, live samples, (chunk, live position) deliveries.
type Case = (u16, Vec<Chunk>, Vec<Live>, Vec<(usize, usize)>);

fn deliver_live(m: &mut ProbeModel, v: &mut MockViews, h: &mut ModelHarness, l: Live) {
    let (sec, gix, mid) = l;
    v.at_tsc(Tsc(sec * 1_000_000_000));
    let mv = &mut *v;
    mv.bbo(I1, (Px(mid - 50), Qty(1)), (Px(mid + 50), Qty(1)));
    model(h, m, &ev::bbo(I1, gix, v.bbo_of(I1)), v);
}

fn deliver_chunk(
    m: &mut ProbeModel,
    v: &MockViews,
    h: &mut ModelHarness,
    t: &GroupTable,
    no: usize,
    c: &Chunk,
) {
    let e = ev::warmup(1, no as u32, false, &warm_chunk(t.get(c.0).id, &c.1));
    model(h, m, &e, v);
}

/// Final checkpoint and the Advice of one tick after everything.
fn finish(m: &mut ProbeModel, v: &mut MockViews, h: &mut ModelHarness) -> Final {
    v.at_tsc(Tsc(10_000 * 1_000_000_000));
    let a = model(h, m, &tick(), v);
    let c = h.snapshot_all(m, 65_536);
    (c.path, c.body, a)
}

fn check_180(n: u16, chunks: &[Chunk], live: &[Live], order: &[(usize, usize)]) -> Result<(), TestCaseError> {
    let t = table(&[gspec(G1), gspec(G2), gspec(G3), gspec(G4)][..usize::from(n)]);
    let p = tp2(n, &[]);
    // Reference: chunks once in chunk_no order, then the live samples.
    let mut v = clocked_views();
    let mut h = ModelHarness::new();
    let mut r = ProbeModel::new(s3(0xFF02), &p, &t, &v.refview());
    for (no, c) in chunks.iter().enumerate() {
        deliver_chunk(&mut r, &v, &mut h, &t, no, c);
    }
    for l in live {
        deliver_live(&mut r, &mut v, &mut h, *l);
    }
    let want = finish(&mut r, &mut v, &mut h);
    // Shuffled, repeated and interleaved: `order` = (chunk index, live position to insert before).
    let mut v = clocked_views();
    let mut m = ProbeModel::new(s3(0xFF02), &p, &t, &v.refview());
    let mut li = 0usize;
    let mut ord: Vec<(usize, usize)> = order
        .iter()
        .map(|&(c, at)| (c % chunks.len(), at % (live.len() + 1)))
        .collect();
    ord.sort_by_key(|x| x.1);
    let mut oi = 0usize;
    while li < live.len() || oi < ord.len() {
        if oi < ord.len() && ord[oi].1 <= li {
            let c = ord[oi].0;
            deliver_chunk(&mut m, &v, &mut h, &t, c, &chunks[c]);
            oi += 1;
        } else {
            deliver_live(&mut m, &mut v, &mut h, live[li]);
            li += 1;
        }
    }
    // Every chunk at least once.
    for (no, c) in chunks.iter().enumerate() {
        if !ord.iter().any(|x| x.0 == no) {
            deliver_chunk(&mut m, &v, &mut h, &t, no, c);
        }
    }
    let got = finish(&mut m, &mut v, &mut h);
    prop_assert_eq!(got, want);
    Ok(())
}

/// Chunks per group: 1–8 chunks of 1–64 samples on disjoint, consecutive ranges of seconds
/// (relative to UTC0, within −700..600 so live samples collide with injected ones).
fn arb_case() -> impl proptest::strategy::Strategy<Value = Case> {
    use proptest::strategy::Strategy as _;
    (1u16..=4).prop_flat_map(|n| {
        (
            proptest::strategy::Just(n),
            proptest::collection::vec(
                (0..n, proptest::collection::vec(1usize..=64, 1..=8), -700i64..0),
                1..=4,
            ),
            proptest::collection::vec((0u64..600, 0..n, 6_400_000i64..6_600_000), 0..=200),
            proptest::collection::vec((0usize..64, 0usize..=200), 0..=48),
        )
            .prop_map(|(n, groups, mut live, order)| {
                let mut chunks = Vec::new();
                // Ranges of one group never overlap (chunks are independent per group and time).
                let mut end = [i64::MIN; 4];
                for (gix, sizes, start) in groups {
                    let mut s = start.max(end[usize::from(gix)]);
                    for len in sizes {
                        let samples = (0..len as i64)
                            .map(|k| (UTC0 + (s + k) * 1_000_000_000, 6_500_000 + (s + k) * 3))
                            .collect();
                        chunks.push((gix, samples));
                        s += len as i64;
                    }
                    end[usize::from(gix)] = s;
                }
                // Live samples arrive in time order (the clock never goes back).
                live.sort_by_key(|l| l.0);
                (n, chunks, live, order)
            })
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn sdk_180_warmup_idempotent_order_free((n, chunks, live, order) in arb_case()) {
        check_180(n, &chunks, &live, &order)?;
    }
}
