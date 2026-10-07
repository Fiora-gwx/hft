"""`az-probe compare report-*.json`: ranks (AZ, CPG) by the sum over the core exchanges of their
smallest handshake minRTT; the decision rules on top of the ranking are in
docs/runbooks/az-probe.md."""

from __future__ import annotations

import math
from dataclasses import dataclass, field
from typing import Any


@dataclass
class Group:
    """All rows of one (AZ, CPG), merged over reports."""

    az: str
    cpg: str
    # exchange -> (minRTT, p99 of that row), core flag per exchange
    best: dict[str, tuple[float, float]] = field(default_factory=dict)
    core: dict[str, bool] = field(default_factory=dict)

    def core_sum(self, core_exchanges: list[str]) -> float:
        if any(e not in self.best for e in core_exchanges):
            return math.inf
        return sum(self.best[e][0] for e in core_exchanges)


def groups(reports: list[dict[str, Any]]) -> tuple[list[Group], list[str], list[str]]:
    """Merged groups, the core exchanges and the optional exchanges (sorted)."""
    by: dict[tuple[str, str], Group] = {}
    core: set[str] = set()
    optional: set[str] = set()
    for r in reports:
        eps = r.get("endpoints", {})
        for row in r.get("rows", []):
            if row.get("min_rtt_us") is None:
                continue
            ep = eps.get(row["endpoint"], {})
            ex = ep.get("exchange", row["endpoint"])
            (core if ep.get("core") else optional).add(ex)
            g = by.setdefault((row["az"], row["cpg"]), Group(row["az"], row["cpg"]))
            cur = g.best.get(ex)
            if cur is None or row["min_rtt_us"] < cur[0]:
                g.best[ex] = (float(row["min_rtt_us"]), float(row.get("p99_us") or 0.0))
            g.core[ex] = bool(ep.get("core"))
    return list(by.values()), sorted(core), sorted(optional - core)


def rank(reports: list[dict[str, Any]]) -> list[tuple[str, str, float]]:
    """`(az, cpg, core minRTT sum µs)` in ascending order of the sum (missing core exchange = ∞)."""
    gs, core, _ = groups(reports)
    out = [(g.az, g.cpg, g.core_sum(core)) for g in gs]
    out.sort(key=lambda x: (x[2], x[0], x[1]))
    return out


def _f(x: float) -> str:
    return "—" if math.isinf(x) else (f"{x:.0f}" if x == int(x) else f"{x:.1f}")


def markdown(reports: list[dict[str, Any]]) -> str:
    """`az-report.md`: the ranking and, per exchange, minRTT / p99 for every (AZ, CPG)."""
    gs, core, optional = groups(reports)
    ranked = rank(reports)
    by = {(g.az, g.cpg): g for g in gs}
    lines = [
        "# AZ probe comparison",
        "",
        (
            f"Reports: {len(reports)}. Ranking key: sum of the core exchanges' smallest TCP handshake "
            f"minRTT ({', '.join(core)}); optional exchanges are listed but not summed. Decision rules: "
            "docs/runbooks/az-probe.md."
        ),
        "",
        "| rank | AZ | CPG | core sum (us) | " + " | ".join(core + optional) + " |",
        "|---|---|---|---|" + "---|" * len(core + optional),
    ]
    for i, (az, cpg, s) in enumerate(ranked, 1):
        g = by[(az, cpg)]
        cells = [_f(g.best[e][0]) if e in g.best else "—" for e in core + optional]
        lines.append(f"| {i} | {az} | {cpg} | {_f(s)} | " + " | ".join(cells) + " |")
    lines += ["", "## Per exchange: minRTT / p99 (us)", ""]
    lines.append("| exchange | " + " | ".join(f"{az} {cpg}" for az, cpg, _ in ranked) + " |")
    lines.append("|---|" + "---|" * len(ranked))
    for e in core + optional:
        cells = []
        for az, cpg, _ in ranked:
            b = by[(az, cpg)].best.get(e)
            cells.append("—" if b is None else f"{_f(b[0])} / {_f(b[1])}")
        lines.append(f"| {e} | " + " | ".join(cells) + " |")
    return "\n".join(lines) + "\n"
