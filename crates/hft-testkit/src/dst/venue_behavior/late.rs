//! Transport copies of real fills. No synthetic terminal report or exchange execution.
use std::collections::VecDeque;

use hft_event::{OrderEvent, OrderEventKind, RecordFlags, RepMsg};
use hft_types::{SlotId, VenueId};
use zerocopy::IntoBytes;

use super::{EXTERNAL_HISTORY_CAP, VenueOut};
use crate::dst::obs::{Obs, SlotRow, TombRow};
use crate::dst::sut::JRec;

pub(super) const COPY_ID: &str = "late-recycle-copy";

/// Evidence for one transport copy. `original` precedes fee splitting; `report` is the
/// actually committed first delivery. Neither is reconstructed from engine balances.
#[derive(Clone, Debug)]
pub struct LateRecycleCopy {
    pub requested_ns: u64,
    pub original: OrderEvent,
    pub report: OrderEvent,
    pub original_seq: u64,
    /// Complete committed boundary at which the matching tomb was observed.
    pub recycle_seq: u64,
    pub recycle_ns: u64,
    pub tomb: TombRow,
    pub current: Option<SlotRow>,
    /// Last scheduled transport copy; actual delivery must be proven from the journal.
    pub delivered_ns: u64,
}

#[derive(Clone, Debug)]
pub struct LateRecycleReport {
    pub venue: VenueId,
    pub requested: u64,
    pub awaiting_fill: usize,
    pub awaiting_recycle: usize,
    pub copies: Vec<LateRecycleCopy>,
}

#[derive(Debug, Default)]
pub(super) struct LateRecycle {
    requested: u64,
    requests: VecDeque<u64>,
    held: Vec<(u64, OrderEvent, OrderEvent)>,
    copies: Vec<LateRecycleCopy>,
}

impl LateRecycle {
    pub fn request(&mut self, count: u32, now: u64) -> Result<(), String> {
        let total = self.requested + u64::from(count);
        if total > EXTERNAL_HISTORY_CAP as u64 {
            return Err("late recycle request capacity exceeded".into());
        }
        self.requests.extend(std::iter::repeat_n(now, count as usize));
        self.requested = total;
        Ok(())
    }

    pub fn capture(&mut self, original: &RepMsg, report: &RepMsg) {
        if let (RepMsg::OrderEvent(raw), RepMsg::OrderEvent(e)) = (original, report)
            && e.detail == 0
            && e.slot != SlotId::UNRESOLVED
            && e.exch_order_id != 0
            && e.cum_filled.0 > 0
            && e.qty.0 > 0
            && matches!(e.what, OrderEventKind::Fill | OrderEventKind::PartialFill)
            && let Some(requested) = self.requests.pop_front()
        {
            self.held.push((requested, *raw, *e));
        }
    }

    pub fn release(
        &mut self,
        obs: &Obs,
        journal: &[JRec],
        now: u64,
        latency: u64,
    ) -> Result<Vec<VenueOut>, String> {
        let mut replies = Vec::new();
        if self.held.is_empty() {
            return Ok(replies);
        }
        if let Some(e) = &obs.observation_error {
            return Err(e.clone());
        }
        let Some(end) = journal
            .iter()
            .rposition(|r| r.flags.contains(RecordFlags::EVENT_END))
        else {
            return Ok(replies);
        };
        let committed = &journal[..=end];
        let boundary = journal[end].end_seq;
        let mut ix = 0;
        while ix < self.held.len() {
            let (requested_ns, original, report) = self.held[ix];
            let current = obs
                .slots
                .iter()
                .find(|s| (s.venue, s.account, s.slot) == (report.venue, report.account, report.slot.0))
                .copied();
            let tomb = obs
                .tombs
                .iter()
                .find(|t| {
                    (t.venue, t.account, t.epoch, t.slot, t.r#gen, t.inst)
                        == (
                            report.venue,
                            report.account,
                            report.epoch,
                            report.slot.0,
                            report.r#gen,
                            report.inst,
                        )
                        && t.filled >= i128::from(report.cum_filled.0)
                })
                .copied();
            if current.is_some_and(|s| (s.epoch, s.r#gen) == (report.epoch, report.r#gen)) || tomb.is_none() {
                ix += 1;
                continue;
            }
            let Some(first) = committed
                .iter()
                .find(|r| r.kind == hft_event::EventKind::ORDER_EVENT && r.body == report.as_bytes())
            else {
                ix += 1;
                continue;
            };
            let due = now
                .checked_add(latency)
                .and_then(|t| t.checked_add(1))
                .ok_or("late recycle delivery time overflow")?;
            self.copies.push(LateRecycleCopy {
                requested_ns,
                original,
                report,
                original_seq: first.seq,
                recycle_seq: boundary,
                recycle_ns: now,
                tomb: tomb.unwrap(),
                current,
                delivered_ns: due,
            });
            replies.push(VenueOut {
                due_ns: due,
                rep: RepMsg::OrderEvent(report),
                client_id: COPY_ID.into(),
            });
            self.held.remove(ix);
        }
        Ok(replies)
    }

    pub fn scheduled(&mut self, e: &OrderEvent, due: u64) {
        if let Some(copy) = self
            .copies
            .iter_mut()
            .rev()
            .find(|c| c.report.as_bytes() == e.as_bytes())
        {
            copy.delivered_ns = due;
        }
    }

    pub fn report(&self, venue: VenueId) -> LateRecycleReport {
        LateRecycleReport {
            venue,
            requested: self.requested,
            awaiting_fill: self.requests.len(),
            awaiting_recycle: self.held.len(),
            copies: self.copies.clone(),
        }
    }
}
