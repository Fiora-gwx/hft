//! The four drivers' contract (README "装配与运行" `Driver`) and the read-only `DriverView`.

pub mod live;
pub mod replay;

use hft_event::{BlobRef, ClockStatus, HostProfile};
use hft_refdata::RefData;
use hft_rt::CalibPair;
use hft_strategy_sdk::StrategySet;
use hft_types::{AccountId, KillBits, Tsc, VenueId};

use crate::engine::Engine;
use crate::journal::JournalSink;
use crate::parsed::{ParamBase, ParsedConfig, RiskDecode};
use crate::types::{ConfigErr, EmitResult, InputSlot, OutRecord};

pub use live::{LiveDriver, LiveIo, RingId};
pub use replay::{Mismatch, ReplayDriver, ReplayMode, ReplaySink};

/// Which driver runs the sequencer.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum DriverMode {
    Live,
    Recover,
    Verify,
    Mirror,
}

/// Segment header values of a roll (from the driver's clock).
#[derive(Copy, Clone, Debug, Default)]
pub struct RollInfo {
    pub utc_ns: i64,
    pub calib: CalibPair,
    pub tsc_hz_milli: u64,
}

/// What `take_parsed` needs to rebuild a Mailbox object from a logged blob (replay drivers).
pub struct ParseCx<'a, S: StrategySet> {
    pub rd: &'a RefData,
    pub max_groups: u16,
    pub base: Option<ParamBase<'a, S>>,
    pub risk_account: AccountId,
    pub risk_decode: Option<RiskDecode>,
}

impl<S: StrategySet> core::fmt::Debug for ParseCx<'_, S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ParseCx")
            .field("max_groups", &self.max_groups)
            .finish_non_exhaustive()
    }
}

