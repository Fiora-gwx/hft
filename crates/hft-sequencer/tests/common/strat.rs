//! Test strategy kinds of the B0 baseline (`probe-taker` 0xFF01, `probe-target` 0xFF02 with a
//! Model, `probe-np` 0xFF03 `non_prod_only`), written by hand on the SDK trait. They stand in for
//! hft-testkit's `TestStrategies` (not yet on main) and are driven by per-thread scripts: every
//! delivery is logged, and an optional hook may act on the `Ctx`.
#![allow(dead_code)]

use std::cell::RefCell;

use hft_strategy_sdk::params::{RawParams, decode_body, encode_body, schema_digest, schema_json};
use hft_strategy_sdk::{
    AdviceOut, BboView, CmdDesc, Ctx, EvMask, EventRef, FieldDesc, FundingView, GroupTable, LegView, MinorChange,
    ModelLayoutDesc, ModelParamEffect, ModelViews, Now, OrderRef, OrderView, ParamError, ParamSchemaDesc, Part,
    RefView, Relax, ResetPreview, RestoreError, RiskStateView, SnapshotProgress, SnapshotReader, SnapshotWriter,
    StrategyDesc, StrategyInstance, StrategySet, SubsBuilder, TargetView, Unit, Views,
};
use hft_types::{AccountId, GroupId, InstId, RunMode, StrategyId};

const fn field(name: &'static str, max: i64, structural: bool) -> FieldDesc {
    FieldDesc {
        name,
        unit: Unit::Count,
        min: 0,
        max,
        default: 0,
        per_group: false,
        structural,
        manual_only: false,
        relax: Relax::None,
        minor_change: MinorChange::None,
        since: 1,
        doc: "test field",
    }
}

/// Field indices: win 0 (structural), thr 1, bad 2 (`validate` rejects ≠ 0), mem 3, ckpt 4.
pub const FIELDS: &[FieldDesc] = &[
    field("win", 1_000_000, true),
    field("thr", 1_000_000, false),
    field("bad", 1, false),
    field("mem", i64::MAX / 4, false),
    field("ckpt", i64::MAX / 4, false),
];
pub const F_WIN: usize = 0;
pub const F_THR: usize = 1;
pub const F_BAD: usize = 2;
pub const F_MEM: usize = 3;
pub const F_CKPT: usize = 4;

const QTY: &[FieldDesc] = &[FieldDesc {
    name: "qty",
    unit: Unit::Count,
    min: 1,
    max: 100,
    default: 1,
    per_group: false,
    structural: false,
    manual_only: false,
    relax: Relax::None,
    minor_change: MinorChange::None,
    since: 1,
    doc: "lots",
}];

pub const CMDS: &[CmdDesc] = &[
    CmdDesc {
        op: 1,
        name: "open",
        reduce_only: false,
        fields: QTY,
        doc: "open one group",
    },
    CmdDesc {
        op: 2,
        name: "close",
        reduce_only: true,
        fields: &[],
        doc: "close",
    },
];

pub static SCHEMA_TAKER: ParamSchemaDesc = ParamSchemaDesc {
    id: 0xFF01,
    version: 1,
    fields: FIELDS,
};
pub static SCHEMA_TARGET: ParamSchemaDesc = ParamSchemaDesc {
    id: 0xFF02,
    version: 1,
    fields: FIELDS,
};
pub static SCHEMA_NP: ParamSchemaDesc = ParamSchemaDesc {
    id: 0xFF03,
    version: 1,
    fields: FIELDS,
};

pub const fn desc(
    name: &'static str,
    kind_id: u16,
    version: u16,
    non_prod_only: bool,
    schema: &'static ParamSchemaDesc,
) -> StrategyDesc {
    StrategyDesc {
        name,
        kind_id,
        version,
        non_prod_only,
        schema,
        notes: &[],
        report: &[],
        cmds: CMDS,
    }
}

pub const TAKER: u16 = 0xFF01;
pub const TARGET: u16 = 0xFF02;
pub const NP: u16 = 0xFF03;

pub static KINDS_V3: [StrategyDesc; 3] = [
    desc("probe-taker", TAKER, 3, false, &SCHEMA_TAKER),
    desc("probe-target", TARGET, 3, false, &SCHEMA_TARGET),
    desc("probe-np", NP, 3, true, &SCHEMA_NP),
];
pub static KINDS_V4: [StrategyDesc; 3] = [
    desc("probe-taker", TAKER, 4, false, &SCHEMA_TAKER),
    desc("probe-target", TARGET, 4, false, &SCHEMA_TARGET),
    desc("probe-np", NP, 4, true, &SCHEMA_NP),
];

