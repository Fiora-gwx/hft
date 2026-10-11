"""config_at(seq): boot block, applied commands, ReplaceRefdata, blob shapes, early seq
(TESTS.md PY-085–PY-089; fixture ``J_CFG``)."""

from __future__ import annotations

import hashlib
import shutil
from pathlib import Path

import polars as pl
import pytest

import hft_py
from conftest import Journal, Shm, pyfixture, run_tw


class Cfg:
    """``J_CFG`` and the seqs the cases name."""

    def __init__(self, j: Journal) -> None:
        self.j = j
        jj = hft_py.open_journal(j.path)
        self.jj = jj
        segs = jj.info()["segments"]
        self.seg2_first = segs[1]["first_seq"]
        roll = pl.DataFrame(jj.events("SegmentRoll"))
        done = roll.filter(pl.col("mark") == 3)["seq"].to_list()
        self.b1, self.b2 = done[0], done[1]
        self.f = segs[0]["first_seq"]
        blobs = pl.DataFrame(jj.events("ConfigBlob")).filter(pl.col("boot") == 0)
        self.p1 = blobs["seq"][0]
        res = pl.DataFrame(jj.events("CmdResult"))["seq"].to_list()
        self.p3, self.r3, self.k3, self.a3 = res[0], res[1], res[2], res[3]
        self.seg1_last = self.seg2_first - 1


@pytest.fixture(scope="module")
def jcfg(tools: Path, shm: Shm, tmp_path_factory: pytest.TempPathFactory) -> Cfg:
    base = tmp_path_factory.mktemp("j_cfg")
    gen = base / "gen"
    pyfixture(tools, "cfg", str(gen))
    hot = shm.dir("cfg")
    cold = base / "cold"
    run_tw(tools, hot, cold, (gen / "script.txt").read_text())
    run_tw(tools, hot, cold, (gen / "run2" / "script.txt").read_text())
    shutil.rmtree(hot, ignore_errors=True)
    return Cfg(Journal(cold, "prod"))


def test_py_085_boot_block_config(jcfg: Cfg) -> None:
    c = jcfg.jj.config_at(jcfg.b1)
    assert c["refdata"]["version"] == 1
    assert c["risk"]["version"] == 1
    sp = c["strategy_params"]["alpha/bn-1"]
    assert sp["version"] == 7
    boot = pl.DataFrame(jcfg.jj.events("ConfigBlob")).filter((pl.col("boot") == 1) & (pl.col("seq") < jcfg.b1))
    by_kind = {r["kind"]: r for r in boot.iter_rows(named=True)}
    for key, kind in (("refdata", 1), ("risk", 3)):
        assert c[key]["seq"] == by_kind[kind]["seq"]
        assert c[key]["sha256"] == by_kind[kind]["sha256"].hex()
    assert sp["seq"] == by_kind[2]["seq"]


def test_py_086_applied_only(jcfg: Cfg) -> None:
    sp = lambda s: jcfg.jj.config_at(s)["strategy_params"]["alpha/bn-1"]  # noqa: E731
    assert sp(jcfg.p1)["version"] == 7, "PY-086.1"
    assert sp(jcfg.p3 - 1)["version"] == 7, "PY-086.2"
    v8 = sp(jcfg.p3)
    blob1 = pl.DataFrame(jcfg.jj.events("ConfigBlob")).filter(pl.col("seq") == jcfg.p1)["sha256"][0]
    assert v8["version"] == 8 and v8["sha256"] == blob1.hex(), "PY-086.3"
    assert sp(jcfg.r3)["version"] == 8, "PY-086.4"
    assert jcfg.jj.config_at(jcfg.k3 - 1)["risk"]["version"] == 1, "PY-086.5"
    assert jcfg.jj.config_at(jcfg.k3)["risk"]["version"] == 2, "PY-086.6"


def test_py_087_refdata_next_boot(jcfg: Cfg) -> None:
    assert jcfg.jj.config_at(jcfg.a3)["refdata"]["version"] == 1, "PY-087.1"
    assert jcfg.jj.config_at(jcfg.seg1_last)["refdata"]["version"] == 1, "PY-087.2"
    c = jcfg.jj.config_at(jcfg.b2)
    assert c["refdata"]["version"] == 2 and c["strategy_params"]["alpha/bn-1"]["version"] == 8, "PY-087.3"


def test_py_088_blob_shapes(jcfg: Cfg) -> None:
    c = jcfg.jj.config_at(jcfg.k3)
    rd = c["refdata"]
    acc = [a for a in rd["accounts"] if a["label"] == "bn-1"]
    assert acc and acc[0]["id"] == 1 and acc[0]["kind"] == "Live", "PY-088.1"
    assert [i for i in rd["insts"] if i["id"] == 101][0]["px_exp"] == -2, "PY-088.1"
    risk = c["risk"]
    assert risk["bytes"] == b"risk-params-v2" and hashlib.sha256(risk["bytes"]).hexdigest() == risk["sha256"], "PY-088.2"
    eng = c["engine"]
    assert "ledger" in eng["sections"] and len(eng["sections"]["ledger"]) == 512, "PY-088.3"
    assert eng["sections"]["assembly"] == b"\x01" + bytes(15), "PY-088.3 assembly"
    sp = c["strategy_params"]["alpha/bn-1"]
    h = sp["hdr"]
    assert (h["strategy"], h["account"], h["kind_id"], h["schema_id"], h["schema_ver"], h["source"], h["version"]) == (1, 1, 65281, 65281, 2, "Manual", 8), "PY-088.4"
    assert isinstance(sp["groups"], list) and sp["groups"][0]["legs"][0]["inst"] == 101, "PY-088.4"
    assert isinstance(sp["body"], bytes) and len(sp["body"]) == h["body_len"], "PY-088.4"
    hm = c["host_manifest"]
    assert hashlib.sha256(hm["bytes"]).hexdigest() == hm["sha256"], "PY-088.5"
    assert "sim_config" not in c, "PY-088.6"


def test_py_089_before_boot(jcfg: Cfg) -> None:
    for row, s in (("PY-089.1", jcfg.f), ("PY-089.2", jcfg.b1 - 1), ("PY-089.4", -1)):
        with pytest.raises(ValueError):
            jcfg.jj.config_at(s)
    assert isinstance(jcfg.jj.config_at(jcfg.b1), dict), "PY-089.3"
