//! Group resets on parameter changes (SDK-081, SDK-082, SDK-083, SDK-084, SDK-087, SDK-088).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod common;

use std::collections::BTreeMap;

use common::*;
use hft_event::GroupSpec;
use hft_strategy_sdk::set::reset_preview_of;
use hft_strategy_sdk::testing::{CtxHarness, MockViews, ModelHarness, ev};
use hft_strategy_sdk::*;
use hft_types::{GroupId, Px, Qty, Tsc};
use proptest::prelude::{ProptestConfig, TestCaseError, any};
use proptest::{prop_assert_eq, proptest};

/// `TP` field indices.
const OPEN_TH: u16 = 0;
const COOLDOWN: u16 = 1;
const PCT_BIN: u16 = 2;

/// TP2 parameters for `n` groups: `defaults` overrides (`(field, value)`) and per-group overrides.
fn params(n: u16, defaults: &[(u16, i64)], overrides: &[(u16, u16, i64)]) -> TP2Params {
    let mut d = tp_defaults();
    for &(f, v) in defaults {
        d[usize::from(f)] = v;
    }
    TP2Params::from_raw(&raw(d, overrides), n)
}

fn old_params() -> TP2Params {
    params(3, &[], &[(1, PCT_BIN, 200_000)])
}

/// G3 with leg 1 = I5 (same GroupId, new fingerprint).
fn g3_leg_i5() -> GroupSpec {
    spec(G3, BTC_ID, &[(I1, V1), (I5, V2)])
}

/// A ProbeModel over GT3 with `old_params()` and 100 samples per group.
fn seeded<M: Model<Params = TP2Params>>(v: &mut MockViews, h: &mut ModelHarness) -> M {
    let mut m = M::new(s3(0xFF02), &old_params(), &gt3(), &v.refview());
    for sec in 1..=100u64 {
        v.at_tsc(Tsc(sec * 1_000_000_000));
        for gix in 0..3u16 {
            let mid = mid_at(gix, sec);
            let mv = &mut *v;
            mv.bbo(I1, (Px(mid - 50), Qty(1)), (Px(mid + 50), Qty(1)));
            model(h, &mut m, &ev::bbo(I1, gix, v.bbo_of(I1)), v);
        }
    }
    m
}

/// Per-GroupId record bytes of the raw-sample section.
fn records<M: Model>(m: &M, n: u16) -> BTreeMap<u32, Vec<u8>> {
    let mut out = BTreeMap::new();
    for gix in 0..n {
        let mut w = SnapshotWriter::new(1 << 20);
        m.snapshot_chunk(gix, &mut w, 1);
        let b = w.as_bytes();
        let b = if gix == 0 { &b[8..] } else { b };
        let id = u32::from_le_bytes(b[..4].try_into().unwrap());
        out.insert(id, b.to_vec());
    }
    out
}

fn resets(cap: u16, rows: &[(u16, ResetCause)]) -> ModelParamEffect {
    let mut r = GroupResets::new(cap);
    for &(g, c) in rows {
        r.set(g, c);
    }
    ModelParamEffect::ResetGroups(r)
}

