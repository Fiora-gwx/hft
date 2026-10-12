//! Raw reconciliation receipts are the only source of cash, fees and execution identities.
//! The outcome Note authorizes the group; every booked receipt needs its derived fill Note.

use hft_event::{Note, NoteLevel, NoteSource, ReconcileBatchView, ReconcileReport};
use hft_types::{GROUP_NONE, INST_NONE, StrategyId};

use crate::{JournalApplyError as Error, Ledger, aflags, journal_foreign::Foreign, notes::code};

#[derive(Clone, Debug, Default)]
pub(crate) struct Reconcile {
    body: Vec<u8>,
    decision: Option<bool>,
    next: usize,
}

impl Reconcile {
    pub(crate) fn pending(&self) -> bool {
        !self.body.is_empty()
    }

    pub(crate) fn start(&mut self, body: &[u8]) -> Result<(), Error> {
        if self.pending() {
            return Err(Error::ReconcileEvidence);
        }
        ReconcileBatchView::read(body).map_err(|_| Error::ReconcileEvidence)?;
        self.body.extend_from_slice(body);
        self.decision = None;
        self.next = 0;
        Ok(())
    }

    pub(crate) fn outcome(&mut self, n: &Note, ledger: &Ledger) -> Result<(), Error> {
        let b = ReconcileBatchView::read(&self.body).map_err(|_| Error::ReconcileEvidence)?;
        if self.decision.is_some()
            || n.level != NoteLevel::Info
            || n.strategy != StrategyId(0)
            || n.group != GROUP_NONE
            || n.inst != INST_NONE
            || n._pad != 0
            || n.v[4] > 1
            || n.v
                != [
                    b.hdr.report_id,
                    u64::from(b.hdr.venue.0) | u64::from(b.hdr.account.0) << 8,
                    u64::from(b.hdr.n_reports),
                    u64::from(b.hdr.n_fills),
                    n.v[4],
                    0,
                    0,
                    0,
                ]
        {
            return Err(Error::ReconcileEvidence);
        }
        if n.v[4] == 1 {
            ledger.validate_reconcile_batch(&b).map_err(Error::Settlement)?;
            if b.reports().any(|r| {
                r.item == 1
                    && r.sflags & ReconcileReport::AUTO_EXTERNAL != 0
                    && ledger.pos(r.account, r.inst).qty != r.engine_qty
            }) {
                return Err(Error::ReconcileEvidence);
            }
        }
        self.decision = Some(n.v[4] == 1);
        Ok(())
    }

    pub(crate) fn fill(
        &mut self,
        n: &Note,
        ledger: &mut Ledger,
        foreign: &mut Foreign,
        t: i64,
    ) -> Result<(), Error> {
        if self.decision != Some(true) {
            return Err(Error::ReconcileEvidence);
        }
        let b = ReconcileBatchView::read(&self.body).map_err(|_| Error::ReconcileEvidence)?;
        // Position-row order, preserving execution order within each position.
        let mut index = 0;
        let mut found = None;
        for r in b
            .reports()
            .filter(|r| r.item == 1 && r.sflags & ReconcileReport::AUTO_EXTERNAL != 0)
        {
            for e in b.fills().filter(|e| e.inst == r.inst) {
                if index == self.next {
                    found = Some(e);
                }
                index += 1;
            }
        }
        let e = found.ok_or(Error::ReconcileEvidence)?;
        if n.source != NoteSource::Ledger
            || n.code != code::EXTERNAL_FILL
            || n._pad != 0
            || n.strategy != StrategyId(0)
            || n.inst != e.inst
            || n.v[1] != u64::from(e.inst.0)
            || n.v[2] != e.side as u64
            || n.v[3] != e.qty.0 as u64
            || n.v[4] != e.px.0 as u64
            || n.v[5] != e.exch_order_id
            || (n.v[0] >> 24) & aflags::AUTO_RECON == 0
        {
            return Err(Error::ReconcileEvidence);
        }
        if foreign.validate_external(&e)? {
            return Err(Error::UnsupportedExternalCash);
        }
        foreign.external(&e, false)?;
        let applied = ledger
            .apply_reconciled_external(&e, t)
            .map_err(Error::Settlement)?;
        if applied.is_none() || !ledger.notes().contains(n) {
            return Err(Error::ReconcileEvidence);
        }
        self.next += 1;
        Ok(())
    }

    pub(crate) fn end_event(&mut self) -> Result<(), Error> {
        if !self.pending() {
            return Ok(());
        }
        let b = ReconcileBatchView::read(&self.body).map_err(|_| Error::ReconcileEvidence)?;
        if self.decision.is_none() || (self.decision == Some(true) && self.next != usize::from(b.hdr.n_fills))
        {
            return Err(Error::ReconcileEvidence);
        }
        self.body.clear();
        self.decision = None;
        self.next = 0;
        Ok(())
    }
}
