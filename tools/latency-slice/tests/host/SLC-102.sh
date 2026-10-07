#!/usr/bin/env bash
# SLC-102 (host, M1.5, [需授权]): IRQ core idle versus kept awake. KEEPAWAKE_CMD starts an allowed
# keep-awake form registered in the hostprobe rules (tools/README.md "hostprobe"); SCHED_IDLE spin
# threads are not allowed. Both runs 30 min with ballast. Pass: two reports; kernel_rx,
# nic_to_kernel and tick_to_trade compared in the conclusion.
CASE=SLC-102 . "$(dirname "$0")/common.sh"
header
: "${KEEPAWAKE_CMD:?set KEEPAWAKE_CMD to the allowed keep-awake form of the IRQ core}"
run_slice irq-idle --minutes "${MINUTES:-30}" --ballast "${BALLAST_CONNS:-4}"
bash -c "$KEEPAWAKE_CMD" &
ka=$!
trap 'kill "$ka" 2>/dev/null || true' EXIT
run_slice irq-awake --minutes "${MINUTES:-30}" --ballast "${BALLAST_CONNS:-4}"
log "## compare kernel_rx / nic_to_kernel / tick_to_trade between irq-idle and irq-awake"
