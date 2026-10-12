//! Account truth: spot settlement and locks, perpetual positions, cross margin and liquidation,
//! funding, transfers, reset, seeding and the truth reports (README "5."–"8.", "重启恢复").

use hft_event::{
    BalanceReport, ExternalCause, ExternalFill, FundingSettle, MarginItem, MarginReportHdr, PositionReport,
    RepMsg,
};
use hft_refdata::{FeeAssetRule, FundingSettleKind, MarginData};
use hft_types::{AccountId, AssetId, InstId, Notional, Px, Qty, Rate, RoundMode, Side, Tif, Tsc, VenueId};

use crate::SimExchange;
use crate::exec::st;
use crate::fx::{MARGIN_EXP, NOTIONAL_EXP, RATE_ONE, div_round, pow10, rescale, sat64};
use crate::models::account_rng;
use crate::state::{Bal, Bulk, LateFee, MarginRep, MemEntry, Order, PendTransfer, PosRow, RateWin};
use crate::types::{MAX_MARGIN_ITEMS, MmrSource, SimCfgErr, SimSeedState};

/// Cross-margin figures at `MARGIN_EXP` (`maint9` additionally × 10^9).
#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct MarginCalc {
    pub wallet_e: i128,
    pub upnl_e: i128,
    pub mb_e: i128,
    pub maint9: i128,
    pub initial_e: i128,
    pub occupy_e: i128,
    pub avail_e: i128,
    pub n_pos: usize,
}

impl MarginCalc {
    /// `maint / margin_balance`, ceiling; `margin_balance ≤ 0` → `Rate::MAX`.
    pub fn ratio(&self) -> i64 {
        if self.mb_e <= 0 {
            return Rate::MAX.0;
        }
        sat64(div_round(self.maint9, self.mb_e, RoundMode::Ceil))
    }
}

impl SimExchange {
    /// `K` = 10^(ve − MARGIN_EXP) of inst `ii` (value of `qty_raw × px_raw` at `MARGIN_EXP`).
    fn k_of(&self, ii: usize) -> i128 {
        self.insts
            .get(ii)
            .map_or(1, |r| pow10(r.ve().saturating_sub(MARGIN_EXP).max(0) as u32))
    }

    // ---- spot ----

    /// Asset and amount a spot order of `qty` at `px` must hold (rule 8 / resting lock): buy =
    /// notional (+ taker fee when charged in the quote asset), sell = quantity. Perpetual → nothing.
    pub(crate) fn lock_for(&self, ai: usize, ii: usize, side: Side, px: i64, qty: i64) -> (AssetId, i64) {
        let Some(r) = self.insts.get(ii) else {
            return (AssetId(0), 0);
        };
        if r.perp || qty <= 0 {
            return (AssetId(0), 0);
        }
        match side {
            Side::Sell => (r.base, qty),
            Side::Buy => {
                let qe = self.qty_exp(r.quote);
                let v = i128::from(qty) * i128::from(px);
                let notional = rescale(v, r.ve(), qe, RoundMode::Ceil);
                let rule = self.venues.get(usize::from(r.venue.0)).map(|v| v.fee_rule);
                let fee_in_quote = match rule {
                    Some(FeeAssetRule::Quote) => true,
                    Some(FeeAssetRule::Configured(a)) => a == r.quote,
                    _ => false,
                };
                let fee = if fee_in_quote {
                    let taker = self
                        .accts
                        .get(ai)
                        .and_then(|a| a.fees.get(ii))
                        .map_or(0, |f| f.taker.0);
                    rescale(v * i128::from(taker), r.ve() - 9, qe, RoundMode::Ceil).max(0)
                } else {
                    0
                };
                (r.quote, sat64(notional + fee))
            }
        }
    }

    /// Sets the lock held by order `k` to `amt` (adjusting the balance's `locked`).
    pub(crate) fn set_lock(&mut self, ai: usize, k: usize, amt: i64) {
        let o = self.order_at(ai, k);
        let (asset, _) = self.lock_for(ai, usize::from(o.inst), o.side, o.px, 1);
        let venue = self.insts.get(usize::from(o.inst)).map(|r| r.venue);
        let Some(venue) = venue else {
            return;
        };
        if o.lock != 0
            && let Some(b) = self.bal_mut(ai, venue, o.lock_asset)
        {
            b.locked = b.locked.saturating_sub(o.lock);
        }
        let amt = if asset.0 == 0 { 0 } else { amt };
        if amt != 0
            && let Some(b) = self.bal_mut(ai, venue, asset)
        {
            b.locked = b.locked.saturating_add(amt);
        }
        if let Some(x) = self.accts.get_mut(ai).and_then(|a| a.orders.get_mut(k)) {
            x.lock = amt;
            x.lock_asset = asset;
        }
    }

    fn add_bal(&mut self, ai: usize, venue: VenueId, asset: AssetId, d: i64) {
        if let Some(b) = self.bal_mut(ai, venue, asset) {
            b.total = b.total.saturating_add(d);
        }
    }

