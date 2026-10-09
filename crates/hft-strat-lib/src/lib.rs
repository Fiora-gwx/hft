//! `hft-strat-lib`: building blocks that strategy crates may reuse (README "构件总表").
//!
//! Fast-part components (fixed point, no f64, no allocation, O(1) or bounded O(k)) are called from
//! `Strategy::on_event` on the sequencer thread: [`SpreadCalc`] and [`CostLine`], [`TriggerPx`],
//! [`ConfirmWindow`], [`DirtyFilter`], [`Throttle`], [`ImpactModel::vwap`] and [`Expiring`]. They
//! own no strategy state (they are field types or pure functions), do no I/O and read no clock:
//! times come from the caller (`Views::now()` and the event). No component carries a strategy's
//! threshold; every threshold is a call parameter.
//!
//! Stateful fast-part components store engine_ns only (their snapshots go into the strategy
//! snapshot); `DirtyFilter` compares this run's TSC values of the SDK `BboView`.
//!
//! Model-only components (`FundingStats`, `PercentileStats`, `LeadLag`) arrive with milestone M4.
//! Specification: `crates/hft-strat-lib/README.md`.
#![forbid(unsafe_code)]
#![deny(
    clippy::float_arithmetic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented,
    clippy::indexing_slicing
)]

pub mod confirm;
pub mod cost;
pub mod dirty;
pub mod expiring;
pub mod impact;
pub mod spread;
pub mod throttle;
pub mod trigger;
pub mod units;

pub use confirm::{Confirm, ConfirmParams, ConfirmParamsNs, ConfirmWindow};
pub use cost::{CostErr, CostItem, CostLine, FeeErr, FeeLeg, Liquidity, cost_flags, fee_round_trip};
pub use dirty::{DirtyFilter, DirtyParams, DirtyParamsTsc, DirtyReason, Freshness, LegFresh};
pub use expiring::{Expiring, Fresh};
pub use impact::{ImpactModel, Vwap, VwapParams};
pub use spread::SpreadCalc;
pub use throttle::{
    FireKind, Outcome, OutcomeAction, Throttle, ThrottleParams, ThrottleParamsNs, ThrottleReason,
};
pub use trigger::TriggerPx;
