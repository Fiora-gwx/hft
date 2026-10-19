#!/usr/bin/env python3
"""Bindings for pm-md-probe: the Up/Down token ids of the next 5-minute windows (Gamma API).

usage: discover.py --out bindings.json [--windows 2] [--assets btc,eth,sol,xrp] [--first next|current]
Writes [{"slot": "<asset>-<start_ts>-up|dn", "token": "<id>", "start_ts": n, "asset": a}, ...] and
prints the first window start (epoch seconds).
"""
import argparse
import json
import time
import urllib.request

GAMMA = "https://gamma-api.polymarket.com/events?slug={a}-updown-5m-{ts}"


def tokens(asset: str, ts: int) -> list[str]:
    req = urllib.request.Request(GAMMA.format(a=asset, ts=ts), headers={"User-Agent": "pm-md-probe/1"})
    with urllib.request.urlopen(req, timeout=10) as r:
        ev = json.load(r)
    if not ev:
        raise SystemExit(f"no market {asset} {ts}")
    m = ev[0]["markets"][0]
    out = json.loads(m["clobTokenIds"])
    outcomes = json.loads(m.get("outcomes", "[]"))
    if len(out) != 2 or [o.lower() for o in outcomes] != ["up", "down"]:
        raise SystemExit(f"unexpected outcomes {outcomes} for {asset} {ts}")
    return out


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--windows", type=int, default=2)
    ap.add_argument("--assets", default="btc,eth,sol,xrp")
    ap.add_argument("--first", choices=["next", "current"], default="next")
    a = ap.parse_args()
    base = int(time.time()) // 300 * 300 + (300 if a.first == "next" else 0)
    rows = []
    for w in range(a.windows):
        ts = base + 300 * w
        for asset in a.assets.split(","):
            up, dn = tokens(asset, ts)
            rows += [
                {"slot": f"{asset}-{ts}-up", "token": up, "start_ts": ts, "asset": asset},
                {"slot": f"{asset}-{ts}-dn", "token": dn, "start_ts": ts, "asset": asset},
            ]
    with open(a.out, "w", encoding="utf-8") as f:
        json.dump(rows, f, indent=1)
    print(base)


if __name__ == "__main__":
    main()
