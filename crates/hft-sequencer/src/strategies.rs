//! The instance table (README "策略集"): instances in their dispatch order, parameter installation,
//! the strategy deliveries of one Input (one `Ctx` per instance and round) and the `Ctx`
//! application (README "process" 4, 5, 8).

use hft_event::{BlobRef, CmdScope, GroupSpec, Note, NoteSource, RecordFlags};
use hft_refdata::{AccountKind, RefData};
use hft_risk::mode::{CellKey, ModeInput};
use hft_strategy_sdk::ctx::{CancelReq, NoteReq, Op, SubEntry, TimerReq};
use hft_strategy_sdk::{
    CtxBufs, EventRef, GroupSpecExt, GroupTable, IntentHandle, IntentOutcome, OrderIntent, OwnEvMask, Part,
    RefView, StrategyInstance, StrategySet, TargetIntent, TimerId,
};
use hft_types::{AccountId, CapSource, GroupId, InstId, StrategyId};

use crate::engine::Engine;
use crate::notify::{Notif, Queued};
use crate::params::CapacitiesRow;
use crate::parsed::ParsedParams;
use crate::types::{BootError, CapKind};

/// Owner code of a strategy's timers.
pub const fn strategy_owner(s: StrategyId) -> u16 {
    hft_event::TIMER_OWNER_STRATEGY_BASE + s.0 as u16
}

/// One strategy instance (bound to exactly one account).
pub struct InstanceSlot<S: StrategySet> {
    pub id: StrategyId,
    pub account: AccountId,
    pub kind_id: u16,
    /// Bound account is Live (dispatch order: Live first).
    pub live: bool,
    /// Created on the first `StrategyParams`.
    pub inst: Option<S::Inst>,
    pub cur: Option<Box<ParsedParams<S>>>,
    /// The active parameters' blob reference.
    pub blob: BlobRef,
    /// `(group, spec_seq)` sorted by group.
    pub spec_seq: Vec<(GroupId, u64)>,
    pub bufs: Box<CtxBufs>,
    pub subs: Vec<SubEntry>,
    pub model_tick: Option<u32>,
    pub own: OwnEvMask,
    pub warmup: Part,
    /// Live strategy registrations in increasing TimerId order, including their exact deadline.
    pub timers: Vec<TimerReq>,
    pub model_mem: u64,
    pub model_ckpt: u64,
    /// Calls (tests).
    pub n_create: u32,
    pub n_on_params: u32,
}

impl<S: StrategySet> core::fmt::Debug for InstanceSlot<S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InstanceSlot")
            .field("id", &self.id)
            .field("account", &self.account)
            .field("kind_id", &self.kind_id)
            .field("created", &self.inst.is_some())
            .finish_non_exhaustive()
    }
}

impl<S: StrategySet> InstanceSlot<S> {
    pub fn groups(&self) -> Option<&GroupTable> {
        self.cur.as_ref().map(|p| &p.groups)
    }
    pub fn gix_of(&self, g: GroupId) -> Option<u16> {
        self.groups()?.gix_of(g)
    }
    pub fn spec_seq_of(&self, g: GroupId) -> Option<u64> {
        self.spec_seq
            .binary_search_by_key(&g, |x| x.0)
            .ok()
            .and_then(|i| self.spec_seq.get(i))
            .map(|x| x.1)
    }
    pub fn params_version(&self) -> u64 {
        self.cur.as_ref().map_or(0, |p| p.version)
    }
}

/// Owned copy of one `Ctx` operation.
#[derive(Copy, Clone, Debug)]
pub(crate) enum OpOwned {
    Place(OrderIntent),
    Target(TargetIntent),
    Clear(AccountId, GroupId),
    Cancel(CancelReq),
    Timer(TimerReq),
    CancelTimer(TimerId),
    Halt { group: GroupId, reason: u16 },
}

