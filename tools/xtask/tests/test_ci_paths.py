"""CI path classification regression (tools/TESTS.md TOOL-191)."""
from contextlib import redirect_stderr, redirect_stdout
import importlib.util
import io
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).resolve().parents[3] / ".github" / "scripts" / "changed-paths.py"
SPEC = importlib.util.spec_from_file_location("changed_paths", SCRIPT)
paths = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(paths)


class PathPolicyTests(unittest.TestCase):
    def test_tool_191_host_checks_follow_code_and_config_changes(self):
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

            def git(*args):
                return subprocess.run(
                    ["git", "-C", directory, "-c", "user.name=CI", "-c", "user.email=ci@example.invalid",
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
