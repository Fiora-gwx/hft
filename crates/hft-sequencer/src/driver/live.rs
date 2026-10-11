//! The live driver (README "主循环" a–g, "环境决定写成 Input", "BBO 合并", "出环满", "毒丸输入隔离"):
//! ring order, yielding, per-round limits, mid-round timer checks, environment decisions written
//! as Inputs, BBO conflation, synthetic `SendReport{RingFull}`, staged `FeedCmd`, kill words and
//! quarantine. The rings, the TSC source and the Mailbox come through [`LiveIo`].

use std::collections::VecDeque;

use hft_event::{
    BlobRef, ClockStatus, CmdHdr, CmdScope, ControlCmd, EngineClock, EventKind, FeedCmd, GwCmd, GwOp,
    HostProfile, Note, NoteLevel, NoteSource, RecordFlags, SendReport, SendStatus, Timer,
};
use hft_journal::{RollAt, RollReason};
use hft_strategy_sdk::StrategySet;
use hft_types::{AccountId, GroupId, InstId, KillBits, KillLevel, SlotId, StrategyId, Tsc, VenueId};
use zerocopy::IntoBytes;

use super::{Driver, DriverMode, DriverView, ParseCx, RollInfo};
use crate::journal::JournalSink;
use crate::parsed::ParsedConfig;
use crate::types::{ConfigErr, EmitResult, InputSlot, OutRecord, OutRing};

/// An input ring.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum RingId {
    Ctl,
    Hk,
    Rep(u8),
    Chain,
    Bbo(u8),
    Advice,
    Md(u8),
    RepSim,
}

/// The live environment: rings, TSC, Mailbox, kill words, journal (production: hft-rt rings
/// assembled by apps/hft-core; tests: in-memory rings and a manual TSC).
pub trait LiveIo<S: StrategySet> {
    type J: JournalSink;
    fn journal(&mut self) -> &mut Self::J;
    /// The current TSC.
    fn tsc(&mut self) -> Tsc;
    fn n_rep(&self) -> u8;
    fn n_bbo(&self) -> u8;
    fn n_md(&self) -> u8;
    fn has_chain(&self) -> bool {
        false
    }
    /// Acquire read of a ring's backlog (head check).
    fn len(&mut self, r: RingId) -> usize;
    /// Byte length of the next record (ctl byte budget); `None` = empty.
    fn peek_len(&mut self, r: RingId) -> Option<usize>;
    /// Instrument of the next `Bbo` (conflation); `None` = empty or not a Bbo.
    fn peek_inst(&mut self, r: RingId) -> Option<InstId> {
        let _ = r;
        None
    }
    /// Pops the next element as an Input (`kind`, `flags`, body); `false` = empty.
    fn pop(&mut self, r: RingId, out: &mut InputSlot) -> bool;
    fn push_out(&mut self, ring: OutRing, c: &GwCmd) -> bool;
    fn push_feed(&mut self, v: VenueId, c: &FeedCmd) -> bool;
    fn take_parsed(&mut self, r: &BlobRef) -> Option<ParsedConfig<S>>;
    fn retire(&mut self, p: ParsedConfig<S>);
    fn write_kill(&mut self, bits: &[KillBits]);
    fn roll_info(&mut self) -> RollInfo;
    fn host_profile(&self) -> HostProfile;
    fn clock_status(&self) -> ClockStatus;
    #[cfg(feature = "testkit")]
    fn observe_input(&mut self, _e: &crate::Engine, _input: &InputSlot, _before: bool) {}
    #[cfg(feature = "testkit")]
    fn observe_boundary(&mut self, _state: crate::audit::StateView<'_, S>) {}
    /// UTC of a TSC (TscClock calibration).
    fn utc_of(&self, t: Tsc) -> i64;
    /// Called after every hand-over (test hook for arrivals "during processing").
    fn after_take(&mut self, r: Option<RingId>) {
        let _ = r;
    }
    /// The measurement TSC (`decided_tsc`); production = `tsc()`.
    fn measure(&mut self) -> Tsc {
        self.tsc()
    }
    /// The old journal of a cold start (records of every readable segment, in seq order).
    fn recovery_log(&mut self) -> Option<super::replay::Log> {
        None
    }
}

/// Live driver configuration.
#[derive(Clone, Debug, Default)]
pub struct LiveCfg {
    /// Stop marker found at startup: a `Kill{scope}` from `cli:systemd` right after EngineClock.
    pub stop_kill: Option<(KillLevel, CmdScope)>,
    /// `EngineClock.base_engine_ns` (= `stop_engine_ns()` of the boot).
    pub base_engine_ns: Option<i64>,
    /// `EngineClock.prev_last_seq`.
    pub prev_last_seq: u64,
    /// `EngineClock.source` (Fresh=1 Tail=2 Snapshot=3).
    pub source: u8,
    /// Run-time poison inputs.
    pub quarantine: Vec<u64>,
    /// Capacity of `FeedCmd` staging.
    pub feed_stage_cap: usize,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum Phase {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    Done,
}

#[derive(Clone, Debug, Default)]
struct Counts {
    ctl: u16,
    ctl_bytes: usize,
    hk: u16,
    rep: u16,
    rep_per: Vec<u16>,
    chain: u16,
    bbo: u16,
    bbo_per: Vec<u16>,
    advice: u16,
    sim: u16,
    timers: u16,
    md_start: Option<u64>,
    rep_rr: u8,
    bbo_rr: u8,
    md_rr: u8,
}

/// The live driver.
pub struct LiveDriver<S: StrategySet, IO: LiveIo<S>> {
    pub io: IO,
    pub cfg: LiveCfg,
    started: bool,
    pending_kill: bool,
    phase: Phase,
    c: Counts,
    prev_idle: bool,
    took_any: bool,
    exclusive: bool,
    backpressure: bool,
    rings_empty_at_begin: bool,
    a_done_oms: bool,
    a_done_notify: bool,
    a_done_recheck: bool,
    a_env_done: bool,
    synth: VecDeque<InputSlot>,
    feed_stage: Vec<(VenueId, FeedCmd)>,
    conf: VecDeque<(InputSlot, InstId)>,
    conf_ring: Option<u8>,
    /// Bbo dropped by conflation since the last report.
    pub conflated: u64,
    last_conflate_tsc: u64,
    last_tick_tsc: u64,
    rolled_hour: i64,
    keyframe_due_tsc: Option<u64>,
    kf_window_start: u64,
    kf_done: Vec<InstId>,
    funding_written: Option<(VenueId, i64)>,
    snapshot_round: bool,
    /// A `Timer{Sys, Roll}` was handed in this round (at most one per round).
    roll_this_round: bool,
    lim: Limits,
    /// Inputs handed per round (tests).
    pub taken_log: Vec<(EventKind, Option<RingId>)>,
    _s: core::marker::PhantomData<S>,
}

impl<S: StrategySet, IO: LiveIo<S>> core::fmt::Debug for LiveDriver<S, IO> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LiveDriver").finish_non_exhaustive()
    }
}

