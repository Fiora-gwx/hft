"""Zero-copy hand-over of the Arrow buffers to Polars, pyarrow and DuckDB (TESTS.md PY-055, PY-056).
Needs the ``testing`` build (``hft_py._testing``)."""

from __future__ import annotations

import duckdb
import polars as pl
import pyarrow as pa
import pytest

import hft_py
from conftest import Journal

testing = getattr(hft_py, "_testing", None)


def _need_testing() -> None:
    assert testing is not None, "hft_py was built without the `testing` feature"


def test_py_055_polars_zero_copy(j_md: Journal) -> None:
    _need_testing()
    j = hft_py.open_journal(j_md.path)
    df = pl.DataFrame(j.events("Bbo", inst=101, batch_rows=60000))
    a = testing.last_buffer_addrs()
    for col in ("bid_px", "ask_px", "bid_qty", "seq"):
        assert df[col].to_numpy(allow_copy=False).ctypes.data == a[col], col


@pytest.mark.parametrize("row", ["PY-056.1", "PY-056.2"])
def test_py_056_pyarrow_duckdb(j_md: Journal, row: str) -> None:
    _need_testing()
    j = hft_py.open_journal(j_md.path)
    rdr = j.events("Bbo", inst=101, batch_rows=60000)
    if row == "PY-056.1":
        b = pa.RecordBatchReader.from_stream(rdr).read_next_batch()
        assert b.column("bid_px").buffers()[1].address == testing.last_buffer_addrs()["bid_px"]
    else:
        assert duckdb.sql("select count(*), sum(bid_px) from rdr").fetchone() == (60000, 360000000000)
