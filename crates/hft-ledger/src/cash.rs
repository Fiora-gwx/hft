//! Cash, inventory attribution and fee buckets; balance initialization, checks and baselines;
//! transfers and Sim resets (README "仓位与已实现" steps 4–5, "余额初始化、校验与基线", "划转与模拟账户").

use hft_event::{BalanceReport, NoteLevel};
use hft_types::{AccountId, AssetId, GROUP_NONE, GroupId, Notional, Qty, RoundMode, VenueId};

use crate::fixed::FixedVec;
use crate::ledger::{Ledger, PosDyn};
use crate::notes::{code, note};
use crate::types::*;

impl Ledger {
    /// Index of the cash entry of `asset` in `(account, venue)` `av`, created when missing.
    pub(crate) fn cash_ix(&mut self, av: usize, asset: AssetId) -> Option<usize> {
        let b = self.books.get_mut(av)?;
        if let Some(k) = b.cash.position(|e| e.asset == asset) {
            return Some(k);
        }
        // Keep the entries sorted by asset.
        let at = b
            .cash
            .iter()
            .position(|e| e.asset.0 > asset.0)
            .unwrap_or(b.cash.len());
        if b.cash.insert(at, CashEntry::new(asset)) {
            Some(at)
        } else {
            None
        }
    }

    pub(crate) fn cash_mut(&mut self, av: usize, asset: AssetId) -> Option<&mut CashEntry> {
        let k = self.cash_ix(av, asset)?;
        self.books.get_mut(av)?.cash.get_mut(k)
    }

    /// `total += delta` of a quote/margin asset; the margin wallet follows its margin asset.
    pub(crate) fn quote_add(&mut self, av: usize, asset: AssetId, delta: i64) {
        if delta == 0 {
            return;
        }
        if let Some(e) = self.cash_mut(av, asset) {
            e.total = Qty(e.total.0.saturating_add(delta));
        }
        self.wallet_follow(av, asset, delta);
    }

    /// Moves the margin wallet by the USDT value of `delta` of the venue's margin asset.
    pub(crate) fn wallet_follow(&mut self, av: usize, asset: AssetId, delta: i64) {
        let Some(s) = self.avs.get(av).copied() else {
            return;
        };
        let is_margin = self
            .venue_s(s.venue)
            .is_some_and(|v| v.perp && v.margin_asset == asset);
        if !is_margin {
            return;
        }
        let e = i32::from(self.asset_s(asset).qty_exp);
        let n = self.usdt(asset, i128::from(delta), e, RoundMode::TowardZero);
        if let Some(b) = self.books.get_mut(av) {
            b.margin.wallet = Notional(b.margin.wallet.0.saturating_add(n.0));
        }
        // est_margin_balance = wallet + upnl, and the estimates derived from it.
        self.margin_refresh(av);
    }

