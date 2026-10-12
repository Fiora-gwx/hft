//! Intent collector `Ctx` with its per-instance buffers `CtxBufs` and capacities, and the
//! subscription builder `SubsBuilder` (README "Ctx 与订单意图", "Ctx 的内存与排空",
//! "Strategy 与 Model").
//!
//! `Ctx` borrows `CtxBufs`, which the sequencer preallocates per instance: one fixed array per
//! intent kind plus the operation sequence in call order. After `on_event` returns the sequencer
//! drains the operations in call order ([`CtxBufs::ops`]) and writes each intent's result with
//! [`CtxBufs::set_outcome`]; the strategy reads them through [`Ctx::outcome`] during the next
//! call only.

use hft_event::NoteLevel;
use hft_refdata::RefData;
use hft_types::{AccountId, GroupId, InstId, Px, Qty, Tsc, VenueId};

use crate::intent::{IntentHandle, IntentOutcome, OrderIntent, OrderRef};
use crate::target::TargetIntent;
use crate::view_types::VenueSet;

/// `place` per input event.
pub const CAP_PLACE: usize = 16;
/// `target` + `clear_target` per input event.
pub const CAP_TARGET: usize = 64;
/// `cancel` + `amend` per input event.
pub const CAP_CANCEL: usize = 16;
/// `timer_at` per input event.
pub const CAP_TIMER: usize = 8;
/// `cancel_timer` per input event.
pub const CAP_CANCEL_TIMER: usize = 8;
/// `note` per input event (excess dropped and counted).
pub const CAP_NOTE: usize = 16;
/// `request_halt` per input event.
pub const CAP_HALT: usize = 4;
/// Operation sequence (every kind except notes, in call order).
pub const CAP_OPS: usize = 128;
const _: () =
    assert!(CAP_PLACE + CAP_TARGET + CAP_CANCEL + CAP_TIMER + CAP_CANCEL_TIMER + CAP_HALT <= CAP_OPS);

/// The kind's per-event capacity is used up; the strategy treats it as "drop this intent for this
/// event" and does not retry within the same event.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct CtxFull;

/// Id of a strategy timer; stable across snapshots (the counter is part of the sequencer's
/// instance state, see [`CtxBufs::next_timer_id`]).
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct TimerId(pub u64);

/// One requested timer.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct TimerReq {
    pub id: TimerId,
    /// This run's TSC.
    pub at: Tsc,
    pub tag: u32,
    pub group: GroupId,
}

/// One strategy Note (→ `Note{source = Strategy}`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct NoteReq {
    pub code: u32,
    pub level: NoteLevel,
    pub group: GroupId,
    pub inst: InstId,
    pub v: [u64; 8],
}

/// One cancel or amend.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct CancelReq {
    pub r: OrderRef,
    /// `Some((px, qty))` = amend.
    pub amend: Option<(Px, Qty)>,
}

