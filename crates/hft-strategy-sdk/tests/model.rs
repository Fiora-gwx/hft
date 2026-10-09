//! Model constants, declarations and outputs (SDK-030, SDK-032, SDK-063, SDK-064, SDK-178).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod common;

use common::alloc::{Counting, METER};
use common::*;
use hft_strategy_sdk::testing::{MockViews, ModelHarness, assert_within_declaration, ev};
use hft_strategy_sdk::*;
use hft_types::{GROUP_NONE, GroupId, Px, Qty, Tsc};

#[global_allocator]
static A: Counting = Counting;

fn tick() -> ev::Ev {
    ev::timer(MODEL_TICK, GROUP_NONE, GIX_NONE)
}

/// Sets I1's quote around `mid` and delivers it to group `gix` at the current time.
fn bbo<M: Model>(h: &mut ModelHarness, m: &mut M, v: &mut MockViews, gix: u16, mid: i64) {
    v.bbo(
        I1,
        (Px(mid - 50), Qty(100_000_000)),
        (Px(mid + 50), Qty(100_000_000)),
    );
    model(h, m, &ev::bbo(I1, gix, v.bbo_of(I1)), v);
}

#[test]
fn sdk_030_no_model_is_empty() {
    type N = NoModel<TPParams>;
    let p = tp(8, &[]);
    assert_eq!(
        (
            N::LAYOUT_VERSION,
            N::PATH_VERSION,
            N::WARMUP,
            N::LAYOUT.sections.len()
        ),
        (0, 0, false, 0)
    );
    assert_eq!(
        (
            N::mem_bytes(&p, 8),
            N::snapshot_bytes(&p, 8),
            N::path_settle_ms(&p),
            N::cold_start_ms(&p, 0)
        ),
        (0, 0, 0, 0)
    );
    let mut v = clocked_views();
    let mut m = N::new(s3(0xFF01), &p, &gt3(), &v.refview());
    let mut h = ModelHarness::new();
    let c = h.snapshot_all(&m, 65_536);
    assert_eq!((c.path.len(), c.body.len(), c.n_chunks), (0, 0, 0), "step 2");
    let m_ = &mut v;
    m_.bbo(I1, (Px(6_500_000), Qty(1)), (Px(6_500_100), Qty(1)));
    assert!(
        model(&mut h, &mut m, &ev::bbo(I1, 0, v.bbo_of(I1)), &v).is_empty(),
        "step 3 bbo"
    );
    assert!(
        model(&mut h, &mut m, &ev::warmup(1, 0, true, &[1]), &v).is_empty(),
        "step 3 warmup"
    );
}

#[test]
fn sdk_032_model_on_params_idempotent() {
    let mut v = clocked_views();
    let p = tp2(3, &[]);
    let t = gt3();
    let mut m = ProbeModel::new(s3(0xFF02), &p, &t, &v.refview());
    let mut h = ModelHarness::new();
    for sec in 1..=100u64 {
        v.at_tsc(Tsc(sec * 1_000_000_000));
        for gix in 0..3 {
            bbo(&mut h, &mut m, &mut v, gix, mid_at(gix, sec));
        }
    }
    model(&mut h, &mut m, &tick(), &v);
    assert!(m.groups.iter().all(|g| g.s.len() == 100));
    let before = h.snapshot_all(&m, 65_536);
    for i in 0..2 {
        assert_eq!(
            m.on_params(&p, &t, &v.refview()),
            ModelParamEffect::None,
            "call {i}"
        );
        let c = h.snapshot_all(&m, 65_536);
        assert_eq!((&c.path, &c.body), (&before.path, &before.body), "call {i}");
    }
}

/// SDK-063's script: 8 groups, 256 warmup samples each, then 300 live Bbos per group (one per
/// second); returns the checkpoint.
fn full_window<M: Model<Params = TP2Params>>(
    h: &mut ModelHarness,
) -> (TP2Params, hft_strategy_sdk::testing::Ckpt) {
    let specs: Vec<_> = (1..=8u32).map(|n| gspec(GroupId(0x0300_0000 + n))).collect();
    let t = GroupTable::new(&specs, 8);
    let p = tp2(8, &[]);
    let mut v = clocked_views();
    let mut m = h.track(|| M::new(s3(0xFF02), &p, &t, &v.refview()));
    for (gix, g) in t.iter() {
        let samples: Vec<(i64, i64)> = (0..256)
            .map(|k| (UTC0 - (300 - k) * 1_000_000_000, 6_500_000 + i64::from(gix) + k))
            .collect();
        let e = ev::warmup(1, u32::from(gix), gix == 7, &warm_chunk(g.id, &samples));
        model(h, &mut m, &e, &v);
    }
    for sec in 1..=300u64 {
        v.at_tsc(Tsc(sec * 1_000_000_000));
        for gix in 0..8 {
            bbo(h, &mut m, &mut v, gix, 6_500_000 + sec as i64);
        }
        model(h, &mut m, &tick(), &v);
    }
    let c = h.snapshot_all(&m, 65_536);
    (p, c)
}

#[test]
fn sdk_063_model_alloc_within_declaration() {
    alloc::start();
    let mut h = ModelHarness::with_meter(METER);
    let (p, c) = full_window::<ProbeModel>(&mut h);
    alloc::stop();
    assert!(h.peak_alloc_bytes() > 0, "the meter saw the Model's heap");
    assert_eq!(ProbeModel::mem_bytes(&p, 8), 131_072);
    assert_eq!(ProbeModel::snapshot_bytes(&p, 8), 35_072);
    assert!(h.peak_alloc_bytes() <= 131_072, "peak {}", h.peak_alloc_bytes());
    assert!(c.total() <= 35_072, "ckpt {}", c.total());
    assert_within_declaration::<ProbeModel>(&h, &p, 8, &c);
}

#[test]
#[should_panic(expected = "mem_bytes")]
fn sdk_064_under_declared_model_fails() {
    alloc::start();
    let mut h = ModelHarness::with_meter(METER);
    let (p, c) = full_window::<UnderModel>(&mut h);
    alloc::stop();
    assert_within_declaration::<UnderModel>(&h, &p, 8, &c);
}

#[test]
fn sdk_178_nan_not_output() {
    let mut v = clocked_views();
    let t = table(&[gspec(G1), gspec(G2)]);
    let p = tp2(2, &[]);
    let mut m = ProbeModel::new(s3(0xFF02), &p, &t, &v.refview());
    let mut h = ModelHarness::new();
    let mids: Vec<i64> = (0..10).map(|k| 6_500_000 + 3 * k + (k % 2)).collect();
    for (k, mid) in mids.iter().enumerate() {
        v.at_tsc(Tsc((k as u64 + 1) * 1_000_000_000));
        bbo(&mut h, &mut m, &mut v, 1, *mid);
    }
    let out = model(&mut h, &mut m, &tick(), &v);
    assert_eq!(out.len(), 2);
    let a: Vec<(GroupId, BandAdvice)> = out
        .iter()
        .map(|(g, k, b)| {
            assert_eq!(*k, 1);
            (*g, BandAdvice::decode(b).unwrap())
        })
        .collect();
    assert_eq!(
        a[0],
        (
            G1,
            BandAdvice {
                flags: NOT_READY,
                band: 0
            }
        )
    );
    let sum: i64 = mids.iter().sum();
    let want = model_math::round_even(sum as f64 / 10.0).unwrap();
    assert_eq!(a[1].0, G2);
    assert_eq!(a[1].1.flags & NOT_READY, 0);
    assert_eq!(a[1].1.band, want);
}
