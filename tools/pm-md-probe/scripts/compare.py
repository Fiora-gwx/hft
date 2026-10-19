#!/usr/bin/env python3
"""Compares pm-md-probe output with an independent pmrec recording of the same windows.

usage: compare.py --probe out.jsonl --bindings bindings.json --pmrec DIR
DIR is pmrec's raw/<YYYYMMDD> directory (clob/<asset>_<start_ts>.jsonl.gz, lines "<rx_ns>\t<json>").

Per token: the exchange's `price_change` entries (timestamp, best_bid, best_ask) in arrival order
versus the probe's `batch` lines (ex_ms, bb, ba) of the slot, over the time range both cover, plus
`book` and trade counts. A token passes when both sequences are equal in that range.
"""
import argparse
import gzip
import json
import os
import sys


def px(s):
    return round(float(s) * 1000) if s not in (None, "") else -1


def pmrec_rows(path, token):
    changes, books, trades = [], 0, 0
    with gzip.open(path, "rt", encoding="utf-8", errors="replace") as f:
        for line in f:
            try:
                _, raw = line.rstrip("\n").split("\t", 1)
                d = json.loads(raw)
            except ValueError:
                continue  # the open tail of a live file
            for m in d if isinstance(d, list) else [d]:
                et = m.get("event_type")
                if et == "price_change":
                    for c in m.get("price_changes", []):
                        if c.get("asset_id") == token:
                            changes.append((int(m["timestamp"]), px(c.get("best_bid")), px(c.get("best_ask"))))
                elif et == "book" and m.get("asset_id") == token:
                    books += 1
                elif et == "last_trade_price" and m.get("asset_id") == token:
                    trades += 1
    return changes, books, trades


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--probe", required=True)
    ap.add_argument("--bindings", required=True)
    ap.add_argument("--pmrec", required=True)
    a = ap.parse_args()
    binds = json.load(open(a.bindings, encoding="utf-8"))
    probe = {}
    with open(a.probe, encoding="utf-8") as f:
        for line in f:
            d = json.loads(line)
            if d.get("k") in ("batch", "snap", "trade"):
                probe.setdefault(d["slot"], []).append(d)
    ok_all = True
    for b in binds:
        path = os.path.join(a.pmrec, "clob", f"{b['asset']}_{b['start_ts']}.jsonl.gz")
        if not os.path.exists(path):
            print(f"{b['slot']}: no pmrec file {path}")
            ok_all = False
            continue
        ref, ref_books, ref_trades = pmrec_rows(path, b["token"])
        mine = probe.get(b["slot"], [])
        got = [(d["ex_ms"], d["bb"], d["ba"]) for d in mine if d["k"] == "batch"]
        n_snap = sum(1 for d in mine if d["k"] == "snap")
        n_trade = sum(1 for d in mine if d["k"] == "trade")
        if not ref or not got:
            print(f"{b['slot']}: empty (pmrec {len(ref)}, probe {len(got)})")
            ok_all = False
            continue
        lo = max(ref[0][0], got[0][0])
        hi = min(ref[-1][0], got[-1][0])
        r = [x for x in ref if lo <= x[0] <= hi]
        g = [x for x in got if lo <= x[0] <= hi]
        # The probe's book exists only after its first snapshot: compare from its first non-empty book.
        first = next((i for i, x in enumerate(g) if x[1] >= 0 or x[2] >= 0), 0)
        start_ms = g[first][0] if g else lo
        r = [x for x in r if x[0] > start_ms]
        g = [x for x in g if x[0] > start_ms]
        same = r == g
        mism = sum(1 for x, y in zip(r, g) if x != y)
        ok_all &= same
        print(
            f"{b['slot']}: {'OK ' if same else 'DIFF'} compared {len(r)} vs {len(g)} level changes "
            f"(mismatch {mism}); books pmrec {ref_books} probe {n_snap}; trades pmrec {ref_trades} probe {n_trade}; "
            f"range {lo}..{hi}"
        )
    print("RESULT", "PASS" if ok_all else "FAIL")
    sys.exit(0 if ok_all else 1)


if __name__ == "__main__":
    main()
