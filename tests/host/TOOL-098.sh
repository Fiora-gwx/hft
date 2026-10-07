#!/usr/bin/env bash
# TOOL-098 (tools/TESTS.md): coverage of the first AZ probe report. Read-only: reads the
# `az-probe run` reports collected from the probes (docs/runbooks/az-probe.md steps A–D), builds
# `az-report.md` with `az-probe compare` and checks the pass criteria. Opening and deleting the
# probe instances is the owner's `[需授权]` step and is not done here.
#
#   bash tests/host/TOOL-098.sh report-*.json
#
# Evidence: ${EVIDENCE_DIR:-ops-log}/<UTC date>-TOOL-098/{az-report.md,result.txt} (link it from
# ops-log/<date>-az-probe.md).
set -euo pipefail

(($#)) || { echo "usage: $0 report-*.json" >&2; exit 2; }
dir=${EVIDENCE_DIR:-ops-log}/$(date -u +%F)-TOOL-098
mkdir -p "$dir"
python3 -m az_probe compare "$@" >"$dir/az-report.md"

python3 - "$@" <<'EOF' | tee "$dir/result.txt"
import datetime as dt, json, sys
reports = [json.load(open(p)) for p in sys.argv[1:]]
ok = True
azs = sorted({r["az"] for r in reports})
print("AZs:", azs)
# Active session: the window overlaps 00:00-09:00 UTC (Asia) or 13:30-20:00 UTC (US) on a weekday.
def active(r):
    s = dt.datetime.strptime(r["started_utc"], "%Y-%m-%dT%H:%M:%SZ")
    e = s + dt.timedelta(seconds=r["duration_s"])
    t = s
    while t < e:
        m = t.hour * 60 + t.minute
        if t.weekday() < 5 and (m < 540 or 810 <= m < 1200):
            return True
        t += dt.timedelta(minutes=5)
    return False
cover = any(active(r) for r in reports)
print("covers an active UTC session:", cover)
ok &= cover
for ex in ("binance", "okx", "bitget"):
    seen = set()
    for r in reports:
        for row in r["rows"]:
            if r["endpoints"].get(row["endpoint"], {}).get("exchange") == ex and row["min_rtt_us"] is not None:
                seen.add(row["az"])
    print(f"{ex}: minRTT/p99 for AZs {sorted(seen)}")
    ok &= len(seen) >= 3
low = []
for r in reports:
    n = {}
    for row in r["rows"]:
        k = (row["az"], row["cpg"], row["endpoint"])
        n[k] = n.get(k, 0) + row["n"]
    low += [k for k, v in n.items() if v < 9]
print("(AZ, CPG, endpoint) with < 9 TCP samples:", low or "none")
ok &= not low
print("TOOL-098:", "PASS" if ok else "FAIL")
sys.exit(0 if ok else 1)
EOF
