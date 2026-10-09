"""tail(): new records while TW writes, rolls, retirement, crash recovery and visibility latency
(TESTS.md PY-070–PY-080)."""

from __future__ import annotations

import contextlib
import signal
import sys
import threading
import time
from pathlib import Path
from typing import Iterator

import numpy as np
import pytest

import hft_py
from conftest import D, MS, REPO, S, Journal, Shm, Writer, refdata_boot, synth

sys.path.insert(0, str(REPO / "python"))
from _generated import events as gen  # noqa: E402


@contextlib.contextmanager
def deadline(seconds: float) -> Iterator[None]:
    """Raises TimeoutError in the main thread after `seconds` (tail checks signals between polls)."""

    def on_alarm(signum: int, frame: object) -> None:
        raise TimeoutError

    old = signal.signal(signal.SIGALRM, on_alarm)
    signal.setitimer(signal.ITIMER_REAL, seconds)
    try:
        yield
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)
        signal.signal(signal.SIGALRM, old)


def start(tools: Path, shm: Shm, tmp: Path, *boot: str, **kw: int) -> tuple[Writer, Path, Path]:
    """A running TW whose first segment (boot block only) is published."""
    hot = shm.dir("tail")
    cold = tmp / "cold"
    w = Writer(tools, hot, cold, **kw)
    w.send(*boot, "publish")
    w.mark("ready")
    return w, hot, cold


def open_j(hot: Path, cold: Path) -> object:
    return hft_py.open_journal(str(cold / "prod" / "seq"), hot_root=str(hot))


def collect(it: object, n: int, seconds: float) -> list[tuple[str, list[int]]]:
    out: list[tuple[str, list[int]]] = []
    total = 0
    with deadline(seconds):
        for b in it:  # type: ignore[attr-defined]
            out.append((b.metadata["kind"], list(b["seq"])))
            total += b.num_rows
            if total >= n:
                break
    return out


def seqs(batches: list[tuple[str, list[int]]]) -> list[int]:
    return [s for _, ss in batches for s in ss]


def test_py_070_tail_no_loss_no_dup(tools: Path, shm: Shm, tmp_path: Path) -> None:
    w, hot, cold = start(tools, shm, tmp_path)
    it = open_j(hot, cold).tail(["Bbo"], from_seq=None, poll_ms=50)  # type: ignore[attr-defined]
    w.send(synth(200_000, D + 10 * S, per_s=20_000), "flush")
    got = seqs(collect(it, 200_000, 30))
    w.close()
    assert len(got) == 200_000
    assert got == sorted(got) and len(set(got)) == len(got)
    ref = [s for s in hft_py.open_journal(str(cold / "prod" / "seq"), hot_root=str(hot)).events("Bbo") for s in s["seq"]]
    assert got == ref
    assert len(open_j(hot, cold).info()["segments"]) == 4  # type: ignore[attr-defined]


def test_py_071_roll_reads_boot_block(tools: Path, shm: Shm, tmp_path: Path) -> None:
    w, hot, cold = start(tools, shm, tmp_path, refdata_boot(tools, tmp_path))
    j = open_j(hot, cold)
    it = j.tail(["ConfigBlob"], poll_ms=50)  # type: ignore[attr-defined]
    w.send(synth(200_000, D + 10 * S, per_s=20_000), "flush")
    got = collect(it, 3, 30)
    w.close()
    rows = seqs(got)
    assert len(rows) == 3 and len(set(rows)) == 3
    segs = open_j(hot, cold).info()["segments"]  # type: ignore[attr-defined]
    for s, seg in zip(rows, segs[1:]):
        assert seg["first_seq"] <= s < seg["first_seq"] + 8


def test_py_072_from_seq_none(tools: Path, shm: Shm, tmp_path: Path) -> None:
    w, hot, cold = start(tools, shm, tmp_path)
    w.send(synth(1000, D + 10 * S))
    w.mark("a")
    j = open_j(hot, cold)
    s = j.info()["last_seq"]  # type: ignore[attr-defined]
    it = j.tail(["Bbo"], from_seq=None, poll_ms=50)  # type: ignore[attr-defined]
    w.send(synth(10, D + 20 * S))
    got = seqs(collect(it, 10, 10))
    w.close()
    assert len(got) == 10 and all(x > s for x in got)


