//! `hft-oms`: every order's engine-side state — per-(venue, account) slot tables, client order
//! id allocation semantics, the order state machine with Unknown convergence, LegLink second legs
//! (OnFill / Simultaneous, retries, deadlines, resumption after recovery), the LegTracker and the
//! in-flight book, smoke orders, residual write-off and the snapshot sections 2, 3 and 12.
//!
//! Pure state machine on the sequencer thread: no I/O, no clock (time is the event `tsc` passed
//! in), no allocation after `new`/`restore`. Specification: `crates/hft-oms/README.md`.
#![forbid(unsafe_code)]
#![deny(
    clippy::float_arithmetic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

#[cfg(target_endian = "big")]
compile_error!("hft-oms: snapshot rows are little-endian only");

mod config;
mod engine;
mod external;
mod fsm;
mod inflight;
mod latency;
mod legs;
mod link;
pub mod notes;
mod out;
mod price;
mod recover;
mod send;
mod smoke;
mod snapshot;
mod sweep;
mod table;
#[cfg(feature = "testing")]
pub mod testing;
mod types;
mod unknown;

use hft_refdata::{CapFlags, DeadManCap, InstKind, RefData};
use hft_strategy_sdk::{HedgePlan, OrderRef};
use hft_types::{AccountId, AssetExp, GroupId, Rate, StrategyId, Tsc, VenueId};

pub use config::{OmsConfig, PerTable, PerVenue};
pub use inflight::InflightBook;
pub use price::{link_valid, price_hedge};
pub use table::VenueBook;
pub use types::*;

use table::{Fifo, FifoMap, GroupEntry, InstInfo, Table};

/// Resolved global parameters (ticks where the config is in ms).
#[derive(Clone, Debug)]
pub(crate) struct Params {
    pub(crate) tpm: u64,
    pub(crate) query_interval: u64,
    pub(crate) unknown_max_queries: u8,
    pub(crate) unknown_alarm: u64,
    pub(crate) smoke_far_bp: Rate,
    pub(crate) reuse_quarantine: u64,
    pub(crate) tomb_gens: u8,
    pub(crate) tomb_keep: u64,
    pub(crate) sweep_batch: u16,
    pub(crate) k: u16,
    pub(crate) hedge_retry_max: u8,
    pub(crate) hedge_retry_min: u64,
    pub(crate) hedge_deadline: u64,
    pub(crate) hedge_deadline_ms: u32,
    pub(crate) resume_hold_max: u64,
    pub(crate) decode_fail_down_n: u8,
    pub(crate) lpa: u32,
}

/// Per-link bookkeeping recomputable from the slots (not in the snapshot).
#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct LinkAux {
    /// Non-terminal `Reverse` slots.
    pub(crate) rev_open: u16,
    /// `Unknown` slots of the link.
    pub(crate) unknown: u16,
}

/// The order management system (one instance, owned by the sequencer thread).
#[derive(Debug)]
pub struct Oms {
    pub(crate) p: Params,
    pub(crate) engine_epoch: u32,
    pub(crate) venue_ix: [u8; 256],
    pub(crate) n_accounts: usize,
    /// Per account index: bound strategy instance and whether it is a Sim account.
    pub(crate) acct_strategy: Box<[StrategyId]>,
    pub(crate) tables: Box<[Option<Box<Table>>]>,
    pub(crate) insts: Box<[InstInfo]>,
    pub(crate) links: Box<[LinkEntry]>,
    pub(crate) plans: Box<[(Rate, HedgePlan)]>,
    pub(crate) aux: Box<[LinkAux]>,
    pub(crate) groups: Vec<GroupEntry>,
    pub(crate) inflight: InflightBook,
    pub(crate) gq: Fifo<OutItem>,
    pub(crate) queued_decisions: usize,
    pub(crate) resume_hold_until: Tsc,
    pub(crate) stats: OmsStats,
    /// Slot of the last `FeeOnly` (target of the following `net_adjust` in the same event).
    pub(crate) fee_ctx: Option<(usize, u32)>,
    /// Set while a local send failure moves a slot to Rejected (failure classification).
    pub(crate) local_fail: Option<hft_event::SendStatus>,
}

