import json
from dataclasses import asdict, replace
from pathlib import Path

import pandas as pd
import pytest

from strat_calib.cli import run
from strat_calib.core import (
    ONE,
    Book,
    CarryParams,
    Funding,
    Meta,
    TakerParams,
    carry_edge,
    carry_target,
    net_edge,
    signal,
)
from strat_calib.data import Config, FundingEvent, pairs, read_daily_funding, read_funding_events, read_symbol
from strat_calib.simulation import carry_daily, carry_simulate, clean_pair, taker_simulate

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = json.loads((Path(__file__).parent / "fixtures/signals.json").read_text())


@pytest.mark.parametrize("c", FIXTURES["taker"], ids=lambda c: c["name"])
def test_shared_rust_signal_sizing(c):
    bm = Meta(-2, -2, 1, 2, 2, 1000000, 1000000)
    sm = replace(bm, **c.get("sell_meta", {}), spot=c.get("spot", False))
    bm = replace(bm, **c.get("buy_meta", {}))
    p = TakerParams(min_edge=1000000, slip=c.get("slip", 0))
    got = signal(
        Book(*c["buy"], 1),
        Book(*c["sell"], 1),
        bm,
        sm,
        (1000000, 1000000),
        tuple(c.get("positions", [0, 0])),
        p,
        c.get("spot_free", 0),
    )
    assert (asdict(got) if got else None) == c["expected"]


@pytest.mark.parametrize("c", FIXTURES["carry"])
def test_shared_rust_funding(c):
    a = Funding(1, *c["a"], 100)
    b = Funding(1, *c["b"], 100) if c["b"] else None
    p = CarryParams(borrow_apr=c["borrow_apr"], max_cross=c["max_cross"], fee_horizon_ms=c["horizon"])
    assert carry_edge(a, b, c["fees"], p, c["direction"]) == c["expected"]


@pytest.mark.parametrize("c", FIXTURES["carry_targets"])
def test_shared_real_rust_carry_target(c):
    b = Book(10000, 10001, 1000000000, 1000000000, 1)
    m = Meta(-2, -8, 1, 1000, 1000, 100000000000, 5000000)
    result = carry_target(
        (b, b),
        (m, replace(m, spot=True)),
        (c["capital"],) * 2,
        (1, 1),
        (c["leverage"], 0),
        (0, 0),
        CarryParams(),
    )
    assert result == c["expected"]


def frame(prices=None, ages=None):
    prices = prices or [(100, 100, 102, 102)] * 6
    ages = ages or [0] * len(prices)
    records = []
    for k, (ba, aa, bb, ab) in enumerate(prices):
        t = (k + 100) * 10**9
        records.append(
            {
                "sec": k + 100,
                "last_ns_a": t,
                "last_ns_b": t + ages[k],
                "bid_a": ba,
                "ask_a": aa,
                "bid_b": bb,
                "ask_b": ab,
                "bid_base_a": 100.0,
                "ask_base_a": 100.0,
                "bid_base_b": 100.0,
                "ask_base_b": 100.0,
            }
        )
    return pd.DataFrame(records)


def test_caps_cooldown_age_and_terminal_cost_prevent_fake_arbitrage_profit():
    m = Meta(-2, -2, 1, 1, 1, 100000, 0)
    p = TakerParams(
        min_edge=100000,
        slip=0,
        cooldown_ms=2000,
        max_chunk_notional=100000000,
        max_position_notional=200000000,
    )
    stats, trades = taker_simulate(frame(ages=[0, 5_000_000, 0, 0, 0, 0]), (m, m), (500000, 500000), p)
    assert [t["ts_ns"] for t in trades] == [100_000_000_000, 102_000_000_000]
    assert stats["age_eligible"] == 5  # equality fails strictly, unlike funding age
    assert stats["final_position_raw"][0] <= 196
    assert stats["spread_cash_quote"] > 0
    assert stats["pnl_after_liquidation_quote"] < 0  # persistent basis is not locked-in profit
    disabled, _ = taker_simulate(frame(), (m, m), (0, 0), replace(p, enabled=False))
    assert disabled["trades"] == 0


