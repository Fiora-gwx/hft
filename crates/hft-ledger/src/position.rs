//! Average-cost positions, crossing zero, the net-quantity rule and `NetAdjust`; the shared trade
//! booking used by fills, external fills and adoptions; unrealized revaluation
//! (README "成交增量与净量", "仓位与已实现", "未实现与当日 PnL").

use hft_event::NoteLevel;
use hft_types::{
    ASSET_NONE, AccountId, GROUP_NONE, GroupId, InstId, Notional, Px, Qty, RoundMode, Side, VenueId,
};

use crate::fixed::{FixedVec, rescale, sat64};
use crate::ledger::Ledger;
use crate::notes::{code, note};
use crate::types::*;

/// PnL column a trade's realized part goes to.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum Col {
    Price,
    External,
    Adjust,
}

/// One booking of a quantity on a position (and optionally a group leg).
#[derive(Copy, Clone, Debug)]
pub(crate) struct Trade {
    pub acc: AccountId,
    pub inst: InstId,
    pub side: Side,
    pub gross: Qty,
    /// Net quantity (≥ 0) applied to the position.
    pub net: Qty,
    pub px: Px,
    /// `(cell, leg)` the quantity is attributed to.
    pub cell: Option<(usize, usize)>,
    pub col: Col,
    pub t: i64,
    pub seq: u64,
    /// Spot: move the quote cash and the base inventory.
    pub cash: bool,
    /// This cell's caller books a fee before checking whether the trade closes its round.
    pub defer_round: Option<usize>,
}

/// Average-cost update of `(qty, cost)` by `s × q` at `p`; returns the realized amount
/// (same exponent as `cost`).
pub(crate) fn avg_apply(qty: &mut i64, cost: &mut i128, s: i64, q: i64, p: i64) -> i128 {
    if q <= 0 {
        return 0;
    }
    let open_cost = |open: i64| -> i128 { i128::from(p) * i128::from(open) };
    let qq = *qty;
    if qq == 0 || qq.signum() == s {
        *qty = qq.saturating_add(s.saturating_mul(q));
        *cost = cost.saturating_add(open_cost(q));
        return 0;
    }
    let abs = qq.saturating_abs();
    let close = q.min(abs);
    let removed = if close == abs {
        *cost
    } else {
        crate::fixed::div_round(
            cost.saturating_mul(i128::from(close)),
            i128::from(abs),
            RoundMode::TowardZero,
        )
    };
    let realized = i128::from(qq.signum()) * (i128::from(p) * i128::from(close) - removed);
    *cost -= removed;
    *qty = qq.saturating_add(s.saturating_mul(close));
    let open = q - close;
    if open > 0 {
        *qty = qty.saturating_add(s.saturating_mul(open));
        *cost = cost.saturating_add(open_cost(open));
    }
    realized
}

/// Adds `d` to the PnL column `col`.
pub(crate) fn col_add(b: &mut PnlBreakdown, col: Col, d: Notional) {
    let x = match col {
        Col::Price => &mut b.price_realized,
        Col::External => &mut b.external,
        Col::Adjust => &mut b.adjust,
    };
    x.0 = x.0.saturating_add(d.0);
}

impl Ledger {
    /// Adds `d` to the group's cumulative, day and round books (when attributed) and to the
    /// account's day book.
    pub(crate) fn add_pnl(&mut self, acc: AccountId, cell: Option<usize>, d: &PnlBreakdown) {
        if let Some(c) = cell.and_then(|c| self.cells.get_mut(c)) {
            c.book.pnl.add(d);
            c.book.day.add(d);
            let mut r = *d;
            r.unrealized = Notional(0);
            c.book.round.pnl.add(&r);
        }
        if let Some(a) = self.accounts.get_mut(usize::from(acc.0)) {
            a.day.add(d);
        }
    }

    /// Books one trade: position, leg, cash and inventory, realized PnL; then revaluation,
    /// round and margin upkeep. Returns the PnL booked (realized column only).
    pub(crate) fn book_trade(&mut self, tr: &Trade) -> PnlBreakdown {
        self.book_trade_with_cash(tr, None)
    }

