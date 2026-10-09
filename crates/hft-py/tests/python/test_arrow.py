"""Arrow mapping: golden comparison with the Rust reading, header columns, fixed-point metadata,
variable records, schema versions, schema hash, unknown and test kinds (TESTS.md PY-040–PY-053)."""

from __future__ import annotations

import hashlib
import json
import re
import struct
import sys
from pathlib import Path

import numpy as np
import polars as pl
import pyarrow as pa
import pytest

import hft_py
import jfile
from conftest import D, MS, REPO, S, Journal

sys.path.insert(0, str(REPO / "python"))
from _generated import events as gen  # noqa: E402

VAR_BINARY = {"ControlCmd": "control_cmd", "ConfigBlob": "config_blob", "StateSnapshot": "state_snapshot", "Advice": "advice"}
ENTRY_STRUCT = {"L2Batch": "LEVEL", "L2Snapshot": "LEVEL", "MarginReport": "MARGIN_ITEM", "AccountConfigReport": "LEVERAGE_ITEM"}


def table(rdr: object) -> pa.Table:
    return pa.RecordBatchReader.from_stream(rdr).read_all()


def expected(ref: Path, kind: str) -> list[dict]:
    return [json.loads(line) for line in (ref / f"{kind}.jsonl").open()]


def _cmp(v: object, want: object) -> bool:
    if isinstance(v, (bytes, bytearray)):
        return v.hex() == want
    return v == want


def test_py_040_all_kinds_golden(j_all: Journal, j_all_ref: Path) -> None:
    j = hft_py.open_journal(j_all.path)
    kinds = sorted(p.stem for p in j_all_ref.glob("*.jsonl"))
    assert {"Bbo", "OrderDecision", "ExternalFill", "MarginReport", "ConfigBlob"} <= set(kinds)
    for kind in kinds:
        t = table(j.events(kind))
        exp = expected(j_all_ref, kind)
        assert t.num_rows == len(exp), kind
        paths = {f.name: (f.metadata or {}).get(b"hft.path", b"").decode() for f in t.schema}
        rows = t.to_pylist()
        for r, e in zip(rows, exp):
            assert (r["seq"], r["tsc"], r["flags"]) == (e["seq"], e["tsc"], e["flags"]), kind
            for col, path in paths.items():
                if path:
                    assert _cmp(r[col], e["fields"][path]), (kind, col, r[col], e["fields"][path])
            if "body" in r:
                assert r["body"].hex() == e["body_hex"], kind


def test_py_042_header_columns(j_md: Journal) -> None:
    j = hft_py.open_journal(j_md.path)
    a = pl.DataFrame(j.events("Bbo", inst=101))
    assert {"seq", "tsc", "proc_ns", "flags"} <= set(a.columns), "PY-042.1"
    assert (a["tsc"][0], a["proc_ns"][0]) == (30_000_000_000, 1_790_812_810_000_000_000), "PY-042.1"
    b = pl.DataFrame(j.events("Bbo", inst=102))
    assert b["proc_ns"][0] == 1_790_812_810_005_000_000, "PY-042.2"
    c = pl.DataFrame(j.events("ClockStatus"))
    assert {"seq", "tsc", "proc_ns", "flags"} <= set(c.columns), "PY-042.3"
    off = (a["proc_ns"] - 1_790_812_810_000_000_000).to_list()
    assert off == [i * 10_000_000 for i in range(a.height)], "PY-042.4"


def _md(t: pa.Table, col: str) -> dict[bytes, bytes]:
    return dict(t.schema.field(col).metadata or {})


