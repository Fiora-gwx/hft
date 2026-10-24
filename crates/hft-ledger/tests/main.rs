//! hft-ledger integration tests as one binary: one module per test file (files, function names
//! and TESTS.md locations unchanged); files with process-wide effects are separate targets.
#![allow(
    clippy::disallowed_types,
    clippy::needless_range_loop,
    clippy::too_many_arguments
)] // allow-list: test code, not a hot path (crate level, as in the former per-file binaries)
mod adjust;
mod api;
mod balance;
mod cold_reader;
mod common;
mod day_loss;
mod equity;
mod external;
mod fill;
mod funding;
mod funding_norm;
mod funding_recon;
mod journal_ledger;
mod layout;
mod margin;
mod position;
mod prop_adjust;
mod prop_funding;
mod prop_journal_ledger;
mod prop_ledger;
mod prop_margin;
mod prop_snapshot;
mod rates;
mod reconcile_cash;
mod rounds;
mod settlement;
mod settlement_journal;
mod snapshot;
mod transfer;
mod upnl;
