#!/usr/bin/env python3
"""Require every applicable quality-gates job; fast feedback is never acceptance."""
from __future__ import annotations

import json
import os
from pathlib import Path
import sys


JOBS = ("paths", "docs", "decoupling-gate", "related-tests", "rust", "arch-check", "benchmarks", "ops-py", "replay-dst")
CODE_JOBS = {"related-tests", "rust", "arch-check", "benchmarks", "ops-py", "replay-dst"}


def evaluate(needs: dict) -> tuple[bool, list[dict]]:
    paths = needs.get("paths", {})
    outputs = paths.get("outputs", {})
    docs_only = paths.get("result") == "success" and outputs.get("docs_only") == "true"
    bench_known = all(k in outputs for k in ("bench_crates", "benchmarks", "toolchain"))
    no_bench = (paths.get("result") == "success" and bench_known
                and not outputs["bench_crates"] and outputs["benchmarks"] == "false"
                and outputs["toolchain"] == "false")
    rows = []
    for job in JOBS:
        actual = needs.get(job, {}).get("result", "missing")
        may_skip = (docs_only and job in CODE_JOBS) or (job == "benchmarks" and no_bench)
        passed = actual == "success" or (actual == "skipped" and may_skip)
        rows.append({"job": job, "result": actual, "skip_allowed": may_skip, "passed": passed})
    return all(row["passed"] for row in rows), rows


def main() -> int:
    passed, rows = evaluate(json.loads(os.environ["NEEDS_JSON"]))
    sha = os.environ["HEAD"]
    report = {"sha": sha, "result": "pass" if passed else "fail", "jobs": rows}
    path = Path("target/xtask/acceptance.json")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    text = f"Full acceptance for `{sha}`: **{report['result']}**\n\n| Job | Result | Accepted |\n|---|---|---|\n"
    text += "".join(f"| {r['job']} | {r['result']} | {r['passed']} |\n" for r in rows)
    print(text)
    if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(summary, "a", encoding="utf-8") as stream:
            stream.write(text)
    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main())
