"""ledger() / pnl(): the cold ledger through PyO3 against hft-ledger's JournalLedger read in Rust
(``RUST_REF``), start points, windows, accounts, keys (TESTS.md PY-100–PY-113, LED-246)."""

from __future__ import annotations

import json
import shutil
from decimal import Decimal
from pathlib import Path

import polars as pl
import pytest

import hft_py
from conftest import Journal, Shm, pyfixture, run_tw

PNL = ["price_realized", "funding", "fee_cex", "gas", "bribe", "unrealized", "adjust", "external"]
GROUP_NONE = 0


class Led:
    def __init__(self, j: Journal, tools: Path, base: Path) -> None:
        self.j = j
        self.tools = tools
        self.base = base

    def ref(self, from_seq: int = 0, cold_root: Path | None = None) -> dict:
        out = self.base / f"ref-{from_seq}-{abs(hash(str(cold_root)))}.json"
        if not out.exists():
            pyfixture(self.tools, "ledger-ref", str(cold_root or self.j.cold_root), self.j.instance, str(from_seq), str(out))
        return json.loads(out.read_text())


_CACHE: dict[int, Led] = {}


@pytest.fixture(scope="session")
def j_led(tools: Path, shm: Shm, tmp_path_factory: pytest.TempPathFactory):  # type: ignore[no-untyped-def]
    """``J_LED20``: seed → generated journal (3 sealed segments, cold only)."""

    def get(seed: int) -> Led:
        if seed not in _CACHE:
            base = tmp_path_factory.mktemp(f"j_led{seed}")
            gen = base / "gen"
            pyfixture(tools, "led", str(seed), str(gen))
            hot = shm.dir(f"led{seed}")
            run_tw(tools, hot, base / "cold", (gen / "script.txt").read_text())
            shutil.rmtree(hot, ignore_errors=True)
            _CACHE[seed] = Led(Journal(base / "cold", "prod"), tools, base)
        return _CACHE[seed]

    return get


def rows(b: object) -> list[dict]:
    return pl.DataFrame(b).to_dicts()


def _norm(rs: list[dict], drop: tuple[str, ...] = ("account", "account_kind")) -> list[dict]:
    out = []
    for r in rs:
        out.append({k: (str(int(v)) if isinstance(v, Decimal) else v) for k, v in r.items() if k not in drop})
    return out


def views_equal(led: object, ref: dict) -> None:
    assert _norm(rows(led.positions())) == ref["positions"]  # type: ignore[attr-defined]
    assert _norm(rows(led.inventory())) == ref["inventory"]  # type: ignore[attr-defined]
    assert _norm(rows(led.portfolio())) == ref["portfolio"]  # type: ignore[attr-defined]


def ref_sum(ref: dict, lo: int | None = None, hi: int | None = None) -> list[int]:
    s = [0] * 8
    for e in ref["events"]:
        if (lo is None or e["proc_ns"] >= lo) and (hi is None or e["proc_ns"] < hi):
            for c in e["cells"]:
                s = [a + b for a, b in zip(s, c["d"])]
    return s


def bd(led: object) -> list[int]:
    r = rows(led.breakdown())[0]  # type: ignore[attr-defined]
    return [r[c] for c in PNL]


def test_py_100_ledger_views_match_rust(j_led) -> None:  # type: ignore[no-untyped-def]
    for seed in range(1, 21):
        lj = j_led(seed)
        led = hft_py.ledger(hft_py.open_journal(lj.j.path), account="bn-1")
        views_equal(led, lj.ref())


def _keyframes(j: object) -> list[tuple[int, int, int]]:
    d = pl.DataFrame(j.events("StateSnapshot"))  # type: ignore[attr-defined]
    k = d.filter(pl.col("purpose") == 2)
    return list(zip(k["seq"].to_list(), k["proc_ns"].to_list(), [0] * k.height))


@pytest.mark.parametrize("row", ["PY-101.1", "PY-101.2", "PY-101.3", "PY-101.4"])
def test_py_101_start_snapshot(j_led, tmp_path: Path, row: str) -> None:  # type: ignore[no-untyped-def]
    lj = j_led(1)
    j = hft_py.open_journal(lj.j.path)
    segs = j.info()["segments"]
    assert len(segs) == 3
    kf = _keyframes(j)
    k3, t3 = [(s, t) for s, t, _ in kf if s >= segs[2]["first_seq"]][0]
    assert [s for s, _, _ in kf if segs[1]["first_seq"] <= s < segs[2]["first_seq"]], "no keyframe in segment 2"
    if row == "PY-101.1":
        views_equal(hft_py.ledger(j, account="bn-1", t0=t3), lj.ref(k3))
    elif row == "PY-101.4":
        views_equal(hft_py.ledger(j, account="bn-1", t0=None), lj.ref())
    else:
        cut = lj.j.copy(tmp_path / row)
        for p in cut.segments()[:2]:
            p.unlink()
            p.with_suffix(".seal").unlink(missing_ok=True)
        jj = hft_py.open_journal(cut.path)
        if row == "PY-101.2":
            views_equal(hft_py.ledger(jj, account="bn-1", t0=t3 + 1), lj.ref(k3))
        else:
            boot = pl.DataFrame(jj.events("StateSnapshot")).filter(pl.col("purpose") == 1)["seq"][0]
            views_equal(hft_py.ledger(jj, account="bn-1", t0=t3 - 1), lj.ref(boot, cold_root=cut.cold_root))


