//! `TriggerPx` (README "TriggerPx(预计算触发价)"): trigger prices computed when the slow side
//! changes, so a fast-side tick is one integer comparison.

use hft_strategy_sdk::{RestoreError, SnapshotReader, SnapshotWriter};
use hft_types::{Px, Rate, RoundMode};

/// Precomputed trigger prices of the fast leg. `Default` = invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct TriggerPx {
    /// Fast bid at or above which the opening spread reaches `θ_open`.
    pub buy_bid_fast: Px,
    /// Fast ask at or below which the closing spread is at most `θ_close`.
    pub sell_ask_fast: Px,
    pub valid: bool,
    /// engine_ns of the slow-side update the prices were computed from.
    pub slow_ens: i64,
}

impl Default for TriggerPx {
    fn default() -> Self {
        TriggerPx {
            buy_bid_fast: Px(0),
            sell_ask_fast: Px(0),
            valid: false,
            slow_ens: 0,
        }
    }
}

impl TriggerPx {
    /// Slow-side update (other leg's quote, external quote, parameters or advice changing θ; θ
    /// already includes the cost line and the strategy threshold):
    /// `buy_bid_fast = ceil_to_tick(buy_px_slow × (1 + θ_open))`,
    /// `sell_ask_fast = floor_to_tick(sell_px_slow × (1 + θ_close))` — the exact 1e-9 product
    /// rounded once to the fast tick, always towards the harder-to-trigger side; beyond `MAX_RAW`
    /// the nearest in-range tick multiple. Non-positive slow prices or tick → `invalidate()`.
    pub fn update(
        &mut self,
        buy_px_slow: Px,
        sell_px_slow: Px,
        theta_open: Rate,
        theta_close: Rate,
        tick_fast: Px,
        now_ens: i64,
    ) {
        if buy_px_slow.0 <= 0 || sell_px_slow.0 <= 0 || tick_fast.0 <= 0 {
            self.invalidate();
            return;
        }
        // Ceil (resp. Floor) to the raw step and then to the tick equals one rounding of the
        // exact product to the tick (nested ceil/floor over integer steps).
        self.buy_bid_fast = buy_px_slow
            .mul_rate(theta_open, RoundMode::Ceil)
            .round_to_tick(tick_fast.0, RoundMode::Ceil);
        self.sell_ask_fast = sell_px_slow
            .mul_rate(theta_close, RoundMode::Floor)
            .round_to_tick(tick_fast.0, RoundMode::Floor);
        self.valid = true;
        self.slow_ens = now_ens;
    }

    /// Slow side unusable (dirty/resync): every `hit_*` is false until the next `update`.
    #[inline]
    pub fn invalidate(&mut self) {
        self.valid = false;
    }

    /// `valid && bid_fast >= buy_bid_fast`.
    #[inline]
    pub fn hit_buy(&self, bid_fast: Px) -> bool {
        self.valid && bid_fast.0 >= self.buy_bid_fast.0
    }

    /// `valid && ask_fast <= sell_ask_fast`.
    #[inline]
    pub fn hit_sell(&self, ask_fast: Px) -> bool {
        self.valid && ask_fast.0 <= self.sell_ask_fast.0
    }

    /// 25 B: `buy_bid_fast` i64, `sell_ask_fast` i64, `valid` u8, `slow_ens` i64.
    pub fn snapshot(&self, w: &mut SnapshotWriter) {
        w.put_i64(self.buy_bid_fast.0);
        w.put_i64(self.sell_ask_fast.0);
        w.put_u8(u8::from(self.valid));
        w.put_i64(self.slow_ens);
    }

    /// Reads the fields back and leaves the prices invalid until the next slow-side update (they
    /// were computed from quotes before the restart). On error `self` is unchanged.
    pub fn restore(&mut self, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        let buy = r.get_i64()?;
        let sell = r.get_i64()?;
        let valid = r.get_u8()?;
        let slow = r.get_i64()?;
        if valid > 1 {
            return Err(RestoreError::BadValue);
        }
        *self = TriggerPx {
            buy_bid_fast: Px(buy),
            sell_ask_fast: Px(sell),
            valid: false,
            slow_ens: slow,
        };
        Ok(())
    }
}