    /// Inventory of `(asset, group)`: `+delta` (or `−`), keeping the base invariant
    /// `total = baseline + unassigned + Σ inv`. A decrease beyond the group's inventory takes the
    /// rest from `unassigned` and writes `InvShortfall`.
    pub(crate) fn base_add(&mut self, av: usize, asset: AssetId, g: GroupId, delta: i64) {
        if delta == 0 {
            return;
        }
        let Some(ci) = self.cash_ix(av, asset) else { return };
        let Some(b) = self.books.get_mut(av) else { return };
        if let Some(e) = b.cash.get_mut(ci) {
            e.total = Qty(e.total.0.saturating_add(delta));
        }
        if g == GROUP_NONE {
            if let Some(e) = b.cash.get_mut(ci) {
                e.unassigned = Qty(e.unassigned.0.saturating_add(delta));
            }
            return;
        }
        let k = b.inv.position(|x| x.asset == asset && x.group == g);
        if delta > 0 {
            match k {
                Some(k) => {
                    if let Some(x) = b.inv.get_mut(k) {
                        x.qty = Qty(x.qty.0.saturating_add(delta));
                    }
                }
                None => {
                    let at = b
                        .inv
                        .iter()
                        .position(|x| (x.asset.0, x.group.0) > (asset.0, g.0))
                        .unwrap_or(b.inv.len());
                    if !b.inv.insert(
                        at,
                        InvEntry {
                            asset,
                            group: g,
                            qty: Qty(delta),
                        },
                    ) && let Some(e) = b.cash.get_mut(ci)
                    {
                        // Inventory table full: keep the invariant through `unassigned`.
                        e.unassigned = Qty(e.unassigned.0.saturating_add(delta));
                    }
                }
            }
            return;
        }
        let want = delta.saturating_neg();
        let have = k.and_then(|k| b.inv.get(k)).map_or(0, |x| x.qty.0.max(0));
        let take = want.min(have);
        if let Some(k) = k {
            if let Some(x) = b.inv.get_mut(k) {
                x.qty = Qty(x.qty.0.saturating_sub(take));
            }
            if b.inv.get(k).is_some_and(|x| x.qty.0 == 0) {
                b.inv.remove(k);
            }
        }
        let rest = want.saturating_sub(take);
        if rest > 0 {
            if let Some(e) = b.cash.get_mut(ci) {
                e.unassigned = Qty(e.unassigned.0.saturating_sub(rest));
            }
            let s = self.avs.get(av).copied();
            if let Some(s) = s {
                self.push_note(note(
                    code::INV_SHORTFALL,
                    NoteLevel::P1,
                    g,
                    hft_types::INST_NONE,
                    [
                        u64::from(s.venue.0) | u64::from(s.account.0) << 8,
                        u64::from(asset.0),
                        rest as u64,
                        u64::from(g.0),
                        0,
                        0,
                        0,
                        0,
                    ],
                ));
            }
        }
    }

    /// Moves `q` of `asset` from group `g`'s inventory to `unassigned` (`q < 0`: from `unassigned`
    /// into the group). `total` is unchanged and no note is written; a positive `q` moves at most
    /// what the group holds. Returns the quantity moved.
    pub(crate) fn inv_to_unassigned(&mut self, av: usize, asset: AssetId, g: GroupId, q: i64) -> i64 {
        if q == 0 || g == GROUP_NONE {
            return 0;
        }
        let Some(ci) = self.cash_ix(av, asset) else {
            return 0;
        };
        let Some(b) = self.books.get_mut(av) else { return 0 };
        let k = b.inv.position(|x| x.asset == asset && x.group == g);
        let have = k.and_then(|k| b.inv.get(k)).map_or(0, |x| x.qty.0);
        let moved = if q > 0 { q.min(have.max(0)) } else { q };
        if moved == 0 {
            return 0;
        }
        match k {
            Some(k) => {
                if let Some(x) = b.inv.get_mut(k) {
                    x.qty = Qty(x.qty.0.saturating_sub(moved));
                }
                if b.inv.get(k).is_some_and(|x| x.qty.0 == 0) {
                    b.inv.remove(k);
                }
            }
            None => {
                let at = b
                    .inv
                    .iter()
                    .position(|x| (x.asset.0, x.group.0) > (asset.0, g.0))
                    .unwrap_or(b.inv.len());
                if !b.inv.insert(
                    at,
                    InvEntry {
                        asset,
                        group: g,
                        qty: Qty(moved.saturating_neg()),
                    },
                ) {
                    return 0;
                }
            }
        }
        if let Some(e) = b.cash.get_mut(ci) {
            e.unassigned = Qty(e.unassigned.0.saturating_add(moved));
        }
        moved
    }

