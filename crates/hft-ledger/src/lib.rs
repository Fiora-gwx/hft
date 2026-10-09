//! `hft-ledger`: event-sourced equity and PnL books — per (account, venue) cash, spot inventory
//! attribution and margin data, per (account, inst) average-cost positions, per (account, group)
//! PnL and rounds, USDT conversion — and the journal cold consumer `JournalLedger`.
//!
//! Pure: no I/O, no clock, no floating point; tables are allocated by `new`/`restore` and the
//! event entries do not allocate afterwards. Specification: `crates/hft-ledger/README.md`.
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

#[cfg(target_endian = "big")]
compile_error!("hft-ledger: snapshot rows are little-endian only");

mod adjust;
mod attrib;
mod cash;
mod config;
mod dedup;
mod fixed;
mod funding;
mod journal;
mod ledger;
mod margin;
pub mod notes;
mod position;
mod rates;
mod rounds;
pub mod snapshot;
mod types;
pub mod views;
pub use views::Portfolio;

pub use config::{
    AssetName, Capacities, LEDGER_ROW_VERSION, LEDGER_VALIDATOR, LedgerConfig, LedgerConfigRow, PerpVal,
    SpotVal, Valuation, check_ledger_section, default_ref_inst,
};
pub use dedup::{ExtKey, KEY_ORDER_FILL, KEY_TRADE_ID};
pub use fixed::FixedVec;
pub use funding::{FundHistView, HFLAG_PUSHED};
pub use journal::{JournalLedger, JournalLedgerStats, OMS_SECTION_SLOTS, OMS_SLOT_ROW_LEN, SLOT_MAP_CAP};
pub use ledger::{Ledger, NOTE_BUF};
pub use margin::MarginReportView;
pub use notes::NOTE_CODES;
pub use types::*;
