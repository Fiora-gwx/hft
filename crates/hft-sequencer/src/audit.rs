//! Bounded, read-only test diagnostics. Not journaled, snapshotted or built into production.

use hft_event::{OrderAction, OrderDecision};
use hft_exec::ExecOrderKind;
use hft_oms::{
    ApprovedIntent, Origin, PlaceError,
    audit::{HedgeFacts, LinkAudit},
};
use hft_strategy_sdk::{IntentClass, LegView, LinkRole, OrderIntent, TargetReason};
use hft_types::{AccountId, AssetExp, GroupId, InstId, Px, Qty, Rate, StrategyId, VenueId};

use crate::{Engine, intents::Src};

pub const CAPACITY: usize = 512;

/// Immutable state at a completed Input or boot boundary. Available only to test drivers.
pub struct StateView<'a, S: hft_strategy_sdk::StrategySet> {
    pub(crate) e: &'a Engine,
    pub(crate) inst: &'a crate::Instances<S>,
}

impl<S: hft_strategy_sdk::StrategySet> core::fmt::Debug for StateView<'_, S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StateView").finish_non_exhaustive()
    }
}

impl<S: hft_strategy_sdk::StrategySet> StateView<'_, S> {
    /// Serializes the live components, independently of any journal snapshot body.
    pub fn write_sections(&self, w: &mut hft_strategy_sdk::SnapshotWriter) {
        crate::snapshot::write_sections(self.e, self.inst, w);
    }

    /// Complete ledger views at this boundary, including derived fields absent from snapshots.
    pub fn ledger(&self) -> &hft_ledger::Ledger {
        &self.e.ledger
    }

    /// The immutable reference data that defines the ledger's account/venue/inst universe.
    pub fn refdata(&self) -> &hft_refdata::RefData {
        &self.e.rd
    }
}

/// Original facts captured after pricing/classification, before risk commit and OMS place.
#[derive(Copy, Clone, Debug)]
pub struct DecisionFacts {
    pub class: IntentClass,
    /// 1 strategy, 2 exec, 3 OMS, 4 command.
    pub origin: u8,
    pub exec_kind: Option<ExecOrderKind>,
    pub margin_stress: bool,
    pub hedge: Option<(LinkRole, Option<HedgeFacts>)>,
    pub link: u32,
    /// Unknown slots belonging to this link, sampled independently of the group's counters.
    pub link_unknown: bool,
    pub legs: LegView,
    pub leg_insts: [InstId; 4],
    pub pos: Qty,
    pub open_buy: Qty,
    pub open_sell: Qty,
    pub imm_buy: Qty,
    pub imm_sell: Qty,
    pub max_pos_qty: Qty,
    pub exp: AssetExp,
    pub quote_rate: Rate,
    pub max_hedge_notional: i64,
    pub max_reduce_notional: i64,
    pub exec_cell: Option<hft_exec::TargetCell>,
    pub group_positions: [Qty; 4],
}

#[derive(Copy, Clone, Debug)]
pub struct DecisionRow {
    pub seq: u64,
    pub facts: DecisionFacts,
    index: u32,
}

/// Raw state immediately before an OMS hedge calls `place`. No outcome is inferred from counters.
#[derive(Copy, Clone, Debug)]
pub struct PlaceFacts {
    pub input_seq: u64,
    pub link_id: u32,
    pub link: Option<LinkAudit>,
    pub venue: VenueId,
    pub account: AccountId,
    pub group: GroupId,
    pub inst: InstId,
    pub free_slots: Option<u32>,
}

#[derive(Copy, Clone, Debug)]
pub struct PlaceFailure {
    pub facts: PlaceFacts,
    pub error: PlaceError,
}

/// Completed cold-restore calls. These survive event-level diagnostic resets.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RestoreRow {
    ModelBudget {
        strategy: StrategyId,
        mem: u64,
        ckpt: u64,
    },
    LedgerGroups {
        strategy: StrategyId,
        groups: u16,
    },
    RiskGroups {
        account: AccountId,
        groups: u16,
    },
    RestoreMarks {
        count: usize,
    },
    Mark {
        inst: InstId,
        px: Px,
        engine_ns: i64,
    },
    FundingInterval {
        inst: InstId,
        seconds: u32,
    },
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RestoreObservationError {
    Overflow,
}

#[derive(Debug)]
pub(crate) struct Audit {
    rows: Vec<DecisionRow>,
    pending: Vec<(OrderDecision, DecisionFacts)>,
    place_failures: Vec<PlaceFailure>,
    failed: bool,
    restore: Vec<RestoreRow>,
    restore_failed: bool,
}

