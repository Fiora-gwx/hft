//! Model checkpoints (SDK-068, SDK-070–SDK-073, SDK-075–SDK-080, SDK-192).
//!
//! Goldens: `fixtures/strategies/sdk-probe/ckpt/{layout.json, path.bin, meta.toml}`;
//! `HFT_REGEN_GOLDEN=1` rewrites them (and `meta.toml`'s versions).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod common;

use std::path::PathBuf;

use common::*;
use hft_strategy_sdk::snapshot::{PutTy, WriteRec};
use hft_strategy_sdk::testing::{Ckpt, MockViews, ModelHarness, check_layout, ev, layout_json};
use hft_strategy_sdk::*;
use hft_types::{GROUP_NONE, GroupId, Px, Qty, Tsc};
use proptest::prelude::{ProptestConfig, TestCaseError};
use proptest::{prop_assert, prop_assert_eq, proptest};

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/strategies/sdk-probe/ckpt")
}

fn regen() -> bool {
    std::env::var_os("HFT_REGEN_GOLDEN").is_some()
}

fn tick() -> ev::Ev {
    ev::timer(MODEL_TICK, GROUP_NONE, GIX_NONE)
}

fn n_groups(n: u16) -> GroupTable {
    let specs: Vec<_> = (1..=u32::from(n)).map(|k| gspec(GroupId(0x0300_0000 + k))).collect();
    GroupTable::new(&specs, 8)
}

/// A Model over `n` groups with `samples` one-second samples each (a tick every second).
fn filled<M: Model<Params = TP2Params>>(n: u16, samples: u64, v: &mut MockViews, h: &mut ModelHarness) -> M {
    let mut m = M::new(s3(0xFF02), &tp2(n, &[]), &n_groups(n), &v.refview());
    for sec in 1..=samples {
        script_second(&mut m, v, h, n, sec);
    }
    m
}

#[test]
fn sdk_068_restore_path_err_drops_path_only() {
    let mut v = clocked_views();
    let mut h = ModelHarness::new();
    let m: ProbeModel = filled(3, 200, &mut v, &mut h);
    let c = h.snapshot_all(&m, 65_536);
    let mut cut = c.clone();
    cut.path.pop();
    let p = tp2(3, &[]);
    let mut m1 = ProbeModel::new(s3(0xFF02), &p, &n_groups(3), &v.refview());
    let r = h.restore_all(&mut m1, &cut, true);
    assert!(r.is_err(), "step 1: {r:?}");
    assert!(m1.groups.iter().all(|g| g.s.len() == 200), "raw samples restored");
    let mut m2 = ProbeModel::new(s3(0xFF02), &p, &n_groups(3), &v.refview());
    h.restore_all(&mut m2, &c, false).unwrap();
    let (a, b) = (h.snapshot_all(&m1, 65_536), h.snapshot_all(&m2, 65_536));
    assert_eq!((a.path, a.body), (b.path, b.body));
}

/// Recorded raw-sample bytes: (instance section S, one full group record R).
fn s_and_r(m: &ProbeModel) -> (usize, usize) {
    let mut w = SnapshotWriter::recording(1 << 20);
    m.snapshot_chunk(0, &mut w, u32::MAX);
    let mut sizes = Vec::new();
    for r in w.records() {
        match *r {
            WriteRec::Section(_) => sizes.push(0usize),
            WriteRec::Put(_, n) => *sizes.last_mut().unwrap() += n,
        }
    }
    (sizes[0], sizes[1])
}

#[test]
fn sdk_070_path_first_then_group_chunks() {
    let mut v = clocked_views();
    let mut h = ModelHarness::new();
    let m: ProbeModel = filled(8, 256, &mut v, &mut h);
    let (s, r) = s_and_r(&m);
    assert!(s < r, "S {s} < R {r}");
    let c = h.snapshot_all(&m, 3 * r as u32);
    assert_eq!(c.path.len(), 4 + 8 * 32, "snapshot_path first (path section bytes)");
    let calls: Vec<(u16, SnapshotProgress, u32)> = c.chunks.clone();
    let r32 = r as u32;
    assert_eq!(
        calls,
        vec![
            (0, SnapshotProgress::More(2), (s + 2 * r) as u32),
            (2, SnapshotProgress::More(5), 3 * r32),
            (5, SnapshotProgress::Done, 3 * r32),
        ]
    );
    assert_eq!(c.n_chunks, 3);
    assert!(calls.iter().all(|x| x.2 <= 3 * r32));
}