def test_py_043_fixed_point_metadata(j_all: Journal) -> None:
    j = hft_py.open_journal(j_all.path)
    t = table(j.events("Bbo"))
    assert t.schema.field("bid_px").type == pa.int64()
    assert json.loads(_md(t, "bid_px")[b"px_exp"]) == {"101": -2, "102": -2, "202": -2}, "PY-043.1"
    assert json.loads(_md(t, "bid_qty")[b"qty_exp"]) == {"101": -8, "102": -8, "202": -6}, "PY-043.2"
    t3 = table(j.events("Bbo", inst=202))
    assert json.loads(_md(t3, "bid_qty")[b"qty_exp"]) == {"202": -6}, "PY-043.3"
    oe = table(j.events("OrderEvent"))
    assert oe.schema.field("fee").type == pa.int64()
    md = _md(oe, "fee")
    assert md[b"exp_asset_col"] == b"fee_asset", "PY-043.4"
    assert json.loads(md[b"qty_exp_by_asset"]) == {"1": -8, "2": -6, "3": -6}, "PY-043.4"
    assert t.schema.field("update_id").type == pa.uint64()
    assert b"px_exp" not in _md(t, "update_id") and b"qty_exp" not in _md(t, "update_id"), "PY-043.5"


def _raw_body() -> bytes:
    """A RawRx/RawTx logical body (raw kinds only occur in raw journals, not in ``J_ALL``)."""
    hdr = np.zeros(1, dtype=gen.RAW_RX)
    hdr["venue"] = 2
    hdr["payload_len"] = 5
    return hdr.tobytes() + b"hello"


@pytest.mark.parametrize(
    ("row", "kind"),
    [
        ("PY-044.1", "ControlCmd"),
        ("PY-044.2", "ConfigBlob"),
        ("PY-044.3", "StateSnapshot"),
        ("PY-044.4", "Advice"),
        ("PY-044.5", "RawRx"),
        ("PY-044.6", "RawTx"),
        ("PY-044.7", "ConfigBlob-1"),
    ],
)
def test_py_044_var_kinds_binary(j_all: Journal, j_all_ref: Path, row: str, kind: str) -> None:
    if kind in ("RawRx", "RawTx"):
        d = getattr(hft_py.decode, "raw_rx" if kind == "RawRx" else "raw_tx")(_raw_body())
        assert d["venue"] == 2 and d["payload_len"] == 5 and d["payload"] == b"hello", row
        return
    j = hft_py.open_journal(j_all.path)
    if kind == "ConfigBlob-1":
        b = table(j.events("ConfigBlob")).to_pylist()[0]["body"]
        with pytest.raises(ValueError):
            hft_py.decode.config_blob(b[:-1])
        return
    t = table(j.events(kind))
    dec = getattr(hft_py.decode, VAR_BINARY[kind])
    exp = expected(j_all_ref, kind)
    assert t.num_rows == len(exp) > 0, row
    for r, e in zip(t.to_pylist(), exp):
        body = r["body"]
        assert body.hex() == e["body_hex"], row
        d = dec(body)
        for path, want in e["fields"].items():
            leaf = path.split(".")[-1]
            if leaf in d:
                assert _cmp(d[leaf], want), (row, path)
        if kind == "ConfigBlob":
            assert len(d["payload"]) == d["len"], row
            assert hashlib.sha256(d["payload"]).digest() == d["sha256"], row


def test_py_045_advice_hdr_and_payload(j_all: Journal) -> None:
    j = hft_py.open_journal(j_all.path)
    d = pl.DataFrame(j.events("Advice"))
    r = d.filter(pl.col("basis_seq") == 1000).row(0, named=True)
    assert (r["strategy"], r["account"], r["local_kind"], r["group"], r["len"], r["basis_tsc"]) == (3, 0, 1, 0x0300_0001, 16, 3_000_000_000)
    assert r["payload"] == bytes([1, 0, 0, 0, 0, 0, 0, 0]) + struct.pack("<q", 12345)
    names = {n for n, _ in gen.ADVICE.fields.items() if not n.startswith("_")}
    assert names <= set(d.columns)
    assert not [c for c in d.columns if c.startswith("band")]