    /// `cash` replaces only the perpetual wallet leg of this trade. Position/leg average
    /// cost and analytical PnL still follow the recorded fill order.
    pub(crate) fn book_trade_with_cash(&mut self, tr: &Trade, cash: Option<Qty>) -> PnlBreakdown {
        self.book_trade_settled(tr, cash, None)
    }

    fn book_trade_settled(
        &mut self,
        tr: &Trade,
        cash: Option<Qty>,
        spot_notional: Option<i128>,
    ) -> PnlBreakdown {
        let mut d = PnlBreakdown::zero();
        let Some(s) = self.inst(tr.inst).copied() else {
            return d;
        };
        let Some(av) = self.av(tr.acc, s.venue) else {
            return d;
        };
        let Some(p) = self.pos_ix(tr.acc, tr.inst) else {
            return d;
        };
        let sign = tr.side.sign();
        let q = tr.net.0.max(0);
        let px = tr.px.0;
        // Round leg aggregation before the leg moves.
        if let Some((c, l)) = tr.cell {
            self.round_leg_trade(
                c,
                l,
                s.quote,
                i32::from(s.px_exp) + i32::from(s.qty_exp),
                sign,
                q,
                px,
            );
        }
        // Position.
        let realized = match self.pos.get_mut(p) {
            Some(x) => {
                let mut qty = x.qty.0;
                let r = avg_apply(&mut qty, &mut x.cost, sign, q, px);
                x.qty = Qty(qty);
                x.realized_quote = x.realized_quote.saturating_add(r);
                x.last_fill_ens = tr.t;
                r
            }
            None => 0,
        };
        if let Some(x) = self.ipx.get_mut(tr.inst.0 as usize) {
            x.last_px = tr.px;
        }
        // Leg.
        let mut group = GROUP_NONE;
        if let Some((c, l)) = tr.cell
            && let Some(cell) = self.cells.get_mut(c)
        {
            group = cell.group;
            cell.last_fill_ens = tr.t;
            if let Some(leg) = cell.legs.get_mut(l) {
                let mut n = leg.net.0;
                avg_apply(&mut n, &mut leg.cost, sign, q, px);
                leg.net = Qty(n);
            }
        }
        let pe = i32::from(s.px_exp) + i32::from(s.qty_exp);
        // Cash.
        if s.perp {
            let amount = match cash {
                Some(actual) => actual.0,
                None => {
                    let me = i32::from(self.asset_s(s.quote).qty_exp);
                    let (r, lossy) = rescale(realized, pe, me, RoundMode::TowardZero);
                    if lossy {
                        self.rounding = self.rounding.saturating_add(1);
                    }
                    sat64(r)
                }
            };
            self.quote_add(av, s.quote, amount);
        } else if tr.cash {
            let qe = i32::from(self.asset_s(s.quote).qty_exp);
            // Settle each gross spot execution in the quote asset's smallest unit.
            // Paying rounds up; receiving rounds down, independently of PnL conversion.
            let rounding = if sign > 0 {
                RoundMode::Ceil
            } else {
                RoundMode::Floor
            };
            let (amt, lossy) = rescale(
                spot_notional.unwrap_or_else(|| i128::from(px).saturating_mul(i128::from(tr.gross.0))),
                pe,
                qe,
                rounding,
            );
            if lossy {
                self.rounding = self.rounding.saturating_add(1);
            }
            let amt = sat64(amt);
            self.quote_add(av, s.quote, if sign > 0 { amt.saturating_neg() } else { amt });
            self.base_add(av, s.base, group, sign.saturating_mul(q));
        }
        // Realized.
        let rn = self.usdt(s.quote, realized, pe, RoundMode::TowardZero);
        col_add(&mut d, tr.col, rn);
        let cell = tr.cell.map(|(c, _)| c);
        self.add_pnl(tr.acc, cell, &d);
        // Upkeep. While the inst is valued at its last trade price (no mark / no BBO yet), that
        // price just moved for every holder: revalue them all, so the caches stay the function of
        // the price rows that `restore` rebuilds them from.
        self.track_open_perp(av, p);
        if self.val_by_last_px(tr.inst) {
            self.revalue_inst(tr.inst, tr.t, None, tr.defer_round);
        }
        self.revalue_pos(tr.acc, p, tr.inst);
        if let Some((c, l)) = tr.cell {
            self.revalue_leg(c, l);
            if tr.defer_round != Some(c) {
                self.cell_upkeep(c, tr.t, tr.seq);
            }
        }
        if s.perp {
            self.margin_refresh(av);
        }
        self.refresh_summary(av);
        d
    }

