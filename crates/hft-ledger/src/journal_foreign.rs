//! Independent cumulative settlement proof for defensive OMS fills. Only a matching Note in
//! the original Input's event authorizes booking; no amount is copied from an output.

use hft_event::{ExternalFill, Note, NoteLevel, OrderEvent, OrderEventKind};
use hft_strategy_sdk::RestoreError;
use hft_types::{AccountId, GROUP_NONE, InstId, Qty, VenueId};

use crate::{JournalApplyError as Error, Ledger};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Key(VenueId, AccountId, InstId, u64);

#[derive(Clone, Copy, Debug)]
struct Cursor {
    key: Key,
    // 0 = original ExternalFill identity, 1/2 = foreign cash cursor.
    side: u8,
    filled: i64,
    cash: i64,
}

#[derive(Clone, Copy, Debug)]
struct Pending {
    event: OrderEvent,
    next: Cursor,
    qty: i64,
    cash: i64,
    at: i64,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Foreign {
    rows: Vec<Cursor>,
    pending: Option<Pending>,
}

impl Foreign {
    pub(crate) fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub(crate) fn overlaps_order(&self, e: &OrderEvent) -> bool {
        e.exch_order_id != 0
            && self
                .rows
                .iter()
                .any(|r| r.key == Key(e.venue, e.account, e.inst, e.exch_order_id))
    }

    pub(crate) fn sim_reset(&mut self, account: AccountId) {
        self.rows.retain(|r| r.key.1 != account);
    }

    pub(crate) fn restore_row(&mut self, b: &[u8], ledger: &Ledger) -> Result<(), RestoreError> {
        if b.len() != 32 {
            return Err(RestoreError::BadValue);
        }
        // Decode the owning crate's documented row independently; no dependency on OMS.
        let u32_at = |at: usize| {
            b.get(at..at + 4)
                .and_then(|x| x.try_into().ok())
                .map(u32::from_le_bytes)
                .ok_or(RestoreError::Truncated)
        };
        let u64_at = |at: usize| {
            b.get(at..at + 8)
                .and_then(|x| x.try_into().ok())
                .map(u64::from_le_bytes)
                .ok_or(RestoreError::Truncated)
        };
        let side = b.get(2).copied().ok_or(RestoreError::Truncated)?;
        let key = Key(VenueId(b[0]), AccountId(b[1]), InstId(u32_at(4)?), u64_at(8)?);
        let filled = u64_at(16)? as i64;
        let cash = u64_at(24)? as i64;
        if b[3] != 0
            || key.3 == 0
            || side > 2
            || (side == 0 && (filled != 0 || cash != 0))
            || (side != 0 && filled <= 0)
            || ledger
                .inst(key.2)
                .is_none_or(|i| !i.perp_cash || i.venue != key.0)
            || ledger.av(key.1, key.0).is_none()
            || self.rows.iter().any(|r| r.key == key)
        {
            return Err(RestoreError::BadValue);
        }
        if self.rows.len() == super::journal::SLOT_MAP_CAP {
            return Err(RestoreError::Capacity);
        }
        self.rows.push(Cursor {
            key,
            side,
            filled,
            cash,
        });
        Ok(())
    }

    pub(crate) fn external(&mut self, e: &ExternalFill) -> Result<(), Error> {
        if e.exch_order_id == 0 {
            return Ok(());
        }
        let key = Key(e.venue, e.account, e.inst, e.exch_order_id);
        if let Some(r) = self.rows.iter().find(|r| r.key == key) {
            return if r.side == 0 {
                Ok(())
            } else {
                Err(Error::UnsupportedExternalCash)
            };
        }
        if self.rows.len() == super::journal::SLOT_MAP_CAP {
            return Err(Error::ForeignCapacity);
        }
        self.rows.push(Cursor {
            key,
            side: 0,
            filled: 0,
            cash: 0,
        });
        Ok(())
    }

