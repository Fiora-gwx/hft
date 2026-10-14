"""CI path classification regression (tools/TESTS.md TOOL-191)."""
from contextlib import redirect_stderr, redirect_stdout
import importlib.util
import io
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).resolve().parents[3] / ".github" / "scripts" / "changed-paths.py"
SPEC = importlib.util.spec_from_file_location("changed_paths", SCRIPT)
paths = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(paths)


class PathPolicyTests(unittest.TestCase):
    def check_gen_host_inventory_requires_generator(self):
        bash = shutil.which("bash")
        self.assertIsNotNone(bash)
        script = SCRIPT.with_name("xtask-inventory.sh")
        commands = ("ci decoupling-gate deps hot-check gen-schema doc-check test-trace "
                    "journal-verify setup fixtures bench-gate scaffold-gate replay-verify "
                    "dst new-strategy gen-host fuzz path-digest").split()
        for milestone, generator, code in [("M2", False, 0), ("M2", True, 0),
                                            ("M3", False, 1), ("M3", True, 0)]:
            with self.subTest(milestone=milestone, generator=generator), \
                    tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                config = root / "tools/xtask/test-trace.toml"
                config.parent.mkdir(parents=True)
                config.write_text(f'current_milestone = "{milestone}"\n', encoding="utf-8")
                if generator:
                    (root / "deploy/gen").mkdir(parents=True)
                bins = root / "bin"
                bins.mkdir()
                cargo = bins / "cargo"
                # Fake help only: this regression test must never invoke a real Cargo build.
                cargo.write_text('#!/usr/bin/env bash\n'
                                 '[[ "$*" == "xtask help" ]] || exit 64\n'
                                 "cat <<'HELP'\nsubcommands:\n"
                                 + "".join(f"  {name}  mock\n" for name in commands)
                                 + "\nHELP\n", encoding="utf-8")
                cargo.chmod(0o755)
                output, summary = root / "outputs", root / "summary"
                env = {**os.environ, "PATH": str(bins) + os.pathsep + os.environ["PATH"],
                       "GITHUB_OUTPUT": str(output), "GITHUB_STEP_SUMMARY": str(summary)}
                result = subprocess.run([bash, str(script)], cwd=root, env=env,
                                        capture_output=True, text=True)
                self.assertEqual(result.returncode, code, result.stdout + result.stderr)
                outputs = dict(line.split("=", 1) for line in output.read_text().splitlines())
                self.assertEqual(outputs["gen-host"], str(generator).lower())
                self.assertEqual(outputs["scaffold-gate"], "true")
                if code:
                    self.assertIn("gen-host", result.stdout)
                    self.assertIn("missing", result.stdout)

    def check_benchmark_selection_includes_hot_strategies(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            cfg = root / "tools/xtask/deps.toml"
            cfg.parent.mkdir(parents=True)
            cfg.write_text('''[pkg."crates/core"]
tier = "hot"
[pkg."strategies/*"]
tier = "hot"
[pkg."strategies/registry"]
tier = "cold"
''', encoding="utf-8")
            for path, name in [("crates/core", "hft-core-lib"),
                               ("strategies/carry", "carry"),
                               ("strategies/registry", "hft-strategy-registry"),
                               ("strategies/_template", "template")]:
                manifest = root / path / "Cargo.toml"
                manifest.parent.mkdir(parents=True)
                manifest.write_text(f'[package]\nname = "{name}"\n', encoding="utf-8")
            previous = Path.cwd()
            os.chdir(root)
            try:
                self.assertEqual(paths.hot_crates([
                    "strategies/carry/benches/on_event.rs", "strategies/carry/src/lib.rs",
                    "strategies/registry/src/lib.rs", "strategies/_template/src/lib.rs",
                    "strategies/deleted/src/lib.rs", "crates/core/src/lib.rs",
                ]), ["carry", "hft-core-lib"])
                self.assertEqual(paths.hot_crates(["strategies/carry/README.md"]), [])
            finally:
                os.chdir(previous)

        for event, expected in [("workflow_dispatch", "true"), ("push", "false")]:
            stream = io.StringIO()
            with self.subTest(event=event), \
                    patch.dict(os.environ, {"EVENT": event}, clear=True), \
                    patch.object(paths, "changed_files", return_value=["tools/xtask/src/ci.rs"]), \
                    patch.object(paths, "fuzz_targets", return_value=[]), \
                    patch.object(paths, "hot_crates", return_value=[]), \
                    redirect_stdout(stream), redirect_stderr(io.StringIO()):
                self.assertEqual(paths.main(), 0)
            outputs = dict(line.split("=", 1) for line in stream.getvalue().splitlines())
            self.assertEqual(outputs["benchmarks"], expected)

    def test_tool_191_host_checks_follow_code_and_config_changes(self):
        self.check_gen_host_inventory_requires_generator()
        self.check_benchmark_selection_includes_hot_strategies()
        rows = [
            (["deploy/README.md", "crates/hft-oms/src/lib.rs"], False, False),
            (["apps/hft-core/README.md", ".github/workflows/ci.yml"], False, False),
            (["deploy/README.md", "apps/hft-core/TESTS.md"], False, True),
            (["deploy/hosts/ci-probe.toml"], True, False),
            (["deploy/gen/host.py", "deploy/README.md"], True, False),
            (["deploy/units/hft-core.service"], True, False),
            (["apps/hft-core/src/threads.rs"], True, False),
            (["apps/hft-core/Cargo.toml"], True, False),
        ]
        for files, gen_host, docs_only in rows:
            stream = io.StringIO()
            with self.subTest(files=files), \
                    patch.dict(os.environ, {"EVENT": "pull_request"}, clear=True), \
                    patch.object(paths, "changed_files", return_value=files), \
                    patch.object(paths, "fuzz_targets", return_value=[]), \
                    patch.object(paths, "hot_crates", return_value=[]), \
                    redirect_stdout(stream), redirect_stderr(io.StringIO()):
                self.assertEqual(paths.main(), 0)
            outputs = dict(line.split("=", 1) for line in stream.getvalue().splitlines())
            self.assertEqual(outputs["gen_host"], str(gen_host).lower())
            self.assertEqual(outputs["docs_only"], str(docs_only).lower())
            if "crates/hft-oms/src/lib.rs" in files:
                self.assertEqual(outputs["replay"], "true")
                self.assertEqual(outputs["scaffold"], "true")

        # A real rename out of deploy/ must keep the deleted source path. A synthetic list
        # containing both names would conceal git diff --name-only dropping the old name.
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            git_exe = shutil.which("git")
            self.assertIsNotNone(git_exe)

            def git(*args):
                return subprocess.run(
                    [git_exe, "-C", directory, "-c", "user.name=CI", "-c", "user.email=ci@example.invalid",
                     "-c", "commit.gpgsign=false", "-c", "diff.renames=true", *args],
                    check=True, capture_output=True, text=True,
                ).stdout

            git("init", "-q")
            source = root / "deploy/hosts/retired.toml"
            deleted = root / "apps/hft-core/src/removed.rs"
            source.parent.mkdir(parents=True)
            deleted.parent.mkdir(parents=True)
            source.write_text('[host]\nname = "retired"\n', encoding="utf-8")
            deleted.write_text("// retired thread plan\n", encoding="utf-8")
            git("add", ".")
            git("commit", "-qm", "base")
            base = git("rev-parse", "HEAD").strip()
            destination = root / "docs/retired-host.toml"
            destination.parent.mkdir()
            source.rename(destination)
            deleted.unlink()
            git("add", "-A")
            git("commit", "-qm", "move host input and remove thread plan")
            head = git("rev-parse", "HEAD").strip()
            for event in ["pull_request", "push"]:
                stream = io.StringIO()
                with self.subTest(event=event), patch.object(paths, "git", side_effect=git), \
                        patch.dict(os.environ, {"EVENT": event, "BASE": base, "HEAD": head}, clear=True), \
                        patch.object(paths, "fuzz_targets", return_value=[]), \
                        patch.object(paths, "hot_crates", return_value=[]), \
                        redirect_stdout(stream), redirect_stderr(io.StringIO()):
                    changed = paths.changed_files(event, base, head)
                    self.assertEqual(set(changed), {"deploy/hosts/retired.toml", "docs/retired-host.toml",
                                                    "apps/hft-core/src/removed.rs"})
                    self.assertEqual(paths.main(), 0)
                outputs = dict(line.split("=", 1) for line in stream.getvalue().splitlines())
                self.assertEqual(outputs["gen_host"], "true")
                self.assertEqual(outputs["docs_only"], "false")


if __name__ == "__main__":
    unittest.main()