#[test]
fn sdk_081_model_reset_by_cause() {
    use ResetCause::*;
    let g4 = || gspec(G4);
    let mut g3_asset2 = gspec(G3);
    g3_asset2.asset = ASSET2;
    let rows: Vec<(&str, GroupTable, TP2Params, ModelParamEffect, &[GroupId])> = vec![
        (
            "SDK-081.1",
            table(&[gspec(G1), gspec(G2), gspec(G3), g4()]),
            params(4, &[], &[(1, PCT_BIN, 200_000)]),
            resets(8, &[(3, Fingerprint)]),
            &[G4],
        ),
        (
            "SDK-081.2",
            table(&[gspec(G1), gspec(G2), g3_leg_i5()]),
            old_params(),
            resets(8, &[(2, Fingerprint)]),
            &[G3],
        ),
        (
            "SDK-081.3",
            table(&[gspec(G1), gspec(G2), g3_asset2]),
            old_params(),
            resets(8, &[(2, Fingerprint)]),
            &[G3],
        ),
        (
            "SDK-081.4",
            gt3(),
            params(3, &[], &[(1, PCT_BIN, 300_000)]),
            resets(8, &[(1, Structural)]),
            &[G2],
        ),
        (
            "SDK-081.5",
            gt3(),
            params(3, &[(PCT_BIN, 150_000)], &[(1, PCT_BIN, 200_000)]),
            resets(8, &[(0, Structural), (2, Structural)]),
            &[G1, G3],
        ),
        (
            "SDK-081.6",
            gt3(),
            params(3, &[(PCT_BIN, 150_000)], &[]),
            ModelParamEffect::ResetAll,
            &[G1, G2, G3],
        ),
        (
            "SDK-081.7",
            gt3(),
            params(
                3,
                &[(OPEN_TH, 7_000_000), (COOLDOWN, 30_000)],
                &[(1, PCT_BIN, 200_000)],
            ),
            ModelParamEffect::None,
            &[],
        ),
        (
            "SDK-081.8",
            {
                let mut g1 = gspec(G1);
                g1.open_allowed = 0;
                table(&[g1, gspec(G2), gspec(G3)])
            },
            old_params(),
            ModelParamEffect::None,
            &[],
        ),
        (
            "SDK-081.9",
            table(&[gspec(G3), gspec(G1), gspec(G2)]),
            params(3, &[], &[(2, PCT_BIN, 200_000)]),
            ModelParamEffect::None,
            &[],
        ),
    ];
    for (row, t, p, want, empty) in rows {
        let mut v = clocked_views();
        let mut h = ModelHarness::new();
        let mut m: ProbeModel = seeded(&mut v, &mut h);
        let before = records(&m, 3);
        let path_before = h.snapshot_all(&m, 65_536).path;
        assert_eq!(m.on_params(&p, &t, &v.refview()), want, "{row}");
        let after = records(&m, t.len());
        for (gix, g) in t.iter() {
            let mg = &m.groups[usize::from(gix)];
            assert_eq!(mg.id, g.id, "{row}");
            if empty.contains(&g.id) {
                assert!(mg.s.is_empty(), "{row}: {:?} empty", g.id);
            } else {
                assert_eq!(
                    after.get(&g.id.0),
                    before.get(&g.id.0),
                    "{row}: {:?} bytes unchanged",
                    g.id
                );
            }
        }
        // The path section is in gix order: only rows that keep the order compare it (.7, .8).
        if want == ModelParamEffect::None && row != "SDK-081.9" {
            assert_eq!(
                h.snapshot_all(&m, 65_536).path,
                path_before,
                "{row}: path section unchanged"
            );
        }
    }
}

#[test]
fn sdk_082_pct_config_mismatch() {
    let mut v = clocked_views();
    let mut h = ModelHarness::new();
    // C written with pct_bin = 100_000 for every group.
    let p0 = params(3, &[], &[]);
    let mut m0 = ProbeModel::new(s3(0xFF02), &p0, &gt3(), &v.refview());
    for sec in 1..=20u64 {
        v.at_tsc(Tsc(sec * 1_000_000_000));
        for gix in 0..3u16 {
            let mid = mid_at(gix, sec);
            let mv = &mut v;
            mv.bbo(I1, (Px(mid - 50), Qty(1)), (Px(mid + 50), Qty(1)));
            model(&mut h, &mut m0, &ev::bbo(I1, gix, v.bbo_of(I1)), &v);
        }
    }
    let c = h.snapshot_all(&m0, 65_536);
    let p1 = params(3, &[], &[(1, PCT_BIN, 250_000)]);
    let mut m = ProbeModel::new(s3(0xFF02), &p1, &gt3(), &v.refview());
    h.restore_all(&mut m, &c, true).unwrap();
    assert_eq!(
        m.on_params(&p1, &gt3(), &v.refview()),
        resets(8, &[(1, ResetCause::PctConfig)])
    );
    assert!(m.groups[1].s.is_empty());
    assert_eq!(m.groups[0].s, m0.groups[0].s);
    assert_eq!(m.groups[2].s, m0.groups[2].s);
}

