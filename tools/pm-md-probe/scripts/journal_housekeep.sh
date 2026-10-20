#!/usr/bin/env bash
# Journal housekeeping for hft-core acceptance runs (README "存储"), run hourly by a systemd timer.
#   ROOT/run-*/cfg/cold/<instance>/<journal>/<day>/*.jrn
# 1. Compresses closed cold segments with zstd (.jrn -> .jrn.zst): every segment of a stopped run;
#    in a running run (its core.lock is flocked by hft-core) all but the newest segment of each
#    journal, and only when untouched for MIN_AGE_MIN minutes.
# 2. Deletes stopped runs older than KEEP_DAYS.
# 3. While free space on ROOT is below MIN_FREE_PCT, deletes the oldest stopped run.
# A running run is never deleted. `zstd -d` restores a segment for pm-md-probe journal / replay.
set -euo pipefail
ROOT=${PMCORE_ROOT:-/data/pmcore}
KEEP_DAYS=${PMCORE_KEEP_DAYS:-7}
MIN_FREE_PCT=${PMCORE_MIN_FREE_PCT:-15}
MIN_AGE_MIN=${PMCORE_MIN_AGE_MIN:-30}

running() { # run dir -> 0 when an hft-core holds one of its instance locks
  local l
  for l in "$1"/cfg/cold/*/core.lock; do
    [ -e "$l" ] || continue
    flock -n "$l" true || return 0
  done
  return 1
}

shopt -s nullglob
for run in "$ROOT"/run-*/; do
  run=${run%/}
  if running "$run"; then
    for jdir in "$run"/cfg/cold/*/*/; do
      newest=$(find "$jdir" -name '*.jrn' -printf '%T@ %p\n' | sort -n | tail -1 | cut -d' ' -f2-)
      find "$jdir" -name '*.jrn' -mmin +"$MIN_AGE_MIN" ! -path "$newest" -print0 |
        xargs -0 -r nice -n 15 zstd -q -3 --rm
    done
  else
    find "$run"/cfg/cold -name '*.jrn' -print0 | xargs -0 -r nice -n 15 zstd -q -3 --rm
    if [ -n "$(find "$run" -maxdepth 0 -mtime +"$KEEP_DAYS")" ] &&
       [ -z "$(find "$run" -newermt "-$KEEP_DAYS days" -print -quit)" ]; then
      rm -rf -- "$run"; echo "removed $run (older than $KEEP_DAYS days)"
    fi
  fi
done

free_pct() { df --output=avail,size "$ROOT" | tail -1 | awk '{printf "%d", 100 * $1 / $2}'; }
while [ "$(free_pct)" -lt "$MIN_FREE_PCT" ]; do
  victim=""
  for run in $(ls -dtr "$ROOT"/run-*/ 2>/dev/null); do
    run=${run%/}
    running "$run" || { victim=$run; break; }
  done
  [ -n "$victim" ] || { echo "free space below ${MIN_FREE_PCT}% and only running runs left" >&2; exit 1; }
  rm -rf -- "$victim"; echo "removed $victim (free space below ${MIN_FREE_PCT}%)"
done
