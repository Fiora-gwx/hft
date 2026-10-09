"""events(): kind, inst, time window, time source, account, columns, decimals and batching
(TESTS.md PY-020–PY-036)."""

from __future__ import annotations

from datetime import datetime, timezone
from pathlib import Path

import polars as pl
import pytest

import hft_py
from conftest import D, MS, S, Journal, Shm, run_tw, synth

UTC = timezone.utc
T = D + 10 * S + 100 * MS  # k = 10 of inst 101


def df(rdr: object) -> pl.DataFrame:
    return pl.DataFrame(rdr)


@pytest.fixture(scope="module")
def jmd(j_md: Journal) -> object:
    return hft_py.open_journal(j_md.path)


def test_py_020_kind_name_and_number(jmd: object) -> None:
    a = df(jmd.events("Bbo"))  # type: ignore[attr-defined]
    b = df(jmd.events(0x0101))  # type: ignore[attr-defined]
    assert a.height == b.height == 120_000
    assert a.equals(b)


@pytest.mark.parametrize(
    ("row", "kind", "ok"),
    [("PY-021.1", "Bbo2", False), ("PY-021.2", 0x01FF, False), ("PY-021.3", -1, False), ("PY-021.4", 0x10000, False), ("PY-021.5", "Bbo", True)],
)
def test_py_021_unknown_kind(jmd: object, row: str, kind: object, ok: bool) -> None:
    if ok:
        jmd.events(kind)  # type: ignore[attr-defined]
    else:
        with pytest.raises(ValueError):
            jmd.events(kind)  # type: ignore[attr-defined]


def _clock_count() -> int:
    # Generator rule (examples/pyfixture.rs md): one ClockStatus per whole second from D + 1 s up
    # to the last record time + 1 s; the last record is inst 102's Bbo at D + 609.995 s.
    end = D + 10 * S + 59_999 * 10 * MS + 5 * MS
    return (end + S - (D + S)) // S + 1


def test_py_022_inst_filter(jmd: object) -> None:
    e = jmd.events  # type: ignore[attr-defined]
    d1 = df(e("Bbo", inst=101))
    assert d1.height == 60_000 and set(d1["inst"]) == {101}, "PY-022.1"
    assert df(e("Bbo", inst=[101, 102])).height == 120_000, "PY-022.2"
    assert df(e("Bbo", inst=None)).height == 120_000, "PY-022.3"
    d4 = df(e("Bbo", inst=202))
    assert d4.height == 0 and d4.schema == d1.schema, "PY-022.4"
    with pytest.raises(ValueError):
        e("ClockStatus", inst=101)
    assert df(e("ClockStatus")).height == _clock_count(), "PY-022.6"


@pytest.mark.parametrize(
    ("row", "t0", "t1"),
    [
        ("PY-023.1", 1_790_812_810_000_000_000, 1_790_812_820_000_000_000),
        ("PY-023.2", datetime(2026, 10, 1, 0, 0, 10, tzinfo=UTC), datetime(2026, 10, 1, 0, 0, 20, tzinfo=UTC)),
        ("PY-023.3", "2026-10-01T00:00:10Z", "2026-10-01T00:00:20Z"),
        ("PY-023.4", "2026-10-01T08:00:10+08:00", "2026-10-01T08:00:20+08:00"),
        ("PY-023.5", 1_790_812_810_000_000_000, "2026-10-01T00:00:20Z"),
    ],
)
def test_py_023_time_forms(jmd: object, row: str, t0: object, t1: object) -> None:
    d = df(jmd.events("Bbo", inst=101, t0=t0, t1=t1))  # type: ignore[attr-defined]
    assert d.height == 1000, row
    assert d["proc_ns"][0] == D + 10 * S, row


@pytest.mark.parametrize(
    ("row", "t0"),
    [
        ("PY-024.1", datetime(2026, 10, 1, 0, 0, 10)),
        ("PY-024.2", "2026-10-01T00:00:10"),
        ("PY-024.3", "2026-10-01"),
        ("PY-024.4", "10/01/2026 00:00:10 UTC"),
    ],
)
def test_py_024_naive_time_rejected(jmd: object, row: str, t0: object) -> None:
    with pytest.raises(ValueError):
        jmd.events("Bbo", t0=t0, t1="2026-10-01T00:00:20Z")  # type: ignore[attr-defined]


