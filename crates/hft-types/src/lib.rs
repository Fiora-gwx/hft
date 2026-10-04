//! `hft-types`: the system-wide numeric and identifier vocabulary — per-asset fixed point
//! (`Px`, `Qty`, `Notional`, `Rate`), byte-wise decimal parsing and formatting, rounding to
//! tick/lot, U256 ↔ `Qty`, ID newtypes, trading enums, wire codes (`KillBits`, `RunMode`,
//! `KillLevel`, `CapSource`, `RejectBits`), `ClientOrderId` and pure L1 helpers.
//!
//! No I/O, no threads, no allocation, no floating point; no function panics on any input.
//! Specification: `crates/hft-types/README.md`.
#![forbid(unsafe_code)]
#![deny(
    clippy::arithmetic_side_effects,
    clippy::float_arithmetic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

mod clid;
mod err;
mod fixed;
mod format;
mod parse;
mod types;
mod u256;
mod venue_ids;
mod wire;

pub use clid::{CidErr, ClientOrderId};
pub use err::FxErr;
pub use fixed::{cap_cross_px, funding_settle_norm_ns, pair_lot};
pub use format::write_fixed;
pub use parse::{parse_exact, parse_round};
pub use types::{
    ACCOUNT_ALL, ASSET_NONE, AccountId, AssetExp, AssetId, ConnId, GROUP_ALL, GROUP_NONE, GroupId,
    HedgePxLimits, HostClass, INST_ALL, INST_NONE, InstId, MAX_BBO_CONN_SLOTS, MAX_RAW, MarketKind, Notional,
    Px, Qty, Rate, RoundMode, Seq, Side, SlotId, StrategyId, StrategyInstanceId, Tif, Tsc, VenueId,
};
pub use u256::{U256Limbs, qty_from_u256, qty_to_u256};
pub use wire::{CapSource, KillBits, KillLevel, RejectBits, RunMode};
