//! The inst → strategy dense index (README "inst → 策略的稠密索引"): `sub_ix` per subscribed inst
//! (never recycled), CSR rows `(instance_ix, gix, mask)` in dispatch order, the union of the
//! streams (strategies, Models and the engine's own position-driven subscriptions) and the
//! `FeedCmd::Subscribe` / `Unsubscribe` it produces.

use hft_event::FeedCmd;
use hft_refdata::{InstKind, RefData};
use hft_strategy_sdk::{EvMask, StrategySet, SubsBuilder, SubsLimits};
use hft_types::{AccountId, InstId};

use crate::engine::{Emit, Engine, account_count};
use crate::params::CapacitiesRow;
use crate::strategies::Instances;

/// The subscription index.
#[derive(Clone, Debug)]
pub struct SubIndex {
    /// `InstId → sub_ix` (`u16::MAX` = none).
    pub sub_ix: Vec<u16>,
    /// `sub_ix → InstId`.
    pub insts: Vec<InstId>,
    /// CSR offsets per `sub_ix` (`len = insts.len() + 1`) into `ent`.
    pub off: Vec<u32>,
    /// `(instance_ix, gix, mask)` (fast part), rows sorted by (dispatch position, gix).
    pub ent: Vec<(u16, u16, EvMask)>,
    /// `(instance_ix, gix, mask)` of the Model part, same layout (`moff`).
    pub moff: Vec<u32>,
    pub ment: Vec<(u16, u16, EvMask)>,
    /// Union of every stream per inst (strategies + Models + engine).
    pub union: Vec<u8>,
    max_insts: u32,
    max_subs: u16,
    /// First subscriptions refused because `max_insts` was used up.
    pub overflow: u32,
    scratch: Vec<(u16, u16, u16, EvMask, bool)>,
}

impl SubIndex {
    pub fn new(rd: &RefData, caps: &CapacitiesRow) -> SubIndex {
        let n = rd.inst_count() + 1;
        SubIndex {
            sub_ix: vec![u16::MAX; n],
            insts: Vec::with_capacity(caps.max_insts as usize),
            off: vec![0; 1],
            ent: Vec::with_capacity(caps.max_insts as usize * usize::from(caps.max_subs_per_inst)),
            moff: vec![0; 1],
            ment: Vec::with_capacity(caps.max_insts as usize * usize::from(caps.max_subs_per_inst)),
            union: vec![0; n],
            max_insts: caps.max_insts,
            max_subs: caps.max_subs_per_inst,
            overflow: 0,
            scratch: Vec::with_capacity(caps.max_insts as usize * usize::from(caps.max_subs_per_inst)),
        }
    }
    /// `sub_ix` of an inst.
    pub fn ix_of(&self, i: InstId) -> Option<u16> {
        let x = *self.sub_ix.get(i.0 as usize)?;
        (x != u16::MAX).then_some(x)
    }
    /// Allocates a `sub_ix` (first subscription); `None` when `max_insts` is used up.
    pub fn alloc(&mut self, i: InstId) -> Option<u16> {
        if let Some(x) = self.ix_of(i) {
            return Some(x);
        }
        if self.insts.len() as u32 >= self.max_insts {
            self.overflow += 1;
            return None;
        }
        let x = self.insts.len() as u16;
        self.insts.push(i);
        if let Some(s) = self.sub_ix.get_mut(i.0 as usize) {
            *s = x;
        }
        Some(x)
    }
    /// Fast entries `(gix, mask)` of one instance on an inst, in gix order.
    pub fn fast_entries(&self, i: InstId, inst_ix: u16) -> Vec<(u16, EvMask)> {
        let Some(x) = self.ix_of(i) else {
            return Vec::new();
        };
        let a = self.off.get(usize::from(x)).copied().unwrap_or(0) as usize;
        let b = self.off.get(usize::from(x) + 1).copied().unwrap_or(0) as usize;
        self.ent
            .get(a..b)
            .unwrap_or(&[])
            .iter()
            .filter(|e| e.0 == inst_ix)
            .map(|e| (e.1, e.2))
            .collect()
    }
    /// Fast CSR row of an inst.
    pub fn row(&self, i: InstId) -> &[(u16, u16, EvMask)] {
        let Some(x) = self.ix_of(i) else {
            return &[];
        };
        let a = self.off.get(usize::from(x)).copied().unwrap_or(0) as usize;
        let b = self.off.get(usize::from(x) + 1).copied().unwrap_or(0) as usize;
        self.ent.get(a..b).unwrap_or(&[])
    }
    /// Model entries `(gix, mask)` of one instance on an inst.
    pub fn model_entries(&self, i: InstId, inst_ix: u16) -> Vec<(u16, EvMask)> {
        let Some(x) = self.ix_of(i) else {
            return Vec::new();
        };
        let a = self.moff.get(usize::from(x)).copied().unwrap_or(0) as usize;
        let b = self.moff.get(usize::from(x) + 1).copied().unwrap_or(0) as usize;
        self.ment
            .get(a..b)
            .unwrap_or(&[])
            .iter()
            .filter(|e| e.0 == inst_ix)
            .map(|e| (e.1, e.2))
            .collect()
    }
    /// Entries of all instances on an inst (both parts) — the reference count of the inst.
    pub fn refs(&self, i: InstId) -> usize {
        self.row(i).len()
    }
}

