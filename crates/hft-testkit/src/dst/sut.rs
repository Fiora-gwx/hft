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

/// Inputs whose completed ledger sections must be compared without waiting for a snapshot.
pub const fn ledger_input(kind: EventKind) -> bool {
    matches!(
        kind,
        EventKind::ORDER_EVENT | EventKind::EXTERNAL_FILL | EventKind::RECONCILE_BATCH
    )
}

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
    /// Inclusive last physical seq; equals seq for the default logical-numbered journal.
    pub end_seq: u64,
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
    /// Actual Input boundaries, including internal timers and restart prologues.
    pub events: Vec<EventStep>,
    /// One live-state observation per journal snapshot, at its completed event / boot boundary.
    pub snapshots: Vec<SnapshotPoint>,
    pub snapshot_error: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotPoint {
    pub snapshot_seq: u64,
    /// Last record of the complete boundary, before any later Input can change state.
    pub through_seq: u64,
    pub live: ComponentHashes,
    /// Full read-only views at the same boundary, independent of snapshot serialization.
    #[cfg(feature = "seq")]
    pub ledger: Option<super::seq::ledger::Readings>,
}

#[derive(Clone, Debug)]
pub struct EventStep {
    pub records: Vec<JRec>,
    pub frames: Vec<FrameRec>,
    pub info: StepInfo,
    pub before: Obs,
    pub after: Obs,
    /// Exact hot ledger sections at this Input's end (external fills and receipt batches).
    pub ledger: Option<LedgerEvent>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LedgerEvent {
    pub through_seq: u64,
    pub sections: Vec<u8>,
}

impl StepOut {
    /// Every new journal snapshot needs one ordered observation at a complete boundary.
    pub fn verify_snapshot_observations(&self, journal: &[JRec], checked_through: u64) -> Result<(), String> {
        if let Some(e) = &self.snapshot_error {
            return Err(e.clone());
        }
        let tail = &journal[journal.partition_point(|r| r.seq <= checked_through)..];
        let mut expected = tail.iter().filter(|r| r.kind == EventKind::STATE_SNAPSHOT);
        for p in &self.snapshots {
            let Some(r) = expected.next() else {
                return Err(format!("extra snapshot observation {}", p.snapshot_seq));
            };
            if r.seq != p.snapshot_seq {
                return Err(format!(
                    "missing or reordered snapshot observation {}, got {}",
                    r.seq, p.snapshot_seq
                ));
            }
            let end = tail
                .binary_search_by_key(&p.through_seq, |r| r.end_seq)
                .ok()
                .map(|i| &tail[i]);
            if p.through_seq < r.end_seq || end.is_none_or(|r| r.flags.0 & RecordFlags::EVENT_END.0 == 0) {
                return Err(format!(
                    "snapshot {} has invalid boundary {}",
                    p.snapshot_seq, p.through_seq
                ));
            }
            if tail
                .iter()
                .skip_while(|r| r.seq <= p.snapshot_seq)
                .take_while(|r| r.seq <= p.through_seq)
                .any(|r| hft_event::classify(r.kind, &r.body) == Ok(hft_event::Role::Input))
            {
                return Err(format!(
                    "snapshot {} observation crosses a later Input",
                    p.snapshot_seq
                ));
            }
        }
        if let Some(r) = expected.next() {
            return Err(format!("missing snapshot observation {}", r.seq));
        }
        Ok(())
    }
    /// All journaled Inputs must have exactly one complete observation in journal order.
    pub fn verify_observations(&self) -> Result<(), String> {
        let mut seqs = std::collections::BTreeSet::new();
        let mut previous = 0;
        for e in &self.events {
            let Some(first) = e.records.first() else {
                return Err("empty Input observation".into());
            };
            if first.seq <= previous || !seqs.insert(first.seq) {
                return Err(format!("duplicate or reordered Input observation {}", first.seq));
            }
            if hft_event::classify(first.kind, &first.body) != Ok(hft_event::Role::Input) {
                return Err(format!("observation {} does not begin with an Input", first.seq));
            }
            previous = first.seq;
        }
        for r in &self.records {
            if hft_event::classify(r.kind, &r.body) == Ok(hft_event::Role::Input) && !seqs.contains(&r.seq) {
                return Err(format!("missing Input observation at seq {}", r.seq));
            }
        }
        Ok(())
    }
    pub fn clear(&mut self) {
        self.records.clear();
        self.frames.clear();
        self.info = StepInfo::default();
        self.events.clear();
        self.snapshots.clear();
        self.snapshot_error = None;
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
    /// Planted-defect names this run asks for (framework self-tests; the real engine maps them to
    /// the engine crates' `planted` switches and fails boot when the build lacks one).
    pub planted: &'a [&'static str],
    /// Virtual clock: TSC frequency and start (DstDriver).
    pub tsc_hz: u64,
}

/// Boot failure of the system under test.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct BootFail(pub String);

/// Complete committed journal verification, separate from latest-snapshot state rebuilding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JournalVerification {
    pub through_seq: u64,
    pub records: u64,
    pub segments: u64,
    pub runs: u64,
    pub inputs: u64,
    pub outputs: u64,
    pub snapshots: u64,
}

