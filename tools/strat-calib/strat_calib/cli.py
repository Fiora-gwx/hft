"""Local files in, reproducible research outputs out. No credentials or network access."""

import argparse
import csv
import json
import sys
from dataclasses import asdict
from datetime import UTC, datetime
from pathlib import Path

import numpy as np

from .core import ONE
from .data import (
    Config,
    dump_json,
    inventory,
    pairs,
    read_daily_funding,
    read_funding_events,
    read_symbol,
    sha256,
)
from .report import render
from .simulation import carry_daily, carry_simulate, clean_pair, price_edges, quantiles, taker_simulate


def write_csv(path, rows):
    keys = sorted({k for r in rows for k in r})
    with path.open("w", encoding="utf-8", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=keys or ["pair", "strategy", "ts_ns"])
        writer.writeheader()
        writer.writerows(rows)


def run(quotes: Path, config: Path, output: Path, daily_path=None, events_path=None, report_path=None):
    cfg = Config(json.loads(config.read_text(encoding="utf-8-sig")))
    daily, events = read_daily_funding(daily_path), read_funding_events(events_path)
    symbols, venues = inventory(quotes)
    selected = cfg.content.get("symbols", symbols)
    if not selected or set(selected) - set(symbols):
        raise ValueError("symbols is empty or contains symbols absent from parquet")
    records, all_trades, all_series, start, end = [], [], [], None, None
    for symbol in sorted(set(selected)):
        print(f"calibrating {symbol}", file=sys.stderr, flush=True)
        frame = read_symbol(quotes, symbol)
        low, high = int(frame.sec.min()), int(frame.sec.max())
        start, end = min(start or low, low), max(end or high, high)
        for a, b, paired in pairs(frame, cfg):
            name = a + "|" + b
            record = {"symbol": symbol, "pair": name, "overlap_rows": len(paired)}
            records.append(record)
            if paired.empty:
                record["excluded"] = "no overlapping seconds"
                continue
            ma, aa = cfg.meta(a)
            mb, ab = cfg.meta(b)
            if (ma.px_exp, ma.qty_exp) != (mb.px_exp, mb.qty_exp):
                raise ValueError(f"{name}: common asset exponents required")
            metas, fees = (ma, mb), (cfg.fee(a), cfg.fee(b))
            data, quality = clean_pair(paired, cfg.max_price_ratio)
            record.update(quality)
            if quality["excluded"] or data.empty:
                continue
            record.update(
                {
                    "metadata_assumed": aa or ab,
                    "metadata": [asdict(ma), asdict(mb)],
                    "fee_bps": [f / 100000 for f in fees],
                    "coverage_pct": len(data) / (high - low + 1) * 100,
                }
            )
            taker, trades = taker_simulate(
                data, metas, fees, cfg.taker, (cfg.position(a, ma), cfg.position(b, mb)), cfg.thresholds
            )
            record["taker"] = taker
            all_trades.extend({"pair": name, "strategy": "cross-taker", **t} for t in trades)
            # Equal fee per leg scenarios are sensitivity cases, never claimed as actual fee tiers.
            record["equal_fee_sensitivity"] = {}
            for fee in cfg.fee_scenarios:
                rates = (int(float(fee) * 100000),) * 2
                edges = price_edges(data, metas, rates, cfg.taker.slip)[1]
                record["equal_fee_sensitivity"][str(fee)] = {
                    "net_edge_bps": quantiles(edges),
                    "above_threshold": int((edges > cfg.taker.min_edge / 100000).sum()),
                }
            record["carry_daily"] = carry_daily(
                a,
                b,
                daily,
                fees,
                cfg.carry,
                (
                    quantiles(
                        np.minimum(data.bid_base_a, data.ask_base_b) * np.minimum(data.bid_a, data.ask_b)
                    )["p50"],
                    quantiles(
                        np.minimum(data.ask_base_a, data.bid_base_b) * np.minimum(data.ask_a, data.bid_b)
                    )["p50"],
                ),
                cfg.horizons,
            )
            carry, trades, series = carry_simulate(
                data,
                metas,
                fees,
                cfg.carry,
                (events.get(a, []), events.get(b, [])),
                (cfg.capital(a), cfg.capital(b)),
                (1, 1),
                (cfg.leverage(a), cfg.leverage(b)),
            )
            record["carry"] = carry
            all_trades.extend({"pair": name, "strategy": "portfolio-carry", **t} for t in trades)
            all_series.extend({"pair": name, **s} for s in series)
    if not records:
        raise ValueError("no eligible cross-exchange instrument pairs")
    source_dir = Path(__file__).parent
    root = source_dir.parents[2]
    strategy_sources = [
        "strategies/cross-taker/src/fast/signal.rs",
        "strategies/cross-taker/src/fast/mod.rs",
        "strategies/portfolio-carry/src/fast/mod.rs",
        "tools/strat-calib/tests/fixtures/signals.json",
    ]
    provenance = {
        "quotes": {"path": str(quotes.resolve()), "sha256": sha256(quotes)},
        "config": cfg.content,
        "config_sha256": sha256(config),
        "venues": venues,
        "start_utc": datetime.fromtimestamp(start, UTC).isoformat(),
        "end_utc": datetime.fromtimestamp(end, UTC).isoformat(),
        "source_sha256": {p.name: sha256(p) for p in sorted(source_dir.glob("*.py"))},
        "strategy_source_sha256": {p: sha256(root / p) for p in strategy_sources if (root / p).is_file()},
        "daily_funding": {"path": str(daily_path), "sha256": sha256(daily_path)} if daily_path else None,
        "funding_events": {"path": str(events_path), "sha256": sha256(events_path)} if events_path else None,
        "assumptions": [
            "independent pair wallets, shares=1; not an additive portfolio",
            "both IOC legs fill at limits; no rejection/latency model",
            "quote-currency fee model; spot base-fee dust not modelled",
            "terminal liquidation ignores depth; unknown connection health",
            "funding aggregates are screening only, never forecasts",
        ],
        "rate_scale": ONE,
    }
    result = {"schema_version": 1, "provenance": provenance, "pairs": records}
    output.mkdir(parents=True, exist_ok=True)
    dump_json(output / "summary.json", result)
    write_csv(output / "trades.csv", all_trades)
    write_csv(output / "funding_series.csv", all_series)
    target = report_path or output / "report.md"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(render(result), encoding="utf-8")
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--quotes", type=Path, required=True)
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--funding-daily", type=Path, help="legacy daily CSV, screening only")
    parser.add_argument("--funding-events", type=Path, help="timestamped predictions AND settlements")
    parser.add_argument("--report", type=Path, help="optional separate Chinese Markdown report path")
    args = parser.parse_args(argv)
    try:
        run(args.quotes, args.config, args.output, args.funding_daily, args.funding_events, args.report)
    except (ValueError, KeyError, TypeError, OSError) as exc:
        parser.exit(2, f"strat-calib: {exc}\n")


if __name__ == "__main__":
    main()
