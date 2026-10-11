//! Independent fee attribution and bounded-by-world link accounting across table reuse.

use std::collections::BTreeMap;

use hft_event::{EventKind, OrderEvent, OrderEventKind};
use hft_oms::{LinkState, OrderState, audit::LinkAudit};
use hft_sequencer::{Engine, types::InputSlot};
use hft_strategy_sdk::HedgeKind;
use hft_types::{AccountId, GroupId, MarketKind};

use crate::dst::obs::HedgeHistoryRow;

/// Link identity includes the order epoch and the event which created the link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkKey {
    pub id: u32,
    pub venue: u8,
    pub account: u8,
    pub epoch: u16,
    pub slot: u32,
    pub generation: u32,
    pub created_seq: u64,
}

#[derive(Clone, Debug)]
struct Tracked {
    key: LinkKey,
    row: LinkAudit,
    cap_offset: i128,
}

type GroupKey = (u8, u32, u32);
type HistoryKey = (u8, u32, u32, i64, i64);

#[derive(Clone, Debug, Default)]
pub struct History {
    live: BTreeMap<u32, Tracked>,
    pub retired: BTreeMap<HistoryKey, HedgeHistoryRow>,
    pub fees: BTreeMap<GroupKey, i128>,
    pub autofix_due: BTreeMap<(u8, u32), i64>,
    pub last_seq: u64,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Audit {
    pub history: History,
    checkpoint: History,
    rebase: bool,
    fee: Option<(GroupKey, i128)>,
}

impl Tracked {
    fn same(&self, r: &LinkAudit) -> bool {
        let f = r.link.first;
        self.key.venue == f.venue.0
            && self.key.account == f.account.0
            && self.key.slot == f.slot
            && self.key.generation == f.r#gen
            && (r.first_epoch == 0 || self.key.epoch == r.first_epoch)
            && (r.created_seq == 0 || self.key.created_seq == r.created_seq)
    }
}

impl History {
    fn tracked(&self, r: LinkAudit) -> Tracked {
        if let Some(old) = self.live.get(&r.link.link_id).filter(|old| old.same(&r)) {
            return Tracked {
                key: old.key,
                row: r,
                cap_offset: old.cap_offset,
            };
        }
        let f = r.link.first;
        Tracked {
            key: LinkKey {
                id: r.link.link_id,
                venue: f.venue.0,
                account: f.account.0,
                epoch: r.first_epoch,
                slot: f.slot,
                generation: f.r#gen,
                created_seq: r.created_seq,
            },
            row: r,
            cap_offset: 0,
        }
    }

    fn retire(&mut self, t: Tracked) {
        let r = t.row;
        let l = r.link;
        if r.plan.kind != HedgeKind::OnFill || l.flags & hft_oms::link_flags::REVERSING != 0 {
            return;
        }
        let key = (
            l.account.0,
            l.group.0,
            r.plan.inst.0,
            i64::from(r.plan.ratio_num),
            i64::from(r.plan.ratio_den),
        );
        if self.retired.len() >= 4_096 && !self.retired.contains_key(&key) {
            self.error = Some("retired link ratio history exceeds 4096 rows".into());
            return;
        }
        let h = self.retired.entry(key).or_insert(HedgeHistoryRow {
            account: l.account,
            group: l.group,
            inst: r.plan.inst,
            ratio_num: key.3,
            ratio_den: key.4,
            first_net: 0,
            hedge_req: 0,
            capped: 0,
            links: 0,
        });
        h.first_net += i128::from(l.first_net.0);
        h.hedge_req += i128::from(l.hedge_req.0);
        h.capped += r.capped + t.cap_offset;
        h.links += 1;
    }

    pub fn capped(&self, a: AccountId, g: GroupId) -> i128 {
        self.live
            .values()
            .filter(|t| {
                t.row.link.account == a
                    && t.row.link.group == g
                    && t.row.plan.kind == HedgeKind::OnFill
                    && t.row.link.flags & hft_oms::link_flags::REVERSING == 0
            })
            .map(|t| t.row.capped + t.cap_offset)
            .sum::<i128>()
            + self
                .retired
                .values()
                .filter(|h| h.account == a && h.group == g)
                .map(|h| h.capped)
                .sum::<i128>()
    }
}

impl Audit {
    /// Called only after the sequencer has published the completed round.
    pub fn commit(&mut self) {
        self.checkpoint.clone_from(&self.history);
    }

