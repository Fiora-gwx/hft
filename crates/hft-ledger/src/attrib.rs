//! External fills: de-duplication, `auto_held` offset, attribution (non-Shadow candidates, up to
//! the leg reaching zero), booking into the `external` column, the `AUTO_EXTERNAL` synthesis and
//! the OMS defensive-path note decoding (README "外部成交").

use hft_event::{ExternalCause, ExternalFill, Note, NoteLevel, NoteSource};
use hft_types::{ASSET_NONE, AccountId, AssetId, GROUP_NONE, InstId, Px, Qty, RoundMode, Side, Tsc, VenueId};

use crate::dedup::{ExtKey, KEY_ORDER_FILL, KEY_TRADE_ID};
use crate::ledger::Ledger;
use crate::notes::{code, note};
use crate::position::{Col, Trade};
use crate::types::*;

/// De-duplication key of an external fill (hft-venue-proto contract 22).
pub(crate) fn ext_key(e: &ExternalFill) -> ExtKey {
    if e.trade_id > 0 {
        ExtKey {
            inst: e.inst.0,
            kind: KEY_TRADE_ID,
            a: e.trade_id,
            b: 0,
        }
    } else {
        ExtKey {
            inst: e.inst.0,
            kind: KEY_ORDER_FILL,
            a: e.exch_order_id,
            b: e.fill_seq,
        }
    }
}

impl Ledger {
    /// Liquidation, ADL, manual, delivery or unknown fill: de-duplication, `auto_held` offset,
    /// attribution and booking. `None` = duplicate or fully offset by `auto_held`.
    pub fn apply_external(&mut self, e: &ExternalFill, auto_recon: bool, t: i64) -> Option<ExternalApplied> {
        self.tick(t);
        let s = self.inst(e.inst).copied()?;
        let av = self.av(e.account, e.venue)?;
        let p = self.pos_ix(e.account, e.inst)?;
        if e.qty.0 <= 0 {
            return None;
        }
        let key = ext_key(e);
        if self.avd.get(av).is_some_and(|d| d.ext.contains(&key)) {
            return None;
        }
        if let Some(d) = self.avd.get_mut(av) {
            d.ext.insert(key);
        }
        // auto_held offset.
        let mut qty = e.qty.0;
        let mut fee = e.fee.0;
        if !auto_recon
            && matches!(
                e.cause,
                ExternalCause::Adl | ExternalCause::Liquidation | ExternalCause::Unknown
            )
        {
            let held = self.posd.get(p).map_or(0, |d| match e.side {
                Side::Buy => d.auto_buy.0,
                Side::Sell => d.auto_sell.0,
            });
            let off = qty.min(held.max(0));
            if off > 0 {
                let after = held - off;
                if let Some(d) = self.posd.get_mut(p) {
                    match e.side {
                        Side::Buy => d.auto_buy = Qty(after),
                        Side::Sell => d.auto_sell = Qty(after),
                    }
                }
                self.push_note(note(
                    code::AUTO_HELD_OFFSET,
                    NoteLevel::P0,
                    GROUP_NONE,
                    e.inst,
                    [
                        u64::from(e.venue.0) | u64::from(e.account.0) << 8,
                        u64::from(e.inst.0),
                        e.side as u64,
                        e.qty.0 as u64,
                        off as u64,
                        after as u64,
                        0,
                        0,
                    ],
                ));
                let rest = qty - off;
                fee = if rest == 0 {
                    0
                } else {
                    crate::fixed::sat64(crate::fixed::div_round(
                        i128::from(e.fee.0) * i128::from(rest),
                        i128::from(qty),
                        RoundMode::Ceil,
                    ))
                };
                qty = rest;
                if qty == 0 {
                    return None;
                }
            }
        }
        // Net quantity (spot fee in the base asset).
        let fee_base = if !s.perp && e.fee_asset == s.base { fee } else { 0 };
        let net = if s.perp {
            qty
        } else if e.side == Side::Buy {
            qty.saturating_sub(fee_base)
        } else {
            qty.saturating_add(fee_base)
        };
        // Attribution.
        let (n, first) = self.candidates(e.account, e.inst);
        let mut aflags = if auto_recon { aflags::AUTO_RECON } else { 0 };
        let sign = e.side.sign();
        let (cell, g_qty) = match (n, first) {
            (1, Some((c, l))) => {
                let leg = self
                    .cells
                    .get(c)
                    .and_then(|x| x.legs.get(l))
                    .map_or(0, |x| x.net.0);
                let g = if leg != 0 && leg.signum() == -sign {
                    net.min(leg.saturating_abs())
                } else {
                    net
                };
                (Some((c, l)), g)
            }
            (k, _) => {
                if k > 1 {
                    aflags |= aflags::AMBIGUOUS;
                }
                (None, 0)
            }
        };
        let none_qty = net - g_qty;
        let gross_g = if net == 0 {
            0
        } else {
            crate::fixed::sat64(i128::from(qty) * i128::from(g_qty) / i128::from(net))
        };
        let mut d = PnlBreakdown::zero();
        if g_qty > 0 {
            let x = self.book_trade(&Trade {
                acc: e.account,
                inst: e.inst,
                side: e.side,
                gross: Qty(gross_g),
                net: Qty(g_qty),
                px: e.px,
                cell,
                col: Col::External,
                t,
                seq: 0,
                cash: true,
                defer_round: cell.map(|(c, _)| c),
            });
            d.add(&x);
        }
        if none_qty > 0 {
            let x = self.book_trade(&Trade {
                acc: e.account,
                inst: e.inst,
                side: e.side,
                gross: Qty(qty - gross_g),
                net: Qty(none_qty),
                px: e.px,
                cell: None,
                col: Col::External,
                t,
                seq: 0,
                cash: true,
                defer_round: cell.map(|(c, _)| c),
            });
            d.add(&x);
        }
        let fd = self.book_fee(
            e.account,
            e.inst,
            cell.map(|x| x.0),
            fee,
            e.fee_asset,
            true,
            fee_base != 0,
            e.px,
        );
        d.add(&fd);
        if let Some((c, _)) = cell {
            self.cell_upkeep(c, t, 0);
        }
        if auto_recon && let Some(x) = self.posd.get_mut(p) {
            match e.side {
                Side::Buy => x.auto_buy = Qty(x.auto_buy.0.saturating_add(e.qty.0)),
                Side::Sell => x.auto_sell = Qty(x.auto_sell.0.saturating_add(e.qty.0)),
            }
        }
        let group = cell
            .and_then(|(c, _)| self.cells.get(c))
            .map_or(GROUP_NONE, |x| x.group);
        let leg_ix = cell.map_or(0, |(_, l)| l as u8);
        let pe = i32::from(s.px_exp) + i32::from(s.qty_exp);
        let notional = self.usdt(
            s.quote,
            i128::from(net).saturating_mul(i128::from(e.px.0)),
            pe,
            RoundMode::TowardZero,
        );
        let level = if e.cause == ExternalCause::Manual {
            NoteLevel::P1
        } else {
            NoteLevel::P0
        };
        self.push_note(note(
            code::EXTERNAL_FILL,
            level,
            group,
            e.inst,
            [
                u64::from(e.venue.0) | u64::from(e.account.0) << 8 | (e.cause as u64) << 16 | aflags << 24,
                u64::from(e.inst.0),
                e.side as u64,
                e.qty.0 as u64,
                e.px.0 as u64,
                e.exch_order_id,
                u64::from(group.0),
                u64::from(leg_ix),
            ],
        ));
        Some(ExternalApplied {
            group,
            leg_ix,
            net_qty: Qty(sign.saturating_mul(g_qty)),
            none_qty: Qty(sign.saturating_mul(none_qty)),
            notional,
            cause: e.cause,
        })
    }

