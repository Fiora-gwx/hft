//! Read-only component state and per-Input evidence for the real sequencer.

use std::collections::BTreeMap;

use super::audit::History;
use crate::dst::world::WorldSpec;
use hft_event::{EventKind, ExternalFill, Note, NoteSource};
use hft_oms::{LinkState as OLink, OrderState};
use hft_sequencer::Engine;

use hft_strategy_sdk::{HedgeKind as SdkHedge, StrategySet};
use hft_types::{AccountId, AssetId, GroupId, InstId, VenueId};

use crate::dst::obs::{
    AssetRow, Evidence, EvidenceKind, ExternalKey, InstRow, LegNetRow, LegRow, LimitRow, LinkRow, LinkState,
    NoteInfo, Obs, PlanKind, PosRow, RiskRow, SlotRow, SlotState, SlotTableRow, TombRow,
};
use crate::dst::seq::SeqSut;
use crate::dst::sut::{InputRec, StepOut};

/// State carried across steps by the adapter.
#[derive(Clone, Debug, Default)]
pub struct Carry {
    /// `OmsStats` dup / stale-gen counts of the engine instances before the last restart (the
    /// counters are not in the snapshot; the observed values stay monotonic across restarts).
    pub dup_base: u64,
    pub stale_gen_base: u64,
}

/// What `process` remembers before handing the Input over.
#[derive(Clone, Debug, Default)]
pub struct Before {
    /// The Input may carry fills.
    pub fills: bool,
    pub kind: Option<EventKind>,
    pub ext_key: Option<ExternalKey>,
    pub pos: BTreeMap<(u8, u8, u32), i128>,
}

fn can_fill(k: EventKind) -> bool {
    [
        EventKind::ORDER_EVENT,
        EventKind::EXTERNAL_FILL,
        EventKind::RECONCILE_REPORT,
        EventKind::RECONCILE_BATCH,
        EventKind::CONTROL_CMD,
        EventKind::POSITION_REPORT,
        EventKind::BALANCE_REPORT,
        EventKind::TIMER,
    ]
    .contains(&k)
}

fn positions<S: StrategySet>(s: &SeqSut<S>) -> BTreeMap<(u8, u8, u32), i128> {
    positions_engine(s.s().engine(), &s.world)
}

fn positions_engine(e: &Engine, w: &WorldSpec) -> BTreeMap<(u8, u8, u32), i128> {
    let mut m = BTreeMap::new();
    for a in &w.accounts {
        for i in w.insts.iter().filter(|i| a.venues.contains(&i.venue)) {
            let q = i128::from(e.ledger.pos(a.id, i.id).qty.0);
            m.insert((a.id.0, i.venue.0, i.id.0), q);
        }
    }
    m
}

impl Before {
    pub fn take<S: StrategySet>(s: &SeqSut<S>, i: &InputRec) -> Before {
        let ext_key = (i.kind == EventKind::EXTERNAL_FILL)
            .then(|| {
                i.body
                    .get(..size_of::<ExternalFill>())
                    .and_then(|b| <ExternalFill as zerocopy::TryFromBytes>::try_read_from_bytes(b).ok())
            })
            .flatten()
            .and_then(|x| ExternalKey::from_fill(&x));
        Before {
            fills: can_fill(i.kind),
            kind: Some(i.kind),
            ext_key,
            pos: positions(s),
        }
    }
    pub fn input(e: &Engine, w: &WorldSpec, i: &hft_sequencer::types::InputSlot) -> Before {
        let ext_key = (i.kind == EventKind::EXTERNAL_FILL)
            .then(|| i.read::<ExternalFill>())
            .flatten()
            .and_then(|x| ExternalKey::from_fill(&x));
        Before {
            fills: can_fill(i.kind),
            kind: Some(i.kind),
            ext_key,
            pos: positions_engine(e, w),
        }
    }
    pub fn none<S: StrategySet>(s: &SeqSut<S>) -> Before {
        Before {
            fills: true,
            kind: Some(EventKind::TIMER),
            ext_key: None,
            pos: positions(s),
        }
    }
}