/// The instance table.
pub struct Instances<S: StrategySet> {
    pub slots: Vec<InstanceSlot<S>>,
    /// `StrategyId → slot index` (`u16::MAX` = none).
    pub by_id: Vec<u16>,
    /// Live first, then Sim; each ascending by `StrategyId`.
    pub dispatch_order: Vec<u16>,
    /// Position of each slot in `dispatch_order`.
    pub dispatch_pos: Vec<u16>,
    pub(crate) ops: Vec<(IntentHandle, OpOwned)>,
    pub(crate) outcomes: Vec<(IntentHandle, IntentOutcome)>,
    pub(crate) notes: Vec<NoteReq>,
    pub(crate) touched: Vec<u16>,
    /// Market-delivery subscription index (CSR etc.).
    pub subs: crate::subs::SubIndex,
    /// The Models of an advisor mirror (off in every other sequencer).
    pub models: crate::advisor::MirrorModels<S>,
}

impl<S: StrategySet> core::fmt::Debug for Instances<S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Instances")
            .field("slots", &self.slots)
            .field("dispatch_order", &self.dispatch_order)
            .finish_non_exhaustive()
    }
}

impl<S: StrategySet> Instances<S> {
    /// One slot per bound refdata instance; dispatch order fixed for the process lifetime.
    pub fn new(rd: &RefData, caps: &CapacitiesRow) -> Result<Instances<S>, BootError> {
        let mut slots = Vec::new();
        for m in rd.instances_in_order() {
            let Some(account) = rd.account_of(m.id) else {
                continue;
            };
            let live = matches!(rd.account(account).kind, AccountKind::Live);
            slots.push(InstanceSlot {
                id: m.id,
                account,
                kind_id: m.kind_id,
                live,
                inst: None,
                cur: None,
                blob: zero_blob(),
                spec_seq: Vec::with_capacity(usize::from(caps.max_groups_per_instance)),
                bufs: Box::new(CtxBufs::new()),
                subs: Vec::with_capacity(usize::from(caps.max_groups_per_instance) * 8),
                model_tick: None,
                own: OwnEvMask(0),
                warmup: Part::Model,
                timers: Vec::with_capacity(caps.max_timers as usize),
                model_mem: 0,
                model_ckpt: 0,
                n_create: 0,
                n_on_params: 0,
            });
        }
        if slots.len() > usize::from(caps.max_instances) {
            return Err(BootError::CapacityShrink {
                what: CapKind::Instances,
            });
        }
        let mut by_id = vec![u16::MAX; 256];
        for (i, s) in slots.iter().enumerate() {
            if let Some(b) = by_id.get_mut(usize::from(s.id.0)) {
                *b = i as u16;
            }
        }
        let mut order: Vec<u16> = (0..slots.len() as u16).collect();
        order.sort_by_key(|&i| {
            let s = slots.get(usize::from(i));
            (s.is_some_and(|s| !s.live), s.map_or(0, |s| s.id.0))
        });
        let mut pos = vec![0u16; slots.len()];
        for (p, i) in order.iter().enumerate() {
            if let Some(x) = pos.get_mut(usize::from(*i)) {
                *x = p as u16;
            }
        }
        Ok(Instances {
            slots,
            by_id,
            dispatch_order: order,
            dispatch_pos: pos,
            ops: Vec::with_capacity(hft_strategy_sdk::ctx::CAP_OPS),
            outcomes: Vec::with_capacity(hft_strategy_sdk::ctx::CAP_OPS),
            notes: Vec::with_capacity(hft_strategy_sdk::ctx::CAP_NOTE),
            touched: Vec::with_capacity(256),
            subs: crate::subs::SubIndex::new(rd, caps),
            models: crate::advisor::MirrorModels::default(),
        })
    }
    /// Slot index of a strategy.
    pub fn ix(&self, s: StrategyId) -> Option<usize> {
        let i = *self.by_id.get(usize::from(s.0))?;
        (i != u16::MAX).then_some(usize::from(i))
    }
    pub fn get(&self, s: StrategyId) -> Option<&InstanceSlot<S>> {
        self.slots.get(self.ix(s)?)
    }
    pub fn get_mut(&mut self, s: StrategyId) -> Option<&mut InstanceSlot<S>> {
        let i = self.ix(s)?;
        self.slots.get_mut(i)
    }
    /// Slot index of the instance bound to an account.
    pub fn ix_of_account(&self, a: AccountId) -> Option<usize> {
        self.slots.iter().position(|s| s.account == a)
    }
    /// Every (strategy, account, group) cell of the created instances.
    pub fn cells(&self) -> impl Iterator<Item = CellKey> + '_ {
        self.slots.iter().flat_map(|s| {
            s.groups().into_iter().flat_map(move |g| {
                g.iter().map(move |(_, sp)| CellKey {
                    strategy: s.id,
                    account: s.account,
                    group: sp.id,
                })
            })
        })
    }
    /// Count of cells.
    pub fn n_cells(&self) -> usize {
        self.slots
            .iter()
            .map(|s| s.groups().map_or(0, |g| usize::from(g.len())))
            .sum()
    }
    /// The GroupSpec of a group (any instance).
    pub fn spec(&self, a: AccountId, g: GroupId) -> Option<&GroupSpec> {
        let s = self.slots.get(self.ix(g.strategy())?)?;
        if s.account != a && a.0 != 0 {
            return None;
        }
        let t = s.groups()?;
        Some(t.get(t.gix_of(g)?))
    }
}

