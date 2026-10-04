"""Read-only public market-data probe: one-way latency (local receive wall clock - exchange timestamp)
and message rates for the fastest public BBO channel of each venue. Public streams only, no keys.

Clock: chrony to Amazon Time Sync (error bound ~0.35 ms on this host), so results are good to ~0.5 ms.
Python/asyncio receive overhead is included (typically 50-300 us), so treat numbers as upper bounds.
"""
import asyncio, json, statistics, sys, time
import websockets

DUR = float(sys.argv[1]) if len(sys.argv) > 1 else 120
SYMS = ["BTC", "ETH", "SOL", "DOGE", "XRP"]
res = {}


def rec(name, ts_ms, extra=None):
    now = time.time_ns() / 1e6
    r = res.setdefault(name, {"lat": [], "n": 0, "lat2": []})
    r["n"] += 1
    if ts_ms:
        r["lat"].append(now - float(ts_ms))
    if extra:
        r["lat2"].append(now - float(extra))


async def run(name, url, sub, handler, ping=None):
    t_end = time.time() + DUR
    try:
        async with websockets.connect(url, max_size=2**22, ping_interval=None, compression=None) as ws:
            for m in sub:
                await ws.send(json.dumps(m) if not isinstance(m, str) else m)
            last_ping = time.time()
            while time.time() < t_end:
                try:
                    msg = await asyncio.wait_for(ws.recv(), timeout=5)
                except asyncio.TimeoutError:
                    msg = None
                if ping and time.time() - last_ping > 15:
                    await ws.send(ping)
                    last_ping = time.time()
                if not msg or msg in ("pong",):
                    continue
                handler(json.loads(msg))
    except Exception as e:  # report and continue other venues
        res.setdefault(name, {"lat": [], "n": 0, "lat2": []})["error"] = repr(e)[:200]


def h_binance(d):
    x = d.get("data", d)
    if x.get("e") == "bookTicker":
        rec("binance_usdm_bookTicker(T)", x.get("T"), x.get("E"))


def h_gate(d):
    if d.get("event") == "update" and d.get("channel") == "futures.book_ticker":
        x = d["result"]
        rec("gate_futures_book_ticker(t)", x.get("t"), d.get("time_ms"))


def h_okx(d):
    if "data" in d and d.get("arg", {}).get("channel") == "bbo-tbt":
        rec("okx_swap_bbo-tbt(ts)", d["data"][0].get("ts"))


def h_bybit(d):
    if str(d.get("topic", "")).startswith("orderbook.1."):
        rec("bybit_linear_orderbook.1(cts)", d.get("cts"), d.get("ts"))


def h_bitget(d):
    if d.get("action") in ("snapshot", "update") and d.get("arg", {}).get("channel") == "books1":
        rec("bitget_usdtfut_books1_v2(ts)", d["data"][0].get("ts"), d.get("ts"))


async def main():
    bn = "/".join(f"{s.lower()}usdt@bookTicker" for s in SYMS)
    await asyncio.gather(
        run("binance", f"wss://fstream.binance.com/stream?streams={bn}", [], h_binance),
        run("gate", "wss://fx-ws.gateio.ws/v4/ws/usdt",
            [{"time": int(time.time()), "channel": "futures.book_ticker", "event": "subscribe",
              "payload": [f"{s}_USDT" for s in SYMS]}], h_gate),
        run("okx", "wss://ws.okx.com:8443/ws/v5/public",
            [{"op": "subscribe", "args": [{"channel": "bbo-tbt", "instId": f"{s}-USDT-SWAP"} for s in SYMS]}],
            h_okx, ping="ping"),
        run("bybit", "wss://stream.bybit.com/v5/public/linear",
            [{"op": "subscribe", "args": [f"orderbook.1.{s}USDT" for s in SYMS]}], h_bybit,
            ping=json.dumps({"op": "ping"})),
        run("bitget", "wss://ws.bitget.com/v2/ws/public",
            [{"op": "subscribe", "args": [{"instType": "USDT-FUTURES", "channel": "books1", "instId": f"{s}USDT"} for s in SYMS]}],
            h_bitget, ping="ping"),
    )


asyncio.run(main())


def q(v, p):
    v = sorted(v)
    return round(v[min(len(v) - 1, int(p * len(v)))], 2) if v else None


summary = {}
for k, r in res.items():
    s = {"msgs": r["n"], "msg_per_s": round(r["n"] / DUR, 1)}
    for key, lab in (("lat", "exch_ts"), ("lat2", "send_ts")):
        v = r[key]
        if v:
            s[lab] = {"p1": q(v, .01), "p50": q(v, .5), "p90": q(v, .9), "p99": q(v, .99), "max": q(v, 1.0), "n": len(v)}
    if "error" in r:
        s["error"] = r["error"]
    summary[k] = s
print(json.dumps(summary, indent=1, ensure_ascii=False))
json.dump(summary, open(f"md_probe_{time.strftime('%Y%m%dT%H%M', time.gmtime())}.json", "w"), indent=1)