#[test]
fn sdk_071_oversized_group_own_chunk() {
    let mut v = clocked_views();
    let mut h = ModelHarness::new();
    let m: ProbeModel = filled(8, 256, &mut v, &mut h);
    let (s, r) = s_and_r(&m);
    // GroupId u32 + 2 × (u32 count + 256 × i64) + pct_bin i64 + fp u64.
    assert_eq!(r, 4 + 2 * (4 + 256 * 8) + 8 + 8);
    let c = h.snapshot_all(&m, 1_000);
    assert_eq!(c.n_chunks, 8);
    for (gix, (from, p, n)) in c.chunks.iter().enumerate() {
        let gix = gix as u16;
        assert_eq!(*from, gix);
        let want = if gix == 7 { SnapshotProgress::Done } else { SnapshotProgress::More(gix + 1) };
        assert_eq!(*p, want);
        assert_eq!(*n as usize, if gix == 0 { s + r } else { r }, "gix {gix} not truncated");
    }
}

#[test]
fn sdk_073_no_groups_instance_section_only() {
    let v = clocked_views();
    let mut m = ProbeModel::new(s3(0xFF02), &tp2(0, &[]), &GroupTable::new(&[], 8), &v.refview());
    m.n_ticks = 9;
    let mut w = SnapshotWriter::recording(1 << 20);
    assert_eq!(m.snapshot_chunk(0, &mut w, 65_536), SnapshotProgress::Done);
    assert_eq!(w.as_bytes(), &9u64.to_le_bytes());
    assert_eq!(w.records(), &[WriteRec::Section(1), WriteRec::Put(PutTy::U64, 8)]);
}

#[test]
fn sdk_075_derived_state_recomputed() {
    let mut v = clocked_views();
    let mut h = ModelHarness::new();
    let m: ProbeModel = filled(3, 256, &mut v, &mut h);
    let c = h.snapshot_all(&m, 65_536);
    let mut b = ProbeModelB::new(s3(0xFF02), &tp2(3, &[]), &n_groups(3), &v.refview());
    h.restore_all(&mut b, &c, false).unwrap();
    for (x, y) in m.groups.iter().zip(&b.groups) {
        assert_eq!(x.s, y.s, "samples restored one by one");
    }
    v.at_tsc(Tsc(257 * 1_000_000_000));
    let out = model(&mut h, &mut b, &tick(), &v);
    assert_eq!(out.len(), 3);
    for ((g, _, p), mg) in out.iter().zip(&m.groups) {
        let mut mids: Vec<i64> = mg.s.iter().map(|x| x.1).collect();
        mids.sort_unstable();
        let median = model_math::round_even((mids[127] as f64 + mids[128] as f64) / 2.0).unwrap();
        let a = BandAdvice::decode(p).unwrap();
        assert_eq!((*g, a.band), (mg.id, median));
        assert_ne!(median, ProbeModel::stat(&mg.s).unwrap(), "median differs from the mean here");
    }
}

/// Reads `meta.toml`'s `key` (0 when absent).
fn meta_version(key: &str) -> u32 {
    std::fs::read_to_string(golden_dir().join("meta.toml"))
        .ok()
        .and_then(|s| s.parse::<toml::Table>().ok())
        .and_then(|t| t.get(key).and_then(toml::Value::as_integer))
        .map_or(0, |v| v as u32)
}

fn write_meta() {
    let s = format!(
        "# Versions the goldens in this directory were generated at (SDK-076, SDK-077).\n\
         # Regenerate: HFT_REGEN_GOLDEN=1 cargo test -p hft-strategy-sdk --test checkpoint\n\
         layout_version = {}\npath_version = {}\n",
        ProbeModel::LAYOUT_VERSION,
        ProbeModel::PATH_VERSION
    );
    std::fs::create_dir_all(golden_dir()).unwrap();
    std::fs::write(golden_dir().join("meta.toml"), s).unwrap();
}

