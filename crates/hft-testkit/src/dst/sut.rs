//! The boundary between the DST world and the system under test (README "6. DST 框架").
//!
//! The world owns virtual time, the actors and the fault plan; the engine behind [`Sut`] owns all
//! business state. The production adapter `SeqSut<S: StrategySet>` drives the real
//! `hft_sequencer::Sequencer<DstDriver, S>` single-threaded (README "DST 的世界模型" 2) and copies
//! the components' debug views into [`Obs`]; [`crate::dst::toy::ToyEngine`] is the framework's own
//! self-test engine. One `process` call is one DST step: one Input and all its Outputs.

use hft_event::{EventKind, GwCmd, RecordFlags};
use hft_types::{AccountId, VenueId};

use crate::dst::obs::{Obs, StepInfo, TruthRow};
use crate::dst::world::WorldSpec;
use crate::hash::ComponentHashes;

/// Where an Input came from (the actor that produced it).
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum Source {
    Market,
    /// `MockVenue` of this venue (Live accounts; after fault injection).
    Venue(VenueId),
    /// `SimActor` (Sim accounts; never faulted).
    Sim,
    Ctl,
    Recon,
    Hk,
    Advisor,
    /// Engine timers (the driver's own Inputs).
    Engine,
}

/// One Input as the engine journals it: kind and logical body bytes.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct InputRec {
    pub kind: EventKind,
    pub body: Vec<u8>,
    pub src: Source,
}

impl InputRec {
    /// The body bytes of a fixed hft-event body.
    pub fn of<B: hft_event::Body>(b: &B, src: Source) -> InputRec {
        InputRec {
            kind: B::KIND,
            body: zerocopy::IntoBytes::as_bytes(b).to_vec(),
            src,
        }
    }
    /// Reads the body back as `B` (kind must match).
    pub fn read<B: hft_event::Body + zerocopy::FromBytes>(&self) -> Option<B> {
        if self.kind != B::KIND {
            return None;
        }
        B::read_from_prefix(&self.body).ok().map(|(b, _)| b)
    }
}

/// One journal record of the in-memory segment.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct JRec {
    pub seq: u64,
    pub tsc: u64,
    pub kind: EventKind,
    pub flags: RecordFlags,
    pub body: Vec<u8>,
}

impl JRec {
    /// Reads a fixed body by value (`FromBytes` bodies; `TryFromBytes` ones via `try_read`).
    pub fn read<B: hft_event::Body + zerocopy::FromBytes>(&self) -> Option<B> {
        if self.kind != B::KIND {
            return None;
        }
        B::read_from_prefix(&self.body).ok().map(|(b, _)| b)
    }
    /// Reads a `TryFromBytes` body (enums checked).
    pub fn try_read<B: hft_event::Body>(&self) -> Option<B> {
        if self.kind != B::KIND {
            return None;
        }
        let n = size_of::<B>();
        let bytes = self.body.get(..n)?;
        B::try_read_from_bytes(bytes).ok()
    }
}

/// Out ring of a frame.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum OutRing {
    Venue(VenueId),
    Sim,
}

/// One frame the engine emitted (`Driver::emit`), with the kill word in effect after the Input
/// that produced it (INV-5).
#[derive(Copy, Clone, Debug)]
pub struct FrameRec {
    pub ring: OutRing,
    pub cmd: GwCmd,
    /// Seq of the Input that produced the frame.
    pub trigger_seq: u64,
}

impl PartialEq for FrameRec {
    fn eq(&self, o: &Self) -> bool {
        self.ring == o.ring
            && self.trigger_seq == o.trigger_seq
            && zerocopy::IntoBytes::as_bytes(&self.cmd) == zerocopy::IntoBytes::as_bytes(&o.cmd)
    }
}
impl Eq for FrameRec {}

/// Everything one step produced.
#[derive(Clone, Debug, Default)]
pub struct StepOut {
    /// Journal records appended by this step, Input first, in seq order.
    pub records: Vec<JRec>,
    pub frames: Vec<FrameRec>,
    pub info: StepInfo,
}

impl StepOut {
    pub fn clear(&mut self) {
        self.records.clear();
        self.frames.clear();
        self.info = StepInfo::default();
    }
}

/// Where a crash happens (README "DST 的世界模型" 3).
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum CrashPoint {
    /// Mid-Input of the sequencer (default).
    SeqInput,
    SimMatchedUndelivered,
    SimRepQueued,
    BeforeFirstReport,
}

/// Facts the engine needs at boot that come from the run, not the world.
#[derive(Clone, Debug)]
pub struct BootCtx<'a> {
    pub seed: u64,
    pub world: &'a WorldSpec,
    /// Planted-defect names this run asks for (framework self-tests; real engines map them to
    /// `planted-<name>` features and fail boot when the build lacks one).
    pub planted: &'a [&'static str],
    /// Virtual clock: TSC frequency and start (DstDriver).
    pub tsc_hz: u64,
}

/// Boot failure of the system under test.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct BootFail(pub String);

/// The system under test. Implementations are deterministic functions of their inputs: no clock,
/// no RNG, no environment (README "DST 的世界模型" 5).
pub trait Sut: Sized {
    /// The matching core of the world's venues (`hft_sim::SimExchange` for engine runs).
    type Matcher: crate::dst::venue_behavior::Matcher;
    /// Builds the engine for `ctx.world` (accounts, strategies, parameters, risk).
    fn boot(ctx: &BootCtx<'_>) -> Result<Self, BootFail>;
    /// One step: journal `input` at `tsc`, process it with all its Outputs, emit frames.
    fn process(&mut self, now_ns: u64, tsc: u64, input: &InputRec, out: &mut StepOut);
    /// Virtual time (ns) of the earliest pending engine timer.
    fn next_deadline(&self) -> Option<u64>;
    /// One step of due engine timers at `now_ns` (the driver's Timer Input); `false` = none due.
    fn fire_timers(&mut self, now_ns: u64, tsc: u64, out: &mut StepOut) -> bool;
    /// Copies the debug views (read-only; never changes state).
    fn observe(&self, obs: &mut Obs);
    /// Component hashes of the current state (snapshot sections, README "7. 状态哈希对比").
    fn hashes(&self) -> ComponentHashes;
    /// INV-6: hashes rebuilt from the latest snapshot plus the following Inputs of the journal
    /// (verification replay); `None` = no snapshot yet.
    fn rebuilt_hashes(&self) -> Option<ComponentHashes>;
    /// Journal seq of the last record.
    fn seq(&self) -> u64;
    /// The in-memory journal (all records so far).
    fn journal(&self) -> &[JRec];
    /// `CrashFault`: drop the in-memory engine mid-Input (`input` half processed), keep the
    /// journal, rebuild with `RecoverDriver`. `Err` = the restart refused to boot.
    fn crash_and_recover(&mut self, point: CrashPoint, input: Option<&InputRec>) -> Result<(), BootFail>;
    /// `CleanRestartFault`: Drain, then restart from the Drain snapshot.
    fn clean_restart(&mut self) -> Result<(), BootFail>;
    /// Accounts whose frames go to `out-sim` (Sim accounts).
    fn is_sim(&self, a: AccountId) -> bool;
}

/// The exchange-truth side the world exposes to INV-16.
pub trait TruthSource {
    fn truth(&self, out: &mut Vec<TruthRow>);
}