static EMPTY_LEGS: LegState = LegState::EMPTY;

const OUT_ITEM_ZERO: OutItem = OutItem {
    kind: 0,
    action: 0,
    from: 0,
    cause: 0,
    link_id: 0,
    cause_seq: 0,
    slot: OrderRef::new(VenueId(0), AccountId(0), 0, 0),
    _pad: 0,
};

impl Oms {
    /// Preallocates every table from refdata's (venue, account) grid; never allocates afterwards.
    pub fn new(cfg: &OmsConfig, rd: &RefData) -> Result<Oms, OmsConfigError> {
        if !(1..=4).contains(&cfg.tombstone_gens) {
            return Err(OmsConfigError::TombGens);
        }
        let venues: Box<[VenueId]> = rd.venues_in_order().into();
        let mut n_accounts = 0usize;
        while n_accounts < 255 {
            let a = AccountId((n_accounts + 1) as u8);
            if rd.account(a).id != a {
                break;
            }
            n_accounts += 1;
        }
        if venues.len().saturating_mul(n_accounts) > 256 {
            return Err(OmsConfigError::TooManyTables);
        }
        let mut venue_ix = [u8::MAX; 256];
        for (i, v) in venues.iter().enumerate() {
            if let Some(s) = venue_ix.get_mut(usize::from(v.0)) {
                *s = i as u8;
            }
        }
        let tpm = cfg.tsc_per_ms();
        let ms = |x: u32| u64::from(x).saturating_mul(tpm);
        let p = Params {
            tpm,
            query_interval: ms(cfg.unknown_query_interval_ms),
            unknown_max_queries: cfg.unknown_max_queries,
            unknown_alarm: ms(cfg.unknown_alarm_ms),
            smoke_far_bp: cfg.smoke_far_bp,
            reuse_quarantine: ms(cfg.reuse_quarantine_ms),
            tomb_gens: cfg.tombstone_gens,
            tomb_keep: ms(cfg.tombstone_keep_ms),
            sweep_batch: cfg.sweep_batch.max(1),
            k: cfg.max_out_per_event.max(1),
            hedge_retry_max: cfg.hedge_retry_max,
            hedge_retry_min: ms(cfg.hedge_retry_min_ms),
            hedge_deadline: ms(cfg.hedge_deadline_ms),
            hedge_deadline_ms: cfg.hedge_deadline_ms,
            resume_hold_max: ms(cfg.resume_hold_max_ms),
            decode_fail_down_n: cfg.decode_fail_down_n.max(1),
            lpa: cfg.links_per_account.max(1),
        };
        let mut tables: Vec<Option<Box<Table>>> = Vec::with_capacity(venues.len() * n_accounts);
        let mut total_slots = 0usize;
        for (vix, &v) in venues.iter().enumerate() {
            let caps = rd.caps(v);
            let vm = rd.venue(v);
            for aix in 0..n_accounts {
                let a = AccountId((aix + 1) as u8);
                let am = rd.account(a);
                if am.venues & (1u64 << (v.0 & 63)) == 0 || v.0 >= 64 {
                    tables.push(None);
                    continue;
                }
                let n = cfg.slots.get(v, a);
                if !(1..=65_535).contains(&n) {
                    return Err(OmsConfigError::SlotsOutOfRange { venue: v, account: a });
                }
                total_slots += n as usize;
                let nn = n as usize + 1;
                let mut free = Fifo::new(n as usize, 0u32);
                for s in 1..=n {
                    free.push(s);
                }
                let mut slots = vec![OrderSlot::FREE; nn].into_boxed_slice();
                for (i, s) in slots.iter_mut().enumerate() {
                    s.slot_id = i as u32;
                    s.venue = v;
                    s.account = a;
                }
                tables.push(Some(Box::new(Table {
                    venue: v,
                    account: a,
                    tix: (vix * n_accounts + aix) as u16,
                    aix,
                    n,
                    slots,
                    ext: vec![SlotExt::ZERO; nn].into_boxed_slice(),
                    stamps: vec![SlotStamps::EMPTY; nn].into_boxed_slice(),
                    tombs: vec![Tomb::EMPTY; nn * usize::from(cfg.tombstone_gens)].into_boxed_slice(),
                    free,
                    foreign: FifoMap::new(usize::from(cfg.foreign_fill_track.get(v, a).max(1))),
                    ext_held: FifoMap::new(usize::from(cfg.ext_held_track.get(v, a).max(1))),
                    unknown_count: 0,
                    conn_up: false,
                    decode_fails: 0,
                    hedge_gate: false,
                    sweep_cursor: 0,
                    smoke_slot: 0,
                    q_rr: Fifo::new(2 * nn + 8, OUT_ITEM_ZERO),
                    q_open: Fifo::new(nn + 8, OUT_ITEM_ZERO),
                    ack_to: ms(cfg.ack_timeout_ms.get(v)),
                    cancel_to: ms(cfg.cancel_timeout_ms.get(v)),
                    ioc_to: ms(cfg.ioc_timeout_ms.get(v)),
                    grace: ms(cfg.not_found_grace_of(v, rd)),
                    stale_guard: vm.stale_guard,
                    amend: caps.flags.contains(CapFlags::AMEND),
                    amend_down_only: caps.amend_qty_down_only,
                    batch_cancel: caps.flags.contains(CapFlags::BATCH_CANCEL),
                    has_dms: !matches!(caps.dms, DeadManCap::None),
                    out_quota: cfg.out_quota_of(v, a).max(1),
                })));
            }
        }
        let n_insts = rd.inst_count();
        let mut insts = Vec::with_capacity(n_insts + 1);
        insts.push(InstInfo::NONE);
        for i in 1..=n_insts {
            let h = rd.inst(hft_types::InstId(i as u32));
            insts.push(InstInfo {
                valid: true,
                venue: h.venue,
                spot: matches!(h.kind, InstKind::Spot),
                asset: h.asset,
                exp: AssetExp {
                    px_exp: h.px_exp,
                    qty_exp: h.qty_exp,
                },
                lot: h.lot,
                min_qty: h.min_qty,
                min_notional: h.min_notional,
                no_amend: h.flags & hft_refdata::InstHot::NO_AMEND != 0,
            });
        }
        let mut acct_strategy = Vec::with_capacity(n_accounts);
        for aix in 0..n_accounts {
            let am = rd.account(AccountId((aix + 1) as u8));
            acct_strategy.push(am.bound_instance);
        }
        let n_links = n_accounts * p.lpa as usize;
        let n_tables = venues.len() * n_accounts;
        let max_groups = cfg.max_groups as usize;
        Ok(Oms {
            engine_epoch: cfg.engine_epoch,
            venue_ix,
            n_accounts,
            acct_strategy: acct_strategy.into_boxed_slice(),
            tables: tables.into_boxed_slice(),
            inflight: InflightBook::new(n_accounts, n_insts, n_tables, max_groups, venue_ix),
            insts: insts.into_boxed_slice(),
            links: vec![LinkEntry::FREE; n_links.max(1)].into_boxed_slice(),
            plans: vec![(Rate(0), HedgePlan::NONE); n_links.max(1)].into_boxed_slice(),
            aux: vec![LinkAux::default(); n_links.max(1)].into_boxed_slice(),
            groups: Vec::with_capacity(max_groups),
            gq: Fifo::new(4 * total_slots + 2 * n_links + 64, OUT_ITEM_ZERO),
            queued_decisions: 0,
            resume_hold_until: Tsc(0),
            stats: OmsStats::default(),
            fee_ctx: None,
            local_fail: None,
            p,
        })
    }

