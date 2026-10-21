#!/usr/bin/env python3
"""Slot discovery for hft-core: binds the Polymarket 5-minute windows to the refdata slot insts.

usage: bind_slots.py --socket CORE_SOCK [--assets btc,eth,sol,xrp] [--lead 120] [--once] [--log FILE]
                     [--issuer pipeline] [--slots 2] [--settle]

Slots (refdata of `pm_sim_config`): `<asset>-<up|dn>-<k>`, k = (window start / 300) mod `--slots`,
so consecutive windows of an asset use different slots. The window starting at S is bound from
S - lead on, when its slot's previous window ended (slots - 1) x 300 - lead seconds ago (180 s with
two slots and the default lead). Token ids come from the Gamma API; a missing market leaves the slot
unchanged.

`--settle`: after a bound window ends, its market on the CLOB API (`/markets/<conditionId>`) is
polled until a token is marked `winner` (2026-10-20: 178-204 s after the end), then one ctl `Settle`
per slot (`payout` "1" for the winner, "0" for the other) settles the simulated accounts' inventory
(hft-core: only Sim accounts, NoOp when nobody holds the slot). A slot holding inventory cannot be
rebound before that, so `--settle` needs `--slots 3` (rebinding 480 s after the previous end).

Every binding is one ctl `BindSlot` (account `*`) on the Unix socket: a 4-byte big-endian length
and the request JSON; an `accepted` response is re-sent with the same cmd_id until the result is
final; requests are paced (`--gap`, default 0.25 s) under the gateway's per-issuer rate limit. A
refused binding or settlement (`RateLimited`, `PreconditionFailed` such as `GroupHasPosition`) is
retried with a per-slot backoff of 5 s doubling to 60 s; an `InvalidArgs` settlement (the slot is
not bound to that window) is dropped, and a window that has ended is never bound. Run it as the system user that the principal table maps to `pipeline` (or, by hand, with
`--issuer cli:<login>` as a Trader/Admin cli user). Each request and response is one JSON line.
"""
import argparse
import json
import os
import socket
import struct
import sys
import time
import urllib.request
import uuid

GAMMA = "https://gamma-api.polymarket.com/events?slug={a}-updown-5m-{ts}"
CLOB_MARKET = "https://clob.polymarket.com/markets/{cid}"
WINDOW = 300


def tokens(asset: str, ts: int):
    """(Up/Down token ids, conditionId) of the window starting at `ts`, or None when Gamma has no
    such market."""
    req = urllib.request.Request(GAMMA.format(a=asset, ts=ts), headers={"User-Agent": "bind-slots/1"})
    with urllib.request.urlopen(req, timeout=10) as r:
        ev = json.load(r)
    if not ev:
        return None
    m = ev[0]["markets"][0]
    ids = json.loads(m["clobTokenIds"])
    outcomes = [o.lower() for o in json.loads(m.get("outcomes", "[]"))]
    if len(ids) != 2 or outcomes != ["up", "down"]:
        raise ValueError(f"unexpected outcomes {outcomes} for {asset} {ts}")
    return ids, m["conditionId"]


def winner(cid: str):
    """Winning token id of a resolved market, or None while it is not resolved."""
    req = urllib.request.Request(CLOB_MARKET.format(cid=cid), headers={"User-Agent": "bind-slots/1"})
    with urllib.request.urlopen(req, timeout=10) as r:
        m = json.load(r)
    for t in m.get("tokens", []):
        if t.get("winner"):
            return str(t["token_id"])
    return None


def exchange(path: str, body: bytes, timeout: float = 10.0) -> dict:
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


def command(path: str, issuer: str, cmd: dict, log, info: dict) -> str:
    """Sends one ctl command; an `accepted` response is re-sent with the same cmd_id until final.
    Returns "ok" (Applied or NoOp), "invalid" (InvalidArgs) or "retry"."""
    now = time.time_ns()
    req = {
        "v": 2,
        "cmd_id": str(uuid.uuid4()),
        "issuer": issuer,
        "issued_at_ns": now,
        "expires_at_ns": now + 30_000_000_000,
        "cmd": cmd,
    }
    body = json.dumps(req, separators=(",", ":")).encode()
    for _ in range(20):
        resp = exchange(path, body)
        log({"t": time.time(), **info, "resp": resp})
        if resp.get("status") != "accepted":
            break
        time.sleep(0.5)
    if resp.get("status") == "ok" and resp.get("code") in ("Applied", "NoOp"):
        return "ok"
    return "invalid" if resp.get("code") == "InvalidArgs" else "retry"


class Backoff:
    """Per-key retry delay: 5 s after the first refusal, doubling to 60 s; cleared on success."""

    def __init__(self):
        self.due = {}

    def ready(self, key, now: float) -> bool:
        return self.due.get(key, (0.0, 0))[0] <= now

    def fail(self, key, now: float):
        n = self.due.get(key, (0.0, 0))[1] + 1
        self.due[key] = (now + min(5 * 2 ** (n - 1), 60), n)

    def clear(self, key):
        self.due.pop(key, None)


