//! `probe-target` (README "5. 解耦门槛夹具"): sends `TargetIntent`s on a parameterised schedule —
//! `TakerSlice` at `t_slice_ms`, `PairOnFill` at `t_pair_ms`, `Simultaneous` at `t_simul_ms` of
//! engine time (0 = never) — and has a `Model` producing an opaque Advice every `advice_every`
//! events. With `need_advice`, a schedule point is sent only while the cached Advice (hft-strat-lib
//! `Expiring`, `advice_max_age_ms` from its basis time) is fresh, otherwise the point is skipped.
//! Fixed capabilities the scenarios use: structural field `win`, non-structural `thr`, commands
//! `op 1` (`set_thr`), `op 2` (`exit_all`), `op 3` (`pair_lots`) and `op 4` (`pair_exit`).
//! Uses only hft-strategy-sdk and hft-strat-lib.

use hft_strat_lib::{Expiring, Fresh};
use hft_strategy_sdk::prelude::*;
use hft_strategy_sdk::{
    AdviceOut, CmdDesc, CmdView, Ctx, ElemDesc, ElemTy, EvMask, EventRef, ExecSpec, FieldDesc, FirstLeg,
    GroupTable, LenRule, Model, ModelLayoutDesc, ModelParamEffect, ModelViews, NoteDesc, PairParams,
    ParamError, Part, PxMode, RefView, ResidualPolicy, RestoreError, SectionDesc, SectionScope, SimulOnFail,
    SimulParams, SnapshotProgress, SnapshotReader, SnapshotWriter, Strategy, StrategyDesc, StrategyInstance,
    SubsBuilder, TakerSliceParams, TargetIntent, TargetReason, Urgency, Views,
};

hft_strategy_sdk::define_params! {
    pub struct TargetParams(schema_id = 0xFF02, version = 1, fields = TargetFields) {
        /// structural window (changing it restarts the Model's group state)
        win: Count [min = 1, max = 1_000_000, default = 60, structural],
        /// non-structural threshold
        thr: Count [min = 0, max = 1_000_000, default = 0],
        /// longest age of the cached Advice from its basis time
        advice_max_age_ms: DurationMs [min = 0, max = 3_600_000, default = 1_000],
        /// schedule points of engine time (0 = never)
        t_slice_ms: DurationMs [min = 0, max = 86_400_000, default = 0],
        t_pair_ms: DurationMs [min = 0, max = 86_400_000, default = 0],
        t_simul_ms: DurationMs [min = 0, max = 86_400_000, default = 0],
        /// targets need a fresh Advice
        need_advice: Bool [default = 0],
        /// Model: one Advice every this many events
        advice_every: Count [min = 1, max = 1_000_000, default = 5],
        /// Model heap and checkpoint declarations (KiB; ProbeBuild)
        model_mem_kib: Count [min = 1, max = 1_048_576, default = 2_048],
        model_ckpt_kib: Count [min = 1, max = 1_048_576, default = 64],
    }
}

/// Field indices.
pub const F_WIN: u16 = 0;
pub const F_THR: u16 = 1;

const THR_FIELD: &[FieldDesc] = &[FieldDesc {
    name: "thr",
    unit: hft_strategy_sdk::Unit::Count,
    min: 0,
    max: 1_000_000,
    default: 0,
    per_group: false,
    structural: false,
    manual_only: false,
    relax: hft_strategy_sdk::Relax::None,
    minor_change: hft_strategy_sdk::MinorChange::None,
    since: 1,
    doc: "new threshold",
}];

const LOTS_FIELD: &[FieldDesc] = &[FieldDesc {
    name: "lots",
    unit: hft_strategy_sdk::Unit::Count,
    min: 1,
    max: 1_000_000,
    default: 1,
    per_group: false,
    structural: false,
    manual_only: false,
    relax: hft_strategy_sdk::Relax::None,
    minor_change: hft_strategy_sdk::MinorChange::None,
    since: 1,
    doc: "target quantity in first-leg lots",
}];

pub static CMDS: [CmdDesc; 4] = [
    CmdDesc {
        op: 1,
        name: "set_thr",
        reduce_only: false,
        fields: THR_FIELD,
        doc: "set thr",
    },
    CmdDesc {
        op: 2,
        name: "exit_all",
        reduce_only: true,
        fields: &[],
        doc: "exit every group",
    },
    CmdDesc {
        op: 3,
        name: "pair_lots",
        reduce_only: false,
        fields: LOTS_FIELD,
        doc: "set a PairOnFill target in first-leg lots",
    },
    CmdDesc {
        op: 4,
        name: "pair_exit",
        reduce_only: true,
        fields: &[],
        doc: "exit with PairOnFill",
    },
];

pub const NOTE_SKIPPED: u32 = 1;
pub static NOTES: [NoteDesc; 1] = [NoteDesc {
    code: NOTE_SKIPPED,
    name: "Skipped",
    level: NoteLevel::Info,
    v: &["schedule point"],
    doc: "a schedule point was skipped for lack of a fresh Advice",
}];

