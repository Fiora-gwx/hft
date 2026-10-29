//! `hft-feature`: event-driven feature nodes for strategy fast parts (README "概要").
//!
//! A feature is a **handler** that sees every event of the kinds it declares ([`Feature::EVENTS`])
//! and updates its state in O(1) or bounded O(k), plus **evals** that compute values on demand
//! (inherent query methods, and [`Feature::export`] for the exported columns). Nodes are
//! fixed-point (no f64), allocate only in `new`, read no clock (time comes from the caller) and own
//! no strategy threshold: every threshold is a call parameter. A strategy composes nodes into its
//! own feature set with a fixed per-event-kind plan (README "组合与执行计划").
//! Specification: `crates/hft-feature/README.md`.
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

pub mod depth;
pub mod fixed;
pub mod flow;
pub mod hist;
pub mod lag;
pub mod norm;
pub mod top;
pub mod vol;
pub mod zscore;

pub use depth::{Own, OwnExcludedSide};
pub use fixed::{LN2_NANO, ONE, atanh2, imbalance, isqrt, ln_rate, ln_ratio, quantile_x10};
pub use flow::SignedFlow;
pub use hist::SecHist;
pub use lag::{LagInput, LagQuantiles};
pub use norm::{cdf, inv_cdf};
pub use top::{BboTop, TopInput};
pub use vol::RealizedVol;
pub use zscore::zsum;

/// Event kinds a handler must see (bit set). `ANY` = every event the strategy receives.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct Events(pub u16);

impl Events {
    pub const NONE: Events = Events(0);
    pub const BBO: Events = Events(1);
    pub const BOOK: Events = Events(2);
    pub const TRADE: Events = Events(4);
    pub const ORDER: Events = Events(8);
    pub const TIMER: Events = Events(16);
    /// Every event (time-driven nodes advance on any event).
    pub const ANY: Events = Events(0x8000);
    pub const fn contains(self, o: Events) -> bool {
        self.0 & o.0 == o.0
    }
    pub const fn union(self, o: Events) -> Events {
        Events(self.0 | o.0)
    }
}

/// Column descriptors, `NA` and the export row are the SDK's (`Strategy::FEATURES`,
/// `Strategy::export_features`), re-exported here unchanged.
pub use hft_strategy_sdk::features::{ColumnDesc, FeatureRow, NA, Unit};

/// A feature node: a handler over its `Input` (called for every event of `EVENTS`) and the export
/// of its evals (`COLUMNS` order).
pub trait Feature {
    /// What the handler consumes, extracted by the caller from the event and the views.
    type Input<'a>;
    const EVENTS: Events;
    const COLUMNS: &'static [ColumnDesc];
    fn on_event(&mut self, x: Self::Input<'_>);
    fn export(&self, row: &mut FeatureRow<'_>);
}
