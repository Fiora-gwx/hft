"""Release-policy regression cases (tools/TESTS.md TOOL-187..190)."""
from contextlib import redirect_stdout
import importlib.util
import io
import json
import os
from pathlib import Path
import sys
import unittest
from unittest.mock import patch


SCRIPTS = Path(__file__).resolve().parents[3] / ".github" / "scripts"
sys.path.insert(0, str(SCRIPTS))


def load_script(name):
    spec = importlib.util.spec_from_file_location(name, SCRIPTS / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


preflight = load_script("release-preflight")
nightly_summary = load_script("nightly-summary")


class GitHubEvidence:
    """Only verdict endpoints exist; an accidental usage scan fails the test."""

    def __init__(self, rust="success", decoupling="success", nightly="success"):
        self.rust = rust
        self.decoupling = decoupling
        self.nightly = nightly
        self.calls = []

    def paged(self, path, key, params=None, limit=5000):
        self.calls.append(path)
        if path.endswith("/actions/workflows/ci.yml/runs"):
            assert params == {"head_sha": "a" * 40, "status": "success"}
            return [{"id": 11}]
        if path.endswith("/actions/runs/11/jobs"):
            return [{"name": "quality-gates / rust", "conclusion": self.rust}]
        if path.endswith("/check-runs"):
            return [
                {"id": 20, "name": "decoupling-gate", "started_at": "2026-10-09", "conclusion": "success"},
                {"id": 21, "name": "decoupling-gate", "started_at": "2026-10-10", "conclusion": self.decoupling},
            ]
        if path.endswith("/actions/workflows/nightly.yml/runs"):
            assert params == {"branch": "main", "status": "completed"}
            return [{"id": 31, "conclusion": self.nightly}]
        if path.endswith("/actions/runs/31/jobs"):
            return [{"name": "nightly", "conclusion": self.nightly}]
        raise AssertionError(f"Unexpected GitHub request: {path}")


def run_preflight(evidence, channel="dev", actor="collaborator", on_main=True, **inputs):
    environment = {"GITHUB_REPOSITORY": "owner/hft", "ACTOR": actor, "EVENT": "workflow_dispatch", **inputs}
    decision = {"sha": "a" * 40, "tag": "v0.1.0" if channel == "release" else "v0.1.0-dev.1",
                "version": "0.1.0", "channel": channel, "create_tag": channel == "dev"}
    with patch.dict(os.environ, environment, clear=True), \
            patch.object(preflight, "decide", return_value=decision), \
            patch.object(preflight, "is_ancestor_of_main", return_value=on_main), \
            patch.object(preflight, "is_branch_head", return_value=True), \
            patch.object(preflight.gh, "is_admin", side_effect=lambda login: login == "admin"), \
            patch.object(preflight.gh, "paged", side_effect=evidence.paged), \
            patch.object(preflight, "out") as output, \
            patch.object(preflight, "summary"), redirect_stdout(io.StringIO()):
        result = preflight.main()
    return result, output.call_args.kwargs if output.called else None


class ReleasePolicyTests(unittest.TestCase):
    def test_tool_187_dev_requires_executed_ci_without_usage_scan(self):
        for rust, dg, expected in [("success", "success", 0), ("skipped", "success", 1),
                                   ("failure", "success", 1), ("success", "failure", 1)]:
            with self.subTest(rust=rust, decoupling=dg):
                api = GitHubEvidence(rust=rust, decoupling=dg)
                result, output = run_preflight(api)
                self.assertEqual(result, expected)
                self.assertFalse(any("nightly.yml" in path for path in api.calls))
                if output:
                    evidence = json.loads(output["evidence"])
                    self.assertEqual(evidence, {"ci_run_id": 11, "decoupling_check_run_id": 21,
                                               "nightly_run_id": None, "overrides": []})
        with patch.object(GitHubEvidence, "paged", side_effect=preflight.gh.ApiError("unavailable")):
            self.assertEqual(run_preflight(GitHubEvidence())[0], 2)

    def test_tool_188_admin_nightly_override_preserves_other_gates(self):
        rows = [
            ("collaborator", False, "success", "success", "success", True, 0),
            ("collaborator", False, "success", "success", "failure", True, 1),
            ("collaborator", True, "success", "success", "failure", True, 1),
            ("admin", True, "success", "success", "failure", True, 0),
            ("admin", True, "skipped", "success", "failure", True, 1),
            ("admin", True, "success", "failure", "failure", True, 1),
            ("admin", True, "success", "success", "failure", False, 1),
        ]
        for actor, skip, rust, dg, nightly, on_main, expected in rows:
            with self.subTest(actor=actor, skip=skip, rust=rust, dg=dg, nightly=nightly, on_main=on_main):
                api = GitHubEvidence(rust, dg, nightly)
                result, output = run_preflight(api, "release", actor, on_main,
                                               IN_SKIP_NIGHTLY=str(skip).lower())
                self.assertEqual(result, expected)
                if skip:
                    self.assertFalse(any("nightly.yml" in path for path in api.calls))
                if output:
                    evidence = json.loads(output["evidence"])
                    self.assertEqual(evidence["overrides"], [{"input": "skip_nightly", "actor": "admin"}] if skip else [])
                    self.assertEqual(evidence["nightly_run_id"], None if skip else 31)
        for actor, expected in [("collaborator", 1), ("admin", 0)]:
            result, output = run_preflight(GitHubEvidence(), "release", actor, IN_SKIP_VERIFY="true")
            self.assertEqual(result, expected)
            if output:
                self.assertTrue(output["skip_verify"])
                self.assertEqual(json.loads(output["evidence"])["overrides"], [{"input": "skip_verify", "actor": "admin"}])

    def test_tool_189_latest_executed_nightly_must_pass(self):
        for latest, expected in [("success", 32), ("failure", None), ("cancelled", None)]:
            def pages(path, key, params=None, limit=5000):
                if path.endswith("nightly.yml/runs"):
                    return [{"id": 31, "conclusion": "success"},
                            {"id": 32, "conclusion": latest}, {"id": 33, "conclusion": "success"}]
                return [{"name": "nightly", "conclusion": "skipped" if "/31/" in path else latest}]
            with self.subTest(latest=latest), patch.object(preflight.gh, "paged", side_effect=pages):
                if expected:
                    self.assertEqual(preflight.check_nightly("owner/hft"), expected)
                else:
                    with self.assertRaises(preflight.Fail):
                        preflight.check_nightly("owner/hft")
        with patch.object(preflight.gh, "paged", return_value=[]), self.assertRaises(preflight.Fail):
            preflight.check_nightly("owner/hft")

    def test_tool_190_nightly_summary_reports_jobs_without_quota_state(self):
        rows = [("success", ["PR #2 approved"]), ("success", preflight.gh.ApiError("unavailable")),
                ("failure", []), ("cancelled", []), (None, [])]
        for replay, exemptions in rows:
            env = {"NIGHTLY_RESULT": "success", "ARM_RESULT": "failure", "ARM_CACHE_HIT": "false"}
            if replay is not None:
                env["REPLAY_DST_RESULT"] = replay
            kwargs = {"side_effect": exemptions} if isinstance(exemptions, Exception) else {"return_value": exemptions}
            stream = io.StringIO()
            with patch.dict(os.environ, env, clear=True), \
                    patch.object(nightly_summary, "exemptions", **kwargs), redirect_stdout(stream):
                self.assertEqual(nightly_summary.main(), 0)
            message = stream.getvalue()
            self.assertIn("nightly (x86): success", message)
            self.assertIn("nightly-arm: failure (rust-cache miss", message)
            self.assertIn(f"replay-dst (date): {replay or '?'}", message)
            expected = "unavailable" if isinstance(exemptions, Exception) else "; ".join(exemptions) or "none"
            self.assertIn(f"decoupling exemptions (24 h): {expected}", message)
            self.assertNotIn("QuotaSkip", message)
            self.assertNotIn("CI usage", message)


if __name__ == "__main__":
    unittest.main()