#[test]
fn sdk_083_fast_part_resets_new_and_changed() {
    let mut v = clocked_views();
    let mut s = EchoProbe::new(s3(0xFF01), &tp(3, &[]), &gt3(), &v.refview()).unwrap();
    let mut h = CtxHarness::new(&EchoProbe::DESC);
    for gix in 0..3 {
        let r = fast(&mut h, &mut s, &ev::bbo(I1, gix, quote(6_500_000)), &v);
        assert_eq!(r.places.len(), 1, "gix {gix} fired (cooldown to +20 s)");
    }
    let t = table(&[gspec(G1), gspec(G2), g3_leg_i5(), gspec(G4)]);
    assert_eq!(s.on_params(&tp(4, &[]), &t, &v.refview()), Ok(()), "step 1");
    v.at_engine_ns(ENS0 + 1_000_000_000);
    let mut fired = Vec::new();
    for gix in [0u16, 2, 3] {
        let r = fast(&mut h, &mut s, &ev::bbo(I1, gix, quote(6_500_000)), &v);
        fired.extend(r.places.iter().map(|p| p.group));
    }
    assert_eq!(fired, vec![G3, G4]);
}

#[test]
fn sdk_084_group_resets_min_code() {
    use ResetCause::*;
    let mut r = GroupResets::new(8);
    r.set(5, Fingerprint);
    r.set(1, PctConfig);
    r.set(1, Structural);
    r.set(5, PctConfig);
    assert_eq!(
        (r.get(1), r.get(5), r.get(0)),
        (Some(Structural), Some(Fingerprint), None)
    );
    assert_eq!(
        r.iter().collect::<Vec<_>>(),
        vec![(1, Structural), (5, Fingerprint)]
    );
}

fn preview<const K: u8>(new_g: &GroupTable, new: &TP2Params) -> ResetPreview {
    reset_preview_of::<TargetProbeT<K>>(&old_params(), &gt3(), new, new_g)
}

#[test]
fn sdk_087_reset_preview() {
    let with_g4 = table(&[gspec(G1), gspec(G2), gspec(G3), gspec(G4)]);
    let p4 = params(4, &[], &[(1, PCT_BIN, 200_000)]);
    let pv = |r, n| ResetPreview {
        reset_groups: r,
        no_open_ms: n,
    };
    let non_struct = params(
        3,
        &[(OPEN_TH, 7_000_000), (COOLDOWN, 30_000)],
        &[(1, PCT_BIN, 200_000)],
    );
    let pct = params(3, &[(PCT_BIN, 150_000)], &[(1, PCT_BIN, 200_000)]);
    let swap = table(&[gspec(G1), gspec(G2), g3_leg_i5(), gspec(G4)]);
    let reorder = table(&[gspec(G3), gspec(G1), gspec(G2)]);
    let reorder_p = params(3, &[], &[(2, PCT_BIN, 200_000)]);
    assert_eq!(preview::<K_COLD>(&gt3(), &non_struct), pv(0, 0), "SDK-087.1");
    assert_eq!(preview::<K_COLD>(&with_g4, &p4), pv(1, 400_000), "SDK-087.2");
    assert_eq!(preview::<K_PROBE>(&with_g4, &p4), pv(1, 60_000), "SDK-087.3");
    assert_eq!(preview::<K_COLD>(&gt3(), &pct), pv(2, 300_000), "SDK-087.4");
    assert_eq!(preview::<K_PROBE>(&gt3(), &pct), pv(2, 60_000), "SDK-087.5");
    assert_eq!(preview::<K_COLD>(&swap, &p4), pv(2, 400_000), "SDK-087.6");
    assert_eq!(
        preview::<K_COLD_HUGE>(&with_g4, &p4),
        pv(1, u32::MAX),
        "SDK-087.7"
    );
    assert_eq!(preview::<K_COLD>(&reorder, &reorder_p), pv(0, 0), "SDK-087.8");
}

