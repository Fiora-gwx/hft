#!/usr/bin/env bash
# SLC-100 (testnet, M1.5, [需授权]): the main run — mainnet public BBO of one venue, testnet/demo
# orders, c16 thread subset. Pass: exit 0; every hot thread on its configured core (threads-*.txt);
# report.json samples (HW, or SW when HW stamps are unavailable) n >= 1000 for kernel_rx, ingest,
# decide, egress, tick_to_trade, rep_to_order, order_rtt; event_weighted kernel_rx in both strata.
CASE=SLC-100 . "$(dirname "$0")/common.sh"
header
run_slice main --minutes "${MINUTES:-60}" --print-conn-stats
python3 - "$DIR/main/report.json" <<'PY' | tee -a "$DIR/log.txt"
import json, sys
r = json.load(open(sys.argv[1]))
need = ["kernel_rx", "ingest", "decide", "egress", "tick_to_trade", "rep_to_order", "order_rtt"]
n = {}
for e in r["samples"]["hist"]:
    k = e["key"]
    n[(k["stage"], k["ts_src"])] = n.get((k["stage"], k["ts_src"]), 0) + e["q"]["count"]
src = "Hw" if any(s == "Hw" for (_, s) in n) else "Sw"
bad = [s for s in need if n.get((s, src), 0) < 1000]
strata = {e[0]["stratum"] for d in r["event_weighted"] for e in d["hist"] if e[0]["stage"] == "kernel_rx"}
print("dataset", src, "counts", {s: n.get((s, src), 0) for s in need}, "strata", sorted(strata))
print("SLC-100:", "PASS" if not bad and len(strata) == 2 else f"FAIL {bad} strata={sorted(strata)}")
PY
