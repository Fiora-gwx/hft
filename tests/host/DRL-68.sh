#!/usr/bin/env bash
# DRL-68 (docs/tests/drills.md): read-only check of an M1.5 probe / reference-machine rental
# (docs/runbooks/host-tuning.md section C, no production archive). Records the six facts and the
# HostProfile; changes nothing (XPS writability is read from the file mode, not by writing).
#
#   sudo bash tests/host/DRL-68.sh <eni-b-ifname> [hostprobe-binary]
#
# Evidence: ${EVIDENCE_DIR:-ops-log}/<UTC date>-DRL-68/ (the conclusion goes into the probe's Agent Note).
set -euo pipefail

ifn=${1:?usage: $0 <eni-b-ifname> [hostprobe]}
hp=${2:-hostprobe}
dir=${EVIDENCE_DIR:-ops-log}/$(date -u +%F)-DRL-68
mkdir -p "$dir"
{
  echo "## ENA queues";            ethtool -l "$ifn" 2>&1 || true
  echo "## IRQ layout";            grep -E 'ena|nvme|Tx-Rx' /proc/interrupts || true
  echo "## IRQ affinity";          for d in /proc/irq/[0-9]*; do
                                     printf '%s %s eff=%s\n' "${d##*/}" "$(cat "$d/smp_affinity_list")" "$(cat "$d/effective_affinity_list" 2>/dev/null)"
                                   done
  echo "## managed IRQ (nvme)";    grep nvme /proc/interrupts | awk '{print $1}' | tr -d : | while read -r n; do
                                     echo "$n eff=$(cat /proc/irq/"$n"/effective_affinity_list)"; done
  echo "## NAPI parameters";       for f in napi_defer_hard_irqs gro_flush_timeout threaded; do
                                     echo "$f=$(cat /sys/class/net/"$ifn"/$f 2>/dev/null)"; done
  echo "## XPS writable (mode)";   ls -l /sys/class/net/"$ifn"/queues/tx-*/xps_cpus
  echo "## HW RX timestamps";      ethtool -T "$ifn" 2>&1 || true
  echo "## production archive";    if [[ -e /opt/hft/current-core ]]; then echo "PRESENT (fail)"; else echo "absent"; fi
} | tee "$dir/facts.txt"
"$hp" collect --out "$dir/hostprofile.json"
if [[ -e /opt/hft/current-core ]]; then echo "DRL-68: FAIL (production archive installed)"; exit 1; fi
echo "DRL-68: facts recorded in $dir; write the conclusion into the probe's Agent Note"
