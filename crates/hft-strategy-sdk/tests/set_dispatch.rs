//! `strategy_set!` dispatch and static queries (SDK-185, SDK-186, SDK-189).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::common;

use common::*;
use hft_strategy_sdk::ctx::SubEntry;
use hft_strategy_sdk::params::{RawParams, encode_body};
use hft_strategy_sdk::set::reset_preview_of;
use hft_strategy_sdk::testing::{CtxHarness, MockViews, ModelHarness, Recorded, ev};
use hft_strategy_sdk::*;
use hft_types::{GROUP_NONE, Tsc};
use proptest::prelude::{ProptestConfig, TestCaseError, any};
use proptest::{prop_assert, prop_assert_eq, proptest};

hft_strategy_sdk::strategy_set! {
    pub struct ProbeSet {
        EchoProbe,
        TargetProbe,
    }
}

use __strategy_set::{Inst, ModelInst, Params};

#[test]
fn sdk_186_static_queries() {
    let k: Vec<u16> = ProbeSet::kinds().iter().map(|d| d.kind_id).collect();
    assert_eq!(k, vec![0xFF01, 0xFF02], "SDK-186.1");
    let ops: Vec<u8> = ProbeSet::cmds(0xFF01).iter().map(|c| c.op).collect();
    assert_eq!(ops, vec![1, 2, 3], "SDK-186.2");
    assert!(ProbeSet::cmds(0xFF02).is_empty(), "SDK-186.3");
    assert!(ProbeSet::cmds(0xFF09).is_empty(), "SDK-186.4");
    assert_eq!(
        ProbeSet::schema_json(0xFF01),
        Some(hft_strategy_sdk::params::schema_json(
            &EchoProbe::DESC,
            &ModelLayoutDesc::EMPTY,
            2
        )),
        "SDK-186.5"
    );
    assert_eq!(ProbeSet::schema_json(0xFF09), None, "SDK-186.6");
    assert_eq!(
        (
            ProbeSet::model_path_version(0xFF02),
            ProbeSet::model_path_version(0xFF01)
        ),
        (1, 0),
        "SDK-186.7"
    );
    assert!(
        core::ptr::eq(ProbeSet::model_layout(0xFF02), ProbeModel::LAYOUT),
        "SDK-186.8"
    );
    assert_eq!(
        ProbeSet::decode_params(0xFF09, &[], 2, &gt3()).err(),
        Some(ParamError::SchemaMismatch { want: 0, got: 0xFF09 }),
        "SDK-186.9"
    );
    let p = tp2(3, &[]);
    assert_eq!(
        ProbeSet::model_mem_bytes(&Params::TargetProbe(p.clone()), 8),
        ProbeModel::mem_bytes(&p, 8),
        "SDK-186.10"
    );
    assert_eq!(ProbeModel::mem_bytes(&p, 8), 131_072);
    assert_eq!(ProbeSet::model_layout_version(0xFF02), ProbeModel::LAYOUT_VERSION);
    assert_eq!(ProbeSet::model_layout_version(0xFF01), 0, "NoModel");
    assert_eq!(ProbeSet::model_layout_version(0xFF09), 0, "unknown kind");
}

/// The macro's expansion template (the `strategy_set!` definition in `src/set_macro.rs`): no
/// trait objects, no boxing; `on_event` is one `match` (`cargo expand` is not in the toolchain,
/// the template is what every expansion is made of).
#[test]
fn sdk_189_no_dyn_in_expansion() {
    let src = include_str!("../src/set_macro.rs");
    let start = src
        .find("macro_rules! strategy_set")
        .expect("strategy_set! definition");
    let body = &src[start..];
    let code: String = body
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(code.matches("dyn ").count(), 0, "`dyn ` in the expansion");
    assert_eq!(code.matches("Box<").count(), 0, "`Box<` in the expansion");
    let on_event = code.find("fn on_event").expect("on_event");
    let next_fn = code[on_event + 1..]
        .find("fn ")
        .map_or(code.len(), |i| on_event + 1 + i);
    assert_eq!(
        code[on_event..next_fn].matches("match ").count(),
        1,
        "on_event is one match"
    );
}

/// Encodes random valid TP values: defaults in range, overrides of per-group fields.
fn raw_params(n: u16, d: &[i64; 6], ov: &[(u16, u16, i64)]) -> RawParams {
    let defaults = TP_FIELDS
        .iter()
        .zip(d)
        .map(|(f, &x)| f.min + x.rem_euclid(f.max - f.min + 1))
        .collect();
    let overrides = ov
        .iter()
        .filter_map(|&(g, f, x)| {
            let fd = TP_FIELDS[usize::from(f % 6)];
            fd.per_group
                .then(|| (g % n, f % 6, fd.min + x.rem_euclid(fd.max - fd.min + 1)))
        })
        .fold(Vec::<(u16, u16, i64)>::new(), |mut acc, o| {
            // One value per (gix, field): a repeated key with another value is an encoding error.
            if !acc.iter().any(|x| (x.0, x.1) == (o.0, o.1)) {
                acc.push(o);
            }
            acc
        });
    raw(defaults, &overrides)
}

