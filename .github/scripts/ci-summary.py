#!/usr/bin/env python3
"""Acceptance of one quality-gates run: every job applicable to its lane must pass.

Lanes (docs/development.md#质量门quality-gatesyml, tools/TESTS.md TOOL-207):
- fast (pull requests): paths, docs, decoupling-gate, fast-test, fast-clippy, fast-dst, ops-py;
- full (pushes to codex/m2-integration and main, nightly): paths, docs, decoupling-gate, rust,
  arch-check, benchmarks, ops-py, replay-dst, heavy-tests.
The other lane's jobs are not evaluated (they are skipped by design). Allowed skips: code jobs of a
docs-only PR; fast-test/fast-clippy when no Rust package is affected; fast-dst when hft-testkit is
not affected; benchmarks when no benchmark input changed; decoupling-gate outside push and
pull_request events (nightly: there is no diff to judge).

Environment: NEEDS_JSON (toJSON(needs)), HEAD, LANE (fast|full), EVENT (github.event_name).
"""
from __future__ import annotations

import json
import os
from pathlib import Path
import sys


COMMON = ("paths", "docs", "decoupling-gate", "ops-py")
FAST = ("fast-test", "fast-clippy", "fast-dst")
FULL = ("rust", "arch-check", "benchmarks", "replay-dst", "heavy-tests")
LANES = {"fast": COMMON + FAST, "full": COMMON + FULL}
JOBS = COMMON + FAST + FULL
CODE_JOBS = {"ops-py", *FAST, *FULL}


def evaluate(needs: dict, lane: str = "full", event: str = "push") -> tuple[bool, list[dict]]:
    if lane not in LANES:
        return False, [{"job": "lane", "result": lane, "skip_allowed": False, "passed": False}]
    paths = needs.get("paths", {})
    ok_paths = paths.get("result") == "success"
    outputs = paths.get("outputs", {})
    docs_only = ok_paths and lane == "fast" and outputs.get("docs_only") == "true"
    # toJSON(needs) omits empty-string outputs: a missing `bench_crates` is the empty list, while
    # the true/false classifications must be present.
    bench_known = all(k in outputs for k in ("benchmarks", "toolchain"))
    no_bench = (ok_paths and bench_known and not outputs.get("bench_crates")
                and outputs["benchmarks"] == "false" and outputs["toolchain"] == "false")
    no_rust = ok_paths and outputs.get("has_tests") == "false"
    no_dst = ok_paths and outputs.get("dst") == "false"
    rows = []
    for job in LANES[lane]:
        actual = needs.get(job, {}).get("result", "missing")
        may_skip = ((docs_only and job in CODE_JOBS)
                    or (job == "benchmarks" and no_bench)
                    or (job in ("fast-test", "fast-clippy") and no_rust)
                    or (job == "fast-dst" and (no_rust or no_dst))
                    or (job == "decoupling-gate" and event not in ("push", "pull_request")))
        passed = actual == "success" or (actual == "skipped" and may_skip)
        rows.append({"job": job, "result": actual, "skip_allowed": may_skip, "passed": passed})
    return all(row["passed"] for row in rows), rows


def main() -> int:
    lane = os.environ.get("LANE", "full")
    passed, rows = evaluate(json.loads(os.environ["NEEDS_JSON"]), lane, os.environ.get("EVENT", "push"))
    sha = os.environ["HEAD"]
    report = {"sha": sha, "lane": lane, "result": "pass" if passed else "fail", "jobs": rows}
    path = Path("target/xtask/acceptance.json")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    text = (f"Acceptance ({lane} lane) for `{sha}`: **{report['result']}**\n\n"
            "| Job | Result | Accepted |\n|---|---|---|\n")
    text += "".join(f"| {r['job']} | {r['result']} | {r['passed']} |\n" for r in rows)
    print(text)
    if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(summary, "a", encoding="utf-8") as stream:
            stream.write(text)
    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main())