    /// Token fee of a fill (`q × px × rate` converted at the mid of the `token` spot inst of the
    /// venue); `None` without such an inst or without a two-sided view.
    fn token_fee(&mut self, ii: usize, token: AssetId, v: i128, rate: i64) -> Option<i64> {
        let (venue, quote, ve) = self.insts.get(ii).map(|r| (r.venue, r.quote, r.ve()))?;
        let ti = self
            .venues
            .get(usize::from(venue.0))?
            .insts
            .iter()
            .copied()
            .map(usize::from)
            .filter(|&k| self.insts.get(k).is_some_and(|r| !r.perp && r.base == token))
            .min_by_key(|&k| self.insts.get(k).is_none_or(|r| r.quote != quote))?;
        let pe = i32::from(self.insts.get(ti)?.px_exp);
        let mid_x2 = self.mid_x2(ti)?;
        if mid_x2 <= 0 {
            return None;
        }
        let qe = self.qty_exp(token);
        let n = 2 * v * i128::from(rate);
        let s = ve - 9 - pe - qe;
        let fee = if s >= 0 {
            div_round(n.saturating_mul(pow10(s as u32)), mid_x2, RoundMode::Ceil)
        } else {
            div_round(n, mid_x2.saturating_mul(pow10(s.unsigned_abs())), RoundMode::Ceil)
        };
        Some(sat64(fee))
    }

