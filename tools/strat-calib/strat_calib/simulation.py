"""Independent pair scenarios. Cash flow is never labelled locked-in arbitrage PnL."""

from dataclasses import asdict

import numpy as np

from .core import (
    ONE,
    YEAR_S,
    CarryParams,
    Meta,
    TakerParams,
    carry_edge,
    carry_target,
    ceildiv,
    limit,
    min_qty_at,
    pair_lot,
    qty_for_notional,
    signal,
)
from .data import FundingEvent, books, raw


def quantiles(values) -> dict:
    a = np.asarray(values, dtype=float)
    if len(a) == 0:
        return {"min": None, "p05": None, "p50": None, "p95": None, "p99": None, "max": None}
    return dict(
        zip(
            ("min", "p05", "p50", "p95", "p99", "max"),
            map(float, np.quantile(a, [0, 0.05, 0.5, 0.95, 0.99, 1])),
            strict=True,
        )
    )


def quote_value(qty: int, px: int, m: Meta) -> int:
    """Signed micro quote units, rounding credit down / debit away from zero."""
    shift = m.qty_exp + m.px_exp + 6
    value = qty * px
    return value * 10**shift if shift >= 0 else value // 10 ** (-shift)


def cash_delta(qty: int, px: int, m: Meta, fee: int) -> int:
    notional = -quote_value(-abs(qty), px, m)
    # Fees conservatively ceil independently for both legs; no rebate credit.
    return quote_value(-qty, px, m) - ceildiv(notional * max(0, fee), ONE)


def liquidation(positions, b, metas, fees, slip) -> int:
    return sum(
        cash_delta(-q, limit(book, slip, m.tick, q < 0), m, fee)
        for q, book, m, fee in zip(positions, b, metas, fees, strict=True)
        if q
    )


def price_edges(d, metas, fees, slip):
    """Float64 display statistics only. Every selected order is rechecked with integer signal()."""
    limits = []
    for s, m in zip(("a", "b"), metas, strict=True):
        scale = 10.0**m.px_exp
        ask = np.ceil(d[f"ask_{s}"].to_numpy() / scale)
        bid = np.floor(d[f"bid_{s}"].to_numpy() / scale)
        buy = np.floor(np.floor(ask * (1 + slip / ONE)) / m.tick) * m.tick * scale
        sell = np.ceil(np.ceil(bid * (1 - slip / ONE)) / m.tick) * m.tick * scale
        limits.append((buy, sell))
    edges = []
    for i, j in ((0, 1), (1, 0)):
        bp, sp = limits[i][0], limits[j][1]
        edge = (sp * (1 - max(0, fees[j]) / ONE) / bp - 1 - max(0, fees[i]) / ONE) * 10000
        # A tightened tick limit that cannot reach the touch is not executable.
        si, sj = ("a", "b")[i], ("a", "b")[j]
        edge = np.where((bp >= d[f"ask_{si}"]) & (sp <= d[f"bid_{sj}"]), edge, -1e4)
        edges.append(edge)
    return edges


def clean_pair(d, max_price_ratio):
    valid = np.ones(len(d), dtype=bool)
    for s in ("a", "b"):
        for name in ("bid", "ask", "bid_base", "ask_base"):
            col = d[f"{name}_{s}"]
            valid &= np.isfinite(col) & (col > 0)
        valid &= d[f"ask_{s}"] >= d[f"bid_{s}"]
    good = d.loc[valid].copy()
    if len(good) == 0:
        return good, {"invalid_rows": int((~valid).sum()), "excluded": "no valid overlapping BBO"}
    ratio = ((good.bid_a + good.ask_a) / (good.bid_b + good.ask_b)).median()
    excluded = None if 1 / max_price_ratio <= ratio <= max_price_ratio else "price scale / contract mismatch"
    # Reject isolated implausible spreads as well, count them, never turn bad data into profit.
    row_ratio = (good.bid_a + good.ask_a) / (good.bid_b + good.ask_b)
    comparable = row_ratio.between(1 / max_price_ratio, max_price_ratio)
    if not comparable.any():
        excluded = excluded or "no comparable BBO after scale filtering"
    return good.loc[comparable], {
        "invalid_rows": int((~valid).sum()),
        "excluded": excluded,
        "median_price_ratio": float(ratio),
        "scale_outlier_rows": int((~comparable).sum()),
    }


