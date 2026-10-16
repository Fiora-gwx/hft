"""Bounded-memory reads; no resampling from the future and no funding extrapolation."""

import csv
import hashlib
import itertools
import json
from dataclasses import asdict, dataclass, fields
from decimal import ROUND_CEILING, ROUND_FLOOR, Decimal
from pathlib import Path

import numpy as np
import pandas as pd
import pyarrow.parquet as pq

from .core import I64, ONE, Book, CarryParams, Funding, Meta, TakerParams

KEY = ["exchange", "market", "symbol"]
COLUMNS = KEY + ["canonical_id", "sec", "last_ns", "bid", "ask", "bid_base", "ask_base"]


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def raw(value: object, exp: int, up: bool = False) -> int:
    n = Decimal(str(value)) / Decimal(10) ** exp
    if not n.is_finite():
        raise ValueError("nonfinite numeric input")
    n = int(n.to_integral_value(rounding=ROUND_CEILING if up else ROUND_FLOOR))
    if not -I64 - 1 <= n <= I64:
        raise ValueError("fixed-point input exceeds i64")
    return n


def bps_rate(value: object) -> int:
    return raw(value, -5)


def bounded_int(value: object, low: int, high: int, name: str) -> None:
    if type(value) is not int or not low <= value <= high:
        raise ValueError(f"{name} must be an integer in [{low}, {high}]")


def validate_params(p: TakerParams | CarryParams) -> None:
    for f in fields(p):
        value = getattr(p, f.name)
        if f.name == "enabled":
            if type(value) is not bool:
                raise ValueError("enabled must be boolean")
        else:
            bounded_int(value, -I64, I64, f.name)
    if isinstance(p, TakerParams):
        ranges = {
            "min_edge": (1, 100_000_000),
            "close_edge": (-100_000_000, 100_000_000),
            "slip": (0, 50_000_000),
            "depth_frac": (1, ONE),
            "max_chunk_notional": (1, 10**15),
            "max_position_notional": (p.max_chunk_notional, 10**15),
            "max_quote_age_ms": (1, 60_000),
            "cooldown_ms": (0, 3_600_000),
            "max_hedge_failures": (1, 100),
        }
    else:
        ranges = {
            "enter_apr": (0, 10 * ONE),
            "exit_apr": (-10 * ONE, p.enter_apr - 1),
            "borrow_apr": (0, 10 * ONE),
            "fee_horizon_ms": (60_000, 604_800_000),
            "entry_window_ms": (1000, 28_800_000),
            "min_lead_ms": (1000, min(600_000, p.entry_window_ms - 1)),
            "funding_max_age_ms": (1000, 600_000),
            "max_notional": (10**6, 10**12),
            "capital_fraction": (1, 900_000_000),
            "max_leverage_x100": (100, 1000),
            "max_cross": (0, 10_000_000),
            "retry_ms": (100, 60_000),
            "expected_basis_cost": (0, 100_000_000),
        }
    for name, (low, high) in ranges.items():
        bounded_int(getattr(p, name), low, high, name)


