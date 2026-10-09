"""Damaged journals and inputs that must surface as Python exceptions (TESTS.md PY-060–PY-065)."""

from __future__ import annotations

import os
import struct
from pathlib import Path

import polars as pl
import pytest

import hft_py
import jfile
from conftest import D, S, Journal, Shm, Writer, synth


def _first(raw: bytes, kind: int) -> jfile.Rec:
    return next(r for r in jfile.records(raw) if r.kind == kind)


def test_py_060_bad_record_crc(j_all: Journal, tmp_path: Path) -> None:
    j = j_all.copy(tmp_path)
    segs = j.segments()
    seg, r = next((s, _first(s.read_bytes(), 0x0401)) for s in segs if any(x.kind == 0x0401 for x in jfile.records(s.read_bytes())))
    raw = bytearray(seg.read_bytes())
    raw[r.off + 24 + 3] ^= 0x10
    seg.write_bytes(bytes(raw))
    jj = hft_py.open_journal(j.path)
    with pytest.raises(hft_py.JournalError) as e:
        list(jj.events("ControlCmd"))
    assert e.value.seq == r.seq and e.value.path == str(seg)
    got: list[int] = []
    with pytest.raises(hft_py.JournalError) as e2:
        for b in jj.events("Bbo"):
            got.extend(b["seq"])
    assert e2.value.seq == r.seq
    ref = [s for s in pl.DataFrame(hft_py.open_journal(j_all.path).events("Bbo"))["seq"] if s < r.seq]
    assert got == ref


@pytest.mark.parametrize(("row", "seg_ix", "magic"), [("PY-061.1", 0, False), ("PY-061.2", 1, False), ("PY-061.3", 1, True)])
def test_py_061_bad_header_crc(j_all: Journal, tmp_path: Path, row: str, seg_ix: int, magic: bool) -> None:
    j = j_all.copy(tmp_path)
    segs = j.segments()
    seg = segs[seg_ix]
    raw = bytearray(seg.read_bytes())
    if magic:
        raw[0:8] = b"\0" * 8
    else:
        raw[100] ^= 0x01
    seg.write_bytes(bytes(raw))
    got: list[int] = []
    with pytest.raises(hft_py.JournalError) as e:
        jj = hft_py.open_journal(j.path)
        for b in jj.events("Bbo"):
            got.extend(b["seq"])
    assert e.value.path == str(seg), row
    if seg_ix == 0:
        assert got == [], row
    else:
        full = pl.DataFrame(hft_py.open_journal(j_all.path).events("Bbo"))
        first2 = int(segs[1].name.split("-")[1].split(".")[0])
        assert got == [s for s in full["seq"] if s < first2], row


def test_py_062_truncated_cold_copy(j_md: Journal, tmp_path: Path) -> None:
    j = j_md.copy(tmp_path)
    seg = j.segments()[0]
    raw = seg.read_bytes()
    r = list(jfile.records(raw))[70_000 - 1]
    os.truncate(seg, r.off + r.len // 2)
    got: list[int] = []
    with pytest.raises(hft_py.JournalError) as e:
        for b in hft_py.open_journal(j.path).events("Bbo"):
            got.extend(b["seq"])
    assert e.value.seq == r.seq
    ref = [s for s in pl.DataFrame(hft_py.open_journal(j_md.path).events("Bbo"))["seq"] if s < r.seq]
    assert got == ref


def test_py_063_beyond_committed_len(tools: Path, shm: Shm, tmp_path: Path) -> None:
    hot = shm.dir("py063")
    cold = tmp_path / "cold"
    w = Writer(tools, hot, cold, publish_every=1_000_000)
    w.send(synth(100, D + 10 * S), "publish", "flush", synth(10, D + 11 * S), "abort")
    w.close()
    seg = hot / "prod" / "seq" / f"{1:010d}.jrn"
    raw = bytearray(seg.read_bytes())
    committed = struct.unpack_from("<Q", raw, 3072)[0]
    at = committed + 10 * 120 + 64
    raw[at : at + 4096] = os.urandom(4096)
    seg.write_bytes(bytes(raw))
    j = hft_py.open_journal(str(cold / "prod" / "seq"), hot_root=str(hot))
    assert pl.DataFrame(j.events("Bbo")).height == 100


def _rss_kib() -> int:
    for line in Path("/proc/self/status").read_text().splitlines():
        if line.startswith("VmRSS:"):
            return int(line.split()[1])
    return 0


def test_py_064_reassembly_cap(j_all: Journal, tmp_path: Path) -> None:
    j = j_all.copy(tmp_path)
    for seg in j.segments():
        raw = bytearray(seg.read_bytes())
        parts = [r for r in jfile.records(bytes(raw)) if r.kind == 0x0401 and (r.flags & jfile.FLAG_PART)]
        if not parts:
            continue
        r = parts[0]
        struct.pack_into("<I", raw, r.off + 24, (256 << 20) + 1)
        jfile.set_record_crc(raw, r)
        seg.write_bytes(bytes(raw))
        before = _rss_kib()
        with pytest.raises(hft_py.JournalError) as e:
            list(hft_py.open_journal(j.path).events("ControlCmd"))
        assert e.value.seq == r.seq
        assert _rss_kib() - before < 16 * 1024
        return
    pytest.fail("no multi-part ControlCmd in J_ALL")


def test_py_065_no_panic_across_ffi(j_md: Journal, tmp_path: Path, tools: Path) -> None:
    sched = tmp_path / "schemas"
    sched.mkdir()
    src = Path(__file__).resolve().parents[4] / "fixtures" / "params" / "sdk-tp" / "schema.golden.json"
    s_tp = hft_py.params.load_schema(str(src))
    bad = j_md.copy(tmp_path / "bad")
    seg = bad.segments()[0]
    raw = bytearray(seg.read_bytes())
    r = next(x for x in jfile.records(bytes(raw)) if x.kind == 0x0101)
    struct.pack_into("<I", raw, r.off, 0xFFFF_FFF8)
    seg.write_bytes(bytes(raw))
    shm_root = tmp_path / "shm"
    (shm_root / "prod").mkdir(parents=True)
    (shm_root / "prod" / "heartbeat").write_bytes(b"\0" * 4096)
    j = hft_py.open_journal(j_md.path)
    rows = [
        ("PY-065.1", lambda: list(hft_py.open_journal(bad.path).events("Bbo")), hft_py.JournalError),
        ("PY-065.2", lambda: j.events("Bbo", t0=2**63), ValueError),
        ("PY-065.3", lambda: j.config_at(seq=2**64), ValueError),
        ("PY-065.4", lambda: j.tail(["Bbo"], poll_ms=2**64), ValueError),
        ("PY-065.5", lambda: hft_py.params.decode(s_tp, b""), hft_py.ParamsError),
        ("PY-065.6", lambda: hft_py.params.decode(s_tp, b"\x00" * 47), hft_py.ParamsError),
        ("PY-065.7", lambda: hft_py.heartbeat.read("prod", root=str(shm_root)), hft_py.SchemaError),
        ("PY-065.8", lambda: hft_py.decode.config_blob(b""), ValueError),
    ]
    for row, call, exc in rows:
        with pytest.raises(exc):
            call()
        assert pl.DataFrame(hft_py.open_journal(j_md.path).events("Bbo", inst=101)).height == 60_000, row
