#!/usr/bin/env bash
# DRL-69 (docs/tests/drills.md): first AZ probe — checks the collected `az-probe run` reports
# against the drill's coverage criteria (three AZs, >= 3 CPGs each, every probe >= 1 h) and builds
# the comparison. Read-only; the decisions (winner, second AZ, watchdog AZ), the AZ-name -> AZ-ID
# mapping and the probe clean-up are recorded by hand in ops-log/<date>-az-probe.md
# (docs/runbooks/az-probe.md steps 8-12).
#
#   bash tests/host/DRL-69.sh report-*.json
#
# Evidence: ${EVIDENCE_DIR:-ops-log}/<UTC date>-DRL-69/{az-report.md,result.txt}
set -euo pipefail

(($#)) || { echo "usage: $0 report-*.json" >&2; exit 2; }
dir=${EVIDENCE_DIR:-ops-log}/$(date -u +%F)-DRL-69
mkdir -p "$dir"
python3 -m az_probe compare "$@" >"$dir/az-report.md"
python3 - "$@" <<'EOF' | tee "$dir/result.txt"
import json, sys
reports = [json.load(open(p)) for p in sys.argv[1:]]
ok = True
cpgs = {}
for r in reports:
    cpgs.setdefault(r["az"], set()).add(r["cpg"])
    if r["duration_s"] < 3600:
        print(f"probe {r['az']}/{r['cpg']} ran {r['duration_s']} s (< 1 h)")
        ok = False
for az, c in sorted(cpgs.items()):
    print(f"{az}: {len(c)} CPG(s) {sorted(c)}")
ok &= len(cpgs) >= 3 and all(len(c) >= 3 for c in cpgs.values())
print("DRL-69 coverage:", "PASS" if ok else "FAIL", "- decisions and clean-up: ops-log/<date>-az-probe.md")
sys.exit(0 if ok else 1)
EOF
