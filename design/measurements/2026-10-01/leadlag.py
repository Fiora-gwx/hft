"""Cross-venue lead-lag from historical book_ticker (receive timestamps on our Tokyo host).

For each pair (leader L, follower F) and each L mid-price change at receive time t with sign s,
look for F's first mid change with the same sign in (t, t+W]. Report hit rate and the response delay
distribution, and the same in the reverse direction. Reads parquet already copied under ./bt/.
"""
import glob, json, sys
import numpy as np
import pandas as pd

W_MS = 100
SYMS = sys.argv[1].split(",") if len(sys.argv) > 1 else ["BTCUSDT", "ETHUSDT", "SOLUSDT"]
VENUES = ["binance", "okx", "bitget"]


def load(ex, sym):
    files = sorted(glob.glob(f"bt/{ex}/{sym}/**/*.parquet", recursive=True))
    if not files:
        return None
    df = pd.concat([pd.read_parquet(f, columns=["server_receive_utc_ns", "payload_json"]) for f in files])
    p = df["payload_json"].map(json.loads)
    bid = p.map(lambda x: x.get("bid_price_ticks")).astype("float64")
    ask = p.map(lambda x: x.get("ask_price_ticks")).astype("float64")
    t = df["server_receive_utc_ns"].to_numpy(dtype="int64")
    o = np.argsort(t, kind="stable")
    t, mid = t[o], ((bid + ask) / 2).to_numpy()[o]
    d = np.diff(mid)
    nz = d != 0
    return t[1:][nz], np.sign(d[nz]).astype(np.int8), len(t)


def follow(tl, sl, tf, sf, k=8):
    """For each leader event, delay (ms) to the follower's first same-sign change within W_MS."""
    idx = np.searchsorted(tf, tl, side="right")
    best = np.full(len(tl), np.nan)
    for j in range(k):
        ii = idx + j
        ok = ii < len(tf)
        ii2 = np.where(ok, ii, 0)
        dt = (tf[ii2] - tl) / 1e6
        hit = ok & (sf[ii2] == sl) & (dt <= W_MS) & np.isnan(best)
        best[hit] = dt[hit]
    return best


out = {}
for sym in SYMS:
    data = {ex: load(ex, sym) for ex in VENUES}
    out[sym] = {"changes": {ex: (len(v[0]) if v else 0) for ex, v in data.items()}}
    for a in VENUES:
        for b in VENUES:
            if a == b or not data[a] or not data[b]:
                continue
            r = follow(data[a][0], data[a][1], data[b][0], data[b][1])
            h = r[~np.isnan(r)]
            out[sym][f"{a}->{b}"] = {
                "hit_rate": round(len(h) / max(1, len(r)), 3),
                "delay_ms_p25": round(float(np.percentile(h, 25)), 2) if len(h) else None,
                "delay_ms_p50": round(float(np.percentile(h, 50)), 2) if len(h) else None,
                "delay_ms_p75": round(float(np.percentile(h, 75)), 2) if len(h) else None,
            }
print(json.dumps(out, indent=1))
json.dump(out, open("leadlag.json", "w"), indent=1)
