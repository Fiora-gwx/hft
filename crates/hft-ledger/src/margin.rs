//! Margin data: report values, estimates, the two liquidation distances and the oldest mark time;
//! `on_mark` and `restore_marks` (README "保证金数据").

use hft_event::{MarginItem, MarginReportHdr};
use hft_types::{AccountId, InstId, Notional, Px, Rate, RoundMode, VenueId};
use zerocopy::FromBytes;

use crate::fixed::{FixedVec, div_round, sat64};
use crate::ledger::Ledger;
use crate::types::*;

/// A `MarginReport`: header and its items (`n × 16 B`, `MarginItem` layout).
#[derive(Copy, Clone, Debug)]
pub struct MarginReportView<'a> {
    pub hdr: MarginReportHdr,
    pub items: &'a [u8],
}

impl<'a> MarginReportView<'a> {
    /// Item `k` (by value; the tail need not be aligned).
    pub fn item(&self, k: usize) -> Option<MarginItem> {
        let b = self.items.get(k * 16..k * 16 + 16)?;
        MarginItem::read_from_bytes(b).ok()
    }

    pub fn n_items(&self) -> usize {
        (self.items.len() / 16).min(usize::from(self.hdr.n))
    }
}

const RATE_ONE: i128 = 1_000_000_000;

fn rate_of(num: i128, den: i128) -> Rate {
    if den <= 0 {
        return Rate::MAX;
    }
    Rate(sat64(div_round(
        num.saturating_mul(RATE_ONE),
        den,
        RoundMode::HalfEven,
    )))
}

impl Ledger {
    /// Recomputes `upnl`, the estimates, both liquidation distances and `mark_oldest_ens` of a
    /// perpetual (account, venue) from its open positions.
    pub(crate) fn margin_refresh(&mut self, av: usize) {
        let Some(d) = self.avd.get(av) else { return };
        let mut upnl = 0i64;
        let mut big_n = 0i64;
        let mut oldest = i64::MAX;
        let mut rep_min: Option<i128> = None;
        let mut missing = false;
        let n_open = d.open_perps.len();
        let has_report = self.books.get(av).is_some_and(|b| b.margin.has_report);
        for k in 0..n_open {
            let Some(p) = self
                .avd
                .get(av)
                .and_then(|d| d.open_perps.get(k))
                .map(|x| *x as usize)
            else {
                continue;
            };
            let Some(x) = self.pos.get(p).copied() else {
                continue;
            };
            if x.qty.0 == 0 {
                continue;
            }
            let Some(inst) = self.inst_at(av, p) else { continue };
            upnl = upnl.saturating_add(self.posd.get(p).map_or(0, |d| d.upnl.0));
            big_n = big_n.saturating_add(self.notional_of(inst, x.qty).0);
            let ip = self.ipx.get(inst.0 as usize).copied();
            let mens = ip.filter(|p| p.has_mark).map_or(0, |p| p.mark_ens);
            oldest = oldest.min(mens);
            let m = self.val_px(inst, x.qty.0).map_or(0, |p| i128::from(p.0));
            let liq = self
                .books
                .get(av)
                .and_then(|b| {
                    b.margin
                        .items
                        .iter()
                        .find(|it| it.inst == inst)
                        .map(|it| it.liq_px.0)
                })
                .unwrap_or(0);
            if liq == 0 || !has_report {
                missing = true;
                continue;
            }
            if m <= 0 {
                continue;
            }
            let l = i128::from(liq);
            let num = if x.qty.0 > 0 { m - l } else { l - m };
            // Distance as a 1e-9 rate (HalfEven), crossed → 0.
            let dist = if num <= 0 {
                0
            } else {
                div_round(num * RATE_ONE, m, RoundMode::HalfEven)
            };
            rep_min = Some(rep_min.map_or(dist, |v| v.min(dist)));
        }
        let Some(b) = self.books.get_mut(av) else { return };
        let ms = &mut b.margin;
        ms.upnl = Notional(upnl);
        ms.est_margin_balance = Notional(ms.wallet.0.saturating_add(upnl));
        ms.mark_oldest_ens = oldest;
        ms.rep_liq_dist = rep_min.map_or(Rate::MAX, |v| Rate(sat64(v)));
        if missing {
            ms.lflags |= lflags::LIQ_PX_MISSING;
        } else {
            ms.lflags &= !lflags::LIQ_PX_MISSING;
        }
        if !ms.has_report {
            ms.est_ratio = Rate(0);
            ms.est_liq_dist = Rate::MAX;
            return;
        }
        let mb = i128::from(ms.est_margin_balance.0);
        let maint = i128::from(ms.rep_maint.0);
        let n = i128::from(big_n);
        let rn = i128::from(ms.rep_perp_notional.0);
        // M = maint × max(1, N / rep_N) as the fraction (m_num / m_den).
        let (m_num, m_den) = if rn > 0 && n > rn {
            (maint * n, rn)
        } else {
            (maint, 1)
        };
        ms.est_ratio = if mb <= 0 {
            Rate::MAX
        } else {
            rate_of(m_num, m_den.saturating_mul(mb))
        };
        ms.est_liq_dist = if n_open == 0 || n == 0 {
            Rate::MAX
        } else {
            // (MB − M) / (N + M) = (MB·den − m_num) / (N·den + m_num)
            let num = mb * m_den - m_num;
            let den = n * m_den + m_num;
            if num <= 0 { Rate(0) } else { rate_of(num, den) }
        };
    }

