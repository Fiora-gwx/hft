"""`az-probe compare` ranking (tools/TESTS.md TOOL-097)."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import pytest

from az_probe import cli
from az_probe.compare import rank

ENDPOINTS = {
    "binance-fstream": {"host": "fstream.binance.com", "port": 443, "exchange": "binance", "core": True},
    "okx-ws": {"host": "ws.okx.com", "port": 8443, "exchange": "okx", "core": True},
    "bitget-ws": {"host": "ws.bitget.com", "port": 443, "exchange": "bitget", "core": True},
    "gate-api": {"host": "api.gateio.ws", "port": 443, "exchange": "gate", "core": False},
}


def report(az: str, cpg: str, bn: float, okx: float, bg: float) -> dict[str, Any]:
    def row(ep: str, ip: str, v: float) -> dict[str, Any]:
        return {
            "az": az,
            "cpg": cpg,
            "endpoint": ep,
            "ip": ip,
            "min_rtt_us": v,
            "p50_us": v + 10,
            "p99_us": v + 50,
            "tls_handshake_us": 2000.0,
            "n": 9,
        }

    return {
        "az": az,
        "cpg": cpg,
        "endpoints": ENDPOINTS,
        "rows": [
            row("binance-fstream", "1.1.1.1", bn),
            row("okx-ws", "2.2.2.2", okx),
            row("bitget-ws", "3.3.3.3", bg),
            row("gate-api", "4.4.4.4", 100.0),
        ],
    }


def test_tool_097_ranking(tmp_path: Path, capsys: pytest.CaptureFixture[str]) -> None:
    reps = [
        report("apne1-az1", "cpg-x", 900, 700, 800),
        report("apne1-az4", "cpg-y", 300, 900, 800),
        report("apne1-az4", "cpg-z", 310, 880, 800),
    ]
    assert rank(reps) == [
        ("apne1-az4", "cpg-z", 1990.0),
        ("apne1-az4", "cpg-y", 2000.0),
        ("apne1-az1", "cpg-x", 2400.0),
    ]
    paths = []
    for i, r in enumerate(reps):
        p = tmp_path / f"report-{i}.json"
        p.write_text(json.dumps(r), encoding="utf-8")
        paths.append(str(p))
    assert cli.main(["compare", *paths]) == 0
    md = capsys.readouterr().out
    ranked = [ln for ln in md.splitlines() if ln[:4] in ("| 1 ", "| 2 ", "| 3 ")]
    assert ranked[0].startswith("| 1 | apne1-az4 | cpg-z | 1990 |")
    assert ranked[1].startswith("| 2 | apne1-az4 | cpg-y | 2000 |")
    assert ranked[2].startswith("| 3 | apne1-az1 | cpg-x | 2400 |")
