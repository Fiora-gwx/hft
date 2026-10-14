//! Funding: settle-time normalization grid, booking by `VenueCaps.funding_settle`, push
//! de-duplication, synthesis, push/synthesis pairing and residual, the per-settlement records
//! and watermark, reconciliation corrections, no-history holds and the window sweep
//! (README "资金费", "资金费时刻归一化", "资金费对账校正").

use hft_event::{FundingSettle, NoteLevel};
use hft_refdata::FundingSettleKind;
use hft_types::{
    AccountId, GROUP_NONE, GroupId, INST_NONE, InstId, Notional, Px, Qty, Rate, RoundMode, Tsc, VenueId,
    funding_settle_norm_ns,
};

use crate::fixed::{FixedVec, rescale, sat64};
use crate::ledger::{FUND_PENDING_CAP, FundPend, FundRec, Ledger, NO_HIST_CAP, NoHist, PUSHED};
use crate::notes::{code, note};
use crate::types::*;

const SYNTH_ONLY: u8 = 1;
const PUSH_ONLY: u8 = 2;

impl Ledger {
    /// Settle time normalized on the grid of `(v, inst)`.
    pub(crate) fn norm_ts(&self, v: VenueId, inst: InstId, ts: i64) -> i64 {
        let (iv, an) = self.funding_grid(v, inst);
        funding_settle_norm_ns(ts, iv, an)
    }

    /// The actual funding period of `inst` (`Funding.interval_s ≠ 0`).
    pub fn on_funding_interval(&mut self, inst: InstId, interval_s: u32) {
        if interval_s == 0 {
            return;
        }
        if let Some(p) = self.ipx.get_mut(inst.0 as usize) {
            p.interval_s = interval_s;
        }
    }

    /// Current non-Shadow holder group of `(a, inst)`.
    pub(crate) fn holder(&self, a: AccountId, inst: InstId) -> (Option<usize>, GroupId) {
        match self.candidates(a, inst) {
            (1, Some((c, _))) => (Some(c), self.cells.get(c).map_or(GROUP_NONE, |x| x.group)),
            _ => (None, GROUP_NONE),
        }
    }

    pub(crate) fn hist_find(&self, av: usize, inst: InstId, ts: i64) -> Option<usize> {
        let h = &self.avd.get(av)?.hist;
        let start = h.partition_point(|r| r.settle_ts < ts);
        h.iter()
            .enumerate()
            .skip(start)
            .take_while(|(_, r)| r.settle_ts == ts)
            .find(|(_, r)| r.inst == inst)
            .map(|(k, _)| k)
    }

    /// Inserts a record (sorted by settle time, then inst); a full ring evicts its oldest record
    /// and moves the watermark past it.
    pub(crate) fn hist_insert(&mut self, av: usize, r: FundRec) -> Option<usize> {
        let cap = self.cfg.hist_cap;
        let d = self.avd.get_mut(av)?;
        if d.hist.len() >= cap {
            let old = d.hist.remove(0);
            d.hist_from = Some(old.settle_ts.saturating_add(1));
        }
        let at = d
            .hist
            .partition_point(|x| (x.settle_ts, x.inst.0) < (r.settle_ts, r.inst.0));
        d.hist.insert(at, r);
        Some(at)
    }

    /// Settlement asset (= margin asset) of a perpetual venue.
    pub(crate) fn settle_asset(&self, v: VenueId) -> hft_types::AssetId {
        self.venue_s(v).map_or(hft_types::ASSET_NONE, |x| x.margin_asset)
    }

    /// Notional (1e-6 USDT) → settlement-asset quantity units.
    pub(crate) fn notional_to_asset(&self, v: VenueId, n: Notional) -> i64 {
        let e = i32::from(self.asset_s(self.settle_asset(v)).qty_exp);
        sat64(rescale(i128::from(n.0), -6, e, RoundMode::TowardZero).0)
    }

