#!/usr/bin/env bash
# NET-372: with SO_BUSY_POLL = 0 an empty non-blocking recvmsg never enters napi_busy_loop, even when
# net.core.busy_read is 50 ([需授权]). Pass: napi_busy_loop count 0; setsockopt(SO_BUSY_POLL, 0) succeeds
# unprivileged. Restores busy_read = 0 on exit.
CASE=NET-372 . "$(dirname "$0")/common.sh"
header
need bpftrace
old=$(sysctl -n net.core.busy_read)
trap 'sudo sysctl -w net.core.busy_read="$old" >/dev/null' EXIT
sudo sysctl -w net.core.busy_read=50 | tee -a "$OUT"
sudo bpftrace -e 'kprobe:napi_busy_loop { @calls = count(); } interval:s:30 { exit(); }' >>"$OUT" 2>&1 &
bp=$!
sleep 2
log "## 1e6 poll_rx on an idle connection + net_poll_empty"
"${HFT_SLICE:?set HFT_SLICE}" --idle-polls 1000000 | tee -a "$OUT"
cargo bench -p hft-net --bench rx -- net_poll_empty 2>&1 | tail -5 | tee -a "$OUT" || true
wait "$bp" || true
