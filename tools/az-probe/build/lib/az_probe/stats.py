"""Order statistics used by the report (nearest-rank percentiles)."""

from __future__ import annotations

import math


def percentile(values: list[float], q: float) -> float:
    """Nearest-rank percentile (`q` in 0..1) of a non-empty list."""
    s = sorted(values)
    k = min(len(s), max(1, math.ceil(q * len(s))))
    return s[k - 1]


def us(x: float) -> float:
    """Microseconds rounded to 0.1 µs."""
    return round(x, 1)