/// One drained operation, in call order.
#[derive(Copy, Clone, Debug)]
pub enum Op<'a> {
    Place(&'a OrderIntent),
    Target(&'a TargetIntent),
    ClearTarget(AccountId, GroupId),
    Cancel(&'a CancelReq),
    Timer(&'a TimerReq),
    CancelTimer(TimerId),
    Halt { group: GroupId, reason: u16 },
}

#[derive(Copy, Clone, Debug)]
enum OpKind {
    Place,
    Target,
    Clear,
    Cancel,
    Timer,
    CancelTimer,
    Halt,
}

#[derive(Copy, Clone, Debug)]
struct OpRef {
    kind: OpKind,
    ix: u8,
}

/// Per-instance intent buffers (preallocated once; about 24 KiB, mostly the 64 targets).
#[derive(Clone, Debug)]
pub struct CtxBufs {
    places: [OrderIntent; CAP_PLACE],
    n_places: u8,
    targets: [TargetIntent; CAP_TARGET],
    clears: [(AccountId, GroupId); CAP_TARGET],
    n_targets: u8,
    cancels: [CancelReq; CAP_CANCEL],
    n_cancels: u8,
    timers: [TimerReq; CAP_TIMER],
    n_timers: u8,
    cancel_timers: [TimerId; CAP_CANCEL_TIMER],
    cancel_releases: [bool; CAP_CANCEL_TIMER],
    n_cancel_timers: u8,
    notes: [NoteReq; CAP_NOTE],
    n_notes: u8,
    halts: [(GroupId, u16); CAP_HALT],
    n_halts: u8,
    ops: [OpRef; CAP_OPS],
    n_ops: u8,
    full: u32,
    outcomes: [Option<IntentOutcome>; CAP_OPS],
    next_timer: u64,
}

impl CtxBufs {
    /// Empty buffers (`Box` it per instance: about 24 KiB).
    pub fn new() -> CtxBufs {
        empty_bufs()
    }
    /// Clears the intent arrays and the operation sequence before an `on_event` call; keeps the
    /// outcomes of the previous call.
    pub fn begin(&mut self) {
        self.n_places = 0;
        self.n_targets = 0;
        self.n_cancels = 0;
        self.n_timers = 0;
        self.n_cancel_timers = 0;
        self.cancel_releases = [false; CAP_CANCEL_TIMER];
        self.n_notes = 0;
        self.n_halts = 0;
        self.n_ops = 0;
        self.full = 0;
    }
    /// A `Ctx` over these buffers for one `on_event` call.
    pub fn ctx(&mut self) -> Ctx<'_> {
        Ctx {
            b: self,
            timer_quota: None,
        }
    }
    /// Engine admission for a delivery round. `remaining` is shared by all instances in their
    /// dispatch order; `registered` contains this instance's live timers sorted by `TimerId`.
    /// Keep both consistent with the operations already applied before this round. Requests
    /// and cancellations in these buffers reserve/release capacity immediately, without
    /// changing the engine's later operation order or allocating memory.
    pub fn ctx_with_timer_quota<'a>(
        &'a mut self,
        remaining: &'a mut u32,
        registered: &'a [TimerReq],
    ) -> Ctx<'a> {
        Ctx {
            b: self,
            timer_quota: Some(TimerQuota {
                remaining,
                registered,
            }),
        }
    }
    /// Operations of the last call in call order, with their handles.
    pub fn ops(&self) -> impl Iterator<Item = (IntentHandle, Op<'_>)> + '_ {
        self.ops
            .iter()
            .take(usize::from(self.n_ops))
            .enumerate()
            .filter_map(move |(i, o)| {
                let h = IntentHandle(u16::try_from(i).ok()?);
                let ix = usize::from(o.ix);
                let op = match o.kind {
                    OpKind::Place => Op::Place(self.places.get(ix)?),
                    OpKind::Target => Op::Target(self.targets.get(ix)?),
                    OpKind::Clear => {
                        let (a, g) = *self.clears.get(ix)?;
                        Op::ClearTarget(a, g)
                    }
                    OpKind::Cancel => Op::Cancel(self.cancels.get(ix)?),
                    OpKind::Timer => Op::Timer(self.timers.get(ix)?),
                    OpKind::CancelTimer => Op::CancelTimer(*self.cancel_timers.get(ix)?),
                    OpKind::Halt => {
                        let (group, reason) = *self.halts.get(ix)?;
                        Op::Halt { group, reason }
                    }
                };
                Some((h, op))
            })
    }
    /// Notes of the last call in call order.
    pub fn notes(&self) -> &[NoteReq] {
        self.notes.get(..usize::from(self.n_notes)).unwrap_or(&[])
    }
    /// `CtxFull` count of the last call (every kind, notes included).
    pub fn full_count(&self) -> u32 {
        self.full
    }
    /// Forgets the previous outcomes; called once before the results of a call are written.
    pub fn clear_outcomes(&mut self) {
        self.outcomes = [None; CAP_OPS];
    }
    /// Records the engine's result for one intent of the last call (out of range → ignored).
    pub fn set_outcome(&mut self, h: IntentHandle, o: IntentOutcome) {
        if let Some(s) = self.outcomes.get_mut(usize::from(h.0)) {
            *s = Some(o);
        }
    }
    /// Next `TimerId` to hand out (part of the instance's snapshot state).
    pub fn next_timer_id(&self) -> u64 {
        self.next_timer
    }
    /// Restores the timer id counter.
    pub fn set_next_timer_id(&mut self, n: u64) {
        self.next_timer = n;
    }
}

impl Default for CtxBufs {
    fn default() -> Self {
        CtxBufs::new()
    }
}

fn empty_bufs() -> CtxBufs {
    use crate::intent::LegLinkOpt;
    use crate::target::{ExecSpec, LegTarget, TakerSliceParams, TargetReason, Urgency};
    use hft_types::{Notional, Rate, Side, Tif};
    let oi = OrderIntent {
        venue: VenueId(0),
        account: AccountId(0),
        side: Side::Buy,
        tif: Tif::Ioc,
        class: crate::intent::IntentClass::Open,
        flags: 0,
        _pad: [0; 2],
        inst: InstId(0),
        group: GroupId(0),
        px: Px(0),
        qty: Qty(0),
        link: LegLinkOpt::NONE,
        tag: 0,
        max_rest_ms: 0,
    };
    let ti = TargetIntent {
        account: AccountId(0),
        _pad0: [0; 3],
        group: GroupId(0),
        legs: [LegTarget::ZERO; 4],
        n_legs: 0,
        reason: TargetReason::Rebalance,
        urgency: Urgency::Normal,
        _pad1: [0; 5],
        band: Rate(0),
        exec: ExecSpec::TakerSlice(TakerSliceParams {
            depth_frac: Rate(0),
            depth_levels: 0,
            _pad0: [0; 7],
            max_cross: Rate(0),
            max_chunk_notional: Notional(0),
            min_chunk_notional: Notional(0),
            cooldown_ms: 0,
            _pad1: 0,
        }),
        tag: 0,
        _pad2: 0,
    };
    let cr = CancelReq {
        r: OrderRef::new(VenueId(0), AccountId(0), 0, 0),
        amend: None,
    };
    let tr = TimerReq {
        id: TimerId(0),
        at: Tsc(0),
        tag: 0,
        group: GroupId(0),
    };
    let nr = NoteReq {
        code: 0,
        level: NoteLevel::Info,
        group: GroupId(0),
        inst: InstId(0),
        v: [0; 8],
    };
    CtxBufs {
        places: [oi; CAP_PLACE],
        n_places: 0,
        targets: [ti; CAP_TARGET],
        clears: [(AccountId(0), GroupId(0)); CAP_TARGET],
        n_targets: 0,
        cancels: [cr; CAP_CANCEL],
        n_cancels: 0,
        timers: [tr; CAP_TIMER],
        n_timers: 0,
        cancel_timers: [TimerId(0); CAP_CANCEL_TIMER],
        cancel_releases: [false; CAP_CANCEL_TIMER],
        n_cancel_timers: 0,
        notes: [nr; CAP_NOTE],
        n_notes: 0,
        halts: [(GroupId(0), 0); CAP_HALT],
        n_halts: 0,
        ops: [OpRef {
            kind: OpKind::Place,
            ix: 0,
        }; CAP_OPS],
        n_ops: 0,
        full: 0,
        outcomes: [None; CAP_OPS],
        next_timer: 1,
    }
}

/// Fixed-capacity intent collector of one `on_event` call. Allocation-free; a full kind
/// returns `CtxFull` and counts it. Engine timer cancellation checks a sorted live-id view
/// in O(log registered timers); other methods and the standalone collector are O(1).
#[derive(Debug)]
pub struct Ctx<'a> {
    b: &'a mut CtxBufs,
    timer_quota: Option<TimerQuota<'a>>,
}

