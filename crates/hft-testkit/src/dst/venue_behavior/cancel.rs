//! Delay an original full-fill receipt across a real engine cancel. The matcher alone decides
//! whether the cancel is rejected; no order, fill, fee or cash fact is manufactured here.
use std::collections::VecDeque;

use hft_event::{GwCmd, GwOp, OrderEvent, OrderEventKind, RepMsg};
use hft_types::{AccountId, SlotId, VenueId};

use super::{EXTERNAL_HISTORY_CAP, VenueOut};

pub(super) const RELEASE_ID: &str = "cancel-filled-release";

/// One original receipt held by the fault. A request, a captured fill, a matcher rejection and
/// an actually delivered journal Input are distinct pieces of evidence.
#[derive(Clone, Debug)]
pub struct CancelFilledCase {
    pub requested_ns: u64,
    pub held_ns: u64,
    pub original_due_ns: u64,
    pub deadline_ns: u64,
    pub original: OrderEvent,
    pub command: Option<GwCmd>,
    pub command_ns: Option<u64>,
    pub rejection: Option<OrderEvent>,
    pub rejection_due_ns: Option<u64>,
    /// Why and when the original became eligible for transport. No rejection means timeout.
    pub release_ns: Option<u64>,
    /// The delivered representation, after optional LateFeeOnly splitting.
    pub report: Option<OrderEvent>,
    pub delivered_ns: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct CancelFilledReport {
    pub venue: VenueId,
    pub requested: u64,
    pub awaiting_fill: usize,
    pub awaiting_release: usize,
    pub cases: Vec<CancelFilledCase>,
}

#[derive(Debug, Default)]
pub(super) struct CancelFilled {
    requested: u64,
    requests: VecDeque<(u64, u32)>,
    cases: Vec<CancelFilledCase>,
    // Case index and earliest release time. Case evidence stays immutable after delivery.
    held: Vec<(usize, u64)>,
}

fn identity(a: &OrderEvent, b: &OrderEvent) -> bool {
    (
        a.venue,
        a.account,
        a.inst,
        a.epoch,
        a.slot,
        a.r#gen,
        a.exch_order_id,
    ) == (
        b.venue,
        b.account,
        b.inst,
        b.epoch,
        b.slot,
        b.r#gen,
        b.exch_order_id,
    )
}

impl CancelFilled {
    pub fn request(&mut self, count: u32, hold_ms: u32, now: u64) -> Result<(), String> {
        if count == 0 {
            return Ok(());
        }
        if hold_ms == 0 {
            return Err("cancel-filled hold must be positive".into());
        }
        let total = self.requested + u64::from(count);
        if total > EXTERNAL_HISTORY_CAP as u64 {
            return Err("cancel-filled request capacity exceeded".into());
        }
        self.requests
            .extend(std::iter::repeat_n((now, hold_ms), count as usize));
        self.requested = total;
        Ok(())
    }

    pub fn capture(&mut self, r: &VenueOut, now: u64) -> Result<bool, String> {
        let Some(&(requested_ns, hold_ms)) = self.requests.front() else {
            return Ok(false);
        };
        let RepMsg::OrderEvent(e) = r.rep else {
            return Ok(false);
        };
        if r.client_id == RELEASE_ID
            || e.what != OrderEventKind::Fill
            || e.detail != 0
            || e.slot == SlotId::UNRESOLVED
            || e.exch_order_id == 0
            || e.qty.0 <= 0
            || e.cum_filled.0 <= 0
        {
            return Ok(false);
        }
        let deadline_ns = r
            .due_ns
            .max(now)
            .checked_add(u64::from(hold_ms) * 1_000_000)
            .ok_or("cancel-filled deadline overflow")?;
        self.requests.pop_front();
        self.held.push((self.cases.len(), deadline_ns));
        self.cases.push(CancelFilledCase {
            requested_ns,
            held_ns: now,
            original_due_ns: r.due_ns,
            deadline_ns,
            original: e,
            command: None,
            command_ns: None,
            rejection: None,
            rejection_due_ns: None,
            release_ns: None,
            report: None,
            delivered_ns: None,
        });
        Ok(true)
    }

    pub fn command(&mut self, cmd: &GwCmd, now: u64) {
        if cmd.op != GwOp::Cancel {
            return;
        }
        for &(ix, _) in &self.held {
            let x = &mut self.cases[ix];
            let e = x.original;
            if (cmd.account, cmd.inst, cmd.target_slot, cmd.r#gen) == (e.account, e.inst, e.slot, e.r#gen)
                && x.command.is_none()
            {
                x.command = Some(*cmd);
                x.command_ns = Some(now);
            }
        }
    }

    pub fn rejected(&mut self, e: &OrderEvent, due: u64) -> Result<(), String> {
        if e.what != OrderEventKind::CancelReject || e.detail != 1 {
            return Ok(());
        }
        for (ix, ready) in &mut self.held {
            let x = &mut self.cases[*ix];
            if x.command.is_some()
                && x.rejection.is_none()
                && identity(&x.original, e)
                && e.cum_filled == x.original.cum_filled
                && e.avg_px == x.original.avg_px
                && e.cash_qty == x.original.cash_qty
                && e.cash_sum == x.original.cash_sum
            {
                let after = due.checked_add(1).ok_or("cancel-filled release time overflow")?;
                x.rejection = Some(*e);
                x.rejection_due_ns = Some(due);
                *ready = after;
            }
        }
        Ok(())
    }

    pub fn next_due(&self, stream_up: u64) -> Option<u64> {
        self.held.iter().map(|(_, at)| (*at).max(stream_up)).min()
    }

    pub fn release(&mut self, now: u64, stream_up: u64) -> Vec<VenueOut> {
        let mut out = Vec::new();
        self.held.retain(|(ix, at)| {
            if now < (*at).max(stream_up) {
                return true;
            }
            let x = &mut self.cases[*ix];
            x.release_ns = Some(now);
            out.push(VenueOut {
                due_ns: now,
                rep: RepMsg::OrderEvent(x.original),
                client_id: RELEASE_ID.into(),
            });
            false
        });
        out
    }

    pub fn scheduled(&mut self, e: &OrderEvent, due: u64) {
        if let Some(x) = self
            .cases
            .iter_mut()
            .rev()
            .find(|x| identity(&x.original, e) && x.report.is_none())
        {
            x.report = Some(*e);
            x.delivered_ns = Some(due);
        }
    }

    pub fn undelivered(&self, account: AccountId) -> bool {
        self.held
            .iter()
            .any(|(ix, _)| self.cases[*ix].original.account == account)
    }

    pub fn report(&self, venue: VenueId) -> CancelFilledReport {
        CancelFilledReport {
            venue,
            requested: self.requested,
            awaiting_fill: self.requests.len(),
            awaiting_release: self.held.len(),
            cases: self.cases.clone(),
        }
    }
}