    /// Fee charged in a base asset other than the traded inst's (platform discount token):
    /// `unassigned` first, then `baseline`, then `unassigned` goes negative (`InvShortfall`).
    /// Never touches any group's inventory.
    pub(crate) fn fee_bucket(&mut self, av: usize, asset: AssetId, fee: i64) {
        if fee == 0 {
            return;
        }
        let Some(ci) = self.cash_ix(av, asset) else { return };
        let mut short = 0i64;
        if let Some(e) = self.books.get_mut(av).and_then(|b| b.cash.get_mut(ci)) {
            e.total = Qty(e.total.0.saturating_sub(fee));
            if fee < 0 {
                e.unassigned = Qty(e.unassigned.0.saturating_sub(fee));
                return;
            }
            let t1 = fee.min(e.unassigned.0.max(0));
            e.unassigned = Qty(e.unassigned.0 - t1);
            let mut rest = fee - t1;
            let t2 = rest.min(e.baseline.0.max(0));
            e.baseline = Qty(e.baseline.0 - t2);
            rest -= t2;
            if rest > 0 {
                e.unassigned = Qty(e.unassigned.0.saturating_sub(rest));
                short = rest;
            }
        }
        if short > 0
            && let Some(s) = self.avs.get(av).copied()
        {
            self.push_note(note(
                code::INV_SHORTFALL,
                NoteLevel::P1,
                GROUP_NONE,
                hft_types::INST_NONE,
                [
                    u64::from(s.venue.0) | u64::from(s.account.0) << 8,
                    u64::from(asset.0),
                    short as u64,
                    0,
                    0,
                    0,
                    0,
                    0,
                ],
            ));
        }
    }

    /// First report of an asset initializes it (`Initialized`); later reports only update the
    /// `rep_*` fields and, when `quiet`, are compared with `total` (`Ok` / `Drift`).
    pub fn on_balance(&mut self, r: &BalanceReport, quiet: bool, t: i64) -> BalanceVerdict {
        self.tick(t);
        let Some(av) = self.av(r.account, r.venue) else {
            return BalanceVerdict::Reported;
        };
        let venue = r.venue;
        let quote = self.is_quote(venue, r.asset);
        let first_av = self.books.get(av).is_some_and(|b| b.vflags & vflags::INIT == 0);
        let dust = self.asset_s(r.asset).dust;
        let Some(ci) = self.cash_ix(av, r.asset) else {
            return BalanceVerdict::Reported;
        };
        let rep = r.free.0.saturating_add(r.locked.0);
        let mut verdict = BalanceVerdict::Reported;
        let mut init_total = None;
        if let Some(e) = self.books.get_mut(av).and_then(|b| b.cash.get_mut(ci)) {
            e.rep_free = r.free;
            e.rep_locked = r.locked;
            e.rep_ex_ts_ns = r.ex_ts_ns;
            e.rep_ens = t;
            if e.bflags & bflags::UNINIT != 0 {
                let old = e.total.0;
                e.total = Qty(rep);
                if !quote {
                    e.unassigned = Qty(e.unassigned.0.saturating_add(rep.saturating_sub(old)));
                }
                e.bflags &= !bflags::UNINIT;
                verdict = BalanceVerdict::Initialized;
                init_total = Some((old, rep));
            } else if quiet {
                let diff = rep.saturating_sub(e.total.0);
                verdict = if diff.saturating_abs() >= dust.0 {
                    BalanceVerdict::Drift {
                        asset: r.asset,
                        diff: Qty(diff),
                    }
                } else {
                    BalanceVerdict::Ok
                };
            }
        }
        if let Some((old, new)) = init_total {
            if quote {
                self.wallet_follow(av, r.asset, new.saturating_sub(old));
            }
            self.push_note(note(
                code::BALANCE_INIT,
                NoteLevel::Info,
                GROUP_NONE,
                hft_types::INST_NONE,
                [
                    u64::from(venue.0) | u64::from(r.account.0) << 8,
                    u64::from(r.asset.0),
                    new as u64,
                    0,
                    0,
                    0,
                    0,
                    0,
                ],
            ));
        }
        if let Some(b) = self.books.get_mut(av) {
            b.vflags |= vflags::INIT;
            match verdict {
                BalanceVerdict::Drift { .. } => b.vflags |= vflags::DRIFT,
                BalanceVerdict::Ok => b.vflags &= !vflags::DRIFT,
                _ => {}
            }
        }
        if first_av
            && self.venue_s(venue).is_some_and(|v| v.perp)
            && let Some(d) = self.avd.get_mut(av)
            && d.hist_from.is_none()
        {
            d.hist_from = Some(r.ex_ts_ns);
        }
        self.refresh_summary(av);
        // The cold consumer has no OMS. Record that the quiet gate actually admitted this
        // report, with the original balance facts; it recomputes the difference itself.
        if matches!(verdict, BalanceVerdict::Ok | BalanceVerdict::Drift { .. }) {
            self.push_note(note(
                code::BALANCE_CHECK,
                NoteLevel::Info,
                GROUP_NONE,
                hft_types::INST_NONE,
                [
                    u64::from(venue.0) | u64::from(r.account.0) << 8,
                    u64::from(r.asset.0),
                    r.free.0 as u64,
                    r.locked.0 as u64,
                    r.ex_ts_ns as u64,
                    0,
                    0,
                    0,
                ],
            ));
        }
        verdict
    }