fn rec_eq(a: &Recorded, b: &Recorded) -> Result<(), TestCaseError> {
    prop_assert_eq!(a, b);
    Ok(())
}

fn subs<S: Strategy>(s: &S, g: &GroupTable, v: &MockViews) -> (Vec<SubEntry>, Option<u32>) {
    let room = vec![8u16; 200];
    let lim = SubsLimits {
        rd: &v.refdata().rd,
        feeds: FEEDS,
        room: &room,
    };
    let mut b = SubsBuilder::new(lim, 64);
    s.subscriptions(g, &mut b);
    (b.entries().to_vec(), b.model_tick_ms())
}

fn set_subs(s: &Inst, g: &GroupTable, v: &MockViews) -> (Vec<SubEntry>, Option<u32>) {
    let room = vec![8u16; 200];
    let lim = SubsLimits {
        rd: &v.refdata().rd,
        feeds: FEEDS,
        room: &room,
    };
    let mut b = SubsBuilder::new(lim, 64);
    ProbeSet::subscriptions(s, g, &mut b);
    (b.entries().to_vec(), b.model_tick_ms())
}

fn snap_set(s: &Inst) -> Vec<u8> {
    let mut w = SnapshotWriter::new(262_144);
    ProbeSet::snapshot(s, &mut w);
    w.as_bytes().to_vec()
}

fn snap<S: Strategy>(s: &S) -> Vec<u8> {
    let mut w = SnapshotWriter::new(262_144);
    s.snapshot(&mut w);
    w.as_bytes().to_vec()
}

fn model_bytes_set(m: &ModelInst) -> (Vec<u8>, Vec<u8>) {
    let mut w = SnapshotWriter::new(1 << 20);
    ProbeSet::model_snapshot_path(m, &mut w);
    let path = w.as_bytes().to_vec();
    let mut w = SnapshotWriter::new(1 << 20);
    let mut from = 0;
    while let SnapshotProgress::More(n) = ProbeSet::model_snapshot_chunk(m, from, &mut w, 4_096) {
        from = n;
    }
    (path, w.as_bytes().to_vec())
}

fn model_bytes<M: Model>(m: &M) -> (Vec<u8>, Vec<u8>) {
    let mut w = SnapshotWriter::new(1 << 20);
    m.snapshot_path(&mut w);
    let path = w.as_bytes().to_vec();
    let mut w = SnapshotWriter::new(1 << 20);
    let mut from = 0;
    while let SnapshotProgress::More(n) = m.snapshot_chunk(from, &mut w, 4_096) {
        from = n;
    }
    (path, w.as_bytes().to_vec())
}

type Case = (
    bool,
    u16,
    [i64; 6],
    Vec<(u16, u16, i64)>,
    [i64; 6],
    Vec<(u16, u16, i64)>,
    Vec<(u8, u16, i64, u32)>,
);

