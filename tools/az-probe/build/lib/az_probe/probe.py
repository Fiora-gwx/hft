"""`az-probe run`: the measurement. Every side effect (DNS, sockets, TLS, WebSocket, clocks, IMDS,
HTTP fetches) goes through an `Env`, so tests substitute it."""

from __future__ import annotations

import datetime as dt
import json
import math
import os
import socket
import ssl
import subprocess
import time
import tomllib
import urllib.request
from dataclasses import dataclass, field
from decimal import Decimal
from typing import Any, Protocol

from . import attribution
from .stats import percentile, us

TCP_SAMPLES = 9
TLS_SAMPLES = 3
RESOLVES = 3
FIRST_FRAMES = 200


@dataclass(frozen=True)
class WsSpec:
    """Public market-data stream used by `--ws-first-frame`."""

    url: str
    subscribe: str
    ts: str
    unit: str


@dataclass(frozen=True)
class Target:
    name: str
    host: str
    exchange: str
    core: bool
    port: int = 443
    tls: bool = True
    ws: WsSpec | None = None


def load_targets(text: str) -> list[Target]:
    """Parses `targets.toml` (`[[target]]` tables; format in the file header)."""
    out = []
    for t in tomllib.loads(text).get("target", []):
        w = t.get("ws")
        ws = None if w is None else WsSpec(w["url"], w.get("subscribe", ""), w["ts"], w.get("unit", "ms"))
        out.append(
            Target(
                name=t["name"],
                host=t["host"],
                exchange=t["exchange"],
                core=bool(t.get("core", False)),
                port=int(t.get("port", 443)),
                tls=bool(t.get("tls", True)),
                ws=ws,
            )
        )
    return out


class Env(Protocol):
    """Side effects of a run."""

    def resolve(self, host: str, port: int) -> list[str]:
        """IPv4 A records (one resolution)."""
        ...

    def tcp_handshake_us(self, ip: str, port: int) -> float:
        """Duration of one TCP connect (SYN → SYN-ACK), µs."""
        ...

    def tls_handshake_us(self, ip: str, port: int, host: str) -> float:
        """Duration of one TLS handshake on a fresh connection, µs."""
        ...

    def ws_frames(self, spec: WsSpec, n: int, timeout_s: float) -> list[tuple[int, str]]:
        """Up to `n` text frames of the stream with their receive time (UTC ns)."""
        ...

    def phc_synced(self) -> bool:
        """A PHC exists and chrony uses it as its reference."""
        ...

    def placement(self) -> tuple[str, str]:
        """(AZ ID, placement group) of this instance."""
        ...

    def fetch(self, url: str) -> str | None:
        """Text of a public URL, `None` on failure."""
        ...

    def now_ns(self) -> int:
        """UTC wall clock, ns."""
        ...

    def monotonic(self) -> float:
        """Monotonic clock, s."""
        ...

    def sleep(self, s: float) -> None: ...


class RealEnv:
    """The real network and clocks."""

    def __init__(self) -> None:
        self._ctx = ssl.create_default_context()

    def resolve(self, host: str, port: int) -> list[str]:
        try:
            infos = socket.getaddrinfo(host, port, socket.AF_INET, socket.SOCK_STREAM)
        except OSError:
            return []
        out: list[str] = []
        for info in infos:
            ip = str(info[4][0])
            if ip not in out:
                out.append(ip)
        return out

    def tcp_handshake_us(self, ip: str, port: int) -> float:
        t0 = time.perf_counter_ns()
        s = socket.create_connection((ip, port), timeout=3)
        t1 = time.perf_counter_ns()
        s.close()
        return (t1 - t0) / 1000.0

    def tls_handshake_us(self, ip: str, port: int, host: str) -> float:
        raw = socket.create_connection((ip, port), timeout=3)
        try:
            t0 = time.perf_counter_ns()
            s = self._ctx.wrap_socket(raw, server_hostname=host)
            t1 = time.perf_counter_ns()
            s.close()
        finally:
            raw.close()
        return (t1 - t0) / 1000.0

    def ws_frames(self, spec: WsSpec, n: int, timeout_s: float) -> list[tuple[int, str]]:
        from websockets.sync.client import connect

        out: list[tuple[int, str]] = []
        deadline = time.monotonic() + timeout_s
        with connect(spec.url, open_timeout=5) as ws:
            if spec.subscribe:
                ws.send(spec.subscribe)
            while len(out) < n and time.monotonic() < deadline:
                try:
                    msg = ws.recv(timeout=max(0.1, deadline - time.monotonic()))
                except TimeoutError:
                    break
                t = time.time_ns()
                out.append((t, msg if isinstance(msg, str) else msg.decode("utf-8", "replace")))
        return out

    def phc_synced(self) -> bool:
        if not os.path.exists("/dev/ptp0"):
            return False
        try:
            out = subprocess.run(
                ["chronyc", "-c", "sources"], capture_output=True, text=True, timeout=5, check=False
            ).stdout
        except OSError:
            return False
        return any(
            f[0] == "#" and f[1] == "*" and f[2].startswith("PHC")
            for f in (line.split(",") for line in out.splitlines())
            if len(f) > 2
        )

    def placement(self) -> tuple[str, str]:
        def get(path: str, token: str) -> str:
            req = urllib.request.Request(
                f"http://169.254.169.254/latest/meta-data/{path}",
                headers={"X-aws-ec2-metadata-token": token},
            )
            with urllib.request.urlopen(req, timeout=2) as r:
                return str(r.read().decode().strip())

        try:
            req = urllib.request.Request(
                "http://169.254.169.254/latest/api/token",
                method="PUT",
                headers={"X-aws-ec2-metadata-token-ttl-seconds": "300"},
            )
            with urllib.request.urlopen(req, timeout=2) as r:
                token = r.read().decode().strip()
        except OSError:
            return ("unknown", "unknown")
        az = "unknown"
        cpg = "none"
        try:
            az = get("placement/availability-zone-id", token)
        except OSError:
            pass
        try:
            cpg = get("placement/group-name", token)
        except OSError:
            pass
        return (az, cpg)

    def fetch(self, url: str) -> str | None:
        try:
            with urllib.request.urlopen(url, timeout=10) as r:
                return str(r.read().decode())
        except OSError:
            return None

    def now_ns(self) -> int:
        return time.time_ns()

    def monotonic(self) -> float:
        return time.monotonic()

    def sleep(self, s: float) -> None:
        if s > 0:
            time.sleep(s)


