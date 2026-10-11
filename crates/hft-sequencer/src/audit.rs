//! Bounded, read-only test diagnostics. Not journaled, snapshotted or built into production.

use hft_event::{OrderAction, OrderDecision};
use hft_exec::ExecOrderKind;
use hft_oms::audit::HedgeFacts;
use hft_strategy_sdk::{IntentClass, LegView, LinkRole, OrderIntent, TargetReason};
use hft_types::{AssetExp, InstId, Qty, Rate};

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

#[derive(Debug)]
pub(crate) struct Audit {
    rows: Vec<DecisionRow>,
    pending: Vec<(OrderDecision, DecisionFacts)>,
    failed: bool,
}

impl Audit {
    pub fn new() -> Self {
        Self {
            rows: Vec::with_capacity(CAPACITY),
            pending: Vec::with_capacity(CAPACITY),
            failed: false,
        }
    }
    pub fn begin(&mut self) {
        self.rows.clear();
        self.pending.clear();
        self.failed = false;
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
    pub fn decision_observations(&self) -> &[DecisionRow] {
        &self.audit.rows
    }
    pub fn decision_observation_failed(&self) -> bool {
        self.audit.failed
    }
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
    }
}