    /// Whether `asset` is the base asset of any leg or group of account `a`.
    fn asset_in_group(&self, a: AccountId, asset: AssetId) -> bool {
        self.cells.iter().filter(|c| c.account == a).any(|c| {
            c.legs
                .iter()
                .any(|l| self.inst(l.inst).is_some_and(|s| s.base == asset))
        })
    }

    /// `SetBalanceBaseline`: every reported non-quote asset of `(a, v)` that belongs to no group
    /// becomes baseline (`baseline = reported`, `unassigned = 0`, `total = baseline`,
    /// `NON_STRATEGY`); the change goes to `adjust`.
    pub fn set_balance_baseline(
        &mut self,
        a: AccountId,
        v: VenueId,
        cmd_seq: u64,
        out: &mut FixedVec<AssetId, 64>,
    ) -> BaselineResult {
        out.clear();
        let Some(av) = self.av(a, v) else {
            return BaselineResult::NoBalanceView;
        };
        if self.books.get(av).is_none_or(|b| b.vflags & vflags::INIT == 0) {
            return BaselineResult::NoBalanceView;
        }
        let n = self.books.get(av).map_or(0, |b| b.cash.len());
        let mut adj = 0i64;
        let mut total = 0i64;
        for k in 0..n {
            let Some(e) = self.books.get(av).and_then(|b| b.cash.get(k)).copied() else {
                continue;
            };
            if e.bflags & bflags::UNINIT != 0 || self.is_quote(v, e.asset) {
                continue;
            }
            let inv = self.books.get(av).map_or(0, |b| {
                b.inv
                    .iter()
                    .filter(|x| x.asset == e.asset)
                    .fold(0i64, |s, x| s.saturating_add(x.qty.0))
            });
            if inv != 0 || self.asset_in_group(a, e.asset) {
                continue;
            }
            let rep = e.rep_free.0.saturating_add(e.rep_locked.0);
            let diff = rep.saturating_sub(e.total.0);
            let qe = i32::from(self.asset_s(e.asset).qty_exp);
            let dn = self.usdt(e.asset, i128::from(diff), qe, RoundMode::TowardZero);
            let tn = self.usdt(e.asset, i128::from(rep), qe, RoundMode::TowardZero);
            adj = adj.saturating_add(dn.0);
            total = total.saturating_add(tn.0);
            if let Some(x) = self.books.get_mut(av).and_then(|b| b.cash.get_mut(k)) {
                x.baseline = Qty(rep);
                x.unassigned = Qty(0);
                x.total = Qty(rep);
                x.bflags |= bflags::NON_STRATEGY;
            }
            out.push(e.asset);
        }
        if let Some(acc) = self.accounts.get_mut(usize::from(a.0)) {
            acc.day.adjust = Notional(acc.day.adjust.0.saturating_add(adj));
        }
        self.push_note(note(
            code::BASELINE_SET,
            NoteLevel::Info,
            GROUP_NONE,
            hft_types::INST_NONE,
            [
                out.len() as u64,
                total as u64,
                u64::from(v.0) | u64::from(a.0) << 8,
                cmd_seq,
                0,
                0,
                0,
                0,
            ],
        ));
        self.refresh_summary(av);
        BaselineResult::Set {
            n_assets: out.len() as u16,
            notional: Notional(total),
        }
    }