pub(crate) fn zero_blob() -> BlobRef {
    BlobRef {
        kind: hft_event::BlobKind::StrategyParams,
        encoding: 1,
        len: 0,
        version: 0,
        sha256: [0; 32],
    }
}

/// Installs the first parameters of an instance at an empty start (no command checks).
pub(crate) fn install_initial<D: crate::Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    p: Box<ParsedParams<S>>,
    blob: BlobRef,
    _scope: CmdScope,
) -> Result<(), BootError> {
    let Some(ix) = inst.ix(p.strategy) else {
        return Err(BootError::Config(crate::types::ConfigErr::Params(0)));
    };
    match apply_params(e, inst, drv, ix, p, blob, 0) {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(d)) => Err(BootError::Config(crate::types::ConfigErr::Params(d))),
        Err(_) => Err(BootError::Config(crate::types::ConfigErr::Component("timers"))),
    }
}

/// Applies new parameters to instance `ix` (README "命令路径" SetParams 应用): create or
/// `on_params`, component group tables, `GroupsChanged`, `spec_seq`, timer/target cleanup.
/// Returns the replaced parameters. `Err(detail)` = `InvalidArgs{ParamInvalid{Strategy}}`.
#[allow(clippy::type_complexity)]
pub(crate) fn apply_params<D: crate::Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    ix: usize,
    p: Box<ParsedParams<S>>,
    blob: BlobRef,
    cmd_seq: u64,
) -> Result<Result<Option<Box<ParsedParams<S>>>, u32>, crate::types::FatalReason> {
    let Some(slot) = inst.slots.get_mut(ix) else {
        return Ok(Err(0));
    };
    let account = slot.account;
    let strategy = slot.id;
    let rv = RefView::new(&e.rd, account);
    match slot.inst.as_mut() {
        None => {
            let si = StrategyInstance {
                strategy,
                account,
                kind_id: slot.kind_id,
            };
            let s = match S::create(si, &p.params, &p.groups, &rv) {
                Ok(s) => s,
                Err(x) => return Ok(Err(crate::parsed::strategy_param_detail(&x))),
            };
            slot.inst = Some(s);
            slot.n_create += 1;
        }
        Some(s) => {
            if let Err(x) = S::on_params(s, &p.params, &p.groups, &rv) {
                return Ok(Err(crate::parsed::strategy_param_detail(&x)));
            }
            slot.n_on_params += 1;
        }
    }
    e.trace.rec("strategy_params", u64::from(strategy.0), p.version);
    // Group differences against the old table.
    let old = slot.cur.take();
    let mut added: Vec<CellKey> = Vec::new();
    let mut removed: Vec<CellKey> = Vec::new();
    let mut changed_fp: Vec<GroupId> = Vec::new();
    for (_, g) in p.groups.iter() {
        let prev = old
            .as_ref()
            .and_then(|o| o.groups.gix_of(g.id).map(|gi| *o.groups.get(gi)));
        match prev {
            None => added.push(cell(strategy, account, g.id)),
            Some(pg) if pg.fingerprint() != g.fingerprint() => changed_fp.push(g.id),
            _ => {}
        }
    }
    if let Some(o) = old.as_ref() {
        for (_, g) in o.groups.iter() {
            if p.groups.gix_of(g.id).is_none() {
                removed.push(cell(strategy, account, g.id));
            }
        }
    }
    slot.model_mem = p.model_mem;
    slot.model_ckpt = p.model_ckpt;
    slot.blob = blob;
    // spec_seq: new groups and changed fingerprints take this command's seq.
    let mut ss: Vec<(GroupId, u64)> = Vec::with_capacity(usize::from(p.groups.len()));
    for (_, g) in p.groups.iter() {
        let keep = slot.spec_seq_of(g.id);
        let v = if added.iter().any(|c| c.group == g.id) || changed_fp.contains(&g.id) {
            cmd_seq
        } else {
            keep.unwrap_or(cmd_seq)
        };
        ss.push((g.id, v));
    }
    ss.sort_by_key(|x| x.0);
    slot.spec_seq.clear();
    slot.spec_seq.extend_from_slice(&ss);
    // Removed groups: timers and targets.
    let mut n_timers = 0u64;
    for c in removed.iter() {
        let owner = strategy_owner(strategy);
        n_timers += e.heap.remove_if(|t| t.owner == owner && t.group == c.group.0) as u64;
        slot.timers.retain(|t| t.group != c.group);
    }
    e.strat_timers = e.strat_timers.saturating_sub(n_timers as u32);
    let groups = p.groups.clone();
    slot.cur = Some(p);
    // Component group tables (risk before GroupsChanged: `lev_bad` reads its result).
    e.oms.on_groups_changed(&groups);
    e.ledger.on_groups_changed(strategy, &groups);
    let all = all_groups(inst, e.caps.max_cells_total);
    let mut xo = e.take_exec();
    e.exec
        .on_groups_changed(&all, &RefView::new(&e.rd, account), &mut xo);
    crate::exec_glue::drain_out(e, inst, drv, &mut xo)?;
    e.put_exec(xo);
    e.risk.on_groups_changed(account, &groups);
    e.trace.rec("risk.on_groups_changed", u64::from(account.0), 0);
    for c in removed.iter() {
        if e.exec.cell(account, c.group).is_some_and(|x| x.has_target) {
            let mut xo = e.take_exec();
            e.exec.clear_target(strategy, account, c.group, e.now, &mut xo);
            crate::exec_glue::drain_out(e, inst, drv, &mut xo)?;
            e.put_exec(xo);
            e.seq_note(
                crate::codes::note::TARGET_CLEARED,
                hft_event::NoteLevel::Info,
                c.group,
                InstId(0),
                [u64::from(strategy.0), u64::from(account.0), 0, 0, 0, 0, 0, 0],
            );
        }
        if n_timers > 0 {
            e.seq_note(
                crate::codes::note::TIMERS_CANCELLED,
                hft_event::NoteLevel::Info,
                c.group,
                InstId(0),
                [n_timers, 0, 0, 0, 0, 0, 0, 0],
            );
        }
    }
    let cells: Vec<(AccountId, GroupId)> = inst_cells(inst);
    e.gpos.set_cells(cells.into_iter());
    crate::advisor::on_params(e, inst, ix);
    if !added.is_empty() || !removed.is_empty() {
        e.trace
            .rec("apply.GroupsChanged", added.len() as u64, removed.len() as u64);
        crate::mode_glue::apply_mode(
            e,
            inst,
            drv,
            ModeInput::GroupsChanged {
                added: &added,
                removed: &removed,
            },
        )?;
    }
    Ok(Ok(old))
}

