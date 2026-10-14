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
