//! `SpreadCalc` (README "SpreadCalc 与 CostLine"): two-leg spreads with the slow-side price as the
//! denominator, HalfEven, through `Px::ratio`.

use hft_types::{Px, Rate};

/// Two-leg spread functions (stateless).
#[derive(Copy, Clone, Debug, Default)]
pub struct SpreadCalc;

impl SpreadCalc {
    /// Opening spread: sell fast at its bid, buy slow at `buy_px_slow`:
    /// `(bid_fast − buy_px_slow) / buy_px_slow`. `buy_px_slow ≤ 0` → `Rate(0)`; saturates.
    #[inline]
    pub fn buy(bid_fast: Px, buy_px_slow: Px) -> Rate {
        bid_fast.ratio(buy_px_slow)
    }

    /// Closing spread: buy fast back at its ask, sell slow at `sell_px_slow`:
    /// `(ask_fast − sell_px_slow) / sell_px_slow`.
    #[inline]
    pub fn sell(ask_fast: Px, sell_px_slow: Px) -> Rate {
        ask_fast.ratio(sell_px_slow)
    }

    /// `spread − cost`, saturating.
    #[inline]
    pub fn net(spread: Rate, cost: Rate) -> Rate {
        Rate(spread.0.saturating_sub(cost.0))
    }
}