const PER_ROUND_TIMERS: u16 = 16;
const CONFLATE_TAKE: usize = 32;

impl<S: StrategySet, IO: LiveIo<S>> LiveDriver<S, IO> {
    pub fn new(io: IO, cfg: LiveCfg) -> LiveDriver<S, IO> {
        LiveDriver {
            io,
            cfg,
            started: false,
            pending_kill: false,
            phase: Phase::Done,
            c: Counts::default(),
            prev_idle: false,
            took_any: false,
            exclusive: false,
            backpressure: false,
            rings_empty_at_begin: false,
            a_done_oms: false,
            a_done_notify: false,
            a_done_recheck: false,
            a_env_done: false,
            synth: VecDeque::with_capacity(64),
            feed_stage: Vec::with_capacity(1024),
            conf: VecDeque::with_capacity(CONFLATE_TAKE),
            conf_ring: None,
            conflated: 0,
            last_conflate_tsc: 0,
            last_tick_tsc: 0,
            rolled_hour: i64::MIN,
            keyframe_due_tsc: None,
            kf_window_start: 0,
            kf_done: Vec::with_capacity(64),
            funding_written: None,
            snapshot_round: false,
            roll_this_round: false,
            lim: Limits::default(),
            taken_log: Vec::with_capacity(4096),
            _s: core::marker::PhantomData,
        }
    }

    fn ms_ticks(v: &DriverView<'_>, ms: u64) -> u64 {
        ((u128::from(ms) * u128::from(v.tsc_hz_milli())) / 1_000_000) as u64
    }

    fn ns_ticks(v: &DriverView<'_>, ns: u64) -> u64 {
        ((u128::from(ns) * u128::from(v.tsc_hz_milli())) / 1_000_000_000_000) as u64
    }

    fn all_rings_empty(&mut self) -> bool {
        let mut n = self.io.len(RingId::Ctl)
            + self.io.len(RingId::Hk)
            + self.io.len(RingId::Advice)
            + self.io.len(RingId::RepSim);
        for k in 0..self.io.n_rep() {
            n += self.io.len(RingId::Rep(k));
        }
        if self.io.has_chain() {
            n += self.io.len(RingId::Chain);
        }
        for k in 0..self.io.n_bbo() {
            n += self.io.len(RingId::Bbo(k));
        }
        for k in 0..self.io.n_md() {
            n += self.io.len(RingId::Md(k));
        }
        n == 0 && self.synth.is_empty()
    }

