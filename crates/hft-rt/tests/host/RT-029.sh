#!/usr/bin/env bash
# RT-029 (host, M1.5, [需授权]): shared-memory SPSC ring on hugetlbfs (crates/hft-rt/TESTS.md).
# Environment: the M1.5 rented c8g.4xlarge, deploy tuning applied, /dev/hugepages2M mounted.
# Runs the verification-bundle probe binary ($HFT_RT_PROBE, built by CI; never compiled here):
#   1. `ShmSpsc::create(…, PageSize::Huge2M, …)` under rt.shm.dir (temp name → rename);
#   2. a second process attaches and transfers 10 000 elements in order.
# Pass: step 1 Ok (hugetlbfs supports rename), step 2 complete and ordered. If rename returns
# EINVAL, change the implementation per the README 开发备忘 fallback and record it in the case.
# Evidence: tests/host/RT-029.log (command output and kernel version), archived with the M1.5
# probe Agent Note.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
log="$here/RT-029.log"
# The probe binary and its subcommands come with the M1.5 probe bundle (not built yet).
probe="${HFT_RT_PROBE:?set HFT_RT_PROBE to the verification-bundle hft-rt probe binary}"
dir="${RT_SHM_DIR:-/dev/hugepages2M/hft-rt-029}"
mountpoint -q /dev/hugepages2M || { echo "RT-029: /dev/hugepages2M is not mounted" >&2; exit 2; }
mkdir -p "$dir"
{
  echo "# RT-029 $(date -u +%FT%TZ)"
  uname -a
  "$probe" shm-huge2m --dir "$dir" --elements 10000
} 2>&1 | tee "$log"
