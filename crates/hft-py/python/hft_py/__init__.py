"""hft_py: read-only journal access (Arrow), ledger/PnL and the strategy parameter codec.

The implementation is the Rust extension ``hft_py._native`` (crates/hft-py); this package only
re-exports its public names. Specification: crates/hft-py/README.md.
"""

from hft_py import _native
from hft_py._native import (
    JournalError,
    ParamsError,
    SchemaError,
    decode,
    heartbeat,
    ledger,
    open_journal,
    params,
    pnl,
    schema,
    schema_hash,
)

__all__ = [
    "JournalError",
    "ParamsError",
    "SchemaError",
    "decode",
    "heartbeat",
    "ledger",
    "open_journal",
    "params",
    "pnl",
    "schema",
    "schema_hash",
]

if hasattr(_native, "_testing"):
    _testing = _native._testing
