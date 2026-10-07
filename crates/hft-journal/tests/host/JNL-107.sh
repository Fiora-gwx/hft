#!/usr/bin/env bash
# JNL-107 (host, M1.5): the writer thread makes no munmap and no futex wake during the
# journal_append_80b benchmark, including rolls and retirement (crates/hft-journal/TESTS.md).
# Environment: the reference machine (c8g.4xlarge rented on demand) with bpftrace; the benchmark
# binary from the CI bench artifact ($HFT_JOURNAL_BENCH; never compiled here).
#   1. run journal_append_80b; bpftrace counts sys_enter_munmap and futex(FUTEX_WAKE) by the writer
#      thread's tid.
# Pass: both counts are 0. Evidence: ops-log/bench/JNL-107-<date>.txt.
set -euo pipefail
# The writer-tid hook of the bench binary comes with the M1.5 reference run (not built yet).
bench="${HFT_JOURNAL_BENCH:?set HFT_JOURNAL_BENCH to the hft-journal append bench binary}"
out="${OPS_LOG:-ops-log}/bench/JNL-107-$(date -u +%F).txt"
mkdir -p "$(dirname "$out")"
command -v bpftrace >/dev/null || { echo "JNL-107: bpftrace not installed" >&2; exit 2; }
"$bench" --bench journal_append_80b --writer-tid-file /tmp/jnl107.tid &
pid=$!
for _ in $(seq 100); do [ -s /tmp/jnl107.tid ] && break; sleep 0.1; done
tid="$(cat /tmp/jnl107.tid)"
# FUTEX_WAKE = 1 (op & 0x7f).
bpftrace -e "
tracepoint:syscalls:sys_enter_munmap /tid == $tid/ { @munmap = count(); }
tracepoint:syscalls:sys_enter_futex /tid == $tid && (args->op & 0x7f) == 1/ { @futex_wake = count(); }
END { printf(\"munmap=%d futex_wake=%d\n\", (int64)@munmap, (int64)@futex_wake); clear(@munmap); clear(@futex_wake); }
" -c "tail --pid=$pid -f /dev/null" | tee "$out"
wait "$pid"
grep -q 'munmap=0 futex_wake=0' "$out"
