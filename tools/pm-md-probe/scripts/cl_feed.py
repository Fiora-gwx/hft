"""Chainlink BTC/USD into binary-maker (rule 12, settle_src = 1): subscribes PolyBolt
`price.crypto` (btcusd) and sends every new sample to each hft-core ctl socket as `InjectWarmup`
(local_kind 1, payload v1: version u8 = 1, Chainlink timestamp ms i64 LE, price 1e-8 USD i64 LE).

usage: cl_feed.py --creds CLOB_CREDS.json (--socket RUN/cfg/core.sock | --discover 'DIR/live-*') ...
                  [--strategy pm-maker] [--issuer cli:<user>] [--log FILE]

--discover GLOB (repeatable): every 30 s the newest run directory matching each glob (name order)
that has cfg/core.sock is a target, so restarted runs are picked up.

PolyBolt needs CLOB API credentials (any account; keys api_key / api_secret / api_passphrase or
apiKey / secret / passphrase). Needs the `websockets` package. Samples go out in order, one command
each (the ctl gateway allows 50 InjectWarmup per second per issuer); a socket that refuses or is gone
is retried with the next sample. The log has one JSON line per event and one summary per minute
(never the credentials)."""
import argparse
import asyncio
import base64
import getpass
import glob
import json
import os
import socket
import struct
import time
import uuid
from decimal import Decimal

import websockets

POLYBOLT_URL = "wss://ws-live-v2.polymarket.com/ws"
LOCAL_KIND = 1


def exchange(path: str, body: bytes, timeout: float = 2.0) -> dict:
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.settimeout(timeout)
    try:
        s.connect(path)
        s.sendall(struct.pack(">I", len(body)) + body)
        buf = b""
        while True:
            if len(buf) >= 4:
                (n,) = struct.unpack(">I", buf[:4])
                if len(buf) >= 4 + n:
                    return json.loads(buf[4 : 4 + n])
            chunk = s.recv(65536)
            if not chunk:
                raise ConnectionError("closed before a response")
            buf += chunk
    finally:
        s.close()


def payload(ts_ms: int, px_e8: int) -> bytes:
    return bytes([1]) + struct.pack("<qq", ts_ms, px_e8)


def inject(path: str, issuer: str, strategy: str, chunk: int, data: bytes) -> str:
    now = time.time_ns()
    req = {
        "v": 2,
        "cmd_id": str(uuid.uuid4()),
        "issuer": issuer,
        "issued_at_ns": now,
        "expires_at_ns": now + 5_000_000_000,
        "cmd": {
            "type": "InjectWarmup",
            "strategy": strategy,
            "local_kind": str(LOCAL_KIND),
            "chunk_no": str(chunk),
            "last": True,
            "payload_b64": base64.b64encode(data).decode(),
        },
    }
    r = exchange(path, json.dumps(req, separators=(",", ":")).encode())
    return f"{r.get('status')}/{r.get('code')}"


def creds(path: str) -> dict:
    d = json.load(open(path))
    return {
        "apiKey": d.get("apiKey") or d["api_key"],
        "secret": d.get("secret") or d["api_secret"],
        "passphrase": d.get("passphrase") or d["api_passphrase"],
    }


class Feed:
    def __init__(self, a):
        self.a = a
        self.auth = creds(a.creds)
        self.last_ts = 0
        self.chunk = 0
        self.stats = {"samples": 0, "sent": 0, "fail": 0, "codes": {}}
        self.t_sum = time.time()
        self.log_f = open(a.log, "a") if a.log else None
        self.targets = list(a.socket or [])
        self.t_disc = 0.0

    def log(self, d: dict):
        line = json.dumps({"t": round(time.time(), 3), **d}, separators=(",", ":"))
        if self.log_f:
            self.log_f.write(line + "\n")
            self.log_f.flush()
        else:
            print(line, flush=True)

    def discover(self):
        if not self.a.discover or time.time() - self.t_disc < 30:
            return
        self.t_disc = time.time()
        found = list(self.a.socket or [])
        for g in self.a.discover:
            runs = [d for d in sorted(glob.glob(g)) if os.path.exists(os.path.join(d, "cfg", "core.sock"))]
            if runs:
                found.append(os.path.join(runs[-1], "cfg", "core.sock"))
        if found != self.targets:
            self.log({"ev": "targets", "sockets": found})
            self.targets = found

    def send_all(self, data: bytes):
        self.chunk += 1
        self.discover()
        for path in self.targets:
            try:
                code = inject(path, self.a.issuer, self.a.strategy, self.chunk, data)
            except OSError as e:
                code = f"error/{type(e).__name__}"
            ok = code.startswith("ok/")
            self.stats["sent" if ok else "fail"] += 1
            if not ok:
                self.stats["codes"][code] = self.stats["codes"].get(code, 0) + 1

    def on_msg(self, m: str):
        try:
            d = json.loads(m)
        except ValueError:
            return
        p = d.get("payload") or {}
        if d.get("channel") != "price.crypto" or p.get("symbol") != "btcusd":
            return
        ts = int(p.get("timestamp") or d.get("ts") or 0)
        val = p.get("full_accuracy_value") or p.get("value")
        if ts <= self.last_ts or val is None:
            return
        px = int(Decimal(str(val)) * 100_000_000)
        if px <= 0:
            return
        self.last_ts = ts
        self.stats["samples"] += 1
        self.send_all(payload(ts, px))
        if time.time() - self.t_sum >= 60:
            self.log({"summary": self.stats, "last_ts": ts, "px": px / 1e8, "lag_ms": int(time.time() * 1000) - ts})
            self.stats = {"samples": 0, "sent": 0, "fail": 0, "codes": {}}
            self.t_sum = time.time()

    async def run_once(self):
        async with websockets.connect(POLYBOLT_URL, max_size=None, open_timeout=10) as ws:
            await ws.send(json.dumps({"op": "auth", "rid": "a1", "auth": self.auth}))
            ack = json.loads(await asyncio.wait_for(ws.recv(), 10))
            if ack.get("op") != "authed":
                raise RuntimeError(f"auth refused: op={ack.get('op')}")
            subs = [{"channel": "price.crypto", "filter": {"symbol": "btcusd"}}]
            await ws.send(json.dumps({"op": "subscribe", "rid": "s1", "subscriptions": subs}))
            self.log({"ev": "subscribed"})
            while True:
                m = await asyncio.wait_for(ws.recv(), 30)
                await asyncio.to_thread(self.on_msg, m)

    async def main(self):
        delay = 1
        while True:
            t0 = time.time()
            try:
                await self.run_once()
            except Exception as e:  # reconnect on anything; the reason goes to the log
                self.log({"ev": "disconnected", "err": f"{type(e).__name__}: {str(e)[:200]}"})
            delay = 1 if time.time() - t0 > 60 else min(delay * 2, 30)
            await asyncio.sleep(delay)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--creds", required=True)
    ap.add_argument("--socket", action="append")
    ap.add_argument("--discover", action="append")
    ap.add_argument("--strategy", default="pm-maker")
    ap.add_argument("--issuer", default=f"cli:{getpass.getuser()}")
    ap.add_argument("--log")
    a = ap.parse_args()
    if not a.socket and not a.discover:
        ap.error("--socket or --discover is required")
    f = Feed(a)
    f.log({"ev": "start", "sockets": a.socket, "discover": a.discover, "strategy": a.strategy, "issuer": a.issuer})
    asyncio.run(f.main())


if __name__ == "__main__":
    main()
