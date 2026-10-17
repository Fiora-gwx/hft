"""Release consumes the CI lane verdict; skipped jobs alone never constitute acceptance."""
import importlib.util
from pathlib import Path
import sys

import pytest

SCRIPTS = Path(__file__).parents[2] / ".github/scripts"
sys.path.insert(0, str(SCRIPTS))
spec = importlib.util.spec_from_file_location("hft_release_preflight", SCRIPTS / "release-preflight.py")
preflight = importlib.util.module_from_spec(spec)
spec.loader.exec_module(preflight)
sys.path.pop(0)


@pytest.mark.parametrize("verdict,accepted", [("success", True), ("skipped", False), ("failure", False), (None, False)])
def test_dep_251_release_requires_lane_acceptance(monkeypatch, verdict, accepted):
    def paged(path, key, params=None, **kwargs):
        if key == "workflow_runs":
            assert params == {"head_sha": "a" * 40, "status": "success"}
            return [{"id": 17}]
        jobs = [{"name": "quality-gates / rust", "conclusion": "skipped"},
                {"name": "quality-gates / fast-clippy", "conclusion": "success"}]
        if verdict is not None:
            jobs.append({"name": "quality-gates / acceptance", "conclusion": verdict})
        return jobs
    monkeypatch.setattr(preflight.gh, "paged", paged)
    if accepted:
        assert preflight.check_ci("example/hft", "a" * 40) == 17
    else:
        with pytest.raises(preflight.Fail, match="acceptance"):
            preflight.check_ci("example/hft", "a" * 40)


@pytest.mark.parametrize("verdict", ["success", "failure", "cancelled", None])
def test_dep_251_nightly_skip_preserves_latest_decoupling_verdict(monkeypatch, verdict):
    checks = [
        {"id": 1, "name": "decoupling-gate", "started_at": "2026-10-16T17:00:00Z",
         "status": "completed", "conclusion": "success"},
        {"id": 2, "name": "quality-gates / decoupling-gate", "started_at": "2026-10-16T18:00:00Z",
         "status": "completed" if verdict else "in_progress", "conclusion": verdict},
        {"id": 3, "name": "quality-gates / decoupling-gate", "started_at": "2026-10-16T21:00:00Z",
         "status": "completed", "conclusion": "skipped"},
    ]
    monkeypatch.setattr(preflight.gh, "paged", lambda *args, **kwargs: checks)
    if verdict == "success":
        assert preflight.check_decoupling("example/hft", "a" * 40) == 2
    else:
        with pytest.raises(preflight.Fail, match="check run 2"):
            preflight.check_decoupling("example/hft", "a" * 40)


def test_dep_251_skipped_decoupling_is_not_release_evidence(monkeypatch):
    checks = [{"id": 3, "name": "quality-gates / decoupling-gate", "conclusion": "skipped"}]
    monkeypatch.setattr(preflight.gh, "paged", lambda *args, **kwargs: checks)
    with pytest.raises(preflight.Fail, match="no non-skipped"):
        preflight.check_decoupling("example/hft", "a" * 40)