/// Runs one kind both ways: `S` directly, and through `ProbeSet`.
fn compare<S: Strategy>(kind: u16, desc: &'static StrategyDesc, c: &Case) -> Result<(), TestCaseError> {
    let (_, n, d, ov, d2, ov2, steps) = c;
    let n = *n;
    let t = table(&[gspec(G1), gspec(G2), gspec(G3)][..usize::from(n)]);
    let mut v = clocked_views();
    let fx = gt3_fixture();
    let rd = fx.view();
    let body = encode_body(&TP, n, &raw_params(n, d, ov)).unwrap();
    let al = aligned(&body);
    let body = bytes_of(&al, body.len());
    // decode_params / validate_params.
    let direct = S::Params::decode(body, 2, &t);
    let via = ProbeSet::decode_params(kind, body, 2, &t);
    prop_assert_eq!(direct.is_ok(), via.is_ok());
    let (Ok(p), Ok(sp)) = (direct, via) else {
        return Ok(());
    };
    prop_assert_eq!(p.validate(&t, &rd), ProbeSet::validate_params(&sp, &t, &rd));
    // create / create_model / subscriptions.
    let mut a = S::new(s3(kind), &p, &t, &rd).unwrap();
    let mut b = ProbeSet::create(s3(kind), &sp, &t, &rd).unwrap();
    let mut ma = <S::Model as Model>::new(s3(kind), &p, &t, &rd);
    let mut mb = ProbeSet::create_model(s3(kind), &sp, &t, &rd);
    prop_assert_eq!(subs(&a, &t, &v), set_subs(&b, &t, &v));
    prop_assert_eq!(
        <S::Model as Model>::mem_bytes(&p, 8),
        ProbeSet::model_mem_bytes(&sp, 8)
    );
    prop_assert_eq!(
        <S::Model as Model>::snapshot_bytes(&p, 8),
        ProbeSet::model_snapshot_bytes(&sp, 8)
    );
    prop_assert_eq!(
        <S::Model as Model>::path_settle_ms(&p),
        ProbeSet::model_path_settle_ms(&sp)
    );
    // Event script.
    let (mut ha, mut hb) = (CtxHarness::new(desc), CtxHarness::new(desc));
    let (mut hma, mut hmb) = (ModelHarness::new(), ModelHarness::new());
    for &(k, g, x, dt) in steps {
        let now = Views::now(&v);
        v.at_tsc(Tsc(now.tsc.0 + u64::from(dt) * 1_000_000));
        let e = if k == u8::MAX {
            ev::timer(MODEL_TICK, GROUP_NONE, GIX_NONE)
        } else {
            variant(k, g % n, x, Views::now(&v))
        };
        let er = e.as_ref();
        let ra = ha.run(|ctx| a.on_event(&er, &v, ctx));
        let rb = hb.run(|ctx| ProbeSet::on_event(&mut b, &er, &v, ctx));
        rec_eq(&ra, &rb)?;
        let oa = hma.run(|o| ma.on_event(&er, &v, o));
        let ob = hmb.run(|o| ProbeSet::model_on_event(&mut mb, &er, &v, o));
        prop_assert_eq!(oa, ob);
    }
    // snapshot / restore.
    let sa = snap(&a);
    prop_assert_eq!(&sa, &snap_set(&b));
    let mut b2 = ProbeSet::create(s3(kind), &sp, &t, &rd).unwrap();
    let mut a2 = S::new(s3(kind), &p, &t, &rd).unwrap();
    prop_assert_eq!(
        a2.restore(&mut SnapshotReader::with_desc_version(&sa, desc.version)),
        ProbeSet::restore(&mut b2, &mut SnapshotReader::with_desc_version(&sa, desc.version))
    );
    prop_assert_eq!(snap(&a2), snap_set(&b2));
    // Model checkpoint.
    let (pa, ba) = model_bytes(&ma);
    prop_assert_eq!((&pa, &ba), (&model_bytes_set(&mb).0, &model_bytes_set(&mb).1));
    let mut ma2 = <S::Model as Model>::new(s3(kind), &p, &t, &rd);
    let mut mb2 = ProbeSet::create_model(s3(kind), &sp, &t, &rd);
    prop_assert_eq!(
        ma2.restore(&mut SnapshotReader::new(&ba)),
        ProbeSet::model_restore(&mut mb2, &mut SnapshotReader::new(&ba))
    );
    prop_assert_eq!(
        ma2.restore_path(&mut SnapshotReader::new(&pa)),
        ProbeSet::model_restore_path(&mut mb2, &mut SnapshotReader::new(&pa))
    );
    prop_assert_eq!(model_bytes(&ma2), model_bytes_set(&mb2));
    // on_params / model_on_params / reset_preview with second parameters.
    let body2 = encode_body(&TP, n, &raw_params(n, d2, ov2)).unwrap();
    let al2 = aligned(&body2);
    let body2 = bytes_of(&al2, body2.len());
    if let (Ok(p2), Ok(sp2)) = (
        S::Params::decode(body2, 2, &t),
        ProbeSet::decode_params(kind, body2, 2, &t),
    ) {
        prop_assert_eq!(
            a.on_params(&p2, &t, &rd),
            ProbeSet::on_params(&mut b, &sp2, &t, &rd)
        );
        prop_assert_eq!(
            ma.on_params(&p2, &t, &rd),
            ProbeSet::model_on_params(&mut mb, &sp2, &t, &rd)
        );
        prop_assert_eq!(snap(&a), snap_set(&b));
        prop_assert_eq!(model_bytes(&ma), model_bytes_set(&mb));
        prop_assert_eq!(
            reset_preview_of::<S>(&p, &t, &p2, &t),
            ProbeSet::reset_preview(&sp, &t, &sp2, &t)
        );
    }
    // Parameters of another kind are refused.
    let other = if kind == 0xFF01 { 0xFF02 } else { 0xFF01 };
    let op = ProbeSet::decode_params(other, body, 2, &t).unwrap();
    prop_assert!(
        ProbeSet::on_params(&mut b, &op, &t, &rd).is_err(),
        "other kind's parameters"
    );
    Ok(())
}

fn arb_case() -> impl proptest::strategy::Strategy<Value = Case> {
    let ov = || proptest::collection::vec((0u16..3, 0u16..6, any::<i64>()), 0..6);
    let step = proptest::prop_oneof![
        9 => (0u8..N_VARIANTS, 0u16..3, -100_000i64..100_000, 0u32..2_000),
        1 => (0u32..2_000).prop_map(|dt| (u8::MAX, 0, 0, dt)),
    ];
    (
        any::<bool>(),
        1u16..=3,
        any::<[i64; 6]>(),
        ov(),
        any::<[i64; 6]>(),
        ov(),
        proptest::collection::vec(step, 0..=500),
    )
}

use proptest::strategy::Strategy as _;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1_000))]

    #[test]
    fn sdk_185_dispatch_equivalent(c in arb_case()) {
        if c.0 {
            compare::<EchoProbe>(0xFF01, &EchoProbe::DESC, &c)?;
        } else {
            compare::<TargetProbe>(0xFF02, &TargetProbe::DESC, &c)?;
        }
    }
}
