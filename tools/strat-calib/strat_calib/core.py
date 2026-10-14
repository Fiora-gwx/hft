"""Integer kernels pinned to the strategy Rust functions by shared fixtures.

Rates are 1e-9, notionals 1e-6; price/quantity exponents belong to the asset.
The simulated execution and accounting in simulation.py are explicitly a model.
"""

from dataclasses import dataclass
from math import lcm

ONE = 1_000_000_000
YEAR_S = 365 * 86400
I64 = 2**63 - 1
I128 = 2**127 - 1


@dataclass(frozen=True)
class Meta:
    px_exp: int = -6
    qty_exp: int = -8
    tick: int = 1
    lot: int = 1
    min_qty: int = 1
    max_qty: int = 10**16
    min_notional: int = 0
    spot: bool = False


@dataclass(frozen=True)
class Book:
    bid: int
    ask: int
    bid_qty: int
    ask_qty: int
    ts_ns: int

    def valid(self) -> bool:
        return (
            0 < self.bid <= self.ask <= I64
            and 0 < min(self.bid_qty, self.ask_qty) <= max(self.bid_qty, self.ask_qty) <= I64
        )


@dataclass(frozen=True)
class TakerParams:
    enabled: bool = True
    min_edge: int = 200_000
    slip: int = 100_000
    depth_frac: int = 500_000_000
    max_chunk_notional: int = 100_000_000
    max_position_notional: int = 500_000_000
    max_quote_age_ms: int = 5
    cooldown_ms: int = 100
    max_hedge_failures: int = 3


@dataclass(frozen=True)
class CarryParams:
    enabled: bool = True
    enter_apr: int = 100_000_000
    exit_apr: int = 20_000_000
    borrow_apr: int = 0
    fee_horizon_ms: int = 86_400_000
    entry_window_ms: int = 300_000
    min_lead_ms: int = 10_000
    funding_max_age_ms: int = 30_000
    max_notional: int = 100_000_000
    capital_fraction: int = 400_000_000
    max_leverage_x100: int = 300
    max_cross: int = 100_000
    retry_ms: int = 1000


@dataclass(frozen=True)
class Funding:
    observed_ns: int
    rate: int
    interval_s: int
    next_settle_ns: int

    def fresh(self, now: int, max_age_ms: int) -> bool:
        return (
            self.interval_s > 0
            and self.observed_ns > 0
            and 0 <= now - self.observed_ns <= max_age_ms * 1_000_000
            and self.next_settle_ns > now
        )