def _ks(d: pl.DataFrame) -> list[int]:
    return [(p - (D + 10 * S)) // (10 * MS) for p in d["proc_ns"]]


@pytest.mark.parametrize(
    ("row", "t0", "t1", "ks"),
    [
        ("PY-025.1", T, T + 1, [10]),
        ("PY-025.2", T + 1, T + 10 * MS, []),
        ("PY-025.3", T - 1, T, []),
        ("PY-025.4", T - 10 * MS, T, [9]),
        ("PY-025.5", T - 10 * MS, T + 1, [9, 10]),
    ],
)
def test_py_025_half_open_bounds(jmd: object, row: str, t0: int, t1: int, ks: list[int]) -> None:
    d = df(jmd.events("Bbo", inst=101, time="proc", t0=t0, t1=t1))  # type: ignore[attr-defined]
    assert _ks(d) == ks, row


def test_py_026_t0_after_t1(jmd: object) -> None:
    with pytest.raises(ValueError):
        jmd.events("Bbo", inst=101, t0=T + 1, t1=T)  # type: ignore[attr-defined]
    assert df(jmd.events("Bbo", inst=101, t0=T, t1=T)).height == 0  # type: ignore[attr-defined]
    assert df(jmd.events("Bbo", inst=101, t0=T, t1=T + 1)).height == 1  # type: ignore[attr-defined]


@pytest.mark.parametrize(("row", "time", "ks"), [("PY-027.1", "proc", [0, 1, 2, 3]), ("PY-027.2", "ex", [1, 2, 3, 4]), ("PY-027.3", "hw", [2, 4])])
def test_py_027_time_sources(jmd: object, row: str, time: str, ks: list[int]) -> None:
    t0 = D + 10 * S
    d = df(jmd.events("Bbo", inst=101, t0=t0, t1=t0 + 40 * MS, time=time))  # type: ignore[attr-defined]
    assert _ks(d) == ks, row


def test_py_028_time_errors(jmd: object) -> None:
    for row, kind, time in [("PY-028.1", "ClockStatus", "ex"), ("PY-028.2", "ClockStatus", "hw"), ("PY-028.3", "Bbo", "utc")]:
        with pytest.raises(ValueError):
            jmd.events(kind, time=time)  # type: ignore[attr-defined]
    jmd.events("ClockStatus", time="proc")  # type: ignore[attr-defined]


def test_py_029_account_filter(j_md: Journal, j_sim: Journal) -> None:
    jp = hft_py.open_journal(j_md.path)
    js = hft_py.open_journal(j_sim.path)
    a = df(jp.events("OrderEvent", account="bn-1"))
    assert a.height == 100 and set(a["account"]) == {1} and set(a["account_kind"]) == {"Live"}, "PY-029.1"
    assert a.equals(df(jp.events("OrderEvent", account=1))), "PY-029.2"
    s = df(js.events("OrderEvent", account="sim-a"))
    assert s.height == df(js.events("OrderEvent")).height and set(s["account_kind"]) == {"Sim"}, "PY-029.3"
    assert s.equals(df(js.events("OrderEvent", account=1))), "PY-029.4"


@pytest.mark.parametrize(
    ("row", "kind", "account"),
    [("PY-030.1", "Bbo", "bn-1"), ("PY-030.2", "OrderEvent", "nobody"), ("PY-030.3", "OrderEvent", "sim-a"), ("PY-030.4", "OrderEvent", 200)],
)
def test_py_030_account_errors(jmd: object, row: str, kind: str, account: object) -> None:
    with pytest.raises(ValueError):
        jmd.events(kind, account=account)  # type: ignore[attr-defined]


def test_py_031_columns(jmd: object) -> None:
    assert df(jmd.events("Bbo", columns=["seq", "bid_px"])).columns == ["seq", "bid_px"]  # type: ignore[attr-defined]
    assert df(jmd.events("Bbo", columns=["bid_px", "seq"])).columns == ["bid_px", "seq"]  # type: ignore[attr-defined]
    for cols in (["nope"], ["seq", "seq"]):
        with pytest.raises(ValueError):
            jmd.events("Bbo", columns=cols)  # type: ignore[attr-defined]


def test_py_032_decimals(jmd: object) -> None:
    e = jmd.events  # type: ignore[attr-defined]
    assert not [c for c in df(e("Bbo", inst=101)).columns if c.endswith("_f")], "PY-032.1"
    d = df(e("Bbo", inst=101, decimals=True))
    r = d.row(0, named=True)
    assert r["bid_px_f"] == 60000.0 and r["ask_px_f"] == pytest.approx(60000.01) and r["bid_qty_f"] == 0.5, "PY-032.2"
    assert d.schema["bid_px_f"] == pl.Float64 and r["bid_px"] == 6_000_000, "PY-032.2"
    assert not [c for c in df(e("ClockStatus", decimals=True)).columns if c.endswith("_f")], "PY-032.3"
    assert not [c for c in df(e("Bbo", inst=101, decimals=False)).columns if c.endswith("_f")], "PY-032.4"


def _write(tools: Path, shm: Shm, tmp: Path, script: str, **kw: int) -> object:
    hot = shm.dir("ev")
    cold = tmp / "cold"
    run_tw(tools, hot, cold, script, **kw)
    return hft_py.open_journal(str(cold / "prod" / "seq"))


def test_py_033_batch_rows_default(tools: Path, shm: Shm, tmp_path: Path) -> None:
    j = _write(tools, shm, tmp_path, "\n".join([synth(1_000_001, D + 10 * S), "seal"]), segment_bytes=256 << 20)
    assert len(j.info()["segments"]) == 1  # type: ignore[attr-defined]
    assert [b.num_rows for b in j.events("Bbo")] == [1_000_000, 1]  # type: ignore[attr-defined]


@pytest.mark.parametrize(
    ("row", "counts", "want"),
    [
        ("PY-034.1", [999], [999]),
        ("PY-034.2", [1000], [1000]),
        ("PY-034.3", [1001], [1000, 1]),
        ("PY-034.4", [600, 600], [600, 600]),
        ("PY-034.5", [0], []),
    ],
)
def test_py_034_batch_bounds(tools: Path, shm: Shm, tmp_path: Path, row: str, counts: list[int], want: list[int]) -> None:
    lines = []
    t = D + 10 * S
    for i, n in enumerate(counts):
        if i:
            lines.append("roll")
        if n:
            lines.append(synth(n, t))
            t += n * MS
        else:
            lines.append("publish")
    lines.append("seal")
    j = _write(tools, shm, tmp_path, "\n".join(lines))
    assert len(j.info()["segments"]) == len(counts), row  # type: ignore[attr-defined]
    assert [b.num_rows for b in j.events("Bbo", batch_rows=1000)] == want, row  # type: ignore[attr-defined]


def test_py_035_batch_rows_invalid(jmd: object) -> None:
    for n in (0, -1):
        with pytest.raises(ValueError):
            jmd.events("Bbo", batch_rows=n)  # type: ignore[attr-defined]
    sizes = [b.num_rows for b in jmd.events("Bbo", batch_rows=1)]  # type: ignore[attr-defined]
    assert len(sizes) == 120_000 and set(sizes) == {1}


def test_py_036_lazy_batches_detached(tools: Path, shm: Shm, tmp_path: Path) -> None:
    script = "\n".join([synth(600, D + 10 * S), "roll", synth(600, D + 11 * S), "roll", synth(600, D + 12 * S), "seal"])
    j = _write(tools, shm, tmp_path, script)
    segs = j.info()["segments"]  # type: ignore[attr-defined]
    p3 = Path(segs[2]["path"])
    raw = bytearray(p3.read_bytes())
    raw[4096:4100] = (len(raw) * 2).to_bytes(4, "little")
    p3.write_bytes(bytes(raw))
    rdr = hft_py.open_journal(str(Path(segs[0]["path"]).parents[1])).events("Bbo", batch_rows=1000)
    b1 = next(rdr)
    b2 = next(rdr)
    assert (b1.num_rows, b2.num_rows) == (600, 600)
    with pytest.raises(hft_py.JournalError) as e:
        next(rdr)
    assert e.value.seq == segs[2]["first_seq"]
    for s in segs:
        Path(s["path"]).unlink()
    assert sum(b1["bid_px"]) == 600 * 6_000_000