/// The driver of a sequencer (live, recover, verify, mirror).
pub trait Driver<S: StrategySet> {
    type J: JournalSink;
    /// Once per round: the live driver decides this round's driver-generated Inputs.
    fn begin_round(&mut self, v: &DriverView<'_>);
    /// Hands over the next Input of this round; `false` = nothing more this round.
    fn take(&mut self, v: &DriverView<'_>, out: &mut InputSlot) -> bool;
    /// This run's TscClock reading (live, recover); `None` for verify/mirror.
    fn run_tsc(&mut self) -> Option<Tsc>;
    /// Measurement TSC (`decided_tsc`); never part of a decision.
    fn measure(&mut self) -> Tsc;
    /// Pushes one out-ring record (replay drivers skip).
    fn emit(&mut self, o: &OutRecord<'_>) -> EmitResult;
    /// The parsed large object of a blob (live: Mailbox; replay: parse the logged blob).
    fn take_parsed(&mut self, r: &BlobRef, cx: &ParseCx<'_, S>) -> Result<ParsedConfig<S>, ConfigErr>;
    /// Hands a replaced object back (live: reverse Mailbox; others drop it).
    fn retire(&mut self, old: ParsedConfig<S>);
    fn journal(&mut self) -> &mut Self::J;
    fn mode(&self) -> DriverMode;
    /// Writes the gw kill words (live only).
    fn write_kill(&mut self, bits: &[KillBits]);
    /// Segment header values for a roll.
    fn roll_info(&mut self) -> RollInfo;
    fn host_profile(&self) -> HostProfile;
    fn clock_status(&self) -> ClockStatus;
    /// Immutable diagnostic views around each Input; compiled out of production.
    #[cfg(feature = "testkit")]
    fn observe_input(&mut self, _e: &Engine, _input: &InputSlot, _before: bool) {}
    /// `boot(Journal)`: the logged records to start from (live: the old journal of this
    /// instance, already truncated by hft-journal `open`; verify/mirror: their own log).
    fn start_log(&mut self) -> Option<replay::Log>;
    /// Replay drivers: continue from record index `ix` (the first Input after the start
    /// snapshot). Live: nothing.
    fn start_at(&mut self, ix: usize) {
        let _ = ix;
    }
}

/// Read-only derived state the driver consults before and while taking Inputs.
#[derive(Copy, Clone)]
pub struct DriverView<'a> {
    pub(crate) e: &'a Engine,
}

impl core::fmt::Debug for DriverView<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DriverView").finish_non_exhaustive()
    }
}

/// The earliest unsettled funding time of a synthesizing venue with positions (README "资金费合成").
pub(crate) fn next_funding_settle_of(e: &Engine, v: VenueId) -> Option<i64> {
    if !crate::process::needs_synth(e, v) {
        return None;
    }
    let mut best: Option<i64> = None;
    for i in e.rd.insts_of_venue(v) {
        let Some(r) = e.market.row(*i) else { continue };
        if r.next_funding_ns == 0 || r.settled_cut_ns >= r.next_funding_ns {
            continue;
        }
        if !crate::subs::pos_nonzero(e, *i) {
            continue;
        }
        best = Some(best.map_or(r.next_funding_ns, |b| b.min(r.next_funding_ns)));
    }
    best
}

impl<'a> DriverView<'a> {
    pub fn new(e: &'a Engine) -> DriverView<'a> {
        DriverView { e }
    }
    /// The heap top deadline.
    pub fn next_deadline(&self) -> Option<Tsc> {
        self.e.heap.peek().map(|t| Tsc(t.deadline))
    }
    /// The heap top entry `(deadline, owner, timer_id, group)`.
    pub fn heap_top(&self) -> Option<crate::timers::TimerEntry> {
        self.e.heap.peek().copied()
    }
    /// OMS pending output or notification backlog / `naked_groups` cursor.
    pub fn wants_continue(&self) -> bool {
        self.e.oms.wants_continue() || self.e.notify.backlog_len() > 0 || self.e.naked_scan.is_some()
    }
    pub fn oms_wants_continue(&self) -> bool {
        self.e.oms.wants_continue()
    }
    pub fn notify_wants_continue(&self) -> bool {
        self.e.notify.backlog_len() > 0 || self.e.naked_scan.is_some()
    }
    /// Free backlog slots.
    pub fn notify_headroom(&self) -> usize {
        self.e.notify.headroom()
    }
    /// `U = max_cells_total + 128`.
    pub fn notify_unit(&self) -> usize {
        self.e.caps.notify_unit() as usize
    }
    pub fn snapshot_requested(&self) -> bool {
        self.e.snapshot_requested
    }
    /// `(venue, settle time)` of the earliest synthesis due.
    pub fn next_funding_settle(&self) -> Option<(VenueId, i64)> {
        let mut best: Option<(VenueId, i64)> = None;
        for v in 0..64u8 {
            if let Some(t) = next_funding_settle_of(self.e, VenueId(v))
                && best.is_none_or(|b| t < b.1)
            {
                best = Some((VenueId(v), t));
            }
        }
        best
    }
    pub fn draining(&self) -> bool {
        self.e.drain.active
    }
    pub fn refdata_recheck_due(&self) -> bool {
        self.e.drain.recheck_due
    }
    /// engine_ns of the last applied record (the next run's `EngineClock.base_engine_ns`).
    pub fn stop_engine_ns(&self) -> i64 {
        self.e.last_ens
    }
    pub fn engine_epoch(&self) -> u32 {
        self.e.epoch
    }
    pub fn clock_ok(&self) -> bool {
        self.e.clock_ok
    }
    pub fn last_day_rolled(&self) -> u32 {
        self.e.last_day_rolled
    }
    pub fn inputs_since_keyframe(&self) -> u64 {
        self.e.inputs_since_keyframe
    }
    pub fn last_keyframe_tsc(&self) -> u64 {
        self.e.last_keyframe_tsc
    }
    pub fn params(&self) -> &crate::params::SeqParamsRow {
        &self.e.sp
    }
    /// Insts with a Live L2 book.
    pub fn live_books(&self) -> Vec<hft_types::InstId> {
        crate::snapshot::live_books(self.e)
    }
    /// The engine clock has been set in this run.
    pub fn clock_set(&self) -> bool {
        self.e.clock.is_set() && self.e.boot_seq != 0
    }
    /// Next funding settle times of the subscribed venues (Roll avoidance window).
    pub fn funding_times(&self) -> Vec<i64> {
        let mut v = Vec::new();
        for (ix, r) in self.e.market.rows.iter().enumerate() {
            if ix != 0 && r.next_funding_ns != 0 && self.e.sent_union.get(ix).is_some_and(|u| *u != 0) {
                v.push(r.next_funding_ns);
            }
        }
        v
    }
    pub fn tsc_hz_milli(&self) -> u64 {
        self.e.seg.tsc_hz_milli
    }
    /// UTC of a TSC by the current segment calibration.
    pub fn utc_of(&self, t: Tsc) -> i64 {
        let a = self
            .e
            .clock
            .anchor(Tsc(self.e.seg.calib_tsc), self.e.seg.calib_realtime_ns);
        let a = hft_strategy_sdk::EngineClockAnchor {
            tsc_hz_milli: self.e.seg.tsc_hz_milli,
            ..a
        };
        a.utc_of(t)
    }
    pub fn engine(&self) -> &Engine {
        self.e
    }
}