    fn timer_input(out: &mut InputSlot, owner: u16, id: u32, group: GroupId, deadline: u64, now: Tsc) {
        let t = Timer {
            timer_id: id,
            owner,
            _pad: 0,
            group,
            _pad2: 0,
            deadline_tsc: deadline,
        };
        out.set(RecordFlags(0), now, &t);
    }

    fn sys(out: &mut InputSlot, id: u32, now: Tsc) {
        Self::timer_input(out, 1, id, GroupId(0), now.0, now);
    }

    /// The keyframe (`Sys{Snapshot}`) is due.
    fn keyframe_due(&mut self, v: &DriverView<'_>, now: Tsc) -> bool {
        let p = v.params();
        let period = Self::ms_ticks(v, u64::from(p.keyframe_period_ms));
        let by_time = now.0.saturating_sub(v.last_keyframe_tsc()) >= period;
        let by_events = v.inputs_since_keyframe() >= u64::from(p.keyframe_max_events);
        if (by_time || by_events) && self.keyframe_due_tsc.is_none() {
            self.keyframe_due_tsc = Some(if by_time {
                v.last_keyframe_tsc() + period
            } else {
                now.0
            });
        }
        by_time || by_events
    }

    /// The env decision of this round's a segment (one at a time).
    fn env_input(&mut self, v: &DriverView<'_>, out: &mut InputSlot, now: Tsc) -> bool {
        let idle_ok = self.prev_idle && self.rings_empty_at_begin && !v.wants_continue();
        // Roll: UTC hour (idle round, away from funding settlements) or journal size.
        let utc = self.io.utc_of(now);
        let hour = utc.div_euclid(3_600_000_000_000);
        let size = self.io.journal().needs_roll(now);
        let size_now = matches!(size, Some((_, RollAt::EventBoundary))) && !v.wants_continue();
        let size_idle = matches!(size, Some((_, RollAt::IdleRound))) && idle_ok;
        let near_settle = v
            .funding_times()
            .iter()
            .any(|t| utc >= t - 120_000_000_000 && utc < t + 120_000_000_000);
        let hourly = self.rolled_hour != i64::MIN && hour > self.rolled_hour && idle_ok && !near_settle;
        if self.rolled_hour == i64::MIN {
            self.rolled_hour = hour;
        }
        if (size_now || size_idle || hourly) && !self.roll_this_round {
            self.roll_this_round = true;
            if hourly {
                self.rolled_hour = hour;
            }
            Self::sys(out, 2, now);
            return true;
        }
        // L2 keyframes, amortized over the window.
        let p = *v.params();
        let period = Self::ms_ticks(v, u64::from(p.keyframe_period_ms)).max(1);
        if now.0.saturating_sub(self.kf_window_start) >= period {
            self.kf_window_start = now.0 - (now.0 - self.kf_window_start) % period;
            self.kf_done.clear();
        }
        let books = v.live_books();
        if !books.is_empty() {
            let elapsed = now.0.saturating_sub(self.kf_window_start);
            let target = ((u128::from(elapsed) * books.len() as u128) / u128::from(period)) as usize + 1;
            if self.kf_done.len() < target.min(books.len())
                && let Some(i) = books.iter().find(|b| !self.kf_done.contains(b))
            {
                self.kf_done.push(*i);
                Self::timer_input(out, 2, i.0, GroupId(0), now.0, now);
                return true;
            }
        }
        // SnapshotNow in the first idle round.
        if v.snapshot_requested() && idle_ok {
            Self::sys(out, 7, now);
            return true;
        }
        // DayRoll.
        if v.clock_ok() && v.clock_set() {
            let day = crate::clock::utc_day(utc);
            if day > v.last_day_rolled() {
                Self::sys(out, 3 | (day << 8), now);
                return true;
            }
        }
        // FundingSettle.
        if v.clock_ok()
            && let Some((venue, t)) = v.next_funding_settle()
            && utc >= t
            && self.funding_written != Some((venue, t))
        {
            self.funding_written = Some((venue, t));
            Self::sys(out, 15 | (u32::from(venue.0) << 8), now);
            return true;
        }
        // ConflateReport / AdvisorTick every second.
        let sec = Self::ms_ticks(v, 1_000);
        if now.0.saturating_sub(self.last_conflate_tsc) >= sec {
            self.last_conflate_tsc = now.0;
            let n = self.conflated;
            self.conflated = 0;
            Self::timer_input(out, 1, 10, GroupId(0), n, now);
            return true;
        }
        if now.0.saturating_sub(self.last_tick_tsc) >= sec {
            self.last_tick_tsc = now.0;
            Self::sys(out, 6, now);
            return true;
        }
        false
    }

