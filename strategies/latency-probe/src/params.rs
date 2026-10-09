//! `ProbeParams` (README "参数 schema(ProbeParams)") and its cross-field and group-shape `check`.

use hft_strategy_sdk::prelude::*;
use hft_strategy_sdk::{GroupTable, ParamError, RefView};

hft_strategy_sdk::define_params! {
    pub struct ProbeParams(schema_id = 5, version = 1, fields = ProbeFields) {
        /// false: no new probes or opening targets (resting probes are still cancelled, residuals still flattened)
        enabled: Bool [default = 0, manual_only, relax = Up],
        /// group mode: quote, pair or target
        mode: Enum(["quote", "pair", "target"]) [default = 0, per_group, manual_only],
        /// quote: minimum interval between two probes of one probe point (engine_ns)
        probe_period_ms: DurationMs [min = 100, max = 600_000, default = 1_000, per_group, relax = Down],
        /// quote: distance of the PostOnly price from the opposite touch
        far_bp: Rate [min = 5_000_000, max = 100_000_000, default = 30_000_000, per_group, relax = Down],
        /// quote: side of the probe; spot legs always buy
        quote_side: Enum(["buy", "sell", "alternate"]) [default = 0, per_group],
        /// quote: max_rest_ms of a probe (OMS Aged backstop)
        quote_rest_ms: DurationMs [min = 100, max = 600_000, default = 2_000, per_group],
        /// quote: cross-gw variant (leg 1's Ack cancels leg 0's resting probe)
        quote_xgw: Bool [default = 0, per_group],
        /// quote: minimum interval of a burst-layer trigger; 0 = none
        burst_period_ms: DurationMs [min = 0, max = 600_000, default = 0, per_group],
        /// pair: minimum interval between two openings
        pair_period_ms: DurationMs [min = 10_000, max = 3_600_000, default = 10_000, per_group, relax = Down],
        /// target: minimum interval between two opening targets (provisional)
        target_period_ms: DurationMs [min = 10_000, max = 3_600_000, default = 60_000, per_group, relax = Down],
        /// pair/target: holding time before flattening; also the flatten re-check interval
        pair_flat_ms: DurationMs [min = 1_000, max = 600_000, default = 5_000, per_group],
        /// pair: IOC cross of the first leg and closes; target: slice max_cross and hedge cross (provisional)
        pair_cross: Rate [min = 100_000, max = 50_000_000, default = 5_000_000, per_group, relax = Up],
        /// pair/target: opening direction
        pair_side: Enum(["auto", "buy_a", "sell_a"]) [default = 0, per_group],
        /// notional margin of the per-order quantity
        size_margin: Rate [min = 0, max = 1_000_000_000, default = 100_000_000],
        /// skip probing while the leg's (ask − bid) / mid exceeds this
        max_spread: Rate [min = 100_000, max = 100_000_000, default = 10_000_000, per_group, relax = Up],
        /// residual notional cap of a group (provisional)
        max_inv_notional: Notional [min = 1_000_000, max = 10_000_000_000, default = 100_000_000, per_group, relax = Up],
        /// consecutive exchange rejects before request_halt
        max_consec_fail: Count [min = 1, max = 100, default = 5, relax = Up],
    }
}

/// Field indices (schema order).
pub mod ix {
    pub const QUOTE_SIDE: u16 = 4;
    pub const BURST_PERIOD_MS: u16 = 7;
    pub const PAIR_SIDE: u16 = 12;
}

/// Group modes (`mode` enum index).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Mode {
    Quote,
    Pair,
    Target,
}

impl Mode {
    pub const fn of(v: u8) -> Mode {
        match v {
            1 => Mode::Pair,
            2 => Mode::Target,
            _ => Mode::Quote,
        }
    }
}

/// `quote_side` enum index.
pub const SIDE_SELL: u8 = 1;
pub const SIDE_ALTERNATE: u8 = 2;
/// `pair_side` enum index.
pub const PAIR_BUY_A: u8 = 1;
pub const PAIR_SELL_A: u8 = 2;

/// Whether `inst` is a spot inst (by its venue's market).
pub fn is_spot(rd: &RefView<'_>, inst: InstId) -> bool {
    rd.caps(rd.inst(inst).venue).market == MarketKind::Spot
}

impl ProbeParams {
    /// Group shape and cross-field checks, gix ascending, shape before fields; first error wins
    /// (codes: README "参数 schema(ProbeParams)").
    pub fn check(&self, groups: &GroupTable, rd: &RefView<'_>) -> Result<(), ParamError> {
        for (gix, g) in groups.iter() {
            let f = self.at(gix);
            let n = usize::from(g.n_legs);
            let legs = g.legs.get(..n.min(4)).unwrap_or(&[]);
            let shape = |code: u16| Err(ParamError::GroupShape { gix, code });
            let mode = Mode::of(f.mode);
            match mode {
                Mode::Quote if !(1..=4).contains(&n) => return shape(1),
                Mode::Pair | Mode::Target if n != 2 => return shape(1),
                _ => {}
            }
            let spot = |k: usize| legs.get(k).is_some_and(|l| is_spot(rd, l.inst));
            if mode != Mode::Quote && spot(0) && spot(1) {
                return shape(2);
            }
            if mode == Mode::Quote && f.quote_xgw {
                let same = match (legs.first(), legs.get(1)) {
                    (Some(a), Some(b)) => a.venue == b.venue,
                    _ => true,
                };
                if same {
                    return shape(3);
                }
            }
            let invalid = |field_ix: u16| Err(ParamError::Invalid { field_ix, code: 1 });
            match mode {
                Mode::Quote => {
                    if f.quote_side == SIDE_SELL && (0..n).any(spot) {
                        return invalid(ix::QUOTE_SIDE);
                    }
                    if f.burst_period_ms != 0 && f.burst_period_ms > f.probe_period_ms {
                        return invalid(ix::BURST_PERIOD_MS);
                    }
                }
                Mode::Pair | Mode::Target => {
                    if (spot(0) && f.pair_side == PAIR_SELL_A) || (spot(1) && f.pair_side == PAIR_BUY_A) {
                        return invalid(ix::PAIR_SIDE);
                    }
                }
            }
        }
        Ok(())
    }
}
