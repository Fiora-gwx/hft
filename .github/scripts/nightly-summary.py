#!/usr/bin/env python3
"""Nightly summary (docs/development.md#每夜任务nightlyyml): job verdicts and
the decoupling exemptions of the last 24 h. Writes the step summary and
prints the alert-group message on stdout.

Exemptions are found through the `decoupling-exemption` notice annotations that the
decoupling-gate jobs (quality-gates.yml, decoupling.yml) emit; annotations need `checks: read`.

Environment: NIGHTLY_RESULT, ARM_RESULT,
ARM_CACHE_HIT, RUN_URL, GITHUB_TOKEN, GITHUB_REPOSITORY.
"""

from __future__ import annotations

import datetime as dt
import os
import sys
from pathlib import Path

import github_common as gh


def exemptions(repo: str, since: dt.datetime) -> list[str]:
    found: list[str] = []
    created = since.strftime("%Y-%m-%dT%H:%M:%SZ")
    for run in gh.paged(
        f"/repos/{repo}/actions/runs", "workflow_runs", {"created": f">={created}"}, limit=500
    ):
        if Path(run.get("path", "")).name not in ("ci.yml", "decoupling.yml"):
            continue
        for job in gh.paged(f"/repos/{repo}/actions/runs/{run['id']}/jobs", "jobs"):
            name = job.get("name", "")
            if not (name == "decoupling-gate" or name.endswith(" / decoupling-gate")):
                continue
            for a in gh.api(f"/repos/{repo}/check-runs/{job['id']}/annotations", {"per_page": "100"}):
                if a.get("title") == "decoupling-exemption":
                    found.append(a.get("message", "").strip())
    return found


def main() -> int:
    e = os.environ.get
    repo = e("GITHUB_REPOSITORY", "")
    now = dt.datetime.now(dt.timezone.utc)
    lines = [f"hft nightly {now:%Y-%m-%d}: {e('RUN_URL', '')}"]
    lines.append(f"nightly (x86): {e('NIGHTLY_RESULT', '?')}")
    arm = e("ARM_RESULT", "?")
    if arm != "success" and e("ARM_CACHE_HIT") == "false":
        arm += " (rust-cache miss: infrastructure failure, re-run tonight)"
    lines.append(f"nightly-arm: {arm}")
    try:
        ex = exemptions(repo, now - dt.timedelta(days=1))
        lines.append("decoupling exemptions (24 h): " + ("; ".join(ex) if ex else "none"))
    except gh.ApiError as err:
        lines.append(f"decoupling exemptions (24 h): unavailable ({err})")
    msg = "\n".join(lines)
    path = e("GITHUB_STEP_SUMMARY")
    if path:
        with open(path, "a", encoding="utf-8") as f:
            f.write("### Nightly summary\n\n" + "\n".join(f"- {x}" for x in lines) + "\n")
    print(msg)
    return 0


if __name__ == "__main__":
    sys.exit(main())