    /// Mid-round check: a timer is due → end the round.
    fn timer_due(&mut self, v: &DriverView<'_>) -> bool {
        match v.next_deadline() {
            Some(d) => self.io.tsc().0 >= d.0,
            None => false,
        }
    }

    fn quarantined(&mut self, out: &InputSlot) -> bool {
        if self.cfg.quarantine.is_empty() {
            return false;
        }
        let h = input_hash(out);
        if !self.cfg.quarantine.contains(&h) {
            return false;
        }
        let n = Note {
            code: hft_event::note_code(NoteSource::Boot, crate::codes::boot_note::INPUT_QUARANTINED),
            level: NoteLevel::P0,
            source: NoteSource::Boot,
            strategy: StrategyId(0),
            _pad: 0,
            group: GroupId(0),
            inst: InstId(0),
            v: [h, 0, 0, 0, 0, 0, 0, 0],
        };
        let t = out.tsc;
        let _ = self
            .io
            .journal()
            .append_var(EventKind::NOTE, RecordFlags(0), t, &[n.as_bytes()]);
        true
    }

    /// Pops one element of `r` into `out` (stamping the take time); quarantined inputs are
    /// replaced by their Note and skipped.
    fn pop_one(&mut self, r: RingId, out: &mut InputSlot) -> bool {
        loop {
            if !self.io.pop(r, out) {
                return false;
            }
            out.tsc = self.io.tsc();
            if out.kind == EventKind::NOTE || out.kind == EventKind::CLOCK_STATUS {
                // Meta from the hk / advice rings: recorded as they are.
            }
            if self.quarantined(out) {
                continue;
            }
            return true;
        }
    }

    fn take_ctl(&mut self, out: &mut InputSlot) -> bool {
        let lim = self.lim_ctl();
        if self.c.ctl >= lim.0 {
            return false;
        }
        let Some(n) = self.io.peek_len(RingId::Ctl) else {
            return false;
        };
        if self.c.ctl > 0 && self.c.ctl_bytes + n > lim.1 {
            return false;
        }
        if self.pop_one(RingId::Ctl, out) {
            self.c.ctl += 1;
            self.c.ctl_bytes += n;
            return true;
        }
        false
    }

    fn take_hk(&mut self, out: &mut InputSlot) -> bool {
        if self.c.hk >= self.lim_hk() {
            return false;
        }
        if self.pop_one(RingId::Hk, out) {
            self.c.hk += 1;
            return true;
        }
        false
    }

    fn take_rep(&mut self, out: &mut InputSlot, v: &DriverView<'_>) -> bool {
        let (per, total) = self.lim_rep();
        let n = self.io.n_rep();
        if self.c.rep_per.len() < usize::from(n) {
            self.c.rep_per.resize(usize::from(n), 0);
        }
        for k in 0..n {
            if self.c.rep >= total {
                break;
            }
            let ix = (self.c.rep_rr + k) % n.max(1);
            let used = self.c.rep_per.get(usize::from(ix)).copied().unwrap_or(0);
            if used >= per || self.io.len(RingId::Rep(ix)) == 0 {
                continue;
            }
            if self.backpressure && v.notify_headroom() < v.notify_unit() {
                return false;
            }
            if self.pop_one(RingId::Rep(ix), out) {
                if let Some(u) = self.c.rep_per.get_mut(usize::from(ix)) {
                    *u += 1;
                }
                self.c.rep += 1;
                self.c.rep_rr = (ix + 1) % n.max(1);
                return true;
            }
        }
        if self.io.has_chain()
            && self.c.chain < self.lim_chain()
            && self.c.rep < total
            && self.io.len(RingId::Chain) > 0
            && self.pop_one(RingId::Chain, out)
        {
            self.c.chain += 1;
            self.c.rep += 1;
            return true;
        }
        false
    }

