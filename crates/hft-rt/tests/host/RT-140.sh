#!/usr/bin/env bash
# RT-140 (host, M1.5, [需授权]): calibrated TSC conversion drifts < 1 µs over 10 s on the target
# machine (crates/hft-rt/TESTS.md). Environment: the M1.5 rented c8g.4xlarge (then every reference
# machine and dev2); release test binary from the verification bundle ($HFT_RT_PROBE).
#   1. `TscClock::calibrate(&RtConfig::default().calib)`;
#   2. read (tsc_ordered(), CLOCK_MONOTONIC_RAW), sleep 10 s, read again.
# Pass: step 1 Ok (no NotInvariant/Unstable/Mismatch); |delta_ns(t0, t1) − (m1 − m0)| < 1 000 ns;
# hz_milli within 100 ppm of tsc_hz_nominal().
# Evidence: tests/host/RT-140.json (hz, both pairs, deviation, CNTFRQ_EL0), attached to the M1.5
# probe Agent Note.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
# The probe binary and its subcommands come with the M1.5 probe bundle (not built yet).
probe="${HFT_RT_PROBE:?set HFT_RT_PROBE to the verification-bundle hft-rt probe binary}"
"$probe" tsc-drift --seconds 10 --max-dev-ns 1000 --max-hz-ppm 100 --json "$here/RT-140.json"
