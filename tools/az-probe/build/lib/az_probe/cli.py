"""Command line (tools/README.md "az-probe"):

    az-probe run --targets tools/az-probe/targets.toml --out report-<az>-<ts>.json [--duration 60m]
                 [--ws-first-frame] [--interval 10m] [--az <az-id>] [--cpg <name>]
                 [--ip-ranges <ip-ranges.json>] [--cloudflare <ips-v4>]
    az-probe compare report-*.json > az-report.md

Exit codes: 0 ok, 2 input error.
"""

from __future__ import annotations

import argparse
import json
import sys
from typing import Any

from . import attribution, compare, probe


def _read(path: str | None) -> str | None:
    if path is None:
        return None
    with open(path, encoding="utf-8") as f:
        return f.read()


def main(argv: list[str] | None = None, env: probe.Env | None = None) -> int:
    ap = argparse.ArgumentParser(prog="az-probe")
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("run", help="probe every target endpoint from this instance")
    r.add_argument("--targets", required=True)
    r.add_argument("--out", required=True)
    r.add_argument("--duration", default="60m")
    r.add_argument("--interval", default="10m")
    r.add_argument("--ws-first-frame", action="store_true")
    r.add_argument("--az")
    r.add_argument("--cpg")
    r.add_argument("--ip-ranges")
    r.add_argument("--cloudflare")
    c = sub.add_parser("compare", help="rank (AZ, CPG) from run reports")
    c.add_argument("reports", nargs="+")
    a = ap.parse_args(argv)
    if a.cmd == "compare":
        reports: list[dict[str, Any]] = []
        for p in a.reports:
            try:
                reports.append(json.loads(_read(p) or "{}"))
            except (OSError, ValueError) as exc:
                print(f"az-probe compare: {p}: {exc}", file=sys.stderr)
                return 2
        sys.stdout.write(compare.markdown(reports))
        return 0
    e: probe.Env = env if env is not None else probe.RealEnv()
    try:
        targets = probe.load_targets(_read(a.targets) or "")
        aws = _read(a.ip_ranges) if a.ip_ranges else e.fetch(attribution.AWS_RANGES_URL)
        cf = _read(a.cloudflare) if a.cloudflare else e.fetch(attribution.CLOUDFLARE_URL)
    except (OSError, ValueError, KeyError) as err:
        print(f"az-probe run: {err}", file=sys.stderr)
        return 2
    ranges = attribution.Ranges.parse(aws, cf)
    report = probe.run(
        targets,
        e,
        probe.parse_duration(a.duration),
        ws_first_frame=a.ws_first_frame,
        interval_s=probe.parse_duration(a.interval),
        az=a.az,
        cpg=a.cpg,
        ranges=ranges,
    )
    try:
        with open(a.out, "w", encoding="utf-8") as f:
            json.dump(report, f, indent=2, sort_keys=True)
            f.write("\n")
    except OSError as err:
        print(f"az-probe run: {err}", file=sys.stderr)
        return 2
    print(
        f"az-probe: {len(report['rows'])} (endpoint, IP) rows, {len(report['first_frame'])} first-frame rows "
        f"-> {a.out}",
        file=sys.stderr,
    )
    return 0