    fn take_bbo(&mut self, out: &mut InputSlot, v: &DriverView<'_>) -> bool {
        // Conflated batch first.
        if let Some((s, _)) = self.conf.pop_front() {
            *out = s;
            out.tsc = self.io.tsc();
            self.c.bbo += 1;
            return true;
        }
        let (per, total) = self.lim_bbo();
        let n = self.io.n_bbo();
        if self.c.bbo_per.len() < usize::from(n) {
            self.c.bbo_per.resize(usize::from(n), 0);
        }
        for k in 0..n {
            if self.c.bbo >= total {
                break;
            }
            let ix = (self.c.bbo_rr + k) % n.max(1);
            let used = self.c.bbo_per.get(usize::from(ix)).copied().unwrap_or(0);
            let backlog = self.io.len(RingId::Bbo(ix));
            if used >= per || backlog == 0 {
                continue;
            }
            self.c.bbo_rr = (ix + 1) % n.max(1);
            if backlog > usize::from(v.params().conflate_backlog) {
                // Conflate up to 32 records: keep the last Bbo per inst.
                let mut batch: Vec<InputSlot> = Vec::with_capacity(CONFLATE_TAKE);
                for _ in 0..CONFLATE_TAKE {
                    let mut s = InputSlot::with_capacity(128);
                    if !self.io.pop(RingId::Bbo(ix), &mut s) {
                        break;
                    }
                    batch.push(s);
                }
                if let Some(u) = self.c.bbo_per.get_mut(usize::from(ix)) {
                    *u += batch.len() as u16;
                }
                let insts: Vec<InstId> = batch
                    .iter()
                    .map(|s| s.read::<hft_event::Bbo>().map_or(InstId(0), |b| b.h.inst))
                    .collect();
                let mut kept: Vec<(InputSlot, InstId)> = Vec::new();
                for (k2, s) in batch.into_iter().enumerate() {
                    let i = insts.get(k2).copied().unwrap_or(InstId(0));
                    let later = insts.iter().skip(k2 + 1).any(|x| *x == i);
                    if later {
                        self.conflated += 1;
                        continue;
                    }
                    let dup = insts.iter().take(k2).any(|x| *x == i);
                    let mut s = s;
                    if dup {
                        s.flags = RecordFlags(s.flags.0 | RecordFlags::CONFLATED.0);
                    }
                    kept.push((s, i));
                }
                self.conf.extend(kept);
                self.conf_ring = Some(ix);
                if let Some((s, _)) = self.conf.pop_front() {
                    *out = s;
                    out.tsc = self.io.tsc();
                    if self.quarantined(out) {
                        return self.take_bbo(out, v);
                    }
                    self.c.bbo += 1;
                    return true;
                }
                continue;
            }
            if self.pop_one(RingId::Bbo(ix), out) {
                if let Some(u) = self.c.bbo_per.get_mut(usize::from(ix)) {
                    *u += 1;
                }
                self.c.bbo += 1;
                return true;
            }
        }
        false
    }

    fn take_advice(&mut self, out: &mut InputSlot) -> bool {
        let (per, _) = self.lim_advice();
        if self.c.advice >= per {
            return false;
        }
        if self.pop_one(RingId::Advice, out) {
            self.c.advice += 1;
            return true;
        }
        false
    }

    fn take_md(&mut self, out: &mut InputSlot, v: &DriverView<'_>) -> bool {
        let now = self.io.tsc().0;
        if let Some(s) = self.c.md_start
            && now.saturating_sub(s) >= Self::ns_ticks(v, u64::from(v.params().md_round_ns))
        {
            return false;
        }
        let n = self.io.n_md();
        for k in 0..n {
            let ix = (self.c.md_rr + k) % n.max(1);
            if self.io.len(RingId::Md(ix)) == 0 {
                continue;
            }
            if self.pop_one(RingId::Md(ix), out) {
                if self.c.md_start.is_none() {
                    self.c.md_start = Some(out.tsc.0);
                }
                self.c.md_rr = (ix + 1) % n.max(1);
                return true;
            }
        }
        false
    }

    fn take_sim(&mut self, out: &mut InputSlot, v: &DriverView<'_>) -> bool {
        if self.c.sim >= v.params().sim_max_per_round {
            return false;
        }
        if self.pop_one(RingId::RepSim, out) {
            self.c.sim += 1;
            return true;
        }
        false
    }

    fn lim_ctl(&self) -> (u16, usize) {
        (self.lim.ctl, self.lim.ctl_bytes)
    }
    fn lim_hk(&self) -> u16 {
        self.lim.hk
    }
    fn lim_rep(&self) -> (u16, u16) {
        (self.lim.rep_per, self.lim.rep_total)
    }
    fn lim_chain(&self) -> u16 {
        self.lim.chain
    }
    fn lim_bbo(&self) -> (u16, u16) {
        (self.lim.bbo_per, self.lim.bbo_total)
    }
    fn lim_advice(&self) -> (u16, u16) {
        (self.lim.advice, self.lim.advice)
    }

    /// Yield check after a Bbo / Advice (ctl, live rep) or md (also bbo).
    fn preempt(&mut self, out: &mut InputSlot, v: &DriverView<'_>, with_bbo: bool) -> Option<RingId> {
        if self.io.len(RingId::Ctl) > 0 && self.take_ctl(out) {
            return Some(RingId::Ctl);
        }
        if self.take_rep(out, v) {
            return Some(RingId::Rep(0));
        }
        if with_bbo && self.take_bbo(out, v) {
            return Some(RingId::Bbo(0));
        }
        None
    }

    fn handed(&mut self, out: &InputSlot, r: Option<RingId>) -> bool {
        self.took_any = true;
        self.taken_log.push((out.kind, r));
        self.io.after_take(r);
        true
    }
}