/// `TargetFields` values of the three schedule points.
fn points(f: &TargetFields) -> [u32; 3] {
    [f.t_slice_ms, f.t_pair_ms, f.t_simul_ms]
}

fn slice() -> TakerSliceParams {
    TakerSliceParams {
        depth_frac: Rate(1_000_000_000),
        depth_levels: 1,
        _pad0: [0; 7],
        max_cross: Rate(5_000_000),
        max_chunk_notional: Notional(i64::MAX / 4),
        min_chunk_notional: Notional(0),
        cooldown_ms: 0,
        _pad1: 0,
    }
}

/// Exec spec of schedule point `k` (0 TakerSlice, 1 PairOnFill, 2 Simultaneous).
pub fn spec(k: usize) -> ExecSpec {
    match k {
        0 => ExecSpec::TakerSlice(slice()),
        1 => ExecSpec::PairOnFill(PairParams {
            slice: slice(),
            first_leg: FirstLeg::Fixed,
            first_leg_ix: 0,
            hedge_px_mode: PxMode::Cross,
            _pad0: [0; 5],
            hedge_cross: Rate(5_000_000),
            hedge_max_slip: Rate(5_000_000),
            ratio_num: 1,
            ratio_den: 1,
            fix_tolerance: Rate(0),
            residual: ResidualPolicy::TrimExcess,
            _pad1: [0; 7],
        }),
        _ => ExecSpec::Simultaneous(SimulParams {
            slice: slice(),
            on_fail: SimulOnFail::Hold,
            _pad0: [0; 7],
            fix_tolerance: Rate(0),
            residual: ResidualPolicy::TrimExcess,
            _pad1: [0; 7],
        }),
    }
}

/// The fast part.
#[derive(Debug)]
pub struct ProbeTarget {
    pub inst: StrategyInstance,
    p: TargetParams,
    gid: GroupId,
    lot: i64,
    /// Schedule points already handled (sent or skipped).
    pub done: [bool; 3],
    pub advice: Expiring<u64>,
    pub thr: u32,
}

impl ProbeTarget {
    fn intent(&self, k: usize, q: i64, reason: TargetReason) -> TargetIntent {
        TargetIntent {
            account: self.inst.account,
            _pad0: [0; 3],
            group: self.gid,
            legs: [
                hft_strategy_sdk::LegTarget::new(0, Qty(q)),
                hft_strategy_sdk::LegTarget::new(1, Qty(-q)),
                hft_strategy_sdk::LegTarget::ZERO,
                hft_strategy_sdk::LegTarget::ZERO,
            ],
            n_legs: 2,
            reason,
            urgency: Urgency::Normal,
            _pad1: [0; 5],
            band: Rate(0),
            exec: spec(k),
            tag: k as u32,
            _pad2: 0,
        }
    }

    fn schedule<V: Views>(&mut self, v: &V, ctx: &mut Ctx<'_>) {
        let now = v.now();
        let f = self.p.at(0);
        for (k, t) in points(f).iter().enumerate() {
            if *t == 0 || self.done[k] || now.engine_ns < i64::from(*t) * 1_000_000 {
                continue;
            }
            self.done[k] = true;
            if f.need_advice && !matches!(self.advice.get(&now), Fresh::Fresh(_)) {
                ctx.note(
                    NOTE_SKIPPED,
                    NoteLevel::Info,
                    self.gid,
                    InstId(0),
                    [k as u64, 0, 0, 0, 0, 0, 0, 0],
                );
                continue;
            }
            let _ = ctx.target(self.intent(k, self.lot, TargetReason::Rebalance));
        }
    }
}

impl Strategy for ProbeTarget {
    const DESC: StrategyDesc = StrategyDesc {
        name: "probe-target",
        kind_id: 0xFF02,
        version: 1,
        non_prod_only: false,
        schema: &TargetParams::SCHEMA,
        notes: &NOTES,
        report: &[],
        cmds: &CMDS,
    };
    type Params = TargetParams;
    type Model = ProbeModel;

    fn new(
        inst: StrategyInstance,
        p: &TargetParams,
        groups: &GroupTable,
        rd: &RefView<'_>,
    ) -> Result<Self, ParamError> {
        let (gid, lot) = groups.iter().next().map_or((GROUP_NONE, 1), |(_, g)| {
            (g.id, g.legs.first().map_or(1, |l| rd.inst(l.inst).lot.max(1)))
        });
        Ok(ProbeTarget {
            inst,
            p: p.clone(),
            gid,
            lot,
            done: [false; 3],
            advice: Expiring::new(0),
            thr: p.at(0).thr,
        })
    }

    fn subscriptions(&self, groups: &GroupTable, out: &mut SubsBuilder<'_>) {
        for (gix, g) in groups.iter() {
            if let Some(l) = g.legs.first() {
                let _ = out.market(l.inst, gix, EvMask::BBO, Part::Both);
            }
        }
        out.model_tick(100);
    }

