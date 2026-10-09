//! `hft-py`: the PyO3 extension `hft_py` (README). Read-only journal → Arrow, ledger/PnL through
//! hft-ledger's `JournalLedger`, SDK params codec, read-only heartbeat. Cold component: no hot-path
//! constraints; no write API to a journal or to the hot process exists here.
//! Specification: `crates/hft-py/README.md`.
#![deny(
    unsafe_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

pub mod arrow_map;
pub mod cols;
pub mod config;
pub mod decode;
pub mod error;
pub mod heartbeat;
pub mod journal;
pub mod ledger;
pub mod params;
pub mod py_journal;
pub mod py_ledger;
pub mod schema;
pub mod testing;
pub mod time;

use pyo3::prelude::*;

pub use error::{Error, JournalError, ParamsError, Res, SchemaError};

/// Registers a submodule as `hft_py.<name>` (attribute and `sys.modules` entry).
fn add_sub(py: Python<'_>, parent: &Bound<'_, PyModule>, m: Bound<'_, PyModule>) -> PyResult<()> {
    let name = m.name()?.to_string();
    parent.add_submodule(&m)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item(format!("hft_py.{name}"), &m)?;
    Ok(())
}

/// The extension module (`hft_py._native`; the package `hft_py` re-exports its public names).
#[pymodule]
#[pyo3(name = "_native")]
fn native(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("JournalError", py.get_type::<JournalError>())?;
    m.add("SchemaError", py.get_type::<SchemaError>())?;
    m.add("ParamsError", py.get_type::<ParamsError>())?;
    m.add_function(wrap_pyfunction!(py_journal::open_journal, m)?)?;
    m.add_function(wrap_pyfunction!(schema::schema_hash_py, m)?)?;
    m.add_function(wrap_pyfunction!(py_ledger::ledger_py, m)?)?;
    m.add_function(wrap_pyfunction!(py_ledger::pnl_py, m)?)?;
    add_sub(py, m, params::module(py)?)?;
    add_sub(py, m, heartbeat::module(py)?)?;
    add_sub(py, m, decode::module(py)?)?;
    add_sub(py, m, schema::module(py)?)?;
    if let Some(t) = testing::module(py)? {
        add_sub(py, m, t)?;
    }
    Ok(())
}
