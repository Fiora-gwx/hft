#!/usr/bin/env bash
# Shared helpers of the latency-slice host cases (tools/latency-slice/TESTS.md SLC-100–SLC-102). They
# run on the M1.5 latency probe (rented c8g.4xlarge, [需授权]), never in CI. HFT_SLICE = the
# latency-slice binary of the development build artefact (never compiled on the probe);
# HFT_SLICE_CONFIG = its configuration (keys only in environment variables or a 0600 file);
# evidence goes to $EVIDENCE_DIR (default: current directory)/<CASE>-<host>-<date>/.
set -euo pipefail
CASE=${CASE:?}
SLICE=${HFT_SLICE:?set HFT_SLICE to the latency-slice binary}
: "${HFT_SLICE_CONFIG:?set HFT_SLICE_CONFIG to the slice configuration}"
EVIDENCE_DIR=${EVIDENCE_DIR:-.}
DIR="$EVIDENCE_DIR/$CASE-$(hostname -s)-$(date -u +%Y%m%d)"
mkdir -p "$DIR"
log() { printf '%s\n' "$*" | tee -a "$DIR/log.txt"; }
header() {
  log "# $CASE $(date -u +%FT%TZ) host=$(hostname) kernel=$(uname -r)"
  log "# instance: $(cat /sys/devices/virtual/dmi/id/product_name 2>/dev/null || echo unknown)"
  if command -v hostprobe >/dev/null; then hostprobe collect --out "$DIR/hostprofile.json" || true; fi
}
# Thread → CPU placement of a running slice (pid $1), once.
threads() { ps -L -o tid,psr,comm -p "$1" | tee "$DIR/threads-$2.txt" >/dev/null; }
# One slice run into $DIR/<tag>/ with extra arguments; records the thread placement after 60 s.
run_slice() {
  local tag=$1; shift
  local out="$DIR/$tag"
  "$SLICE" --out "$out" "$@" >"$DIR/$tag.stdout" 2>"$DIR/$tag.stderr" &
  local pid=$!
  sleep 60 && threads "$pid" "$tag" || true
  local rc=0
  wait "$pid" || rc=$?
  log "## $tag exit=$rc report=$out/report.json"
  return $rc
}