/// Every instance's GroupSpecs in one table (hft-exec keeps the cells of all instances).
pub(crate) fn all_groups<S: StrategySet>(inst: &Instances<S>, cap: u32) -> GroupTable {
    let mut v: Vec<GroupSpec> = Vec::new();
    for s in inst.slots.iter() {
        if let Some(g) = s.groups() {
            v.extend_from_slice(g.specs());
        }
    }
    GroupTable::new(&v, u16::try_from(cap).unwrap_or(u16::MAX).max(v.len() as u16))
}

fn inst_cells<S: StrategySet>(inst: &Instances<S>) -> Vec<(AccountId, GroupId)> {
    inst.cells().map(|c| (c.account, c.group)).collect()
}

pub(crate) fn cell(s: StrategyId, a: AccountId, g: GroupId) -> CellKey {
    CellKey {
        strategy: s,
        account: a,
        group: g,
    }
}

/// A market event to deliver through the subscription index.
#[derive(Copy, Clone, Debug)]
pub(crate) enum MarketEv {
    Bbo {
        inst: InstId,
        b: hft_strategy_sdk::BboView,
    },
    Book {
        inst: InstId,
    },
    Trade {
        inst: InstId,
        t: hft_strategy_sdk::TradeView,
    },
    Funding {
        inst: InstId,
    },
    Mark {
        inst: InstId,
        px: hft_types::Px,
    },
    Index {
        inst: InstId,
        px: hft_types::Px,
    },
    Liq {
        inst: InstId,
        l: hft_strategy_sdk::LiquidationView,
    },
}

