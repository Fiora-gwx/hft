//! Fixed-point configuration; all defaults are provisional research values.
use hft_strategy_sdk::prelude::*;

hft_strategy_sdk::define_params! {
    pub struct CarryParams(schema_id = 1, version = 2, fields = CarryFields) {
        /// Enable new positions; false exits existing positions.
        enabled: Bool [default = 0, per_group, manual_only, relax = Up],
        /// Annualised net edge required to enter.
        enter_apr: Rate [min = 0, max = 10_000_000_000, default = 100_000_000, per_group, relax = Down],
        /// Annualised net edge below which an existing position exits.
        exit_apr: Rate [min = -10_000_000_000, max = 10_000_000_000, default = 20_000_000, per_group, relax = Down],
        /// Annual opportunity/borrow charge; the strategy never borrows spot itself.
        borrow_apr: Rate [min = 0, max = 10_000_000_000, default = 0, per_group, manual_only],
        /// Horizon over which round-trip taker fees and crossing cost are amortised.
        fee_horizon_ms: DurationMs [min = 60_000, max = 604_800_000, default = 86_400_000, per_group],
        /// Enter only within this interval before the next short-leg settlement.
        entry_window_ms: DurationMs [min = 1_000, max = 28_800_000, default = 300_000, per_group],
        /// Minimum lead time for execution before settlement.
        min_lead_ms: DurationMs [min = 1_000, max = 600_000, default = 10_000, per_group],
        /// Funding observation expiry; never applied to an unchanged healthy BBO.
        funding_max_age_ms: DurationMs [min = 1_000, max = 600_000, default = 30_000],
        /// Per-group, per-leg notional cap in valuation currency (1e-6).
        max_notional: Notional [min = 1_000_000, max = 1_000_000_000_000, default = 100_000_000, per_group, relax = Up],
        /// Fraction of each wallet allocated, divided across groups sharing that venue.
        capital_fraction: Rate [min = 1, max = 900_000_000, default = 400_000_000, relax = Up],
        /// Upper bound on actual exchange leverage, in hundredths.
        max_leverage_x100: Count [min = 100, max = 1_000, default = 300, manual_only, relax = Up],
        /// IOC crossing allowance, also charged on both entry and exit in the signal.
        max_cross: Rate [min = 0, max = 10_000_000, default = 100_000, per_group, relax = Up],
        /// Evaluation timer and target retry interval, measured in engine time.
        retry_ms: DurationMs [min = 100, max = 60_000, default = 1_000],
        /// Expected adverse basis change over fee_horizon_ms, charged as a return before annualising.
        expected_basis_cost: Rate [since = 2, min = 0, max = 100_000_000, default = 0, per_group, manual_only],
    }
}

impl CarryParams {
    pub fn check(&self, groups: &GroupTable, rd: &RefView<'_>) -> Result<(), ParamError> {
        for (gix, g) in groups.iter() {
            let bad = |code| ParamError::GroupShape { gix, code };
            if g.n_legs != 2
                || g.legs
                    .iter()
                    .take(2)
                    .any(|l| l.kind != LegDef::CEX || l.route != 0)
            {
                return Err(bad(1));
            }
            let a = rd.try_inst(g.legs[0].inst).ok_or(bad(2))?;
            let b = rd.try_inst(g.legs[1].inst).ok_or(bad(2))?;
            if rd.caps(a.venue).market != MarketKind::LinearPerp
                || rd.caps(b.venue).market != MarketKind::Spot
                // Fixed deployment allowlist (protocol IDs), separate from execution capabilities.
                || a.venue != VenueId(1) || b.venue != VenueId(11)
                || rd.refdata().asset(a.quote).name.as_ref() != "USDT"
            {
                return Err(bad(2));
            }
            if a.asset != g.asset
                || b.asset != g.asset
                || a.qty_exp != b.qty_exp
                || a.px_exp != b.px_exp
                || a.quote != b.quote
                || a.venue != g.legs[0].venue
                || b.venue != g.legs[1].venue
            {
                return Err(bad(3));
            }
            if rd.refdata().venue(a.venue).exchange == rd.refdata().venue(b.venue).exchange {
                return Err(bad(4));
            }
            if groups.iter().any(|(ix, other)| {
                ix != gix
                    && other
                        .legs
                        .iter()
                        .take(usize::from(other.n_legs))
                        .any(|l| l.inst == g.legs[0].inst || l.inst == g.legs[1].inst)
            }) {
                return Err(bad(5));
            }
            let f = self.at(gix);
            if f.exit_apr.0 >= f.enter_apr.0 || f.min_lead_ms >= f.entry_window_ms {
                return Err(ParamError::Invalid { field_ix: 1, code: 1 });
            }
        }
        Ok(())
    }
}
