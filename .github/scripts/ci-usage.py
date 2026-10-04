#!/usr/bin/env python3
"""CI minute self-estimate, quota gate and Admin check (docs/development.md#免费额度).

GitHub's run-usage and per-product billing APIs are closed, so the usage of the current UTC month is
estimated from the jobs of every workflow run of this repository: each job counts
ceil(completed_at - started_at) whole minutes times the multiplier of its runner
(.github/runner-multipliers.toml). Shared by nightly `quota-gate`, quality-gates `decoupling-gate`
and the release `preflight`.

Subcommands:
  usage                         print the estimate (minutes) of the current UTC month
  gate --threshold N [--override true|false] [--actor LOGIN] [--non-admin-override fail|ignore]
                                write `minutes`, `over`, `ran`, `override` to $GITHUB_OUTPUT and a
                                summary to $GITHUB_STEP_SUMMARY
  is-admin --actor LOGIN        exit 0 if LOGIN is in .github/hft-admins.txt, else 1

Exit codes: 0 ok; 1 refused (non-Admin override with --non-admin-override fail, or not an Admin);
2 environment or API error.
Environment: GITHUB_TOKEN or GH_TOKEN (actions: read), GITHUB_REPOSITORY, GITHUB_API_URL (optional).
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import math
import os
import sys
import time
import tomllib
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
ADMINS = ROOT / ".github" / "hft-admins.txt"
MULTIPLIERS = ROOT / ".github" / "runner-multipliers.toml"


class ApiError(RuntimeError):
    pass


def _env(name: str) -> str:
    v = os.environ.get(name, "")
    if not v:
        raise ApiError(f"{name} is not set")
    return v


def api(path: str, params: dict[str, str] | None = None) -> dict:
    """GET one GitHub REST resource (JSON), with a few retries on transient errors."""
    token = os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN")
    if not token:
        raise ApiError("GITHUB_TOKEN/GH_TOKEN is not set")
    base = os.environ.get("GITHUB_API_URL", "https://api.github.com")
    url = f"{base}{path}"
    if params:
        url += "?" + urllib.parse.urlencode(params)
    req = urllib.request.Request(
        url,
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
        },
    )
    last: Exception | None = None
    for attempt in range(4):
        try:
            with urllib.request.urlopen(req, timeout=30) as r:
                return json.load(r)
        except urllib.error.HTTPError as e:
            last = e
            if e.code < 500 and e.code != 429:
                raise ApiError(f"GET {path}: HTTP {e.code} {e.reason}") from e
        except (urllib.error.URLError, TimeoutError) as e:
            last = e
        time.sleep(2**attempt)
    raise ApiError(f"GET {path}: {last}")


def paged(path: str, key: str, params: dict[str, str] | None = None, limit: int = 5000):
    """Iterate the items under `key` of a paginated list endpoint."""
    page = 1
    seen = 0
    while True:
        p = dict(params or {})
        p.update({"per_page": "100", "page": str(page)})
        data = api(path, p)
        items = data.get(key, [])
        for it in items:
            yield it
            seen += 1
            if seen >= limit:
                return
        if len(items) < 100:
            return
        page += 1


def parse_ts(s: str | None) -> dt.datetime | None:
    if not s:
        return None
    return dt.datetime.fromisoformat(s.replace("Z", "+00:00"))


def month_start(now: dt.datetime) -> dt.datetime:
    return now.replace(day=1, hour=0, minute=0, second=0, microsecond=0)


def load_multipliers() -> tuple[float, dict[str, float]]:
    with MULTIPLIERS.open("rb") as f:
        t = tomllib.load(f)
    return float(t.get("default", 1)), {k: float(v) for k, v in t.get("labels", {}).items()}


def job_minutes(job: dict, now: dt.datetime, since: dt.datetime) -> int:
    start = parse_ts(job.get("started_at"))
    if start is None or start < since:
        return 0
    end = parse_ts(job.get("completed_at")) or now
    secs = max(0.0, (end - start).total_seconds())
    return math.ceil(secs / 60)


def estimate(now: dt.datetime | None = None) -> tuple[int, dict[str, int]]:
    """Billed minutes of the current UTC month and a per-workflow breakdown."""
    repo = _env("GITHUB_REPOSITORY")
    now = now or dt.datetime.now(dt.timezone.utc)
    since = month_start(now)
    default, table = load_multipliers()
    # A run created just before midnight of the 1st can have jobs in this month: look one day back
    # and count jobs by their own start time.
    created = (since - dt.timedelta(days=1)).date().isoformat()
    total = 0.0
    per_wf: dict[str, int] = {}
    for run in paged(f"/repos/{repo}/actions/runs", "workflow_runs", {"created": f">={created}"}):
        wf = Path(run.get("path", "") or run.get("name", "?")).name or "?"
        run_total = 0.0
        for job in paged(
            f"/repos/{repo}/actions/runs/{run['id']}/jobs", "jobs", {"filter": "all"}
        ):
            mins = job_minutes(job, now, since)
            if mins == 0:
                continue
            mult = default
            for label in job.get("labels") or []:
                if label in table:
                    mult = table[label]
                    break
            run_total += mins * mult
        total += run_total
        per_wf[wf] = per_wf.get(wf, 0) + math.ceil(run_total)
    return math.ceil(total), per_wf


def admins() -> set[str]:
    out: set[str] = set()
    if not ADMINS.exists():
        return out
    for line in ADMINS.read_text(encoding="utf-8").splitlines():
        login = line.split("#", 1)[0].strip()
        if login:
            out.add(login.lower())
    return out


def is_admin(actor: str) -> bool:
    return bool(actor) and actor.lower() in admins()


def write_output(**kv: object) -> None:
    path = os.environ.get("GITHUB_OUTPUT")
    lines = [f"{k}={str(v).lower() if isinstance(v, bool) else v}" for k, v in kv.items()]
    if path:
        with open(path, "a", encoding="utf-8") as f:
            f.write("\n".join(lines) + "\n")
    print("\n".join(lines))


def write_summary(text: str) -> None:
    path = os.environ.get("GITHUB_STEP_SUMMARY")
    if path:
        with open(path, "a", encoding="utf-8") as f:
            f.write(text + "\n")
    print(text, file=sys.stderr)


def cmd_usage(_: argparse.Namespace) -> int:
    minutes, per_wf = estimate()
    print(minutes)
    for wf, m in sorted(per_wf.items(), key=lambda x: -x[1]):
        print(f"  {wf}: {m}", file=sys.stderr)
    return 0


def cmd_gate(a: argparse.Namespace) -> int:
    override_req = a.override == "true"
    admin = is_admin(a.actor)
    if override_req and not admin:
        msg = f"quota_override requested by `{a.actor}`, who is not in .github/hft-admins.txt"
        if a.non_admin_override == "fail":
            write_summary(f"### Quota gate\n\n:x: {msg}; refused.")
            print(f"::error::{msg}")
            return 1
        print(f"::warning::{msg}; ignored")
        override_req = False
    minutes, per_wf = estimate()
    over = minutes >= a.threshold
    override_used = over and override_req
    ran = (not over) or override_used
    write_output(minutes=minutes, over=over, ran=ran, override=override_req)
    rows = "\n".join(f"| `{wf}` | {m} |" for wf, m in sorted(per_wf.items(), key=lambda x: -x[1]))
    state = "QuotaSkip" if not ran else ("Admin override" if override_used else "ok")
    text = (
        f"### Quota gate: {state}\n\n"
        f"Estimated CI usage this UTC month: **{minutes} min** (threshold {a.threshold} min).\n"
    )
    if override_used:
        text += f"\nquota_override by Admin `{a.actor}`.\n"
    if not ran:
        text += "\n`QuotaSkip`: usage is at or over the threshold (docs/development.md#免费额度).\n"
    text += f"\n| workflow | min |\n|---|---|\n{rows}\n"
    write_summary(text)
    return 0


def cmd_is_admin(a: argparse.Namespace) -> int:
    return 0 if is_admin(a.actor) else 1


def main(argv: list[str]) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    sub.add_parser("usage").set_defaults(fn=cmd_usage)
    g = sub.add_parser("gate")
    g.add_argument("--threshold", type=int, required=True)
    g.add_argument("--override", choices=["true", "false", ""], default="false")
    g.add_argument("--actor", default=os.environ.get("GITHUB_ACTOR", ""))
    g.add_argument("--non-admin-override", choices=["fail", "ignore"], default="fail")
    g.set_defaults(fn=cmd_gate)
    i = sub.add_parser("is-admin")
    i.add_argument("--actor", required=True)
    i.set_defaults(fn=cmd_is_admin)
    a = p.parse_args(argv)
    try:
        return a.fn(a)
    except ApiError as e:
        print(f"::error::ci-usage: {e}")
        return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
