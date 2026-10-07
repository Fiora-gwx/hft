"""Substitute `Env` for az-probe tests: scripted DNS answers, handshake times and WebSocket frames."""

from __future__ import annotations

from az_probe.probe import WsSpec


class FakeEnv:
    """Records every call; never touches the network."""

    def __init__(
        self,
        resolves: dict[str, list[list[str]]],
        rtt: dict[str, list[float]] | float = 300.0,
        tls: float = 2000.0,
        frames: list[tuple[int, str]] | None = None,
        phc: bool = False,
        placement: tuple[str, str] = ("apne1-az4", "cpg-a"),
    ) -> None:
        self.resolves = resolves
        self.rtt = rtt
        self.tls = tls
        self.frames = frames or []
        self.phc = phc
        self._placement = placement
        self.resolve_calls: dict[str, int] = {}
        self.tcp_calls: dict[str, int] = {}
        self.ws_calls: list[WsSpec] = []
        self.fetch_calls: list[str] = []
        self.clock = 0.0

    def resolve(self, host: str, port: int) -> list[str]:
        k = self.resolve_calls.get(host, 0)
        self.resolve_calls[host] = k + 1
        seq = self.resolves[host]
        return list(seq[k % len(seq)])

    def tcp_handshake_us(self, ip: str, port: int) -> float:
        k = self.tcp_calls.get(ip, 0)
        self.tcp_calls[ip] = k + 1
        if isinstance(self.rtt, dict):
            seq = self.rtt[ip]
            return seq[k % len(seq)]
        return self.rtt

    def tls_handshake_us(self, ip: str, port: int, host: str) -> float:
        return self.tls

    def ws_frames(self, spec: WsSpec, n: int, timeout_s: float) -> list[tuple[int, str]]:
        self.ws_calls.append(spec)
        return self.frames[:n]

    def phc_synced(self) -> bool:
        return self.phc

    def placement(self) -> tuple[str, str]:
        return self._placement

    def fetch(self, url: str) -> str | None:
        self.fetch_calls.append(url)
        return None

    def now_ns(self) -> int:
        return 1_759_800_000_000_000_000 + int(self.clock * 1e9)

    def monotonic(self) -> float:
        return self.clock

    def sleep(self, s: float) -> None:
        self.clock += max(0.0, s)
