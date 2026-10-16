//! Funding carry; the sequencer owns all trading state, the executor owns linked leg repair.
#![forbid(unsafe_code)]
#![deny(
    clippy::float_arithmetic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented,
    clippy::unreachable
)]

pub mod fast;
pub mod params;
use hft_strategy_sdk::prelude::*;
pub use params::{CarryFields, CarryParams};

#[derive(Debug)]
pub struct PortfolioCarry {
    fast: fast::Fast,
}

impl Strategy for PortfolioCarry {
    const DESC: StrategyDesc = StrategyDesc {
        name: "portfolio-carry",
        kind_id: 1,
        version: 2,
        non_prod_only: false,
        schema: &CarryParams::SCHEMA,
        notes: &fast::NOTES,
        report: &fast::REPORT,
        cmds: &[],
    };
    type Params = CarryParams;
    type Model = NoModel<CarryParams>;

    fn new(
        i: StrategyInstance,
        p: &CarryParams,
        g: &GroupTable,
        rd: &RefView<'_>,
    ) -> Result<Self, ParamError> {
        p.check(g, rd)?;
        Ok(Self {
            fast: fast::Fast::new(i.account, p, g, rd),
        })
    }
    fn subscriptions(&self, _: &GroupTable, out: &mut SubsBuilder<'_>) {
        self.fast.subscriptions(out);
    }
    fn on_event<V: Views>(&mut self, e: &EventRef<'_>, v: &V, c: &mut Ctx<'_>) {
        self.fast.on_event(e, v, c);
    }
    fn on_params(&mut self, p: &CarryParams, g: &GroupTable, rd: &RefView<'_>) -> Result<(), ParamError> {
        p.check(g, rd)?;
        self.fast.update(p, g, rd);
        Ok(())
    }
    fn snapshot(&self, w: &mut SnapshotWriter) {
        self.fast.snapshot(w);
    }
    fn restore(&mut self, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        self.fast.restore(r)
    }
}