def test_reversal_reduces_to_zero_and_keeps_both_legs_equal():
    m = Meta(-2, -2, 1, 1, 1, 100000, 0)
    p = TakerParams(min_edge=100000, slip=0, max_chunk_notional=100000000, max_position_notional=100000000)
    stats, trades = taker_simulate(frame([(100, 100, 102, 102), (102, 102, 100, 100)]), (m, m), (0, 0), p)
    assert len(trades) == 2 and trades[1]["reducing"]
    assert stats["final_position_raw"] == [0, 0]
    assert stats["pnl_after_liquidation_quote"] > 0


def test_negative_fee_not_counted_as_income_and_overflow_is_closed():
    assert net_edge(100, 100, -100, -100) == 0
    assert net_edge(100, 100, ONE, 0) is None
    assert net_edge(0, 100, 0, 0) is None


def test_carry_no_lookahead_settlement_sign_exit_and_stale_hold():
    m = Meta(-2, -2, 1, 1, 1, 100000, 0)
    p = CarryParams(max_cross=0, min_lead_ms=1000, entry_window_ms=10000, funding_max_age_ms=1000)
    data = frame([(100, 100, 100, 100)] * 6)
    # Forecast first becomes visible at t=101; do not open at t=100.
    predictions = [
        FundingEvent("a", "prediction", 101 * 10**9, 1000000, 28800, 103 * 10**9, ""),
        FundingEvent("a", "settlement", 103 * 10**9, 2000000, 0, 0, "100"),
        FundingEvent("a", "prediction", 104 * 10**9, -1000000, 28800, 200 * 10**9, ""),
    ]
    stats, trades, series = carry_simulate(
        data, (m, replace(m, spot=True)), (0, 0), p, (predictions, []), (10000000000,) * 2, (1, 1), (100, 0)
    )
    assert [t["ts_ns"] for t in trades] == [101 * 10**9, 104 * 10**9]
    assert trades[0]["buy_leg"] == 1 and trades[1]["reason"] == "exit"
    assert stats["funding_cash_quote"] == pytest.approx(0.2)
    assert stats["final_position_raw"] == [0, 0]
    assert stats["pnl_after_liquidation_quote"] == pytest.approx(0.2)
    assert series[0]["ts_ns"] == 101 * 10**9


def test_carry_settlement_before_entry_cannot_be_collected():
    m = Meta(-2, -2, 1, 1, 1, 100000, 0)
    e = [
        FundingEvent("a", "settlement", 100 * 10**9, 1000000, 0, 0, "100"),
        FundingEvent("a", "prediction", 100 * 10**9, 1000000, 28800, 120 * 10**9, ""),
    ]
    stats, trades, _ = carry_simulate(
        frame(),
        (m, replace(m, spot=True)),
        (0, 0),
        CarryParams(max_cross=0),
        (e, []),
        (10000000000,) * 2,
        (1, 1),
        (100, 0),
    )
    assert trades and stats["funding_cash_quote"] == 0


def test_carry_depth_slices_keep_target_and_margin_not_levered():
    m = Meta(-2, -2, 1, 1, 1, 100000, 0)
    data = frame([(100, 100, 100, 100)] * 6)
    data["bid_base_a"] = data["ask_base_b"] = 0.2
    e = [FundingEvent("a", "prediction", 100 * 10**9, 1000000, 28800, 120 * 10**9, "")]
    stats, trades, _ = carry_simulate(
        data,
        (m, replace(m, spot=True)),
        (0, 0),
        CarryParams(max_cross=0),
        (e, []),
        (10000000000,) * 2,
        (1, 1),
        (300, 0),
    )
    assert len(trades) == 5
    assert all(t["qty_raw"] == 20 for t in trades)
    assert stats["final_position_raw"] == [-100, 100]


