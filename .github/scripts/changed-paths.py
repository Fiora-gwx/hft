#!/usr/bin/env python3
"""Classify the files changed by this push or PR for the path-triggered CI steps.

Rules: docs/testing.md §6 (PR row) and docs/development.md#质量门quality-gatesyml.
Diff: pull_request -> `git diff BASE...HEAD` (merge base); push -> the commit against its first
parent (a merge commit on main is judged against main's previous head). Needs full history.

Environment: EVENT (github.event_name), BASE (PR base SHA), HEAD (SHA under test).
Writes step outputs (true/false unless noted):
  docs_only     every changed file is *.md (PR only; pushes to main always run everything)
  scaffold      crates/ apps/ strategies/ tools/            -> scaffold-gate
  replay        strategies, hft-exec, hft-risk, hft-oms, hft-sequencer -> replay-verify samples
  fuzz_targets  space-separated fuzz targets whose sources changed (tools/xtask/fuzz-targets.toml)
  bench_crates  space-separated hot crates (tools/xtask/deps.toml tier = "hot") that changed
  benchmarks    benchmarks/
  toolchain     rust-toolchain.toml                         -> base vs head relative benchmark
  cargo_lock    Cargo.lock                                  -> cargo deny check advisories
  gen_host      deploy/ or apps/hft-core/                   -> gen-host --check, deploy container tests
  python        python/ strategies/*/pipeline/ crates/hft-py/ tools/az-probe/
  ops           deploy/ .github/
"""

from __future__ import annotations

import importlib.util
import os
import subprocess
import sys
import tomllib
from pathlib import Path

REPLAY_PREFIXES = (
    "strategies/",
    "crates/hft-exec/",
    "crates/hft-risk/",
    "crates/hft-oms/",
    "crates/hft-sequencer/",
)


def git(*args: str) -> str:
    return subprocess.run(["git", *args], check=True, capture_output=True, text=True).stdout


def changed_files(event: str, base: str, head: str) -> list[str]:
    if event == "pull_request" and base:
        out = git("diff", "--name-only", f"{base}...{head}")
    else:
        parents = git("rev-list", "--parents", "-n", "1", head).split()[1:]
        if parents:
            out = git("diff", "--name-only", parents[0], head)
        else:
            out = git("ls-tree", "-r", "--name-only", head)
    return [line for line in out.splitlines() if line]


def under(files: list[str], *prefixes: str) -> bool:
    return any(f.startswith(prefixes) for f in files)


def fuzz_targets(files: list[str]) -> list[str]:
    """Targets of tools/xtask/fuzz-targets.toml (M3) whose crate or listed `paths` changed."""
    spec = importlib.util.spec_from_file_location(
        "fuzz_targets", Path(__file__).resolve().parent / "fuzz-targets.py"
    )
    assert spec and spec.loader
    ft = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(ft)
    code = [f for f in files if not f.endswith(".md")]
    out = []
    for e in ft.entries():
        prefixes = tuple(p + "/" for p in [*e["paths"], e["dir"]] if p)
        if prefixes and any(f.startswith(prefixes) for f in code):
            out.append(e["name"])
    return out


def hot_crates(files: list[str]) -> list[str]:
    cfg = Path("tools/xtask/deps.toml")
    if not cfg.exists():
        return []
    with cfg.open("rb") as f:
        t = tomllib.load(f)
    out = []
    for path, pkg in t.get("pkg", {}).items():
        if pkg.get("tier") != "hot" or "*" in path:
            continue
        if any(f.startswith(path + "/") and not f.endswith(".md") for f in files):
            out.append(Path(path).name)
    return out


def main() -> int:
    event = os.environ.get("EVENT", "")
    base = os.environ.get("BASE", "")
    head = os.environ.get("HEAD", "") or "HEAD"
    files = changed_files(event, base, head)
    pipeline = any(f.startswith("strategies/") and "/pipeline/" in f for f in files)
    res = {
        "docs_only": event == "pull_request" and bool(files) and all(f.endswith(".md") for f in files),
        "scaffold": under(files, "crates/", "apps/", "strategies/", "tools/"),
        "replay": under(files, *REPLAY_PREFIXES),
        "fuzz_targets": " ".join(fuzz_targets(files)),
        "bench_crates": " ".join(hot_crates(files)),
        "benchmarks": under(files, "benchmarks/"),
        "toolchain": "rust-toolchain.toml" in files,
        "cargo_lock": "Cargo.lock" in files,
        "gen_host": under(files, "deploy/", "apps/hft-core/"),
        "python": under(files, "python/", "crates/hft-py/", "tools/az-probe/") or pipeline,
        "ops": under(files, "deploy/", ".github/"),
    }
    lines = [f"{k}={str(v).lower() if isinstance(v, bool) else v}" for k, v in res.items()]
    out = os.environ.get("GITHUB_OUTPUT")
    if out:
        with open(out, "a", encoding="utf-8") as f:
            f.write("\n".join(lines) + "\n")
    print(f"{len(files)} changed file(s)", file=sys.stderr)
    print("\n".join(lines))
    return 0


if __name__ == "__main__":
    sys.exit(main())
