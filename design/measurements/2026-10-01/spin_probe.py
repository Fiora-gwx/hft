"""How often is a busy-spinning thread interrupted on this host?

Simulates a hot thread of the new framework: one Python thread pinned to one CPU, spinning on the clock,
recording every gap > 2 us between consecutive clock reads (= the thread was not running). Runs at nice 19 so
production processes always win the CPU; this makes interruption *durations* pessimistic, while the interruption
*frequency* is representative of sharing cores with them. GC disabled; Python loop period is ~0.1 us.

usage: nice -n 19 python3 spin_probe.py <seconds> <cpu>
"""
import gc, json, os, sys, time

dur = float(sys.argv[1]) if len(sys.argv) > 1 else 20
cpu = int(sys.argv[2]) if len(sys.argv) > 2 else 1
os.sched_setaffinity(0, {cpu})
gc.disable()
pc = time.perf_counter_ns
edges = [2_000, 10_000, 50_000, 100_000, 1_000_000, 10_000_000]
counts = [0] * len(edges)
lost = 0
top = []
iters = 0
t0 = pc()
end = t0 + int(dur * 1e9)
last = t0
while True:
    t = pc()
    d = t - last
    last = t
    iters += 1
    if d > 2_000:
        lost += d
        for i, e in enumerate(edges):
            if d > e:
                counts[i] += 1
        if len(top) < 10 or d > top[-1]:
            top.append(d)
            top.sort(reverse=True)
            del top[10:]
    if t > end:
        break
el = last - t0
res = {
    "cpu": cpu, "seconds": round(el / 1e9, 2), "loop_ns": round(el / iters, 1),
    "gaps_per_s": {f">{e // 1000}us": round(c / (el / 1e9), 1) for e, c in zip(edges, counts)},
    "time_not_running_pct": round(100 * lost / el, 2),
    "top10_gaps_us": [round(x / 1000) for x in top],
    "loadavg": os.getloadavg(),
}
print(json.dumps(res))
