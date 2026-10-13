#!/usr/bin/env python3
"""Plan and run early package tests; the full CI gate always keeps its own scope.

PR synchronize events use the previous head only when it is an ancestor of the
tested head. New/reopened PRs and force pushes use the complete PR diff instead.
The plan is an artifact, including the actual diff basis, packages and commands.
"""
from __future__ import annotations

import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tomllib


PLAN = Path("target/xtask/related-tests.json")


def changed_files(event: str, action: str, before: str, base: str, head: str) -> tuple[list[str], str]:
    if event == "pull_request" and action == "synchronize" and re.fullmatch(r"[0-9a-fA-F]{40}", before):
        ancestor = subprocess.run(
            ["git", "merge-base", "--is-ancestor", before, head], capture_output=True,
        )
        if ancestor.returncode == 0:
            result = subprocess.run(
                ["git", "diff", "--no-renames", "--name-only", before, head],
                check=True, capture_output=True, text=True,
            )
            return result.stdout.splitlines(), before
    spec = importlib.util.spec_from_file_location("ci_paths", Path(__file__).with_name("changed-paths.py"))
    assert spec and spec.loader
    paths = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(paths)
    return paths.changed_files(event, base, head), "full-pr" if event == "pull_request" else "first-parent"


def workspace(root: Path) -> dict[str, str]:
    config = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]
    excluded = config.get("exclude", [])
    result = {}
    for pattern in config["members"]:
        for manifest in root.glob(f"{pattern}/Cargo.toml"):
            directory = manifest.parent.relative_to(root).as_posix()
            if any(manifest.parent.match(p) for p in excluded):
                continue
            name = tomllib.loads(manifest.read_text(encoding="utf-8"))["package"]["name"]
            if not re.fullmatch(r"[A-Za-z0-9_-]+", name):
                raise ValueError(f"invalid package name in {manifest}")
            result[directory] = name
    if not result:
        raise ValueError("workspace has no packages")
    return result


def select(files: list[str], members: dict[str, str]) -> tuple[list[str], str]:
    code = [f for f in files if not f.endswith(".md")]
    if any(f in {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml"} or f.startswith(".cargo/") for f in code):
        return sorted(members.values()), "workspace configuration"
    selected = {name for directory, name in members.items() if any(f.startswith(directory + "/") for f in code)}
    # Shared fixtures and root integration tests can belong to several crates. Test all
    # members in early feedback too; never silently guess one consumer from a file name.
    if any(f.startswith(("fixtures/", "tests/", "benchmarks/")) for f in code):
        return sorted(members.values()), "shared test input"
    if any(f.startswith(".github/") for f in code):
        selected.update(name for directory, name in members.items() if directory == "tools/xtask")
    return sorted(selected), "changed packages" if selected else "no Rust package changes"


def commands(packages: list[str]) -> list[list[str]]:
    if not packages:
        return []
    selection = [arg for package in packages for arg in ("-p", package)]
    common = ["--locked", "--profile", "ci", *selection, "--all-features"]
    # Proc tests launch examples built alongside the selected test assemblies.
    return [
        ["cargo", "build", *common, "--tests", "--examples", "--keep-going"],
        # `heavy::` tests run in the heavy-tests job, like `cargo xtask ci` (docs/development.md).
        ["cargo", "test", *common, "--no-fail-fast", "--", "--skip", "heavy::"],
    ]


def save(plan: dict) -> None:
    PLAN.parent.mkdir(parents=True, exist_ok=True)
    PLAN.write_text(json.dumps(plan, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def main() -> int:
    if sys.argv[1:] == ["plan"]:
        head = os.environ["HEAD"]
        files, basis = changed_files(
            os.environ.get("EVENT", ""), os.environ.get("ACTION", ""),
            os.environ.get("BEFORE", ""), os.environ.get("BASE", ""), head,
        )
        packages, reason = select(files, workspace(Path.cwd()))
        plan = {"sha": head, "basis": basis, "files": files, "packages": packages,
                "reason": reason, "commands": commands(packages), "results": [], "complete": False}
        save(plan)
        output = os.environ.get("GITHUB_OUTPUT")
        if output:
            with open(output, "a", encoding="utf-8") as stream:
                stream.write(f"has_tests={str(bool(packages)).lower()}\n")
        text = f"Early package tests for `{head}`\n\nBasis `{basis}`; {reason}.\n\n" + ", ".join(packages or ["none"])
        print(text)
        if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
            with open(summary, "a", encoding="utf-8") as stream:
                stream.write(text + "\n\nFull acceptance runs independently.\n")
        return 0
    if sys.argv[1:] == ["run"]:
        plan = json.loads(PLAN.read_text(encoding="utf-8"))
        if plan["sha"] != os.environ["HEAD"] or plan["commands"] != commands(plan["packages"]):
            raise ValueError("test plan does not match this commit or package commands")
        for command in plan["commands"]:
            print("::group::" + " ".join(command), flush=True)
            result = subprocess.run(command).returncode
            print("::endgroup::", flush=True)
            plan["results"].append({"command": command, "exit_code": result})
            save(plan)
            if result:
                return 1
        plan["complete"] = True
        save(plan)
        return 0
    raise ValueError("usage: related-tests.py plan|run")


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (KeyError, ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f"::error::related tests: {error}", file=sys.stderr)
        sys.exit(2)