impl MarketEv {
    pub fn inst(&self) -> InstId {
        match self {
            MarketEv::Bbo { inst, .. }
            | MarketEv::Book { inst }
            | MarketEv::Trade { inst, .. }
            | MarketEv::Funding { inst }
            | MarketEv::Mark { inst, .. }
            | MarketEv::Index { inst, .. }
            | MarketEv::Liq { inst, .. } => *inst,
        }
    }
    pub fn mask(&self) -> hft_strategy_sdk::EvMask {
        use hft_strategy_sdk::EvMask as M;
        match self {
            MarketEv::Bbo { .. } => M::BBO,
            MarketEv::Book { .. } => M::BOOK,
            MarketEv::Trade { .. } => M::TRADE,
            MarketEv::Funding { .. } => M::FUNDING,
            MarketEv::Mark { .. } => M::MARK,
            MarketEv::Index { .. } => M::INDEX,
            MarketEv::Liq { .. } => M::LIQUIDATION,
        }
    }
}

/// A direct delivery to one instance (payload borrowed from the Input).
#[derive(Copy, Clone, Debug)]
pub(crate) enum Direct<'a> {
    Advice {
        gix: u16,
        local_kind: u16,
        payload: &'a [u8],
        basis_seq: u64,
        basis_tsc: hft_types::Tsc,
    },
    Manual {
        gix: u16,
        payload: &'a [u8],
        reduce_only: bool,
    },
    Warmup {
        local_kind: u16,
        chunk_no: u32,
        last: bool,
        payload: &'a [u8],
    },
}

/// `market_ev` for the advisor's Model deliveries.
pub(crate) fn market_ev_pub<'a>(
    m: &'a MarketEv,
    gix: u16,
    market: &'a crate::market::Market,
) -> EventRef<'a> {
    market_ev(m, gix, market)
}

/// `notif_ev` for the advisor's Model deliveries (own events and Notice carry their gix).
pub(crate) fn notif_ev_pub(n: &Notif) -> EventRef<'_> {
    notif_ev(n, |_| hft_strategy_sdk::GIX_NONE)
}

/// Notifications → `EventRef`.
fn notif_ev(n: &Notif, gix_of: impl Fn(GroupId) -> u16) -> EventRef<'_> {
    match n {
        Notif::Order { gix, u } => EventRef::Order { gix: *gix, u },
        Notif::Exec { gix, u } => EventRef::Exec { gix: *gix, u },
        Notif::External { gix, f } => EventRef::External { gix: *gix, f },
        Notif::Risk { a, e } => EventRef::Risk { a: *a, e },
        Notif::Mode { gix, old, new } => EventRef::Mode {
            gix: *gix,
            old: *old,
            new: *new,
        },
        Notif::Conn { venue, state } => EventRef::Conn {
            venue: *venue,
            state: *state,
        },
        Notif::Notice { inst, gix, n } => EventRef::Notice {
            inst: *inst,
            gix: *gix,
            n,
        },
        Notif::Timer { tag, group } => EventRef::Timer {
            tag: *tag,
            group: *group,
            gix: gix_of(*group),
        },
    }
}