def test_py_073_from_seq_bounds(tools: Path, shm: Shm, tmp_path: Path) -> None:
    w, hot, cold = start(tools, shm, tmp_path)
    w.send(synth(1000, D + 10 * S))
    w.mark("a")
    j = open_j(hot, cold)
    info = j.info()  # type: ignore[attr-defined]
    o, last = info["first_seq"], info["last_seq"]
    first = collect(j.tail(["Bbo"], from_seq=o, poll_ms=10), 1, 10)  # type: ignore[attr-defined]
    assert first[0][1][0] >= o, "PY-073.1"
    with pytest.raises(ValueError):
        j.tail(["Bbo"], from_seq=o - 1)  # type: ignore[attr-defined]
    with pytest.raises(ValueError):
        j.tail(["Bbo"], from_seq=last + 2)  # type: ignore[attr-defined]
    it = j.tail(["Bbo"], from_seq=last + 1, poll_ms=10)  # type: ignore[attr-defined]
    w.send(synth(5, D + 20 * S))
    got = seqs(collect(it, 5, 10))
    assert len(got) == 5 and min(got) > last, "PY-073.3"
    w.close()


def test_py_074_poll_ms_bounds(tools: Path, shm: Shm, tmp_path: Path) -> None:
    w, hot, cold = start(tools, shm, tmp_path)
    w.send(synth(10, D + 10 * S))
    w.mark("a")
    j = open_j(hot, cold)
    for p in (0, -1):
        with pytest.raises(ValueError):
            j.tail(["Bbo"], poll_ms=p)  # type: ignore[attr-defined]
    j.tail(["Bbo"], poll_ms=1)  # type: ignore[attr-defined]
    testing = getattr(hft_py, "_testing", None)
    assert testing is not None, "hft_py was built without the `testing` feature"
    it = j.tail(["Bbo"])  # type: ignore[attr-defined]
    stop = threading.Event()

    def run() -> None:
        for _ in it:
            if stop.is_set():
                return

    th = threading.Thread(target=run, daemon=True)
    th.start()
    time.sleep(0.1)
    n0 = testing.tail_polls()
    time.sleep(2.0)
    n1 = testing.tail_polls()
    stop.set()
    w.send(synth(1, D + 30 * S))
    th.join(5)
    w.close()
    assert 9 <= n1 - n0 <= 11, n1 - n0


def _bbo_hex(t: int) -> str:
    a = np.zeros(1, dtype=gen.BBO)
    a["h"]["inst"] = 101
    a["h"]["venue"] = 2
    a["bid_px"] = 6_000_000
    a["ask_px"] = 6_000_001
    return a.tobytes().hex()


def _note_hex() -> str:
    a = np.zeros(1, dtype=gen.NOTE)
    a["code"] = 1
    a["source"] = 1  # Sequencer: an Output note
    return a.tobytes().hex()


class Collector(threading.Thread):
    def __init__(self, it: object) -> None:
        super().__init__(daemon=True)
        self.it = it
        self.rows: list[tuple[str, int]] = []
        self.stop = threading.Event()

    def run(self) -> None:
        for b in self.it:  # type: ignore[attr-defined]
            for s in b["seq"]:
                self.rows.append((b.metadata["kind"], s))
            if self.stop.is_set():
                return


def test_py_075_only_published_events(tools: Path, shm: Shm, tmp_path: Path) -> None:
    w, hot, cold = start(tools, shm, tmp_path, publish_every=1_000_000)
    it = open_j(hot, cold).tail(["Bbo", "Note"], from_seq=None, poll_ms=10)  # type: ignore[attr-defined]
    c = Collector(it)
    c.start()
    tsc = 3 * 10 * S
    w.send(f"rec 0x0101 0 {tsc} {_bbo_hex(D + 10 * S)}", f"rec 0x0506 0 {tsc} {_note_hex()}")
    w.mark("partial")
    time.sleep(0.2)
    assert c.rows == []
    w.send("end", "publish")
    w.mark("done")
    time.sleep(0.2)
    c.stop.set()
    assert sorted(k for k, _ in c.rows) == ["Bbo", "Note"]
    w.send(synth(1, D + 30 * S), "publish")
    c.join(5)
    w.close()