def taker_simulate(d, metas, fees, p: TakerParams, positions=(0, 0), thresholds=(1, 2, 3, 5, 10)):
    ab, ba = price_edges(d, metas, fees, p.slip)
    best = np.maximum(ab, ba)
    age_ok = np.abs(d.last_ns_a.to_numpy() - d.last_ns_b.to_numpy()) < p.max_quote_age_ms * 1_000_000
    caps = np.maximum(
        np.minimum(d.ask_base_a, d.bid_base_b) * np.minimum(d.ask_a, d.bid_b),
        np.minimum(d.ask_base_b, d.bid_base_a) * np.minimum(d.ask_b, d.bid_a),
    )
    out = {
        "observations": len(d),
        "age_eligible": int(age_ok.sum()),
        "net_edge_bps": quantiles(best),
        "buy0_edge_bps": quantiles(ab),
        "buy1_edge_bps": quantiles(ba),
        "top_capacity_quote": quantiles(caps),
        "threshold_counts": {str(t): int((best > t).sum()) for t in thresholds},
        "above_threshold": int((best > p.min_edge / 100000).sum()),
        "at_or_above_threshold": int((best >= p.min_edge / 100000).sum()),
        "parameters": asdict(p),
    }
    candidates = age_ok & (best >= p.min_edge / 100000 - 1e-6)
    pos, cash, last_ns, trades = list(positions), 0, None, []
    first = next(books(d.iloc[:1], metas))
    initial_value = sum(
        quote_value(q, (b.bid + b.ask) // 2, m) for q, b, m in zip(pos, first, metas, strict=True)
    )
    for pair in books(d.loc[candidates], metas):
        now = max(b.ts_ns for b in pair)
        if not p.enabled or (last_ns is not None and now - last_ns < p.cooldown_ms * 1_000_000):
            continue
        signals = []
        for i, j in ((0, 1), (1, 0)):
            s = signal(
                pair[i],
                pair[j],
                metas[i],
                metas[j],
                (fees[i], fees[j]),
                (pos[i], pos[j]),
                p,
                spot_free=max(0, pos[j]),
            )
            if s:
                signals.append((i, j, s))
        if not signals:
            continue
        i, j, s = max(signals, key=lambda x: (x[2].edge, -x[0]))
        change = cash_delta(s.qty, s.buy_px, metas[i], fees[i]) + cash_delta(
            -s.qty, s.sell_px, metas[j], fees[j]
        )
        cash += change
        pos[i] += s.qty
        pos[j] -= s.qty
        last_ns = now
        trades.append(
            {
                "ts_ns": now,
                "buy_leg": i,
                "qty_raw": s.qty,
                "buy_px_raw": s.buy_px,
                "sell_px_raw": s.sell_px,
                "edge_bps": s.edge / 100000,
                "reducing": s.reducing,
                "cash_delta_quote": change / 1e6,
                "position0_raw": pos[0],
            }
        )
    final = next(books(d.iloc[-1:], metas))
    exit_value = liquidation(pos, final, metas, fees, p.slip)
    out.update(
        {
            "trades": len(trades),
            "spread_cash_quote": cash / 1e6,
            "terminal_liquidation_quote": exit_value / 1e6,
            "pnl_after_liquidation_quote": (cash + exit_value - initial_value) / 1e6,
            "final_position_raw": pos,
            "initial_position_raw": positions,
            "funding_included": False,
            "fill_model": "both IOC legs fill at fixed adverse limits",
        }
    )
    return out, trades


def carry_daily(a, b, daily, fees, p: CarryParams, capacity, horizons):
    def rate(inst):
        venue, market, symbol = inst.split(":")
        return 0.0 if market == "spot" else daily.get((venue, symbol))

    ra, rb = rate(a), rate(b)
    if ra is None or rb is None or a.split(":")[1] == "spot":
        return {"status": "missing funding or unsupported spot/spot"}
    difference = ra - rb
    direction = -1 if b.split(":")[1] == "spot" or difference >= 0 else 1
    income = difference * -direction
    round_trip_bps = (2 * sum(max(0, f) for f in fees) + 4 * p.max_cross) / 100000
    net = (
        income * 365 / 10000 - round_trip_bps / 10000 * YEAR_S * 1000 / p.fee_horizon_ms - p.borrow_apr / ONE
    )
    return {
        "status": "daily aggregate screening ONLY; no dated entries or realised funding PnL",
        "day_bps_a": ra,
        "day_bps_b": rb,
        "differential_day_bps": difference,
        "short_leg": 0 if direction < 0 else 1,
        "income_day_bps": income,
        "net_apr": net,
        "round_trip_cost_bps": round_trip_bps,
        "breakeven_days": round_trip_bps / income if income > 0 else None,
        "capacity_quote": capacity[0 if direction < 0 else 1],
        "constant_rate_horizon_pnl_per_100_quote": {
            str(h): 100 * ((income * h - round_trip_bps) / 10000 - p.borrow_apr / ONE * h / 365)
            for h in horizons
        },
        "annualisation_denominator": "one-leg notional; fully funded two-wallet return roughly half",
    }


def carry_simulate(
    d,
    metas,
    fees,
    p: CarryParams,
    funding: tuple[list[FundingEvent], list[FundingEvent]],
    capital,
    shares,
    leverage,
):
    """Observed forecasts drive targets; only explicit settlements credit funding cash.

    Decisions/slices occur on retained paired BBO observations, not unobserved timer ticks.
    Marks used for sizing are BBO ask proxies, clearly labelled in the report.
    """
    if metas[0].spot or not funding[0] or (not metas[1].spot and not funding[1]):
        return {"status": "unavailable: timestamped predictions/settlements required"}, [], []
    if not any(e.kind == "prediction" for e in funding[0]):
        return {"status": "unavailable: no timestamped predictions"}, [], []
    events = sorted(((e.ts_ns, i, e) for i, es in enumerate(funding) for e in es), key=lambda x: (x[0], x[1]))
    index, forecasts, pos, target, cash = 0, [None, None], [0, 0], 0, 0
    funding_cash, borrow, last_attempt, previous_time, previous_books = 0, 0, None, None, None
    trades, series, settlements = [], [], 0
    lot = pair_lot(metas[0].lot, metas[1].lot)
    if lot is None:
        raise ValueError("invalid common lot")
    for pair in books(d, metas):
        now = max(x.ts_ns for x in pair)
        changed = False
        if previous_time is not None and pos[0]:
            n = max(quote_value(abs(q), b.ask, m) for q, b, m in zip(pos, previous_books, metas, strict=True))
            borrow += ceildiv(n * p.borrow_apr * (now - previous_time), ONE * YEAR_S * 10**9)
        while index < len(events) and events[index][0] <= now:
            _, i, e = events[index]
            if e.kind == "prediction":
                forecasts[i] = e.prediction()
                changed = True
            else:
                # An order at the settlement timestamp is too late to earn that settlement.
                mark = raw(e.mark_price, metas[i].px_exp)
                funding_cash += (-quote_value(pos[i], mark, metas[i]) * e.rate) // ONE
                settlements += 1
            index += 1
        previous_time, previous_books = now, pair
        fa, fb = forecasts[0], None if metas[1].spot else forecasts[1]
        fresh = fa is not None and fa.fresh(now, p.funding_max_age_ms)
        fresh = fresh and (metas[1].spot or (fb is not None and fb.fresh(now, p.funding_max_age_ms)))
        edge = None
        direction = 1 if pos[0] > 0 else -1
        if fresh:
            fees_sum = sum(max(0, f) for f in fees)
            short0 = carry_edge(fa, fb, fees_sum, p, -1)
            short1 = carry_edge(fa, fb, fees_sum, p, 1)
            if not pos[0]:
                # Rust Option ordering: None sorts below Some; both missing tie at short leg 0.
                direction = (
                    -1
                    if metas[1].spot or (short0 is not None, short0 or 0) >= (short1 is not None, short1 or 0)
                    else 1
                )
            edge = short0 if direction < 0 else short1
            if edge is None:
                edge = -(2**63)
        if changed:
            series.append(
                {
                    "ts_ns": now,
                    "rate0": fa.rate if fa else None,
                    "rate1": fb.rate if fb else None,
                    "net_apr": edge / ONE if edge is not None else None,
                    "fresh": bool(fresh),
                }
            )
        desired = target
        if (
            not p.enabled
            or (fresh and edge is not None and edge < p.exit_apr)
            or target
            and not pos[0]
            and not fresh
        ):
            desired = 0
        elif not pos[0] and not target and fresh and edge is not None and edge >= p.enter_apr:
            next_settle = fa.next_settle_ns if direction < 0 else fb.next_settle_ns
            if p.min_lead_ms * 1_000_000 <= next_settle - now <= p.entry_window_ms * 1_000_000:
                desired = direction * carry_target(
                    pair, metas, capital, shares, leverage, (pair[0].ask, pair[1].ask), p
                )
        retry_ok = last_attempt is None or now - last_attempt >= p.retry_ms * 1_000_000
        if not retry_ok:
            continue
        target = desired
        remaining = target - pos[0]
        if not remaining:
            continue
        buy, sell = (0, 1) if remaining > 0 else (1, 0)
        bp = limit(pair[buy], p.max_cross, metas[buy].tick, True)
        sp = limit(pair[sell], p.max_cross, metas[sell].tick, False)
        q = min(abs(remaining), pair[buy].ask_qty, pair[sell].bid_qty)
        for i, px in ((buy, bp), (sell, sp)):
            cap = qty_for_notional(p.max_notional, max(px, pair[i].ask), metas[i])
            q = min(q, cap or 0, metas[i].max_qty)
        q = q // lot * lot
        if (
            q <= 0
            or bp < pair[buy].ask
            or sp > pair[sell].bid
            or q < min_qty_at(pair[buy].ask, metas[buy])
            or q < min_qty_at(sp, metas[sell])
        ):
            continue
        delta = cash_delta(q, bp, metas[buy], fees[buy]) + cash_delta(-q, sp, metas[sell], fees[sell])
        pos[buy] += q
        pos[sell] -= q
        cash += delta
        last_attempt = now
        trades.append(
            {
                "ts_ns": now,
                "buy_leg": buy,
                "qty_raw": q,
                "buy_px_raw": bp,
                "sell_px_raw": sp,
                "target0_raw": target,
                "position0_raw": pos[0],
                "reason": "exit" if target == 0 else "entry",
                "net_apr": edge / ONE if edge is not None else None,
            }
        )
    final = next(books(d.iloc[-1:], metas))
    terminal = liquidation(pos, final, metas, fees, p.max_cross)
    return (
        {
            "status": "sampled forecast-driven scenario",
            "parameters": asdict(p),
            "trades": len(trades),
            "final_position_raw": pos,
            "trade_cash_quote": cash / 1e6,
            "funding_cash_quote": funding_cash / 1e6,
            "settlement_records": settlements,
            "borrow_cost_quote": borrow / 1e6,
            "terminal_liquidation_quote": terminal / 1e6,
            "pnl_after_liquidation_quote": (cash + funding_cash - borrow + terminal) / 1e6,
            "funding_complete": False,
            "marks_for_sizing": "BBO ask proxy; account/margin static",
        },
        trades,
        series,
    )