/// Planted defects switched on for one run; dropping it switches them back off.
pub struct PlantScope(Option<Box<dyn FnOnce()>>);

impl PlantScope {
    /// Nothing switched (engines whose planted defects are per instance, like `ToyEngine`).
    pub fn none() -> PlantScope {
        PlantScope(None)
    }
    /// Runs `undo` when the scope ends.
    pub fn new(undo: impl FnOnce() + 'static) -> PlantScope {
        PlantScope(Some(Box::new(undo)))
    }
}

impl core::fmt::Debug for PlantScope {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("PlantScope").field(&self.0.is_some()).finish()
    }
}

impl Drop for PlantScope {
    fn drop(&mut self) {
        if let Some(undo) = self.0.take() {
            undo();
        }
    }
}

/// The system under test. Implementations are deterministic functions of their inputs: no clock,
/// no RNG, no environment (README "DST 的世界模型" 5).
pub trait Sut: Sized {
    /// Switches the planted defects of one run on for the calling thread, before boot and until
    /// the run (with its rebuilds and cold verifiers) ends. The real engine's defects live in the
    /// engine crates (feature `planted`) and act on every engine instance of the thread, like one
    /// buggy binary; an unknown name fails the run's boot.
    fn plant(_planted: &[&'static str]) -> Result<PlantScope, BootFail> {
        Ok(PlantScope::none())
    }
    /// Read-only control version used by the test controller's explicit confirmed commands.
    fn control_state_version(&self) -> Result<u64, String> {
        Err("control state version observation is unavailable".into())
    }
    /// Whether each complete receipt identity was actually booked in the committed journal.
    /// The real adapter uses an independent cold consumer, never an Input's mere presence.
    fn committed_receipts(&self, _fills: &[hft_event::ExternalFill]) -> Result<Vec<bool>, String> {
        Err("committed receipt observation is unavailable".into())
    }

    /// Compare captured hot ledger bytes with an independent journal-only cold ledger at the
    /// same event boundary, including the independently generated auto-held offset Notes.
    fn verify_ledger_event(&self, _event: &EventStep) -> Result<(), String> {
        Err("ledger event observation is unavailable".into())
    }

    /// Real adapters provide a committed reconciliation view even with checkers disabled.
    /// `None` is reserved for the framework toy; real observations cannot take that fallback.
    fn reconcile_inputs(
        &self,
        _actor: &mut crate::dst::actors::ReconActor,
        _truth: &[TruthRow],
        _history: &[crate::dst::venue_behavior::ExternalReceipt],
    ) -> Result<Option<Vec<InputRec>>, String> {
        Ok(None)
    }
    /// Drain read-only observations made by internal Inputs or automatic recovery.
    fn drain_observations(&mut self, _out: &mut StepOut) {}
    /// Real Model outputs, including Advisor Meta Notes. Requested real mode must not fall
    /// back to the synthetic actor when an adapter returns `None`.
    fn advisor_inputs(&mut self) -> Result<Option<Vec<InputRec>>, String> {
        Ok(None)
    }
    /// Optional committed physical-numbered source for the app's offline file writer.
    #[cfg(feature = "seq")]
    fn physical_journal(&self) -> Result<Option<hft_sequencer::driver::replay::Log>, String> {
        Ok(None)
    }
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
    /// (verification replay); `Ok(None)` = unavailable. Errors retain the failing stage / seq.
    /// The runner rejects unavailable rebuilds
    /// for real-engine observations or any journal that already contains a snapshot.
    fn rebuilt_hashes(&self) -> Result<Option<ComponentHashes>, String>;
    /// Verify every retained committed run from the first boot, with no comparison window.
    /// `None` is only available to the framework toy; real adapters must provide evidence.
    fn verify_journal(&self) -> Result<Option<JournalVerification>, String> {
        Ok(None)
    }
    /// Independently restores this snapshot and verifies subsequent Outputs up to its boundary.
    /// Real adapters must implement this; the default cannot silently skip an observation.
    fn rebuilt_snapshot(&self, point: &SnapshotPoint) -> Result<ComponentHashes, String> {
        Err(format!("snapshot {} rebuild unavailable", point.snapshot_seq))
    }
    /// Inclusive final physical journal seq (logical seq in the default in-memory mode).
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