impl Audit {
    pub fn new() -> Self {
        Self {
            rows: Vec::with_capacity(CAPACITY),
            pending: Vec::with_capacity(CAPACITY),
            place_failures: Vec::with_capacity(CAPACITY),
            failed: false,
            restore: Vec::with_capacity(CAPACITY),
            restore_failed: false,
        }
    }
    pub fn begin(&mut self) {
        self.rows.clear();
        self.pending.clear();
        self.place_failures.clear();
        self.failed = false;
    }
    pub fn restored(&mut self, row: RestoreRow) {
        if self.restore.len() == CAPACITY {
            self.restore_failed = true;
        } else {
            self.restore.push(row);
        }
    }
    fn restore_rows(&self) -> Result<&[RestoreRow], RestoreObservationError> {
        if self.restore_failed {
            Err(RestoreObservationError::Overflow)
        } else {
            Ok(&self.restore)
        }
    }
    pub fn place_failed(&mut self, facts: PlaceFacts, error: PlaceError) {
        if self.place_failures.len() == CAPACITY {
            self.failed = true;
            return;
        }
        self.place_failures.push(PlaceFailure { facts, error });
    }
    pub fn record(&mut self, index: u32, facts: DecisionFacts) {
        if self.rows.len() == CAPACITY {
            self.failed = true;
            return;
        }
        self.rows.push(DecisionRow { index, seq: 0, facts });
    }
    pub fn pending(&mut self, d: OrderDecision, f: DecisionFacts) {
        if d.action != OrderAction::New {
            return;
        }
        if self.pending.len() == CAPACITY {
            self.failed = true;
            return;
        }
        self.pending.push((d, f));
    }
    pub fn bind(&mut self, d: &OrderDecision, index: u32) {
        if d.action != OrderAction::New {
            return;
        }
        let k = self.pending.iter().position(|(p, _)| {
            p.venue == d.venue
                && p.account == d.account
                && p.slot == d.slot
                && p.r#gen == d.r#gen
                && p.intent_no == d.intent_no
                && p.inst == d.inst
                && p.link_role == d.link_role
        });
        if let Some(k) = k {
            let (_, f) = self.pending.remove(k);
            self.record(index, f);
        } else {
            self.failed = true;
        }
    }
    pub fn finish(&mut self, seqs: &[u64]) {
        if !self.pending.is_empty() {
            self.failed = true;
        }
        for r in &mut self.rows {
            r.seq = seqs.get(r.index as usize).copied().unwrap_or(0);
            self.failed |= r.seq == 0;
        }
    }
}

impl Engine {
    pub fn hedge_place_failures(&self) -> &[PlaceFailure] {
        &self.audit.place_failures
    }
    pub fn decision_observations(&self) -> &[DecisionRow] {
        &self.audit.rows
    }
    pub fn decision_observation_failed(&self) -> bool {
        self.audit.failed
    }
    /// Overflow rejects the whole observation instead of returning a partial success.
    pub fn restore_observations(&self) -> Result<&[RestoreRow], RestoreObservationError> {
        self.audit.restore_rows()
    }
}

pub(crate) fn capture_place(e: &Engine, it: &ApprovedIntent) -> Option<PlaceFacts> {
    (it.origin == Origin::Oms && it.link_id != 0).then(|| PlaceFacts {
        input_seq: e.cur_seq,
        link_id: it.link_id,
        link: e.oms.audit_link(it.link_id),
        venue: it.intent.venue,
        account: it.intent.account,
        group: it.intent.group,
        inst: it.intent.inst,
        free_slots: e
            .oms
            .table_view(it.intent.venue, it.intent.account)
            .map(|t| t.free_len),
    })
}

