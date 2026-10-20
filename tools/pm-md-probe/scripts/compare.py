#!/usr/bin/env python3
"""Compares pm-md-probe output with an independent pmrec recording of the same windows.

usage: compare.py --probe out.jsonl --bindings bindings.json --pmrec DIR
DIR is pmrec's raw/<YYYYMMDD> directory (clob/<asset>_<start_ts>.jsonl.gz, lines "<rx_ns>\\t<json>").

Reference: the book rebuilt here, independently of the probe's code, from pmrec's raw messages
(`book` replaces, each `price_change` entry sets one level). Per token and exchange timestamp (ms),
the best bid/ask after the last message of that timestamp is compared with the probe's best
prices after its last event of that timestamp, over the time range both cover.
- One-sided timestamps come from reconnect gaps of either recorder (Polymarket closes a connection
  whose send buffer fills, WS 1013); after a probe reconnect its values are skipped until the
  token's next snapshot (the engine treats the book as stale until then).
- The exchange's own `best_bid`/`best_ask` fields are reported for information only: they are
  occasionally computed mid-match and disagree with the level messages themselves.
A token passes when no common timestamp differs and at most 1 % (or 5) timestamps are one-sided.
"""
import argparse
import gzip
import json
import os
import sys


def px(s):
    return round(float(s) * 1000)


def best(side, top):
    live = [p for p, q in side.items() if q > 0]
    if not live:
        return -1
    return max(live) if top else min(live)


def pmrec_rows(path, token):
    """Per message touching `token`: (ts, rebuilt bb, rebuilt ba, exchange bb, exchange ba)."""
    bids, asks = {}, {}
    have = False
    rows, books, trades = [], 0, 0
    with gzip.open(path, "rt", encoding="utf-8", errors="replace") as f:
        for line in f:
            parts = line.rstrip("\n").split("\t", 1)
            if len(parts) != 2:
                continue
            try:
                d = json.loads(parts[1])
            except ValueError:
                continue  # the open tail of a live file
            for m in d if isinstance(d, list) else [d]:
                et = m.get("event_type")
                if et == "book" and m.get("asset_id") == token:
                    bids = {px(x["price"]): float(x["size"]) for x in m.get("bids", [])}
                    asks = {px(x["price"]): float(x["size"]) for x in m.get("asks", [])}
                    have = True
                    books += 1
                    rows.append((int(m["timestamp"]), best(bids, True), best(asks, False), None, None))
                elif et == "price_change":
                    mine = [c for c in m.get("price_changes", []) if c.get("asset_id") == token]
                    if not mine:
                        continue
                    for c in mine:
                        side = bids if c["side"] == "BUY" else asks
                        side[px(c["price"])] = float(c["size"])
                    if have:
                        last = mine[-1]
                        ebb = px(last["best_bid"]) if last.get("best_bid") not in (None, "", "0") else -1
                        eba = px(last["best_ask"]) if last.get("best_ask") not in (None, "", "1") else -1
                        rows.append((int(m["timestamp"]), best(bids, True), best(asks, False), ebb, eba))
                elif et == "last_trade_price" and m.get("asset_id") == token:
                    trades += 1
    return rows, books, trades


def spans(ts):
    out, s0, prev = [], None, None
    for t in ts:
        if s0 is None:
            s0 = prev = t
        elif t - prev > 3000:
            out.append((s0, prev))
            s0 = t
        prev = t
    if s0 is not None:
        out.append((s0, prev))
    return out[:4]


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--probe", required=True)
    ap.add_argument("--bindings", required=True)
    ap.add_argument("--pmrec", required=True)
    a = ap.parse_args()
    binds = json.load(open(a.bindings, encoding="utf-8"))
    probe, resync_at = {}, []
    with open(a.probe, encoding="utf-8") as f:
        for line in f:
            d = json.loads(line)
            if d.get("k") in ("batch", "snap", "trade"):
                probe.setdefault(d["slot"], []).append(d)
            elif d.get("k") == "conn" and "reason: 15" in d.get("state", ""):
                resync_at.append(len(resync_at))
    ok_all = True
    total_common = total_diff = 0
    for b in binds:
        path = os.path.join(a.pmrec, "clob", f"{b['asset']}_{b['start_ts']}.jsonl.gz")
        if not os.path.exists(path):
            print(f"{b['slot']}: no pmrec file {path}")
            ok_all = False
            continue
        ref, ref_books, ref_trades = pmrec_rows(path, b["token"])
        mine = probe.get(b["slot"], [])
        # Probe values: final per timestamp; after a gap (> 150 ms without events while the
        # reference has some) values are stale until the next snapshot, so they are dropped.
        got, stale, prev_t = {}, False, None
        ref_ts = sorted({r[0] for r in ref})
        for d in mine:
            if d["k"] == "trade":
                continue
            t = d["ex_ms"]
            if prev_t is not None and t - prev_t > 150:
                import bisect
                i = bisect.bisect_right(ref_ts, prev_t)
                if i < len(ref_ts) and ref_ts[i] < t:
                    stale = True
            if d["k"] == "snap":
                stale = False
            prev_t = t
            if not stale:
                got[t] = (d["bb"], d["ba"])
        n_snap = sum(1 for d in mine if d["k"] == "snap")
        n_trade = sum(1 for d in mine if d["k"] == "trade")
        rb, ex = {}, {}
        for t, bb, ba, ebb, eba in ref:
            rb[t] = (bb, ba)
            if ebb is not None:
                ex[t] = (ebb, eba)
        if not rb or not got:
            print(f"{b['slot']}: empty (pmrec {len(rb)}, probe {len(got)})")
            ok_all = False
            continue
        first = min(t for t, v in got.items() if v != (-1, -1)) if any(v != (-1, -1) for v in got.values()) else 0
        lo = max(min(rb), min(got), first + 1)
        hi = min(max(rb), max(got))
        r = {t: v for t, v in rb.items() if lo <= t <= hi}
        g = {t: v for t, v in got.items() if lo <= t <= hi}
        common = sorted(set(r) & set(g))
        diff = [t for t in common if r[t] != g[t]]
        only_r = sorted(set(r) - set(g))
        only_g = sorted(set(g) - set(r))
        ex_diff = sum(1 for t in common if t in ex and ex[t] != g[t])
        ok = not diff and len(only_r) + len(only_g) <= max(5, (len(common) + 99) // 100)
        ok_all &= ok
        total_common += len(common)
        total_diff += len(diff)
        print(
            f"{b['slot']}: {'OK ' if ok else 'DIFF'} equal {len(common) - len(diff)}/{len(common)} timestamps; "
            f"only pmrec {len(only_r)} {spans(only_r)}; only probe {len(only_g)} {spans(only_g)}; "
            f"books pmrec {ref_books} probe {n_snap}; trades pmrec {ref_trades} probe {n_trade}; "
            f"(exchange best fields differ at {ex_diff})"
        )
        if diff:
            t = diff[0]
            print(f"   first mismatch at {t}: rebuilt {r[t]} probe {g[t]}")
    print(f"RESULT {'PASS' if ok_all else 'FAIL'}: {total_common - total_diff}/{total_common} timestamps equal")
    sys.exit(0 if ok_all else 1)


if __name__ == "__main__":
    main()