def _entries_ref(kind: str, e: dict) -> list[dict]:
    body = bytes.fromhex(e["body_hex"])
    pre = getattr(gen, re.sub(r"(?<!^)(?=[A-Z])", "_", kind).upper())
    dt = getattr(gen, ENTRY_STRUCT[kind])
    p = np.frombuffer(body[: pre.itemsize], dtype=pre)[0]
    if kind in ("L2Batch", "L2Snapshot"):
        n = int(p["n_bids"]) + int(p["n_asks"])
    elif kind == "MarginReport":
        n = int(p["n"])
    else:
        n = int(p["n_lev"])
    arr = np.frombuffer(body[pre.itemsize : pre.itemsize + n * dt.itemsize], dtype=dt)
    return [{k: int(a[k]) for k in dt.names if not k.startswith("_")} for a in arr]


@pytest.mark.parametrize(
    ("row", "kind"), [("PY-046.1", "L2Batch"), ("PY-046.2", "L2Snapshot"), ("PY-046.3", "MarginReport"), ("PY-046.4", "AccountConfigReport")]
)
def test_py_046_var_prefix_and_list(j_all: Journal, j_all_ref: Path, row: str, kind: str) -> None:
    j = hft_py.open_journal(j_all.path)
    t = table(j.events(kind))
    assert pa.types.is_list(t.schema.field("entries").type), row
    for r, e in zip(t.to_pylist(), expected(j_all_ref, kind)):
        assert r["entries"] == _entries_ref(kind, e), row
    if kind == "MarginReport":
        assert [] in [r["entries"] for r in t.to_pylist()], "PY-046.5"


def test_py_047_multipart_one_row(j_all: Journal, j_all_ref: Path) -> None:
    j = hft_py.open_journal(j_all.path)
    d = pl.DataFrame(j.events("ControlCmd"))
    big = [e for e in expected(j_all_ref, "ControlCmd") if len(e["body_hex"]) > 2 * 65_488 * 2]
    assert len(big) == 1
    s = big[0]["seq"]
    row = d.filter(pl.col("seq") == s)
    assert row.height == 1
    assert row["body"][0].hex() == big[0]["body_hex"]
    assert d.filter(pl.col("seq").is_in([s + 1, s + 2])).height == 0


def _md_copy(j_md: Journal, tmp: Path) -> tuple[Journal, Path, bytes]:
    j = j_md.copy(tmp)
    seg = j.segments()[0]
    return j, seg, seg.read_bytes()


def test_py_048_new_reads_old(j_md: Journal, tmp_path: Path) -> None:
    j, seg, raw = _md_copy(j_md, tmp_path)
    ver, size = jfile.get_schema(raw, 0x0101)
    out = jfile.resize_kind(raw, 0x0101, 88)
    jfile.set_schema(out, 0x0101, ver - 1, 88)
    seg.write_bytes(bytes(out))
    orig = pl.DataFrame(hft_py.open_journal(j_md.path).events("Bbo", inst=101))
    d = pl.DataFrame(hft_py.open_journal(j.path).events("Bbo", inst=101))
    assert d.height == 60_000
    assert set(d["update_id"]) == {0}
    names = [c for c in d.columns if c not in ("seq", "tsc", "proc_ns", "flags", "account_kind", "_missing")]
    bit = 1 << names.index("update_id")
    assert set(d["_missing"]) == {bit}
    for c in names:
        if c != "update_id":
            assert d[c].equals(orig[c]), c


def test_py_049_old_reads_new(j_md: Journal, tmp_path: Path) -> None:
    j, seg, raw = _md_copy(j_md, tmp_path)
    ver, size = jfile.get_schema(raw, 0x0101)
    out = jfile.resize_kind(raw, 0x0101, 104, fill=b"\xff")
    jfile.set_schema(out, 0x0101, ver + 1, 104)
    seg.write_bytes(bytes(out))
    orig = pl.DataFrame(hft_py.open_journal(j_md.path).events("Bbo", inst=101))
    jj = hft_py.open_journal(j.path)
    d = pl.DataFrame(jj.events("Bbo", inst=101))
    assert d.height == 60_000 and d.columns == orig.columns
    for c in d.columns:
        if c not in ("seq", "tsc", "proc_ns", "flags"):
            assert d[c].equals(orig[c]), c
    sch = jj.info()["schema"]
    assert len(sch) == 1 and sch[0]["kind"] == "Bbo"
    assert (sch[0]["journal_version"], sch[0]["reader_version"]) == (ver + 1, ver)


