//! `SimAccountView`: read-only truth of one Sim account (DST invariants, `SimDrift`, tests).

use hft_types::{AccountId, AssetId, InstId, Px, Qty, RoundMode, Side, SlotId, VenueId};

use crate::SimExchange;
use crate::fx::{div_round, sat64};
use crate::types::{MEMORY_RING, SimBalance, SimCounters, SimMargin, SimOrderView, SimPosition};

/// Borrowed view of one Sim account.
#[derive(Copy, Clone)]
pub struct SimAccountView<'a> {
    pub(crate) ex: &'a SimExchange,
    pub(crate) ai: usize,
}

impl core::fmt::Debug for SimAccountView<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SimAccountView")
            .field("account", &self.id())
            .finish_non_exhaustive()
    }
}

impl<'a> SimAccountView<'a> {
    pub fn id(&self) -> AccountId {
        self.ex.accts.get(self.ai).map_or(AccountId(0), |a| a.id)
    }

    /// Venues of the account, ascending.
    pub fn venues(&self) -> &'a [VenueId] {
        self.ex.accts.get(self.ai).map_or(&[], |a| &a.venues)
    }

    /// Balance of `(venue, asset)` (zero when absent).
    pub fn balance(&self, venue: VenueId, asset: AssetId) -> SimBalance {
        let b = self.ex.bal(self.ai, venue, asset);
        SimBalance {
            total: Qty(b.total),
            locked: Qty(b.locked),
            free: Qty(b.total.saturating_sub(b.locked)),
        }
    }

    /// Every non-zero balance, ascending by (venue, asset).
    pub fn balances(&self) -> impl Iterator<Item = (VenueId, AssetId, SimBalance)> + 'a {
        let ex = self.ex;
        let ai = self.ai;
        let n = ex.n_assets;
        let venues: &'a [VenueId] = ex.accts.get(ai).map_or(&[], |a| &a.venues);
        venues.iter().flat_map(move |v| {
            (1..n).filter_map(move |k| {
                let b = ex.bal(ai, *v, AssetId(k as u16));
                (b.total != 0 || b.locked != 0).then_some((
                    *v,
                    AssetId(k as u16),
                    SimBalance {
                        total: Qty(b.total),
                        locked: Qty(b.locked),
                        free: Qty(b.total.saturating_sub(b.locked)),
                    },
                ))
            })
        })
    }

    /// Perpetual position of `inst` (`qty = 0` when flat).
    pub fn position(&self, inst: InstId) -> SimPosition {
        let p = self
            .ex
            .inst_index(inst)
            .and_then(|ii| self.ex.accts.get(self.ai)?.pos.get(ii).copied())
            .unwrap_or_default();
        let entry = if p.qty == 0 {
            0
        } else {
            sat64(div_round(p.cost, i128::from(p.qty).abs(), RoundMode::HalfEven))
        };
        SimPosition {
            qty: Qty(p.qty),
            entry_px: Px(entry),
        }
    }

    /// Every non-zero position, ascending by inst.
    pub fn positions(&self) -> impl Iterator<Item = (InstId, SimPosition)> + 'a {
        let v = *self;
        self.ex.insts.iter().filter_map(move |r| {
            let p = v.position(r.id);
            (p.qty.0 != 0).then_some((r.id, p))
        })
    }

    /// Cross-margin figures of a perpetual venue of the account.
    pub fn margin(&self, venue: VenueId) -> Option<SimMargin> {
        let perp = self.ex.venues.get(usize::from(venue.0)).is_some_and(|v| v.perp);
        let in_acct = self
            .ex
            .accts
            .get(self.ai)
            .and_then(|a| a.vslot.get(usize::from(venue.0)))
            .is_some_and(|s| *s != u8::MAX);
        (perp && in_acct).then(|| self.ex.margin_public(self.ai, venue))
    }

    /// Liquidation price of the position in `inst` (0 = none), same rule as `MarginReport`.
    pub fn liq_px(&self, inst: InstId) -> Px {
        Px(self
            .ex
            .inst_index(inst)
            .map_or(0, |ii| self.ex.liq_px(self.ai, ii)))
    }

    /// Latest order-table record of `slot`.
    pub fn order(&self, slot: SlotId) -> Option<SimOrderView> {
        let k = self.ex.find_slot(self.ai, slot.0)?;
        let o = self.ex.order_at(self.ai, k);
        let inst = self.ex.insts.get(usize::from(o.inst)).map_or(InstId(0), |r| r.id);
        Some(SimOrderView {
            slot: SlotId(o.slot),
            r#gen: o.r#gen,
            inst,
            side: o.side,
            tif: o.tif,
            status: o.status,
            resting: o.resting,
            px: Px(o.px),
            qty: Qty(o.qty),
            filled: Qty(o.filled),
            avg_px: Px(if o.filled > 0 {
                sat64(div_round(o.fill_val, i128::from(o.filled), RoundMode::HalfEven))
            } else {
                0
            }),
            exch_order_id: o.exch_id,
        })
    }

    /// Used order-table entries.
    pub fn order_count(&self) -> usize {
        self.ex
            .accts
            .get(self.ai)
            .map_or(0, |a| a.orders.iter().filter(|o| o.used).count())
    }

    /// Resting orders.
    pub fn resting_count(&self) -> usize {
        self.ex.accts.get(self.ai).map_or(0, |a| a.n_resting as usize)
    }

    /// Stored consumed-liquidity entries of `(inst, taker side)` (expired entries included until
    /// reused).
    pub fn memory_entries(&self, inst: InstId, side: Side) -> usize {
        let Some(ii) = self.ex.inst_index(inst) else {
            return 0;
        };
        let si = if side == Side::Buy { 0 } else { 1 };
        let base = (ii * 2 + si) * MEMORY_RING;
        self.ex
            .accts
            .get(self.ai)
            .and_then(|a| a.mem.get(base..base + MEMORY_RING))
            .map_or(0, |r| r.iter().filter(|e| e.used).count())
    }

    /// Σ quantity of the stored memory entries of `(inst, taker side)` at `px` (not decayed).
    pub fn memory_at(&self, inst: InstId, side: Side, px: Px) -> Qty {
        let Some(ii) = self.ex.inst_index(inst) else {
            return Qty(0);
        };
        let si = if side == Side::Buy { 0 } else { 1 };
        let base = (ii * 2 + si) * MEMORY_RING;
        Qty(self
            .ex
            .accts
            .get(self.ai)
            .and_then(|a| a.mem.get(base..base + MEMORY_RING))
            .map_or(0, |r| {
                r.iter().filter(|e| e.used && e.px == px.0).map(|e| e.qty).sum()
            }))
    }

    /// Per-account Sim counters.
    pub fn counters(&self) -> SimCounters {
        self.ex
            .accts
            .get(self.ai)
            .map_or_else(SimCounters::default, |a| a.counters)
    }
}