def test_daily_totals_never_create_timestamped_trades():
    a, b = "binance:linear_perpetual:X", "okx:linear_perpetual:X"
    d = carry_daily(
        a, b, {("binance", "X"): 10, ("okx", "X"): 2}, (500000,) * 2, CarryParams(), (1000, 2000), [1, 3, 7]
    )
    assert d["differential_day_bps"] == 8
    assert d["capacity_quote"] == 1000
    assert d["net_apr"] < 0
    assert d["constant_rate_horizon_pnl_per_100_quote"]["7"] > 0
    assert "no dated entries" in d["status"]


def test_scale_and_invalid_quotes_are_not_opportunities():
    d = frame([(100, 100, 10000, 10000)])
    cleaned, quality = clean_pair(d, 1.1)
    assert quality["excluded"] and cleaned.empty
    d = frame([(100, 99, 102, 103)])
    assert clean_pair(d, 1.1)[1]["invalid_rows"] == 1


def test_config_rejects_unknown_fee_missing_contract_and_invalid_params():
    c = json.loads((ROOT / "example.json").read_text())
    with pytest.raises(ValueError, match="unknown"):
        Config({**c, "typo": 1})
    with pytest.raises(ValueError, match="max_position"):
        Config({**c, "taker": {"max_position_notional": 1}})
    with pytest.raises(ValueError, match="fee"):
        Config(c).fee("other:spot:X")
    del c["assumed_metadata"]
    with pytest.raises(ValueError, match="metadata"):
        Config(c).meta("binance:spot:X")


def test_funding_rejects_duplicates_and_future_prediction_contract(tmp_path):
    p = tmp_path / "events.csv"
    header = "inst,kind,ts_ns,rate,interval_s,next_settle_ns,mark_price\n"
    row = "x,prediction,100,1000000,28800,200,\n"
    p.write_text(header + row * 2)
    with pytest.raises(ValueError, match="duplicate"):
        read_funding_events(p)
    p.write_text(header + "x,prediction,100,1000000,0,200,\n")
    with pytest.raises(ValueError, match="interval"):
        read_funding_events(p)


def test_legacy_daily_csv_success_prefix_is_not_a_funding_row(tmp_path):
    p = tmp_path / "daily.csv"
    p.write_text("true\nexchange,symbol,settlements,day_bps,per_period_bps\nbinance,X,4,8,2\n")
    assert read_daily_funding(p) == {("binance", "X"): 8.0}
    p.write_text("false\nexchange,symbol,day_bps\nbinance,X,8\n")
    with pytest.raises(ValueError, match="header"):
        read_daily_funding(p)


def test_cli_parquet_outputs_and_no_future_fill(tmp_path):
    records = []
    for v, px, secs in (("binance", 100, [100, 101]), ("okx", 102, [101, 102])):
        for sec in secs:
            records.append(
                {
                    "exchange": v,
                    "market": "linear_perpetual",
                    "symbol": "X",
                    "canonical_id": "x",
                    "sec": sec,
                    "last_ns": sec * 10**9,
                    "bid": px,
                    "ask": px,
                    "bid_base": 10.0,
                    "ask_base": 10.0,
                }
            )
    parquet = tmp_path / "quotes.parquet"
    pd.DataFrame(records).to_parquet(parquet)
    config = ROOT / "example.json"
    cfg = Config(json.loads(config.read_text()))
    assert len(next(pairs(read_symbol(parquet, "x"), cfg))[2]) == 1
    out = tmp_path / "out"
    result = run(parquet, config, out)
    assert result["pairs"][0]["taker"]["observations"] == 1
    assert result["pairs"][0]["carry"]["status"].startswith("unavailable")
    assert len(result["provenance"]["quotes"]["sha256"]) == 64
    assert (out / "summary.json").is_file() and (out / "trades.csv").is_file()
    assert "正式交易暂不启用" in (out / "report.md").read_text(encoding="utf-8")
    table = []
    for line in (out / "report.md").read_text(encoding="utf-8").splitlines() + [""]:
        if line.startswith("|"):
            table.append(line.count("|"))
        elif table:
            assert len(set(table)) == 1  # instrument separator must not break Markdown columns
            table = []
    records.append(records[0])
    pd.DataFrame(records).to_parquet(parquet)
    with pytest.raises(ValueError, match="duplicate"):
        read_symbol(parquet, "x")
