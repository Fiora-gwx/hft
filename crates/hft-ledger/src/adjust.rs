//! `AdjustDelta`: `AdoptExchange` position and balance entries, funding corrections and residual
//! write-offs (README "人工干预的采纳").

use hft_event::NoteLevel;
use hft_types::{GROUP_NONE, Qty, RoundMode, Side};

use crate::ledger::{GF_ADOPTED, Ledger};
use crate::notes::{code, note};
use crate::position::{Col, Trade, avg_apply};
use crate::types::*;

impl Ledger {
    /// Books an adjustment (Position/Balance adoption, Funding correction, WriteOff).
    pub fn apply_adjust(&mut self, a: &AdjustDelta, t: i64) -> PnlDelta {
        self.tick(t);
        let zero = PnlDelta {
            account: a.account,
            group: a.group,
            d: PnlBreakdown::zero(),
        };
        match AdjustKind::try_from(a.kind) {
            Ok(AdjustKind::Position) => self.adopt_position(a, t),
            Ok(AdjustKind::Balance) => self.adopt_balance(a, t),
            Ok(AdjustKind::Funding) => self.book_funding_adj(a, t),
            Ok(AdjustKind::WriteOff) => self.write_off(a, t),
            Err(_) => zero,
        }
    }

    fn adjusted_note(&mut self, a: &AdjustDelta, realized: hft_types::Notional) {
        let id = if a.kind == AdjustKind::Balance as u8 {
            u64::from(a.asset.0)
        } else {
            u64::from(a.inst.0)
        };
        self.push_note(note(
            code::ADJUSTED,
            NoteLevel::Info,
            a.group,
            a.inst,
            [
                u64::from(a.account.0) | u64::from(a.venue.0) << 8,
                id,
                a.report_id,
                a.qty.0 as u64,
                a.px.0 as u64,
                realized.0 as u64,
                u64::from(a.kind) | u64::from(a.px_src) << 8,
                a.cmd_seq,
            ],
        ));
    }

    fn adopt_position(&mut self, a: &AdjustDelta, t: i64) -> PnlDelta {
        let mut g = a.group;
        if g == GROUP_NONE {
            g = self.holder(a.account, a.inst).1;
        }
        let cell = if g == GROUP_NONE {
            None
        } else {
            self.resolve_cell(a.account, g, a.inst, u8::MAX)
        };
        if a.qty.0 == 0 {
            self.adjusted_note(a, hft_types::Notional(0));
            return PnlDelta {
                account: a.account,
                group: g,
                d: PnlBreakdown::zero(),
            };
        }
        let spot = self.inst(a.inst).is_some_and(|s| !s.perp);
        let side = if a.qty.0 > 0 { Side::Buy } else { Side::Sell };
        let q = Qty(a.qty.0.saturating_abs());
        let d = self.book_trade(&Trade {
            acc: a.account,
            inst: a.inst,
            side,
            gross: q,
            net: q,
            px: a.px,
            cell,
            col: Col::Adjust,
            t,
            seq: a.cmd_seq,
            cash: false,
            defer_round: None,
        });
        if spot
            && let Some(s) = self.inst(a.inst).copied()
            && let Some(av) = self.av(a.account, s.venue)
        {
            self.base_add(av, s.base, g, a.qty.0);
            self.refresh_summary(av);
        }
        if let Some((c, _)) = cell
            && let Some(x) = self.cells.get_mut(c)
        {
            x.gflags |= GF_ADOPTED;
        }
        self.adjusted_note(a, d.adjust);
        PnlDelta {
            account: a.account,
            group: g,
            d,
        }
    }

    fn adopt_balance(&mut self, a: &AdjustDelta, t: i64) -> PnlDelta {
        let mut out = PnlDelta {
            account: a.account,
            group: a.group,
            d: PnlBreakdown::zero(),
        };
        let Some(av) = self.av(a.account, a.venue) else {
            return out;
        };
        if self.is_quote(a.venue, a.asset) {
            self.quote_add(av, a.asset, a.qty.0);
        } else {
            self.base_add(av, a.asset, a.group, a.qty.0);
        }
        if a.group != GROUP_NONE && self.inst(a.inst).is_some_and(|s| !s.perp && s.base == a.asset) {
            let cell = self.resolve_cell(a.account, a.group, a.inst, u8::MAX);
            if a.qty.0 != 0 {
                let side = if a.qty.0 > 0 { Side::Buy } else { Side::Sell };
                let q = Qty(a.qty.0.saturating_abs());
                out.d = self.book_trade(&Trade {
                    acc: a.account,
                    inst: a.inst,
                    side,
                    gross: q,
                    net: q,
                    px: a.px,
                    cell,
                    col: Col::Adjust,
                    t,
                    seq: a.cmd_seq,
                    cash: false,
                    defer_round: None,
                });
            }
            if let Some((c, _)) = cell
                && let Some(x) = self.cells.get_mut(c)
            {
                x.gflags |= GF_ADOPTED;
            }
        }
        self.refresh_summary(av);
        self.adjusted_note(a, out.d.adjust);
        out
    }

    /// Moves `qty` of one leg's naked filled quantity from the group to account level
    /// (spot: inventory → `unassigned`; perp: leg net only). Positions and cash unchanged.
    fn write_off(&mut self, a: &AdjustDelta, t: i64) -> PnlDelta {
        let out = PnlDelta {
            account: a.account,
            group: a.group,
            d: PnlBreakdown::zero(),
        };
        let mut notional = 0i64;
        if a.qty.0 != 0
            && let Some(s) = self.inst(a.inst).copied()
            && let Some(av) = self.av(a.account, s.venue)
            && let Some(c) = self.cell_of(a.account, a.group)
        {
            let l = self
                .cells
                .get(c)
                .and_then(|x| x.legs.iter().position(|l| l.inst == a.inst));
            if let Some(l) = l {
                if let Some(leg) = self.cells.get_mut(c).and_then(|x| x.legs.get_mut(l)) {
                    // Remove at the leg's average cost (no realized PnL).
                    let s_ = -a.qty.0.signum();
                    let mut n = leg.net.0;
                    let mut cost = leg.cost;
                    let q = a.qty.0.saturating_abs().min(n.saturating_abs());
                    let _ = avg_apply(&mut n, &mut cost, s_, q, 0);
                    // avg_apply realizes against price 0; keep the cost removal only.
                    leg.net = Qty(leg.net.0.saturating_sub(a.qty.0));
                    leg.cost = cost;
                }
                if !s.perp {
                    // inventory(base, group) −= qty, unassigned += qty (total unchanged).
                    self.inv_to_unassigned(av, s.base, a.group, a.qty.0);
                }
                self.revalue_leg(c, l);
            }
            let px = self.val_px(a.inst, a.qty.0);
            if let Some(px) = px {
                notional = self
                    .usdt_ro(
                        s.quote,
                        i128::from(a.qty.0).abs() * i128::from(px.0),
                        i32::from(s.px_exp) + i32::from(s.qty_exp),
                        RoundMode::TowardZero,
                    )
                    .0;
            }
            self.cell_upkeep(c, t, a.cmd_seq);
            self.refresh_summary(av);
        }
        self.push_note(note(
            code::RESIDUAL_WRITTEN_OFF,
            NoteLevel::P1,
            a.group,
            a.inst,
            [
                u64::from(a.account.0) | u64::from(a.venue.0) << 8,
                u64::from(a.group.0),
                u64::from(a.inst.0),
                a.qty.0 as u64,
                notional as u64,
                a.cmd_seq,
                0,
                0,
            ],
        ));
        out
    }
}
