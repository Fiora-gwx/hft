#!/usr/bin/env bash
# TOOL-113 (tools/TESTS.md): hostprobe collect on the M1.5 reference machine / dev2 (later: the
# production AMI). Read-only: runs `hostprobe collect` as root and as an unprivileged user, checks
# the pass criteria and writes the evidence. Run as root on the host under test.
#
#   sudo bash tests/host/TOOL-113.sh [hostprobe-binary]
#
# Evidence: ${EVIDENCE_DIR:-ops-log}/<UTC date>-TOOL-113/{root.json,nonroot.json,result.txt}
set -euo pipefail

hp=${1:-hostprobe}
dir=${EVIDENCE_DIR:-ops-log}/$(date -u +%F)-TOOL-113
mkdir -p "$dir"
[[ $(id -u) == 0 ]] || { echo "run as root" >&2; exit 2; }
user=${NONROOT_USER:-nobody}

ms() { date +%s%N; }
t0=$(ms); "$hp" collect --out "$dir/root.json"; t1=$(ms)
tmp=$(mktemp -d); chmod 0777 "$tmp"
cp "$(command -v "$hp")" "$tmp/hostprobe"
su -s /bin/bash "$user" -c "$tmp/hostprobe collect --out $tmp/nonroot.json"
cp "$tmp/nonroot.json" "$dir/nonroot.json"; rm -rf "$tmp"
wall_ms=$(( (t1 - t0) / 1000000 ))

python3 - "$dir" "$wall_ms" <<'EOF' | tee "$dir/result.txt"
import json, sys
d, wall = sys.argv[1], int(sys.argv[2])
root = json.load(open(f"{d}/root.json"))
non = json.load(open(f"{d}/nonroot.json"))
cats = ["instance", "kernel", "cmdline", "cpu", "memory", "nic", "irq", "net", "kthreads", "resctrl",
        "clock", "occupancy", "guard", "imds"]
ok = True
missing = [c for c in cats if c not in root]
print("categories present:", "yes" if not missing else f"NO, missing {missing}")
ok &= not missing
print("root unreadable items:", root["unreadable"] or "none")
ok &= not root["unreadable"]
imds = [non["instance"][k] for k in ("type", "az_id", "ami_id")]
print("non-root IMDS items unreadable:", imds)
ok &= all(v == "unreadable" for v in imds)
print(f"collect wall clock: {wall} ms (limit 2000)")
ok &= wall <= 2000
print("resctrl:", root["resctrl"]["mounted"], "| host_profile_hash:", root["host_profile_hash"])
print("TOOL-113:", "PASS" if ok else "FAIL")
sys.exit(0 if ok else 1)
EOF
