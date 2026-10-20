#!/usr/bin/env python3
"""Slot discovery for hft-core: binds the Polymarket 5-minute windows to the refdata slot insts.

usage: bind_slots.py --socket CORE_SOCK [--assets btc,eth,sol,xrp] [--lead 120] [--once] [--log FILE]

Slots (refdata of `pm_sim_config`): `<asset>-<up|dn>-<k>`, k = (window start / 300) mod 2, so the
current and the next window of an asset use different slots. The window starting at S is bound
from S - lead on; the slot's previous window ended at S - 300 and the exchange resolves it about
147 s after its end, so the default lead (120 s) rebinds a slot 180 s after its previous window
ended. Token ids come from the Gamma API; a missing market leaves the slot unchanged.

Every binding is one ctl `BindSlot` (issuer `pipeline`, account `*`) on the Unix socket: a 4-byte
big-endian length and the request JSON; an `accepted` response is re-sent with the same cmd_id
until the result is final. Run it as the system user that the principal table maps to
`pipeline`. Each request and response is logged as one JSON line.
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
WINDOW = 300


def tokens(asset: str, ts: int):
    """Up/Down token ids of the window starting at `ts`, or None when Gamma has no such market."""
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
    return ids


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


def bind(path: str, slot: str, token: str, start: int, log) -> bool:
    now = time.time_ns()
    req = {
        "v": 2,
        "cmd_id": str(uuid.uuid4()),
        "issuer": "pipeline",
        "issued_at_ns": now,
        "expires_at_ns": now + 30_000_000_000,
        "cmd": {
            "type": "BindSlot",
            "venue": "polymarket-clob",
            "inst": f"polymarket-clob:{slot}",
            "start_s": str(start),
            "end_s": str(start + WINDOW),
            "wire_id": token,
            "account": "*",
        },
    }
    body = json.dumps(req, separators=(",", ":")).encode()
    for _ in range(20):
        resp = exchange(path, body)
        log({"t": time.time(), "slot": slot, "start": start, "token": token, "resp": resp})
        if resp.get("status") != "accepted":
            break
        time.sleep(0.5)
    return resp.get("status") == "ok" and resp.get("code") in ("Applied", "NoOp")


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--socket", required=True)
    ap.add_argument("--assets", default="btc,eth,sol,xrp")
    ap.add_argument("--lead", type=int, default=120)
    ap.add_argument("--once", action="store_true")
    ap.add_argument("--log")
    a = ap.parse_args()
    out = open(a.log, "a", encoding="utf-8") if a.log else sys.stdout

    def log(d):
        out.write(json.dumps(d) + "\n")
        out.flush()

    bound = {}  # slot -> (token, start) last applied
    cache = {}  # (asset, start) -> [up, dn] | None
    while True:
        now = int(time.time())
        base = now // WINDOW * WINDOW
        for start in (base, base + WINDOW):
            if now < start - a.lead:
                continue
            k = start // WINDOW % 2
            for asset in a.assets.split(","):
                if (asset, start) not in cache:
                    try:
                        cache[(asset, start)] = tokens(asset, start)
                    except Exception as e:  # network or shape: retry on the next pass
                        log({"t": time.time(), "asset": asset, "start": start, "gamma_error": str(e)})
                        continue
                ids = cache[(asset, start)]
                if ids is None:
                    continue
                for side, token in zip(("up", "dn"), ids):
                    slot = f"{asset}-{side}-{k}"
                    if bound.get(slot) == (token, start):
                        continue
                    try:
                        if bind(a.socket, slot, token, start, log):
                            bound[slot] = (token, start)
                    except OSError as e:
                        log({"t": time.time(), "slot": slot, "ctl_error": str(e)})
        cache = {key: v for key, v in cache.items() if key[1] >= base - WINDOW}
        if a.once:
            break
        time.sleep(5)


if __name__ == "__main__":
    if os.name != "posix":
        raise SystemExit("bind_slots.py needs a Unix socket")
    main()