    /// Inst of position index `p` within (account, venue) `av`.
    pub(crate) fn inst_at(&self, av: usize, p: usize) -> Option<InstId> {
        let s = self.avs.get(av)?;
        let local = p.checked_sub(s.pos_base as usize)?;
        let v = self.venues.get(usize::from(s.venue.0))?;
        let _ = v;
        // Insts of a venue are dense in `insts`; find by local index (cold enough: ≤ venue size).
        self.local_inst(s.venue, local as u32)
    }

    pub(crate) fn local_inst(&self, v: VenueId, local: u32) -> Option<InstId> {
        self.venue_insts
            .get(usize::from(v.0))
            .and_then(|l| l.get(local as usize))
            .copied()
    }

    /// Stores a `MarginReport`, the venue's perpetual notional at this moment, and recomputes.
    pub fn on_margin(&mut self, m: &MarginReportView<'_>, t: i64) {
        self.tick(t);
        let Some(av) = self.av(m.hdr.account, m.hdr.venue) else {
            return;
        };
        let mut items = FixedVec::new();
        for k in 0..m.n_items() {
            if let Some(it) = m.item(k) {
                items.push(MarginItemRow {
                    inst: it.inst,
                    liq_px: it.liq_px,
                    adl_quantile: it.adl_quantile,
                });
            }
        }
        // Σ |qty| × mark of the open positions now.
        let mut big_n = 0i64;
        let n_open = self.avd.get(av).map_or(0, |d| d.open_perps.len());
        for k in 0..n_open {
            let Some(p) = self
                .avd
                .get(av)
                .and_then(|d| d.open_perps.get(k))
                .map(|x| *x as usize)
            else {
                continue;
            };
            let (Some(x), Some(inst)) = (self.pos.get(p).copied(), self.inst_at(av, p)) else {
                continue;
            };
            big_n = big_n.saturating_add(self.notional_of(inst, x.qty).0);
        }
        if let Some(b) = self.books.get_mut(av) {
            let ms = &mut b.margin;
            ms.has_report = true;
            ms.rep_margin_balance = m.hdr.margin_balance;
            ms.rep_maint = m.hdr.maint_margin;
            ms.rep_avail = m.hdr.avail_margin;
            ms.rep_ratio = m.hdr.margin_ratio;
            ms.rep_ens = t;
            ms.rep_perp_notional = Notional(big_n);
            ms.items = items;
        }
        self.margin_refresh(av);
        self.refresh_summary(av);
    }

    /// Mark price of a perpetual: unrealized of its holders, margin estimates, liquidation
    /// distances and mark times; affected (account, venue) go to `touched`.
    pub fn on_mark(
        &mut self,
        inst: InstId,
        mark: Px,
        t: i64,
        touched: &mut FixedVec<(AccountId, VenueId), 4>,
    ) {
        self.upnl_dirty = [0; 4];
        self.tick(t);
        touched.clear();
        if let Some(p) = self.ipx.get_mut(inst.0 as usize) {
            p.mark = mark;
            p.mark_ens = t;
            p.has_mark = true;
        }
        self.revalue_inst(inst, t, Some(touched), None);
    }

    /// After `restore`: rebuilds the caches that are not in the snapshot (holder unrealized
    /// caches, margin estimates, liquidation distances, mark times) from the latest marks; no PnL
    /// accumulator changes.
    pub fn restore_marks(&mut self, marks: &[(InstId, Px, i64)]) {
        for &(i, px, ens) in marks {
            if let Some(p) = self.ipx.get_mut(i.0 as usize) {
                p.mark = px;
                p.mark_ens = ens;
                p.has_mark = true;
            }
        }
        // Position caches (account upnl is in the snapshot: set caches without deltas).
        for av in 0..self.avs.len() {
            let Some(s) = self.avs.get(av).copied() else {
                continue;
            };
            let n = self.venues.get(usize::from(s.venue.0)).map_or(0, |v| v.n_insts) as usize;
            for p in (s.pos_base as usize)..(s.pos_base as usize + n) {
                let Some(x) = self.pos.get(p).copied() else {
                    continue;
                };
                let Some(inst) = self.inst_at(av, p) else { continue };
                let u = self.upnl_of(inst, x.qty, x.cost);
                let nn = self.notional_of(inst, x.qty);
                if let Some(d) = self.posd.get_mut(p) {
                    d.upnl = u;
                    d.notional = nn;
                }
                self.track_open_perp(av, p);
            }
        }
        for c in 0..self.cells.len() {
            for l in 0..4 {
                let Some(leg) = self.cells.get(c).and_then(|x| x.legs.get(l)).copied() else {
                    continue;
                };
                let u = self.upnl_of(leg.inst, leg.net, leg.cost);
                if let Some(x) = self.cells.get_mut(c).and_then(|x| x.leg_upnl.get_mut(l)) {
                    *x = u;
                }
            }
        }
        for av in 0..self.avs.len() {
            self.margin_refresh(av);
            self.refresh_summary(av);
        }
    }
}