@pytest.mark.parametrize(
    ("row", "dver", "dsize", "ok"),
    [("PY-050.1", 0, 8, False), ("PY-050.2", 0, -8, False), ("PY-050.3", 0, 0, True), ("PY-050.4", 1, 8, True)],
)
def test_py_050_same_version_size_mismatch(j_md: Journal, tmp_path: Path, row: str, dver: int, dsize: int, ok: bool) -> None:
    j, seg, raw = _md_copy(j_md, tmp_path)
    ver, size = jfile.get_schema(raw, 0x0101)
    out = bytearray(raw)
    jfile.set_schema(out, 0x0101, ver + dver, size + dsize)
    seg.write_bytes(bytes(out))
    if ok:
        jj = hft_py.open_journal(j.path)
        if dver == 0:
            assert jj.info()["schema"] == [], row
        else:
            assert pl.DataFrame(jj.events("Bbo")).height == 120_000, row
    else:
        with pytest.raises(hft_py.SchemaError):
            hft_py.open_journal(j.path)


def _catalog_md() -> list[dict]:
    text = (REPO / "docs" / "generated" / "event-catalog.md").read_text()
    rows = []
    for m in re.finditer(r"^### 0x([0-9A-F]{4}) (\w+)\n(.*?)(?=^### |\Z)", text, re.M | re.S):
        body = m.group(3)
        layout = re.search(r"- 布局:(.+)", body)
        role = re.search(r"- 角色:(.+)", body)
        if layout and role:
            rows.append({"kind": int(m.group(1), 16), "name": m.group(2), "layout": layout.group(1).strip(), "role": role.group(1).strip()})
    return rows


def _fnv(kinds: list[tuple[int, int, int]]) -> int:
    h = 0xCBF29CE484222325
    for k, v, s in kinds:
        for x in struct.pack("<HHI", k, v, s):
            h ^= x
            h = (h * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


def test_py_051_schema_hash_catalog() -> None:
    h = hft_py.schema_hash()
    c = hft_py.schema.catalog()
    table_ = [(k, v, int(lay.split()[1])) for k, (_, v, lay, _d) in sorted(gen.KINDS.items())]
    assert h == _fnv(table_) == gen.SCHEMA_HASH
    md = _catalog_md()
    assert len(c) == len(md) == len(gen.KINDS)
    for row, m in zip(c, md):
        assert (row["kind"], row["name"], row["layout"], row["role"]) == (m["kind"], m["name"], m["layout"], m["role"])


def test_py_052_dev_kind_rejected(j_md: Journal, tmp_path: Path) -> None:
    j, seg, raw = _md_copy(j_md, tmp_path)
    out = bytearray(raw)
    recs = list(jfile.records(raw))
    r = recs[5000]
    struct.pack_into("<H", out, r.off + 4, 0x7F01)
    seg.write_bytes(bytes(out))
    got: list[int] = []
    with pytest.raises(hft_py.JournalError) as e:
        for b in hft_py.open_journal(j.path).events("Bbo"):
            got.extend(b["seq"])
    assert e.value.seq == r.seq
    assert all(s < r.seq for s in got)


def test_py_053_unknown_kind_skipped(j_md: Journal, tmp_path: Path) -> None:
    j, seg, raw = _md_copy(j_md, tmp_path)
    out = bytearray(raw)
    n = 0
    for r in jfile.records(raw):
        if r.kind == 0x0104 and n < 100:
            struct.pack_into("<H", out, r.off + 4, 0x01FE)
            n += 1
    seg.write_bytes(bytes(out))
    jj = hft_py.open_journal(j.path)
    assert pl.DataFrame(jj.events("Bbo")).height == 120_000
    assert pl.DataFrame(jj.events("Trade")).height == 500