/// Re-runs every instance's `subscriptions()` and rebuilds the index and the union (command path);
/// pushes `FeedCmd` for every inst whose union changed.
pub(crate) fn rebuild_all<S: StrategySet>(e: &mut Engine, inst: &mut Instances<S>) {
    // 1. collect subscriptions per instance.
    let n_inst = e.rd.inst_count() + 1;
    let mut counts = vec![0u16; n_inst];
    let order = inst.dispatch_order.clone();
    for &ix16 in order.iter() {
        let ix = usize::from(ix16);
        let Some(slot) = inst.slots.get_mut(ix) else { continue };
        slot.subs.clear();
        let (Some(s), Some(p)) = (slot.inst.as_ref(), slot.cur.as_ref()) else {
            continue;
        };
        let room: Vec<u16> = (0..n_inst)
            .map(|i| {
                let used = counts.get(i).copied().unwrap_or(0);
                let has_ix = inst.subs.sub_ix.get(i).is_some_and(|x| *x != u16::MAX);
                if !has_ix && inst.subs.insts.len() as u32 >= inst.subs.max_insts {
                    0
                } else {
                    inst.subs.max_subs.saturating_sub(used)
                }
            })
            .collect();
        let lim = SubsLimits {
            rd: &e.rd,
            feeds: e.setup.feeds,
            room: &room,
        };
        let mut b = SubsBuilder::new(lim, 256);
        S::subscriptions(s, &p.groups, &mut b);
        for se in b.entries() {
            if let Some(c) = counts.get_mut(se.inst.0 as usize) {
                *c += 1;
            }
        }
        slot.subs.extend_from_slice(b.entries());
        slot.model_tick = b.model_tick_ms();
        slot.own = b.own_mask();
        slot.warmup = b.warmup_part();
    }
    // 2. sub_ix for every subscribed inst (never recycled).
    for &ix16 in order.iter() {
        let ix = usize::from(ix16);
        let Some(slot) = inst.slots.get(ix) else { continue };
        let insts: Vec<InstId> = slot.subs.iter().map(|x| x.inst).collect();
        for i in insts {
            inst.subs.alloc(i);
        }
    }
    // 3. CSR rows.
    let si = &mut inst.subs;
    si.scratch.clear();
    for &ix16 in order.iter() {
        let pos = inst.dispatch_pos.get(usize::from(ix16)).copied().unwrap_or(0);
        let Some(slot) = inst.slots.get(usize::from(ix16)) else { continue };
        for se in slot.subs.iter() {
            if let Some(x) = si.ix_of(se.inst) {
                if se.part.fast() {
                    si.scratch.push((x, pos, se.gix, se.ev, true));
                }
                if se.part.model() {
                    si.scratch.push((x, pos, se.gix, se.ev, false));
                }
                let _ = ix16;
            }
        }
    }
    si.scratch.sort_by_key(|t| (t.0, t.1, t.2));
    let n = si.insts.len();
    si.off.clear();
    si.ent.clear();
    si.moff.clear();
    si.ment.clear();
    let mut k = 0usize;
    for x in 0..n as u16 {
        si.off.push(si.ent.len() as u32);
        si.moff.push(si.ment.len() as u32);
        while let Some(t) = si.scratch.get(k) {
            if t.0 != x {
                break;
            }
            let inst_ix = inst.dispatch_order.get(usize::from(t.1)).copied().unwrap_or(0);
            if t.4 {
                si.ent.push((inst_ix, t.2, t.3));
            } else {
                si.ment.push((inst_ix, t.2, t.3));
            }
            k += 1;
        }
    }
    si.off.push(si.ent.len() as u32);
    si.moff.push(si.ment.len() as u32);
    // 4. unions.
    for u in si.union.iter_mut() {
        *u = 0;
    }
    for x in 0..n {
        let Some(i) = si.insts.get(x).copied() else { continue };
        let a = si.off.get(x).copied().unwrap_or(0) as usize;
        let b = si.off.get(x + 1).copied().unwrap_or(0) as usize;
        let am = si.moff.get(x).copied().unwrap_or(0) as usize;
        let bm = si.moff.get(x + 1).copied().unwrap_or(0) as usize;
        let mut u = 0u8;
        for t in si.ent.get(a..b).unwrap_or(&[]) {
            u |= t.2.0;
        }
        for t in si.ment.get(am..bm).unwrap_or(&[]) {
            u |= t.2.0;
        }
        if let Some(d) = si.union.get_mut(i.0 as usize) {
            *d = u;
        }
    }
    engine_subscriptions(e, inst);
    send_union_changes(e, inst);
}