    /// `RecordTransfer` / `SimTransfer`: moves cash, books no PnL.
    pub fn apply_transfer(&mut self, tr: &TransferDelta, t: i64) {
        self.tick(t);
        let (Some(from), Some(to)) = (
            self.av(tr.account, tr.from_venue),
            self.av(tr.account, tr.to_venue),
        ) else {
            return;
        };
        for (av, v, d) in [
            (from, tr.from_venue, tr.amount.0.saturating_neg()),
            (to, tr.to_venue, tr.amount.0),
        ] {
            if self.is_quote(v, tr.asset) {
                self.quote_add(av, tr.asset, d);
            } else {
                self.base_add(av, tr.asset, GROUP_NONE, d);
            }
            if let Some(e) = self.cash_mut(av, tr.asset) {
                e.bflags &= !bflags::UNINIT;
            }
            self.refresh_summary(av);
        }
    }

    /// `SimReset` (Sim accounts only): clears every position, inventory, PnL, round and the
    /// external-fill / push de-duplication and `auto_held`; cash = `init`.
    pub fn sim_reset(&mut self, a: AccountId, init: &SimInit, t: i64) {
        self.tick(t);
        if !self.acc_sim.get(usize::from(a.0)).copied().unwrap_or(false) {
            return;
        }
        let avs = self.acc_avs.get(usize::from(a.0)).cloned().unwrap_or_default();
        for &av in &avs {
            let av = usize::from(av);
            let Some(s) = self.avs.get(av).copied() else {
                continue;
            };
            let n = self.venues.get(usize::from(s.venue.0)).map_or(0, |v| v.n_insts) as usize;
            for p in (s.pos_base as usize)..(s.pos_base as usize + n) {
                if let Some(x) = self.pos.get_mut(p) {
                    *x = InstPos::ZERO;
                }
                if let Some(x) = self.posd.get_mut(p) {
                    *x = PosDyn {
                        upnl: Notional(0),
                        auto_buy: Qty(0),
                        auto_sell: Qty(0),
                        notional: Notional(0),
                    };
                }
            }
            if let Some(b) = self.books.get_mut(av) {
                for e in b.cash.iter_mut() {
                    *e = CashEntry::new(e.asset);
                    e.bflags = 0;
                }
                b.inv.clear();
                let wallet_keep = Notional(0);
                b.margin = MarginState::new();
                b.margin.wallet = wallet_keep;
                b.vflags |= vflags::INIT;
                b.vflags &= !vflags::DRIFT;
            }
            if let Some(d) = self.avd.get_mut(av) {
                d.ext.clear();
                d.auto_cash.clear();
                d.seen.clear();
                d.pending.clear();
                d.hist.clear();
                d.no_hist.clear();
                d.open_perps.clear();
            }
        }
        for c in self.cells.iter_mut().filter(|c| c.account == a) {
            let (active, shadow, n_legs, group) = (c.active, c.shadow, c.n_legs, c.group);
            let insts = c.legs.map(|l| l.inst);
            *c = crate::ledger::Cell::new(a, group);
            c.active = active;
            c.shadow = shadow;
            c.n_legs = n_legs;
            for (l, i) in c.legs.iter_mut().zip(insts) {
                l.inst = i;
            }
        }
        if let Some(x) = self.accounts.get_mut(usize::from(a.0)) {
            *x = AccountBook::ZERO;
        }
        for b in init {
            if let Some(av) = self.av(a, b.venue) {
                if self.is_quote(b.venue, b.asset) {
                    self.quote_add(av, b.asset, b.qty.0);
                } else {
                    self.base_add(av, b.asset, GROUP_NONE, b.qty.0);
                }
            }
        }
        self.push_note(note(
            code::SIM_RESET,
            NoteLevel::Info,
            GROUP_NONE,
            hft_types::INST_NONE,
            [u64::from(a.0), init.len() as u64, 0, 0, 0, 0, 0, 0],
        ));
        for &av in &avs {
            self.refresh_summary(usize::from(av));
        }
    }
}
