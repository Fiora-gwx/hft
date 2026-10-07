#!/usr/bin/env bash
# NET-374: with data queued, a non-blocking TCP recvmsg fills the given space (the "short read means
# empty" rule, README 开发备忘 11). Loopback writer runs 10 min; bpftrace compares the return value, the
# requested length and sk_rmem_alloc at tcp_recvmsg return. Pass: no sample with ret < len and
# sk_rmem_alloc > 0 afterwards; otherwise switch to "read again" and update NET-080.
CASE=NET-374 . "$(dirname "$0")/common.sh"
header
need bpftrace
sudo timeout 620 bpftrace -e '
kprobe:tcp_recvmsg { @len[tid] = arg2; @sk[tid] = arg0; }
kretprobe:tcp_recvmsg /@len[tid]/ {
  $sk = (struct sock *)@sk[tid];
  if (retval > 0 && retval < @len[tid] && $sk->sk_backlog.rmem_alloc.counter > 0) { @short_with_data = count(); }
  @reads = count(); delete(@len[tid]); delete(@sk[tid]);
}' >>"$OUT" 2>&1 &
bp=$!
"${HFT_SLICE:?set HFT_SLICE}" --loopback-stream-minutes 10 | tee -a "$OUT"
wait "$bp" || true
