//! Fee-aware two-venue taker strategy. Execution and naked-leg repair belong to the engine.
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

use hft_strategy_sdk::prelude::*;
pub use params::{XtFields, XtParams};

#[derive(Debug)]
pub struct CrossTaker {
    fast: fast::Fast,
}

impl Strategy for CrossTaker {
    const DESC: StrategyDesc = StrategyDesc {
        name: "cross-taker",
        kind_id: 3,
        version: 1,
        non_prod_only: false,
        schema: &XtParams::SCHEMA,
        notes: &desc::NOTES,
        report: &desc::REPORT,
        cmds: &[],
    };
    type Params = XtParams;
    type Model = NoModel<XtParams>;
    fn new(
        inst: StrategyInstance,
        p: &XtParams,
        groups: &GroupTable,
        rd: &RefView<'_>,
    ) -> Result<Self, ParamError> {
        p.validate(groups, rd)?;
        Ok(Self {
            fast: fast::Fast::new(inst.account, p, groups),
        })
    }
    fn subscriptions(&self, groups: &GroupTable, out: &mut SubsBuilder<'_>) {
        for (gix, g) in groups.iter() {
            for l in g.legs.iter().take(2) {
                let _ = out.market(l.inst, gix, EvMask::BBO, Part::Fast);
            }
        }
    }
    fn on_event<V: Views>(&mut self, ev: &EventRef<'_>, v: &V, ctx: &mut Ctx<'_>) {
        self.fast.on_event(ev, v, ctx);
    }
    fn on_params(&mut self, p: &XtParams, groups: &GroupTable, rd: &RefView<'_>) -> Result<(), ParamError> {
        p.validate(groups, rd)?;
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