#[derive(Debug)]
struct TimerQuota<'a> {
    remaining: &'a mut u32,
    registered: &'a [TimerReq],
}

impl<'a> Ctx<'a> {
    fn push_op(&mut self, kind: OpKind, ix: u8) -> IntentHandle {
        let h = IntentHandle(u16::from(self.b.n_ops));
        if let Some(s) = self.b.ops.get_mut(usize::from(self.b.n_ops)) {
            *s = OpRef { kind, ix };
            self.b.n_ops = self.b.n_ops.saturating_add(1);
        }
        h
    }
    fn full(&mut self) -> CtxFull {
        self.b.full = self.b.full.saturating_add(1);
        CtxFull
    }

    /// Places an order intent.
    pub fn place(&mut self, it: OrderIntent) -> Result<IntentHandle, CtxFull> {
        let n = self.b.n_places;
        match self.b.places.get_mut(usize::from(n)) {
            Some(s) => {
                *s = it;
                self.b.n_places = n + 1;
                Ok(self.push_op(OpKind::Place, n))
            }
            None => Err(self.full()),
        }
    }
    /// Replaces the (account, group) target (a field-for-field equal target is not a replacement).
    pub fn target(&mut self, t: TargetIntent) -> Result<IntentHandle, CtxFull> {
        let n = self.b.n_targets;
        if usize::from(n) >= CAP_TARGET {
            return Err(self.full());
        }
        if let Some(s) = self.b.targets.get_mut(usize::from(n)) {
            *s = t;
        }
        self.b.n_targets = n + 1;
        Ok(self.push_op(OpKind::Target, n))
    }
    /// Execution stops tracking the group; positions stay.
    pub fn clear_target(&mut self, a: AccountId, g: GroupId) -> Result<(), CtxFull> {
        let n = self.b.n_targets;
        if usize::from(n) >= CAP_TARGET {
            return Err(self.full());
        }
        if let Some(s) = self.b.clears.get_mut(usize::from(n)) {
            *s = (a, g);
        }
        self.b.n_targets = n + 1;
        self.push_op(OpKind::Clear, n);
        Ok(())
    }
    fn cancel_req(&mut self, c: CancelReq) -> Result<(), CtxFull> {
        let n = self.b.n_cancels;
        match self.b.cancels.get_mut(usize::from(n)) {
            Some(s) => {
                *s = c;
                self.b.n_cancels = n + 1;
                self.push_op(OpKind::Cancel, n);
                Ok(())
            }
            None => Err(self.full()),
        }
    }
    pub fn cancel(&mut self, h: OrderRef) -> Result<(), CtxFull> {
        self.cancel_req(CancelReq { r: h, amend: None })
    }
    pub fn amend(&mut self, h: OrderRef, px: Px, qty: Qty) -> Result<(), CtxFull> {
        self.cancel_req(CancelReq {
            r: h,
            amend: Some((px, qty)),
        })
    }
    /// `at` is this run's TSC (from an engine_ns deadline use `Now::tsc_of`); persistence across
    /// restarts belongs to hft-sequencer.
    pub fn timer_at(&mut self, at: Tsc, tag: u32, group: GroupId) -> Result<TimerId, CtxFull> {
        let n = self.b.n_timers;
        if usize::from(n) >= CAP_TIMER
            || self.b.next_timer == u64::MAX
            || self.timer_quota.as_ref().is_some_and(|q| *q.remaining == 0)
        {
            return Err(self.full());
        }
        if let Some(q) = self.timer_quota.as_mut() {
            *q.remaining -= 1;
        }
        let id = TimerId(self.b.next_timer);
        self.b.next_timer += 1;
        if let Some(s) = self.b.timers.get_mut(usize::from(n)) {
            *s = TimerReq { id, at, tag, group };
        }
        self.b.n_timers = n + 1;
        self.push_op(OpKind::Timer, n);
        Ok(id)
    }
    /// Cancels a registered timer (excess beyond the per-event capacity is dropped and counted).
    pub fn cancel_timer(&mut self, id: TimerId) {
        let n = self.b.n_cancel_timers;
        if usize::from(n) >= CAP_CANCEL_TIMER {
            self.full();
            return;
        }
        let already_released = self
            .b
            .cancel_timers
            .iter()
            .zip(&self.b.cancel_releases)
            .take(usize::from(n))
            .any(|(old, released)| *old == id && *released);
        let pending = self
            .b
            .timers
            .iter()
            .take(usize::from(self.b.n_timers))
            .any(|t| t.id == id);
        let release = !already_released
            && self
                .timer_quota
                .as_ref()
                .is_some_and(|q| pending || q.registered.binary_search_by_key(&id, |t| t.id).is_ok());
        if release && let Some(q) = self.timer_quota.as_mut() {
            *q.remaining = q.remaining.saturating_add(1);
        }
        if let Some(s) = self.b.cancel_releases.get_mut(usize::from(n)) {
            *s = release;
        }
        match self.b.cancel_timers.get_mut(usize::from(n)) {
            Some(s) => {
                *s = id;
                self.b.n_cancel_timers = n + 1;
                self.push_op(OpKind::CancelTimer, n);
            }
            None => {
                self.full();
            }
        }
    }
    /// → `Note{source = Strategy}`; full → dropped and counted. `code`/`level` must be registered
    /// in `DESC.notes`; `group` = the Note's GroupId (`GROUP_NONE` for instance level).
    pub fn note(&mut self, code: u32, level: NoteLevel, group: GroupId, inst: InstId, v: [u64; 8]) {
        let n = self.b.n_notes;
        match self.b.notes.get_mut(usize::from(n)) {
            Some(s) => {
                *s = NoteReq {
                    code,
                    level,
                    group,
                    inst,
                    v,
                };
                self.b.n_notes = n + 1;
            }
            None => {
                self.full();
            }
        }
    }
    /// Requests a `Halted` cap on one cell of this instance (its effective mode drops to
    /// CloseOnly; only `Resume{Halted}` releases it).
    pub fn request_halt(&mut self, g: GroupId, reason: u16) -> Result<(), CtxFull> {
        let n = self.b.n_halts;
        match self.b.halts.get_mut(usize::from(n)) {
            Some(s) => {
                *s = (g, reason);
                self.b.n_halts = n + 1;
                self.push_op(OpKind::Halt, n);
                Ok(())
            }
            None => Err(self.full()),
        }
    }
    /// Result of an intent of the immediately preceding `on_event` call of this instance.
    pub fn outcome(&self, h: IntentHandle) -> Option<IntentOutcome> {
        self.b.outcomes.get(usize::from(h.0)).copied().flatten()
    }
}

