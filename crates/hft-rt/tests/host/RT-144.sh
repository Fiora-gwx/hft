#!/usr/bin/env bash
# RT-144 (host, M1.5, [需授权]): counter frequency, PHC reads and 1 h drift on the target machine
# (crates/hft-rt/TESTS.md). Environment: the M1.5 rented c8g.4xlarge with rt.phc.device available;
# probe binary from the verification bundle ($HFT_RT_PROBE).
#   1. record tsc_hz_nominal() (CNTFRQ_EL0) and `ls /dev/ptp*`;
#   2. every 1 s for 1 h: sample_pair(&clock, Some(&phc)) + DriftTracker::push.
# Pass: CNTFRQ_EL0 ≠ 0; no Stepped in 1 h; err_ns distribution (p50/p99/max) in the evidence;
# one PHC ioctl per second.
# Evidence: tests/host/RT-144.json; conclusions in the M1.5 probe Agent Note.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
# The probe binary and its subcommands come with the M1.5 probe bundle (not built yet).
probe="${HFT_RT_PROBE:?set HFT_RT_PROBE to the verification-bundle hft-rt probe binary}"
phc="${RT_PHC_DEVICE:-$(ls /dev/ptp* 2>/dev/null | head -n1)}"
[ -n "$phc" ] || { echo "RT-144: no /dev/ptp* device" >&2; exit 2; }
ls -l /dev/ptp* >&2
"$probe" phc-drift --phc "$phc" --seconds 3600 --interval-ms 1000 --json "$here/RT-144.json"
