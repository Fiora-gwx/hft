#!/usr/bin/env bash
# NET-370: HW RX timestamp availability and coverage on the M1.5 probe ([需授权]: the owner rents and
# deletes the instance). Pass: hw_ts_reads / ts_reads >= 0.999 when PHC/HW stamps exist; otherwise record
# "unavailable" (testing §8.2: use the SW data set). Also record multi_skb / frames.
CASE=NET-370 . "$(dirname "$0")/common.sh"
header
IF=${ENI_B:?set ENI_B to the trading interface}
need ethtool
log "## ethtool -T $IF"; ethtool -T "$IF" | tee -a "$OUT"
log "## vertical slice >= 1 h (ts_expect_hw = true), BBO stream of one venue"
"${HFT_SLICE:?set HFT_SLICE to the M1.5 slice binary}" --minutes "${MINUTES:-60}" --ts-expect-hw --print-conn-stats | tee -a "$OUT"
log "## evaluate: per BBO connection ts_reads, hw_ts_reads, multi_skb, frames, TS_MISSING count (above)"