/// Decoded parameters of a test kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P {
    pub kind_id: u16,
    pub raw: RawParams,
}

impl P {
    pub fn get(&self, f: usize) -> i64 {
        self.raw.defaults.get(f).copied().unwrap_or(0)
    }
}

/// Body bytes of `values` (one default per field, no overrides).
pub fn body(values: [i64; 5], n_groups: u16) -> Vec<u8> {
    encode_body(
        &SCHEMA_TAKER,
        n_groups,
        &RawParams {
            defaults: values.to_vec(),
            overrides: Vec::new(),
        },
    )
    .unwrap()
}

/// Schema digest of a kind (as the SPB1 header carries it).
pub fn digest(kind_id: u16) -> u64 {
    let d = KINDS_V3.iter().find(|d| d.kind_id == kind_id).unwrap();
    schema_digest(d, 1)
}

// ------------------------------------------------------------------------------- scripts

/// One delivery as the test sees it.
#[derive(Clone, Debug)]
pub struct Got {
    pub s: StrategyId,
    pub model: bool,
    pub kind: &'static str,
    pub gix: u16,
    pub inst: InstId,
    pub now: Now,
    /// Debug rendering of the event.
    pub text: String,
    pub bbo: Option<BboView>,
}

/// Read access to the views from a hook.
pub trait DynViews {
    fn now(&self) -> Now;
    fn bbo(&self, i: InstId) -> BboView;
    fn target(&self, a: AccountId, g: GroupId) -> Option<TargetView>;
    fn funding(&self, i: InstId) -> FundingView;
    fn mode(&self, a: AccountId, g: GroupId) -> RunMode;
    fn leverage(&self, a: AccountId, i: InstId) -> Option<u32>;
    fn order(&self, h: OrderRef) -> Option<OrderView>;
    fn legs(&self, a: AccountId, g: GroupId) -> LegView;
    fn risk(&self, a: AccountId) -> RiskStateView;
    fn mark(&self, i: InstId) -> hft_types::Px;
}

struct W<'a, V: Views>(&'a V);

impl<V: Views> DynViews for W<'_, V> {
    fn now(&self) -> Now {
        self.0.now()
    }
    fn bbo(&self, i: InstId) -> BboView {
        self.0.bbo(i)
    }
    fn target(&self, a: AccountId, g: GroupId) -> Option<TargetView> {
        self.0.target(a, g).copied()
    }
    fn funding(&self, i: InstId) -> FundingView {
        *self.0.funding(i)
    }
    fn mode(&self, a: AccountId, g: GroupId) -> RunMode {
        self.0.mode(a, g)
    }
    fn leverage(&self, a: AccountId, i: InstId) -> Option<u32> {
        self.0.leverage(a, i)
    }
    fn order(&self, h: OrderRef) -> Option<OrderView> {
        self.0.order(h)
    }
    fn legs(&self, a: AccountId, g: GroupId) -> LegView {
        *self.0.legs(a, g)
    }
    fn risk(&self, a: AccountId) -> RiskStateView {
        self.0.risk(a)
    }
    fn mark(&self, i: InstId) -> hft_types::Px {
        self.0.mark(i)
    }
}

/// What a hook sees.
pub struct Hx<'a> {
    pub s: StrategyInstance,
    pub v: &'a dyn DynViews,
    pub groups: &'a GroupTable,
}

