#!/usr/bin/env bash
# NET-373: TCP recvmsg reports the stamp of the last skb of a read (README 开发备忘 1). Loopback + netem
# delays packets 1 ms apart; the receiver reads both in one recvmsg; 1 000 samples. Pass: >= 99% of
# sw_rx_ns within ±50 µs of the second packet's arrival. (dev2: no authorization; probe: [需授权].)
CASE=NET-373 . "$(dirname "$0")/common.sh"
header
need tc
trap 'sudo tc qdisc del dev lo root 2>/dev/null || true' EXIT
sudo tc qdisc add dev lo root netem delay 1ms | tee -a "$OUT" || true
"${HFT_SLICE:?set HFT_SLICE}" --two-packet-stamp-probe 1000 | tee -a "$OUT"
