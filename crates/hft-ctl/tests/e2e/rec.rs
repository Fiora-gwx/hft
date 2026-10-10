//! `RecSet`: hft-testkit's `TestStrategies` behind a recording wrapper. Every method delegates
//! unchanged; the calls (and the events the fast and slow parts receive) go into a per-thread log
//! the E2E tests read (the gateway and the sequencer both run on the test thread).

use std::cell::RefCell;

use hft_strategy_sdk::{
    AdviceOut, CmdDesc, Ctx, EventRef, GroupTable, ModelLayoutDesc, ModelParamEffect, ModelViews, ParamError,
    RefView, ResetPreview, RestoreError, SnapshotProgress, SnapshotReader, SnapshotWriter, StrategyDesc,
    StrategyInstance, StrategySet, SubsBuilder, Views,
};
use hft_testkit::TestStrategies as T;
use hft_types::StrategyId;

/// One recorded call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Got {
    /// Method name (`on_event`, `model_on_event`, `decode_params`, …).
    pub call: &'static str,
    /// Instance (0 for the static calls).
    pub s: StrategyId,
    /// `EventRef` variant of `on_event`/`model_on_event` ("" otherwise).
    pub ev: &'static str,
    pub gix: u16,
    /// `Manual`/`Warmup` payload.
    pub payload: Vec<u8>,
    /// `Manual.reduce_only`; `Warmup.last`.
    pub flag: bool,
    /// `Warmup.chunk_no`.
    pub n: u32,
}

thread_local! {
    static LOG: RefCell<Vec<Got>> = const { RefCell::new(Vec::new()) };
}

pub fn clear() {
    LOG.with(|l| l.borrow_mut().clear());
}

pub fn log() -> Vec<Got> {
    LOG.with(|l| l.borrow().clone())
}

fn push(call: &'static str, s: StrategyId) {
    push_ev(call, s, &None);
}

fn push_ev(call: &'static str, s: StrategyId, ev: &Option<(&'static str, u16, Vec<u8>, bool, u32)>) {
    let (name, gix, payload, flag, n) = ev.clone().unwrap_or(("", 0, Vec::new(), false, 0));
    LOG.with(|l| {
        l.borrow_mut().push(Got {
            call,
            s,
            ev: name,
            gix,
            payload,
            flag,
            n,
        })
    });
}

fn describe(ev: &EventRef<'_>) -> (&'static str, u16, Vec<u8>, bool, u32) {
    match *ev {
        EventRef::Bbo { gix, .. } => ("Bbo", gix, Vec::new(), false, 0),
        EventRef::Order { gix, .. } => ("Order", gix, Vec::new(), false, 0),
        EventRef::Exec { gix, .. } => ("Exec", gix, Vec::new(), false, 0),
        EventRef::Risk { .. } => ("Risk", 0, Vec::new(), false, 0),
        EventRef::Notice { gix, .. } => ("Notice", gix, Vec::new(), false, 0),
        EventRef::Timer { gix, .. } => ("Timer", gix, Vec::new(), false, 0),
        EventRef::Advice { gix, .. } => ("Advice", gix, Vec::new(), false, 0),
        EventRef::Mode { gix, .. } => ("Mode", gix, Vec::new(), false, 0),
        EventRef::Manual {
            gix,
            payload,
            reduce_only,
        } => ("Manual", gix, payload.to_vec(), reduce_only, 0),
        EventRef::Warmup {
            chunk_no,
            last,
            payload,
            ..
        } => ("Warmup", 0, payload.to_vec(), last, chunk_no),
        _ => ("Other", 0, Vec::new(), false, 0),
    }
}

/// Fast part: the instance id and the testkit instance.
pub struct RecInst {
    pub s: StrategyId,
    pub inner: <T as StrategySet>::Inst,
}

/// Slow part.
pub struct RecModel {
    pub s: StrategyId,
    pub inner: <T as StrategySet>::ModelInst,
}

/// The recording strategy set.
#[derive(Debug)]
pub struct RecSet;

impl StrategySet for RecSet {
    type Inst = RecInst;
    type ModelInst = RecModel;
    type Params = <T as StrategySet>::Params;

