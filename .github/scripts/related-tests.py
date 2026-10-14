#!/usr/bin/env python3
"""PR fast lane: select the affected workspace packages and run their tests with cargo-nextest.

Selection (docs/development.md#质量门quality-gatesyml, tools/TESTS.md TOOL-206):
- the diff is the complete PR (`BASE...HEAD`, renames as delete/add pairs; changed-paths.py);
- a changed non-Markdown file under a member directory selects that member;
- root Cargo.toml / Cargo.lock / rust-toolchain.toml / .cargo/, root tests/ and benchmarks/ select
  every member; `fixtures/<dir>/` selects the members whose sources or manifests name
  `fixtures/<dir>` (every member when none does: never guess one consumer); `fixtures/dst-seeds/`
  also selects hft-testkit (the DST regression seeds);
- .github/ selects tools/xtask (its CI policy tests);
- then every workspace member that depends on a selected one (normal, dev, build or target
  dependencies, transitively) is added: the reverse-dependency closure.

`plan` writes target/xtask/related-tests.json and step outputs `has_tests`, `packages` (space
separated) and `dst` (hft-testkit is affected: the DST smoke runs). `run` executes the plan's
commands (env `SHARD` = `k/n`: one nextest partition of the same selection). Proc-level test
binaries (`proc*`), trybuild compile-fail binaries, `heavy::` tests, the RPL-012 batch and doctests
are left to the full lane (`cargo xtask ci`, nightly and integration-branch pushes).
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
DEP_TABLES = ("dependencies", "dev-dependencies", "build-dependencies")
# nextest filterset of the fast lane: no proc binaries, no trybuild compile-fail binaries (nested
# cargo builds), no heavy:: tests, no RPL-012 batch (the full lane's replay-dst job runs it).
FILTER = "not (binary(/^proc/) | binary(compile_fail) | test(/heavy::/) | test(rpl_012_dst_journals_verify))"
SOURCE_SUFFIXES = (".rs", ".toml", ".py")


def changed_files(event: str, base: str, head: str) -> list[str]:
    spec = importlib.util.spec_from_file_location("ci_paths", Path(__file__).with_name("changed-paths.py"))
    assert spec and spec.loader
    paths = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(paths)
    return paths.changed_files(event, base, head)


def _manifests(root: Path) -> dict[str, tuple[str, dict]]:
    """directory -> (package name, parsed manifest) for every workspace member."""
    config = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]
    excluded = config.get("exclude", [])
    result = {}
    for pattern in config["members"]:
        for manifest in sorted(root.glob(f"{pattern}/Cargo.toml")):
            if any(manifest.parent.match(p) for p in excluded):
                continue
            directory = manifest.parent.relative_to(root).as_posix()
            data = tomllib.loads(manifest.read_text(encoding="utf-8"))
            name = data["package"]["name"]
            if not re.fullmatch(r"[A-Za-z0-9_-]+", name):
                raise ValueError(f"invalid package name in {manifest}")
            result[directory] = (name, data)
    if not result:
        raise ValueError("workspace has no packages")
    return result


def workspace(root: Path) -> dict[str, str]:
    return {d: name for d, (name, _) in _manifests(root).items()}


def _dep_names(manifest: dict, ws_deps: dict) -> set[str]:
    tables = [manifest.get(t, {}) for t in DEP_TABLES]
    for target in manifest.get("target", {}).values():
        tables.extend(target.get(t, {}) for t in DEP_TABLES)
    out = set()
    for table in tables:
        for key, value in table.items():
            name = key
            if isinstance(value, dict):
                if value.get("workspace") is True:
                    inherited = ws_deps.get(key, {})
                    if isinstance(inherited, dict):
                        name = inherited.get("package", key)
                name = value.get("package", name)
            out.add(name)
    return out


def dependencies(root: Path) -> dict[str, set[str]]:
    """package -> workspace packages it depends on (any dependency kind)."""
    ws_deps = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["workspace"].get("dependencies", {})
    manifests = _manifests(root)
    names = {name for name, _ in manifests.values()}
    return {name: (_dep_names(data, ws_deps) & names) - {name} for name, data in manifests.values()}


def reverse_closure(selected: set[str], deps: dict[str, set[str]]) -> set[str]:
    out = set(selected)
    changed = True
    while changed:
        changed = False
        for package, uses in deps.items():
            if package not in out and uses & out:
                out.add(package)
                changed = True
    return out


def fixture_consumers(root: Path, directory: str, members: dict[str, str]) -> set[str]:
    needle = f"fixtures/{directory}"
    found = set()
    for member_dir, name in members.items():
        for path in (root / member_dir).rglob("*"):
            if "target" in path.parts or not path.is_file() or path.suffix not in SOURCE_SUFFIXES:
                continue
            try:
                if needle in path.read_text(encoding="utf-8", errors="ignore"):
                    found.add(name)
                    break
            except OSError:
                continue
    return found


def select(files: list[str], members: dict[str, str], deps: dict[str, set[str]] | None = None,
           root: Path | None = None) -> tuple[list[str], str]:
    code = [f for f in files if not f.endswith(".md")]
    everything = sorted(members.values())
    if any(f in {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml"} or f.startswith(".cargo/") for f in code):
        return everything, "workspace configuration"
    if any(f.startswith(("tests/", "benchmarks/")) for f in code):
        return everything, "shared test input"
    selected = {name for directory, name in members.items() if any(f.startswith(directory + "/") for f in code)}
    fixture_dirs = sorted({f.split("/")[1] for f in code if f.startswith("fixtures/") and f.count("/") >= 2})
    for directory in fixture_dirs:
        users = fixture_consumers(root or Path.cwd(), directory, members)
        if not users:
            return everything, f"shared test input fixtures/{directory}"
        selected |= users
    if any(f.startswith("fixtures/dst-seeds/") for f in code):
        # The DST regression seeds feed `cargo xtask dst` (hft-testkit's runner).
        selected.update(name for name in members.values() if name == "hft-testkit")
    if any(f.startswith(".github/") for f in code):
        selected.update(name for directory, name in members.items() if directory == "tools/xtask")
    if not selected:
        return [], "no Rust package changes"
    closure = reverse_closure(selected, deps or {})
    reason = "changed packages" + (" and their reverse dependencies" if closure != selected else "")
    return sorted(closure), reason


def commands(packages: list[str], shard: str = "") -> list[list[str]]:
    """One nextest command; `shard` = `k/n` runs the k-th of n test partitions (same build)."""
    if not packages:
        return []
    selection = [arg for package in packages for arg in ("-p", package)]
    partition = ["--partition", f"count:{shard}"] if shard else []
    return [[
        "cargo", "nextest", "run", "--locked", "--cargo-profile", "ci", *selection, "--all-features",
        "--no-fail-fast", "-E", FILTER, *partition,
    ]]


def save(plan: dict) -> None:
    PLAN.parent.mkdir(parents=True, exist_ok=True)
    PLAN.write_text(json.dumps(plan, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def make_plan() -> dict:
    head = os.environ["HEAD"]
    files = changed_files(os.environ.get("EVENT", ""), os.environ.get("BASE", ""), head)
    root = Path.cwd()
    packages, reason = select(files, workspace(root), dependencies(root), root)
    return {"sha": head, "files": files, "packages": packages, "reason": reason,
            "commands": commands(packages, os.environ.get("SHARD", "")), "results": [], "complete": False}


def main() -> int:
    if sys.argv[1:] == ["plan"]:
        plan = make_plan()
        head, packages, reason = plan["sha"], plan["packages"], plan["reason"]
        save(plan)
        output = os.environ.get("GITHUB_OUTPUT")
        if output:
            with open(output, "a", encoding="utf-8") as stream:
                stream.write(f"has_tests={str(bool(packages)).lower()}\n")
                stream.write(f"packages={' '.join(packages)}\n")
                stream.write(f"dst={str('hft-testkit' in packages).lower()}\n")
        text = f"Fast-lane packages for `{head}`: {reason}.\n\n" + ", ".join(packages or ["none"])
        print(text)
        if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
            with open(summary, "a", encoding="utf-8") as stream:
                stream.write(text + "\n\nThe full lane (integration-branch pushes, nightly) tests the whole workspace.\n")
        return 0
    if sys.argv[1:] == ["run"]:
        # The cargo cache restore between `plan` and `run` may replace target/ (a plan selecting
        # every member, e.g. after a root Cargo.toml change, lost its file): plan again, the
        # selection is a pure function of the commit range.
        plan = json.loads(PLAN.read_text(encoding="utf-8")) if PLAN.exists() else make_plan()
        if plan["sha"] != os.environ["HEAD"] or plan["commands"] != commands(plan["packages"], os.environ.get("SHARD", "")):
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