    /// Books a fee: PnL column (`fee_cex` or `external`) and the cash bucket; a fee in the
    /// inst's base asset is already in the net quantity when `in_net`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn book_fee(
        &mut self,
        acc: AccountId,
        inst: InstId,
        cell: Option<usize>,
        fee: i64,
        fee_asset: hft_types::AssetId,
        external: bool,
        in_net: bool,
        px: Px,
    ) -> PnlBreakdown {
        let mut d = PnlBreakdown::zero();
        if fee_asset == ASSET_NONE || fee == 0 {
            return d;
        }
        let Some(s) = self.inst(inst).copied() else {
            return d;
        };
        let Some(av) = self.av(acc, s.venue) else { return d };
        let fe = i32::from(self.asset_s(fee_asset).qty_exp);
        let usd = Notional(
            self.usdt(fee_asset, i128::from(fee), fe, RoundMode::Ceil)
                .0
                .saturating_neg(),
        );
        if external {
            d.external = usd;
        } else {
            d.fee_cex = usd;
        }
        if !in_net {
            if self.is_quote(s.venue, fee_asset) {
                self.quote_add(av, fee_asset, fee.saturating_neg());
            } else {
                self.fee_bucket(av, fee_asset, fee);
            }
        }
        // fees_quote: quote units.
        let fq = if fee_asset == s.quote {
            fee
        } else if fee_asset == s.base {
            let qe = i32::from(self.asset_s(s.quote).qty_exp);
            sat64(
                rescale(
                    i128::from(fee).saturating_mul(i128::from(px.0)),
                    i32::from(s.px_exp) + i32::from(s.qty_exp),
                    qe,
                    RoundMode::TowardZero,
                )
                .0,
            )
        } else {
            let qe = i32::from(self.asset_s(s.quote).qty_exp);
            sat64(rescale(i128::from(usd.0.saturating_neg()), -6, qe, RoundMode::TowardZero).0)
        };
        if let Some(p) = self.pos_ix(acc, inst)
            && let Some(x) = self.pos.get_mut(p)
        {
            x.fees_quote = x.fees_quote.saturating_add(fq);
        }
        if let Some(c) = cell
            && let Some(cl) = self.cells.get_mut(c)
            && let Some(l) = cl.legs.iter().position(|l| l.inst == inst)
            && let Some(agg) = cl.book.round.legs.get_mut(l)
        {
            agg.fee = Notional(agg.fee.0.saturating_add(usd.0));
        }
        self.add_pnl(acc, cell, &d);
        self.refresh_summary(av);
        d
    }

    /// Cell and leg of a fill's `(group, leg_ix)`.
    pub(crate) fn resolve_cell(
        &mut self,
        a: AccountId,
        g: GroupId,
        inst: InstId,
        leg_ix: u8,
    ) -> Option<(usize, usize)> {
        let c = self.cell_or_new(a, g)?;
        let l = self.leg_of(c, inst, leg_ix)?;
        Some((c, l))
    }

    /// CEX fill increment (README "成交增量与净量"): the net quantity is recomputed; a mismatch
    /// with OMS writes `NetQtyMismatch`, books the recomputed value and returns `net_adjust`.
    pub fn apply_fill(&mut self, f: &FillDelta, t: i64) -> FillApplied {
        self.fill_with_cash(f, None, t)
    }

    /// Books a matched spot increment using its original gross Px×Qty notional.
    /// Cash rounds once in the quote asset; the rounded increment price still drives cost.
    /// No allocation; budget: ledger/apply_spot_fill (README "延迟与资源契约").
    pub fn apply_spot_fill(
        &mut self,
        f: &FillDelta,
        notional: i128,
        t: i64,
    ) -> Result<FillApplied, SettlementError> {
        let Some(s) = self.inst(f.inst) else {
            return Err(SettlementError::UnknownInstrument);
        };
        if s.perp {
            return Err(SettlementError::NotSpot);
        }
        if s.venue != f.venue {
            return Err(SettlementError::VenueMismatch);
        }
        if self.av(f.account, s.venue).is_none() {
            return Err(SettlementError::AccountVenue);
        }
        if f.qty.0 <= 0 {
            return Err(SettlementError::InvalidQuantity);
        }
        if f.px.0 <= 0 {
            return Err(SettlementError::InvalidPrice);
        }
        if notional <= 0 {
            return Err(SettlementError::InvalidNotional);
        }
        Ok(self.fill_settled(f, None, Some(notional), t))
    }

    /// Books an already matched/de-duplicated perpetual fill and its actual cash settlement.
    /// Fees remain separate. Zero does not fall back to the local average-cost estimate.
    /// No allocation; budget: ledger/apply_settled_fill (README "延迟与资源契约").
    pub fn apply_settled_fill(
        &mut self,
        f: &FillDelta,
        settled: PerpCashSettlement,
        t: i64,
    ) -> Result<FillApplied, SettlementError> {
        let Some(s) = self.inst(f.inst) else {
            return Err(SettlementError::UnknownInstrument);
        };
        if !s.perp {
            return Err(SettlementError::NotPerpetual);
        }
        if s.venue != f.venue {
            return Err(SettlementError::VenueMismatch);
        }
        if self.av(f.account, s.venue).is_none() {
            return Err(SettlementError::AccountVenue);
        }
        if settled.asset != s.quote {
            return Err(SettlementError::WrongAsset);
        }
        if f.qty.0 <= 0 || f.net_qty != f.qty {
            return Err(SettlementError::InvalidQuantity);
        }
        if f.px.0 <= 0 {
            return Err(SettlementError::InvalidPrice);
        }
        Ok(self.fill_with_cash(f, Some(settled.amount), t))
    }

    fn fill_with_cash(&mut self, f: &FillDelta, cash: Option<Qty>, t: i64) -> FillApplied {
        self.fill_settled(f, cash, None, t)
    }

    fn fill_settled(
        &mut self,
        f: &FillDelta,
        cash: Option<Qty>,
        spot_notional: Option<i128>,
        t: i64,
    ) -> FillApplied {
        self.tick(t);
        let zero = FillApplied {
            pnl: PnlDelta {
                account: f.account,
                group: f.group,
                d: PnlBreakdown::zero(),
            },
            net_adjust: None,
        };
        let Some(s) = self.inst(f.inst).copied() else {
            return zero;
        };
        if self.av(f.account, s.venue).is_none() || f.qty.0 <= 0 {
            return zero;
        }
        let fee_base = if !s.perp && f.fee_asset == s.base {
            f.fee
        } else {
            0
        };
        let recalc = if s.perp {
            f.qty.0
        } else if f.side == Side::Buy {
            f.qty.0.saturating_sub(fee_base)
        } else {
            f.qty.0.saturating_add(fee_base)
        };
        let mut net_adjust = None;
        if recalc != f.net_qty.0 {
            let delta = recalc.saturating_sub(f.net_qty.0).saturating_mul(f.side.sign());
            net_adjust = Some(NetAdjust {
                account: f.account,
                venue: f.venue,
                group: f.group,
                inst: f.inst,
                delta: Qty(delta),
            });
            self.push_note(note(
                code::NET_QTY_MISMATCH,
                NoteLevel::P1,
                f.group,
                f.inst,
                [
                    u64::from(f.venue.0) | u64::from(f.account.0) << 8,
                    f.net_qty.0 as u64,
                    recalc as u64,
                    delta as u64,
                    f.seq,
                    0,
                    0,
                    0,
                ],
            ));
        }
        let cell = self.resolve_cell(f.account, f.group, f.inst, f.leg_ix);
        let col = match FillClass::try_from(f.class) {
            Ok(FillClass::External) => Col::External,
            Ok(FillClass::Adjust) => Col::Adjust,
            _ => Col::Price,
        };
        let mut d = self.book_trade_settled(
            &Trade {
                acc: f.account,
                inst: f.inst,
                side: f.side,
                gross: f.qty,
                net: Qty(recalc),
                px: f.px,
                cell,
                col,
                t,
                seq: f.seq,
                cash: true,
                defer_round: cell.map(|(c, _)| c),
            },
            cash,
            spot_notional,
        );
        let fd = self.book_fee(
            f.account,
            f.inst,
            cell.map(|x| x.0),
            f.fee,
            f.fee_asset,
            col == Col::External,
            fee_base != 0,
            f.px,
        );
        d.add(&fd);
        if let Some((c, _)) = cell {
            self.cell_upkeep(c, t, f.seq);
        }
        FillApplied {
            pnl: PnlDelta {
                account: f.account,
                group: cell.map_or(GROUP_NONE, |_| f.group),
                d,
            },
            net_adjust,
        }
    }

    /// Late fee (OMS `FeeOut`); a fee in the inst's base asset also reduces the position, the
    /// inventory and the leg (`delta = −fee`) and returns `net_adjust`.
    pub fn apply_fee(&mut self, f: &FeeDelta, t: i64) -> FeeApplied {
        self.tick(t);
        let zero = FeeApplied {
            pnl: PnlDelta {
                account: f.account,
                group: f.group,
                d: PnlBreakdown::zero(),
            },
            net_adjust: None,
        };
        #[cfg(feature = "planted")]
        if crate::planted::on(crate::planted::IGNORE_FEE) {
            return zero;
        }
        let Some(s) = self.inst(f.inst).copied() else {
            return zero;
        };
        let Some(av) = self.av(f.account, s.venue) else {
            return zero;
        };
        let cell = if f.group == GROUP_NONE {
            None
        } else {
            self.resolve_cell(f.account, f.group, f.inst, u8::MAX)
        };
        let base = !s.perp && f.fee_asset == s.base && f.fee != 0;
        let mut net_adjust = None;
        if base {
            let fee = f.fee;
            if let Some(p) = self.pos_ix(f.account, f.inst) {
                if let Some(x) = self.pos.get_mut(p) {
                    reduce_by(&mut x.qty, &mut x.cost, fee);
                }
                if let Some((c, l)) = cell
                    && let Some(leg) = self.cells.get_mut(c).and_then(|c| c.legs.get_mut(l))
                {
                    reduce_by(&mut leg.net, &mut leg.cost, fee);
                }
                self.base_add(
                    av,
                    s.base,
                    cell.map_or(GROUP_NONE, |_| f.group),
                    fee.saturating_neg(),
                );
                self.revalue_pos(f.account, p, f.inst);
                if let Some((c, l)) = cell {
                    self.revalue_leg(c, l);
                }
            }
            if cell.is_some() {
                net_adjust = Some(NetAdjust {
                    account: f.account,
                    venue: f.venue,
                    group: f.group,
                    inst: f.inst,
                    delta: Qty(fee.saturating_neg()),
                });
            }
        }
        let px = self.ipx.get(f.inst.0 as usize).map_or(Px(0), |p| p.last_px);
        let d = self.book_fee(
            f.account,
            f.inst,
            cell.map(|x| x.0),
            f.fee,
            f.fee_asset,
            false,
            base,
            px,
        );
        if base && let Some((c, _)) = cell {
            self.cell_upkeep(c, t, f.seq);
        }
        FeeApplied {
            pnl: PnlDelta {
                account: f.account,
                group: cell.map_or(GROUP_NONE, |_| f.group),
                d,
            },
            net_adjust,
        }
    }

    /// Keeps the open-perpetual list of `av` in step with position `p`.
    pub(crate) fn track_open_perp(&mut self, av: usize, p: usize) {
        let open = self.pos.get(p).is_some_and(|x| x.qty.0 != 0);
        let Some(d) = self.avd.get_mut(av) else { return };
        let k = d.open_perps.iter().position(|x| *x as usize == p);
        let perp = self
            .avs
            .get(av)
            .and_then(|s| self.venues.get(usize::from(s.venue.0)))
            .is_some_and(|v| v.perp);
        if !perp {
            return;
        }
        match (open, k) {
            (true, None) => {
                if d.open_perps.len() < d.open_perps.capacity() {
                    d.open_perps.push(p as u32);
                }
            }
            (false, Some(k)) => {
                d.open_perps.swap_remove(k);
            }
            _ => {}
        }
    }

    /// Recomputes the unrealized and held notional of position `p`; deltas go to the account.
    pub(crate) fn revalue_pos(&mut self, acc: AccountId, p: usize, inst: InstId) {
        let Some(x) = self.pos.get(p).copied() else { return };
        let u = self.upnl_of(inst, x.qty, x.cost);
        let n = self.notional_of(inst, x.qty);
        let Some(pd) = self.posd.get_mut(p) else { return };
        let du = u.0.saturating_sub(pd.upnl.0);
        let dn = n.0.saturating_sub(pd.notional.0);
        pd.upnl = u;
        pd.notional = n;
        if let Some(a) = self.accounts.get_mut(usize::from(acc.0)) {
            a.upnl = Notional(a.upnl.0.saturating_add(du));
            a.day.unrealized = Notional(a.day.unrealized.0.saturating_add(du));
            a.held_notional = Notional(a.held_notional.0.saturating_add(dn));
        }
    }

    /// Recomputes the unrealized of leg `l` of cell `c`; the delta goes to the group books.
    pub(crate) fn revalue_leg(&mut self, c: usize, l: usize) {
        let Some(leg) = self.cells.get(c).and_then(|x| x.legs.get(l)).copied() else {
            return;
        };
        let u = self.upnl_of(leg.inst, leg.net, leg.cost);
        let Some(cell) = self.cells.get_mut(c) else { return };
        let Some(old) = cell.leg_upnl.get_mut(l) else {
            return;
        };
        let du = u.0.saturating_sub(old.0);
        *old = u;
        cell.book.pnl.unrealized = Notional(cell.book.pnl.unrealized.0.saturating_add(du));
        cell.book.day.unrealized = Notional(cell.book.day.unrealized.0.saturating_add(du));
    }

    /// Revalues every holder of `inst` after a price change; perpetual holders' (account,
    /// venue) go to `touched`.
    pub(crate) fn revalue_inst(
        &mut self,
        inst: InstId,
        t: i64,
        mut touched: Option<&mut FixedVec<(AccountId, VenueId), 4>>,
        defer_round: Option<usize>,
    ) {
        let Some(s) = self.inst(inst).copied() else { return };
        let Some(vs) = self.venue_s(s.venue) else { return };
        let n_av = vs.avs.len();
        for k in 0..n_av {
            let Some(av) = self
                .venue_s(s.venue)
                .and_then(|v| v.avs.get(k))
                .map(|x| usize::from(*x))
            else {
                continue;
            };
            let Some(st) = self.avs.get(av).copied() else {
                continue;
            };
            let p = st.pos_base as usize + s.local as usize;
            let held = self.pos.get(p).is_some_and(|x| x.qty.0 != 0);
            let had = self
                .posd
                .get(p)
                .is_some_and(|d| d.upnl.0 != 0 || d.notional.0 != 0);
            if held || had {
                self.revalue_pos(st.account, p, inst);
            }
            let mut any_leg = false;
            for j in 0..self.w {
                let v = self.cand.get(p * self.w + j).copied().unwrap_or(0);
                if v == 0 {
                    break;
                }
                let (c, l) = ((v >> 2) as usize - 1, (v & 3) as usize);
                let nz = self
                    .cells
                    .get(c)
                    .and_then(|x| {
                        x.legs
                            .get(l)
                            .map(|g| g.net.0 != 0 || x.leg_upnl.get(l).is_some_and(|u| u.0 != 0))
                    })
                    .unwrap_or(false);
                if nz {
                    self.revalue_leg(c, l);
                    if defer_round != Some(c) {
                        self.cell_upkeep(c, t, 0);
                    }
                    any_leg = true;
                }
            }
            if s.perp && (held || had) {
                self.margin_refresh(av);
                if let Some(tc) = touched.as_deref_mut() {
                    tc.push((st.account, st.venue));
                }
            }
            if held || had || any_leg {
                self.refresh_summary(av);
            }
        }
    }
}

/// `qty −= fee` (signed position: towards less base asset) with the cost reduced at the average
/// cost; no realized PnL.
fn reduce_by(qty: &mut Qty, cost: &mut i128, fee: i64) {
    let q = qty.0;
    if q == 0 {
        *qty = Qty(q.saturating_sub(fee));
        return;
    }
    let abs = i128::from(q).abs();
    let take = i128::from(fee).min(abs);
    let removed = crate::fixed::div_round(cost.saturating_mul(take), abs, RoundMode::TowardZero);
    *cost -= removed;
    *qty = Qty(q.saturating_sub(fee));
}
