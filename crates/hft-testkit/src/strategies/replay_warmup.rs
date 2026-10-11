//! Replay fixture whose Advice exposes every warmup delivery, including order and last flags.
use hft_strategy_sdk::prelude::*;
use hft_strategy_sdk::{
    AdviceOut, Ctx, ElemDesc, ElemTy, EventRef, GroupTable, LenRule, Model, ModelLayoutDesc,
    ModelParamEffect, ModelViews, ParamError, RefView, RestoreError, SectionDesc, SectionScope,
    SnapshotProgress, SnapshotReader, SnapshotWriter, Strategy, StrategyDesc, StrategyInstance, SubsBuilder,
    Views,
};

use super::replay_probe::{ReplayA, ReplayParams};

#[derive(Debug)]
pub struct ReplayWarmup(ReplayA);

impl Strategy for ReplayWarmup {
    const DESC: StrategyDesc = StrategyDesc {
        name: "replay-warmup",
        kind_id: 0xFF05,
        ..ReplayA::DESC
    };
    type Params = ReplayParams;
    type Model = WarmupModel;
    fn new(
        i: StrategyInstance,
        p: &ReplayParams,
        g: &GroupTable,
        r: &RefView<'_>,
    ) -> Result<Self, ParamError> {
        ReplayA::new(i, p, g, r).map(Self)
    }
    fn subscriptions(&self, g: &GroupTable, out: &mut SubsBuilder<'_>) {
        self.0.subscriptions(g, out);
    }
    fn on_event<V: Views>(&mut self, e: &EventRef<'_>, v: &V, c: &mut Ctx<'_>) {
        self.0.on_event(e, v, c);
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

static LAYOUT: ModelLayoutDesc = ModelLayoutDesc {
    sections: &[SectionDesc {
        tag: 1,
        name: "warmup",
        scope: SectionScope::Instance,
        elems: &[
            ElemDesc {
                name: "count",
                ty: ElemTy::U64,
                len: LenRule::One,
            },
            ElemDesc {
                name: "order",
                ty: ElemTy::U64,
                len: LenRule::One,
            },
            ElemDesc {
                name: "sum",
                ty: ElemTy::U64,
                len: LenRule::One,
            },
            ElemDesc {
                name: "last_mask",
                ty: ElemTy::U64,
                len: LenRule::One,
            },
            ElemDesc {
                name: "bbo_count",
                ty: ElemTy::U64,
                len: LenRule::One,
            },
        ],
    }],
};

/// Five independently observable integers; no hidden state or process-global counters.
#[derive(Debug, Default)]
pub struct WarmupModel {
    words: [u64; 5],
}

impl Model for WarmupModel {
    type Params = ReplayParams;
    const LAYOUT_VERSION: u32 = 1;
    const LAYOUT: &'static ModelLayoutDesc = &LAYOUT;
    const PATH_VERSION: u32 = 0;
    const WARMUP: bool = true;
    fn new(_: StrategyInstance, _: &ReplayParams, _: &GroupTable, _: &RefView<'_>) -> Self {
        Self::default()
    }
    fn mem_bytes(_: &ReplayParams, _: u16) -> u64 {
        40
    }
    fn snapshot_bytes(_: &ReplayParams, _: u16) -> u64 {
        40
    }
    fn path_settle_ms(_: &ReplayParams) -> u64 {
        0
    }
    fn cold_start_ms(_: &ReplayParams, _: u16) -> u64 {
        0
    }
    fn on_params(&mut self, _: &ReplayParams, _: &GroupTable, _: &RefView<'_>) -> ModelParamEffect {
        ModelParamEffect::None
    }
    fn on_event<V: ModelViews>(&mut self, e: &EventRef<'_>, _: &V, out: &mut AdviceOut<'_>) {
        match e {
            EventRef::Warmup {
                local_kind: 9,
                chunk_no,
                last,
                payload,
            } => {
                self.words[0] = self.words[0].wrapping_add(1);
                self.words[1] = self.words[1]
                    .wrapping_mul(256)
                    .wrapping_add(u64::from(*chunk_no) + 1);
                let value = (*payload).try_into().map(u64::from_le_bytes).unwrap_or(0);
                self.words[2] = self.words[2].wrapping_add(value);
                if *last {
                    self.words[3] |= 1u64.checked_shl(*chunk_no).unwrap_or(0);
                }
            }
            EventRef::Bbo { .. } => self.words[4] = self.words[4].wrapping_add(1),
            _ => return,
        }
        let mut payload = [0; 40];
        for (chunk, word) in payload.chunks_exact_mut(8).zip(self.words) {
            chunk.copy_from_slice(&word.to_le_bytes());
        }
        let _ = out.emit_raw(GROUP_NONE, 1, &payload);
    }
    fn snapshot_path(&self, _: &mut SnapshotWriter) {}
    fn restore_path(&mut self, _: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        Ok(())
    }
    fn snapshot_chunk(&self, from: u16, w: &mut SnapshotWriter, _: u32) -> SnapshotProgress {
        if from == 0 {
            w.section(1);
            for word in self.words {
                w.put_u64(word);
            }
        }
        SnapshotProgress::Done
    }
    fn restore(&mut self, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        let mut words = [0; 5];
        for word in &mut words {
            *word = r.get_u64()?;
        }
        self.words = words;
        Ok(())
    }
}
