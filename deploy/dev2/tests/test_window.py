import importlib.util
from pathlib import Path

import pytest

spec = importlib.util.spec_from_file_location("window", Path(__file__).parents[1] / "window.py")
window = importlib.util.module_from_spec(spec)
spec.loader.exec_module(window)


def dep_079_window_serializes_batch_and_preserves_failure_marker(tmp_path):
    calls = []
    marker = tmp_path / "run/hft/window/active"
    def run(*args):
        calls.append(args)
        if args[1] in ("stop", "start"):
            assert marker.exists()
        return b"inactive" if "--property=ActiveState" in args else b"loaded"
    window.window("start", tmp_path, run)
    assert marker.exists()
    assert calls[-1] == ("systemctl", "start", window.PROD)
    assert [a[-1] for a in calls if a[1] == "stop"] == list(window.BATCH)
    with pytest.raises(ValueError, match="hot pages remain"):
        window.window("stop", tmp_path, run, usage=lambda path: 8192)
    assert marker.exists()
    window.window("stop", tmp_path, run, usage=lambda path: 4096)
    assert not marker.exists()
    assert all(a[-1].startswith("hft-") for a in calls)
    calls.clear()
    window.window("start", tmp_path, run, dry_run=True)
    assert not calls and not marker.exists()
    def failed(*args):
        if args[1] == "start":
            raise OSError("start failed")
        return b"not-found"
    with pytest.raises(OSError):
        window.window("start", tmp_path, failed)
    assert marker.exists()
    with pytest.raises(ValueError, match="not stopped"):
        window.window("stop", tmp_path, lambda *args: b"active")
    assert marker.exists()
    trip = tmp_path / "var/lib/hft/guard/tripped"
    trip.parent.mkdir(parents=True)
    trip.touch()
    with pytest.raises(ValueError, match="tripped"):
        window.window("start", tmp_path, run)