pub(crate) fn capture(e: &mut Engine, it: &OrderIntent, src: Src, class: IntentClass) -> DecisionFacts {
    let hedge = if let Src::Hedge { req } = src {
        Some((req.role, e.oms.audit_take_hedge(&req)))
    } else {
        None
    };
    let link = if let Src::Hedge { req } = src {
        req.link_id
    } else {
        0
    };
    let fl = e.oms.inflight().pos(it.account, it.inst);
    let p = e.risk.params();
    let limit = p
        .groups
        .iter()
        .find(|g| g.account == it.account && g.group == it.group)
        .map(|g| g.l)
        .or_else(|| {
            p.accounts
                .iter()
                .find(|a| a.account == it.account)
                .map(|a| a.default_group)
        });
    let mut leg_insts = [InstId(0); 4];
    if let Some(c) = e.exec.cell(it.account, it.group) {
        for (dest, l) in leg_insts.iter_mut().zip(&c.legs) {
            *dest = l.inst;
        }
    }
    DecisionFacts {
        class,
        origin: match src {
            Src::Strategy { .. } => 1,
            Src::Exec { .. } => 2,
            Src::Hedge { .. } => 3,
            Src::Smoke { .. } => 4,
        },
        exec_kind: if let Src::Exec { o } = src {
            Some(o.kind)
        } else {
            None
        },
        margin_stress: matches!(src, Src::Exec { o } if matches!(o.reason, TargetReason::MarginStress | TargetReason::Hard)),
        hedge,
        link,
        link_unknown: link != 0
            && e.rd.account_venues().iter().any(|(a, v, _)| {
                e.oms.table_view(*v, *a).is_some_and(|t| {
                    (1..=t.n_slots).any(|slot| {
                        e.oms.raw_slot(*v, *a, slot).is_some_and(|(s, _, _)| {
                            s.link_id == link && s.state == hft_oms::OrderState::Unknown
                        })
                    })
                })
            }),
        legs: e.oms.legs(it.account, it.group).view,
        leg_insts,
        pos: e.ledger.pos(it.account, it.inst).qty,
        open_buy: fl.open_buy,
        open_sell: fl.open_sell,
        imm_buy: fl.imm_buy,
        imm_sell: fl.imm_sell,
        max_pos_qty: limit.map_or(Qty(0), |l| l.max_pos_qty),
        exp: e.rd.asset(e.rd.inst(it.inst).asset).exp,
        quote_rate: crate::views::quote_rate(&e.rd, &e.ledger, it.inst),
        max_hedge_notional: p
            .accounts
            .iter()
            .find(|a| a.account == it.account)
            .map_or(0, |a| a.max_hedge_notional.0),
        max_reduce_notional: p
            .accounts
            .iter()
            .find(|a| a.account == it.account)
            .map_or(0, |a| a.max_reduce_notional.0),
        exec_cell: e.exec.cell(it.account, it.group).copied(),
        group_positions: e.ledger.group_pos(it.account, it.group).legs.map(|p| p.qty),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seq_373_diagnostic_capacity_is_explicit() {
        let f = DecisionFacts {
            class: IntentClass::Open,
            origin: 1,
            exec_kind: None,
            margin_stress: false,
            hedge: None,
            link: 0,
            link_unknown: false,
            legs: LegView::EMPTY,
            leg_insts: [InstId(0); 4],
            pos: Qty(0),
            open_buy: Qty(0),
            open_sell: Qty(0),
            imm_buy: Qty(0),
            imm_sell: Qty(0),
            max_pos_qty: Qty(0),
            exp: AssetExp {
                px_exp: -4,
                qty_exp: -8,
            },
            quote_rate: Rate(1_000_000_000),
            max_hedge_notional: 0,
            max_reduce_notional: 0,
            exec_cell: None,
            group_positions: [Qty(0); 4],
        };
        let mut a = Audit::new();
        a.begin();
        for ix in 0..CAPACITY {
            a.record(ix as u32, f);
        }
        assert!(!a.failed);
        a.record(CAPACITY as u32, f);
        assert!(a.failed);
        assert_eq!(a.rows.len(), CAPACITY);
        a.begin();
        a.record(0, f);
        a.finish(&[17]);
        assert!(!a.failed);
        assert_eq!(a.rows[0].seq, 17);
        a.begin();
        a.record(1, f);
        a.finish(&[17]);
        assert!(a.failed, "missing journal sequence must fail");
        a.begin();
        let place = PlaceFacts {
            input_seq: 17,
            link_id: 1,
            link: None,
            venue: VenueId(1),
            account: AccountId(1),
            group: hft_types::GROUP_NONE,
            inst: InstId(1),
            free_slots: Some(0),
        };
        for _ in 0..CAPACITY {
            a.place_failed(place, PlaceError::NoFreeSlot);
        }
        assert!(!a.failed);
        a.place_failed(place, PlaceError::NoFreeSlot);
        assert!(a.failed);
        assert_eq!(a.place_failures.len(), CAPACITY);
        assert_eq!(a.place_failures.capacity(), CAPACITY);
        a.begin();
        assert!(a.place_failures.is_empty());
        assert!(!a.failed);
        for _ in 0..CAPACITY {
            a.restored(RestoreRow::RestoreMarks { count: 1 });
        }
        assert_eq!(a.restore_rows().map(|rows| rows.len()), Ok(CAPACITY));
        a.begin();
        assert_eq!(a.restore_rows().map(|rows| rows.len()), Ok(CAPACITY));
        a.restored(RestoreRow::RestoreMarks { count: 1 });
        assert_eq!(a.restore_rows(), Err(RestoreObservationError::Overflow));
        assert_eq!(a.restore.len(), CAPACITY);
        assert_eq!(a.restore.capacity(), CAPACITY);
        a.begin();
        assert_eq!(a.restore_rows(), Err(RestoreObservationError::Overflow));
    }
}
