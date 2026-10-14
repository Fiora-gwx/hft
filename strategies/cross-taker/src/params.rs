//! Operator parameters; all rates use 1e-9 and notionals use 1e-6 quote units.
use hft_strategy_sdk::prelude::*;

hft_strategy_sdk::define_params! {
    pub struct XtParams(schema_id = 3, version = 1, fields = XtFields) {
        /// Explicit operator enable; provisional default is disabled.
        enabled: Bool [default = 0, manual_only, relax = Up],
        /// Required net return after both taker fees and price allowances; must be set before enabling.
        min_edge: Rate [min = 0, max = 100_000_000, default = 0, per_group, relax = Down],
        /// IOC price allowance on each leg; provisional default crosses only the touch.
        slip: Rate [min = 0, max = 50_000_000, default = 0, per_group, relax = Up],
        /// Fraction of each taking top level; provisional default 50 percent.
        depth_frac: Rate [min = 1, max = 1_000_000_000, default = 500_000_000, per_group, relax = Up],
        /// Per-leg order notional cap; must be set before enabling.
        max_chunk_notional: Notional [min = 0, max = 1_000_000_000_000_000, default = 0, per_group, relax = Up],
        /// Absolute position notional cap on each leg; must be set before enabling.
        max_position_notional: Notional [min = 0, max = 1_000_000_000_000_000, default = 0, per_group, relax = Up],
        /// Opportunity age limit on both quotes, distinct from connection health; provisional 5 ms.
        max_quote_age_ms: DurationMs [min = 1, max = 60_000, default = 5, per_group, relax = Up],
        /// Minimum engine time between attempts including gated and shadow attempts; provisional 100 ms.
        cooldown_ms: DurationMs [min = 0, max = 3_600_000, default = 100, per_group, relax = Down],
        /// Consecutive hedge failures before requesting a group halt; provisional three.
        max_hedge_failures: Count [min = 1, max = 100, default = 3, per_group, relax = Up],
    }
}

impl XtParams {
    pub fn check(&self, groups: &GroupTable, rd: &RefView<'_>) -> Result<(), ParamError> {
        for (gix, g) in groups.iter() {
            let err = |code| ParamError::GroupShape { gix, code };
            if g.n_legs != 2 {
                return Err(err(1));
            }
            let [a, b, ..] = g.legs;
            let ah = rd.try_inst(a.inst).ok_or(err(2))?;
            let bh = rd.try_inst(b.inst).ok_or(err(2))?;
            if a.venue != ah.venue
                || b.venue != bh.venue
                || a.venue == b.venue
                || rd.refdata().venue(a.venue).exchange == rd.refdata().venue(b.venue).exchange
                || ah.asset != g.asset
                || bh.asset != g.asset
                || ah.quote != bh.quote
                || ah.px_exp != bh.px_exp
                || ah.qty_exp != bh.qty_exp
            {
                return Err(err(3));
            }
            for l in [a, b] {
                let h = rd.inst(l.inst);
                if !matches!(rd.caps(l.venue).market, MarketKind::Spot | MarketKind::LinearPerp)
                    || h.tick <= 0
                    || h.lot <= 0
                    || rd.try_fee(rd.account().id, l.inst).is_none()
                {
                    return Err(err(4));
                }
            }
            // A per-symbol cap must not be multiplied by duplicate groups in this instance.
            if groups.iter().any(|(ix, other)| {
                ix < gix
                    && other
                        .legs
                        .iter()
                        .take(usize::from(other.n_legs))
                        .any(|l| l.inst == a.inst || l.inst == b.inst)
            }) {
                return Err(err(5));
            }
            let f = self.at(gix);
            if f.enabled
                && (f.min_edge.0 <= 0
                    || f.max_chunk_notional.0 <= 0
                    || f.max_position_notional.0 < f.max_chunk_notional.0)
            {
                return Err(ParamError::Invalid { field_ix: 4, code: 1 });
            }
        }
        Ok(())
    }
}
