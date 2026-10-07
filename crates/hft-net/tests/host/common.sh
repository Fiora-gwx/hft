#!/usr/bin/env bash
# Shared helpers of the hft-net host cases (crates/hft-net/TESTS.md NET-370–NET-374). These run on the
# M1.5 latency probe (rented c8g instance) or dev2, never in CI. The vertical-slice binary that opens
# connections and prints `ConnStats` (apps/hft-core M1.5 slice) is given by HFT_SLICE; evidence goes to
# $EVIDENCE_DIR (default: current directory) as <CASE>-<host>-<date>.txt.
set -euo pipefail
CASE=${CASE:?}
EVIDENCE_DIR=${EVIDENCE_DIR:-.}
OUT="$EVIDENCE_DIR/$CASE-$(hostname -s)-$(date -u +%Y%m%d).txt"
log() { printf '%s\n' "$*" | tee -a "$OUT"; }
need() { command -v "$1" >/dev/null || { log "missing tool: $1"; exit 2; }; }
header() {
  : >"$OUT"
  log "# $CASE $(date -u +%FT%TZ) host=$(hostname) kernel=$(uname -r)"
  log "# instance: $(cat /sys/devices/virtual/dmi/id/product_name 2>/dev/null || echo unknown)"
}