class Config:
    def __init__(self, content: dict):
        allowed = {
            "fees_bps",
            "taker",
            "carry",
            "metadata",
            "assumed_metadata",
            "capital_quote",
            "leverage_x100",
            "initial_position_qty",
            "symbols",
            "pair_allowlist",
            "max_price_ratio",
            "threshold_grid_bps",
            "fee_scenarios_bps",
            "carry_horizons_days",
        }
        unknown = content.keys() - allowed
        if unknown:
            raise ValueError(f"unknown config fields: {sorted(unknown)}")
        for pair in content.get("pair_allowlist", []):
            parts = pair.split("|")
            if len(parts) != 2 or not supported_pair(*parts):
                raise ValueError("pair_allowlist requires Binance perpetual / Bitget spot, same USDT symbol")
        self.content = content
        self.taker = TakerParams(**content.get("taker", {}))
        self.carry = CarryParams(**content.get("carry", {}))
        validate_params(self.taker)
        validate_params(self.carry)
        self.fees = {k: bps_rate(v) for k, v in content["fees_bps"].items()}
        if not self.fees or any(not -ONE < v < ONE for v in self.fees.values()):
            raise ValueError("fees_bps must be nonempty and each fee strictly between -10000 and 10000")
        self.max_price_ratio = float(content.get("max_price_ratio", 1.10))
        if not np.isfinite(self.max_price_ratio) or not 1 < self.max_price_ratio <= 2:
            raise ValueError("max_price_ratio must be in (1, 2]")
        self.thresholds = content.get("threshold_grid_bps", [1, 2, 3, 5, 10])
        self.fee_scenarios = content.get("fee_scenarios_bps", [1, 2, 5])
        self.horizons = content.get("carry_horizons_days", [1, 3, 7])
        if any(not np.isfinite(float(x)) or float(x) <= 0 for x in self.thresholds + self.horizons):
            raise ValueError("thresholds and horizons must be finite and positive")
        if any(not 0 <= float(x) < 10000 for x in self.fee_scenarios):
            raise ValueError("fee scenarios must be nonnegative bps below 10000")
        for key in ("capital_quote", "initial_position_qty"):
            for value in content.get(key, {}).values():
                if not Decimal(str(value)).is_finite() or Decimal(str(value)) < 0:
                    raise ValueError(f"{key} values must be finite and nonnegative")
        for value in content.get("leverage_x100", {}).values():
            bounded_int(value, 0, 1000, "leverage_x100")

    def fee(self, inst: str) -> int:
        market_key = ":".join(inst.split(":")[:2])
        if inst in self.fees:
            return self.fees[inst]
        if market_key in self.fees:
            return self.fees[market_key]
        raise ValueError(f"missing explicit fee for {inst}")

    def meta(self, inst: str) -> tuple[Meta, bool]:
        supplied = self.content.get("metadata", {}).get(inst)
        assumed = supplied is None
        if assumed:
            supplied = self.content.get("assumed_metadata")
        if supplied is None:
            raise ValueError(f"missing contract metadata for {inst}; assumptions must be explicit")
        m = Meta(**supplied, spot=inst.split(":")[1] == "spot")
        for name in ("px_exp", "qty_exp"):
            bounded_int(getattr(m, name), -18, 0, name)
        for name in ("tick", "lot", "min_qty", "max_qty"):
            bounded_int(getattr(m, name), 1, I64, name)
        bounded_int(m.min_notional, 0, I64, "min_notional")
        if m.min_qty > m.max_qty:
            raise ValueError("min_qty exceeds max_qty")
        return m, assumed

    def capital(self, inst: str) -> int:
        return raw(self.content.get("capital_quote", {}).get(inst.split(":")[0], 0), -6)

    def leverage(self, inst: str) -> int:
        return self.content.get("leverage_x100", {}).get(inst.split(":")[0], 0)

    def position(self, inst: str, meta: Meta) -> int:
        return raw(self.content.get("initial_position_qty", {}).get(inst, 0), meta.qty_exp)


def inventory(path: Path) -> tuple[list[str], list[str]]:
    symbols, venues = set(), set()
    for batch in pq.ParquetFile(path).iter_batches(
        batch_size=65536, columns=["canonical_id", "exchange"], use_threads=False
    ):
        d = batch.to_pydict()
        symbols.update(d["canonical_id"])
        venues.update(d["exchange"])
    if None in symbols or None in venues:
        raise ValueError("null canonical_id/exchange")
    return sorted(symbols), sorted(venues)