def bind(path: str, issuer: str, slot: str, token: str, start: int, log) -> str:
    cmd = {
        "type": "BindSlot",
        "venue": "polymarket-clob",
        "inst": f"polymarket-clob:{slot}",
        "start_s": str(start),
        "end_s": str(start + WINDOW),
        "wire_id": token,
        "account": "*",
    }
    return command(path, issuer, cmd, log, {"slot": slot, "start": start, "token": token})


def settle(path: str, issuer: str, slot: str, end: int, payout: str, log) -> str:
    cmd = {
        "type": "Settle",
        "venue": "polymarket-clob",
        "inst": f"polymarket-clob:{slot}",
        "end_s": str(end),
        "payout": payout,
        "account": "*",
    }
    return command(path, issuer, cmd, log, {"settle": slot, "end": end, "payout": payout})


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--socket", required=True)
    ap.add_argument("--assets", default="btc,eth,sol,xrp")
    ap.add_argument("--lead", type=int, default=120)
    ap.add_argument("--once", action="store_true")
    ap.add_argument("--log")
    ap.add_argument("--issuer", default="pipeline")
    ap.add_argument("--gap", type=float, default=0.25)
    ap.add_argument("--slots", type=int, default=2)
    ap.add_argument("--settle", action="store_true")
    a = ap.parse_args()
    out = open(a.log, "a", encoding="utf-8") if a.log else sys.stdout

    def log(d):
        out.write(json.dumps(d) + "\n")
        out.flush()

    bound = {}  # slot -> (token, start) last applied
    cache = {}  # (asset, start) -> ([up, dn], conditionId) | None
    unsettled = {}  # (asset, start) -> ([up, dn], conditionId, k) of bound windows not yet settled
    backoff = Backoff()
    settled = set()  # (slot, end) settled or dropped
    while True:
        now = int(time.time())
        base = now // WINDOW * WINDOW
        for start in (base, base + WINDOW):
            if now < start - a.lead:
                continue
            k = start // WINDOW % a.slots
            for asset in a.assets.split(","):
                if (asset, start) not in cache:
                    try:
                        cache[(asset, start)] = tokens(asset, start)
                    except Exception as e:  # network or shape: retry on the next pass
                        log({"t": time.time(), "asset": asset, "start": start, "gamma_error": str(e)})
                        continue
                hit = cache[(asset, start)]
                if hit is None:
                    continue
                ids, cid = hit
                for side, token in zip(("up", "dn"), ids):
                    slot = f"{asset}-{side}-{k}"
                    if bound.get(slot) == (token, start) or now >= start + WINDOW:
                        continue
                    if not backoff.ready(("bind", slot), time.time()):
                        continue
                    try:
                        r = bind(a.socket, a.issuer, slot, token, start, log)
                    except OSError as e:
                        log({"t": time.time(), "slot": slot, "ctl_error": str(e)})
                        r = "retry"
                    if r == "ok":
                        backoff.clear(("bind", slot))
                        bound[slot] = (token, start)
                        if a.settle:
                            unsettled[(asset, start)] = (ids, cid, k)
                    else:
                        backoff.fail(("bind", slot), time.time())
                    time.sleep(a.gap)
        cache = {key: v for key, v in cache.items() if key[1] >= base - WINDOW}
        for (asset, start), (ids, cid, k) in list(unsettled.items()):
            end = start + WINDOW
            if now < end + 60:
                continue
            try:
                win = winner(cid)
            except Exception as e:  # network: retry on the next pass
                log({"t": time.time(), "asset": asset, "start": start, "clob_error": str(e)})
                continue
            if win is None:
                if now > end + 3600:
                    log({"t": time.time(), "asset": asset, "start": start, "unresolved": True})
                    unsettled.pop((asset, start))
                continue
            key = ("settle", asset, start)
            if not backoff.ready(key, time.time()):
                continue
            done = True
            for side, token in zip(("up", "dn"), ids):
                slot = f"{asset}-{side}-{k}"
                if (slot, end) in settled:
                    continue
                try:
                    r = settle(a.socket, a.issuer, slot, end, "1" if token == win else "0", log)
                except OSError as e:
                    log({"t": time.time(), "settle": slot, "ctl_error": str(e)})
                    r = "retry"
                if r == "invalid":
                    log({"t": time.time(), "settle": slot, "end": end, "dropped": True})
                if r == "retry":
                    done = False
                else:
                    settled.add((slot, end))
                time.sleep(a.gap)
            if done:
                backoff.clear(key)
                unsettled.pop((asset, start))
            else:
                backoff.fail(key, time.time())
        settled = {x for x in settled if x[1] >= now - 2 * 3600}
        if a.once:
            break
        time.sleep(5)


if __name__ == "__main__":
    if os.name != "posix":
        raise SystemExit("bind_slots.py needs a Unix socket")
    main()
