//! Rounds: per-leg open/close aggregation, start and end against `round_dust_notional`, and
//! `Note{RoundClosed}` (README "轮次").

use hft_event::NoteLevel;
use hft_types::{AssetId, INST_NONE, Notional, Qty, RoundMode};

use crate::ledger::{GF_ROUND_OPEN, Ledger};
use crate::notes::{code, note};
use crate::types::*;

impl Ledger {
    /// Adds a trade of `s × q` at `px` to the round aggregation of leg `l` of cell `c`
    /// (opening while it moves away from zero, closing towards it).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn round_leg_trade(
        &mut self,
        c: usize,
        l: usize,
        quote: AssetId,
        pe: i32,
        s: i64,
        q: i64,
        px: i64,
    ) {
        let Some(net) = self.cells.get(c).and_then(|x| x.legs.get(l)).map(|g| g.net.0) else {
            return;
        };
        let (open, close) = if net == 0 || net.signum() == s {
            (q, 0)
        } else {
            let c0 = q.min(net.saturating_abs());
            (q - c0, c0)
        };
        let on = self.usdt(
            quote,
            i128::from(open) * i128::from(px),
            pe,
            RoundMode::TowardZero,
        );
        let cn = self.usdt(
            quote,
            i128::from(close) * i128::from(px),
            pe,
            RoundMode::TowardZero,
        );
        if let Some(a) = self.cells.get_mut(c).and_then(|x| x.book.round.legs.get_mut(l)) {
            a.open_qty = Qty(a.open_qty.0.saturating_add(open));
            a.open_notional = Notional(a.open_notional.0.saturating_add(on.0));
            a.close_qty = Qty(a.close_qty.0.saturating_add(close));
            a.close_notional = Notional(a.close_notional.0.saturating_add(cn.0));
        }
    }

    /// Held notional, peak, and round start/end of cell `c` after a change at `t`.
    pub(crate) fn cell_upkeep(&mut self, c: usize, t: i64, seq: u64) {
        let Some(cell) = self.cells.get(c) else { return };
        let legs = cell.legs;
        let mut held = 0i64;
        let mut any_over = false;
        for leg in legs.iter() {
            if leg.inst == INST_NONE || leg.net.0 == 0 {
                continue;
            }
            let n = self.notional_of(leg.inst, leg.net);
            held = held.saturating_add(n.0);
            if n.0 >= self.cfg.round_dust.0 {
                any_over = true;
            }
        }
        let dust = self.cfg.round_dust;
        let Some(cell) = self.cells.get_mut(c) else { return };
        cell.book.held_notional = Notional(held);
        let open = cell.gflags & GF_ROUND_OPEN != 0;
        if !open && any_over {
            let r = &mut cell.book.round;
            r.round_id = r.round_id.wrapping_add(1);
            r.open_seq = seq;
            r.open_ens = t;
            r.peak_notional = Notional(held);
            cell.gflags |= GF_ROUND_OPEN;
            return;
        }
        if open {
            if held > cell.book.round.peak_notional.0 {
                cell.book.round.peak_notional = Notional(held);
            }
            if !any_over {
                let _ = dust;
                self.close_round(c, t);
            }
        }
    }

    /// Writes one `RoundClosed` per active leg and resets the round.
    fn close_round(&mut self, c: usize, t: i64) {
        let Some(cell) = self.cells.get(c) else { return };
        let r = cell.book.round;
        let g = cell.group;
        let dur = t.saturating_sub(r.open_ens);
        let mut notes = [None; 4];
        for (l, a) in r.legs.iter().enumerate() {
            if a.open_qty.0 == 0 && a.close_qty.0 == 0 {
                continue;
            }
            let inst = cell.legs.get(l).map_or(INST_NONE, |x| x.inst);
            if let Some(n) = notes.get_mut(l) {
                *n = Some(note(
                    code::ROUND_CLOSED,
                    NoteLevel::Info,
                    g,
                    inst,
                    [
                        u64::from(r.round_id) | (l as u64) << 32,
                        dur as u64,
                        a.open_qty.0 as u64,
                        a.open_notional.0 as u64,
                        a.close_qty.0 as u64,
                        a.close_notional.0 as u64,
                        r.pnl.price_realized.0 as u64,
                        r.pnl.net().0 as u64,
                    ],
                ));
            }
        }
        for n in notes.into_iter().flatten() {
            self.push_note(n);
        }
        if let Some(cell) = self.cells.get_mut(c) {
            let id = cell.book.round.round_id;
            cell.book.round = RoundBook::ZERO;
            cell.book.round.round_id = id;
            cell.gflags &= !GF_ROUND_OPEN;
        }
    }
}
