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
        self.external_with_cash(e, auto_recon, None, t)
    }

    /// An external perpetual execution carrying complete actual cash facts. The existing
    /// trade key owns de-duplication, including after restore. Cash is booked once even if
    /// attribution splits the position between a group and GROUP_NONE.
    /// Unproven AUTO_EXTERNAL overlap fails before time, keys or balances change.
    /// No allocation; budget: ledger/apply_settled_external.
    pub fn apply_settled_external(
        &mut self,
        e: &ExternalFill,
        t: i64,
    ) -> Result<Option<ExternalApplied>, SettlementError> {
        let av = self.validate_external_cash(e)?;
        let key = ext_key(e);
        if let Some(ix) = self
            .avd
            .get(av)
            .and_then(|d| d.auto_cash.iter().position(|r| ext_key(r) == key))
        {
            let receipt = self
                .avd
                .get(av)
                .and_then(|d| d.auto_cash.get(ix))
                .ok_or(SettlementError::ReceiptIdentity)?;
            let diff = receipt_diff(receipt, e);
            if diff != 0 {
                // The original push disagrees on money with the pending receipt of the same
                // execution: neither is trusted to settle the other. Book nothing, keep the
                // receipt pending and raise a P0 alarm (README "原始凭据补账").
                self.push_note(note(
                    code::RECEIPT_MISMATCH,
                    NoteLevel::P0,
                    GROUP_NONE,
                    e.inst,
                    [
                        u64::from(e.venue.0) | u64::from(e.account.0) << 8,
                        u64::from(e.inst.0),
                        e.trade_id,
                        e.exch_order_id,
                        e.fill_seq,
                        u64::from(diff),
                        0,
                        0,
                    ],
                ));
                return Ok(None);
            }
            let after = self
                .auto_held(e.account, e.inst, e.side)
                .0
                .checked_sub(e.qty.0)
                .filter(|n| *n >= 0)
                .ok_or(SettlementError::ExternalOverlap)?;
            let p = self
                .pos_ix(e.account, e.inst)
                .ok_or(SettlementError::UnknownInstrument)?;
            self.tick(t);
            if let Some(d) = self.avd.get_mut(av) {
                d.auto_cash.remove(ix);
                // The ordinary FIFO may have wrapped while this receipt was pending.
                if !d.ext.contains(&key) {
                    d.ext.insert(key);
                }
            }
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
                    e.qty.0 as u64,
                    after as u64,
                    0,
                    0,
                ],
            ));
            return Ok(None);
        }
        if self.avd.get(av).is_some_and(|d| d.ext.contains(&key)) {
            return Ok(None);
        }
        if eligible_auto(e) && self.opaque_auto_held(av, e) {
            return Err(SettlementError::ExternalOverlap);
        }
        Ok(self.external_with_cash(e, false, Some(e.cash_amount), t))
    }

    /// Book an independently supplied execution once and retain its exact receipt until the
    /// matching original push arrives. Capacity and payload conflicts fail before mutation.
    /// No allocation; budget: ledger/apply_reconciled_external.
    pub fn apply_reconciled_external(
        &mut self,
        e: &ExternalFill,
        t: i64,
    ) -> Result<Option<ExternalApplied>, SettlementError> {
        let av = self.validate_external_cash(e)?;
        if !eligible_auto(e)
            || (e.trade_id == 0 && (e.exch_order_id == 0 || e.fill_seq == 0))
            || (e.trade_id != 0 && e.fill_seq != 0)
            || e._pad != [0; 6]
        {
            return Err(SettlementError::ReceiptIdentity);
        }
        let d = self.avd.get(av).ok_or(SettlementError::AccountVenue)?;
        if let Some(r) = d.auto_cash.iter().find(|r| ext_key(r) == ext_key(e)) {
            return if receipt_diff(r, e) == 0 {
                Ok(None)
            } else {
                Err(SettlementError::ReceiptConflict)
            };
        }
        if d.ext.contains(&ext_key(e)) {
            return Ok(None);
        }
        if d.auto_cash.len() >= self.cfg.ext_cap {
            return Err(SettlementError::ReceiptCapacity);
        }
        if self.opaque_auto_held(av, e)
            || self
                .auto_held(e.account, e.inst, e.side)
                .0
                .checked_add(e.qty.0)
                .is_none()
        {
            return Err(SettlementError::ExternalOverlap);
        }
        let applied = self.external_with_cash(e, true, Some(e.cash_amount), t);
        if applied.is_some()
            && let Some(d) = self.avd.get_mut(av)
        {
            let mut receipt = *e;
            receipt.rx_tsc = Tsc(0);
            d.auto_cash.push(receipt);
        }
        Ok(applied)
    }

    /// Preflight every execution and its position delta before an atomic reconciliation.
    /// Pending receipts cannot be evicted, and an already booked key cannot explain a new gap.
    /// No allocation; budget: ledger/validate_reconcile_batch.
    pub fn validate_reconcile_batch(
        &self,
        batch: &hft_event::ReconcileBatchView<'_>,
    ) -> Result<(), SettlementError> {
        let av = self
            .av(batch.hdr.account, batch.hdr.venue)
            .ok_or(SettlementError::AccountVenue)?;
        let d = self.avd.get(av).ok_or(SettlementError::AccountVenue)?;
        if d.auto_cash.len().saturating_add(usize::from(batch.hdr.n_fills)) > self.cfg.ext_cap {
            return Err(SettlementError::ReceiptCapacity);
        }
        for (i, e) in batch.fills().enumerate() {
            self.validate_external_cash(&e)?;
            if !self.inst(e.inst).is_some_and(|s| s.perp_cash)
                || !eligible_auto(&e)
                || (e.trade_id == 0 && (e.exch_order_id == 0 || e.fill_seq == 0))
                || (e.trade_id != 0 && e.fill_seq != 0)
                || d.ext.contains(&ext_key(&e))
                || d.auto_cash.iter().any(|r| ext_key(r) == ext_key(&e))
                || batch.fills().take(i).any(|r| ext_key(&r) == ext_key(&e))
            {
                return Err(SettlementError::ReceiptIdentity);
            }
            if self.opaque_auto_held(av, &e) {
                return Err(SettlementError::ExternalOverlap);
            }
            let r = batch
                .reports()
                .find(|r| {
                    r.item == 1
                        && r.inst == e.inst
                        && r.sflags & hft_event::ReconcileReport::AUTO_EXTERNAL != 0
                })
                .ok_or(SettlementError::ReceiptIdentity)?;
            let delta = i128::from(r.exch_qty.0) - i128::from(r.engine_qty.0);
            if delta.signum() != i128::from(e.side.sign()) {
                return Err(SettlementError::InvalidQuantity);
            }
        }
        for r in batch
            .reports()
            .filter(|r| r.item == 1 && r.sflags & hft_event::ReconcileReport::AUTO_EXTERNAL != 0)
        {
            let qty: i128 = batch
                .fills()
                .filter(|e| e.inst == r.inst)
                .map(|e| i128::from(e.qty.0))
                .sum();
            let delta = (i128::from(r.engine_qty.0) - i128::from(r.exch_qty.0)).abs();
            if qty == 0
                || qty != delta
                || delta > i128::from(i64::MAX)
                || (r.exch_qty.0 != 0 && r.engine_qty.0.signum() != r.exch_qty.0.signum())
                || i128::from(r.exch_qty.0).abs() >= i128::from(r.engine_qty.0).abs()
            {
                return Err(SettlementError::InvalidQuantity);
            }
            let side = if r.engine_qty.0 > 0 { Side::Sell } else { Side::Buy };
            if i128::from(self.auto_held(r.account, r.inst, side).0) + qty > i128::from(i64::MAX) {
                return Err(SettlementError::InvalidQuantity);
            }
        }
        Ok(())
    }

    pub(crate) fn validate_external_cash(&self, e: &ExternalFill) -> Result<usize, SettlementError> {
        let s = self.inst(e.inst).ok_or(SettlementError::UnknownInstrument)?;
        if !s.perp {
            return Err(SettlementError::NotPerpetual);
        }
        if s.venue != e.venue {
            return Err(SettlementError::VenueMismatch);
        }
        let av = self.av(e.account, e.venue).ok_or(SettlementError::AccountVenue)?;
        if e.qty.0 <= 0 {
            return Err(SettlementError::InvalidQuantity);
        }
        if e.px.0 <= 0 {
            return Err(SettlementError::InvalidPrice);
        }
        if e.cash_qty.0 < 0 || e.cash_qty.0 > e.qty.0 || (e.cash_qty.0 == 0 && e.cash_amount.0 != 0) {
            return Err(SettlementError::CashCoverage);
        }
        if e.cash_qty != e.qty {
            return Err(SettlementError::MissingCash);
        }
        if (e.fee_asset == ASSET_NONE && e.fee.0 != 0)
            || (e.fee_asset != ASSET_NONE && self.assets.get(usize::from(e.fee_asset.0)).is_none())
        {
            return Err(SettlementError::WrongAsset);
        }
        Ok(av)
    }

    fn opaque_auto_held(&self, av: usize, e: &ExternalFill) -> bool {
        let exact = self.avd.get(av).map_or(0i128, |d| {
            d.auto_cash
                .iter()
                .filter(|r| r.inst == e.inst && r.side == e.side)
                .map(|r| i128::from(r.qty.0))
                .sum()
        });
        exact != i128::from(self.auto_held(e.account, e.inst, e.side).0)
    }

    fn external_with_cash(
        &mut self,
        e: &ExternalFill,
        auto_recon: bool,
        cash: Option<Qty>,
        t: i64,
    ) -> Option<ExternalApplied> {
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
        if cash.is_none()
            && !auto_recon
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
            let x = self.book_trade_with_cash(
                &Trade {
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
                },
                cash,
            );
            d.add(&x);
        }
        if none_qty > 0 {
            let x = self.book_trade_with_cash(
                &Trade {
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
                },
                cash.map(|amount| if g_qty > 0 { Qty(0) } else { amount }),
            );
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

/// `ReceiptMismatch.v[5]` bits: which fields of two same-key executions differ.
pub mod receipt_field {
    pub const QTY: u32 = 1;
    pub const PX: u32 = 2;
    pub const FEE: u32 = 4;
    pub const FEE_ASSET: u32 = 8;
    pub const CASH: u32 = 16;
    pub const SIDE: u32 = 32;
    pub const IDENTITY: u32 = 64;
    pub const CAUSE: u32 = 128;
}

/// Compares a pending receipt with another report of the same de-duplication key on the
/// execution identity and the money fields only. Timestamps (`ex_ts_ns`, `rx_tsc`) are delivery
/// facts and may differ between a REST receipt and the WS push; `fill_seq` only identifies an
/// execution without a trade id. Liquidation, ADL and Unknown are one cause class. `0` = same.
pub(crate) fn receipt_diff(a: &ExternalFill, b: &ExternalFill) -> u32 {
    use receipt_field as f;
    let mut d = 0;
    if a.qty != b.qty {
        d |= f::QTY;
    }
    if a.px != b.px {
        d |= f::PX;
    }
    if a.fee != b.fee {
        d |= f::FEE;
    }
    if a.fee_asset != b.fee_asset {
        d |= f::FEE_ASSET;
    }
    if a.cash_qty != b.cash_qty || a.cash_amount != b.cash_amount {
        d |= f::CASH;
    }
    if a.side != b.side {
        d |= f::SIDE;
    }
    if a.venue != b.venue
        || a.account != b.account
        || a.inst != b.inst
        || a.exch_order_id != b.exch_order_id
        || a.trade_id != b.trade_id
        || (a.trade_id == 0 && a.fill_seq != b.fill_seq)
        || a._pad != b._pad
    {
        d |= f::IDENTITY;
    }
    if a.cause != b.cause && !(eligible_auto(a) && eligible_auto(b)) {
        d |= f::CAUSE;
    }
    d
}

pub(crate) fn eligible_auto(e: &ExternalFill) -> bool {
    matches!(
        e.cause,
        ExternalCause::Adl | ExternalCause::Liquidation | ExternalCause::Unknown
    )
}