    pub(crate) fn prepare(&mut self, e: &OrderEvent, ledger: &Ledger, at: i64) -> Result<bool, Error> {
        if self.pending.is_some() {
            return Err(Error::ForeignEvidence);
        }
        if e.exch_order_id == 0
            || !matches!(e.what, OrderEventKind::PartialFill | OrderEventKind::Fill)
            || !matches!(e.detail, 1 | 2)
        {
            return Err(Error::Unmapped);
        }
        if ledger.inst(e.inst).is_none_or(|i| i.venue != e.venue) || ledger.av(e.account, e.venue).is_none() {
            return Err(Error::Identity);
        }
        let key = Key(e.venue, e.account, e.inst, e.exch_order_id);
        let old = self.rows.iter().find(|r| r.key == key);
        if let Some(r) = old {
            if r.side == 0 {
                return Err(Error::UnsupportedExternalCash);
            }
            if r.side != e.detail {
                return Err(Error::Identity);
            }
        }
        let filled = old.map_or(0, |r| r.filled);
        let cash = old.map_or(0, |r| r.cash);
        if e.cum_filled.0 < 0
            || e.cash_qty.0 < 0
            || e.cash_qty > e.cum_filled
            || (e.cash_qty.0 == 0 && e.cash_sum.0 != 0)
        {
            return Err(Error::CashCoverage);
        }
        if e.cum_filled.0 < filled {
            return Ok(false);
        }
        if e.cash_qty != e.cum_filled {
            return Err(Error::MissingCash);
        }
        if e.cum_filled.0 == filled {
            return if e.cash_sum.0 == cash {
                Ok(false)
            } else {
                Err(Error::CashConflict)
            };
        }
        if old.is_none() && self.rows.len() == super::journal::SLOT_MAP_CAP {
            return Err(Error::ForeignCapacity);
        }
        self.pending = Some(Pending {
            event: *e,
            next: Cursor {
                key,
                side: e.detail,
                filled: e.cum_filled.0,
                cash: e.cash_sum.0,
            },
            qty: e.cum_filled.0 - filled,
            cash: e.cash_sum.0.checked_sub(cash).ok_or(Error::CashOverflow)?,
            at,
        });
        Ok(true)
    }

    pub(crate) fn note(&mut self, n: &Note, ledger: &mut Ledger, at: i64) -> Result<(), Error> {
        let p = self.pending.ok_or(Error::ForeignEvidence)?;
        let e = p.event;
        let path = if e.slot.0 == 0 { 1u64 } else { 2 };
        let px = if e.avg_px.0 != 0 { e.avg_px } else { e.px };
        let v0 = u64::from(e.venue.0)
            | u64::from(e.account.0) << 8
            | u64::from(e.detail) << 16
            | path << 24
            | u64::from(e.fee_asset.0) << 32;
        if p.at != at
            || n.inst != e.inst
            || n.group != GROUP_NONE
            || n.level != NoteLevel::Info
            || n.v
                != [
                    v0,
                    p.qty as u64,
                    px.0 as u64,
                    e.fee.0 as u64,
                    e.exch_order_id,
                    0,
                    e.cum_filled.0 as u64,
                    e.cum_filled.0 as u64,
                ]
        {
            return Err(Error::ForeignEvidence);
        }
        let mut fill = Ledger::external_from_oms_note(n).ok_or(Error::ForeignEvidence)?;
        fill.cash_qty = Qty(p.qty);
        fill.cash_amount = Qty(p.cash);
        if ledger
            .apply_settled_external(&fill, at)
            .map_err(Error::Settlement)?
            .is_none()
        {
            return Err(Error::ForeignEvidence);
        }
        if let Some(r) = self.rows.iter_mut().find(|r| r.key == p.next.key) {
            *r = p.next;
        } else {
            self.rows.push(p.next);
        }
        self.pending = None;
        Ok(())
    }

    pub(crate) fn end_event(&self) -> Result<(), Error> {
        if self.pending.is_some() {
            Err(Error::ForeignEvidence)
        } else {
            Ok(())
        }
    }
}
