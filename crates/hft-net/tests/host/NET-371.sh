#!/usr/bin/env bash
# NET-371: on ENA, napi_id after the handshake is non-zero and maps (netdev netlink, kernel >= 6.9) to the
# trading interface ([需授权]). Pass: all 20 connections napi_id != 0 and napi_ifindex == ifindex(ENI-B);
# otherwise apply README 开发备忘 9 and retest.
CASE=NET-371 . "$(dirname "$0")/common.sh"
header
IF=${ENI_B:?set ENI_B to the trading interface}
need ip
log "## ip -j link"; ip -j link show "$IF" | tee -a "$OUT"
log "## 20 market-data connections: Ready.napi_id and Connector::napi_ifindex"
"${HFT_SLICE:?set HFT_SLICE}" --build-only 20 --print-napi | tee -a "$OUT"
