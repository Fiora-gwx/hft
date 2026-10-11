//! A real, stateless Model: empty layout and zero declarations must not mean NoModel.
use crate::common::{EchoProbe, TPParams};
use hft_strategy_sdk::*;

#[derive(Debug)]
pub struct Stateless;
#[derive(Debug)]
pub struct EmptyLayout;

impl Strategy for Stateless {
    const DESC: StrategyDesc = EchoProbe::DESC;
    type Params = TPParams;
    type Model = EmptyLayout;
    fn new(_: StrategyInstance, _: &TPParams, _: &GroupTable, _: &RefView<'_>) -> Result<Self, ParamError> {
        Ok(Self)
    }
    fn subscriptions(&self, _: &GroupTable, _: &mut SubsBuilder<'_>) {}
    fn on_event<V: Views>(&mut self, _: &EventRef<'_>, _: &V, _: &mut Ctx<'_>) {}
    fn on_params(&mut self, _: &TPParams, _: &GroupTable, _: &RefView<'_>) -> Result<(), ParamError> {
        Ok(())
    }
    fn snapshot(&self, _: &mut SnapshotWriter) {}
    fn restore(&mut self, _: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        Ok(())
    }
}

impl Model for EmptyLayout {
    type Params = TPParams;
    const LAYOUT_VERSION: u32 = 0;
    const LAYOUT: &'static ModelLayoutDesc = &ModelLayoutDesc::EMPTY;
    const PATH_VERSION: u32 = 0;
    fn mem_bytes(_: &TPParams, _: u16) -> u64 {
        0
    }
    fn snapshot_bytes(_: &TPParams, _: u16) -> u64 {
        0
    }
    fn path_settle_ms(_: &TPParams) -> u64 {
        0
    }
    fn cold_start_ms(_: &TPParams, _: u16) -> u64 {
        0
    }
    fn new(_: StrategyInstance, _: &TPParams, _: &GroupTable, _: &RefView<'_>) -> Self {
        Self
    }
    fn on_params(&mut self, _: &TPParams, _: &GroupTable, _: &RefView<'_>) -> ModelParamEffect {
        ModelParamEffect::None
    }
    fn on_event<V: ModelViews>(&mut self, _: &EventRef<'_>, v: &V, out: &mut AdviceOut<'_>) {
        let _ = out.emit_raw(hft_types::GROUP_NONE, 1, &v.now().seq.to_le_bytes());
    }
    fn snapshot_path(&self, _: &mut SnapshotWriter) {}
    fn restore_path(&mut self, _: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        Ok(())
    }
    fn snapshot_chunk(&self, _: u16, _: &mut SnapshotWriter, _: u32) -> SnapshotProgress {
        SnapshotProgress::Done
    }
    fn restore(&mut self, _: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        Ok(())
    }
}

hft_strategy_sdk::strategy_set! { pub struct ZeroSet { Stateless } }