    fn funding_pnl(&mut self, a: AccountId, cell: Option<usize>, n: Notional) -> PnlBreakdown {
        let mut d = PnlBreakdown::zero();
        d.funding = n;
        self.add_pnl(a, cell, &d);
        d
    }

    fn residual_add(&mut self, a: AccountId, n: i64) {
        if let Some(x) = self.accounts.get_mut(usize::from(a.0)) {
            x.funding_residual = Notional(x.funding_residual.0.saturating_add(n));
        }
    }

    fn pending_add(&mut self, av: usize, p: FundPend) {
        let Some(d) = self.avd.get_mut(av) else { return };
        let at = d
            .pending
            .iter()
            .position(|x| x.settle_ts > p.settle_ts)
            .unwrap_or(d.pending.len());
        d.pending.insert(at, p);
        if d.pending.len() > FUND_PENDING_CAP {
            d.pending.remove(0);
        }
    }

    fn tolerance(&self, pushed: Notional) -> i64 {
        let rel = sat64(crate::fixed::div_round(
            i128::from(pushed.0.saturating_abs()) * i128::from(self.cfg.tol_rel),
            1_000_000_000,
            RoundMode::Floor,
        ));
        self.cfg.tol_abs.0.max(rel)
    }

    /// Books realized funding: a `PerSymbolPush` push (de-duplicated by `(inst, settle_ts)`) or a
    /// synthesis (`synth = 1`); `None` = duplicate.
    pub fn apply_funding(&mut self, f: &FundingSettle, t: i64) -> Option<PnlDelta> {
        self.tick(t);
        let av = self.av(f.account, f.venue)?;
        let kind = self.venue_s(f.venue)?.funding;
        let ts = self.norm_ts(f.venue, f.inst, f.settle_ts_ns);
        let pushed = f.synth == 0;
        let existing = self.hist_find(av, f.inst, ts);
        // An authenticated history poll supplies the complete per-symbol settlement total.
        // Finalize its estimate, retaining the settlement-time owner even after a close/rebind.
        // Pre-initialization bills are already included in the opening balance.
        if kind == FundingSettleKind::RateOnly && pushed {
            if self.inst(f.inst).is_none_or(|i| i.venue != f.venue)
                || self
                    .avd
                    .get(av)
                    .and_then(|d| d.hist_from)
                    .is_none_or(|from| ts < from)
            {
                return None;
            }
            let old = existing.and_then(|k| self.avd.get(av)?.hist.get(k)).copied();
            let (cell, group) = old.map_or_else(
                || self.holder(f.account, f.inst),
                |r| (self.cell_of(f.account, r.group), r.group),
            );
            let booked = old.map_or(0, |r| r.booked.0.saturating_add(r.corrected.0));
            let delta = Notional(f.amount.0.saturating_sub(booked));
            let row = FundRec {
                inst: f.inst,
                group,
                hflags: PUSHED,
                settle_ts: ts,
                booked: f.amount,
                corrected: Notional(0),
            };
            match existing {
                Some(k) => {
                    if let Some(r) = self.avd.get_mut(av).and_then(|d| d.hist.get_mut(k)) {
                        *r = row;
                    }
                }
                None => {
                    self.hist_insert(av, row);
                }
            }
            if delta.0 == 0 {
                return None;
            }
            self.quote_add(
                av,
                self.settle_asset(f.venue),
                self.notional_to_asset(f.venue, delta),
            );
            let d = self.funding_pnl(f.account, cell, delta);
            self.refresh_summary(av);
            return Some(PnlDelta {
                account: f.account,
                group,
                d,
            });
        }
        // Neither a repeated timer nor a late prediction may add to a finalized settlement.
        if !pushed && existing.is_some() {
            return None;
        }
        if pushed
            && existing
                .and_then(|k| self.avd.get(av).and_then(|d| d.hist.get(k)))
                .is_some_and(|r| r.hflags & PUSHED != 0)
        {
            return None;
        }
        let (cell, group) = self.holder(f.account, f.inst);
        let asset = self.settle_asset(f.venue);
        let d = if kind == FundingSettleKind::BalanceOnlyPush && !pushed {
            // Attribution only; the wallet follows the pushes.
            self.residual_add(f.account, f.amount.0.saturating_neg());
            let k = self
                .avd
                .get(av)
                .and_then(|d| d.pending.position(|x| x.settle_ts == ts));
            match k.and_then(|k| {
                self.avd
                    .get(av)
                    .and_then(|d| d.pending.get(k).copied())
                    .map(|p| (k, p))
            }) {
                Some((k, p)) if p.state == PUSH_ONLY => {
                    let synth = p.synth_sum.0.saturating_add(f.amount.0);
                    let res = p.pushed.0.saturating_sub(synth);
                    if res.saturating_abs() > self.tolerance(p.pushed) {
                        self.push_note(note(
                            code::FUNDING_MISMATCH,
                            NoteLevel::P1,
                            GROUP_NONE,
                            INST_NONE,
                            [
                                u64::from(f.venue.0) | u64::from(f.account.0) << 8,
                                ts as u64,
                                p.pushed.0 as u64,
                                synth as u64,
                                res as u64,
                                0,
                                0,
                                0,
                            ],
                        ));
                    }
                    if let Some(d) = self.avd.get_mut(av) {
                        d.pending.remove(k);
                    }
                }
                Some((k, _)) => {
                    if let Some(x) = self.avd.get_mut(av).and_then(|d| d.pending.get_mut(k)) {
                        x.synth_sum = Notional(x.synth_sum.0.saturating_add(f.amount.0));
                    }
                }
                None => self.pending_add(
                    av,
                    FundPend {
                        settle_ts: ts,
                        synth_sum: f.amount,
                        pushed: Notional(0),
                        state: SYNTH_ONLY,
                    },
                ),
            }
            self.funding_pnl(f.account, cell, f.amount)
        } else {
            let q = self.notional_to_asset(f.venue, f.amount);
            self.quote_add(av, asset, q);
            self.funding_pnl(f.account, cell, f.amount)
        };
        match existing {
            Some(k) => {
                if let Some(r) = self.avd.get_mut(av).and_then(|d| d.hist.get_mut(k)) {
                    r.booked = Notional(r.booked.0.saturating_add(f.amount.0));
                    if pushed {
                        r.hflags |= PUSHED;
                    }
                }
            }
            None => {
                self.hist_insert(
                    av,
                    FundRec {
                        inst: f.inst,
                        group,
                        hflags: if pushed { PUSHED } else { 0 },
                        settle_ts: ts,
                        booked: f.amount,
                        corrected: Notional(0),
                    },
                );
            }
        }
        self.refresh_summary(av);
        Some(PnlDelta {
            account: f.account,
            group,
            d,
        })
    }