/// One delivery round of the current Input: the market event to its subscribers, the queued
/// notifications and an optional direct delivery, per instance in dispatch order, one `Ctx` per
/// instance; then every `Ctx` is applied in delivery order. `ro` marks the direct delivery's Ctx
/// as reduce-only (`StrategyCmd{reduce_only}`).
pub(crate) fn deliver_round<D: crate::Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    market: Option<MarketEv>,
    queued: &[Queued],
    direct: Option<(usize, Direct<'_>, bool)>,
) -> Result<(), crate::types::FatalReason> {
    e.gpos.refresh(&e.ledger);
    if inst.models.on {
        let manual = match direct {
            Some((
                dix,
                Direct::Manual {
                    gix,
                    payload,
                    reduce_only,
                },
                _,
            )) => Some((dix, gix, payload, reduce_only)),
            _ => None,
        };
        crate::advisor::deliver(e, inst, market, queued, manual);
    }
    inst.touched.clear();
    let order: Vec<u16> = inst.dispatch_order.clone();
    // Reserve in the same instance/callback order used below. One shared balance spans all
    // market, queued and direct deliveries; a later instance cannot reuse an earlier promise.
    let mut timer_remaining = e
        .caps
        .max_timers
        .saturating_sub(e.engine_timer_demand)
        .saturating_sub(e.strat_timers);
    let mut ro_ix: Option<usize> = None;
    for &ix16 in order.iter() {
        let ix = usize::from(ix16);
        let mut any = false;
        // Market entries of this instance (CSR row order: dispatch position, gix).
        if let Some(m) = market {
            let entries = inst.subs.fast_entries(m.inst(), ix16);
            if !entries.is_empty() {
                let Some(slot) = inst.slots.get_mut(ix) else {
                    continue;
                };
                let Some(st) = slot.inst.as_mut() else { continue };
                if !any {
                    slot.bufs.begin();
                    any = true;
                }
                for (gix, mask) in entries.iter() {
                    if !mask.contains(m.mask()) {
                        continue;
                    }
                    e.trace
                        .rec("deliver.market", u64::from(slot.id.0), u64::from(*gix));
                    let v = full_views(e);
                    let ev = market_ev(&m, *gix, &e.market);
                    S::on_event(
                        st,
                        &ev,
                        &v,
                        &mut slot.bufs.ctx_with_timer_quota(&mut timer_remaining, &slot.timers),
                    );
                }
            }
        }
        for q in queued.iter().filter(|q| usize::from(q.inst) == ix) {
            let Some(slot) = inst.slots.get_mut(ix) else {
                continue;
            };
            let groups = slot.cur.as_ref().map(|p| p.groups.clone());
            let Some(st) = slot.inst.as_mut() else { continue };
            if !any {
                slot.bufs.begin();
                any = true;
            }
            e.trace
                .rec("deliver.notify", u64::from(slot.id.0), notif_code(&q.n));
            let v = full_views(e);
            let ev = notif_ev(&q.n, |g| {
                groups
                    .as_ref()
                    .and_then(|t| t.gix_of(g))
                    .unwrap_or(hft_strategy_sdk::GIX_NONE)
            });
            S::on_event(
                st,
                &ev,
                &v,
                &mut slot.bufs.ctx_with_timer_quota(&mut timer_remaining, &slot.timers),
            );
        }
        if let Some((dix, d, ro)) = direct
            && dix == ix
        {
            let Some(slot) = inst.slots.get_mut(ix) else {
                continue;
            };
            let Some(st) = slot.inst.as_mut() else { continue };
            if !any {
                slot.bufs.begin();
                any = true;
            }
            e.trace.rec("deliver.direct", u64::from(slot.id.0), 0);
            let v = full_views(e);
            let ev = match d {
                Direct::Advice {
                    gix,
                    local_kind,
                    payload,
                    basis_seq,
                    basis_tsc,
                } => EventRef::Advice {
                    gix,
                    local_kind,
                    payload,
                    basis_seq,
                    basis_tsc,
                },
                Direct::Manual {
                    gix,
                    payload,
                    reduce_only,
                } => EventRef::Manual {
                    gix,
                    payload,
                    reduce_only,
                },
                Direct::Warmup {
                    local_kind,
                    chunk_no,
                    last,
                    payload,
                } => EventRef::Warmup {
                    local_kind,
                    chunk_no,
                    last,
                    payload,
                },
            };
            S::on_event(
                st,
                &ev,
                &v,
                &mut slot.bufs.ctx_with_timer_quota(&mut timer_remaining, &slot.timers),
            );
            if ro {
                ro_ix = Some(ix);
            }
        }
        if any {
            inst.touched.push(ix16);
        }
    }
    let touched: Vec<u16> = inst.touched.clone();
    for ix16 in touched {
        let ix = usize::from(ix16);
        apply_ctx(e, inst, drv, ix, ro_ix == Some(ix))?;
    }
    Ok(())
}

