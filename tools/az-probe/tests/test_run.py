"""`az-probe run` with a substitute environment (tools/TESTS.md TOOL-092, TOOL-094–TOOL-096)."""

from __future__ import annotations

import builtins
import json
import os
import re
from collections.abc import Iterator, MutableMapping
from pathlib import Path
from typing import Any

import pytest
from fakes import FakeEnv

from az_probe import cli

FIXTURES = Path(__file__).resolve().parent / "fixtures"

ONE = """
[[target]]
name = "binance-fstream"
host = "fstream.binance.com"
exchange = "binance"
core = true
ws = { url = "wss://fstream.binance.com/public/ws/btcusdt@bookTicker", subscribe = "", ts = "E", unit = "ms" }
"""

TWO = """
[[target]]
name = "binance-fstream"
host = "fstream.binance.com"
exchange = "binance"
core = true

[[target]]
name = "okx-ws"
host = "ws.okx.com"
port = 8443
exchange = "okx"
core = true
"""


def run(tmp_path: Path, env: FakeEnv, *extra: str, targets: str = ONE) -> dict[str, Any]:
    t = tmp_path / "targets.toml"
    t.write_text(targets, encoding="utf-8")
    out = tmp_path / "r.json"
    code = cli.main(["run", "--targets", str(t), "--out", str(out), "--duration", "1m", *extra], env=env)
    assert code == 0
    report: dict[str, Any] = json.loads(out.read_text(encoding="utf-8"))
    return report


def test_tool_092_probes_all_a_records(tmp_path: Path) -> None:
    env = FakeEnv({"fstream.binance.com": [["1.1.1.1", "1.1.1.2"], ["1.1.1.2", "1.1.1.3"], ["1.1.1.1"]]})
    r = run(tmp_path, env)
    rows = [x for x in r["rows"] if x["endpoint"] == "binance-fstream"]
    assert [x["ip"] for x in rows] == ["1.1.1.1", "1.1.1.2", "1.1.1.3"]
    for x in rows:
        assert x["n"] == 9
        assert x["min_rtt_us"] == 300
    assert env.resolve_calls["fstream.binance.com"] == 3


def frames(n: int) -> list[tuple[int, str]]:
    """`n` bookTicker frames received 1.5 ms after their event time `E`."""
    base = 1_759_800_000_000
    return [
        ((base + i) * 1_000_000 + 1_500_000, json.dumps({"e": "bookTicker", "E": base + i})) for i in range(n)
    ]


def test_tool_094_first_frame_unsynced(tmp_path: Path) -> None:
    env = FakeEnv({"fstream.binance.com": [["1.1.1.1"]]}, frames=frames(10), phc=False)
    r = run(tmp_path, env, "--ws-first-frame")
    ff = r["first_frame"]
    assert len(ff) == 1
    assert ff[0]["clock"] == "unsynced"
    assert ff[0]["n"] == 10
    assert ff[0]["p50_us"] == 1500
    assert len(r["rows"]) == 1 and r["rows"][0]["n"] == 9


class Recording(MutableMapping[str, str]):
    """`os.environ` stand-in that records every key read."""

    def __init__(self, inner: MutableMapping[str, str]) -> None:
        self.inner = inner
        self.read: list[str] = []

    def __getitem__(self, k: str) -> str:
        self.read.append(k)
        return self.inner[k]

    def __setitem__(self, k: str, v: str) -> None:
        self.inner[k] = v

    def __delitem__(self, k: str) -> None:
        del self.inner[k]

    def __iter__(self) -> Iterator[str]:
        self.read.append("*")
        return iter(self.inner)

    def __len__(self) -> int:
        return len(self.inner)


def test_tool_095_handshake_only(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    env = FakeEnv({"fstream.binance.com": [["1.1.1.1"]]}, frames=frames(10))
    monkeypatch.setenv("BINANCE_API_KEY", "S3CR3T")
    rec = Recording(os.environ)
    monkeypatch.setattr(os, "environ", rec)
    opened: list[str] = []
    real_open = builtins.open

    def spy(file: Any, *a: Any, **k: Any) -> Any:
        opened.append(str(file))
        return real_open(file, *a, **k)

    t = tmp_path / "targets.toml"
    t.write_text(ONE, encoding="utf-8")
    out = tmp_path / "r.json"
    monkeypatch.setattr(builtins, "open", spy)
    code = cli.main(["run", "--targets", str(t), "--out", str(out), "--duration", "1m"], env=env)
    monkeypatch.setattr(builtins, "open", real_open)
    assert code == 0
    r = json.loads(out.read_text(encoding="utf-8"))
    assert env.ws_calls == []
    assert r["first_frame"] == []
    assert not [k for k in rec.read if k == "*" or re.search("key|secret|token|pass", k, re.IGNORECASE)], (
        rec.read
    )
    assert sorted(Path(p).name for p in opened) == ["r.json", "targets.toml"], opened


def test_tool_096_output_rows(tmp_path: Path) -> None:
    env = FakeEnv(
        {"fstream.binance.com": [["1.1.1.1", "1.1.1.2"]], "ws.okx.com": [["2.2.2.1", "2.2.2.2"]]},
        rtt={
            "1.1.1.1": [310, 305, 300, 320, 330, 315, 340, 400, 302],
            "1.1.1.2": [1300, 1290, 1310, 1305, 1295, 1500, 1301, 1299, 1302],
            "2.2.2.1": [880, 881, 882, 883, 884, 885, 886, 887, 2000],
            "2.2.2.2": [700.5, 700.4, 700.3, 700.2, 700.1, 700.6, 700.7, 700.8, 700.9],
        },
        tls=2500.0,
        placement=("apne1-az4", "cpg-a"),
    )
    r = run(tmp_path, env, targets=TWO)
    expected = json.loads((FIXTURES / "expected-run.json").read_text(encoding="utf-8"))
    assert r["rows"] == expected["rows"]
