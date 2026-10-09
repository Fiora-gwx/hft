//! `ImpactModel::vwap` (README "ImpactModel::vwap"): VWAP and impact of a taker quantity from the
//! L2 view, at most 64 levels copied to the stack.

use hft_strategy_sdk::prelude::Level;
use hft_strategy_sdk::{BookRead, BookStatus};
use hft_types::{Px, Qty, Rate, Side};

/// Most levels walked (and copied to the stack).
pub const MAX_LEVELS: usize = 64;

/// VWAP parameters: the top level alone suffices when `top_qty × book_frac ≥ qty`
/// (`book_frac ≤ 0` skips that step); at most `max_levels` (≤ 64) levels are walked.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct VwapParams {
    pub book_frac: Rate,
    pub max_levels: u8,
}

/// Result of [`ImpactModel::vwap`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Vwap {
    /// The top level covers `qty / book_frac`: its price.
    Top(Px),
    /// `vwap` rounded to the worse side for the taker (buying Ceil, selling Floor), `levels` used,
    /// `impact = |vwap.ratio(top)|`.
    Walked { vwap: Px, levels: u8, impact: Rate },
    /// Not enough quantity within the walked levels; `avail` = their total.
    Insufficient { avail: Qty },
}

/// L2 VWAP (stateless).
#[derive(Clone, Copy, Debug, Default)]
pub struct ImpactModel;

const RATE_ONE: i128 = 1_000_000_000;

impl ImpactModel {
    /// Taker of `qty` on `side` (`Buy` eats the asks, `Sell` eats the bids). Book not `Live` or that
    /// side empty → `Insufficient{avail: 0}` (BBO-only insts have an `Uninit` view; a TopN book has
    /// only N levels). `qty ≤ 0` walks to `Walked{vwap: top, levels: 1, impact: 0}` unless the top
    /// step already returns `Top`.
    pub fn vwap<B: BookRead>(book: &B, side: Side, qty: Qty, p: &VwapParams) -> Vwap {
        const NONE: Vwap = Vwap::Insufficient { avail: Qty(0) };
        if book.status() != BookStatus::Live {
            return NONE;
        }
        let eat = match side {
            Side::Buy => Side::Sell,
            Side::Sell => Side::Buy,
        };
        let Some(top) = book.best(eat) else {
            return NONE;
        };
        let q = i128::from(qty.0);
        if p.book_frac.0 > 0
            && i128::from(top.qty.0).saturating_mul(i128::from(p.book_frac.0)) >= q.saturating_mul(RATE_ONE)
        {
            return Vwap::Top(top.px);
        }
        if q <= 0 {
            return Vwap::Walked {
                vwap: top.px,
                levels: 1,
                impact: Rate(0),
            };
        }
        let max = usize::from(p.max_levels).min(MAX_LEVELS);
        if max == 0 {
            return NONE;
        }
        let mut buf = [Level {
            px: Px(0),
            qty: Qty(0),
        }; MAX_LEVELS];
        let out = buf.get_mut(..max).unwrap_or(&mut []);
        let n = book.levels(eat, out).min(out.len());
        let mut rem = q;
        let mut sum = 0i128;
        let mut avail = 0i128;
        for (i, l) in out.iter().take(n).enumerate() {
            let lq = i128::from(l.qty.0.max(0));
            avail = avail.saturating_add(lq);
            let take = lq.min(rem);
            sum = sum.saturating_add(i128::from(l.px.0).saturating_mul(take));
            rem -= take;
            if rem == 0 {
                let mode = match side {
                    Side::Buy => hft_types::RoundMode::Ceil,
                    Side::Sell => hft_types::RoundMode::Floor,
                };
                let v = div_round(sum, q, mode);
                let vwap = Px(v.clamp(-i128::from(i64::MAX), i128::from(i64::MAX)) as i64);
                let impact = Rate(vwap.ratio(top.px).0.saturating_abs());
                return Vwap::Walked {
                    vwap,
                    levels: (i + 1) as u8,
                    impact,
                };
            }
        }
        Vwap::Insufficient {
            avail: Qty(avail.min(i128::from(i64::MAX)) as i64),
        }
    }
}

/// `n / d` (d > 0) rounded towards −∞ (Floor) or +∞ (Ceil).
fn div_round(n: i128, d: i128, m: hft_types::RoundMode) -> i128 {
    if d <= 0 {
        return 0;
    }
    let q = n.div_euclid(d);
    if m == hft_types::RoundMode::Ceil && n.rem_euclid(d) != 0 {
        q.saturating_add(1)
    } else {
        q
    }
}