/// Round limits (copied from section 5 at the first round).
#[derive(Copy, Clone, Debug, Default)]
struct Limits {
    ctl: u16,
    ctl_bytes: usize,
    hk: u16,
    rep_per: u16,
    rep_total: u16,
    chain: u16,
    bbo_per: u16,
    bbo_total: u16,
    advice: u16,
}

/// `xxh3_64(kind ‖ body)`.
pub fn input_hash(s: &InputSlot) -> u64 {
    let mut h = xxhash_rust::xxh3::Xxh3::new();
    h.update(&s.kind.0.to_le_bytes());
    h.update(s.body());
    h.digest()
}

// The limits live outside `Counts` (they do not reset per round).
impl<S: StrategySet, IO: LiveIo<S>> LiveDriver<S, IO> {
    #[allow(clippy::needless_pass_by_ref_mut)]
    fn set_limits(&mut self, v: &DriverView<'_>) {
        use crate::params::RingClass as C;
        let p = v.params();
        let r = |c: C| p.ring(c);
        self.lim = Limits {
            ctl: r(C::Ctl).per_ring.min(r(C::Ctl).total),
            ctl_bytes: p.ctl_bytes_per_round as usize,
            hk: r(C::Hk).per_ring.min(r(C::Hk).total),
            rep_per: r(C::Rep).per_ring,
            rep_total: r(C::Rep).total,
            chain: r(C::Chain).per_ring,
            bbo_per: r(C::Bbo).per_ring,
            bbo_total: r(C::Bbo).total,
            advice: r(C::Advice).per_ring.min(r(C::Advice).total),
        };
    }
}

impl<S: StrategySet, IO: LiveIo<S>> Driver<S> for LiveDriver<S, IO> {
    type J = IO::J;

    fn begin_round(&mut self, v: &DriverView<'_>) {
        self.set_limits(v);
        self.prev_idle = !self.took_any;
        self.took_any = false;
        self.c = Counts {
            rep_per: core::mem::take(&mut self.c.rep_per),
            bbo_per: core::mem::take(&mut self.c.bbo_per),
            rep_rr: self.c.rep_rr,
            bbo_rr: self.c.bbo_rr,
            md_rr: self.c.md_rr,
            ..Counts::default()
        };
        for x in self.c.rep_per.iter_mut() {
            *x = 0;
        }
        for x in self.c.bbo_per.iter_mut() {
            *x = 0;
        }
        self.phase = Phase::A;
        self.a_done_oms = false;
        self.a_done_notify = false;
        self.a_done_recheck = false;
        self.a_env_done = false;
        self.exclusive = false;
        self.snapshot_round = false;
        self.roll_this_round = false;
        self.backpressure = false;
        self.rings_empty_at_begin = self.all_rings_empty();
        // Staged FeedCmd first.
        if !self.feed_stage.is_empty() {
            let staged = core::mem::take(&mut self.feed_stage);
            let mut rest = Vec::with_capacity(staged.len());
            let mut blocked = false;
            for (venue, c) in staged {
                if blocked || !self.io.push_feed(venue, &c) {
                    blocked = true;
                    rest.push((venue, c));
                }
            }
            self.feed_stage = rest;
        }
        if !self.started {
            self.exclusive = true;
            return;
        }
        let now = self.io.tsc();
        // Keyframe round: due, previous round idle, rings empty, nothing to continue; or the
        // maximum deferral (2 periods) at an event boundary.
        let due = self.keyframe_due(v, now);
        if due && !v.wants_continue() {
            let period = Self::ms_ticks(v, u64::from(v.params().keyframe_period_ms));
            let forced = self
                .keyframe_due_tsc
                .is_some_and(|d| now.0 >= d.saturating_add(2 * period));
            if (self.prev_idle && self.rings_empty_at_begin) || forced {
                self.snapshot_round = true;
                self.exclusive = true;
                return;
            }
        }
        // Backpressure: headroom < 2U.
        if v.notify_headroom() < 2 * v.notify_unit() {
            self.backpressure = true;
        }
    }

