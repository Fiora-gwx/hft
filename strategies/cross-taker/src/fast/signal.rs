//! Checked integer signal and sizing. Invalid/overflowing inputs produce no order.
use crate::params::XtFields;
use hft_strategy_sdk::prelude::*;

const ONE: i128 = 1_000_000_000;

/// Net proceeds less buy cost, relative to the buy limit; adverse fee rounding, no rebate credit.
pub fn net_edge(buy: Px, sell: Px, buy_fee: Rate, sell_fee: Rate) -> Option<Rate> {
    if buy.0 <= 0 || sell.0 <= 0 {
        return None;
    }
    let bf = i128::from(buy_fee.0.max(0));
    let sf = i128::from(sell_fee.0.max(0));
    if bf >= ONE || sf >= ONE {
        return None;
    }
    let profit = i128::from(sell.0) * (ONE - sf) - i128::from(buy.0) * (ONE + bf);
    i64::try_from(profit.div_euclid(i128::from(buy.0))).ok().map(Rate)
}

/// Common lot for a 1:1 pair (LCM); no partial lot is rounded upward.
pub fn pair_lot(a: i64, b: i64) -> Option<i64> {
    if a <= 0 || b <= 0 {
        return None;
    }
    let (mut x, mut y) = (a, b);
    while y != 0 {
        (x, y) = (y, x % y);
    }
    a.checked_div(x)?.checked_mul(b)
}

/// Conservative maximum raw quantity within a quote notional cap.
pub fn qty_for_notional(n: Notional, px: Px, h: &InstHot) -> Option<i64> {
    let (num, den) = qty_fraction(n, px, h)?;
    Some(i64::try_from(num.checked_div(den)?).unwrap_or(i64::MAX))
}

fn qty_fraction(n: Notional, px: Px, h: &InstHot) -> Option<(i128, i128)> {
    if n.0 < 0 || px.0 <= 0 {
        return None;
    }
    let shift = -6 - i32::from(h.px_exp) - i32::from(h.qty_exp);
    let scale = 10_i128.checked_pow(shift.unsigned_abs())?;
    let (num, den) = if shift >= 0 {
        (i128::from(n.0).checked_mul(scale)?, i128::from(px.0))
    } else {
        (i128::from(n.0), i128::from(px.0).checked_mul(scale)?)
    };
    Some((num, den))
}

fn min_qty_at(px: Px, h: &InstHot) -> Option<i64> {
    let (num, den) = qty_fraction(h.min_notional, px, h)?;
    let ceil = num.checked_div(den)?.checked_add(i128::from(num % den != 0))?;
    Some(h.min_qty.0.max(i64::try_from(ceil).ok()?))
}

pub fn limit(side: Side, b: BboView, slip: Rate, tick: i64) -> Px {
    let (touch, rate) = match side {
        Side::Buy => (b.ask_px, slip),
        Side::Sell => (b.bid_px, Rate(-slip.0)),
    };
    touch
        .mul_rate(rate, side.price_round())
        .round_to_tick(tick, side.price_round())
}

/// One direction of a pair. Position is actual signed quantity including assigned spot inventory.
#[derive(Clone, Copy, Debug)]
pub struct LegInput {
    pub book: BboView,
    pub hot: InstHot,
    pub fee: Rate,
    pub position: Qty,
    pub spot: bool,
    pub spot_free: Qty,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Signal {
    pub buy_px: Px,
    pub qty: Qty,
    pub edge: Rate,
    pub reducing: bool,
}

pub fn signal(buy: LegInput, sell: LegInput, f: &XtFields) -> Option<Signal> {
    let valid = |b: BboView| b.bid_px.0 > 0 && b.ask_px >= b.bid_px && b.bid_qty.0 > 0 && b.ask_qty.0 > 0;
    if !valid(buy.book) || !valid(sell.book) {
        return None;
    }
    let bp = limit(Side::Buy, buy.book, f.slip, buy.hot.tick);
    let sp = limit(Side::Sell, sell.book, f.slip, sell.hot.tick);
    if bp < buy.book.ask_px || sp > sell.book.bid_px {
        return None;
    }
    let edge = net_edge(bp, sp, buy.fee, sell.fee)?;
    if edge < f.min_edge {
        return None;
    }
    let lot = pair_lot(buy.hot.lot, sell.hot.lot)?;
    let depth = i128::from(buy.book.ask_qty.0.min(sell.book.bid_qty.0))
        .checked_mul(i128::from(f.depth_frac.0))?
        .checked_div(ONE)?;
    let mut qty = i64::try_from(depth).ok()?;
    for (leg, px, side) in [(buy, bp, Side::Buy), (sell, sp, Side::Sell)] {
        qty = qty
            .min(qty_for_notional(
                f.max_chunk_notional,
                px.max(leg.book.ask_px),
                &leg.hot,
            )?)
            .min(leg.hot.max_qty.0);
        let cap = qty_for_notional(f.max_position_notional, px.max(leg.book.ask_px), &leg.hot)?;
        let directional = if side == Side::Buy {
            leg.position.0
        } else {
            leg.position.0.checked_neg()?
        };
        let room = if directional < 0 {
            cap.saturating_add(directional.saturating_abs())
        } else {
            cap.saturating_sub(directional)
        };
        qty = qty.min(room);
        // Above-cap positions may only move toward zero; never cross into a new position.
        if leg.position.0.unsigned_abs() > cap as u64 {
            if directional >= 0 {
                return None;
            }
            qty = qty.min(directional.saturating_abs());
        }
        if leg.spot && side == Side::Sell {
            qty = qty.min(leg.position.0.max(0)).min(leg.spot_free.0.max(0));
        }
    }
    // Closing pairs stop at zero on either leg, allowing Reduce/Halt to remain meaningful.
    let reducing = buy.position.0 < 0 && sell.position.0 > 0;
    if reducing {
        qty = qty.min(buy.position.0.saturating_abs()).min(sell.position.0);
    }
    qty = qty.checked_div(lot)?.checked_mul(lot)?;
    if qty <= 0
        || qty < min_qty_at(buy.book.ask_px, &buy.hot)?
        || qty < min_qty_at(sell.book.bid_px, &sell.hot)?
    {
        return None;
    }
    Some(Signal {
        buy_px: bp,
        qty: Qty(qty),
        edge,
        reducing,
    })
}
