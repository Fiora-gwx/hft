//! `book-observer` (kind_id 6): a market-data-only strategy. It subscribes to the book (and trades)
//! of every group leg so that an instance can pull a venue's data without trading, and keeps
//! per-leg counters and the last top of book. It never sends an intent.
//! Specification: `strategies/book-observer/README.md`.
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

pub use params::{BookObserverFields, BookObserverParams};

/// `StrategyDesc.version` (fast-part snapshot layout and decision semantics).
pub const DESC_VERSION: u16 = 1;

/// The strategy (registry row `book_observer::BookObserver`).
#[derive(Debug)]
pub struct BookObserver {
    pub inst: StrategyInstance,
    pub fast: fast::Fast,
}

impl Strategy for BookObserver {
    const DESC: StrategyDesc = StrategyDesc {
        name: "book-observer",
        kind_id: 6,
        version: DESC_VERSION,
        non_prod_only: false,
        schema: &BookObserverParams::SCHEMA,
        notes: &desc::NOTES,
        report: &desc::REPORT,
        cmds: &desc::CMDS,
    };
    type Params = BookObserverParams;
    type Model = NoModel<BookObserverParams>;

    fn new(
        inst: StrategyInstance,
        p: &BookObserverParams,
        groups: &GroupTable,
        _rd: &RefView<'_>,
    ) -> Result<Self, ParamError> {
        Ok(BookObserver {
            inst,
            fast: fast::Fast::new(p, groups),
        })
    }

    fn subscriptions(&self, groups: &GroupTable, out: &mut SubsBuilder<'_>) {
        fast::subscribe(groups, self.fast.trades, out);
    }

    fn on_event<V: Views>(&mut self, ev: &EventRef<'_>, v: &V, ctx: &mut Ctx<'_>) {
        self.fast.on_event(ev, v, ctx);
    }

    fn on_params(
        &mut self,
        p: &BookObserverParams,
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
