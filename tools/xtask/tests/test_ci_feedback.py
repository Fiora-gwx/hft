"""PR fast-lane selection and per-lane acceptance (TOOL-206, TOOL-207)."""
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


def write_workspace(root: Path) -> dict:
    """alpha <- beta (dev-dependency through a workspace rename) ; xtask alone."""
    members = {"crates/a": "alpha", "crates/b": "beta", "tools/xtask": "xtask"}
    (root / "Cargo.toml").write_text(
        '[workspace]\nmembers = ["crates/*", "tools/xtask"]\n\n'
        '[workspace.dependencies]\nalpha-renamed = { path = "crates/a", package = "alpha" }\n',
        encoding="utf-8")
    for directory, name in members.items():
        package = root / directory
        (package / "src").mkdir(parents=True)
        (package / "src/lib.rs").write_text("// initial\n", encoding="utf-8")
        manifest = f'[package]\nname = "{name}"\n'
        if name == "beta":
            manifest += '\n[dev-dependencies]\nalpha-renamed.workspace = true\n'
            (package / "src/fx.rs").write_text('const F: &str = "fixtures/foo/a.bin";\n', encoding="utf-8")
        (package / "Cargo.toml").write_text(manifest, encoding="utf-8")
    return members


class FeedbackPolicyTests(unittest.TestCase):
    def test_tool_206_fast_lane_selects_affected_packages_and_reverse_deps(self):
        real_run = subprocess.run
        # Background git maintenance can still be writing .git when the directory is removed.
        with tempfile.TemporaryDirectory(ignore_cleanup_errors=True) as directory:
            root = Path(directory)
            members = write_workspace(root)
            self.assertEqual(related.workspace(root), members)
            deps = related.dependencies(root)
            self.assertEqual(deps, {"alpha": set(), "beta": {"alpha"}, "xtask": set()})
            rows = [
                # TOOL-206.1: a changed member plus every member that depends on it.
                (["crates/a/src/lib.rs"], ["alpha", "beta"]),
                (["crates/b/tests/main.rs", "crates/a/README.md"], ["beta"]),
                # TOOL-206.3: both ends of a move.
                (["crates/a/src/deleted.rs", "crates/b/src/moved.rs"], ["alpha", "beta"]),
                # TOOL-206.5
                (["docs/testing.md"], []),
                (["python/ops/test_control.py"], []),
                # TOOL-206.6
                ([".github/workflows/quality-gates.yml"], ["xtask"]),
                # TOOL-206.4
                (["Cargo.lock"], ["alpha", "beta", "xtask"]),
                ([".cargo/config.toml"], ["alpha", "beta", "xtask"]),
                (["tests/dst/main.rs"], ["alpha", "beta", "xtask"]),
                (["fixtures/foo/a.bin"], ["beta"]),
                (["fixtures/nobody/x.toml"], ["alpha", "beta", "xtask"]),
            ]
            for files, expected in rows:
                with self.subTest(files=files):
                    self.assertEqual(related.select(files, members, deps, root)[0], expected)

            def git(*args):
                return real_run(
                    ["git", "-c", "user.name=CI", "-c", "user.email=ci@example.invalid",
                     "-c", "commit.gpgsign=false", "-c", "gc.auto=0", "-c", "maintenance.auto=false",
                     *args], cwd=root,
                    check=True, capture_output=True, text=True,
                ).stdout.strip()

            # TOOL-206.2: the complete PR diff (merge base), never only the latest push.
            git("init", "-q")
            git("add", ".")
            git("commit", "-qm", "base")
            base = git("rev-parse", "HEAD")
            (root / "crates/a/src/lib.rs").write_text("// first push\n", encoding="utf-8")
            git("commit", "-qam", "previous head")
            (root / "crates/b/src/lib.rs").rename(root / "crates/b/src/moved.rs")
            git("add", "-A")
            git("commit", "-qm", "current push")
            head = git("rev-parse", "HEAD")

            def in_repo(command, **kwargs):
                return real_run(command, cwd=root, **kwargs)

            loader = importlib.util.spec_from_file_location("ci_paths", SCRIPTS / "changed-paths.py")
            cp = importlib.util.module_from_spec(loader)
            loader.loader.exec_module(cp)
            with patch.object(cp.subprocess, "run", side_effect=in_repo):
                files = cp.changed_files("pull_request", base, head)
            self.assertEqual(set(files), {"crates/a/src/lib.rs", "crates/b/src/lib.rs", "crates/b/src/moved.rs"})
            self.assertEqual(related.select(files, members, deps, root)[0], ["alpha", "beta"])

            # TOOL-206.7: one nextest command over the selection; no proc binaries, no heavy::.
            self.assertEqual(related.commands([]), [])
            command = related.commands(["alpha", "beta"])
            self.assertEqual(len(command), 1)
            self.assertEqual(command[0][:3], ["cargo", "nextest", "run"])
            for arg in ["--locked", "--all-features", "--no-fail-fast", "--cargo-profile"]:
                self.assertIn(arg, command[0])
            self.assertEqual(command[0][command[0].index("--cargo-profile") + 1], "ci")
            self.assertEqual(command[0].count("-p"), 2)
            self.assertEqual(command[0][command[0].index("-E") + 1],
                             "not (binary(/^proc/) | binary(compile_fail) | test(/heavy::/)"
                             " | test(rpl_012_dst_journals_verify))")
            self.assertNotIn("--ignored", command[0])
            self.assertNotIn("--partition", command[0])
            sharded = related.commands(["alpha"], "2/2")[0]
            self.assertEqual(sharded[-2:], ["--partition", "count:2/2"])

            # plan: step outputs; dst only when hft-testkit is affected.
            out = root / "out.txt"
            for changed, dst in [(["crates/a/src/lib.rs"], "false")]:
                out.write_text("", encoding="utf-8")
                with patch.object(related, "changed_files", return_value=changed), \
                        patch.object(related, "PLAN", root / "plan.json"), \
                        patch.object(related.Path, "cwd", return_value=root), \
                        patch.dict(os.environ, {"HEAD": head, "GITHUB_OUTPUT": str(out)}, clear=True), \
                        patch.object(related.sys, "argv", ["related-tests.py", "plan"]), \
                        redirect_stdout(io.StringIO()):
                    self.assertEqual(related.main(), 0)
                outputs = dict(line.split("=", 1) for line in out.read_text(encoding="utf-8").splitlines())
                self.assertEqual(outputs, {"has_tests": "true", "packages": "alpha beta", "dst": dst})

            # A failing command is recorded and stops the run; success marks the plan complete.
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
                self.assertEqual(run.call_count, 1)
                self.assertEqual(saved["results"][0]["exit_code"], code)

    def test_tool_207_acceptance_requires_every_applicable_job(self):
        outputs = {"docs_only": "false", "bench_crates": "hft-oms", "benchmarks": "false",
                   "toolchain": "false", "has_tests": "true", "dst": "true"}
        for lane in ["fast", "full"]:
            needs = {name: {"result": "success"} for name in summary.LANES[lane]}
            needs["paths"]["outputs"] = dict(outputs)
            # TOOL-207.1
            with self.subTest(lane=lane):
                self.assertTrue(summary.evaluate(needs, lane, "push")[0])
            # TOOL-207.2
            for job in summary.LANES[lane]:
                for state in ["failure", "cancelled", "skipped", None]:
                    with self.subTest(lane=lane, job=job, state=state):
                        broken = copy.deepcopy(needs)
                        if state is None:
                            del broken[job]
                        else:
                            broken[job]["result"] = state
                        self.assertFalse(summary.evaluate(broken, lane, "push")[0])
            # TOOL-207.5: the other lane's jobs are not evaluated; this lane's are not replaceable.
            other = "full" if lane == "fast" else "fast"
            swapped = {name: {"result": "success"} for name in summary.LANES[other]}
            swapped["paths"]["outputs"] = dict(outputs)
            for job in set(summary.LANES[lane]) - set(summary.LANES[other]):
                swapped[job] = {"result": "skipped"}
            self.assertFalse(summary.evaluate(swapped, lane, "push")[0])
        self.assertFalse(summary.evaluate({}, "medium", "push")[0])

        full = {name: {"result": "success"} for name in summary.LANES["full"]}
        full["paths"]["outputs"] = dict(outputs)
        # TOOL-207.3
        no_bench = copy.deepcopy(full)
        no_bench["paths"]["outputs"]["bench_crates"] = ""
        no_bench["benchmarks"]["result"] = "skipped"
        self.assertTrue(summary.evaluate(no_bench, "full", "push")[0])
        # GitHub drops empty-string outputs from toJSON(needs).
        omitted = copy.deepcopy(no_bench)
        del omitted["paths"]["outputs"]["bench_crates"]
        self.assertTrue(summary.evaluate(omitted, "full", "push")[0])
        del no_bench["paths"]["outputs"]["benchmarks"]
        self.assertFalse(summary.evaluate(no_bench, "full", "push")[0])
        # TOOL-207.6: nightly (schedule) has no diff for the decoupling gate.
        nightly = copy.deepcopy(full)
        nightly["decoupling-gate"]["result"] = "skipped"
        self.assertTrue(summary.evaluate(nightly, "full", "schedule")[0])
        self.assertFalse(summary.evaluate(nightly, "full", "push")[0])

        fast = {name: {"result": "success"} for name in summary.LANES["fast"]}
        fast["paths"]["outputs"] = dict(outputs)
        # TOOL-207.4: docs-only PR.
        docs = copy.deepcopy(fast)
        docs["paths"]["outputs"]["docs_only"] = "true"
        for job in set(summary.LANES["fast"]) & summary.CODE_JOBS:
            docs[job]["result"] = "skipped"
        self.assertTrue(summary.evaluate(docs, "fast", "pull_request")[0])
        for job in ["paths", "docs", "decoupling-gate"]:
            broken = copy.deepcopy(docs)
            broken[job]["result"] = "failure"
            self.assertFalse(summary.evaluate(broken, "fast", "pull_request")[0])
        # TOOL-207.7: no Rust package / hft-testkit not affected.
        no_dst = copy.deepcopy(fast)
        no_dst["paths"]["outputs"]["dst"] = "false"
        no_dst["fast-dst"]["result"] = "skipped"
        self.assertTrue(summary.evaluate(no_dst, "fast", "pull_request")[0])
        no_rust = copy.deepcopy(no_dst)
        no_rust["paths"]["outputs"]["has_tests"] = "false"
        for job in ["fast-test", "fast-clippy"]:
            no_rust[job]["result"] = "skipped"
        self.assertTrue(summary.evaluate(no_rust, "fast", "pull_request")[0])
        skipped_dst = copy.deepcopy(fast)
        skipped_dst["fast-dst"]["result"] = "skipped"
        self.assertFalse(summary.evaluate(skipped_dst, "fast", "pull_request")[0])


if __name__ == "__main__":
    unittest.main()