def ceildiv(n: int, d: int) -> int:
    return -(-n // d)


def pair_lot(a: int, b: int) -> int | None:
    if a <= 0 or b <= 0:
        return None
    value = lcm(a, b)
    return value if value <= I64 else None


def limit(book: Book, slip: int, tick: int, buy: bool) -> int:
    # Side::price_round: Buy floors, Sell ceils, so tick rounding tightens the bound.
    if buy:
        return (book.ask * (ONE + slip) // ONE) // tick * tick
    return ceildiv(ceildiv(book.bid * (ONE - slip), ONE), tick) * tick


def net_edge(buy: int, sell: int, buy_fee: int, sell_fee: int) -> int | None:
    bf, sf = max(0, buy_fee), max(0, sell_fee)
    if min(buy, sell) <= 0 or max(bf, sf) >= ONE:
        return None
    value = (sell * (ONE - sf) - buy * (ONE + bf)) // buy
    return value if -I64 - 1 <= value <= I64 else None


def qty_fraction(n: int, px: int, meta: Meta) -> tuple[int, int] | None:
    if n < 0 or px <= 0:
        return None
    shift = -6 - meta.px_exp - meta.qty_exp
    power = 10 ** abs(shift)
    num, den = (n * power, px) if shift >= 0 else (n, px * power)
    return (num, den) if max(num, den, power) <= I128 else None


def qty_for_notional(n: int, px: int, meta: Meta) -> int | None:
    fraction = qty_fraction(n, px, meta)
    return min(I64, fraction[0] // fraction[1]) if fraction else None


def min_qty_at(px: int, meta: Meta) -> int:
    fraction = qty_fraction(meta.min_notional, px, meta)
    return max(meta.min_qty, ceildiv(*fraction)) if fraction else I64 + 1


@dataclass(frozen=True)
class Signal:
    buy_px: int
    sell_px: int
    qty: int
    edge: int
    reducing: bool


def signal(
    buy: Book,
    sell: Book,
    bm: Meta,
    sm: Meta,
    fees: tuple[int, int],
    positions: tuple[int, int],
    p: TakerParams,
    spot_free: int = 0,
) -> Signal | None:
    if not buy.valid() or not sell.valid():
        return None
    bp, sp = limit(buy, p.slip, bm.tick, True), limit(sell, p.slip, sm.tick, False)
    if bp < buy.ask or sp > sell.bid:
        return None
    edge, lot = net_edge(bp, sp, *fees), pair_lot(bm.lot, sm.lot)
    if edge is None or edge < p.min_edge or lot is None:
        return None
    qty = min(buy.ask_qty, sell.bid_qty) * p.depth_frac // ONE
    if qty > I64:
        return None
    for book, meta, px, pos, side in ((buy, bm, bp, positions[0], 1), (sell, sm, sp, positions[1], -1)):
        chunk = qty_for_notional(p.max_chunk_notional, max(px, book.ask), meta)
        cap = qty_for_notional(p.max_position_notional, max(px, book.ask), meta)
        if chunk is None or cap is None:
            return None
        directional = pos * side
        if directional > I64:
            return None
        room = min(I64, cap + abs(directional)) if directional < 0 else max(0, cap - directional)
        qty = min(qty, chunk, meta.max_qty, room)
        if abs(pos) > cap:
            if directional >= 0:
                return None
            qty = min(qty, abs(directional))
        if meta.spot and side < 0:
            qty = min(qty, max(0, pos), max(0, spot_free))
    reducing = positions[0] < 0 < positions[1]
    if reducing:
        qty = min(qty, abs(positions[0]), positions[1])
    qty = qty // lot * lot
    if qty <= 0 or qty < min_qty_at(buy.ask, bm) or qty < min_qty_at(sp, sm):
        return None
    return Signal(bp, sp, qty, edge, reducing)


def annual(rate: int, interval_s: int) -> int | None:
    return rate * YEAR_S // interval_s if interval_s > 0 else None


def carry_edge(a: Funding, b: Funding | None, fees: int, p: CarryParams, direction: int) -> int | None:
    aa, bb = annual(a.rate, a.interval_s), annual(b.rate, b.interval_s) if b else 0
    if aa is None or bb is None or p.fee_horizon_ms <= 0:
        return None
    cost = ceildiv((max(0, fees) * 2 + p.max_cross * 4) * YEAR_S * 1000, p.fee_horizon_ms)
    value = (aa - bb) * -direction - cost - p.borrow_apr
    return value if -I64 - 1 <= value <= I64 else None


def carry_target(
    books: tuple[Book, Book],
    metas: tuple[Meta, Meta],
    capital: tuple[int, int],
    shares: tuple[int, int],
    leverage: tuple[int, int],
    marks: tuple[int, int],
    p: CarryParams,
) -> int:
    """Port of the private Fast::evaluate sizing; real-strategy fixture checks the result."""
    qty = I64
    for b, m, cash, count, lev, mark in zip(books, metas, capital, shares, leverage, marks, strict=True):
        if not b.valid() or count <= 0 or (not m.spot and not 0 < lev <= p.max_leverage_x100):
            return 0
        cap = min(max(0, cash) * p.capital_fraction // ONE // count, p.max_notional)
        px = min(I64, ceildiv(max(b.ask, mark) * (ONE + p.max_cross), ONE))
        fraction = qty_fraction(cap, px, m)
        q = fraction[0] // fraction[1] if fraction else None
        if q is None or q > I64:
            return 0
        qty = min(qty, q, m.max_qty)
    lot = pair_lot(metas[0].lot, metas[1].lot)
    if lot is None:
        return 0
    qty = qty // lot * lot
    if any(qty < min_qty_at(b.bid, m) for b, m in zip(books, metas, strict=True)):
        return 0
    return qty