    /// The `AUTO_EXTERNAL` synthesized fill (pure; shared by the sequencer and cold readers):
    /// `engine_minus_exch > 0` → Sell, else Buy, at `mark_px`, key `(inst, 0, report_id)`.
    pub fn auto_external_fill(
        a: AccountId,
        v: VenueId,
        inst: InstId,
        engine_minus_exch: Qty,
        mark_px: Px,
        report_id: u64,
    ) -> ExternalFill {
        ExternalFill {
            venue: v,
            account: a,
            side: if engine_minus_exch.0 > 0 {
                Side::Sell
            } else {
                Side::Buy
            },
            cause: ExternalCause::Unknown,
            inst,
            qty: Qty(engine_minus_exch.0.saturating_abs()),
            px: mark_px,
            fee: Qty(0),
            fee_asset: ASSET_NONE,
            _pad: [0; 6],
            exch_order_id: 0,
            trade_id: 0,
            ex_ts_ns: 0,
            rx_tsc: Tsc(0),
            fill_seq: report_id,
            cash_qty: hft_types::Qty(0),
            cash_amount: hft_types::Qty(0),
        }
    }

    /// Cold readers: the external fill of an hft-oms `Note{OmsForeignFill}` (other notes →
    /// `None`). Layout: hft-oms "回报匹配与成交增量" item 8.
    pub fn external_from_oms_note(n: &Note) -> Option<ExternalFill> {
        if n.source != NoteSource::Oms || n.code != code::OMS_FOREIGN_FILL {
            return None;
        }
        let v0 = n.v[0];
        let side = match (v0 >> 16) & 0xFF {
            1 => Side::Buy,
            2 => Side::Sell,
            _ => return None,
        };
        Some(ExternalFill {
            venue: VenueId((v0 & 0xFF) as u8),
            account: AccountId(((v0 >> 8) & 0xFF) as u8),
            side,
            cause: ExternalCause::Unknown,
            inst: n.inst,
            qty: Qty(n.v[1] as i64),
            px: Px(n.v[2] as i64),
            fee: Qty(n.v[3] as i64),
            fee_asset: AssetId(((v0 >> 32) & 0xFFFF) as u16),
            _pad: [0; 6],
            exch_order_id: n.v[4],
            trade_id: n.v[5],
            ex_ts_ns: 0,
            rx_tsc: Tsc(0),
            fill_seq: n.v[6],
            cash_qty: hft_types::Qty(0),
            cash_amount: hft_types::Qty(0),
        })
    }

    /// External-fill de-duplication keys of `(a, v)` in booking order.
    pub fn ext_seen(&self, a: AccountId, v: VenueId) -> Vec<ExtKey> {
        self.av(a, v)
            .and_then(|av| self.avd.get(av))
            .map(|d| d.ext.iter().copied().collect())
            .unwrap_or_default()
    }
}