/// Golden check shared by SDK-076/077: bytes differ while the version is the recorded one →
/// "not bumped"; version bumped → "regenerate".
fn golden_vs_version(file: &str, got: &[u8], key: &str, version: u32, what: &str) {
    let p = golden_dir().join(file);
    if regen() {
        std::fs::create_dir_all(golden_dir()).unwrap();
        std::fs::write(&p, got).unwrap();
        write_meta();
        return;
    }
    let want = std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e} (HFT_REGEN_GOLDEN=1 to create)", p.display()));
    let recorded = meta_version(key);
    if want != got {
        if version == recorded {
            panic!("{file} changed but {what} is still {version}: bump {what} and regenerate the golden");
        }
        panic!("{what} bumped {recorded} → {version}: regenerate {file} (HFT_REGEN_GOLDEN=1)");
    }
    assert_eq!(version, recorded, "{file} unchanged but meta.toml records {key} = {recorded}");
}

#[test]
fn sdk_076_layout_golden_requires_bump() {
    let got = layout_json(ProbeModel::LAYOUT);
    golden_vs_version("layout.json", got.as_bytes(), "layout_version", ProbeModel::LAYOUT_VERSION, "LAYOUT_VERSION");
}

#[test]
fn sdk_077_path_golden_requires_bump() {
    let mut v = clocked_views();
    let mut h = ModelHarness::new();
    let mut m = ProbeModel::new(s3(0xFF02), &tp2(3, &[]), &gt3(), &v.refview());
    path_script(&mut m, &mut v, &mut h);
    let mut w = SnapshotWriter::new(1 << 20);
    m.snapshot_path(&mut w);
    golden_vs_version("path.bin", w.as_bytes(), "path_version", ProbeModel::PATH_VERSION, "PATH_VERSION");
}

#[test]
fn sdk_192_layout_json_stable() {
    let got = layout_json(ProbeModel::LAYOUT);
    assert_eq!(got, layout_json(&PROBE_LAYOUT), "same input, same bytes");
    assert_eq!(
        got,
        concat!(
            r#"{"sections":[{"tag":1,"name":"ticks","scope":"Instance","elems":[{"name":"n_ticks","ty":"U64","len":"one"}]},"#,
            r#"{"tag":2,"name":"samples","scope":"Group","elems":[{"name":"utc_ns","ty":"I64","len":"prefixed"},"#,
            r#"{"name":"mid","ty":"I64","len":"prefixed"},{"name":"pct_bin","ty":"I64","len":"one"},"#,
            r#"{"name":"fp","ty":"U64","len":"one"}]}]}"#
        )
    );
    if !regen() {
        let want = std::fs::read_to_string(golden_dir().join("layout.json")).unwrap();
        assert_eq!(got, want);
    }
    let fixed = ModelLayoutDesc {
        sections: &[SectionDesc {
            tag: 7,
            name: "q\"\\",
            scope: SectionScope::Group,
            elems: &[ElemDesc { name: "x", ty: ElemTy::F64Bits, len: LenRule::Fixed(3) }],
        }],
    };
    assert_eq!(
        layout_json(&fixed),
        r#"{"sections":[{"tag":7,"name":"q\"\\","scope":"Group","elems":[{"name":"x","ty":"F64Bits","len":{"fixed":3}}]}]}"#
    );
}

fn layout_of<const K: u8>(v: &mut MockViews) -> Result<(), testing::LayoutMismatch> {
    let mut h = ModelHarness::new();
    let m: ProbeModelT<K> = filled(3, 10, v, &mut h);
    check_layout(&m)
}

#[test]
fn sdk_078_check_layout() {
    let mut v = clocked_views();
    assert_eq!(layout_of::<K_PROBE>(&mut v), Ok(()), "SDK-078.1");
    for (row, r) in [
        ("SDK-078.2", layout_of::<K_LAYOUT_EXTRA_U64>(&mut clocked_views())),
        ("SDK-078.3", layout_of::<K_LAYOUT_MID_I32>(&mut clocked_views())),
        ("SDK-078.4", layout_of::<K_LAYOUT_NO_GID>(&mut clocked_views())),
        ("SDK-078.5", layout_of::<K_LAYOUT_TAG3>(&mut clocked_views())),
        ("SDK-078.6", layout_of::<K_LAYOUT_BAD_COUNT>(&mut clocked_views())),
    ] {
        assert!(r.is_err(), "{row}: {r:?}");
    }
}