def test_py_076_kind_runs(tools: Path, shm: Shm, tmp_path: Path) -> None:
    w, hot, cold = start(tools, shm, tmp_path, publish_every=1_000_000)
    it = open_j(hot, cold).tail(["Bbo", "OrderEvent"], poll_ms=10)  # type: ignore[attr-defined]
    oe = np.zeros(1, dtype=gen.ORDER_EVENT)
    oe["what"] = 1
    oe["account"] = 1
    oe_hex = oe.tobytes().hex()
    tsc = 3 * 10 * S
    lines = []
    for k in ["b", "b", "o", "b", "o", "o"]:
        lines.append(f"rec {'0x0101' if k == 'b' else '0x0203'} 0 {tsc} {_bbo_hex(D) if k == 'b' else oe_hex}")
        lines.append("end")
    w.send(*lines, "publish")
    got = collect(it, 6, 10)
    w.close()
    assert [(k, len(s)) for k, s in got] == [("Bbo", 2), ("OrderEvent", 1), ("Bbo", 1), ("OrderEvent", 2)]
    allseq = seqs(got)
    assert allseq == sorted(allseq) and len(set(allseq)) == 6


def test_py_077_tail_unknown_kind(j_md: Journal) -> None:
    j = hft_py.open_journal(j_md.path)
    for kinds in (["Bbo", "Nope"], [0x01FF]):
        with pytest.raises(ValueError):
            j.tail(kinds)
    j.tail(["Bbo", 0x0101])


def test_py_078_retired_segment_unmapped(tools: Path, shm: Shm, tmp_path: Path) -> None:
    w, hot, cold = start(tools, shm, tmp_path)
    it = open_j(hot, cold).tail(["Bbo"], from_seq=None, poll_ms=50)  # type: ignore[attr-defined]
    w.send(synth(200_000, D + 10 * S, per_s=20_000), "flush")
    got: list[int] = []
    seg1 = hot / "prod" / "seq" / f"{1:010d}.jrn"
    deleted_at = None
    with deadline(40):
        for b in it:  # type: ignore[attr-defined]
            got.extend(b["seq"])
            if deleted_at is None and list(cold.glob("prod/seq/*/0000000001-*.seal")):
                seg1.unlink()
                deleted_at = time.monotonic()
            if len(got) >= 200_000:
                break
    w.close()
    assert deleted_at is not None
    assert len(got) == 200_000 and len(set(got)) == len(got) and got == sorted(got)
    time.sleep(max(0.0, 2.0 - (time.monotonic() - deleted_at)))
    maps = Path("/proc/self/maps").read_text()
    assert not [m for m in maps.splitlines() if str(seg1) in m and "(deleted)" in m]


def test_py_079_writer_crash_recovery(tools: Path, shm: Shm, tmp_path: Path) -> None:
    w, hot, cold = start(tools, shm, tmp_path)
    it = open_j(hot, cold).tail(["Bbo"], from_seq=None, poll_ms=50)  # type: ignore[attr-defined]
    w.send(synth(5000, D + 10 * S), f"rec 0x0101 0 {3 * 20 * S} {_bbo_hex(D)}")
    w.mark("half")
    first = seqs(collect(it, 5000, 20))
    assert len(first) == 5000
    s = first[-1]
    w.kill()
    w2 = Writer(tools, hot, cold)
    w2.send(synth(1000, D + 30 * S))
    more = seqs(collect(it, 1000, 30))
    w2.close()
    assert len(more) == 1000 and min(more) > s and more == sorted(more)


def test_py_080_tail_visibility_latency(tools: Path, shm: Shm, tmp_path: Path) -> None:
    w, hot, cold = start(tools, shm, tmp_path)
    it = open_j(hot, cold).tail(["Bbo"], from_seq=None, poll_ms=200)  # type: ignore[attr-defined]
    marks: list[int] = []

    def read_marks() -> None:
        assert w.p.stdout is not None
        while len(marks) < 50:
            line = w.p.stdout.readline().decode().split()
            if not line:
                return
            marks.append(int(line[1]))

    lines = []
    for i in range(50):
        lines += [synth(1, D + (10 + i) * S), f"mark m{i}", "sleep 1000"]
    th = threading.Thread(target=read_marks, daemon=True)
    th.start()
    w.send(*lines)
    seen: list[int] = []
    with deadline(80):
        for b in it:  # type: ignore[attr-defined]
            now = time.monotonic_ns()
            seen.extend([now] * b.num_rows)
            if len(seen) >= 50:
                break
    th.join(5)
    w.close()
    lat = [(a - m) / 1e6 for a, m in zip(seen, marks)]
    assert len(lat) == 50
    assert max(lat) <= 205, sorted(lat)[-5:]