    /// `BalanceOnlyPush` wallet push (`inst = INST_NONE`): de-duplicated by `(settle_ts, amount)`,
    /// booked into the wallet at the pushed amount and paired with the syntheses.
    pub fn apply_funding_push(&mut self, f: &FundingSettle, t: i64) -> FundingPush {
        self.tick(t);
        let Some(av) = self.av(f.account, f.venue) else {
            return FundingPush::Duplicate;
        };
        let ts = self.norm_ts(f.venue, INST_NONE, f.settle_ts_ns);
        let seen_cap = self.cfg.seen_cap;
        if let Some(d) = self.avd.get_mut(av) {
            if d.seen.iter().any(|(s, a)| *s == ts && *a == f.amount) {
                return FundingPush::Duplicate;
            }
            if d.seen.len() >= seen_cap {
                d.seen.remove(0);
            }
            d.seen.push((ts, f.amount));
        }
        let asset = self.settle_asset(f.venue);
        let q = self.notional_to_asset(f.venue, f.amount);
        self.quote_add(av, asset, q);
        self.residual_add(f.account, f.amount.0);
        match self.hist_find(av, INST_NONE, ts) {
            Some(k) => {
                if let Some(r) = self.avd.get_mut(av).and_then(|d| d.hist.get_mut(k)) {
                    r.booked = Notional(r.booked.0.saturating_add(f.amount.0));
                    r.hflags |= PUSHED;
                }
            }
            None => {
                self.hist_insert(
                    av,
                    FundRec {
                        inst: INST_NONE,
                        group: GROUP_NONE,
                        hflags: PUSHED,
                        settle_ts: ts,
                        booked: f.amount,
                        corrected: Notional(0),
                    },
                );
            }
        }
        let k = self
            .avd
            .get(av)
            .and_then(|d| d.pending.position(|x| x.settle_ts == ts));
        let mut check = FundingCheck::Ok;
        match k.and_then(|k| {
            self.avd
                .get(av)
                .and_then(|d| d.pending.get(k).copied())
                .map(|p| (k, p))
        }) {
            Some((k, p)) if p.state == SYNTH_ONLY => {
                let pushed = Notional(p.pushed.0.saturating_add(f.amount.0));
                let res = pushed.0.saturating_sub(p.synth_sum.0);
                if res.saturating_abs() > self.tolerance(pushed) {
                    check = FundingCheck::Mismatch {
                        residual: Notional(res),
                    };
                }
                if let Some(d) = self.avd.get_mut(av) {
                    d.pending.remove(k);
                }
            }
            Some((k, _)) => {
                if let Some(x) = self.avd.get_mut(av).and_then(|d| d.pending.get_mut(k)) {
                    x.pushed = Notional(x.pushed.0.saturating_add(f.amount.0));
                }
            }
            None => self.pending_add(
                av,
                FundPend {
                    settle_ts: ts,
                    synth_sum: Notional(0),
                    pushed: f.amount,
                    state: PUSH_ONLY,
                },
            ),
        }
        self.refresh_summary(av);
        FundingPush::Booked { check }
    }