    pub fn restart(&mut self, committed_seq: u64) {
        if self.history.last_seq > committed_seq {
            self.history.clone_from(&self.checkpoint);
            if self.history.last_seq > committed_seq {
                self.history.error = Some("observation checkpoint is ahead of the published journal".into());
            }
        }
        self.rebase = true;
        self.fee = None;
    }

    pub fn before(&mut self, e: &Engine, input: &InputSlot) {
        if self.rebase {
            let ids: Vec<_> = self.history.live.keys().copied().collect();
            for id in ids {
                if let Some(row) = e.oms.audit_link(id)
                    && let Some(t) = self.history.live.get_mut(&id).filter(|t| t.same(&row))
                {
                    t.cap_offset += t.row.capped - row.capped;
                    t.row = row;
                } else if let Some(t) = self.history.live.remove(&id) {
                    self.history.retire(t);
                }
            }
            self.rebase = false;
        }
        self.fee = fee_correction(e, input);
    }

    pub fn after(&mut self, e: &Engine) {
        if e.cur_seq <= self.history.last_seq {
            return;
        }
        if e.oms.audit_overflow() {
            self.history.error = Some(format!("OMS diagnostic overflow at Input {}", e.cur_seq));
        }
        for f in e.exec.audit_arms() {
            self.history.autofix_due.insert(
                (f.account.0, f.group.0),
                f.engine_ns.saturating_add(i64::from(f.window_ms) * 1_000_000),
            );
        }
        for row in e.oms.audit_retired() {
            let t = self.history.tracked(*row);
            self.history.live.remove(&row.link.link_id);
            self.history.retire(t);
        }
        for row in e.oms.audit_links() {
            let id = row.link.link_id;
            let t = self.history.tracked(row);
            self.history.live.insert(id, t);
        }
        if let Some((key, fee)) = self.fee.take() {
            *self.history.fees.entry(key).or_default() += fee;
        }
        self.history.last_seq = e.cur_seq;
    }
}

/// Expected correction comes from the Input and its pre-event attribution, never the observed
/// residual delta or the ledger's NetAdjust output.
fn fee_correction(e: &Engine, input: &InputSlot) -> Option<(GroupKey, i128)> {
    if input.kind != EventKind::ORDER_EVENT {
        return None;
    }
    let ev = input.read::<OrderEvent>()?;
    if ev.what != OrderEventKind::FeeOnly || ev.fee.0 == 0 {
        return None;
    }
    let matched = e
        .oms
        .raw_slot(ev.venue, ev.account, ev.slot.0)
        .filter(|(s, _, _)| s.state != OrderState::Free && s.r#gen == ev.r#gen && s.epoch == ev.epoch)
        .map(|(s, _, _)| (s.group, s.inst))
        .or_else(|| {
            e.oms
                .tomb(ev.venue, ev.account, ev.slot.0, ev.r#gen)
                .filter(|t| t.epoch == ev.epoch)
                .map(|t| (t.group, t.inst))
        })?;
    let (group, inst) = matched;
    if group.0 == 0 || ev.venue.market() != Some(MarketKind::Spot) || e.rd.inst(inst).asset != ev.fee_asset {
        return None;
    }
    let id = e.oms.legs(ev.account, group).last_link;
    let l = e.oms.link(id)?;
    if l.state == LinkState::Broken {
        return None;
    }
    let first_inst = e.oms.slot(l.first).map(|s| s.inst).or_else(|| {
        e.oms
            .tomb(l.first.venue, l.first.account, l.first.slot, l.first.r#gen)
            .map(|t| t.inst)
    })?;
    if first_inst != inst {
        return None;
    }
    let p = e.oms.link_plan(id)?.1;
    let sign = match p.side {
        hft_types::Side::Buy => -1i128,
        hft_types::Side::Sell => 1,
    };
    let delta = -i128::from(ev.fee.0) * sign * i128::from(p.ratio_num) / i128::from(p.ratio_den.max(1));
    Some(((ev.account.0, group.0, p.inst.0), delta))
}