    // ------------------------------------------------------------------ read-only views

    /// The slot of a live handle (`None` for a stale generation or a Free slot).
    pub fn slot(&self, s: OrderRef) -> Option<&OrderSlot> {
        let (ti, i) = self.locate(s)?;
        self.tables.get(ti)?.as_ref()?.slots.get(i as usize)
    }
    /// `SlotExt` of a live handle.
    pub fn slot_ext(&self, s: OrderRef) -> Option<&SlotExt> {
        let (ti, i) = self.locate(s)?;
        self.tables.get(ti)?.as_ref()?.ext.get(i as usize)
    }
    /// `SlotStamps` of a live handle.
    pub fn slot_stamps(&self, s: OrderRef) -> Option<&SlotStamps> {
        let (ti, i) = self.locate(s)?;
        self.tables.get(ti)?.as_ref()?.stamps.get(i as usize)
    }
    /// Raw slot `s` of (v, a) whatever its state or generation.
    pub fn raw_slot(&self, v: VenueId, a: AccountId, s: u32) -> Option<(&OrderSlot, &SlotExt, &SlotStamps)> {
        let t = self.table(v, a)?;
        if s == 0 || s > t.n {
            return None;
        }
        Some((
            t.slots.get(s as usize)?,
            t.ext.get(s as usize)?,
            t.stamps.get(s as usize)?,
        ))
    }
    /// Tombstone of generation `gen` of slot `s` of (v, a), if kept.
    pub fn tomb(&self, v: VenueId, a: AccountId, s: u32, r#gen: u32) -> Option<&Tomb> {
        let t = self.table(v, a)?;
        let tb = t.tombs.get(t.tomb_ix(s, r#gen, self.p.tomb_gens))?;
        (tb.r#gen == r#gen && tb.r#gen != 0).then_some(tb)
    }
    pub fn inflight(&self) -> &InflightBook {
        &self.inflight
    }
    /// LegTracker state of (a, g) (`GROUP_NONE` and unknown groups → all zero).
    pub fn legs(&self, a: AccountId, g: GroupId) -> &LegState {
        self.group_ix(a, g)
            .and_then(|i| self.groups.get(i))
            .map_or(&EMPTY_LEGS, |e| &e.st)
    }
    /// (venue, account) of the other leg of a link.
    pub fn link_peer(&self, link_id: u32) -> Option<(VenueId, AccountId)> {
        let i = self.link_ix(link_id)?;
        let (_, plan) = self.plans.get(i)?;
        Some((plan.venue, plan.account))
    }
    /// The link entry (any non-free state).
    pub fn link(&self, link_id: u32) -> Option<&LinkEntry> {
        let i = self.link_ix(link_id)?;
        self.links.get(i)
    }
    /// The hedge plan and tolerance of a link.
    pub fn link_plan(&self, link_id: u32) -> Option<&(Rate, HedgePlan)> {
        let i = self.link_ix(link_id)?;
        self.plans.get(i)
    }
    /// No Unknown slot and the order connection is Up.
    pub fn venue_ready(&self, v: VenueId, a: AccountId) -> bool {
        self.table(v, a).is_some_and(Table::ready)
    }
    pub fn stats(&self) -> &OmsStats {
        &self.stats
    }
    pub fn engine_epoch(&self) -> u32 {
        self.engine_epoch
    }
    /// `resume_hold_until` (0 = no resumption window).
    pub fn resume_hold_until(&self) -> Tsc {
        self.resume_hold_until
    }
    /// Counters of one table (`None` when (v, a) has no table).
    pub fn table_view(&self, v: VenueId, a: AccountId) -> Option<TableView> {
        let t = self.table(v, a)?;
        Some(TableView {
            table_ix: t.tix,
            n_slots: t.n,
            free_len: t.free.len() as u32,
            free_tail: t.free.get(t.free.len().wrapping_sub(1)).unwrap_or(0),
            unknown_count: t.unknown_count,
            conn_up: t.conn_up,
            decode_fails: t.decode_fails,
            hedge_gate: t.hedge_gate,
            sweep_cursor: t.sweep_cursor,
            smoke_slot: t.smoke_slot,
            n_foreign: t.foreign.len() as u32,
            n_ext_held: t.ext_held.len() as u32,
        })
    }
    /// `foreign[eid]` of a table.
    pub fn foreign_of(&self, v: VenueId, a: AccountId, eid: u64) -> Option<hft_types::Qty> {
        self.table(v, a)?.foreign.get(eid)
    }
    /// `ext_held[eid]` of a table.
    pub fn ext_held_of(&self, v: VenueId, a: AccountId, eid: u64) -> Option<hft_types::Qty> {
        self.table(v, a)?.ext_held.get(eid)
    }
    /// Number of items waiting in `pending_out` (global queue plus decision queues).
    pub fn pending_len(&self) -> usize {
        self.gq.len() + self.queued_decisions
    }

    // ------------------------------------------------------------------ index helpers

    #[inline]
    pub(crate) fn tix(&self, v: VenueId, a: AccountId) -> Option<usize> {
        let vix = *self.venue_ix.get(usize::from(v.0))?;
        if vix == u8::MAX {
            return None;
        }
        let aix = usize::from(a.0).checked_sub(1)?;
        if aix >= self.n_accounts {
            return None;
        }
        let ti = usize::from(vix) * self.n_accounts + aix;
        self.tables.get(ti)?.as_ref().map(|_| ti)
    }
    #[inline]
    pub(crate) fn table(&self, v: VenueId, a: AccountId) -> Option<&Table> {
        let ti = self.tix(v, a)?;
        self.tables.get(ti)?.as_deref()
    }
    #[inline]
    pub(crate) fn t(&self, ti: usize) -> Option<&Table> {
        self.tables.get(ti)?.as_deref()
    }
    #[inline]
    pub(crate) fn tm(&mut self, ti: usize) -> Option<&mut Table> {
        self.tables.get_mut(ti)?.as_deref_mut()
    }
    /// (table index, slot) of a live handle.
    pub(crate) fn locate(&self, s: OrderRef) -> Option<(usize, u32)> {
        let ti = self.tix(s.venue, s.account)?;
        let t = self.t(ti)?;
        let sl = t.slot(s.slot)?;
        (sl.r#gen == s.r#gen && sl.state != OrderState::Free).then_some((ti, s.slot))
    }
    #[inline]
    pub(crate) fn aix(&self, a: AccountId) -> Option<usize> {
        let i = usize::from(a.0).checked_sub(1)?;
        (i < self.n_accounts).then_some(i)
    }
    #[inline]
    pub(crate) fn inst(&self, i: hft_types::InstId) -> &InstInfo {
        self.insts.get(i.0 as usize).unwrap_or(&InstInfo::NONE)
    }
    #[inline]
    pub(crate) fn link_ix(&self, link_id: u32) -> Option<usize> {
        let i = (link_id as usize).checked_sub(1)?;
        let e = self.links.get(i)?;
        (e.link_id == link_id && e.state != LinkState::Free).then_some(i)
    }
    #[inline]
    pub(crate) fn group_ix(&self, a: AccountId, g: GroupId) -> Option<usize> {
        if g == hft_types::GROUP_NONE {
            return None;
        }
        self.groups
            .binary_search_by_key(&(a, g), |e| (e.account, e.group))
            .ok()
    }
    #[inline]
    pub(crate) fn ms(&self, x: u32) -> u64 {
        u64::from(x).saturating_mul(self.p.tpm)
    }
}

/// Counters of one (venue, account) table.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct TableView {
    pub table_ix: u16,
    pub n_slots: u32,
    pub free_len: u32,
    /// Last slot of the free FIFO (0 when empty).
    pub free_tail: u32,
    pub unknown_count: u32,
    pub conn_up: bool,
    pub decode_fails: u8,
    pub hedge_gate: bool,
    pub sweep_cursor: u32,
    pub smoke_slot: u32,
    pub n_foreign: u32,
    pub n_ext_held: u32,
}
