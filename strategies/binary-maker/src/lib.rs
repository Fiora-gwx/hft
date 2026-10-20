//! `binary-maker` (kind_id 7): a maker of binary outcome tokens. It rests a post-only bid at the best
//! bid of the Up and the Down token of the current window while a reference market's realized
//! volatility is high enough, and pulls the quotes near the window end and after a reference jump.
//! Specification: `strategies/binary-maker/README.md`.
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
pub mod vol;

use hft_strategy_sdk::{
    Ctx, EventRef, GroupTable, NoModel, ParamError, RefView, RestoreError, SnapshotReader, SnapshotWriter,
    Strategy, StrategyDesc, StrategyInstance, SubsBuilder, Views,
};

pub use params::{BinaryMakerFields, BinaryMakerParams};

/// `StrategyDesc.version` (fast-part snapshot layout and decision semantics).
pub const DESC_VERSION: u16 = 1;

/// The strategy (registry row `binary_maker::BinaryMaker`).
#[derive(Debug)]
pub struct BinaryMaker {
    pub inst: StrategyInstance,
    pub fast: fast::Fast,
}

impl Strategy for BinaryMaker {
    const DESC: StrategyDesc = StrategyDesc {
        name: "binary-maker",
        kind_id: 7,
        version: DESC_VERSION,
        non_prod_only: false,
        schema: &BinaryMakerParams::SCHEMA,
        notes: &desc::NOTES,
        report: &desc::REPORT,
        cmds: &desc::CMDS,
    };
    type Params = BinaryMakerParams;
    type Model = NoModel<BinaryMakerParams>;

    fn new(
        inst: StrategyInstance,
        p: &BinaryMakerParams,
        groups: &GroupTable,
        _rd: &RefView<'_>,
    ) -> Result<Self, ParamError> {
        Ok(BinaryMaker {
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
        p: &BinaryMakerParams,
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