    fn on_event<V: Views>(&mut self, ev: &EventRef<'_>, v: &V, ctx: &mut Ctx<'_>) {
        match *ev {
            EventRef::Advice {
                payload,
                basis_seq,
                basis_tsc,
                ..
            } => {
                let n = payload
                    .get(..8)
                    .and_then(|b| b.try_into().ok())
                    .map_or(0, u64::from_le_bytes);
                let now = v.now();
                self.advice
                    .put_advice(n, basis_seq, basis_tsc, self.p.at(0).advice_max_age_ms, &now);
            }
            EventRef::Manual { payload, .. } => {
                if let Ok(c) = CmdView::parse(payload) {
                    match c.op() {
                        1 => {
                            self.thr = c
                                .val(0)
                                .map_or(self.thr, |x| x.clamp(0, i64::from(u32::MAX)) as u32)
                        }
                        2 => {
                            let _ = ctx.target(self.intent(0, 0, TargetReason::Exit));
                        }
                        3 => {
                            if let Some(q) = c
                                .val(0)
                                .filter(|n| (1..=1_000_000).contains(n))
                                .and_then(|n| self.lot.checked_mul(n))
                            {
                                let _ = ctx.target(self.intent(1, q, TargetReason::Rebalance));
                            }
                        }
                        4 => {
                            let _ = ctx.target(self.intent(1, 0, TargetReason::Exit));
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        self.schedule(v, ctx);
    }

    fn on_params(
        &mut self,
        p: &TargetParams,
        _groups: &GroupTable,
        _rd: &RefView<'_>,
    ) -> Result<(), ParamError> {
        self.p = p.clone();
        self.thr = p.at(0).thr;
        Ok(())
    }

    fn snapshot(&self, w: &mut SnapshotWriter) {
        for d in self.done {
            w.put_u8(u8::from(d));
        }
        w.put_u32(self.thr);
        self.advice.snapshot(w, |v, w| w.put_u64(*v));
    }

    fn restore(&mut self, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        for d in self.done.iter_mut() {
            *d = r.get_u8()? != 0;
        }
        self.thr = r.get_u32()?;
        self.advice.restore(r, |r| r.get_u64())
    }
}

/// `ProbeModel::LAYOUT`: instance section tag 1 `{n_events: U64 One}`.
pub static PROBE_LAYOUT: ModelLayoutDesc = ModelLayoutDesc {
    sections: &[SectionDesc {
        tag: 1,
        name: "events",
        scope: SectionScope::Instance,
        elems: &[ElemDesc {
            name: "n_events",
            ty: ElemTy::U64,
            len: LenRule::One,
        }],
    }],
};

/// The slow part: counts events and emits an opaque 16-byte Advice (`n_events`, engine_ns) every
/// `advice_every` events.
#[derive(Debug)]
pub struct ProbeModel {
    every: u32,
    pub n_events: u64,
}

impl Model for ProbeModel {
    type Params = TargetParams;
    const LAYOUT_VERSION: u32 = 1;
    const LAYOUT: &'static ModelLayoutDesc = &PROBE_LAYOUT;
    const PATH_VERSION: u32 = 0;
    fn mem_bytes(p: &TargetParams, _n_groups: u16) -> u64 {
        u64::from(p.at(0).model_mem_kib) * 1024
    }
    fn snapshot_bytes(p: &TargetParams, _n_groups: u16) -> u64 {
        u64::from(p.at(0).model_ckpt_kib) * 1024
    }
    fn path_settle_ms(_: &TargetParams) -> u64 {
        0
    }
    fn cold_start_ms(_: &TargetParams, _: u16) -> u64 {
        0
    }
    fn new(_inst: StrategyInstance, p: &TargetParams, _groups: &GroupTable, _rd: &RefView<'_>) -> Self {
        ProbeModel {
            every: p.at(0).advice_every.max(1),
            n_events: 0,
        }
    }
    fn on_params(&mut self, p: &TargetParams, _groups: &GroupTable, _rd: &RefView<'_>) -> ModelParamEffect {
        self.every = p.at(0).advice_every.max(1);
        ModelParamEffect::None
    }
    fn on_event<V: ModelViews>(&mut self, _ev: &EventRef<'_>, v: &V, out: &mut AdviceOut<'_>) {
        self.n_events += 1;
        if self.n_events.is_multiple_of(u64::from(self.every)) {
            let mut b = [0u8; 16];
            b[..8].copy_from_slice(&self.n_events.to_le_bytes());
            b[8..].copy_from_slice(&v.now().engine_ns.to_le_bytes());
            let _ = out.emit_raw(GROUP_NONE, 1, &b);
        }
    }
    fn snapshot_path(&self, _w: &mut SnapshotWriter) {}
    fn restore_path(&mut self, _r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        Ok(())
    }
    fn snapshot_chunk(&self, from_gix: u16, w: &mut SnapshotWriter, _max_bytes: u32) -> SnapshotProgress {
        if from_gix == 0 {
            w.section(1);
            w.put_u64(self.n_events);
        }
        SnapshotProgress::Done
    }
    fn restore(&mut self, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        self.n_events = r.get_u64()?;
        Ok(())
    }
}