def parse_duration(s: str) -> float:
    """`90s`, `1m`, `60m`, `1h` → seconds."""
    units = {"s": 1, "m": 60, "h": 3600}
    if s and s[-1] in units:
        return float(s[:-1]) * units[s[-1]]
    return float(s)


def _ts(frame: str, path: str) -> Decimal | None:
    try:
        v: Any = json.loads(frame)
    except ValueError:
        return None
    for part in path.split("."):
        if isinstance(v, list) and part.isdigit() and int(part) < len(v):
            v = v[int(part)]
        elif isinstance(v, dict) and part in v:
            v = v[part]
        else:
            return None
    if isinstance(v, bool) or not isinstance(v, (int, float, str)):
        return None
    try:
        return Decimal(str(v))
    except ArithmeticError:
        return None


@dataclass
class _Acc:
    tcp: list[float] = field(default_factory=list)
    tls: list[float] = field(default_factory=list)
    errors: int = 0


def run(
    targets: list[Target],
    env: Env,
    duration_s: float,
    ws_first_frame: bool = False,
    interval_s: float = 600.0,
    az: str | None = None,
    cpg: str | None = None,
    ranges: attribution.Ranges | None = None,
    resolve_gap_s: float = 2.0,
) -> dict[str, Any]:
    """Runs `ceil(duration / interval)` cycles (at least one); returns the report."""
    if az is None or cpg is None:
        paz, pcpg = env.placement()
        az = az or paz
        cpg = cpg or pcpg
    started = env.now_ns()
    t_start = env.monotonic()
    cycles = max(1, math.ceil(duration_s / interval_s))
    acc: dict[tuple[str, str], _Acc] = {}
    for c in range(cycles):
        t_cycle = env.monotonic()
        for t in targets:
            ips: list[str] = []
            for i in range(RESOLVES):
                if i:
                    env.sleep(resolve_gap_s)
                for ip in env.resolve(t.host, t.port):
                    if ip not in ips:
                        ips.append(ip)
            for ip in ips:
                a = acc.setdefault((t.name, ip), _Acc())
                for _ in range(TCP_SAMPLES):
                    try:
                        a.tcp.append(env.tcp_handshake_us(ip, t.port))
                    except OSError:
                        a.errors += 1
                if t.tls:
                    for _ in range(TLS_SAMPLES):
                        try:
                            a.tls.append(env.tls_handshake_us(ip, t.port, t.host))
                        except (OSError, ssl.SSLError):
                            a.errors += 1
        if c + 1 < cycles:
            env.sleep(interval_s - (env.monotonic() - t_cycle))
    rows = []
    for (name, ip), a in sorted(acc.items()):
        rows.append(
            {
                "az": az,
                "cpg": cpg,
                "endpoint": name,
                "ip": ip,
                "min_rtt_us": us(min(a.tcp)) if a.tcp else None,
                "p50_us": us(percentile(a.tcp, 0.5)) if a.tcp else None,
                "p99_us": us(percentile(a.tcp, 0.99)) if a.tcp else None,
                "tls_handshake_us": us(min(a.tls)) if a.tls else None,
                "n": len(a.tcp),
            }
        )
    first = []
    if ws_first_frame:
        clock = "phc" if env.phc_synced() else "unsynced"
        scale = {"ms": Decimal(1_000_000), "us": Decimal(1_000), "ns": Decimal(1)}
        for t in targets:
            if t.ws is None:
                continue
            frames = env.ws_frames(t.ws, FIRST_FRAMES, 60.0)
            d = []
            for recv_ns, text in frames:
                e = _ts(text, t.ws.ts)
                if e is not None:
                    e_ns = int(e * scale.get(t.ws.unit, Decimal(1_000_000)))
                    d.append((recv_ns - e_ns) / 1000.0)
            first.append(
                {
                    "az": az,
                    "cpg": cpg,
                    "endpoint": t.name,
                    "clock": clock,
                    "n": len(d),
                    "min_us": us(min(d)) if d else None,
                    "p50_us": us(percentile(d, 0.5)) if d else None,
                    "p99_us": us(percentile(d, 0.99)) if d else None,
                }
            )
    return {
        "schema": 1,
        "az": az,
        "cpg": cpg,
        "started_utc": dt.datetime.fromtimestamp(started / 1e9, dt.UTC).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "duration_s": round(env.monotonic() - t_start, 1),
        "cycles": cycles,
        "endpoints": {
            t.name: {"host": t.host, "port": t.port, "exchange": t.exchange, "core": t.core} for t in targets
        },
        "attribution": {ip: attribution.classify(ranges, ip) for (_, ip) in sorted(acc)},
        "rows": rows,
        "first_frame": first,
    }
