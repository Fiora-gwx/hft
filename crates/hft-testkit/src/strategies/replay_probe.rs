//! A replay-only strategy set's test A. Its fast part uses the taker fixture, while the Model
//! keeps a raw event count and a separate finite path window. The path-v2 variant changes only
//! the version, so dropping a v1 path has a measurable, bounded convergence interval.

use hft_strategy_sdk::prelude::*;
use hft_strategy_sdk::{
    AdviceOut, Ctx, ElemDesc, ElemTy, EvMask, EventRef, GroupTable, LenRule, Model, ModelLayoutDesc,
    ModelParamEffect, ModelViews, ParamError, Part, RefView, RestoreError, SectionDesc, SectionScope,
    SnapshotProgress, SnapshotReader, SnapshotWriter, Strategy, StrategyDesc, StrategyInstance, SubsBuilder,
    Views,
};

use super::probe_taker::{ProbeTakerT, TakerCfg, TakerCfgV, TakerKind};

hft_strategy_sdk::define_params! {
    pub struct ReplayParams(schema_id = 0xFF04, version = 1, fields = ReplayFields) {
        spread_th: Rate [min = 0, max = 1_000_000_000, default = 0, per_group],
        qty_lots: Count [min = 1, max = 1_000, default = 1, per_group],
        close_on: Count [min = 0, max = 7, default = 7],
        hold_ms: DurationMs [min = 10, max = 3_600_000, default = 5_000],
        /// The maximum age of a path sample, in engine time (at most 120 one-second buckets).
        path_settle_ms: DurationMs [min = 1, max = 120_000, default = 120_000, structural],
    }
}

impl ReplayParams {
    pub fn check(&self, _: &GroupTable, _: &RefView<'_>) -> Result<(), ParamError> {
        Ok(())
    }
}
impl TakerCfg for ReplayParams {
    fn cfg(&self, gix: u16) -> TakerCfgV {
        let p = self.at(gix);
        TakerCfgV {
            spread_th: p.spread_th.0,
            qty_lots: p.qty_lots,
            close_on: p.close_on,
            hold_ms: p.hold_ms,
        }
    }
}
impl TakerKind for ReplayParams {
    const NP: bool = false;
}

#[derive(Debug)]
pub struct ReplayProbe<const PATH: u32>(ProbeTakerT<ReplayParams>);
pub type ReplayA = ReplayProbe<1>;
pub type ReplayPathV2 = ReplayProbe<2>;

impl<const PATH: u32> Strategy for ReplayProbe<PATH> {
    const DESC: StrategyDesc = StrategyDesc {
        name: "replay-a",
        kind_id: 0xFF04,
        version: 1,
        non_prod_only: false,
        schema: &ReplayParams::SCHEMA,
        notes: &super::probe_taker::NOTES,
        report: &[],
        cmds: &[],
    };
    type Params = ReplayParams;
    type Model = ReplayModel<PATH>;

    fn new(
        i: StrategyInstance,
        p: &ReplayParams,
        g: &GroupTable,
        r: &RefView<'_>,
    ) -> Result<Self, ParamError> {
        ProbeTakerT::new(i, p, g, r).map(Self)
    }
    fn subscriptions(&self, groups: &GroupTable, out: &mut SubsBuilder<'_>) {
        for (gix, g) in groups.iter() {
            for leg in g.legs.iter().take(usize::from(g.n_legs).min(2)) {
                let _ = out.market(leg.inst, gix, EvMask::BBO, Part::Both);
            }
        }
    }
    fn on_event<V: Views>(&mut self, ev: &EventRef<'_>, v: &V, ctx: &mut Ctx<'_>) {
        if let EventRef::Bbo { gix, .. } = ev {
            let Some(g) = self.0.groups.get(usize::from(*gix)) else {
                return;
            };
            if v.mode(self.0.inst.account, GroupId(g.id)) != RunMode::Live {
                return;
            }
        }
        self.0.on_event(ev, v, ctx);
    }
    fn on_params(&mut self, p: &ReplayParams, g: &GroupTable, r: &RefView<'_>) -> Result<(), ParamError> {
        self.0.on_params(p, g, r)
    }
    fn snapshot(&self, w: &mut SnapshotWriter) {
        self.0.snapshot(w);
    }
    fn restore(&mut self, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        self.0.restore(r)
    }
}

