//! USDT conversion rates, valuation prices, the per-venue summary rows and `on_ref_px`
//! (README "USDT 统一计价", "权益数据").

use hft_types::{AssetId, InstId, Notional, Px, Qty, RoundMode};

use crate::config::{PerpVal, SpotVal};
use crate::fixed::{rescale, sat64};
use crate::ledger::Ledger;
use crate::types::vflags;
use crate::views::VenueEquityView;

/// A price as `raw × 10^exp` (USDT or quote units).
#[derive(Copy, Clone, Debug)]
pub(crate) struct Price {
    pub raw: i128,
    pub exp: i32,
}

impl Price {
    pub const ONE: Price = Price { raw: 1, exp: 0 };
}

impl Ledger {
    /// Stale: not par and (no quote, older than the run start, or older than `rate_max_age_ms`).
    pub(crate) fn rate_stale(&self, asset: AssetId) -> bool {
        let s = self.asset_s(asset);
        if s.par {
            return false;
        }
        let Some(r) = self.rates.get(usize::from(asset.0)) else {
            return true;
        };
        !r.has || r.ens < self.run_start || self.now.saturating_sub(r.ens) > self.cfg.rate_max_age_ns
    }

    pub(crate) fn recompute_next_stale(&mut self) {
        let mut next = i64::MAX;
        for (ix, r) in self.rates.iter().enumerate() {
            let s = self.assets.get(ix).copied();
            if s.is_some_and(|s| s.par) || !r.has || r.ens < self.run_start {
                continue;
            }
            let at = r.ens.saturating_add(self.cfg.rate_max_age_ns);
            if at >= self.now && at < next {
                next = at;
            }
        }
        self.next_stale = next;
    }

    /// USDT rate of `asset`: par 1; fresh → mid (floored); stale → stable fallback 1.0, else the
    /// old mid (0 if never seen).
    pub(crate) fn rate_px(&self, asset: AssetId) -> Price {
        let s = self.asset_s(asset);
        if s.par {
            return Price::ONE;
        }
        let r = self.rates.get(usize::from(asset.0));
        let stale = self.rate_stale(asset);
        if stale && s.fallback {
            return Price::ONE;
        }
        match r {
            Some(r) if r.has => {
                let ref_exp = self
                    .inst(s.ref_inst)
                    .map_or(i32::from(s.px_exp), |i| i32::from(i.px_exp));
                Price {
                    raw: i128::from(Px::mid_floor(r.bid, r.ask).0),
                    exp: ref_exp,
                }
            }
            _ => Price { raw: 0, exp: 0 },
        }
    }

    /// `amount × 10^exp` of `asset` in USDT `Notional`, rounded by `m`.
    pub(crate) fn usdt(&mut self, asset: AssetId, amount: i128, exp: i32, m: RoundMode) -> Notional {
        let r = self.rate_px(asset);
        let (v, lossy) = rescale(amount.saturating_mul(r.raw), exp + r.exp, -6, m);
        if lossy {
            self.rounding = self.rounding.saturating_add(1);
        }
        Notional(sat64(v))
    }

    /// Same as `usdt` without counting (read paths).
    pub(crate) fn usdt_ro(&self, asset: AssetId, amount: i128, exp: i32, m: RoundMode) -> Notional {
        let r = self.rate_px(asset);
        Notional(sat64(rescale(amount.saturating_mul(r.raw), exp + r.exp, -6, m).0))
    }

    /// Valuation price of `inst` for a position of sign `sign` (quote units, inst `px_exp`);
    /// falls back to the last fill price.
    pub(crate) fn val_px(&self, inst: InstId, sign: i64) -> Option<Px> {
        let s = self.inst(inst)?;
        let p = self.ipx.get(inst.0 as usize)?;
        let v = if s.perp {
            match self.cfg.valuation.perp {
                PerpVal::Mark => p.has_mark.then_some(p.mark),
                PerpVal::ExitSide => p.has_bbo.then_some(if sign >= 0 { p.bid } else { p.ask }),
            }
        } else {
            p.has_bbo.then(|| match self.cfg.valuation.spot {
                SpotVal::Bid => p.bid,
                SpotVal::Mid => Px::mid_floor(p.bid, p.ask),
            })
        };
        v.or((p.last_px.0 != 0).then_some(p.last_px))
    }

    /// The valuation price of `inst` is its last trade price (the mark / BBO it is valued at is
    /// not known yet).
    pub(crate) fn val_by_last_px(&self, inst: InstId) -> bool {
        let (Some(s), Some(p)) = (self.inst(inst), self.ipx.get(inst.0 as usize)) else {
            return false;
        };
        if s.perp {
            match self.cfg.valuation.perp {
                PerpVal::Mark => !p.has_mark,
                PerpVal::ExitSide => !p.has_bbo,
            }
        } else {
            !p.has_bbo
        }
    }

    /// Unrealized of `qty` with absolute `cost` at the valuation price, in USDT.
    pub(crate) fn upnl_of(&self, inst: InstId, qty: Qty, cost: i128) -> Notional {
        if qty.0 == 0 {
            return Notional(0);
        }
        let (Some(s), Some(px)) = (self.inst(inst), self.val_px(inst, qty.0)) else {
            return Notional(0);
        };
        let value = i128::from(qty.0).saturating_mul(i128::from(px.0));
        let u = if qty.0 > 0 { value - cost } else { value + cost };
        self.usdt_ro(
            s.quote,
            u,
            i32::from(s.px_exp) + i32::from(s.qty_exp),
            RoundMode::TowardZero,
        )
    }