    fn take(&mut self, v: &DriverView<'_>, out: &mut InputSlot) -> bool {
        // Synthetic inputs (SendReport{RingFull}) go first.
        if let Some(s) = self.synth.pop_front() {
            *out = s;
            out.tsc = self.io.tsc();
            return self.handed(out, None);
        }
        loop {
            match self.phase {
                Phase::A => {
                    let now = self.io.tsc();
                    if !self.started {
                        self.started = true;
                        let hz = self.io.roll_info().tsc_hz_milli;
                        let ec = EngineClock {
                            base_engine_ns: self.cfg.base_engine_ns.unwrap_or(v.stop_engine_ns()),
                            tsc_hz_milli: hz,
                            prev_last_seq: self.cfg.prev_last_seq,
                            source: if self.cfg.source == 0 { 1 } else { self.cfg.source },
                            _pad: [0; 7],
                        };
                        out.set(RecordFlags(0), now, &ec);
                        self.pending_kill = self.cfg.stop_kill.is_some();
                        if !self.pending_kill {
                            self.phase = Phase::Done;
                        }
                        let now2 = self.io.tsc();
                        self.last_conflate_tsc = now2.0;
                        self.last_tick_tsc = now2.0;
                        self.kf_window_start = now2.0;
                        return self.handed(out, None);
                    }
                    if self.pending_kill {
                        self.pending_kill = false;
                        self.phase = Phase::Done;
                        if let Some((level, scope)) = self.cfg.stop_kill {
                            stop_kill_input(out, now, level, scope);
                            return self.handed(out, None);
                        }
                    }
                    if self.snapshot_round {
                        self.snapshot_round = false;
                        self.phase = Phase::Done;
                        self.keyframe_due_tsc = None;
                        Self::sys(out, 1, now);
                        return self.handed(out, None);
                    }
                    if self.exclusive {
                        self.phase = Phase::Done;
                        continue;
                    }
                    if self.backpressure {
                        self.phase = Phase::C;
                        if v.notify_wants_continue() {
                            Self::sys(out, 17, now);
                            return self.handed(out, None);
                        }
                        continue;
                    }
                    if v.refdata_recheck_due() && !self.a_done_recheck {
                        self.a_done_recheck = true;
                        Self::sys(out, 18, now);
                        return self.handed(out, None);
                    }
                    if self.c.timers < PER_ROUND_TIMERS
                        && let Some(t) = v.heap_top()
                        && now.0 >= t.deadline
                    {
                        self.c.timers += 1;
                        Self::timer_input(out, t.owner, t.timer_id, GroupId(t.group), t.deadline, now);
                        return self.handed(out, None);
                    }
                    if !self.a_done_oms && v.oms_wants_continue() {
                        self.a_done_oms = true;
                        Self::timer_input(out, 3, 1, GroupId(0), now.0, now);
                        return self.handed(out, None);
                    }
                    if !self.a_done_notify && v.notify_wants_continue() {
                        self.a_done_notify = true;
                        Self::sys(out, 17, now);
                        return self.handed(out, None);
                    }
                    if !self.a_env_done {
                        if self.env_input(v, out, now) {
                            return self.handed(out, None);
                        }
                        self.a_env_done = true;
                    }
                    self.phase = Phase::B;
                }
                Phase::B => {
                    if self.take_ctl(out) {
                        return self.handed(out, Some(RingId::Ctl));
                    }
                    if self.take_hk(out) {
                        hk_to_input(out);
                        return self.handed(out, Some(RingId::Hk));
                    }
                    self.phase = Phase::C;
                }
                Phase::C => {
                    if self.timer_due(v) {
                        self.phase = Phase::Done;
                        return false;
                    }
                    if self.take_rep(out, v) {
                        return self.handed(out, Some(RingId::Rep(0)));
                    }
                    self.phase = if self.backpressure { Phase::Done } else { Phase::D };
                }
                Phase::D => {
                    if self.timer_due(v) {
                        self.phase = Phase::Done;
                        return false;
                    }
                    if let Some(r) = self.preempt(out, v, false) {
                        return self.handed(out, Some(r));
                    }
                    if self.take_bbo(out, v) {
                        return self.handed(out, Some(RingId::Bbo(0)));
                    }
                    self.phase = Phase::E;
                }
                Phase::E => {
                    if self.timer_due(v) {
                        self.phase = Phase::Done;
                        return false;
                    }
                    if let Some(r) = self.preempt(out, v, false) {
                        return self.handed(out, Some(r));
                    }
                    if self.take_advice(out) {
                        return self.handed(out, Some(RingId::Advice));
                    }
                    self.phase = Phase::F;
                }
                Phase::F => {
                    if self.timer_due(v) {
                        self.phase = Phase::Done;
                        return false;
                    }
                    if let Some(r) = self.preempt(out, v, true) {
                        return self.handed(out, Some(r));
                    }
                    if self.take_md(out, v) {
                        return self.handed(out, Some(RingId::Md(0)));
                    }
                    self.phase = Phase::G;
                }
                Phase::G => {
                    if self.timer_due(v) {
                        self.phase = Phase::Done;
                        return false;
                    }
                    if self.take_sim(out, v) {
                        return self.handed(out, Some(RingId::RepSim));
                    }
                    self.phase = Phase::Done;
                }
                Phase::Done => return false,
            }
        }
    }

    fn run_tsc(&mut self) -> Option<Tsc> {
        Some(self.io.tsc())
    }

    fn measure(&mut self) -> Tsc {
        self.io.measure()
    }