/// One generated side (old or new): ordered group numbers (1..=12), per-number alternate spec
/// flag and `pct_bin` override, `pct_bin` default, `open_th` default.
#[derive(Clone, Debug)]
struct Side {
    ids: Vec<u32>,
    alt: Vec<bool>,
    ovr: Vec<Option<i64>>,
    pct: i64,
    open_th: i64,
}

fn side() -> impl proptest::strategy::Strategy<Value = Side> {
    use proptest::strategy::Strategy as _;
    (
        proptest::sample::subsequence((1..=12u32).collect::<Vec<_>>(), 1..=8).prop_shuffle(),
        proptest::collection::vec(proptest::bool::weighted(0.2), 13),
        proptest::collection::vec(proptest::option::weighted(0.3, 10_000i64..=1_000_000), 13),
        proptest::bool::weighted(0.3),
        10_000i64..=1_000_000,
        0i64..=100_000_000,
    )
        .prop_map(|(ids, alt, ovr, pct_changed, pct, open_th)| Side {
            ids,
            alt,
            ovr,
            pct: if pct_changed { pct } else { 100_000 },
            open_th,
        })
}

fn build(s: &Side) -> (GroupTable, TP2Params, Vec<(GroupId, u64, i64)>) {
    let specs: Vec<GroupSpec> = s
        .ids
        .iter()
        .map(|&n| {
            let id = GroupId(0x0300_0000 + n);
            if s.alt[n as usize] {
                if n % 2 == 0 {
                    spec(id, BTC_ID, &[(I1, V1), (I5, V2)])
                } else {
                    spec(id, ASSET2, &[(I1, V1), (I2, V2)])
                }
            } else {
                gspec(id)
            }
        })
        .collect();
    let ov: Vec<(u16, u16, i64)> = s
        .ids
        .iter()
        .enumerate()
        .filter_map(|(gix, &n)| s.ovr[n as usize].map(|v| (gix as u16, PCT_BIN, v)))
        .collect();
    let n = s.ids.len() as u16;
    let p = params(n, &[(PCT_BIN, s.pct), (OPEN_TH, s.open_th)], &ov);
    let eff = specs
        .iter()
        .enumerate()
        .map(|(gix, g)| (g.id, g.fingerprint(), p.at(gix as u16).pct_bin.0))
        .collect();
    (GroupTable::new(&specs, 8), p, eff)
}

fn check_088(old: &Side, new: &Side) -> Result<(), TestCaseError> {
    let (og, op, oe) = build(old);
    let (ng, np, ne) = build(new);
    let reference = ne
        .iter()
        .filter(|(id, fp, pct)| {
            !oe.iter()
                .any(|(oid, ofp, opct)| oid == id && ofp == fp && opct == pct)
        })
        .count() as u32;
    let pv = reset_preview_of::<TargetProbe>(&op, &og, &np, &ng);
    prop_assert_eq!(pv.reset_groups, reference);
    let v = gt3_views();
    let mut m = ProbeModel::new(s3(0xFF02), &op, &og, &v.refview());
    let n = match m.on_params(&np, &ng, &v.refview()) {
        ModelParamEffect::None => 0,
        ModelParamEffect::ResetAll => u32::from(ng.len()),
        ModelParamEffect::ResetGroups(r) => r.count() as u32,
    };
    prop_assert_eq!(pv.reset_groups, n);
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    #[test]
    fn sdk_088_preview_matches_on_params(old in side(), new in side(), same in any::<bool>()) {
        // Half the cases reuse the old group order so most GroupIds survive.
        let new = if same { Side { ids: old.ids.clone(), ..new } } else { new };
        check_088(&old, &new)?;
    }
}
