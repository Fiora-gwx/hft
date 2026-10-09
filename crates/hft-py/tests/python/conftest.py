"""Shared fixtures of the hft-py pytest suite (crates/hft-py/TESTS.md "共用夹具").

Journals are written at test time by hft-journal's test writer (``TW``, ``testwriter --script``)
from inputs produced by ``examples/pyfixture.rs``; nothing here writes journal bytes itself except
the corruption cases, which edit copies. Linux only (hot segments live on tmpfs).
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import uuid
from pathlib import Path
from typing import Iterator

import pytest

D = 1_790_812_800_000_000_000
S = 1_000_000_000
MS = 1_000_000
HZ_MILLI = 3_000_000_000_000
REPO = Path(__file__).resolve().parents[4]

if not sys.platform.startswith("linux"):
    pytest.skip("hft-py journal tests need Linux (tmpfs hot segments)", allow_module_level=True)


def _target_dir() -> Path:
    return Path(os.environ.get("CARGO_TARGET_DIR", REPO / "target"))


def tools_dir() -> Path:
    """Directory holding ``testwriter`` and ``pyfixture`` (``HFT_PY_TOOLS`` or a fresh cargo build)."""
    env = os.environ.get("HFT_PY_TOOLS")
    if env:
        return Path(env)
    profile = os.environ.get("HFT_PY_TOOLS_PROFILE", "ci")
    cmd = [
        "cargo",
        "build",
        "--quiet",
        "--locked",
        "--profile",
        profile,
        "-p",
        "hft-journal",
        "--example",
        "testwriter",
        "--features",
        "hft-journal/writer",
        "-p",
        "hft-py",
        "--example",
        "pyfixture",
    ]
    subprocess.run(cmd, cwd=REPO, check=True)
    return _target_dir() / profile / "examples"


@pytest.fixture(scope="session")
def tools() -> Path:
    return tools_dir()


class Shm:
    """Temporary directories on tmpfs (hot segments)."""

    def __init__(self) -> None:
        base = Path("/dev/shm")
        self.root = base / f"hftpy-{os.getpid()}-{uuid.uuid4().hex[:8]}"
        self.root.mkdir(parents=True)

    def dir(self, name: str) -> Path:
        p = self.root / f"{name}-{uuid.uuid4().hex[:6]}"
        p.mkdir(parents=True)
        return p

    def cleanup(self) -> None:
        shutil.rmtree(self.root, ignore_errors=True)


@pytest.fixture(scope="session")
def shm() -> Iterator[Shm]:
    s = Shm()
    yield s
    s.cleanup()


def tw_args(
    tools: Path,
    hot: Path,
    cold: Path,
    instance: str = "prod",
    *,
    script: str = "-",
    segment_bytes: int = 8 << 20,
    publish_every: int = 1,
    sync_ms: int = 50,
    clock_every_ns: int = 0,
) -> list[str]:
    return [
        str(tools / "testwriter"),
        "--hot",
        str(hot),
        "--cold",
        str(cold),
        "--instance",
        instance,
        "--script",
        script,
        "--segment-bytes",
        str(segment_bytes),
        "--publish-every",
        str(publish_every),
        "--sync-interval-ms",
        str(sync_ms),
        "--tsc-hz-milli",
        str(HZ_MILLI),
        "--calib-utc-ns",
        str(D),
        "--clock-every-ns",
        str(clock_every_ns),
    ]


def run_tw(tools: Path, hot: Path, cold: Path, script: str, instance: str = "prod", **kw: int) -> None:
    """Runs TW to completion on a script text."""
    args = tw_args(tools, hot, cold, instance, **kw)
    r = subprocess.run(args, input=script.encode(), capture_output=True)
    assert r.returncode == 0, f"testwriter failed ({r.returncode}): {r.stderr.decode()}"


def pyfixture(tools: Path, *args: str) -> None:
    r = subprocess.run([str(tools / "pyfixture"), *args], capture_output=True)
    assert r.returncode == 0, f"pyfixture {args} failed: {r.stderr.decode()}"


class Journal:
    """A generated journal: ``cold`` = ``<cold_root>/<instance>/seq``, ``hot_root`` when kept."""

    def __init__(self, cold_root: Path, instance: str, hot_root: Path | None = None) -> None:
        self.cold_root = cold_root
        self.instance = instance
        self.hot_root = hot_root

    @property
    def path(self) -> str:
        return str(self.cold_root / self.instance / "seq")

    def segments(self) -> list[Path]:
        return sorted((self.cold_root / self.instance / "seq").glob("*/*.jrn"))

    def copy(self, dst: Path, instance: str | None = None) -> "Journal":
        inst = instance or self.instance
        shutil.copytree(self.cold_root / self.instance / "seq", dst / inst / "seq")
        return Journal(dst, inst)


def build_fixture(tools: Path, shm: Shm, base: Path, kind: str, *args: str, instance: str = "prod",
                  segment_bytes: int = 64 << 20, keep_hot: bool = False) -> Journal:
    gen = base / "gen"
    pyfixture(tools, kind, *args, str(gen))
    hot = shm.dir(kind)
    cold = base / "cold"
    script = (gen / "script.txt").read_text()
    run_tw(tools, hot, cold, script, instance, segment_bytes=segment_bytes)
    if not keep_hot:
        shutil.rmtree(hot, ignore_errors=True)
        return Journal(cold, instance)
    return Journal(cold, instance, hot)


@pytest.fixture(scope="session")
def j_md(tools: Path, shm: Shm, tmp_path_factory: pytest.TempPathFactory) -> Journal:
    """``J_MD``: prod, one sealed segment, cold copy only."""
    return build_fixture(tools, shm, tmp_path_factory.mktemp("j_md"), "md", "prod", "60000")


@pytest.fixture(scope="session")
def j_sim(tools: Path, shm: Shm, tmp_path_factory: pytest.TempPathFactory) -> Journal:
    """``J_SIM``: sim instance, account ``sim-a``, 1 000 Bbo per inst."""
    return build_fixture(tools, shm, tmp_path_factory.mktemp("j_sim"), "md", "sim", "1000", instance="sim")


@pytest.fixture(scope="session")
def j_all(tools: Path, shm: Shm, tmp_path_factory: pytest.TempPathFactory) -> Journal:
    """``J_ALL``: every seq kind, two sealed segments."""
    return build_fixture(tools, shm, tmp_path_factory.mktemp("j_all"), "all")


@pytest.fixture(scope="session")
def j_all_ref(tools: Path, j_all: Journal, tmp_path_factory: pytest.TempPathFactory) -> Path:
    """``RUST_REF`` reading of ``J_ALL``: ``<dir>/<Kind>.jsonl``."""
    out = tmp_path_factory.mktemp("j_all_ref")
    pyfixture(tools, "ref", str(j_all.cold_root), "prod", str(out))
    return out


class Writer:
    """An interactive TW process (``--script -``): commands go to stdin, ``mark`` lines come back."""

    def __init__(self, tools: Path, hot: Path, cold: Path, instance: str = "prod", **kw: int) -> None:
        self.p = subprocess.Popen(
            tw_args(tools, hot, cold, instance, **kw),
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

    def send(self, *lines: str) -> None:
        assert self.p.stdin is not None
        self.p.stdin.write(("\n".join(lines) + "\n").encode())
        self.p.stdin.flush()

    def mark(self, tag: str) -> int:
        """Sends ``mark`` and returns the writer's CLOCK_MONOTONIC at that point."""
        self.send(f"mark {tag}")
        assert self.p.stdout is not None
        line = self.p.stdout.readline().decode().split()
        assert line and line[0] == tag, line
        return int(line[1])

    def close(self) -> int:
        if self.p.stdin and not self.p.stdin.closed:
            try:
                self.p.stdin.close()
            except BrokenPipeError:
                pass
        rc = self.p.wait(timeout=120)
        return rc

    def kill(self) -> None:
        self.p.kill()
        self.p.wait(timeout=30)


def synth(n: int, t0: int, step: int = MS, inst: int = 101, per_s: int = 0) -> str:
    """``synth`` command: ``n`` synthetic Bbo (README TW "合成 Bbo")."""
    line = f"synth {n} {inst} 6000000 6000001 50000000 {t0} {step}"
    return line + (f" {per_s}" if per_s else "")


def refdata_boot(tools: Path, tmp: Path, instance: str = "prod") -> str:
    """``boot`` line with the RD_PY refdata ConfigBlob."""
    gen = tmp / f"rd-{uuid.uuid4().hex[:6]}"
    gen.mkdir()
    pyfixture(tools, "md", instance, "0", str(gen))
    return next(line for line in (gen / "script.txt").read_text().splitlines() if line.startswith("boot 0x0403"))