    fn emit(&mut self, o: &OutRecord<'_>) -> EmitResult {
        match o {
            OutRecord::Out(ring, c) => {
                if self.io.push_out(*ring, c) {
                    return EmitResult::Pushed;
                }
                if matches!(c.op, GwOp::Place | GwOp::Cancel | GwOp::Amend | GwOp::Query) {
                    let venue = match ring {
                        OutRing::Venue(v) => *v,
                        OutRing::Sim => VenueId(0),
                    };
                    let sr = SendReport {
                        decision_seq: c.decision_seq,
                        sent_tsc: Tsc(0),
                        slot: c.slot,
                        r#gen: c.r#gen,
                        target_slot: c.target_slot,
                        target_gen: 0,
                        venue,
                        account: c.account,
                        conn: hft_types::ConnId(0),
                        status: SendStatus::RingFull,
                        op: c.op,
                        _pad: [0; 3],
                        os_errno: 0,
                        _pad2: 0,
                    };
                    let mut s = InputSlot::with_capacity(64);
                    s.set(RecordFlags(0), Tsc(0), &sr);
                    self.synth.push_back(s);
                }
                EmitResult::Full
            }
            OutRecord::FeedCtl(venue, c) => {
                if self.feed_stage.is_empty() && self.io.push_feed(*venue, c) {
                    return EmitResult::Pushed;
                }
                let key = feed_inst(c);
                self.feed_stage
                    .retain(|(v, x)| !(*v == *venue && key.is_some() && feed_inst(x) == key));
                self.feed_stage.push((*venue, **c));
                EmitResult::Full
            }
        }
    }

    fn take_parsed(&mut self, r: &BlobRef, _cx: &ParseCx<'_, S>) -> Result<ParsedConfig<S>, ConfigErr> {
        match self.io.take_parsed(r) {
            Some(p) if p.sha256() == &r.sha256 => Ok(p),
            Some(p) => {
                self.io.retire(p);
                Err(ConfigErr::Missing)
            }
            None => Err(ConfigErr::Missing),
        }
    }

    fn retire(&mut self, old: ParsedConfig<S>) {
        self.io.retire(old);
    }

    fn journal(&mut self) -> &mut Self::J {
        self.io.journal()
    }

    fn mode(&self) -> DriverMode {
        DriverMode::Live
    }

    fn write_kill(&mut self, bits: &[KillBits]) {
        self.io.write_kill(bits);
    }

    fn roll_info(&mut self) -> RollInfo {
        self.io.roll_info()
    }

    fn host_profile(&self) -> HostProfile {
        self.io.host_profile()
    }

    fn clock_status(&self) -> ClockStatus {
        self.io.clock_status()
    }
    #[cfg(feature = "testkit")]
    fn observe_input(&mut self, e: &crate::Engine, input: &InputSlot, before: bool) {
        self.io.observe_input(e, input, before);
    }

    #[cfg(feature = "testkit")]
    fn observe_boundary(&mut self, state: crate::audit::StateView<'_, S>) {
        self.io.observe_boundary(state);
    }

    fn start_log(&mut self) -> Option<super::replay::Log> {
        self.io.recovery_log()
    }
}

fn feed_inst(c: &FeedCmd) -> Option<InstId> {
    match c {
        FeedCmd::Subscribe { inst, .. } | FeedCmd::Unsubscribe { inst } | FeedCmd::Resync { inst } => {
            Some(*inst)
        }
        FeedCmd::Rotate { .. } => None,
    }
}

/// An hk `Sys{timer_id}` element becomes a `Timer{Sys}` Input (the test rings deliver it as a
/// Timer body already; nothing to convert).
fn hk_to_input(_out: &mut InputSlot) {}

/// The stop-marker `Kill` from `cli:systemd` (non-HARD, no flatten).
fn stop_kill_input(out: &mut InputSlot, now: Tsc, level: KillLevel, scope: CmdScope) {
    let mut h = CmdHdr::zeroed();
    let issuer = b"cli:systemd";
    if let Some(d) = h.issuer.get_mut(..issuer.len()) {
        d.copy_from_slice(issuer);
    }
    h.cmd_id[0] = 0x5D;
    h.cmd_id[1..9].copy_from_slice(&now.0.to_le_bytes());
    h.account = scope.account;
    let c = ControlCmd::Kill {
        scope,
        level,
        flatten: false,
        cancel_resting: true,
        hard: false,
    };
    let mut buf = [0u8; 512];
    let n = c.encode(&h, &mut buf).unwrap_or(0);
    out.set_bytes(
        EventKind::CONTROL_CMD,
        RecordFlags(0),
        now,
        buf.get(..n).unwrap_or(&[]),
    );
}

#[allow(dead_code)]
fn _unused(_: RollReason, _: AccountId, _: SlotId) {}