def read_symbol(path: Path, symbol: str) -> pd.DataFrame:
    d = pq.read_table(
        path, columns=COLUMNS, filters=[("canonical_id", "=", symbol)], use_threads=False
    ).to_pandas()
    if d[COLUMNS].isna().any().any():
        raise ValueError(f"{symbol}: null quote fields")
    if not d.market.isin(["spot", "linear_perpetual"]).all():
        raise ValueError(f"{symbol}: unsupported market")
    if d.duplicated(KEY + ["sec"]).any():
        raise ValueError(f"{symbol}: duplicate instrument/second")
    if ((d.last_ns // 10**9) != d.sec).any():
        raise ValueError(f"{symbol}: last_ns outside its declared second")
    if (d.sec <= 0).any():
        raise ValueError(f"{symbol}: invalid timestamp")
    d["inst"] = d[KEY].agg(":".join, axis=1)
    return d.sort_values(["inst", "sec"])


def supported_pair(a: str, b: str) -> bool:
    aa, bb = a.split(":"), b.split(":")
    return (
        len(aa) == len(bb) == 3
        and aa[:2] == ["binance", "linear_perpetual"]
        and bb[:2] == ["bitget", "spot"]
        and aa[2] == bb[2]
        and aa[2].endswith("USDT")
    )


def pairs(d: pd.DataFrame, cfg: Config):
    for a, b in itertools.combinations(sorted(d.inst.unique()), 2):
        if a.split(":")[0] == b.split(":")[0]:
            continue
        if a.split(":")[1] == "spot" and b.split(":")[1] != "spot":
            a, b = b, a
        if not supported_pair(a, b):
            continue
        name = a + "|" + b
        allowed = cfg.content.get("pair_allowlist")
        if allowed is not None and name not in allowed:
            continue
        left, right = (d[d.inst == x].drop(columns=KEY + ["canonical_id", "inst"]) for x in (a, b))
        joined = left.merge(right, on="sec", how="inner", suffixes=("_a", "_b"), validate="one_to_one")
        yield a, b, joined.sort_values("sec")


def books(d: pd.DataFrame, metas: tuple[Meta, Meta]):
    cols = [f"{name}_{s}" for s in ("a", "b") for name in ("bid", "ask", "bid_base", "ask_base", "last_ns")]
    for row in d[cols].itertuples(index=False, name=None):
        pair = []
        for k, m in enumerate(metas):
            bid, ask, bq, aq, ts = row[k * 5 : k * 5 + 5]
            # Outward price rounding cannot manufacture an executable spread.
            pair.append(
                Book(
                    raw(bid, m.px_exp),
                    raw(ask, m.px_exp, True),
                    raw(bq, m.qty_exp),
                    raw(aq, m.qty_exp),
                    int(ts),
                )
            )
        yield tuple(pair)


@dataclass(frozen=True)
class FundingEvent:
    inst: str
    kind: str
    ts_ns: int
    rate: int
    interval_s: int
    next_settle_ns: int
    mark_price: str

    def prediction(self) -> Funding:
        return Funding(self.ts_ns, self.rate, self.interval_s, self.next_settle_ns)


def read_funding_events(path: Path | None) -> dict[str, list[FundingEvent]]:
    result: dict[str, list[FundingEvent]] = {}
    if path is None:
        return result
    seen = set()
    with path.open(encoding="utf-8-sig", newline="") as f:
        for r in csv.DictReader(f):
            e = FundingEvent(
                r["inst"],
                r["kind"],
                int(r["ts_ns"]),
                int(r["rate"]),
                int(r["interval_s"] or 0),
                int(r["next_settle_ns"] or 0),
                r["mark_price"],
            )
            identity = (e.inst, e.kind, e.ts_ns)
            if identity in seen or e.kind not in {"prediction", "settlement"} or e.ts_ns <= 0:
                raise ValueError("duplicate or invalid funding event")
            bounded_int(e.rate, -I64, I64, "funding rate")
            if e.kind == "prediction" and not (0 < e.interval_s <= 2**32 - 1 and e.next_settle_ns > e.ts_ns):
                raise ValueError("prediction requires positive interval and future settlement")
            if e.kind == "settlement" and (
                not Decimal(e.mark_price).is_finite() or Decimal(e.mark_price) <= 0
            ):
                raise ValueError("settlement requires its actual positive mark_price")
            seen.add(identity)
            result.setdefault(e.inst, []).append(e)
    return {k: sorted(v, key=lambda e: (e.ts_ns, e.kind)) for k, v in result.items()}


def read_daily_funding(path: Path | None) -> dict[tuple[str, str], float]:
    """The legacy 'raw' CSV contains daily totals, not timestamped settlements."""
    if path is None:
        return {}
    out = {}
    with path.open(encoding="utf-8-sig", newline="") as f:
        # The legacy exporter prepended its boolean success result before the CSV header.
        # Accept only this exact known prefix; malformed headers still fail closed.
        first = f.readline()
        if first.strip() != "true":
            f.seek(0)
        reader = csv.DictReader(f)
        if not {"exchange", "symbol", "day_bps"}.issubset(reader.fieldnames or []):
            raise ValueError("daily funding CSV requires exchange,symbol,day_bps header")
        for r in reader:
            key, value = (r["exchange"], r["symbol"]), float(r["day_bps"])
            if key in out or not np.isfinite(value):
                raise ValueError("duplicate/nonfinite daily funding")
            out[key] = value
    return out


def dump_json(path: Path, value: object) -> None:
    path.write_text(
        json.dumps(value, ensure_ascii=False, indent=2, allow_nan=False, default=lambda x: asdict(x)),
        encoding="utf-8",
    )
