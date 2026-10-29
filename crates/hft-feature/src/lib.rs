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

/// Unit of an exported column; the column value is `raw × 10^exp` of that unit.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum Unit {
    /// Dimensionless (returns, imbalances, probabilities, standard deviations).
    Ratio,
    /// Price in the inst's `px_exp` (exp is relative to it).
    Px,
    /// Quantity in the inst's `qty_exp` (exp is relative to it).
    Qty,
    Ns,
    Count,
    /// UTC minute number (minutes since 1970-01-01).
    Minute,
}

/// One exported column: name, unit and decimal exponent. `NA` marks a missing value.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct ColumnDesc {
    pub name: &'static str,
    pub unit: Unit,
    pub exp: i8,
}

/// Missing value of an exported column.
pub const NA: i64 = i64::MIN;

/// A row of exported fixed-point columns, written in `COLUMNS` order into caller-owned storage.
#[derive(Debug)]
pub struct FeatureRow<'a> {
    vals: &'a mut [i64],
    n: usize,
}

impl<'a> FeatureRow<'a> {
    pub fn new(vals: &'a mut [i64]) -> FeatureRow<'a> {
        FeatureRow { vals, n: 0 }
    }
    /// Appends one value; values beyond the storage are dropped (counted in `len`).
    pub fn put(&mut self, v: i64) {
        if let Some(x) = self.vals.get_mut(self.n) {
            *x = v;
        }
        self.n += 1;
    }
    /// Appends `v` or `NA`.
    pub fn put_opt(&mut self, v: Option<i64>) {
        self.put(v.unwrap_or(NA));
    }
    /// Values written (may exceed the storage length).
    pub fn len(&self) -> usize {
        self.n
    }
    pub fn is_empty(&self) -> bool {
        self.n == 0
    }
}

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
