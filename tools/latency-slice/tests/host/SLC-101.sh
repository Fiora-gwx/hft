#!/usr/bin/env bash
# SLC-101 (host, M1.5, [需授权]): market-data segments with and without ballast depth connections
# (only read, never decoded). Two rounds of (no ballast, ballast) runs of 30 min each; the
# configuration's ballast.symbols lists the ballast streams (e.g. the top 200 USDT pairs by volume).
# Pass: four reports; event_weighted kernel_rx (lower and upper bound) and nic_to_kernel p50/p99 per
# stratum side by side in the conclusion.
CASE=SLC-101 . "$(dirname "$0")/common.sh"
header
for round in 1 2; do
  run_slice "r$round-noballast" --minutes "${MINUTES:-30}" --ballast 0
  run_slice "r$round-ballast" --minutes "${MINUTES:-30}" --ballast "${BALLAST_CONNS:-4}"
done
log "## compare event_weighted kernel_rx / kernel_rx_hi / nic_to_kernel per stratum in $DIR/*/report.json"