// ------------------------------------------------------------------------------- subscriptions

/// Which part receives a subscribed stream.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum Part {
    Fast,
    Model,
    Both,
}

impl Part {
    pub const fn fast(self) -> bool {
        matches!(self, Part::Fast | Part::Both)
    }
    pub const fn model(self) -> bool {
        matches!(self, Part::Model | Part::Both)
    }
}

/// Market streams of a subscription.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct EvMask(pub u8);

impl EvMask {
    pub const BBO: EvMask = EvMask(1);
    pub const BOOK: EvMask = EvMask(2);
    pub const TRADE: EvMask = EvMask(4);
    pub const FUNDING: EvMask = EvMask(8);
    pub const MARK: EvMask = EvMask(16);
    pub const INDEX: EvMask = EvMask(32);
    pub const LIQUIDATION: EvMask = EvMask(64);
    pub const ALL: EvMask = EvMask(127);
    pub const fn contains(self, o: EvMask) -> bool {
        self.0 & o.0 == o.0
    }
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl core::ops::BitOr for EvMask {
    type Output = EvMask;
    fn bitor(self, o: EvMask) -> EvMask {
        EvMask(self.0 | o.0)
    }
}

/// Own-instance events a Model may read (`SubsBuilder::model_own`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct OwnEvMask(pub u8);

impl OwnEvMask {
    pub const ORDER: OwnEvMask = OwnEvMask(1);
    pub const EXEC: OwnEvMask = OwnEvMask(2);
    pub const MANUAL: OwnEvMask = OwnEvMask(4);
    pub const fn contains(self, o: OwnEvMask) -> bool {
        self.0 & o.0 == o.0
    }
}

impl core::ops::BitOr for OwnEvMask {
    type Output = OwnEvMask;
    fn bitor(self, o: OwnEvMask) -> OwnEvMask {
        OwnEvMask(self.0 | o.0)
    }
}

/// `market` failure; the strategy skips the subscription and writes a Note — `SetParams` still
/// succeeds.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum SubsErr {
    /// The inst's entries (all instances) reached `max_subs_per_inst`, or a new subscription index
    /// is needed and `max_insts` is used up.
    Full,
    /// The inst's venue has no feed in this hft-core instance.
    NoFeed,
}