    /// `|qty| × valuation price` in USDT.
    pub(crate) fn notional_of(&self, inst: InstId, qty: Qty) -> Notional {
        if qty.0 == 0 {
            return Notional(0);
        }
        let (Some(s), Some(px)) = (self.inst(inst), self.val_px(inst, qty.0)) else {
            return Notional(0);
        };
        let v = i128::from(qty.0).abs().saturating_mul(i128::from(px.0));
        self.usdt_ro(
            s.quote,
            v,
            i32::from(s.px_exp) + i32::from(s.qty_exp),
            RoundMode::TowardZero,
        )
    }

    /// USDT price of one unit of a base asset (valuation.spot on its reference inst).
    pub(crate) fn spot_price(&self, asset: AssetId) -> Price {
        let s = self.asset_s(asset);
        if s.par {
            return Price::ONE;
        }
        if s.fallback && self.rate_stale(asset) {
            return Price::ONE;
        }
        let Some(ri) = self.inst(s.ref_inst) else {
            return self.rate_px(asset);
        };
        let p = self.ipx.get(s.ref_inst.0 as usize);
        let px = match p {
            Some(p) if p.has_bbo => match self.cfg.valuation.spot {
                SpotVal::Bid => p.bid,
                SpotVal::Mid => Px::mid_floor(p.bid, p.ask),
            },
            _ => return self.rate_px(asset),
        };
        let q = self.rate_px(ri.quote);
        Price {
            raw: i128::from(px.0).saturating_mul(q.raw),
            exp: i32::from(ri.px_exp) + q.exp,
        }
    }

    /// Recomputes the summary row and flags of `(account, venue)` index `av`.
    pub(crate) fn refresh_summary(&mut self, av: usize) {
        let Some(s) = self.avs.get(av).copied() else {
            return;
        };
        let Some(vs) = self.venues.get(usize::from(s.venue.0)) else {
            return;
        };
        let perp = vs.perp;
        let Some(b) = self.books.get(av) else { return };
        let inc_base = self.cfg.valuation.include_baseline;
        let inc_un = self.cfg.valuation.include_unassigned;
        let mut quote_cash = 0i64;
        let mut spot_value = 0i64;
        let mut stale = false;
        for e in b.cash.iter() {
            let a = self.asset_s(e.asset);
            if !a.par && self.rate_stale(e.asset) {
                stale = true;
            }
            if self.is_quote(s.venue, e.asset) {
                let v = self.usdt_ro(
                    e.asset,
                    i128::from(e.total.0),
                    i32::from(a.qty_exp),
                    RoundMode::TowardZero,
                );
                quote_cash = quote_cash.saturating_add(v.0);
            } else {
                let mut q = b
                    .inv
                    .iter()
                    .filter(|x| x.asset == e.asset)
                    .fold(0i64, |acc, x| acc.saturating_add(x.qty.0));
                if inc_un {
                    q = q.saturating_add(e.unassigned.0);
                }
                if inc_base {
                    q = q.saturating_add(e.baseline.0);
                }
                if q != 0 {
                    let p = self.spot_price(e.asset);
                    let (v, _) = rescale(
                        i128::from(q).saturating_mul(p.raw),
                        i32::from(a.qty_exp) + p.exp,
                        -6,
                        RoundMode::TowardZero,
                    );
                    spot_value = spot_value.saturating_add(sat64(v));
                }
            }
        }
        let m = &b.margin;
        let perp_upnl = if perp { m.upnl.0 } else { 0 };
        let mut flags = 0u16;
        if stale {
            flags |= VenueEquityView::RATE_STALE;
        }
        if perp && (!m.has_report || m.rep_ens < self.run_start) {
            flags |= VenueEquityView::MARGIN_STALE;
        }
        if b.vflags & vflags::DRIFT != 0 {
            flags |= VenueEquityView::BALANCE_UNRECONCILED;
        }
        let mb = if perp { m.est_margin_balance } else { Notional(0) };
        let now = self.now;
        if let Some(b) = self.books.get_mut(av) {
            let sm = &mut b.summary;
            sm.quote_cash = Notional(quote_cash);
            sm.spot_value = Notional(spot_value);
            sm.perp_upnl = Notional(perp_upnl);
            sm.equity = Notional(quote_cash.saturating_add(spot_value).saturating_add(perp_upnl));
            sm.margin_balance = mb;
            sm.flags = flags;
            sm.as_of_ens = now;
            if stale {
                b.vflags |= vflags::STALE_RATE;
            } else {
                b.vflags &= !vflags::STALE_RATE;
            }
        }
    }

    /// Reference quote of `inst`: conversion rate (when it is a reference inst), spot valuation
    /// and unrealized of its holders.
    pub fn on_ref_px(&mut self, inst: InstId, bid: Px, ask: Px, t: i64) {
        self.tick(t);
        let Some(s) = self.inst(inst).copied() else { return };
        if let Some(p) = self.ipx.get_mut(inst.0 as usize) {
            p.bid = bid;
            p.ask = ask;
            p.has_bbo = true;
        }
        if s.ref_of.0 != 0 {
            if let Some(r) = self.rates.get_mut(usize::from(s.ref_of.0)) {
                r.bid = bid;
                r.ask = ask;
                r.ens = t;
                r.has = true;
            }
            self.recompute_next_stale();
        }
        if !s.perp || self.cfg.valuation.perp == PerpVal::ExitSide {
            self.revalue_inst(inst, t, None, None);
        }
        if s.ref_of.0 != 0 {
            self.refresh_all_summaries_with(s.ref_of);
        }
    }

    fn refresh_all_summaries_with(&mut self, asset: AssetId) {
        // A rate change can move the value of every venue holding the asset and the staleness
        // flags of every venue.
        let _ = asset;
        for av in 0..self.avs.len() {
            self.refresh_summary(av);
        }
    }
}