type Hook = Box<dyn FnMut(&Hx<'_>, &EventRef<'_>, &mut Ctx<'_>)>;
type MHook = Box<dyn FnMut(&Hx<'_>, &EventRef<'_>, &mut AdviceOut<'_>)>;

/// Subscription script entry: `(inst, gix, mask, part)`; `None` = default (each group's legs,
/// BBO, fast part).
pub type SubScript = Vec<(InstId, u16, EvMask, Part)>;

#[derive(Default)]
pub struct Script {
    pub log: Vec<Got>,
    pub hook: Option<Hook>,
    pub mhook: Option<MHook>,
    pub subs: Vec<(StrategyId, SubScript)>,
    pub model_tick: Vec<(StrategyId, u32)>,
    pub warmup_both: Vec<StrategyId>,
    pub model_own: Vec<(StrategyId, u8)>,
    pub calls: Vec<(StrategyId, &'static str)>,
    pub fail_create: bool,
    pub fail_on_params: bool,
    /// `model_on_params` effect for a params version.
    pub reset_effect: Vec<(u64, ModelParamEffect)>,
    /// `Model::PATH_VERSION` of the binary.
    pub path_version: u32,
    pub fail_model_restore: bool,
    /// Another `model_layout` (stands in for `LAYOUT_VERSION + 1`).
    pub layout_alt: bool,
    /// `model_on_params` calls: (strategy, params version).
    pub model_params: Vec<(StrategyId, i64)>,
}

thread_local! {
    pub static SCRIPT: RefCell<Script> = RefCell::new(Script::default());
}

pub fn reset_script() {
    SCRIPT.with(|s| *s.borrow_mut() = Script::default());
}
pub fn log() -> Vec<Got> {
    SCRIPT.with(|s| s.borrow().log.clone())
}
pub fn clear_log() {
    SCRIPT.with(|s| s.borrow_mut().log.clear());
}
pub fn calls() -> Vec<(StrategyId, &'static str)> {
    SCRIPT.with(|s| s.borrow().calls.clone())
}
pub fn set_hook(h: impl FnMut(&Hx<'_>, &EventRef<'_>, &mut Ctx<'_>) + 'static) {
    SCRIPT.with(|s| s.borrow_mut().hook = Some(Box::new(h)));
}
pub fn clear_hook() {
    SCRIPT.with(|s| s.borrow_mut().hook = None);
}
pub fn set_mhook(h: impl FnMut(&Hx<'_>, &EventRef<'_>, &mut AdviceOut<'_>) + 'static) {
    SCRIPT.with(|s| s.borrow_mut().mhook = Some(Box::new(h)));
}
pub fn set_subs(s: StrategyId, v: SubScript) {
    SCRIPT.with(|x| {
        let mut x = x.borrow_mut();
        x.subs.retain(|e| e.0 != s);
        x.subs.push((s, v));
    });
}
pub fn set_warmup_both(s: StrategyId) {
    SCRIPT.with(|x| x.borrow_mut().warmup_both.push(s));
}

fn event_name(ev: &EventRef<'_>) -> (&'static str, u16, InstId) {
    match ev {
        EventRef::Bbo { inst, gix, .. } => ("Bbo", *gix, *inst),
        EventRef::Book { inst, gix } => ("Book", *gix, *inst),
        EventRef::Trade { inst, gix, .. } => ("Trade", *gix, *inst),
        EventRef::Funding { inst, gix, .. } => ("Funding", *gix, *inst),
        EventRef::Mark { inst, gix, .. } => ("Mark", *gix, *inst),
        EventRef::Index { inst, gix, .. } => ("Index", *gix, *inst),
        EventRef::Liquidation { inst, gix, .. } => ("Liquidation", *gix, *inst),
        EventRef::Order { gix, .. } => ("Order", *gix, InstId(0)),
        EventRef::Exec { gix, .. } => ("Exec", *gix, InstId(0)),
        EventRef::External { gix, f } => ("External", *gix, f.inst),
        EventRef::Risk { .. } => ("Risk", 0, InstId(0)),
        EventRef::Notice { inst, gix, .. } => ("Notice", *gix, *inst),
        EventRef::Timer { gix, .. } => ("Timer", *gix, InstId(0)),
        EventRef::Advice { gix, .. } => ("Advice", *gix, InstId(0)),
        EventRef::Mode { gix, .. } => ("Mode", *gix, InstId(0)),
        EventRef::Conn { .. } => ("Conn", 0, InstId(0)),
        EventRef::Manual { gix, .. } => ("Manual", *gix, InstId(0)),
        EventRef::Warmup { .. } => ("Warmup", 0, InstId(0)),
        _ => ("Other", 0, InstId(0)),
    }
}

/// The fast part.
#[derive(Debug)]
pub struct TestInst {
    pub i: StrategyInstance,
    pub n_events: u64,
    pub groups: GroupTable,
}

/// The Model.
#[derive(Debug)]
pub struct TestModel {
    pub i: StrategyInstance,
    pub n_events: u64,
    pub groups: GroupTable,
}

macro_rules! test_set {
    ($name:ident, $kinds:ident) => {
        /// A hand-written `StrategySet` over the test kinds.
        #[derive(Debug)]
        pub struct $name;

        impl StrategySet for $name {
            type Inst = TestInst;
            type ModelInst = TestModel;
            type Params = P;
            fn kinds() -> &'static [StrategyDesc] {
                &$kinds
            }
            fn decode_params(kind_id: u16, body: &[u8], schema_ver: u16, g: &GroupTable) -> Result<P, ParamError> {
                let d = $kinds.iter().find(|d| d.kind_id == kind_id).ok_or(ParamError::SchemaMismatch {
                    want: 0,
                    got: u32::from(kind_id),
                })?;
                if g.len() > g.capacity() {
                    return Err(ParamError::TooManyGroups);
                }
                let raw = decode_body(d.schema, schema_ver, g.len(), body)?;
                Ok(P { kind_id, raw })
            }
            fn validate_params(p: &P, _g: &GroupTable, _rd: &RefView<'_>) -> Result<(), ParamError> {
                if p.get(F_BAD) != 0 {
                    return Err(ParamError::Invalid {
                        field_ix: F_BAD as u16,
                        code: 1,
                    });
                }
                Ok(())
            }
            fn create(i: StrategyInstance, _: &P, g: &GroupTable, _: &RefView<'_>) -> Result<TestInst, ParamError> {
                let fail = SCRIPT.with(|s| {
                    let mut s = s.borrow_mut();
                    s.calls.push((i.strategy, "create"));
                    s.fail_create
                });
                if fail {
                    return Err(ParamError::Invalid { field_ix: 0, code: 9 });
                }
                Ok(TestInst {
                    i,
                    n_events: 0,
                    groups: g.clone(),
                })
            }
            fn create_model(i: StrategyInstance, _: &P, g: &GroupTable, _: &RefView<'_>) -> TestModel {
                SCRIPT.with(|s| s.borrow_mut().calls.push((i.strategy, "create_model")));
                TestModel {
                    i,
                    n_events: 0,
                    groups: g.clone(),
                }
            }
            fn on_event<V: Views>(s: &mut TestInst, ev: &EventRef<'_>, v: &V, ctx: &mut Ctx<'_>) {
                s.n_events += 1;
                let (kind, gix, inst) = event_name(ev);
                let bbo = match ev {
                    EventRef::Bbo { b, .. } => Some(*b),
                    _ => None,
                };
                let got = Got {
                    s: s.i.strategy,
                    model: false,
                    kind,
                    gix,
                    inst,
                    now: v.now(),
                    text: format!("{ev:?}"),
                    bbo,
                };
                let hook = SCRIPT.with(|x| {
                    let mut x = x.borrow_mut();
                    x.log.push(got);
                    x.hook.take()
                });
                if let Some(mut h) = hook {
                    let w = W(v);
                    let hx = Hx {
                        s: s.i,
                        v: &w,
                        groups: &s.groups,
                    };
                    h(&hx, ev, ctx);
                    SCRIPT.with(|x| {
                        let mut x = x.borrow_mut();
                        if x.hook.is_none() {
                            x.hook = Some(h);
                        }
                    });
                }
            }
            fn on_params(s: &mut TestInst, _: &P, g: &GroupTable, _: &RefView<'_>) -> Result<(), ParamError> {
                let fail = SCRIPT.with(|x| {
                    let mut x = x.borrow_mut();
                    x.calls.push((s.i.strategy, "on_params"));
                    x.fail_on_params
                });
                if fail {
                    return Err(ParamError::Invalid { field_ix: 1, code: 9 });
                }
                s.groups = g.clone();
                Ok(())
            }
            fn subscriptions(s: &TestInst, g: &GroupTable, out: &mut SubsBuilder<'_>) {
                let script = SCRIPT.with(|x| {
                    let x = x.borrow();
                    (
                        x.subs.iter().find(|e| e.0 == s.i.strategy).map(|e| e.1.clone()),
                        x.model_tick.iter().find(|e| e.0 == s.i.strategy).map(|e| e.1),
                        x.warmup_both.contains(&s.i.strategy),
                        x.model_own.iter().find(|e| e.0 == s.i.strategy).map(|e| e.1),
                    )
                });
                match script.0 {
                    Some(v) => {
                        for (i, gix, m, p) in v {
                            let _ = out.market(i, gix, m, p);
                        }
                    }
                    None => {
                        for (gix, sp) in g.iter() {
                            for l in sp.legs.iter().take(usize::from(sp.n_legs)) {
                                let _ = out.market(l.inst, gix, EvMask::BBO, Part::Fast);
                            }
                        }
                    }
                }
                if let Some(t) = script.1 {
                    out.model_tick(t);
                }
                if script.2 {
                    out.warmup(Part::Both);
                }
                if let Some(o) = script.3 {
                    out.model_own(hft_strategy_sdk::OwnEvMask(o));
                }
            }
            fn snapshot(s: &TestInst, w: &mut SnapshotWriter) {
                w.put_u64(s.n_events);
            }
            fn restore(s: &mut TestInst, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
                let cur = $kinds.iter().find(|d| d.kind_id == s.i.kind_id).map_or(0, |d| d.version);
                let v = r.desc_version();
                if v != cur && v + 1 != cur {
                    return Err(RestoreError::VersionUnsupported);
                }
                s.n_events = r.get_u64()?;
                Ok(())
            }
            fn model_mem_bytes(p: &P, _n_groups: u16) -> u64 {
                p.get(F_MEM) as u64
            }
            fn model_snapshot_bytes(p: &P, _n_groups: u16) -> u64 {
                p.get(F_CKPT) as u64
            }
            fn model_layout(_: u16) -> &'static ModelLayoutDesc {
                static ALT: ModelLayoutDesc = ModelLayoutDesc {
                    sections: &[hft_strategy_sdk::SectionDesc {
                        tag: 1,
                        name: "alt",
                        scope: hft_strategy_sdk::SectionScope::Instance,
                        elems: &[],
                    }],
                };
                if SCRIPT.with(|x| x.borrow().layout_alt) { &ALT } else { &ModelLayoutDesc::EMPTY }
            }
            fn model_on_event<V: ModelViews>(m: &mut TestModel, ev: &EventRef<'_>, v: &V, out: &mut AdviceOut<'_>) {
                m.n_events += 1;
                let (kind, gix, inst) = event_name(ev);
                let got = Got {
                    s: m.i.strategy,
                    model: true,
                    kind,
                    gix,
                    inst,
                    now: v.now(),
                    text: format!("{ev:?}"),
                    bbo: None,
                };
                let hook = SCRIPT.with(|x| {
                    let mut x = x.borrow_mut();
                    x.log.push(got);
                    x.mhook.take()
                });
                if let Some(mut h) = hook {
                    let w = W(v);
                    let hx = Hx {
                        s: m.i,
                        v: &w,
                        groups: &m.groups,
                    };
                    h(&hx, ev, out);
                    SCRIPT.with(|x| {
                        let mut x = x.borrow_mut();
                        if x.mhook.is_none() {
                            x.mhook = Some(h);
                        }
                    });
                }
            }
            fn model_on_params(m: &mut TestModel, p: &P, g: &GroupTable, _: &RefView<'_>) -> ModelParamEffect {
                m.groups = g.clone();
                SCRIPT.with(|x| {
                    let mut x = x.borrow_mut();
                    x.calls.push((m.i.strategy, "model_on_params"));
                    x.model_params.push((m.i.strategy, p.get(F_THR)));
                    let v = p.get(F_THR);
                    x.reset_effect.iter().find(|e| e.0 as i64 == v).map(|e| e.1.clone()).unwrap_or(ModelParamEffect::None)
                })
            }
            fn model_snapshot_chunk(m: &TestModel, from_gix: u16, w: &mut SnapshotWriter, _max: u32) -> SnapshotProgress {
                w.put_u64(m.n_events);
                if usize::from(from_gix) + 1 < usize::from(m.groups.len()) {
                    SnapshotProgress::More(from_gix + 1)
                } else {
                    SnapshotProgress::Done
                }
            }
            fn model_restore(m: &mut TestModel, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
                if SCRIPT.with(|x| x.borrow().fail_model_restore) {
                    return Err(RestoreError::VersionUnsupported);
                }
                m.n_events = r.get_u64()?;
                Ok(())
            }
            fn model_path_version(_: u16) -> u32 {
                SCRIPT.with(|x| x.borrow().path_version)
            }
            fn model_snapshot_path(_: &TestModel, _: &mut SnapshotWriter) {}
            fn model_restore_path(_: &mut TestModel, _: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
                Ok(())
            }
            fn model_path_settle_ms(_: &P) -> u64 {
                0
            }
            fn reset_preview(old: &P, old_g: &GroupTable, new: &P, new_g: &GroupTable) -> ResetPreview {
                let changed = old.get(F_WIN) != new.get(F_WIN);
                let n = new_g
                    .specs()
                    .iter()
                    .filter(|s| changed || old_g.gix_of(s.id).is_none())
                    .count() as u32;
                ResetPreview {
                    reset_groups: n,
                    no_open_ms: n.saturating_mul(1_000),
                }
            }
            fn schema_json(kind_id: u16) -> Option<String> {
                $kinds
                    .iter()
                    .find(|d| d.kind_id == kind_id)
                    .map(|d| schema_json(d, &ModelLayoutDesc::EMPTY, d.schema.version))
            }
            fn cmds(kind_id: u16) -> &'static [CmdDesc] {
                $kinds.iter().find(|d| d.kind_id == kind_id).map_or(&[], |d| d.cmds)
            }
        }
    };
}

test_set!(TestSet, KINDS_V3);
test_set!(TestSetV4, KINDS_V4);