/// One market subscription.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct SubEntry {
    pub inst: InstId,
    pub gix: u16,
    pub ev: EvMask,
    pub part: Part,
}

/// Engine-side limits of one `subscriptions` call (filled by the sequencer from ECF1 section 3 and
/// the other instances' subscriptions).
#[derive(Copy, Clone, Debug)]
pub struct SubsLimits<'a> {
    pub rd: &'a RefData,
    /// Venues with a feed in this hft-core instance.
    pub feeds: VenueSet,
    /// Per `InstId`: entries this instance may still add (`max_subs_per_inst` minus the other
    /// instances' entries; 0 when the inst has no subscription index and none is left). Out of
    /// range → 0.
    pub room: &'a [u16],
}

/// Collects the subscriptions of one instance (command path; preallocated by the sequencer and
/// reused with [`SubsBuilder::reset`]).
#[derive(Debug)]
pub struct SubsBuilder<'a> {
    lim: SubsLimits<'a>,
    entries: Vec<SubEntry>,
    used: Vec<u16>,
    model_tick: Option<u32>,
    own: OwnEvMask,
    warmup: Part,
}

impl<'a> SubsBuilder<'a> {
    /// A builder for `cap_entries` entries.
    pub fn new(lim: SubsLimits<'a>, cap_entries: usize) -> SubsBuilder<'a> {
        SubsBuilder {
            entries: Vec::with_capacity(cap_entries),
            used: vec![0; lim.room.len()],
            lim,
            model_tick: None,
            own: OwnEvMask(0),
            warmup: Part::Model,
        }
    }
    /// Clears the collected subscriptions and sets new limits (reuses the allocations when the
    /// room table has the same length).
    pub fn reset(&mut self, lim: SubsLimits<'a>) {
        self.entries.clear();
        self.used.clear();
        self.used.resize(lim.room.len(), 0);
        self.lim = lim;
        self.model_tick = None;
        self.own = OwnEvMask(0);
        self.warmup = Part::Model;
    }
    /// Market subscription; an inst subscribed by several groups is delivered once per group in
    /// gix order. Any refdata inst is allowed, including reference insts outside the account's
    /// venues (read only).
    pub fn market(&mut self, inst: InstId, gix: u16, ev: EvMask, part: Part) -> Result<(), SubsErr> {
        let venue = self.lim.rd.inst(inst).venue;
        if !self.lim.feeds.contains(venue) {
            return Err(SubsErr::NoFeed);
        }
        let ix = inst.0 as usize;
        let room = self.lim.room.get(ix).copied().unwrap_or(0);
        let used = self.used.get_mut(ix).ok_or(SubsErr::Full)?;
        if *used >= room {
            return Err(SubsErr::Full);
        }
        *used += 1;
        self.entries.push(SubEntry { inst, gix, ev, part });
        Ok(())
    }
    /// Model tick every `every_ms`, UTC aligned (see README for the delivery rule).
    pub fn model_tick(&mut self, every_ms: u32) {
        self.model_tick = Some(every_ms);
    }
    /// The Model reads this instance's Order/Exec/Manual (read only).
    pub fn model_own(&mut self, m: OwnEvMask) {
        self.own = OwnEvMask(self.own.0 | m.0);
    }
    /// Warmup delivery: not called = Model only; `Part::Both` = the fast part too.
    pub fn warmup(&mut self, part: Part) {
        self.warmup = part;
    }
    /// Collected market entries in call order.
    pub fn entries(&self) -> &[SubEntry] {
        &self.entries
    }
    pub fn model_tick_ms(&self) -> Option<u32> {
        self.model_tick
    }
    pub fn own_mask(&self) -> OwnEvMask {
        self.own
    }
    pub fn warmup_part(&self) -> Part {
        self.warmup
    }
}
