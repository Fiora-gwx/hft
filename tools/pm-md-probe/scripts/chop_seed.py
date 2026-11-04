"""Seed binary-maker's chop gate after a restart (README "规则" 13 后"震荡开关预热"): decode the seq
journals of other runs (newest first), rebuild each recent window's Up-mid efficiency ratio the way the
strategy does (Up mid at the first decision of each second 30..240 s of the window, at least 100
seconds, |last - first| / path), and send them as InjectWarmup local_kind 2 to every target socket.

usage: chop_seed.py --socket RUN/cfg/core.sock [--socket ...] --runs '/data/pmcore/maker-*' [--runs ...]
                    --probe BUNDLE/bin/pm-md-probe [--windows 12] [--strategy pm-maker]
                    [--issuer cli:<user>] [--dry-run]

Source runs are the directories matching --runs other than the targets' runs, newest name first; a
window found in an earlier source is kept. Segments modified in the last windows × 300 s + 1 500 s
(window binds happen up to ~15 min ahead) are decoded; the segment still being written is skipped and
.jrn.zst segments are decompressed to a temporary directory. Prints one JSON line per step."""
import argparse
import base64
import getpass
import glob
import json
import os
import socket
import struct
import subprocess
import tempfile
import time
import uuid

ONE = 1_000_000_000
LOCAL_KIND = 2


def exchange(path: str, body: bytes, timeout: float = 5.0) -> dict:
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


def segments(run: str, since: float, tmp: str) -> list[str]:
    out = []
    for kind in ("prod", "sim"):
        d = glob.glob(os.path.join(run, "cfg", "cold", kind, "seq", "*"))
        for day in d:
            files = sorted(glob.glob(os.path.join(day, "*.jrn")) + glob.glob(os.path.join(day, "*.jrn.zst")))
            for f in files:
                if os.path.getmtime(f) < since:
                    continue
                if f.endswith(".zst"):
                    dst = os.path.join(tmp, os.path.basename(f)[:-4])
                    subprocess.run(["zstd", "-dqf", f, "-o", dst], check=True)
                    out.append(dst)
                elif os.path.exists(f[:-4] + ".seal"):
                    out.append(f)
    return sorted(out, key=os.path.basename)


def ratios(probe: str, run: str, segs: list[str], wanted: set[int]) -> dict[int, int]:
    """Efficiency ratio (1e-9) per window start in `wanted` found in these segments."""
    if not segs:
        return {}
    with tempfile.NamedTemporaryFile(suffix=".json") as b:
        p = subprocess.Popen([probe, "journal", "--refdata", os.path.join(run, "cfg", "refdata.bin"), "--out", "/dev/stdout",
                              "--bindings-out", b.name] + segs, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        book: dict[str, tuple[int, int]] = {}
        acc: dict[int, list] = {}  # window -> [first, last, path, n]
        last_sec = None
        for line in p.stdout:
            if not (line.startswith('{"k":"batch"') or line.startswith('{"k":"snap"') or line.startswith('{"k":"bbo"')):
                continue
            d = json.loads(line)
            if d["k"] != "bbo":
                if d["slot"] == "?":
                    continue
                book[d["slot"]] = (d["bb"], d["ba"])
            sec = d["eu"] // ONE
            if sec == last_sec:
                continue
            last_sec = sec
            w = sec // 300 * 300
            if w not in wanted or not (w + 30 <= sec <= w + 240):
                continue
            bb, ba = book.get(f"btc-{w}-up", (-1, -1))
            if bb <= 0 or ba <= 0 or bb >= ba:
                continue
            mid = (bb + ba) * ONE // 2000
            a = acc.setdefault(w, [mid, mid, 0, 0])
            if a[3] > 0:
                a[2] += abs(mid - a[1])
            a[1] = mid
            a[3] += 1
        p.wait()
    return {w: (abs(a[1] - a[0]) * ONE // a[2]) for w, a in acc.items() if a[3] >= 100 and a[2] > 0}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--socket", action="append", required=True)
    ap.add_argument("--runs", action="append", required=True)
    ap.add_argument("--probe", required=True)
    ap.add_argument("--windows", type=int, default=12)
    ap.add_argument("--strategy", default="pm-maker")
    ap.add_argument("--issuer", default=f"cli:{getpass.getuser()}")
    ap.add_argument("--dry-run", action="store_true")
    a = ap.parse_args()
    now = time.time()
    last = int(now) // 300 * 300 - 300
    wanted = {last - 300 * k for k in range(a.windows)}
    targets = {os.path.dirname(os.path.dirname(os.path.abspath(s))) for s in a.socket}
    runs = sorted({r for g in a.runs for r in glob.glob(g) if os.path.isdir(r) and os.path.abspath(r) not in targets},
                  key=os.path.basename, reverse=True)
    found: dict[int, int] = {}
    since = now - a.windows * 300 - 1500
    for run in runs:
        missing = wanted - set(found)
        if not missing:
            break
        with tempfile.TemporaryDirectory() as tmp:
            segs = segments(run, since, tmp)
            got = ratios(a.probe, run, segs, missing)
        print(json.dumps({"run": run, "segments": len(segs), "windows": len(got)}), flush=True)
        for w, er in got.items():
            found.setdefault(w, er)
    entries = sorted(found.items())
    print(json.dumps({"windows": [[w, round(er / ONE, 3)] for w, er in entries]}), flush=True)
    if not entries or a.dry_run:
        return
    data = bytes([1, len(entries)]) + b"".join(struct.pack("<qq", w, er) for w, er in entries)
    for path in a.socket:
        t = time.time_ns()
        req = {"v": 2, "cmd_id": str(uuid.uuid4()), "issuer": a.issuer, "issued_at_ns": t, "expires_at_ns": t + 30_000_000_000,
               "cmd": {"type": "InjectWarmup", "strategy": a.strategy, "local_kind": str(LOCAL_KIND), "chunk_no": "0",
                       "last": True, "payload_b64": base64.b64encode(data).decode()}}
        try:
            r = exchange(path, json.dumps(req, separators=(",", ":")).encode())
            print(json.dumps({"socket": path, "status": r.get("status"), "code": r.get("code")}), flush=True)
        except OSError as e:
            print(json.dumps({"socket": path, "error": f"{type(e).__name__}: {e}"}), flush=True)


if __name__ == "__main__":
    main()