@pytest.mark.parametrize("row", ["PY-102.1", "PY-102.2", "PY-102.3", "PY-102.4"])
def test_py_102_breakdown_window(j_led, row: str) -> None:  # type: ignore[no-untyped-def]
    lj = j_led(2)
    j = hft_py.open_journal(lj.j.path)
    ref = lj.ref()
    ts = [e["proc_ns"] for e in ref["events"]]
    s, e = min(ts), max(ts) + 1
    m = ts[len(ts) // 2]
    if row == "PY-102.1":
        assert bd(hft_py.ledger(j, account="bn-1")) == ref_sum(ref)
    elif row == "PY-102.2":
        assert bd(hft_py.ledger(j, account="bn-1", t0=s, t1=m)) == ref_sum(ref, s, m)
    elif row == "PY-102.3":
        # Start point: the latest ledger snapshot at or before M (the reference starts there too).
        snaps = pl.DataFrame(j.events("StateSnapshot")).filter(pl.col("proc_ns") <= m).sort("seq")
        k = snaps["seq"][-1] if snaps.height else 0
        assert bd(hft_py.ledger(j, account="bn-1", t0=m, t1=e)) == ref_sum(lj.ref(k), m, e)
    else:
        assert bd(hft_py.ledger(j, account="bn-1", t0=m, t1=m)) == [0] * 8


def test_py_103_ledger_accounts(j_led, j_sim: Journal, tools: Path, tmp_path: Path) -> None:  # type: ignore[no-untyped-def]
    lj = j_led(3)
    j = hft_py.open_journal(lj.j.path)
    a = rows(hft_py.ledger(j, account="bn-1").portfolio())
    b = rows(hft_py.ledger(j, account=1).portfolio())
    assert a == b and {r["account_kind"] for r in a} == {"Live"}, "PY-103.1"
    js = hft_py.open_journal(j_sim.path)
    sim = hft_py.ledger(js, account="sim-a")
    assert {r["account_kind"] for r in rows(sim.portfolio())} == {"Sim"}, "PY-103.2"
    out = tmp_path / "sim.json"
    pyfixture(tools, "ledger-ref", str(j_sim.cold_root), "sim", "0", str(out))
    views_equal(sim, json.loads(out.read_text()))
    with pytest.raises(ValueError):
        hft_py.ledger(j, account="sim-a")


def _ref_by(ref: dict, by: str) -> dict:
    out: dict = {}
    for e in ref["events"]:
        day = e["proc_ns"] // 86_400_000_000_000
        for c in e["cells"]:
            key = {"group": (c["group"],), "account": (), "round": (c["group"], c["round"]), "day": (day,)}[by]
            cur = out.setdefault(key, [0] * 8)
            out[key] = [x + y for x, y in zip(cur, c["d"])]
    return out


@pytest.mark.parametrize("row", ["PY-104.1", "PY-104.2", "PY-104.3", "PY-104.4"])
def test_py_104_pnl_by_keys(j_led, row: str) -> None:  # type: ignore[no-untyped-def]
    import datetime as dt

    lj = j_led(4)
    j = hft_py.open_journal(lj.j.path)
    by = {"PY-104.1": "group", "PY-104.2": "account", "PY-104.3": "round", "PY-104.4": "day"}[row]
    got = {}
    for r in rows(hft_py.pnl(j, by=by)):
        if by == "group":
            key = (r["group"],)
        elif by == "account":
            key = ()
        elif by == "round":
            key = (r["group"], r["round"])
        else:
            d = dt.date.fromisoformat(r["day"])
            key = ((d - dt.date(1970, 1, 1)).days,)
        got[key] = [r[c] for c in PNL]
    assert got == _ref_by(lj.ref(), by), row
    if by == "day":
        assert len(got) >= 2
        total = [sum(v[i] for v in got.values()) for i in range(8)]
        acc = rows(hft_py.pnl(j, by="account"))[0]
        assert total == [acc[c] for c in PNL]


@pytest.mark.parametrize(("row", "by"), [("PY-105.1", "week"), ("PY-105.2", ""), ("PY-105.3", "Group"), ("PY-105.4", None)])
def test_py_105_pnl_bad_by(j_led, row: str, by: object) -> None:  # type: ignore[no-untyped-def]
    j = hft_py.open_journal(j_led(4).j.path)
    with pytest.raises(ValueError):
        hft_py.pnl(j, by=by)


def test_py_110_deterministic(j_led, j_sim: Journal) -> None:  # type: ignore[no-untyped-def]
    for path, acct in ((j_led(5).j.path, "bn-1"), (j_sim.path, "sim-a")):
        j = hft_py.open_journal(path)
        a, b = hft_py.ledger(j, account=acct), hft_py.ledger(j, account=acct)
        assert pl.DataFrame(a.positions()).equals(pl.DataFrame(b.positions()))
        assert pl.DataFrame(a.breakdown()).equals(pl.DataFrame(b.breakdown()))
        assert pl.DataFrame(hft_py.pnl(j, by="group")).equals(pl.DataFrame(hft_py.pnl(j, by="group")))


def test_led_246_pyo3_matches_rust(j_led) -> None:  # type: ignore[no-untyped-def]
    testing = getattr(hft_py, "_testing", None)
    assert testing is not None, "hft_py was built without the `testing` feature"
    for seed in range(1, 21):
        lj = j_led(seed)
        ref = lj.ref()
        j = hft_py.open_journal(lj.j.path)
        led = hft_py.ledger(j, account="bn-1")
        assert testing.ledger_snapshot(led).hex() == ref["snapshot_hex"], seed
        for e in ref["events"][::10]:
            hi = e["proc_ns"] + 1
            assert bd(hft_py.ledger(j, account="bn-1", t1=hi)) == ref_sum(ref, None, hi), (seed, e["seq"])