/// ORs the engine's own streams into the union: ① legs of every bound GroupSpec (BBO; perp legs
/// MARK|FUNDING), ② non-zero positions (perp: BBO|MARK|FUNDING, spot: BBO), ③ `ref_inst` (BBO).
pub(crate) fn engine_subscriptions<S: StrategySet>(e: &mut Engine, inst: &mut Instances<S>) {
    let perp = EvMask::BBO.0 | EvMask::MARK.0 | EvMask::FUNDING.0;
    let mut add: Vec<(InstId, u8)> = Vec::new();
    for s in inst.slots.iter() {
        let Some(g) = s.groups() else { continue };
        for (_, sp) in g.iter() {
            for l in sp.legs.iter().take(usize::from(sp.n_legs)) {
                let m = if e.rd.inst(l.inst).kind == InstKind::Perp { perp } else { EvMask::BBO.0 };
                add.push((l.inst, m));
            }
        }
    }
    for i in 1..=e.rd.inst_count() {
        let id = InstId(i as u32);
        if pos_nonzero(e, id) {
            let m = if e.rd.inst(id).kind == InstKind::Perp { perp } else { EvMask::BBO.0 };
            add.push((id, m));
        }
    }
    for (_, i) in e.setup.ledger.ref_inst.iter() {
        add.push((*i, EvMask::BBO.0));
    }
    for (i, m) in add {
        if inst.subs.alloc(i).is_none() {
            continue;
        }
        if let Some(u) = inst.subs.union.get_mut(i.0 as usize) {
            *u |= m;
        }
    }
}

/// Whether any account holds a non-zero position in the inst.
pub(crate) fn pos_nonzero(e: &Engine, i: InstId) -> bool {
    (1..=account_count(&e.rd)).any(|a| e.ledger.pos(AccountId(a as u8), i).qty.0 != 0)
}

/// Emits `Subscribe`/`Unsubscribe` for every inst whose union differs from the last sent value.
pub(crate) fn send_union_changes<S: StrategySet>(e: &mut Engine, inst: &mut Instances<S>) {
    for (ix, (u, sent)) in inst.subs.union.iter().zip(e.sent_union.iter_mut()).enumerate() {
        if *u == *sent {
            continue;
        }
        let i = InstId(ix as u32);
        let venue = e.rd.inst(i).venue;
        let cmd = if *u == 0 {
            FeedCmd::Unsubscribe { inst: i }
        } else {
            FeedCmd::Subscribe { inst: i, streams: *u }
        };
        e.out.emits.push(Emit::Feed { venue, cmd });
        *sent = *u;
    }
}

/// Recomputes the union of the insts whose position crossed zero in this Input.
pub(crate) fn position_changed<S: StrategySet>(e: &mut Engine, inst: &mut Instances<S>) {
    if e.pos_touched.is_empty() {
        return;
    }
    rebuild_unions_only(e, inst);
    e.pos_touched.clear();
}

/// Union recomputation without re-running `subscriptions()`.
pub(crate) fn rebuild_unions_only<S: StrategySet>(e: &mut Engine, inst: &mut Instances<S>) {
    let si = &mut inst.subs;
    for u in si.union.iter_mut() {
        *u = 0;
    }
    for x in 0..si.insts.len() {
        let Some(i) = si.insts.get(x).copied() else { continue };
        let a = si.off.get(x).copied().unwrap_or(0) as usize;
        let b = si.off.get(x + 1).copied().unwrap_or(0) as usize;
        let am = si.moff.get(x).copied().unwrap_or(0) as usize;
        let bm = si.moff.get(x + 1).copied().unwrap_or(0) as usize;
        let mut u = 0u8;
        for t in si.ent.get(a..b).unwrap_or(&[]) {
            u |= t.2.0;
        }
        for t in si.ment.get(am..bm).unwrap_or(&[]) {
            u |= t.2.0;
        }
        if let Some(d) = si.union.get_mut(i.0 as usize) {
            *d = u;
        }
    }
    engine_subscriptions(e, inst);
    send_union_changes(e, inst);
}

/// Every inst with a non-empty union is subscribed once at startup (sent unions are per run).
pub(crate) fn resend_all<S: StrategySet>(e: &mut Engine, inst: &mut Instances<S>) {
    for s in e.sent_union.iter_mut() {
        *s = 0;
    }
    send_union_changes(e, inst);
}