    /// Synthesized funding `−pos_qty × mark × rate` (income Floor, expense Ceil in magnitude),
    /// at the normalized settle time; flat → `None`. Pure.
    pub fn synth_funding(
        &self,
        a: AccountId,
        inst: InstId,
        mark: Px,
        rate: Rate,
        settle_ts_ns: i64,
    ) -> Option<FundingSettle> {
        let s = self.inst(inst)?;
        let q = self.pos(a, inst).qty;
        if q.0 == 0 {
            return None;
        }
        let v = i128::from(q.0)
            .saturating_neg()
            .saturating_mul(i128::from(mark.0))
            .saturating_mul(i128::from(rate.0));
        let e = i32::from(s.qty_exp) + i32::from(s.px_exp) - 9;
        let amount = sat64(rescale(v, e, -6, RoundMode::Floor).0);
        Some(FundingSettle {
            venue: s.venue,
            account: a,
            _pad: 0,
            inst,
            amount: Notional(amount),
            rate,
            pos_qty: q,
            settle_ts_ns: self.norm_ts(s.venue, inst, settle_ts_ns),
            rx_tsc: Tsc(0),
            synth: 1,
            _pad2: [0; 7],
        })
    }

    /// Books a `Funding` adjustment (reconciliation or window sweep): cash and wallet, the group
    /// or account funding (wallet items of `BalanceOnlyPush` venues: the residual), and the
    /// record's `corrected` (created when missing, inside the watermark only).
    pub(crate) fn book_funding_adj(&mut self, adj: &AdjustDelta, t: i64) -> PnlDelta {
        let mut out = PnlDelta {
            account: adj.account,
            group: adj.group,
            d: PnlBreakdown::zero(),
        };
        let Some(av) = self.av(adj.account, adj.venue) else {
            return out;
        };
        let Some(kind) = self.venue_s(adj.venue).map(|v| v.funding) else {
            return out;
        };
        let asset = if adj.asset.0 != 0 {
            adj.asset
        } else {
            self.settle_asset(adj.venue)
        };
        let e = i32::from(self.asset_s(asset).qty_exp);
        let n = self.usdt(asset, i128::from(adj.qty.0), e, RoundMode::TowardZero);
        self.quote_add(av, asset, adj.qty.0);
        if kind == FundingSettleKind::BalanceOnlyPush && adj.inst == INST_NONE {
            self.residual_add(adj.account, n.0);
        } else {
            let cell = self.cell_of(adj.account, adj.group);
            out.d = self.funding_pnl(adj.account, cell, n);
        }
        let inside = self
            .avd
            .get(av)
            .and_then(|d| d.hist_from)
            .is_some_and(|h| adj.settle_ts_ns >= h);
        if inside {
            match self.hist_find(av, adj.inst, adj.settle_ts_ns) {
                Some(k) => {
                    if let Some(r) = self.avd.get_mut(av).and_then(|d| d.hist.get_mut(k)) {
                        r.corrected = Notional(r.corrected.0.saturating_add(n.0));
                    }
                }
                None => {
                    self.hist_insert(
                        av,
                        FundRec {
                            inst: adj.inst,
                            group: adj.group,
                            hflags: 0,
                            settle_ts: adj.settle_ts_ns,
                            booked: Notional(0),
                            corrected: n,
                        },
                    );
                }
            }
        }
        let _ = t;
        self.refresh_summary(av);
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn corrected_note(
        &mut self,
        a: AccountId,
        v: VenueId,
        inst: InstId,
        ts: i64,
        exch: Notional,
        booked: Notional,
        diff: Notional,
        g: GroupId,
        flags: u64,
    ) {
        self.push_note(note(
            code::FUNDING_CORRECTED,
            NoteLevel::P1,
            g,
            inst,
            [
                u64::from(v.0) | u64::from(a.0) << 8,
                u64::from(inst.0),
                ts as u64,
                exch.0 as u64,
                booked.0 as u64,
                diff.0 as u64,
                u64::from(g.0),
                flags,
            ],
        ));
    }

    #[allow(clippy::too_many_arguments)]
    fn funding_adj(
        &self,
        a: AccountId,
        v: VenueId,
        inst: InstId,
        g: GroupId,
        diff: Notional,
        ts: i64,
        report_id: u64,
    ) -> AdjustDelta {
        AdjustDelta {
            account: a,
            venue: v,
            kind: AdjustKind::Funding as u8,
            px_src: 0,
            inst,
            asset: self.settle_asset(v),
            _pad: 0,
            group: g,
            qty: Qty(self.notional_to_asset(v, diff)),
            px: Px(0),
            report_id,
            cmd_seq: 0,
            settle_ts_ns: ts,
        }
    }

    /// Books a held no-history item at the exchange amount (window end or table overflow).
    fn book_no_hist(&mut self, a: AccountId, v: VenueId, it: NoHist, t: i64) -> FundingRecon {
        let diff = Notional(it.exch.0.saturating_sub(it.engine.0));
        let g = if it.inst == INST_NONE {
            GROUP_NONE
        } else {
            self.holder(a, it.inst).1
        };
        let adj = self.funding_adj(a, v, it.inst, g, diff, it.settle_ts, it.report_id);
        let pnl = self.book_funding_adj(&adj, t);
        self.corrected_note(
            a,
            v,
            it.inst,
            it.settle_ts,
            it.exch,
            it.engine,
            diff,
            g,
            fc_flags::NO_HISTORY | fc_flags::WINDOW_END,
        );
        FundingRecon::Corrected { adj, pnl }
    }

    /// One `Funding` item of a reconciliation report (README "资金费对账校正" steps 0–5).
    pub fn reconcile_funding(&mut self, it: &FundingReconItem, now_ns: i64, t: i64) -> FundingRecon {
        self.tick(t);
        let Some(av) = self.av(it.account, it.venue) else {
            return FundingRecon::NotApplicable;
        };
        let Some(kind) = self.venue_s(it.venue).map(|v| v.funding) else {
            return FundingRecon::NotApplicable;
        };
        if kind == FundingSettleKind::BalanceOnlyPush && it.inst != INST_NONE {
            return FundingRecon::NotApplicable;
        }
        let ts = self.norm_ts(it.venue, it.inst, it.settle_ts_ns);
        if ts > now_ns.saturating_sub(self.cfg.lag_ns) {
            return FundingRecon::TooRecent;
        }
        let horizon = now_ns.saturating_sub(self.cfg.window_ns);
        if ts < horizon {
            return FundingRecon::OutsideWindow;
        }
        let hist_from = match self.avd.get_mut(av) {
            Some(d) => {
                let k = d.hist.partition_point(|r| r.settle_ts < horizon);
                if k > 0 {
                    d.hist.drain(..k);
                }
                *d.hist_from.get_or_insert(now_ns)
            }
            None => return FundingRecon::NotApplicable,
        };
        if ts < hist_from {
            let item = NoHist {
                inst: it.inst,
                settle_ts: ts,
                exch: it.exch_amount,
                engine: it.engine_amount,
                report_id: it.report_id,
            };
            let k = self
                .avd
                .get(av)
                .and_then(|d| d.no_hist.position(|x| x.inst == it.inst && x.settle_ts == ts));
            let mut out = FundingRecon::NoHistoryHeld;
            match k {
                Some(k) => {
                    if let Some(x) = self.avd.get_mut(av).and_then(|d| d.no_hist.get_mut(k)) {
                        *x = item;
                    }
                }
                None => {
                    let full = self.avd.get(av).is_some_and(|d| d.no_hist.len() >= NO_HIST_CAP);
                    if full {
                        // Book the earliest settlement now.
                        let e = self.avd.get(av).and_then(|d| {
                            d.no_hist
                                .iter()
                                .enumerate()
                                .min_by_key(|(_, x)| (x.settle_ts, x.inst.0))
                                .map(|(k, x)| (k, *x))
                        });
                        if let Some((k, x)) = e {
                            if let Some(d) = self.avd.get_mut(av) {
                                d.no_hist.remove(k);
                            }
                            out = self.book_no_hist(it.account, it.venue, x, t);
                        }
                    }
                    if let Some(d) = self.avd.get_mut(av) {
                        let at = d
                            .no_hist
                            .iter()
                            .position(|x| (x.inst.0, x.settle_ts) > (it.inst.0, ts))
                            .unwrap_or(d.no_hist.len());
                        d.no_hist.insert(at, item);
                    }
                }
            }
            let g = if it.inst == INST_NONE {
                GROUP_NONE
            } else {
                self.holder(it.account, it.inst).1
            };
            self.corrected_note(
                it.account,
                it.venue,
                it.inst,
                ts,
                it.exch_amount,
                it.engine_amount,
                Notional(it.exch_amount.0.saturating_sub(it.engine_amount.0)),
                g,
                fc_flags::NO_HISTORY,
            );
            return out;
        }
        let rec = self
            .hist_find(av, it.inst, ts)
            .and_then(|k| self.avd.get(av).and_then(|d| d.hist.get(k)).copied());
        let booked_total = rec.map_or(0, |r| r.booked.0.saturating_add(r.corrected.0));
        let diff = it.exch_amount.0.saturating_sub(booked_total);
        let asset = self.settle_asset(it.venue);
        let a = self.asset_s(asset);
        let dust = sat64(
            rescale(
                i128::from(a.dust.0),
                i32::from(a.qty_exp),
                -6,
                RoundMode::TowardZero,
            )
            .0,
        );
        if diff.saturating_abs() < dust {
            return FundingRecon::Matched;
        }
        let g = match rec {
            Some(r) => r.group,
            None if it.inst == INST_NONE => GROUP_NONE,
            None => self.holder(it.account, it.inst).1,
        };
        let adj = self.funding_adj(it.account, it.venue, it.inst, g, Notional(diff), ts, it.report_id);
        let pnl = self.book_funding_adj(&adj, t);
        self.corrected_note(
            it.account,
            it.venue,
            it.inst,
            ts,
            it.exch_amount,
            Notional(booked_total),
            Notional(diff),
            g,
            if rec.is_none() { fc_flags::NEW_RECORD } else { 0 },
        );
        FundingRecon::Corrected { adj, pnl }
    }

    /// After a reconciliation group of `(a, v)`: books the held no-history items that left the
    /// comparison window at the exchange amount.
    pub fn funding_window_sweep(
        &mut self,
        a: AccountId,
        v: VenueId,
        now_ns: i64,
        t: i64,
        out: &mut FixedVec<FundingRecon, 64>,
    ) {
        self.tick(t);
        out.clear();
        let Some(av) = self.av(a, v) else { return };
        let horizon = now_ns.saturating_sub(self.cfg.window_ns);
        loop {
            let k = self
                .avd
                .get(av)
                .and_then(|d| d.no_hist.position(|x| x.settle_ts < horizon));
            let Some(k) = k else { break };
            let Some(x) = self.avd.get_mut(av).and_then(|d| d.no_hist.remove(k)) else {
                break;
            };
            let r = self.book_no_hist(a, v, x, t);
            out.push(r);
        }
    }

    /// Watermark of `(a, v)`: since when every booking is recorded (`None` = no `FundMark`).
    pub fn hist_from_ns(&self, a: AccountId, v: VenueId) -> Option<i64> {
        self.av(a, v)
            .and_then(|av| self.avd.get(av))
            .and_then(|d| d.hist_from)
    }

    /// Funding records of `(a, v)`: `(inst, group, hflags, settle_ts_ns, booked, corrected)`.
    pub fn fund_hist(&self, a: AccountId, v: VenueId) -> Vec<FundHistView> {
        self.av(a, v)
            .and_then(|av| self.avd.get(av))
            .map(|d| {
                d.hist
                    .iter()
                    .map(|r| FundHistView {
                        inst: r.inst,
                        group: r.group,
                        hflags: r.hflags,
                        settle_ts_ns: r.settle_ts,
                        booked: r.booked,
                        corrected: r.corrected,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Pairing table of `(a, v)`: `(settle_ts_ns, synth_sum, pushed, state)`.
    pub fn fund_pending(&self, a: AccountId, v: VenueId) -> Vec<(i64, Notional, Notional, u8)> {
        self.av(a, v)
            .and_then(|av| self.avd.get(av))
            .map(|d| {
                d.pending
                    .iter()
                    .map(|p| (p.settle_ts, p.synth_sum, p.pushed, p.state))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Held no-history items of `(a, v)`: `(inst, settle_ts_ns, exch, engine, report_id)`.
    pub fn fund_no_hist(&self, a: AccountId, v: VenueId) -> Vec<(InstId, i64, Notional, Notional, u64)> {
        self.av(a, v)
            .and_then(|av| self.avd.get(av))
            .map(|d| {
                d.no_hist
                    .iter()
                    .map(|x| (x.inst, x.settle_ts, x.exch, x.engine, x.report_id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Push de-duplication entries of `(a, v)` in booking order.
    pub fn fund_seen(&self, a: AccountId, v: VenueId) -> Vec<(i64, Notional)> {
        self.av(a, v)
            .and_then(|av| self.avd.get(av))
            .map(|d| d.seen.clone())
            .unwrap_or_default()
    }
}

/// One funding record (`FundHist`).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct FundHistView {
    pub inst: InstId,
    pub group: GroupId,
    /// `PUSHED = 1`.
    pub hflags: u8,
    pub settle_ts_ns: i64,
    pub booked: Notional,
    pub corrected: Notional,
}

/// `FundHistView.hflags`: booked from an exchange push.
pub const HFLAG_PUSHED: u8 = PUSHED;
