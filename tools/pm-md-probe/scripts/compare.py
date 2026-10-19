#!/usr/bin/env python3
"""Compares pm-md-probe output with an independent pmrec recording of the same windows.

usage: compare.py --probe out.jsonl --bindings bindings.json --pmrec DIR
DIR is pmrec's raw/<YYYYMMDD> directory (clob/<asset>_<start_ts>.jsonl.gz, lines "<rx_ns>\t<json>").

Per token, grouped by exchange timestamp (ms) over the time range both cover: the exchange's
`price_change` best_bid/best_ask versus the probe's best prices after each `batch`. A token passes
when every timestamp seen by both sides carries equal values and at most 1 % (or 5) timestamps are
seen by one side only (reconnect gaps of either recorder).
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
        # The probe's book exists only after its first snapshot.
        first = next((x[0] for x in got if x[1] >= 0 or x[2] >= 0), lo)
        lo = max(lo, first + 1)
        by_r, by_g = {}, {}
        for x in ref:
            if lo <= x[0] <= hi:
                by_r.setdefault(x[0], []).append(x[1:])
        for x in got:
            if lo <= x[0] <= hi:
                by_g.setdefault(x[0], []).append(x[1:])
        common = sorted(set(by_r) & set(by_g))
        same = sum(1 for t in common if by_r[t] == by_g[t])
        diff = [t for t in common if by_r[t] != by_g[t]]
        only_r = sorted(set(by_r) - set(by_g))
        only_g = sorted(set(by_g) - set(by_r))
        n_r = sum(len(v) for v in by_r.values())
        n_g = sum(len(v) for v in by_g.values())
        ok = not diff and len(only_r) + len(only_g) <= max(5, (len(common) + 99) // 100)
        ok_all &= ok
        def spans(ts):
            out, s0, prev = [], None, None
            for t in ts:
                if s0 is None:
                    s0 = prev = t
                elif t - prev > 3000:
                    out.append((s0, prev)); s0 = t
                prev = t
            if s0 is not None:
                out.append((s0, prev))
            return out[:4]
        print(
            f"{b['slot']}: {'OK ' if ok else 'DIFF'} timestamps equal {same}/{len(common)} "
            f"(entries pmrec {n_r}, probe {n_g}); value mismatch {len(diff)}; only pmrec {len(only_r)} {spans(only_r)}; "
            f"only probe {len(only_g)} {spans(only_g)}; books pmrec {ref_books} probe {n_snap}; "
            f"trades pmrec {ref_trades} probe {n_trade}"
        )
        if diff:
            t = diff[0]
            print(f"   first mismatch at {t}: pmrec {by_r[t]} probe {by_g[t]}")
    print("RESULT", "PASS" if ok_all else "FAIL")
    sys.exit(0 if ok_all else 1)


if __name__ == "__main__":
    main()