fn notif_code(n: &Notif) -> u64 {
    match n {
        Notif::Order { .. } => 1,
        Notif::Exec { .. } => 2,
        Notif::External { .. } => 3,
        Notif::Risk { .. } => 4,
        Notif::Mode { .. } => 5,
        Notif::Conn { .. } => 6,
        Notif::Notice { .. } => 7,
        Notif::Timer { .. } => 8,
    }
}

fn market_ev<'a>(m: &'a MarketEv, gix: u16, market: &'a crate::market::Market) -> EventRef<'a> {
    match m {
        MarketEv::Bbo { inst, b } => EventRef::Bbo {
            inst: *inst,
            gix,
            b: *b,
        },
        MarketEv::Book { inst } => EventRef::Book { inst: *inst, gix },
        MarketEv::Trade { inst, t } => EventRef::Trade { inst: *inst, gix, t },
        MarketEv::Funding { inst } => {
            static EMPTY: hft_strategy_sdk::FundingView = hft_strategy_sdk::FundingView::EMPTY;
            EventRef::Funding {
                inst: *inst,
                gix,
                f: market.funding.get(inst.0 as usize).unwrap_or(&EMPTY),
            }
        }
        MarketEv::Mark { inst, px } => EventRef::Mark {
            inst: *inst,
            gix,
            px: *px,
        },
        MarketEv::Index { inst, px } => EventRef::Index {
            inst: *inst,
            gix,
            px: *px,
        },
        MarketEv::Liq { inst, l } => EventRef::Liquidation { inst: *inst, gix, l },
    }
}

/// The full strategy-side views of the current Input.
pub(crate) fn full_views(e: &Engine) -> crate::views::SeqViews<'_> {
    crate::views::SeqViews {
        rd: &e.rd,
        books: &e.books,
        oms: &e.oms,
        risk: &e.risk,
        ledger: &e.ledger,
        exec: Some(&e.exec),
        market: &e.market,
        gpos: &e.gpos,
        now: e.now,
        model: false,
    }
}