/// One script step of SDK-079/080.
#[derive(Clone, Copy, Debug)]
enum Op {
    /// Advance `dt_ms`, then a Bbo of `gix` around `mid`.
    Bbo { gix: u16, dt_ms: u32, mid: i64 },
    /// Advance `dt_ms`, then a model tick.
    Tick { dt_ms: u32 },
    /// A warmup chunk of `n` samples for `gix`, `back_s` seconds before now.
    Warm { gix: u16, n: u16, back_s: u32 },
    /// `on_params` with a new `open_th` default (non-structural).
    Params { open_th: i64 },
    /// Downtime: engine time stays, UTC advances `down_s`.
    Down { down_s: u32 },
}

fn arb_op(n: u16) -> impl proptest::strategy::Strategy<Value = Op> {
    use proptest::strategy::Strategy as _;
    proptest::prop_oneof![
        60 => (0..n, 0u32..3_000, 6_400_000i64..6_600_000).prop_map(|(gix, dt_ms, mid)| Op::Bbo { gix, dt_ms, mid }),
        30 => (0u32..3_000).prop_map(|dt_ms| Op::Tick { dt_ms }),
        5 => (0..n, 1u16..40, 0u32..600).prop_map(|(gix, n, back_s)| Op::Warm { gix, n, back_s }),
        3 => (0i64..100_000_000).prop_map(|open_th| Op::Params { open_th }),
    ]
}

/// Replays `ops` on fresh views from a fresh (or restored) Model; collects every Advice with the
/// engine time of its record, then the final checkpoint.
struct Run {
    advice: Vec<(usize, i64, Vec<(GroupId, u16, Vec<u8>)>)>,
    ckpt: Ckpt,
}

fn apply(m: &mut ProbeModel, v: &mut MockViews, h: &mut ModelHarness, t: &GroupTable, op: Op, boot: &mut u64) -> Option<Vec<(GroupId, u16, Vec<u8>)>> {
    let adv = |v: &mut MockViews, ms: u32| {
        let n = Views::now(v);
        v.at_tsc(Tsc(n.tsc.0 + u64::from(ms) * 1_000_000));
    };
    match op {
        Op::Bbo { gix, dt_ms, mid } => {
            adv(v, dt_ms);
            let mv = &mut *v;
            mv.bbo(I1, (Px(mid - 50), Qty(1)), (Px(mid + 50), Qty(1)));
            model(h, m, &ev::bbo(I1, gix, v.bbo_of(I1)), v);
            None
        }
        Op::Tick { dt_ms } => {
            adv(v, dt_ms);
            Some(model(h, m, &tick(), v))
        }
        Op::Warm { gix, n, back_s } => {
            let now = Views::now(v).utc_ns.div_euclid(1_000_000_000) * 1_000_000_000;
            let base = now - i64::from(back_s) * 1_000_000_000;
            let samples: Vec<(i64, i64)> =
                (0..i64::from(n)).map(|k| (base - k * 1_000_000_000, 6_500_000 + k * 7)).collect();
            let id = t.get(gix).id;
            model(h, m, &ev::warmup(1, 0, true, &warm_chunk(id, &samples)), v);
            None
        }
        Op::Params { open_th } => {
            let p = TP2Params::from_raw(&raw({
                let mut d = tp_defaults();
                d[0] = open_th;
                d
            }, &[]), t.len());
            assert_eq!(m.on_params(&p, t, &v.refview()), ModelParamEffect::None);
            None
        }
        Op::Down { down_s } => {
            *boot += 1_000;
            let n = Views::now(v);
            v.restart_after(Tsc(n.tsc.0 + 13), *boot, i64::from(down_s) * 1_000_000_000);
            None
        }
    }
}

/// Runs `ops`; when `cut = Some((k, max_bytes, keep_path))`, checkpoints after `k` ops and
/// continues on a restored instance. Also returns E0 (engine time at the checkpoint).
fn run(n: u16, ops: &[Op], cut: Option<(usize, u32, bool)>) -> (Run, i64) {
    let t = n_groups(n);
    let mut v = clocked_views();
    let mut h = ModelHarness::new();
    let mut boot = 1u64;
    let mut m = ProbeModel::new(s3(0xFF02), &tp2(n, &[]), &t, &v.refview());
    let mut out = Run { advice: Vec::new(), ckpt: Ckpt::default() };
    let mut e0 = Views::now(&v).engine_ns;
    for (i, op) in ops.iter().enumerate() {
        if let Some((k, max, keep)) = cut
            && i == k
        {
            e0 = Views::now(&v).engine_ns;
            let c = h.snapshot_all(&m, max);
            let mut fresh = ProbeModel::new(s3(0xFF02), &tp2(n, &[]), &t, &v.refview());
            // The current parameters (non-structural changes only) re-applied after restore.
            h.restore_all(&mut fresh, &c, keep).unwrap();
            m = fresh;
        }
        if let Some(a) = apply(&mut m, &mut v, &mut h, &t, *op, &mut boot) {
            out.advice.push((i, Views::now(&v).engine_ns, a));
        }
    }
    out.ckpt = h.snapshot_all(&m, 65_536);
    (out, e0)
}