    /// Settles one fill; returns `(fee, fee_asset, realized_cash)`. Cash excludes fees. With
    /// `late_fee` the fee is charged at `due + 50 ms`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn settle_fill(
        &mut self,
        ai: usize,
        ii: usize,
        side: Side,
        px: i64,
        q: i64,
        rate: i64,
        due: u64,
    ) -> (i64, AssetId, i64) {
        let Some((venue, base, quote, ve, perp)) = self
            .insts
            .get(ii)
            .map(|r| (r.venue, r.base, r.quote, r.ve(), r.perp))
        else {
            return (0, AssetId(0), 0);
        };
        let v = i128::from(q) * i128::from(px);
        let qe_q = self.qty_exp(quote);
        let quote_fee = sat64(rescale(v * i128::from(rate), ve - 9, qe_q, RoundMode::Ceil));
        if perp {
            let cash = self.perp_fill(ai, ii, side, px, q);
            let masset = self.margin_asset(venue);
            self.charge(ai, venue, masset, quote_fee, due);
            return (quote_fee, masset, cash);
        }
        let base_fee = sat64(div_round(
            i128::from(q) * i128::from(rate),
            RATE_ONE,
            RoundMode::Ceil,
        ));
        let rule = self
            .venues
            .get(usize::from(venue.0))
            .map_or(FeeAssetRule::Quote, |v| v.fee_rule);
        let received = if side == Side::Buy {
            (base, base_fee)
        } else {
            (quote, quote_fee)
        };
        let (fee_asset, fee) = match rule {
            FeeAssetRule::Quote => (quote, quote_fee),
            FeeAssetRule::ReceivedAsset => received,
            FeeAssetRule::ReceivedAssetUnlessDiscount { token } => match self.token_fee(ii, token, v, rate) {
                Some(f) => {
                    let b = self.bal(ai, venue, token);
                    if b.total.saturating_sub(b.locked) >= f {
                        (token, f)
                    } else {
                        received
                    }
                }
                None => received,
            },
            FeeAssetRule::Configured(a) if a == quote => (quote, quote_fee),
            FeeAssetRule::Configured(a) => match self.token_fee(ii, a, v, rate) {
                Some(f) => (a, f),
                None => (quote, quote_fee),
            },
        };
        match side {
            Side::Buy => {
                let cost = sat64(rescale(v, ve, qe_q, RoundMode::Ceil));
                self.add_bal(ai, venue, quote, -cost);
                self.add_bal(ai, venue, base, q);
            }
            Side::Sell => {
                let gain = sat64(rescale(v, ve, qe_q, RoundMode::Floor));
                self.add_bal(ai, venue, base, -q);
                self.add_bal(ai, venue, quote, gain);
            }
        }
        self.charge(ai, venue, fee_asset, fee, due);
        (fee, fee_asset, 0)
    }

    /// Deducts a fee now, or at `due + 50 ms` with `late_fee`.
    fn charge(&mut self, ai: usize, venue: VenueId, asset: AssetId, fee: i64, due: u64) {
        if fee == 0 {
            return;
        }
        if self.tune.late_fee {
            let at = due.saturating_add(self.ms_to_tsc(50));
            if self.late_fees.len() < self.late_fees.capacity() {
                self.late_fees.push(LateFee {
                    at,
                    acct: ai as u8,
                    venue,
                    asset,
                    fee,
                });
                return;
            }
        }
        self.add_bal(ai, venue, asset, -fee);
    }

    pub(crate) fn charge_late_fees(&mut self, at: u64) {
        let mut k = 0;
        while k < self.late_fees.len() {
            if self.late_fees.get(k).is_some_and(|f| f.at <= at) {
                let f = self.late_fees.swap_remove(k);
                self.add_bal(usize::from(f.acct), f.venue, f.asset, -f.fee);
            } else {
                k += 1;
            }
        }
    }

    // ---- perpetual ----

    pub(crate) fn margin_asset(&self, venue: VenueId) -> AssetId {
        self.venues
            .get(usize::from(venue.0))
            .map_or(AssetId(0), |v| v.margin_asset)
    }

    /// Position update of a fill; realised PnL goes to the wallet. Removed cost and the wallet
    /// amount round toward zero, as hft-ledger books them (README "7. 永续全仓保证金与强平").
    fn perp_fill(&mut self, ai: usize, ii: usize, side: Side, px: i64, q: i64) -> i64 {
        let Some((venue, ve, quote)) = self.insts.get(ii).map(|r| (r.venue, r.ve(), r.quote)) else {
            return 0;
        };
        let qe_q = self.qty_exp(quote);
        let Some(p) = self.accts.get_mut(ai).and_then(|a| a.pos.get_mut(ii)) else {
            return 0;
        };
        let sgn = side.sign();
        let mut pnl: i128 = 0;
        let mut left = i128::from(q);
        if p.qty != 0 && p.qty.signum() != sgn {
            let abs = i128::from(p.qty).abs();
            let c = left.min(abs);
            let removed = if c == abs {
                p.cost
            } else {
                div_round(p.cost * c, abs, RoundMode::TowardZero)
            };
            let cp = c * i128::from(px);
            pnl = if p.qty > 0 { cp - removed } else { removed - cp };
            p.cost -= removed;
            p.qty = sat64(i128::from(p.qty) + c * i128::from(sgn));
            left -= c;
            if p.qty == 0 {
                p.cost = 0;
            }
        }
        if left > 0 {
            p.cost += left * i128::from(px);
            p.qty = sat64(i128::from(p.qty) + left * i128::from(sgn));
        }
        let cash = sat64(rescale(pnl, ve, qe_q, RoundMode::TowardZero));
        self.add_bal(ai, venue, quote, cash);
        cash
    }

    /// `|q| × px` value at `MARGIN_EXP` / leverage (ceiling) for `q` of inst `ii`.
    pub(crate) fn initial_e(&self, ai: usize, ii: usize, q: i128, px: i64) -> i128 {
        let lev = self.accts.get(ai).map_or(1, |a| i128::from(a.mm.leverage.max(1)));
        div_round(q.abs() * i128::from(px) * self.k_of(ii), lev, RoundMode::Ceil)
    }

    /// Maintenance rate of inst `ii` for a position value `val_e` (at `MARGIN_EXP`).
    fn mmr(&self, ai: usize, ii: usize, val_e: i128) -> i128 {
        let Some(a) = self.accts.get(ai) else {
            return 0;
        };
        match a.mm.mmr {
            MmrSource::Flat(r) => i128::from(r.0),
            MmrSource::RefdataTiers => {
                let Some(r) = self.insts.get(ii) else {
                    return 0;
                };
                let notional = rescale(val_e.abs(), MARGIN_EXP, NOTIONAL_EXP, RoundMode::Floor);
                r.tiers
                    .iter()
                    .find(|t| notional <= i128::from(t.notional_max.0))
                    .or_else(|| r.tiers.last())
                    .map_or(0, |t| i128::from(t.mmr.0))
            }
        }
    }

    /// `|q| × mark` (raw product) of position `p` of inst `ii`; the entry cost without a mark.
    fn pos_value(&self, ii: usize, p: &PosRow) -> i128 {
        match self.insts.get(ii) {
            Some(r) if r.mark_set => i128::from(p.qty).abs() * i128::from(r.mark),
            _ => p.cost,
        }
    }

    /// Cross-margin figures of `(ai, venue)`.
    pub(crate) fn margin_calc(&self, ai: usize, venue: VenueId) -> MarginCalc {
        let mut m = MarginCalc::default();
        let Some(a) = self.accts.get(ai) else {
            return m;
        };
        let Some(vr) = self.venues.get(usize::from(venue.0)) else {
            return m;
        };
        let qe = self.qty_exp(vr.margin_asset);
        m.wallet_e = rescale(
            i128::from(self.bal(ai, venue, vr.margin_asset).total),
            qe,
            MARGIN_EXP,
            RoundMode::Floor,
        );
        let lev = i128::from(a.mm.leverage.max(1));
        for &ii in &vr.insts {
            let ii = usize::from(ii);
            let Some(p) = a.pos.get(ii) else {
                continue;
            };
            if p.qty == 0 {
                continue;
            }
            m.n_pos += 1;
            let k = self.k_of(ii);
            let val = self.pos_value(ii, p) * k;
            let signed_val = if p.qty > 0 { val } else { -val };
            let signed_cost = if p.qty > 0 { p.cost * k } else { -p.cost * k };
            m.upnl_e += signed_val - signed_cost;
            m.maint9 += val * self.mmr(ai, ii, val);
            m.initial_e += div_round(val, lev, RoundMode::Ceil);
        }
        for o in a.orders.iter().filter(|o| o.used && o.resting) {
            let ii = usize::from(o.inst);
            if self.insts.get(ii).is_some_and(|r| r.perp && r.venue == venue) {
                m.occupy_e += self.initial_e(ai, ii, i128::from(o.remaining()), o.px);
            }
        }
        m.mb_e = m.wallet_e + m.upnl_e;
        m.avail_e = m.mb_e - m.initial_e - m.occupy_e;
        m
    }

    /// Liquidation price of inst `ii` (README "7.": other marks fixed; tiered mmr re-evaluated at
    /// most 3 times; short rounds up, long down; 0 = none).
    pub(crate) fn liq_px(&self, ai: usize, ii: usize) -> i64 {
        let Some(a) = self.accts.get(ai) else {
            return 0;
        };
        let Some(r) = self.insts.get(ii) else {
            return 0;
        };
        let Some(p) = a.pos.get(ii).copied() else {
            return 0;
        };
        if p.qty == 0 {
            return 0;
        }
        let m = self.margin_calc(ai, r.venue);
        let k = self.k_of(ii);
        let val = self.pos_value(ii, &p) * k;
        let signed_val = if p.qty > 0 { val } else { -val };
        let signed_cost = if p.qty > 0 { p.cost * k } else { -p.cost * k };
        let mmr_now = self.mmr(ai, ii, val);
        // Other insts: U≠ and M≠.
        let u_other = m.upnl_e - (signed_val - signed_cost);
        let m_other9 = m.maint9 - val * mmr_now;
        let q = i128::from(p.qty);
        let short = q < 0;
        // Exact trigger condition at mark `px` (other marks fixed).
        let cond = |px: i128| -> bool {
            let v = q.abs() * px * k;
            let sv = if short { -v } else { v };
            m_other9 + v * self.mmr(ai, ii, v) >= RATE_ONE * (m.wallet_e + u_other + sv - signed_cost)
        };
        // Root of the linear condition for a fixed rate (short rounds up, long down); None = no root.
        let root = |mmr: i128| -> Option<i128> {
            let mut num = RATE_ONE * (m.wallet_e + u_other - signed_cost) - m_other9;
            let mut den = k * (q.abs() * mmr - RATE_ONE * q);
            if den < 0 {
                num = -num;
                den = -den;
            }
            if den == 0 || num <= 0 {
                return None;
            }
            Some(div_round(
                num,
                den,
                if short { RoundMode::Ceil } else { RoundMode::Floor },
            ))
        };
        let (lo_mmr, hi_mmr) = match a.mm.mmr {
            MmrSource::Flat(x) => (i128::from(x.0), i128::from(x.0)),
            MmrSource::RefdataTiers => (
                r.tiers.first().map_or(0, |t| i128::from(t.mmr.0)),
                r.tiers.last().map_or(0, |t| i128::from(t.mmr.0)),
            ),
        };
        let px = if short {
            // Smallest mark that triggers: the minimum-rate root triggers, mark 0 does not.
            let Some(hi) = root(lo_mmr) else {
                return 0;
            };
            let (mut lo, mut hi) = (0i128, hi);
            if cond(lo) {
                return 0;
            }
            while hi - lo > 1 {
                let mid = lo + (hi - lo) / 2;
                if cond(mid) { hi = mid } else { lo = mid }
            }
            hi
        } else {
            // Largest mark that triggers, between the minimum- and maximum-rate roots.
            let Some(lo) = root(lo_mmr) else {
                return 0;
            };
            let top = root(hi_mmr).unwrap_or(lo).max(lo);
            if !cond(lo) {
                return sat64(lo.max(0));
            }
            let (mut lo, mut hi) = (lo, top + 1);
            while hi - lo > 1 {
                let mid = lo + (hi - lo) / 2;
                if cond(mid) { lo = mid } else { hi = mid }
            }
            lo
        };
        sat64(px.max(0))
    }

    /// `maint ≥ margin_balance` with at least one position.
    pub(crate) fn liquidation_due(&self, ai: usize, venue: VenueId) -> bool {
        let m = self.margin_calc(ai, venue);
        m.n_pos > 0 && m.maint9 >= m.mb_e * RATE_ONE
    }

    /// Closes every position of `(ai, venue)` at `mark × (1 ∓ slippage)` with the liquidation fee.
    pub(crate) fn liquidate(&mut self, ai: usize, venue: VenueId, t: u64, ex_ts: i64) {
        let Some((id, slip, fee_rate)) = self
            .accts
            .get(ai)
            .map(|a| (a.id, i128::from(a.mm.liq_slippage_bp), a.mm.liq_fee.0))
        else {
            return;
        };
        let due = self.resp_due(ai, t);
        let n = self.venues.get(usize::from(venue.0)).map_or(0, |v| v.insts.len());
        for j in 0..n {
            let Some(ii) = self
                .venues
                .get(usize::from(venue.0))
                .and_then(|v| v.insts.get(j))
                .copied()
            else {
                continue;
            };
            let ii = usize::from(ii);
            let qty = self
                .accts
                .get(ai)
                .and_then(|a| a.pos.get(ii))
                .map_or(0, |p| p.qty);
            if qty == 0 {
                continue;
            }
            let Some((mark, entry_cost)) = self.insts.get(ii).map(|r| (r.mark, r.mark_set)) else {
                continue;
            };
            let mark = if entry_cost {
                mark
            } else {
                let p = self
                    .accts
                    .get(ai)
                    .and_then(|a| a.pos.get(ii))
                    .copied()
                    .unwrap_or_default();
                sat64(div_round(p.cost, i128::from(p.qty).abs(), RoundMode::HalfEven))
            };
            let side = if qty > 0 { Side::Sell } else { Side::Buy };
            let m = i128::from(mark) * 10_000;
            let px = match side {
                Side::Buy => div_round(m * (10_000 + slip), 100_000_000, RoundMode::Ceil),
                Side::Sell => div_round(m * (10_000 - slip), 100_000_000, RoundMode::Floor),
            };
            let px = sat64(px);
            let q = qty.abs();
            let (fee, fee_asset, cash) = self.settle_fill(ai, ii, side, px, q, fee_rate, due);
            let inst = self.insts.get(ii).map_or(InstId(0), |r| r.id);
            let trade_id = self.accts.get_mut(ai).map_or(0, |a| {
                let x = a.next_trade_id;
                a.next_trade_id += 1;
                a.counters.liquidations += 1;
                x
            });
            let ef = ExternalFill {
                venue,
                account: id,
                side,
                cause: ExternalCause::Liquidation,
                inst,
                qty: Qty(q),
                px: Px(px),
                fee: Qty(fee),
                fee_asset,
                _pad: [0; 6],
                exch_order_id: 0,
                trade_id,
                ex_ts_ns: ex_ts,
                rx_tsc: Tsc(due),
                fill_seq: 0,
                cash_qty: Qty(q),
                cash_amount: Qty(cash),
            };
            self.push_reply(id, due, RepMsg::ExternalFill(ef), Bulk::None);
        }
    }

    /// `MarginReport` of `(ai, venue slot k)` by the venue's `margin_data` rule; `force` sends one.
    pub(crate) fn margin_check(&mut self, ai: usize, k: usize, t: u64, force: bool) {
        let Some(venue) = self.accts.get(ai).and_then(|a| a.venues.get(k)).copied() else {
            return;
        };
        let Some(vr) = self.venues.get(usize::from(venue.0)) else {
            return;
        };
        if !vr.perp {
            return;
        }
        let md = vr.margin_data;
        let m = self.margin_calc(ai, venue);
        let rep = self
            .accts
            .get(ai)
            .and_then(|a| a.mrep.get(k))
            .copied()
            .unwrap_or_default();
        if m.n_pos == 0 && !force {
            return;
        }
        let ratio = m.ratio();
        let since = t.saturating_sub(rep.last_tsc);
        let send = force
            || !rep.has
            || match md {
                MarginData::Push => {
                    i128::from(ratio).abs_diff(i128::from(rep.last_ratio)) >= 10_000_000
                        || since >= self.ms_to_tsc(1_000)
                }
                MarginData::Poll { interval_ms } => since >= self.ms_to_tsc(u64::from(interval_ms)),
                MarginData::NotApplicable => false,
            };
        if !send || md == MarginData::NotApplicable {
            return;
        }
        if let Some(r) = self.accts.get_mut(ai).and_then(|a| a.mrep.get_mut(k)) {
            *r = MarginRep {
                has: true,
                last_tsc: t,
                last_ratio: ratio,
            };
        }
        // Items.
        let slot = self.slab_free.pop();
        let mut n = 0usize;
        let mut truncated = false;
        let n_insts = self.venues.get(usize::from(venue.0)).map_or(0, |v| v.insts.len());
        for j in 0..n_insts {
            let Some(ii) = self
                .venues
                .get(usize::from(venue.0))
                .and_then(|v| v.insts.get(j))
                .copied()
            else {
                continue;
            };
            let ii = usize::from(ii);
            if self
                .accts
                .get(ai)
                .and_then(|a| a.pos.get(ii))
                .is_none_or(|p| p.qty == 0)
            {
                continue;
            }
            let lp = self.liq_px(ai, ii);
            let inst = self.insts.get(ii).map_or(InstId(0), |r| r.id);
            match slot.and_then(|s| self.margin_slab.get_mut(usize::from(s))) {
                Some(items) if n < MAX_MARGIN_ITEMS => {
                    if let Some(it) = items.get_mut(n) {
                        *it = MarginItem {
                            inst,
                            adl_quantile: 0,
                            _pad: [0; 3],
                            liq_px: Px(lp),
                        };
                    }
                    n += 1;
                }
                _ => truncated = true,
            }
        }
        if let Some(s) = slot
            && let Some(c) = self.margin_n.get_mut(usize::from(s))
        {
            *c = n as u16;
        }
        let Some(id) = self.accts.get(ai).map(|a| a.id) else {
            return;
        };
        let due = self.resp_due(ai, t);
        let h = MarginReportHdr {
            venue,
            account: id,
            source: if matches!(md, MarginData::Poll { .. }) {
                2
            } else {
                1
            },
            mflags: (if ratio >= RATE_ONE as i64 {
                MarginReportHdr::MARGIN_CALL
            } else {
                0
            }) | (if truncated {
                MarginReportHdr::ITEMS_TRUNCATED
            } else {
                0
            }),
            n: n as u16,
            _pad: 0,
            margin_balance: Notional(sat64(rescale(m.mb_e, MARGIN_EXP, NOTIONAL_EXP, RoundMode::Floor))),
            maint_margin: Notional(sat64(rescale(
                m.maint9,
                MARGIN_EXP - 9,
                NOTIONAL_EXP,
                RoundMode::Ceil,
            ))),
            avail_margin: Notional(sat64(rescale(
                m.avail_e,
                MARGIN_EXP,
                NOTIONAL_EXP,
                RoundMode::Floor,
            ))),
            margin_ratio: Rate(ratio),
            ex_ts_ns: 0,
            rx_tsc: Tsc(due),
            _rsv: 0,
        };
        let bulk = match slot {
            Some(s) => Bulk::Margin(s),
            None => Bulk::None,
        };
        self.push_reply(id, due, RepMsg::MarginReport(h), bulk);
    }

    /// Margin figures in the public units.
    pub(crate) fn margin_public(&self, ai: usize, venue: VenueId) -> crate::types::SimMargin {
        let m = self.margin_calc(ai, venue);
        let n = |v: i128, e: i32, md: RoundMode| Notional(sat64(rescale(v, e, NOTIONAL_EXP, md)));
        crate::types::SimMargin {
            wallet: n(m.wallet_e, MARGIN_EXP, RoundMode::Floor),
            unrealized: n(m.upnl_e, MARGIN_EXP, RoundMode::Floor),
            margin_balance: n(m.mb_e, MARGIN_EXP, RoundMode::Floor),
            maint: n(m.maint9, MARGIN_EXP - 9, RoundMode::Ceil),
            initial: n(m.initial_e, MARGIN_EXP, RoundMode::Ceil),
            avail: n(m.avail_e, MARGIN_EXP, RoundMode::Floor),
            margin_ratio: Rate(m.ratio()),
        }
    }

    // ---- funding ----

    /// Settles the listed insts of `venue` for every account (README "6.").
    pub(crate) fn settle_insts(&mut self, venue: VenueId, list: &[u16], t: u64) {
        let kind = self
            .venues
            .get(usize::from(venue.0))
            .map_or(FundingSettleKind::RateOnly, |v| v.funding_settle);
        let masset = self.margin_asset(venue);
        let qe_m = self.qty_exp(masset);
        for ai in 0..self.accts.len() {
            if self
                .accts
                .get(ai)
                .and_then(|a| a.vslot.get(usize::from(venue.0)))
                .is_none_or(|s| *s == u8::MAX)
            {
                continue;
            }
            let mut total: i128 = 0;
            let mut any = false;
            let mut due = None;
            for &ii in list {
                let ii = usize::from(ii);
                let Some(r) = self.insts.get(ii) else {
                    continue;
                };
                let (ve, inst, rate, settle_ns) = (r.ve(), r.id, r.rate, r.settled_ns);
                let Some(p) = self.accts.get(ai).and_then(|a| a.pos.get(ii)).copied() else {
                    continue;
                };
                if p.qty == 0 {
                    continue;
                }
                let mark = match self.insts.get(ii) {
                    Some(r) if r.mark_set => i128::from(r.mark),
                    _ => div_round(p.cost, i128::from(p.qty).abs(), RoundMode::HalfEven),
                };
                let raw = -i128::from(p.qty) * mark * i128::from(rate);
                let amount = sat64(rescale(raw, ve - 9, NOTIONAL_EXP, RoundMode::Floor));
                let wallet_d = sat64(rescale(raw, ve - 9, qe_m, RoundMode::Floor));
                self.add_bal(ai, venue, masset, wallet_d);
                total += i128::from(amount);
                any = true;
                if kind == FundingSettleKind::PerSymbolPush {
                    let d = match due {
                        Some(d) => d,
                        None => {
                            let d = self.resp_due(ai, t);
                            due = Some(d);
                            d
                        }
                    };
                    let Some(id) = self.accts.get(ai).map(|a| a.id) else {
                        continue;
                    };
                    let fs = FundingSettle {
                        venue,
                        account: id,
                        _pad: 0,
                        inst,
                        amount: Notional(amount),
                        rate: Rate(rate),
                        pos_qty: Qty(p.qty),
                        settle_ts_ns: settle_ns,
                        rx_tsc: Tsc(d),
                        synth: 0,
                        _pad2: [0; 7],
                    };
                    self.push_reply(id, d, RepMsg::FundingSettle(fs), Bulk::None);
                }
            }
            if any && kind == FundingSettleKind::BalanceOnlyPush {
                let d = self.resp_due(ai, t);
                let Some(id) = self.accts.get(ai).map(|a| a.id) else {
                    continue;
                };
                let settle_ns = list
                    .first()
                    .and_then(|ii| self.insts.get(usize::from(*ii)))
                    .map_or(0, |r| r.settled_ns);
                let fs = FundingSettle {
                    venue,
                    account: id,
                    _pad: 0,
                    inst: InstId(0),
                    amount: Notional(sat64(total)),
                    rate: Rate(0),
                    pos_qty: Qty(0),
                    settle_ts_ns: settle_ns,
                    rx_tsc: Tsc(d),
                    synth: 0,
                    _pad2: [0; 7],
                };
                self.push_reply(id, d, RepMsg::FundingSettle(fs), Bulk::None);
            }
        }
    }

    // ---- truth reports ----

    fn balance_report(&self, ai: usize, venue: VenueId, asset: AssetId, b: Bal, t: u64) -> BalanceReport {
        BalanceReport {
            venue,
            account: self.accts.get(ai).map_or(AccountId(0), |a| a.id),
            asset,
            _pad: 0,
            free: Qty(b.total.saturating_sub(b.locked)),
            locked: Qty(b.locked),
            ex_ts_ns: 0,
            rx_tsc: Tsc(t),
        }
    }

    /// `BalanceReport` per non-zero (or previously reported) balance, `PositionReport` per
    /// non-zero (or previously reported) position; `due = t`.
    pub(crate) fn truth_reports(&mut self, ai: usize, t: u64) {
        let n_assets = self.n_assets;
        let Some(nv) = self.accts.get(ai).map(|a| a.venues.len()) else {
            return;
        };
        let id = self.accts.get(ai).map_or(AccountId(0), |a| a.id);
        for k in 0..nv {
            let Some(venue) = self.accts.get(ai).and_then(|a| a.venues.get(k)).copied() else {
                continue;
            };
            for asset in 1..n_assets {
                let ix = k * n_assets + asset;
                let Some(b) = self.accts.get(ai).and_then(|a| a.bal.get(ix)).copied() else {
                    continue;
                };
                let nonzero = b.total != 0 || b.locked != 0;
                if !nonzero && !b.reported {
                    continue;
                }
                let br = self.balance_report(ai, venue, AssetId(asset as u16), b, t);
                if let Some(x) = self.accts.get_mut(ai).and_then(|a| a.bal.get_mut(ix)) {
                    x.reported = nonzero;
                }
                self.push_reply(id, t, RepMsg::BalanceReport(br), Bulk::None);
            }
        }
        let n_insts = self.insts.len();
        for ii in 0..n_insts {
            let Some(p) = self.accts.get(ai).and_then(|a| a.pos.get(ii)).copied() else {
                continue;
            };
            if p.qty == 0 && !p.reported {
                continue;
            }
            self.push_position_report(ai, ii, t);
        }
    }

    fn push_position_report(&mut self, ai: usize, ii: usize, t: u64) {
        let Some(p) = self.accts.get(ai).and_then(|a| a.pos.get(ii)).copied() else {
            return;
        };
        let Some((venue, inst, ve)) = self.insts.get(ii).map(|r| (r.venue, r.id, r.ve())) else {
            return;
        };
        let id = self.accts.get(ai).map_or(AccountId(0), |a| a.id);
        let entry = if p.qty == 0 {
            0
        } else {
            sat64(div_round(p.cost, i128::from(p.qty).abs(), RoundMode::HalfEven))
        };
        let val = match self.insts.get(ii) {
            Some(r) if r.mark_set => i128::from(p.qty) * i128::from(r.mark),
            _ => 0,
        };
        let signed_cost = if p.qty >= 0 { p.cost } else { -p.cost };
        let upnl = if p.qty == 0 || val == 0 {
            0
        } else {
            val - signed_cost
        };
        let pr = PositionReport {
            venue,
            account: id,
            _pad: 0,
            inst,
            qty: Qty(p.qty),
            entry_px: Px(entry),
            upnl: Notional(sat64(rescale(upnl, ve, NOTIONAL_EXP, RoundMode::Floor))),
            liq_px: Px(self.liq_px(ai, ii)),
            ex_ts_ns: 0,
            rx_tsc: Tsc(t),
        };
        if let Some(x) = self.accts.get_mut(ai).and_then(|a| a.pos.get_mut(ii)) {
            x.reported = p.qty != 0;
        }
        self.push_reply(id, t, RepMsg::PositionReport(pr), Bulk::None);
    }

    // ---- transfers and reset ----

    pub(crate) fn sim_transfer(&mut self, ai: usize, c: &hft_event::GwCmd, t: u64) {
        let asset = AssetId((c.arg & 0xFFFF) as u16);
        let from = VenueId(((c.arg >> 16) & 0xFF) as u8);
        let to = VenueId((c.arg >> 24) as u8);
        let amount = c.qty.0;
        let in_acct = |v: VenueId| {
            self.accts
                .get(ai)
                .and_then(|a| a.vslot.get(usize::from(v.0)))
                .is_some_and(|s| *s != u8::MAX)
        };
        if !in_acct(from) || !in_acct(to) || amount <= 0 || usize::from(asset.0) >= self.n_assets {
            return;
        }
        self.add_bal(ai, from, asset, -amount);
        let b = self.bal(ai, from, asset);
        let br = self.balance_report(ai, from, asset, b, t);
        if let Some(x) = self.bal_mut(ai, from, asset) {
            x.reported = b.total != 0 || b.locked != 0;
        }
        let id = self.accts.get(ai).map_or(AccountId(0), |a| a.id);
        self.push_reply(id, t, RepMsg::BalanceReport(br), Bulk::None);
        let at = t.saturating_add(self.ms_to_tsc(u64::from(self.tune.transfer_delay_ms)));
        if self.transfers.len() < self.transfers.capacity() {
            self.transfers.push(PendTransfer {
                at,
                acct: ai as u8,
                venue: to,
                asset,
                amount,
            });
        }
    }

    /// Credits every transfer due at `at`.
    pub(crate) fn land_transfers(&mut self, at: u64) {
        let mut k = 0;
        while k < self.transfers.len() {
            if self.transfers.get(k).is_some_and(|x| x.at <= at) {
                let x = self.transfers.swap_remove(k);
                let ai = usize::from(x.acct);
                self.add_bal(ai, x.venue, x.asset, x.amount);
                let b = self.bal(ai, x.venue, x.asset);
                let br = self.balance_report(ai, x.venue, x.asset, b, x.at);
                if let Some(y) = self.bal_mut(ai, x.venue, x.asset) {
                    y.reported = b.total != 0 || b.locked != 0;
                }
                let id = self.accts.get(ai).map_or(AccountId(0), |a| a.id);
                self.push_reply(id, x.at, RepMsg::BalanceReport(br), Bulk::None);
            } else {
                k += 1;
            }
        }
    }

    /// `SimReset{account}` (README "8."): cancel every resting order, balances back to
    /// `initial_balances`, positions and memory cleared, truth reports and `AccountConfigReport`s.
    pub(crate) fn sim_reset(&mut self, ai: usize, t: u64) {
        let n = self.accts.get(ai).map_or(0, |a| a.orders.len());
        for k in 0..n {
            let o = self.order_at(ai, k);
            if !(o.used && o.resting) {
                continue;
            }
            let ex = self.inst_ex_ts(usize::from(o.inst));
            self.finish(ai, k, st::CANCELLED);
            let o = self.order_at(ai, k);
            let ev = self.order_event(
                ai,
                &o,
                hft_event::OrderEventKind::CancelAck,
                o.px,
                o.remaining(),
                0,
                AssetId(0),
                0,
                0,
                ex,
                t,
            );
            self.emit(ai, ev, t);
        }
        // Every balance and position that changes to its reset value is reported once.
        if let Some(a) = self.accts.get_mut(ai) {
            for b in a.bal.iter_mut() {
                b.reported |= b.total != 0 || b.locked != 0;
            }
            for p in a.pos.iter_mut() {
                p.reported |= p.qty != 0;
            }
        }
        self.reset_balances(ai);
        self.transfers.retain(|x| usize::from(x.acct) != ai);
        self.late_fees.retain(|x| usize::from(x.acct) != ai);
        if let Some(a) = self.accts.get_mut(ai) {
            for p in a.pos.iter_mut() {
                p.qty = 0;
                p.cost = 0;
            }
            for e in a.mem.iter_mut() {
                *e = MemEntry::default();
            }
            for r in a.rate.iter_mut() {
                *r = [RateWin::default(); 4];
            }
            for m in a.mrep.iter_mut() {
                *m = MarginRep::default();
            }
        }
        self.truth_reports(ai, t);
        let lev = self.accts.get(ai).map_or(1, |a| a.mm.leverage);
        self.push_config_reports(ai, t, lev);
    }

    // ---- seeding ----

    /// `seed_account_from` (README "重启恢复").
    pub(crate) fn seed(&mut self, ai: usize, s: &SimSeedState) -> Result<(), SimCfgErr> {
        let in_acct = |v: VenueId| {
            self.accts
                .get(ai)
                .and_then(|a| a.vslot.get(usize::from(v.0)))
                .is_some_and(|x| *x != u8::MAX)
        };
        for (v, asset, _) in &s.balances {
            if !in_acct(*v) {
                return Err(SimCfgErr::UnknownVenue);
            }
            if asset.0 == 0 || !self.asset_exp.get(usize::from(asset.0)).is_some_and(|e| e.2) {
                return Err(SimCfgErr::UnknownAsset);
            }
        }
        for (i, _, _) in &s.positions {
            let ok = self
                .inst_index(*i)
                .and_then(|ii| self.insts.get(ii))
                .is_some_and(|r| in_acct(r.venue));
            if !ok {
                return Err(SimCfgErr::UnknownVenue);
            }
        }
        for o in &s.orders {
            let ok = self
                .inst_index(o.inst)
                .and_then(|ii| self.insts.get(ii))
                .is_some_and(|r| in_acct(r.venue) && r.venue == o.venue);
            if !ok {
                return Err(SimCfgErr::UnknownVenue);
            }
            let perp = self
                .inst_index(o.inst)
                .and_then(|ii| self.insts.get(ii))
                .is_some_and(|r| r.perp);
            if o.cash_qty.0 < 0
                || o.cash_qty.0 > o.filled.0
                || (o.cash_qty.0 == 0 && o.cash_sum.0 != 0)
                || (!perp && (o.cash_qty.0 != 0 || o.cash_sum.0 != 0))
            {
                return Err(SimCfgErr::SettlementFacts);
            }
        }
        // Transients of the account.
        self.cmds.retain(|c| usize::from(c.0.acct) != ai);
        let id = self.accts.get(ai).map_or(AccountId(0), |a| a.id);
        let mut freed: Vec<u16> = Vec::new();
        self.replies.retain(|r| {
            if r.0.account == id && !r.0.keep {
                if let Bulk::Margin(k) = r.0.bulk {
                    freed.push(k);
                }
                false
            } else {
                true
            }
        });
        self.slab_free.extend(freed);
        self.transfers.retain(|x| usize::from(x.acct) != ai);
        self.late_fees.retain(|x| usize::from(x.acct) != ai);
        let seed = self.tune.seed;
        let n_assets = self.n_assets;
        let n_insts = self.insts.len();
        let Some(a) = self.accts.get_mut(ai) else {
            return Ok(());
        };
        a.rng = account_rng(seed, id.0);
        for b in a.bal.iter_mut() {
            *b = Bal::default();
        }
        for p in a.pos.iter_mut() {
            *p = PosRow::default();
        }
        for e in a.mem.iter_mut() {
            *e = MemEntry::default();
        }
        for o in a.orders.iter_mut() {
            *o = Order::EMPTY;
        }
        a.n_resting = 0;
        for r in a.rate.iter_mut() {
            *r = [RateWin::default(); 4];
        }
        for m in a.mrep.iter_mut() {
            *m = MarginRep::default();
        }
        for (v, asset, q) in &s.balances {
            let Some(&vs) = a.vslot.get(usize::from(v.0)) else {
                continue;
            };
            if let Some(b) = a.bal.get_mut(usize::from(vs) * n_assets + usize::from(asset.0)) {
                b.total = q.0;
            }
        }
        let _ = n_insts;
        for (i, q, px) in &s.positions {
            let Some(ii) = self.inst_index(*i) else {
                continue;
            };
            if let Some(p) = self.accts.get_mut(ai).and_then(|a| a.pos.get_mut(ii)) {
                p.qty = q.0;
                p.cost = i128::from(q.0).abs() * i128::from(px.0);
            }
        }
        for (i, next, rate, mark) in &s.funding {
            let Some(ii) = self.inst_index(*i) else {
                continue;
            };
            if let Some(r) = self.insts.get_mut(ii) {
                if r.next_funding_ns == 0 {
                    r.next_funding_ns = *next;
                    r.rate = rate.0;
                }
                if !r.mark_set && mark.0 > 0 {
                    r.mark = mark.0;
                    r.mark_set = true;
                }
            }
        }
        for so in &s.orders {
            let Some(ii) = self.inst_index(so.inst) else {
                continue;
            };
            let Some(k) = self.alloc_order(ai) else {
                break;
            };
            let resting = matches!(so.tif, Tif::Gtc | Tif::PostOnly) && matches!(so.prev_state, 2..=4);
            let Some(a) = self.accts.get_mut(ai) else {
                break;
            };
            a.ins_seq += 1;
            let o = Order {
                used: true,
                resting: false,
                status: 0,
                slot: so.slot.0,
                r#gen: so.r#gen,
                inst: ii as u16,
                side: so.side,
                tif: so.tif,
                px: so.px.0,
                qty: so.qty.0,
                filled: so.filled.0,
                fill_val: i128::from(so.filled.0) * i128::from(so.avg_px.0),
                cash_qty: so.cash_qty.0,
                cash_sum: so.cash_sum.0,
                exch_id: so.exch_order_id,
                ins_seq: a.ins_seq,
                term_seq: 0,
                last_ex_ts: 0,
                last_due: 0,
                lock: 0,
                lock_asset: AssetId(0),
            };
            if let Some(x) = a.orders.get_mut(k) {
                *x = o;
            }
            if resting && o.remaining() > 0 {
                if let Some(a) = self.accts.get_mut(ai) {
                    if let Some(x) = a.orders.get_mut(k) {
                        x.resting = true;
                        x.status = if o.filled > 0 { st::PARTIAL } else { st::NEW };
                    }
                    a.n_resting += 1;
                }
                let (_, amt) = self.lock_for(ai, ii, o.side, o.px, o.remaining());
                self.set_lock(ai, k, amt);
            } else {
                let status = if o.filled >= o.qty {
                    st::FILLED
                } else {
                    st::EXPIRED
                };
                self.finish(ai, k, status);
            }
        }
        Ok(())
    }
}