/// Applies one instance's `Ctx` in call order (README "process" 5).
pub(crate) fn apply_ctx<D: crate::Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    ix: usize,
    ro: bool,
) -> Result<(), crate::types::FatalReason> {
    let Some(slot) = inst.slots.get_mut(ix) else {
        return Ok(());
    };
    let strategy = slot.id;
    let account = slot.account;
    e.ctx_full = e.ctx_full.saturating_add(u64::from(slot.bufs.full_count()));
    inst.ops.clear();
    for (h, op) in slot.bufs.ops() {
        let o = match op {
            Op::Place(i) => OpOwned::Place(*i),
            Op::Target(t) => OpOwned::Target(*t),
            Op::ClearTarget(a, g) => OpOwned::Clear(a, g),
            Op::Cancel(c) => OpOwned::Cancel(*c),
            Op::Timer(t) => OpOwned::Timer(*t),
            Op::CancelTimer(id) => OpOwned::CancelTimer(id),
            Op::Halt { group, reason } => OpOwned::Halt { group, reason },
        };
        inst.ops.push((h, o));
    }
    inst.notes.clear();
    inst.notes.extend_from_slice(slot.bufs.notes());
    inst.outcomes.clear();
    let ops: Vec<(IntentHandle, OpOwned)> = inst.ops.clone();
    for (h, op) in ops {
        match op {
            OpOwned::Place(it) => {
                e.trace
                    .rec("ctx.place", u64::from(strategy.0), u64::from(it.inst.0));
                // `OrderDecision.intent_no` = the SDK intent number (`IntentHandle`) of this call.
                let r = crate::intents::place_strategy(e, inst, drv, strategy, &it, ro, u32::from(h.0))?;
                inst.outcomes.push((h, r));
                crate::exec_glue::note_action(e, inst, drv, it.account, it.group)?;
            }
            OpOwned::Target(t) => {
                e.trace
                    .rec("exec.set_target", u64::from(strategy.0), u64::from(t.group.0));
                let r = crate::exec_glue::set_target(e, inst, drv, strategy, &t, ro)?;
                inst.outcomes.push((h, r));
            }
            OpOwned::Clear(a, g) => {
                e.trace.rec("exec.clear_target", u64::from(a.0), u64::from(g.0));
                crate::exec_glue::clear_target(e, inst, drv, strategy, a, g)?;
            }
            OpOwned::Cancel(c) => {
                crate::intents::cancel(e, inst, drv, &c)?;
            }
            OpOwned::Timer(t) => {
                let quota = (e.caps.max_timers.saturating_sub(e.engine_timer_demand)) as usize;
                if e.strat_timers as usize >= quota {
                    // A successful SDK reservation must be honored. Reaching this branch
                    // means the engine's admission state disagrees with its heap.
                    return Err(crate::types::FatalReason::TimerOverflow);
                }
                let owner = strategy_owner(strategy);
                e.heap
                    .push(crate::timers::TimerEntry {
                        deadline: t.at.0,
                        owner,
                        timer_id: t.tag,
                        group: t.group.0,
                    })
                    .map_err(|_| crate::types::FatalReason::TimerOverflow)?;
                e.strat_timers += 1;
                e.trace.rec("heap.push", u64::from(owner), u64::from(t.tag));
                if let Some(s) = inst.slots.get_mut(ix) {
                    s.timers.push(t);
                }
            }
            OpOwned::CancelTimer(id) => {
                if let Some(s) = inst.slots.get_mut(ix)
                    && let Ok(p) = s.timers.binary_search_by_key(&id, |t| t.id)
                {
                    let t = s.timers.remove(p);
                    let removed = e.heap.remove_exact(&crate::timers::TimerEntry {
                        deadline: t.at.0,
                        owner: strategy_owner(strategy),
                        timer_id: t.tag,
                        group: t.group.0,
                    });
                    if !removed {
                        return Err(crate::types::FatalReason::TimerOverflow);
                    }
                    e.strat_timers = e.strat_timers.saturating_sub(1);
                }
            }
            OpOwned::Halt { group, reason } => {
                e.trace
                    .rec("ctx.request_halt", u64::from(strategy.0), u64::from(group.0));
                let rec = hft_event::CapRecord {
                    source: CapSource(CapSource::HALTED.0),
                    level: 1,
                    cflags: 0,
                    _pad: [0; 5],
                    scope: hft_risk::mode::cell_scope(strategy, account, group),
                    set_seq: e.cur_seq,
                    // The strategy's reason code (SDK `request_halt`): into the CapRecord and so
                    // into its CapSet Note.
                    params: [u64::from(reason), 0],
                };
                crate::mode_glue::apply_mode(e, inst, drv, ModeInput::Auto(rec))?;
            }
        }
    }
    // Notes.
    let notes: Vec<NoteReq> = inst.notes.clone();
    for n in notes {
        // `Note.code` = source << 24 | the strategy's code (low 24 bits, hft-event Note codes).
        let note = Note {
            code: ((NoteSource::Strategy as u32) << 24) | (n.code & 0xFF_FFFF),
            level: n.level,
            source: NoteSource::Strategy,
            strategy,
            _pad: 0,
            group: n.group,
            inst: n.inst,
            v: n.v,
        };
        e.out.push(RecordFlags(0), &note);
    }
    if let Some(slot) = inst.slots.get_mut(ix) {
        slot.bufs.clear_outcomes();
        for (h, o) in inst.outcomes.iter() {
            slot.bufs.set_outcome(*h, *o);
        }
    }
    Ok(())
}

/// Queues a notification for the instance bound to `account` (cell gix looked up by group).
pub(crate) fn queue_for_account<S: StrategySet>(
    e: &mut Engine,
    inst: &Instances<S>,
    account: AccountId,
    n: Notif,
) {
    if let Some(ix) = inst.ix_of_account(account) {
        e.notify.push(ix as u16, n);
    }
}

/// gix of a group in the instance bound to `account` (`GIX_NONE` when absent).
pub(crate) fn gix_for<S: StrategySet>(inst: &Instances<S>, account: AccountId, g: GroupId) -> u16 {
    inst.ix_of_account(account)
        .and_then(|ix| inst.slots.get(ix))
        .and_then(|s| s.gix_of(g))
        .unwrap_or(hft_strategy_sdk::GIX_NONE)
}