    fn kinds() -> &'static [StrategyDesc] {
        T::kinds()
    }
    fn decode_params(
        kind_id: u16,
        body: &[u8],
        schema_ver: u16,
        g: &GroupTable,
    ) -> Result<Self::Params, ParamError> {
        push("decode_params", StrategyId(0));
        T::decode_params(kind_id, body, schema_ver, g)
    }
    fn validate_params(p: &Self::Params, g: &GroupTable, rd: &RefView<'_>) -> Result<(), ParamError> {
        push("validate_params", StrategyId(0));
        T::validate_params(p, g, rd)
    }
    fn create(
        i: StrategyInstance,
        p: &Self::Params,
        g: &GroupTable,
        rd: &RefView<'_>,
    ) -> Result<Self::Inst, ParamError> {
        push("create", i.strategy);
        T::create(i, p, g, rd).map(|inner| RecInst { s: i.strategy, inner })
    }
    fn create_model(
        i: StrategyInstance,
        p: &Self::Params,
        g: &GroupTable,
        rd: &RefView<'_>,
    ) -> Self::ModelInst {
        push("create_model", i.strategy);
        RecModel {
            s: i.strategy,
            inner: T::create_model(i, p, g, rd),
        }
    }
    fn on_event<V: Views>(s: &mut Self::Inst, ev: &EventRef<'_>, v: &V, ctx: &mut Ctx<'_>) {
        push_ev("on_event", s.s, &Some(describe(ev)));
        T::on_event(&mut s.inner, ev, v, ctx)
    }
    fn on_params(
        s: &mut Self::Inst,
        p: &Self::Params,
        g: &GroupTable,
        rd: &RefView<'_>,
    ) -> Result<(), ParamError> {
        push("on_params", s.s);
        T::on_params(&mut s.inner, p, g, rd)
    }
    fn subscriptions(s: &Self::Inst, g: &GroupTable, out: &mut SubsBuilder<'_>) {
        T::subscriptions(&s.inner, g, out)
    }
    fn snapshot(s: &Self::Inst, w: &mut SnapshotWriter) {
        T::snapshot(&s.inner, w)
    }
    fn restore(s: &mut Self::Inst, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        T::restore(&mut s.inner, r)
    }
    fn model_mem_bytes(p: &Self::Params, n_groups: u16) -> u64 {
        T::model_mem_bytes(p, n_groups)
    }
    fn model_snapshot_bytes(p: &Self::Params, n_groups: u16) -> u64 {
        T::model_snapshot_bytes(p, n_groups)
    }
    fn model_layout(kind_id: u16) -> &'static ModelLayoutDesc {
        T::model_layout(kind_id)
    }
    fn model_on_event<V: ModelViews>(
        m: &mut Self::ModelInst,
        ev: &EventRef<'_>,
        v: &V,
        out: &mut AdviceOut<'_>,
    ) {
        push_ev("model_on_event", m.s, &Some(describe(ev)));
        T::model_on_event(&mut m.inner, ev, v, out)
    }
    fn model_on_params(
        m: &mut Self::ModelInst,
        p: &Self::Params,
        g: &GroupTable,
        rd: &RefView<'_>,
    ) -> ModelParamEffect {
        push("model_on_params", m.s);
        T::model_on_params(&mut m.inner, p, g, rd)
    }
    fn model_snapshot_chunk(
        m: &Self::ModelInst,
        from_gix: u16,
        w: &mut SnapshotWriter,
        max_bytes: u32,
    ) -> SnapshotProgress {
        T::model_snapshot_chunk(&m.inner, from_gix, w, max_bytes)
    }
    fn model_restore(m: &mut Self::ModelInst, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        T::model_restore(&mut m.inner, r)
    }
    fn model_path_version(kind_id: u16) -> u32 {
        T::model_path_version(kind_id)
    }
    fn model_snapshot_path(m: &Self::ModelInst, w: &mut SnapshotWriter) {
        T::model_snapshot_path(&m.inner, w)
    }
    fn model_restore_path(m: &mut Self::ModelInst, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        T::model_restore_path(&mut m.inner, r)
    }
    fn model_path_settle_ms(p: &Self::Params) -> u64 {
        T::model_path_settle_ms(p)
    }
    fn reset_preview(
        old: &Self::Params,
        old_g: &GroupTable,
        new: &Self::Params,
        new_g: &GroupTable,
    ) -> ResetPreview {
        T::reset_preview(old, old_g, new, new_g)
    }
    fn schema_json(kind_id: u16) -> Option<String> {
        T::schema_json(kind_id)
    }
    fn cmds(kind_id: u16) -> &'static [CmdDesc] {
        T::cmds(kind_id)
    }
}
