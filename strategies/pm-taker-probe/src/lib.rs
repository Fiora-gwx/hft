//! `pm-taker-probe` (kind_id 8): the smallest strategy that drives one taker round end to end on a
//! live account — a spend buy (`OrderIntent::SPEND`, Ioc) at the best ask, then an Ioc sell of the
//! position at the best bid — armed only by a manual `enabled` 0 → 1 change.
//! Specification: `strategies/pm-taker-probe/README.md`.
#![forbid(unsafe_code)]
#![deny(
    clippy::float_arithmetic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

pub mod desc;
pub mod fast;
pub mod params;

use hft_strategy_sdk::{
    Ctx, EventRef, GroupTable, NoModel, ParamError, RefView, RestoreError, SnapshotReader, SnapshotWriter,
    Strategy, StrategyDesc, StrategyInstance, SubsBuilder, Views,
};

pub use params::{PmTakerProbeFields, PmTakerProbeParams};

/// `StrategyDesc.version` (fast-part snapshot layout and decision semantics).
pub const DESC_VERSION: u16 = 1;

/// The strategy (registry row `pm_taker_probe::PmTakerProbe`).
#[derive(Debug)]
pub struct PmTakerProbe {
    pub inst: StrategyInstance,
    pub fast: fast::Fast,
}

impl Strategy for PmTakerProbe {
    const DESC: StrategyDesc = StrategyDesc {
        name: "pm-taker-probe",
        kind_id: 8,
        version: DESC_VERSION,
        non_prod_only: false,
        schema: &PmTakerProbeParams::SCHEMA,
        notes: &desc::NOTES,
        report: &desc::REPORT,
        cmds: &desc::CMDS,
    };
    type Params = PmTakerProbeParams;
    type Model = NoModel<PmTakerProbeParams>;

    fn new(
        inst: StrategyInstance,
        p: &PmTakerProbeParams,
        groups: &GroupTable,
        _rd: &RefView<'_>,
    ) -> Result<Self, ParamError> {
        Ok(PmTakerProbe {
            inst,
            fast: fast::Fast::new(inst.account, p, groups),
        })
    }

    fn subscriptions(&self, groups: &GroupTable, out: &mut SubsBuilder<'_>) {
        fast::subscribe(groups, out);
    }

    fn on_event<V: Views>(&mut self, ev: &EventRef<'_>, v: &V, ctx: &mut Ctx<'_>) {
        self.fast.on_event(ev, v, ctx);
    }

    fn on_params(
        &mut self,
        p: &PmTakerProbeParams,
        groups: &GroupTable,
        _rd: &RefView<'_>,
    ) -> Result<(), ParamError> {
        self.fast.on_params(p, groups);
        Ok(())
    }

    fn snapshot(&self, w: &mut SnapshotWriter) {
        self.fast.snapshot(w);
    }

    fn restore(&mut self, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        self.fast.restore(r)
    }
}
