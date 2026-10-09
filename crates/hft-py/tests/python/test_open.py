"""open_journal: hot/cold sources, single segments, info(), instance names, missing and damaged
journals (TESTS.md PY-001–PY-011)."""

from __future__ import annotations

import hashlib
import os
import shutil
import stat
from pathlib import Path

import polars as pl
import pytest

import hft_py
from conftest import D, MS, S, Journal, Shm, run_tw, synth


def _bbo_seqs(j: object, **kw: object) -> list[int]:
    return pl.DataFrame(j.events("Bbo", **kw))["seq"].to_list()  # type: ignore[attr-defined]


@pytest.fixture()
def hot_cold(tools: Path, shm: Shm, tmp_path: Path) -> tuple[Path, Path]:
    """TW wrote 2 000 Bbo to the hot segment; persist synced the first 1 500, then stopped."""
    hot = shm.dir("py001")
    cold = tmp_path / "cold"
    script = "\n".join(
        [
            synth(1500, D + 10 * S),
            "flush",
            "persist off",
            synth(500, D + 10 * S + 1500 * MS),
            "publish",
        ]
    )
    run_tw(tools, hot, cold, script)
    return hot, cold


def test_py_001_hot_root_reads_unsynced_tail(hot_cold: tuple[Path, Path]) -> None:
    hot, cold = hot_cold
    j = hft_py.open_journal(str(cold / "prod" / "seq"), hot_root=str(hot))
    df = pl.DataFrame(j.events("Bbo", inst=101))
    assert df.height == 2000
    seqs = df["seq"].to_list()
    assert all(b - a == 1 for a, b in zip(seqs, seqs[1:]))
    assert seqs[-1] == j.info()["last_seq"]


def test_py_002_no_hot_root_cold_only(hot_cold: tuple[Path, Path]) -> None:
    hot, cold = hot_cold
    j = hft_py.open_journal(str(cold / "prod" / "seq"))
    assert pl.DataFrame(j.events("Bbo", inst=101)).height == 1500
    jh = hft_py.open_journal(str(cold / "prod" / "seq"), hot_root=str(hot))
    assert j.info()["last_seq"] < jh.info()["last_seq"]
    assert j.info()["last_seq"] == _bbo_seqs(j)[-1]


def test_py_003_single_segment_file(j_all: Journal) -> None:
    segs = j_all.segments()
    assert len(segs) == 2
    p2 = segs[1]
    j = hft_py.open_journal(str(p2))
    info = j.info()
    assert [s["segment_no"] for s in info["segments"]] == [2]
    first = int(p2.name.split("-")[1].split(".")[0])
    assert info["first_seq"] == first
    full = hft_py.open_journal(j_all.path).info()
    for kind in ["Bbo", "OrderEvent", "ClockStatus", "ControlCmd", "MarginReport"]:
        df = pl.DataFrame(j.events(kind))
        assert df.height > 0, kind
        assert df["seq"].min() >= first, kind
        assert df["seq"].max() <= full["last_seq"], kind


def test_py_004_info_keys(j_all: Journal) -> None:
    j = hft_py.open_journal(j_all.path)
    info = j.info()
    rows = {
        "PY-004.1": set(info) == {"journal_name", "instance", "segments", "first_seq", "last_seq", "schema", "host_profile"},
        "PY-004.2": info["journal_name"] == "seq",
        "PY-004.3": info["instance"] == "prod",
    }
    segs = info["segments"]
    rows["PY-004.4"] = [s["segment_no"] for s in segs] == [1, 2] and all(
        Path(s["path"]).exists() and s["first_seq"] > 0 for s in segs
    )
    seg_files = j_all.segments()
    last = max(pl.DataFrame(j.events(k))["seq"].max() for k in ["Bbo", "ClockStatus"])
    rows["PY-004.5"] = info["first_seq"] == segs[0]["first_seq"] and info["last_seq"] >= last
    rows["PY-004.6"] = info["schema"] == []
    hp = info["host_profile"]
    rows["PY-004.7"] = isinstance(hp, dict) and "profile_sha256" in hp and hp["tsc_hz_milli"] == 3_000_000_000_000
    assert len(seg_files) == 2
    bad = [k for k, ok in rows.items() if not ok]
    assert not bad, (bad, info)


@pytest.mark.parametrize(
    ("row", "inst", "ok"),
    [("PY-005.1", "prod", True), ("PY-005.2", "sim", True), ("PY-005.3", "test", False), ("PY-005.4", "Prod", False)],
)
def test_py_005_instance_name(j_md: Journal, tmp_path: Path, row: str, inst: str, ok: bool) -> None:
    dst = tmp_path / row
    shutil.copytree(j_md.cold_root / "prod" / "seq", dst / inst / "seq")
    if ok:
        assert hft_py.open_journal(str(dst / inst / "seq")).info()["instance"] == inst, row
    else:
        with pytest.raises(ValueError):
            hft_py.open_journal(str(dst / inst / "seq"))


