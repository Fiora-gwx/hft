"""Early CI selection and complete acceptance (TOOL-206, TOOL-207)."""
from contextlib import redirect_stdout
import copy
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch


SCRIPTS = Path(__file__).resolve().parents[3] / ".github" / "scripts"


def load_script(name):
    spec = importlib.util.spec_from_file_location(name, SCRIPTS / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


related = load_script("related-tests")
summary = load_script("ci-summary")


class FeedbackPolicyTests(unittest.TestCase):
    def test_tool_206_related_tests_select_the_push_and_keep_full_fallbacks(self):
        members = {"crates/a": "alpha", "crates/b": "beta", "tools/xtask": "xtask"}
        rows = [
            (["crates/a/src/lib.rs"], ["alpha"]),
            (["crates/b/tests/main.rs", "crates/a/README.md"], ["beta"]),
            (["crates/a/src/deleted.rs", "crates/b/src/moved.rs"], ["alpha", "beta"]),
            (["docs/testing.md"], []),
            (["python/ops/test_control.py"], []),
            ([".github/workflows/quality-gates.yml"], ["xtask"]),
            (["Cargo.lock"], ["alpha", "beta", "xtask"]),
            ([".cargo/config.toml"], ["alpha", "beta", "xtask"]),
            (["fixtures/dst/hashtrail.toml"], ["alpha", "beta", "xtask"]),
            (["tests/dst/main.rs"], ["alpha", "beta", "xtask"]),
        ]
        for files, expected in rows:
            with self.subTest(files=files):
                self.assertEqual(related.select(files, members)[0], expected)
        self.assertEqual(related.commands([]), [])
        commands = related.commands(["alpha", "beta"])
        self.assertIn("--examples", commands[0])
        self.assertIn("--tests", commands[0])
        self.assertIn("--all-features", commands[1])
        self.assertIn("--no-fail-fast", commands[1])
        self.assertNotIn("--ignored", commands[1])
        self.assertEqual(commands[1].count("-p"), 2)

        real_run = subprocess.run
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)

            def git(*args):
                return real_run(
                    ["git", "-c", "user.name=CI", "-c", "user.email=ci@example.invalid",
                     "-c", "commit.gpgsign=false", *args], cwd=root,
                    check=True, capture_output=True, text=True,
                ).stdout.strip()

            git("init", "-q")
            (root / "Cargo.toml").write_text('[workspace]\nmembers = ["crates/*", "tools/xtask"]\n', encoding="utf-8")
            for directory_name, name in members.items():
                package = root / directory_name
                (package / "src").mkdir(parents=True)
                (package / "Cargo.toml").write_text(f'[package]\nname = "{name}"\n', encoding="utf-8")
                (package / "src/lib.rs").write_text("// initial\n", encoding="utf-8")
            self.assertEqual(related.workspace(root), members)
            git("add", ".")
            git("commit", "-qm", "base")
            base = git("rev-parse", "HEAD")
            (root / "crates/a/src/lib.rs").write_text("// first push\n", encoding="utf-8")
            git("commit", "-qam", "previous head")
            before = git("rev-parse", "HEAD")
            (root / "crates/b/src/lib.rs").rename(root / "crates/b/src/moved.rs")
            git("add", "-A")
            git("commit", "-qm", "current push")
            head = git("rev-parse", "HEAD")
            git("checkout", "-q", "--detach", base)
            (root / "unrelated.txt").write_text("force-pushed history\n", encoding="utf-8")
            git("add", ".")
            git("commit", "-qm", "other history")
            unrelated = git("rev-parse", "HEAD")

            def in_repo(command, **kwargs):
                return real_run(command, cwd=root, **kwargs)

            with patch.object(related.subprocess, "run", side_effect=in_repo):
                files, basis = related.changed_files("pull_request", "synchronize", before, base, head)
                self.assertEqual(basis, before)
                self.assertEqual(set(files), {"crates/b/src/lib.rs", "crates/b/src/moved.rs"})
                self.assertEqual(related.select(files, members)[0], ["beta"])
                for action, prior in [("opened", before), ("reopened", before),
                                      ("synchronize", ""), ("synchronize", "0" * 40),
                                      ("synchronize", unrelated), ("synchronize", "--help")]:
                    with self.subTest(action=action, prior=prior):
                        files, basis = related.changed_files("pull_request", action, prior, base, head)
                        self.assertEqual(basis, "full-pr")
                        self.assertIn("crates/a/src/lib.rs", files)
                        self.assertEqual(related.select(files, members)[0], ["alpha", "beta"])

            # A failing command is recorded and stops the selected run; an empty selection
            # still has a complete, explicit result. Neither result changes the full gate.
            plan = {"sha": head, "packages": ["alpha"], "commands": related.commands(["alpha"]),
                    "results": [], "complete": False}
            destination = root / "plan.json"
            for code in [0, 1]:
                destination.write_text(json.dumps(plan), encoding="utf-8")
                with patch.object(related, "PLAN", destination), \
                        patch.dict(os.environ, {"HEAD": head}, clear=True), \
                        patch.object(related.sys, "argv", ["related-tests.py", "run"]), \
                        patch.object(related.subprocess, "run", return_value=subprocess.CompletedProcess([], code)) as run, \
                        redirect_stdout(io.StringIO()):
                    self.assertEqual(related.main(), code)
                saved = json.loads(destination.read_text(encoding="utf-8"))
                self.assertEqual(saved["complete"], code == 0)
                self.assertEqual(run.call_count, 2 if code == 0 else 1)
                self.assertEqual(saved["results"][0]["exit_code"], code)

    def test_tool_207_acceptance_requires_every_applicable_job(self):
        needs = {name: {"result": "success"} for name in summary.JOBS}
        needs["paths"]["outputs"] = {"docs_only": "false", "bench_crates": "hft-oms",
                                       "benchmarks": "false", "toolchain": "false"}
        self.assertTrue(summary.evaluate(needs)[0])
        for job in summary.JOBS:
            for state in ["failure", "cancelled", "skipped", None]:
                with self.subTest(job=job, state=state):
                    broken = copy.deepcopy(needs)
                    if state is None:
                        del broken[job]
                    else:
                        broken[job]["result"] = state
                    self.assertFalse(summary.evaluate(broken)[0])
        no_bench = copy.deepcopy(needs)
        no_bench["paths"]["outputs"]["bench_crates"] = ""
        no_bench["benchmarks"]["result"] = "skipped"
        self.assertTrue(summary.evaluate(no_bench)[0])
        del no_bench["paths"]["outputs"]["benchmarks"]
        self.assertFalse(summary.evaluate(no_bench)[0])
        docs = copy.deepcopy(needs)
        docs["paths"]["outputs"]["docs_only"] = "true"
        for job in summary.CODE_JOBS:
            docs[job]["result"] = "skipped"
        self.assertTrue(summary.evaluate(docs)[0])
        for job in ["paths", "docs", "decoupling-gate"]:
            broken = copy.deepcopy(docs)
            broken[job]["result"] = "failure"
            self.assertFalse(summary.evaluate(broken)[0])
        docs["rust"]["result"] = "failure"
        self.assertFalse(summary.evaluate(docs)[0])
        fast_only = copy.deepcopy(needs)
        for job in ["rust", "arch-check", "benchmarks", "ops-py", "replay-dst"]:
            fast_only[job]["result"] = "skipped"
        self.assertFalse(summary.evaluate(fast_only)[0])


if __name__ == "__main__":
    unittest.main()