/// Name of an engine Note code (each crate's `NOTE_CODES` table).
pub fn note_name(code: u32) -> &'static str {
    let tables: [&[(NoteSource, u32, &'static str)]; 5] = [
        hft_sequencer::codes::NOTE_CODES,
        hft_oms::notes::NOTE_CODES,
        hft_ledger::NOTE_CODES,
        hft_risk::NOTE_CODES,
        hft_exec::NOTE_CODES,
    ];
    for t in tables {
        for (src, n, name) in t {
            if ((*src as u32) << 24 | *n) == code {
                return name;
            }
        }
    }
    "?"
}

fn account_of_group(w: &WorldSpec, g: GroupId) -> AccountId {
    w.groups
        .iter()
        .find(|x| x.group == g)
        .map_or(AccountId(0), |x| x.account)
}

/// Fills `out.info` from the step's records and the position changes.
pub fn step_info<S: StrategySet>(s: &mut SeqSut<S>, before: &Before, out: &mut StepOut) {
    step_info_engine(s.s().engine(), &s.world, before, out);
}

pub fn step_info_engine(e: &Engine, w: &WorldSpec, before: &Before, out: &mut StepOut) {
    for r in &out.records {
        if r.kind != EventKind::NOTE {
            continue;
        }
        let Some(n) = r.try_read::<Note>() else { continue };
        let name = note_name(n.code);
        let account = account_of_group(w, n.group);
        out.info.notes.push(NoteInfo {
            seq: r.seq,
            name,
            account,
            group: n.group,
            inst: n.inst,
            v: n.v,
        });
    }
    if !before.fills {
        return;
    }
    let after = positions_engine(e, w);
    let kind = match before.kind {
        Some(EventKind::ORDER_EVENT) => EvidenceKind::Fill,
        Some(EventKind::EXTERNAL_FILL) => EvidenceKind::External,
        _ => EvidenceKind::Adjust,
    };
    for ((a, v, i), q) in &after {
        let d = q - before.pos.get(&(*a, *v, *i)).copied().unwrap_or(0);
        if d != 0 {
            out.info.evidence.push(Evidence {
                kind,
                account: AccountId(*a),
                venue: VenueId(*v),
                inst: InstId(*i),
                delta: d,
                ext_key: if kind == EvidenceKind::External {
                    before.ext_key
                } else {
                    None
                },
            });
        }
    }
}

fn slot_state(s: OrderState) -> SlotState {
    match s {
        OrderState::Free => SlotState::Free,
        OrderState::PendingNew => SlotState::PendingNew,
        OrderState::Live => SlotState::Live,
        OrderState::PendingCancel => SlotState::PendingCancel,
        OrderState::PendingAmend => SlotState::PendingAmend,
        OrderState::Filled => SlotState::Filled,
        OrderState::Cancelled => SlotState::Cancelled,
        OrderState::Rejected => SlotState::Rejected,
        OrderState::Unknown => SlotState::Unknown,
    }
}

/// Leg insts of a group in parameter-blob order.
fn legs_of(w: &WorldSpec, g: GroupId) -> [InstId; 2] {
    let Some(x) = w.groups.iter().find(|x| x.group == g) else {
        return [InstId(0); 2];
    };
    let kind = w
        .strategies
        .iter()
        .find(|b| b.instance == x.instance)
        .map_or(0, super::build::kind_of);
    super::build::leg_order(w, kind, x.first, x.second)
}

/// `Sut::observe`.
pub fn fill<S: StrategySet>(s: &SeqSut<S>, o: &mut Obs) {
    fill_engine(
        s.s().engine(),
        &s.world,
        &s.s().driver().io.audit.history,
        &s.carry,
        o,
    );
}

pub fn fill_engine(e: &Engine, w: &WorldSpec, audit: &History, carry: &Carry, o: &mut Obs) {
    o.hedge_history.extend(audit.retired.values().cloned());
    o.observation_error.clone_from(&audit.error);
    // Ledger.
    for a in &w.accounts {
        for i in w.insts.iter().filter(|i| a.venues.contains(&i.venue)) {
            let q = i128::from(e.ledger.pos(a.id, i.id).qty.0);
            if q != 0 {
                o.positions.push(PosRow {
                    account: a.id,
                    venue: i.venue,
                    inst: i.id,
                    asset: i.base,
                    qty: q,
                });
            }
        }
        for v in &a.venues {
            let mut bases: Vec<AssetId> = Vec::new();
            let mut quotes: Vec<AssetId> = Vec::new();
            for i in w.insts.iter().filter(|i| i.venue == *v) {
                if !i.perp && !bases.contains(&i.base) {
                    bases.push(i.base);
                }
                if !quotes.contains(&i.quote) {
                    quotes.push(i.quote);
                }
            }
            for x in bases {
                let q = i128::from(e.ledger.inventory_view(a.id, *v, x).total.0);
                if q != 0 {
                    o.inventory.push(AssetRow {
                        account: a.id,
                        venue: *v,
                        asset: x,
                        qty: q,
                    });
                }
            }
            for x in quotes {
                let c = e.ledger.cash_view(a.id, *v, x);
                o.balances.push(AssetRow {
                    account: a.id,
                    venue: *v,
                    asset: x,
                    qty: i128::from(c.free.0) + i128::from(c.locked.0),
                });
            }
        }
    }
    // OMS slots and tables.
    for a in &w.accounts {
        for v in &a.venues {
            let Some(t) = e.oms.table_view(*v, a.id) else {
                continue;
            };
            o.tables.push(SlotTableRow {
                venue: *v,
                account: a.id,
                capacity: t.n_slots,
                free: t.free_len,
                unknown_count: t.unknown_count,
                free_list_consistent: true,
            });
            for ix in 1..=t.n_slots {
                let Some((sl, ext, _)) = e.oms.raw_slot(*v, a.id, ix) else {
                    continue;
                };
                if sl.r#gen != 0 {
                    // recycle keeps the slot's generation: its newest tomb is the current
                    // generation while Free. After reuse it is one of the preceding generations.
                    let mut g = sl.r#gen;
                    for _ in 0..=e.setup.oms.tombstone_gens {
                        if let Some(tb) = e.oms.tomb(*v, a.id, ix, g) {
                            o.tombs.push(TombRow {
                                venue: *v,
                                account: a.id,
                                slot: ix,
                                r#gen: tb.r#gen,
                                epoch: tb.epoch,
                                inst: tb.inst,
                                group: tb.group,
                                filled: i128::from(tb.filled.0),
                            });
                        }
                        g = if g == 1 { u32::MAX } else { g - 1 };
                    }
                }
                if sl.state == OrderState::Free {
                    continue;
                }
                let st = slot_state(sl.state);
                o.slots.push(SlotRow {
                    venue: *v,
                    account: a.id,
                    slot: ix,
                    r#gen: sl.r#gen,
                    epoch: sl.epoch,
                    state: st,
                    group: sl.group,
                    inst: sl.inst,
                    side: sl.side,
                    qty: i128::from(sl.qty.0),
                    filled: i128::from(sl.filled.0),
                    immediate: matches!(sl.tif, hft_types::Tif::Ioc | hft_types::Tif::Fok),
                    risk_reducing: sl.oflags & hft_event::OrderFlags::RISK_REDUCING.0 != 0,
                    terminal_ens: if st.is_terminal() {
                        e.now.ens_of(sl.t_state).max(1)
                    } else {
                        0
                    },
                });
                let _ = ext;
            }
        }
    }
    // Ledger external-fill dedup keys (INV-2), with the complete scope and key domain.
    for a in &w.accounts {
        for v in &a.venues {
            for k in e.ledger.ext_seen(a.id, *v) {
                let key = if k.kind == hft_ledger::KEY_TRADE_ID {
                    ExternalKey::Trade(k.a)
                } else {
                    ExternalKey::OrderSeq {
                        exch_order_id: k.a,
                        fill_seq: k.b,
                    }
                };
                o.ext_seen.push((a.id, *v, InstId(k.inst), key));
            }
        }
    }
    let st = e.oms.stats();
    o.oms_dup = carry.dup_base + st.dup;
    o.oms_stale_gen = carry.stale_gen_base + st.stale_gen;
    // Links.
    let n_links = e
        .setup
        .oms
        .links_per_account
        .saturating_mul(w.accounts.len() as u32 + 1);
    for id in 0..n_links {
        let Some(l) = e.oms.link(id) else { continue };
        let state = match l.state {
            OLink::Free => continue,
            OLink::Open => LinkState::Open,
            OLink::Hedging => LinkState::Hedging,
            OLink::Done => LinkState::Done,
            OLink::Broken => LinkState::Broken,
        };
        let Some((_, plan)) = e.oms.link_plan(l.link_id) else {
            continue;
        };
        let kind = match plan.kind {
            SdkHedge::OnFill => PlanKind::OnFill,
            SdkHedge::Simultaneous => PlanKind::Simultaneous,
            _ => PlanKind::Other,
        };
        let hedge_leg = legs_of(w, l.group)
            .iter()
            .position(|i| *i == plan.inst)
            .map_or(1, |k| k as u8);
        let Some(raw) = e.oms.audit_link(l.link_id) else {
            o.observation_error = Some("missing OMS link identity".into());
            continue;
        };
        o.links.push(LinkRow {
            id: l.link_id,
            key: Some(audit.key(raw)),
            account: l.account,
            group: l.group,
            plan: kind,
            state,
            hedge_leg,
            hedge_inst: plan.inst,
            ratio_num: i64::from(plan.ratio_num),
            ratio_den: i64::from(plan.ratio_den),
            first_net: i128::from(l.first_net.0),
            hedge_req: i128::from(l.hedge_req.0),
            hedge_filled: i128::from(l.hedge_filled.0),
            reversed: l.flags & 128 != 0,
            retries: u32::from(l.retries),
            has_unknown: e.oms.legs(l.account, l.group).unknown_slots > 0,
        });
    }
    // Legs.
    for g in &w.groups {
        let ls = e.oms.legs(g.account, g.group);
        let v = &ls.view;
        // The second leg of the parameter blob is the hedge leg of the takers' links.
        let hedge = 1usize;
        let first = legs_of(w, g.group)[0];
        let net: i128 = v
            .leg
            .iter()
            .take(usize::from(v.n_legs))
            .map(|l| i128::from(l.net.0))
            .sum();
        let mut imm = [0i128; 4];
        for (k, inst) in legs_of(w, g.group).iter().enumerate().take(usize::from(v.n_legs)) {
            o.hedge_balances.push(crate::dst::obs::HedgeBalanceRow {
                account: g.account,
                group: g.group,
                inst: *inst,
                residual: i128::from(v.leg[k].residual.0),
                fee_adjust: audit
                    .fees
                    .get(&(g.account.0, g.group.0, inst.0))
                    .copied()
                    .unwrap_or(0),
                naked_capped: audit.capped_inst(g.account, g.group, Some(*inst)),
            });
        }
        for (k, t) in v.leg.iter().enumerate() {
            imm[k] = i128::from(t.inflight_immediate.0);
            o.leg_net.push(LegNetRow {
                account: g.account,
                group: g.group,
                leg: k as u8,
                net: i128::from(t.net.0),
            });
        }
        o.legs.push(LegRow {
            account: g.account,
            group: g.group,
            unknown_slots: u32::from(ls.unknown_slots),
            naked_qty: i128::from(v.naked_qty.0).abs(),
            naked_buy: (-net).max(0),
            naked_sell: net.max(0),
            max_pos_qty: g.max_pos_qty,
            residual: v.leg.get(hedge).map_or(0, |t| i128::from(t.residual.0)),
            fee_adjust: audit
                .fees
                .get(&(g.account.0, g.group.0, legs_of(w, g.group)[hedge].0))
                .copied()
                .unwrap_or(0),
            naked_capped: audit.capped(g.account, g.group),
            inflight_immediate: imm,
        });
        o.limits.push(LimitRow {
            account: g.account,
            asset: w.inst(first).map_or(AssetId(0), |i| i.base),
            max_pos_qty: g.max_pos_qty,
        });
    }
    o.limits.sort_by_key(|l| (l.account.0, l.asset.0));
    o.limits.dedup_by_key(|l| (l.account.0, l.asset.0));
    for i in &w.insts {
        o.insts.push(InstRow {
            inst: i.id,
            venue: i.venue,
            asset: i.base,
            perp: i.perp,
            lot: i128::from(i.lot),
            min_qty: i128::from(i.min_qty),
        });
    }
    o.risk = RiskRow {
        max_hedge_notional: w.risk.max_hedge_notional,
        max_reduce_notional: w.risk.max_reduce_notional,
        force_step_bp: w.risk.force_step_bp,
        broken_auto_fix_ms: w.risk.broken_auto_fix_ms,
        reuse_quarantine_ms: i64::from(e.setup.oms.reuse_quarantine_ms),
        sweep_batch: u32::from(e.setup.oms.sweep_batch),
    };
    super::state::fill(e, w, audit, o);
}