def test_py_006_missing_path(tmp_path: Path) -> None:
    nope = tmp_path / "prod" / "nope"
    with pytest.raises(hft_py.JournalError) as e:
        hft_py.open_journal(str(nope))
    assert e.value.path == str(nope), "PY-006.1"
    empty = tmp_path / "prod" / "seq"
    empty.mkdir(parents=True)
    with pytest.raises(hft_py.JournalError) as e:
        hft_py.open_journal(str(empty))
    assert e.value.path == str(empty), "PY-006.2"
    f = empty / "20261001" / f"{1:010d}-{1:020d}.jrn"
    with pytest.raises(hft_py.JournalError):
        hft_py.open_journal(str(f))


def _snapshot(root: Path) -> dict[str, tuple[int, int, str]]:
    out = {}
    for p in sorted(root.rglob("*")):
        if p.is_file():
            out[str(p)] = (p.stat().st_size, p.stat().st_mtime_ns, hashlib.sha256(p.read_bytes()).hexdigest())
        else:
            out[str(p)] = (0, 0, "dir")
    return out


def test_py_007_readonly_mount(j_all: Journal, j_all_ref: Path, tmp_path: Path) -> None:
    ro = tmp_path / "ro"
    j = j_all.copy(ro)
    for p in sorted(ro.rglob("*"), reverse=True):
        os.chmod(p, 0o444 if p.is_file() else 0o555)
    os.chmod(ro, 0o555)
    before = _snapshot(ro)
    try:
        jj = hft_py.open_journal(j.path)
        for ref in sorted(j_all_ref.glob("*.jsonl")):
            kind = ref.stem
            n = sum(1 for _ in ref.open())
            assert pl.DataFrame(jj.events(kind)).height == n, kind
        assert _snapshot(ro) == before
    finally:
        for p in [ro, *ro.rglob("*")]:
            os.chmod(p, stat.S_IRWXU)


def test_py_008_hot_retired_switch_to_cold(tools: Path, shm: Shm, tmp_path: Path) -> None:
    hot = shm.dir("py008")
    cold = tmp_path / "cold"
    # ≈ 8 MiB segments of 120-byte records: three segments, the first two sealed and synced.
    run_tw(tools, hot, cold, "\n".join([synth(150_000, D + 10 * S), "flush"]))
    j = hft_py.open_journal(str(cold / "prod" / "seq"), hot_root=str(hot))
    assert len(j.info()["segments"]) == 3
    rdr = j.events("Bbo", batch_rows=1000)
    first = next(rdr)
    (hot / "prod" / "seq" / f"{1:010d}.jrn").unlink()
    seqs = list(first["seq"])
    for b in rdr:
        seqs.extend(b["seq"])
    assert len(seqs) == 150_000
    assert all(b - a >= 1 for a, b in zip(seqs, seqs[1:]))
    assert len(set(seqs)) == len(seqs)


def test_py_009_both_copies_deleted(tools: Path, shm: Shm, tmp_path: Path) -> None:
    hot = shm.dir("py009")
    cold = tmp_path / "cold"
    run_tw(tools, hot, cold, "\n".join([synth(150_000, D + 10 * S), "flush"]))
    j = hft_py.open_journal(str(cold / "prod" / "seq"), hot_root=str(hot))
    seg2 = j.info()["segments"][1]
    (hot / "prod" / "seq" / f"{2:010d}.jrn").unlink()
    Path(seg2["path"]).unlink()
    Path(seg2["path"]).with_suffix(".seal").unlink(missing_ok=True)
    got = []
    with pytest.raises(hft_py.JournalError) as e:
        for b in j.events("Bbo"):
            got.extend(b["seq"])
    assert got and max(got) < seg2["first_seq"]
    assert e.value.seq == seg2["first_seq"]
    assert f"{2:010d}-{seg2['first_seq']:020d}.jrn" in e.value.path


def test_py_010_chain_broken(tools: Path, shm: Shm, tmp_path: Path) -> None:
    hot = shm.dir("py010")
    cold = tmp_path / "cold"
    run_tw(tools, hot, cold, "\n".join([synth(150_000, D + 10 * S), "seal"]))
    j = hft_py.open_journal(str(cold / "prod" / "seq"))
    segs = j.info()["segments"]
    assert len(segs) == 3
    p2 = Path(segs[1]["path"])
    p2.unlink()
    p2.with_suffix(".seal").unlink()
    first3 = segs[2]["first_seq"]
    got: list[int] = []
    with pytest.raises(hft_py.JournalError) as e:
        jj = hft_py.open_journal(str(cold / "prod" / "seq"))
        for b in jj.events("Bbo"):
            got.extend(b["seq"])
    assert e.value.seq == segs[1]["first_seq"]
    assert all(s < first3 for s in got)


def test_py_011_two_instances(j_md: Journal, j_sim: Journal) -> None:
    jp = hft_py.open_journal(j_md.path)
    js = hft_py.open_journal(j_sim.path)
    assert jp.info()["instance"] == "prod"
    assert js.info()["instance"] == "sim"
    dp = pl.DataFrame(jp.events("OrderEvent"))
    ds = pl.DataFrame(js.events("OrderEvent"))
    assert dp.height == 100 and set(dp["account"]) == {1} and set(dp["account_kind"]) == {"Live"}
    assert ds.height > 0 and set(ds["account"]) == {1} and set(ds["account_kind"]) == {"Sim"}
    assert dp["seq"].min() >= jp.info()["segments"][0]["first_seq"]
    assert ds["seq"].min() >= js.info()["segments"][0]["first_seq"]
