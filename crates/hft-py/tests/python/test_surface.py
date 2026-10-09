"""Public surface: no write API, no test hooks in the release build, stubs match the runtime
(TESTS.md PY-140, PY-143). The release-build rows run in the interpreter named by
``HFT_PY_RELEASE_PYTHON`` (a venv with the wheel built without the ``testing`` feature)."""

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
from pathlib import Path

import hft_py
from conftest import Journal

WRITE = re.compile(r"write|append|publish|commit|inject|send|create|delete|set_")

PROBE = r"""
import json, sys, hft_py
j = hft_py.open_journal(sys.argv[1])
pub = lambda o: sorted(n for n in dir(o) if not n.startswith("_"))
print(json.dumps({
    "hft_py": pub(hft_py), "journal": pub(j), "ledger": pub(hft_py.ledger(j, account="bn-1")),
    "params": pub(hft_py.params), "heartbeat": pub(hft_py.heartbeat), "decode": pub(hft_py.decode),
    "schema": pub(hft_py.schema), "testing": hasattr(hft_py, "_testing"),
}))
"""


def release_python() -> str:
    p = os.environ.get("HFT_PY_RELEASE_PYTHON")
    assert p, "HFT_PY_RELEASE_PYTHON must name an interpreter with the release-build wheel installed"
    return p


def test_py_140_no_write_api(j_md: Journal) -> None:
    out = subprocess.run([release_python(), "-c", PROBE, j_md.path], capture_output=True, text=True)
    assert out.returncode == 0, out.stderr
    got = json.loads(out.stdout)
    want = {
        "PY-140.1": ("hft_py", {"open_journal", "ledger", "pnl", "params", "heartbeat", "decode", "schema", "schema_hash", "JournalError", "SchemaError", "ParamsError"}),
        "PY-140.2": ("journal", {"info", "events", "tail", "config_at"}),
        "PY-140.3": ("ledger", {"positions", "inventory", "portfolio", "breakdown"}),
        "PY-140.4": ("params", {"load_schemas", "load_schema", "encode", "decode", "encode_cmd", "parse_value", "format_value", "Error"}),
        "PY-140.5": ("heartbeat", {"read", "Watch"}),
        "PY-140.6": ("decode", {"control_cmd", "config_blob", "state_snapshot", "advice", "raw_rx", "raw_tx"}),
        "PY-140.7": ("schema", {"catalog"}),
    }
    for row, (key, names) in want.items():
        assert set(got[key]) == names, (row, got[key])
    every = [n for key, _ in want.values() for n in got[key]]
    assert not [n for n in every if WRITE.search(n)], "PY-140.8"
    assert got["testing"] is False, "PY-140.9"
    # The in-process (testing) build exposes the same public names plus the private hook module.
    assert {n for n in dir(hft_py) if not n.startswith("_")} == want["PY-140.1"][1]


def test_py_143_stubtest(tmp_path: Path) -> None:
    py = release_python()
    r = subprocess.run([py, "-m", "mypy.stubtest", "hft_py"], capture_output=True, text=True)
    assert r.returncode == 0, r.stdout + r.stderr
    src = Path(__file__).resolve().parents[2] / "python" / "hft_py"
    stubs = tmp_path / "stubs" / "hft_py"
    stubs.mkdir(parents=True)
    for f in src.glob("*.pyi"):
        shutil.copy(f, stubs / f.name)
    hb = stubs / "heartbeat.pyi"
    text = hb.read_text()
    hb.write_text(re.sub(r"@final\nclass Watch:.*\Z", "", text, flags=re.S))
    env = dict(os.environ, MYPYPATH=str(tmp_path / "stubs"))
    r2 = subprocess.run([py, "-m", "mypy.stubtest", "hft_py"], capture_output=True, text=True, env=env)
    assert r2.returncode != 0
    assert "hft_py.heartbeat.Watch" in r2.stdout + r2.stderr