fn script(downs: bool) -> impl proptest::strategy::Strategy<Value = (u16, Vec<Op>, proptest::sample::Index, u32)> {
    use proptest::sample::Index;
    use proptest::strategy::Strategy as _;
    (1u16..=8).prop_flat_map(move |n| {
        (
            proptest::strategy::Just(n),
            proptest::collection::vec(arb_op(n), 100..=5_000),
            proptest::collection::vec((proptest::prelude::any::<Index>(), 1u32..=7_200), if downs { 0..=3 } else { 0..=0 }),
            proptest::prelude::any::<Index>(),
            1u32..=65_536,
        )
            .prop_map(|(n, mut ops, d, k, max)| {
                // 0–3 downtime segments (engine time stops, UTC advances 1 s – 2 h).
                for (at, down_s) in d {
                    let i = at.index(ops.len() + 1);
                    ops.insert(i, Op::Down { down_s });
                }
                (n, ops, k, max)
            })
    })
}

fn check_079(n: u16, ops: &[Op], k: usize, max: u32) -> Result<(), TestCaseError> {
    let (a, _) = run(n, ops, None);
    let (b, _) = run(n, ops, None);
    prop_assert!(a.advice == b.advice, "two continuous runs differ");
    let (c, _) = run(n, ops, Some((k, max, true)));
    let after: Vec<_> = a.advice.iter().filter(|x| x.0 >= k).collect();
    let after_c: Vec<_> = c.advice.iter().filter(|x| x.0 >= k).collect();
    prop_assert!(after == after_c, "advice after the checkpoint differs");
    prop_assert_eq!(&a.ckpt.path, &c.ckpt.path);
    prop_assert_eq!(&a.ckpt.body, &c.ckpt.body);
    Ok(())
}

fn check_080(n: u16, ops: &[Op], k: usize, max: u32) -> Result<(), TestCaseError> {
    let (a, _) = run(n, ops, None);
    let (c, e0) = run(n, ops, Some((k, max, false)));
    let settle = 60_000i64 * 1_000_000;
    let late = |r: &Run| -> Vec<(usize, i64, Vec<(GroupId, u16, Vec<u8>)>)> {
        r.advice.iter().filter(|x| x.0 >= k && x.1 >= e0 + settle).cloned().collect()
    };
    prop_assert!(late(&a) == late(&c), "advice ≥ E0 + path_settle_ms differs");
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn sdk_072_chunking_invariant(n in 1u16..=8, samples in 0u64..=256) {
        let mut v = clocked_views();
        let mut h = ModelHarness::new();
        let m: ProbeModel = filled(n, samples, &mut v, &mut h);
        let reference = h.snapshot_all(&m, u32::MAX);
        let one = 4 + 2 * (4 + samples as u32 * 8) + 16;
        for max in [1, one, 65_536, u32::MAX] {
            let c = h.snapshot_all(&m, max);
            prop_assert_eq!(&c.path, &reference.path);
            prop_assert_eq!(&c.body, &reference.body);
            let mut r = ProbeModel::new(s3(0xFF02), &tp2(n, &[]), &n_groups(n), &v.refview());
            h.restore_all(&mut r, &c, true).unwrap();
            let again = h.snapshot_all(&r, max);
            prop_assert_eq!(&again.path, &reference.path);
            prop_assert_eq!(&again.body, &reference.body);
        }
    }

    #[test]
    fn sdk_079_checkpoint_resume_bytewise((n, ops, k, max) in script(false)) {
        check_079(n, &ops, k.index(ops.len()), max)?;
    }

    #[test]
    fn sdk_080_path_reset_converges_after_settle((n, ops, k, max) in script(true)) {
        check_080(n, &ops, k.index(ops.len()), max)?;
    }
}