const BUCKETS: usize = 120;
const SECOND: i64 = 1_000_000_000;
const EMPTY: (i64, i64) = (i64::MIN, 0);
pub static REPLAY_LAYOUT: ModelLayoutDesc = ModelLayoutDesc {
    sections: &[SectionDesc {
        tag: 1,
        name: "samples",
        scope: SectionScope::Instance,
        elems: &[ElemDesc {
            name: "bbo_count",
            ty: ElemTy::U64,
            len: LenRule::One,
        }],
    }],
};

/// Raw state is the cumulative sample count. Path state is the last bid seen in each second
/// bucket. Its timestamps are engine_ns, and each bucket expires at age >= path_settle_ms.
#[derive(Debug)]
pub struct ReplayModel<const PATH: u32> {
    count: u64,
    settle_ns: i64,
    buckets: [(i64, i64); BUCKETS],
}

impl<const PATH: u32> Model for ReplayModel<PATH> {
    type Params = ReplayParams;
    const LAYOUT_VERSION: u32 = 1;
    const LAYOUT: &'static ModelLayoutDesc = &REPLAY_LAYOUT;
    const PATH_VERSION: u32 = PATH;

    fn mem_bytes(_: &ReplayParams, _: u16) -> u64 {
        size_of::<Self>() as u64
    }
    fn snapshot_bytes(_: &ReplayParams, _: u16) -> u64 {
        8 + 16 * BUCKETS as u64
    }
    fn path_settle_ms(p: &ReplayParams) -> u64 {
        u64::from(p.at(0).path_settle_ms)
    }
    fn cold_start_ms(p: &ReplayParams, _: u16) -> u64 {
        Self::path_settle_ms(p)
    }
    fn new(_: StrategyInstance, p: &ReplayParams, _: &GroupTable, _: &RefView<'_>) -> Self {
        Self {
            count: 0,
            settle_ns: i64::from(p.at(0).path_settle_ms) * 1_000_000,
            buckets: [EMPTY; BUCKETS],
        }
    }
    fn on_params(&mut self, p: &ReplayParams, _: &GroupTable, _: &RefView<'_>) -> ModelParamEffect {
        self.settle_ns = i64::from(p.at(0).path_settle_ms) * 1_000_000;
        ModelParamEffect::None
    }
    fn on_event<V: ModelViews>(&mut self, ev: &EventRef<'_>, v: &V, out: &mut AdviceOut<'_>) {
        let EventRef::Bbo { b, .. } = ev else { return };
        let now = v.now().engine_ns;
        self.count += 1;
        for slot in &mut self.buckets {
            if slot.0 <= now.saturating_sub(self.settle_ns) {
                *slot = EMPTY;
            }
        }
        let ix = (now.div_euclid(SECOND).rem_euclid(BUCKETS as i64)) as usize;
        self.buckets[ix] = (now, b.bid_px.0);
        let sum: i128 = self.buckets.iter().map(|(_, bid)| i128::from(*bid)).sum();
        let mut payload = [0u8; 32];
        payload[..8].copy_from_slice(&self.count.to_le_bytes());
        payload[8..16].copy_from_slice(&now.to_le_bytes());
        payload[16..].copy_from_slice(&sum.to_le_bytes());
        let _ = out.emit_raw(GROUP_NONE, 1, &payload);
    }
    fn snapshot_path(&self, w: &mut SnapshotWriter) {
        for (at, bid) in self.buckets {
            w.put_i64(at);
            w.put_i64(bid);
        }
    }
    fn restore_path(&mut self, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        let mut buckets = [EMPTY; BUCKETS];
        for (ix, slot) in buckets.iter_mut().enumerate() {
            *slot = (r.get_i64()?, r.get_i64()?);
            if *slot != EMPTY && (slot.0 < 0 || slot.0 / SECOND % BUCKETS as i64 != ix as i64) {
                return Err(RestoreError::BadValue);
            }
        }
        self.buckets = buckets;
        Ok(())
    }
    fn snapshot_chunk(&self, from_gix: u16, w: &mut SnapshotWriter, _: u32) -> SnapshotProgress {
        if from_gix == 0 {
            w.section(1);
            w.put_u64(self.count);
        }
        SnapshotProgress::Done
    }
    fn restore(&mut self, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        self.count = r.get_u64()?;
        self.buckets = [EMPTY; BUCKETS];
        Ok(())
    }
}
