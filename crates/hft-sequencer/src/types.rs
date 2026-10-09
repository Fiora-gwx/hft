//! Public value types of the sequencer API (README "装配与运行").

use hft_event::{Body, EventKind, FeedCmd, GwCmd, RecordFlags};
use hft_types::{AccountId, Tsc, VenueId};

/// Result of one `step`.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum StepOutcome {
    /// At least one Input was processed this round.
    Busy,
    /// No Input this round.
    Idle,
    /// Drain finished; exit codes and restart belong to apps/hft-core.
    Done {
        restart: bool,
        cause: DoneCause,
    },
    Fatal(FatalReason),
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum DoneCause {
    Drain,
    ReplaceRefdata,
    StorageCritical,
    IdentityMismatch,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum FatalReason {
    JournalFull,
    ClockRange,
    TimerOverflow,
}

/// Which capacity a restored state exceeds (checked in this order).
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum CapKind {
    Insts,
    Cells,
    Instances,
    GroupsPerInstance,
    Timers,
}

/// Journal-level boot failures.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum JournalErr {
    /// Segment chain broken or unreadable.
    Chain,
    /// No usable snapshot.
    NoSnapshot,
    /// A record could not be decoded.
    Corrupt { seq: u64 },
}

/// Configuration failures (boot and `take_parsed`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum ConfigErr {
    /// A boot-block / logged `ConfigBlob` does not decode (`LoadErrCode` value, 0 = other).
    Blob { kind: u16, code: u16 },
    /// The referenced blob is not available (Mailbox empty or sha mismatch).
    Missing,
    /// Component construction failed.
    Component(&'static str),
    /// Strategy parameter decode (CmdResult detail of the failure).
    Params(u32),
}

/// `boot` failures (exit codes belong to apps/hft-core).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum BootError {
    VersionNeedsDrain,
    SnapshotUndecodable { section: u16 },
    CapacityShrink { what: CapKind },
    TimerCapacity { demand: u32 },
    Journal(JournalErr),
    Config(ConfigErr),
}

/// Snapshot choice of a recovery.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum SnapshotSel {
    Latest,
    DrainTail,
    AtSeq(u64),
    NewestReadable,
}

/// Recovery flags handed to hft-risk (`RecoveryFlags` bits).
pub use hft_risk::mode::RecoveryFlags;

/// `xxh3_64(kind ‖ body)` of an Input (poison detection).
pub type InputHash = u64;

/// Cold-start recovery plan (built by apps/hft-core).
#[derive(Clone, Debug)]
pub struct RecoverPlan {
    pub start: SnapshotSel,
    /// false = do not replay the tail (`TAIL_LOST`).
    pub replay_tail: bool,
    /// The persisted `engine_epoch` from hft-core's `DurableMeta`.
    pub epoch: u32,
    pub flags: RecoveryFlags,
    /// Run-time poison inputs (≤ 4).
    pub quarantine: Vec<InputHash>,
}

/// Which ring an out record goes to.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum OutRing {
    Venue(VenueId),
    Sim,
}

/// One message the sequencer pushes after the event is published.
#[derive(Copy, Clone, Debug)]
pub enum OutRecord<'a> {
    Out(OutRing, &'a GwCmd),
    FeedCtl(VenueId, &'a FeedCmd),
}

/// Result of `Driver::emit`.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum EmitResult {
    Pushed,
    /// The ring is full (orders: a synthetic `SendReport{RingFull}` follows as the next Input).
    Full,
    /// Not emitted (replay drivers).
    Skipped,
}

/// One Input handed over by the driver.
#[derive(Clone, Debug)]
pub struct InputSlot {
    pub kind: EventKind,
    pub flags: RecordFlags,
    /// Record-header tsc (live: the take time; replay: the logged value).
    pub tsc: Tsc,
    /// Logged seq (replay drivers; live = 0).
    pub seq: u64,
    buf: Vec<u8>,
}

impl InputSlot {
    /// A slot with `cap` bytes preallocated.
    pub fn with_capacity(cap: usize) -> InputSlot {
        InputSlot {
            kind: EventKind(0),
            flags: RecordFlags(0),
            tsc: Tsc(0),
            seq: 0,
            buf: Vec::with_capacity(cap),
        }
    }
    /// Sets a fixed-layout body.
    pub fn set<B: Body>(&mut self, flags: RecordFlags, tsc: Tsc, b: &B) {
        self.set_bytes(B::KIND, flags, tsc, b.as_bytes());
    }
    /// Sets a body from bytes (logical body for Var/Multi kinds).
    pub fn set_bytes(&mut self, kind: EventKind, flags: RecordFlags, tsc: Tsc, body: &[u8]) {
        self.kind = kind;
        self.flags = flags;
        self.tsc = tsc;
        self.seq = 0;
        self.buf.clear();
        self.buf.extend_from_slice(body);
    }
    /// Sets a body from pieces.
    pub fn set_pieces(&mut self, kind: EventKind, flags: RecordFlags, tsc: Tsc, pieces: &[&[u8]]) {
        self.kind = kind;
        self.flags = flags;
        self.tsc = tsc;
        self.seq = 0;
        self.buf.clear();
        for p in pieces {
            self.buf.extend_from_slice(p);
        }
    }
    pub fn body(&self) -> &[u8] {
        &self.buf
    }
    /// The body as `B` (zero-extended when shorter; `None` on illegal enums).
    pub fn read<B: Body>(&self) -> Option<B> {
        read_body::<B>(&self.buf)
    }
}

/// A body as `B` from a byte slice, zero-extended when shorter.
pub fn read_body<B: Body>(body: &[u8]) -> Option<B> {
    let size = core::mem::size_of::<B>();
    if let Some(b) = body.get(..size) {
        return B::try_read_from_bytes(b).ok();
    }
    let mut buf = [0u8; 1024];
    let dst = buf.get_mut(..size)?;
    let n = body.len().min(size);
    dst.get_mut(..n)?.copy_from_slice(body.get(..n)?);
    B::try_read_from_bytes(dst).ok()
}

/// Per-account identity of a gw kill word (index into the driver's `KillWord` table).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct GwKey {
    pub venue: VenueId,
    pub account: AccountId,
}

/// A plain (non-`Body`) row from bytes, zero-extended when shorter.
pub fn read_body_plain<T: zerocopy::FromBytes + zerocopy::KnownLayout + zerocopy::Immutable>(
    body: &[u8],
) -> Option<T> {
    let size = core::mem::size_of::<T>();
    if let Some(b) = body.get(..size) {
        return T::read_from_bytes(b).ok();
    }
    let mut buf = [0u8; 1024];
    let dst = buf.get_mut(..size)?;
    let n = body.len().min(size);
    dst.get_mut(..n)?.copy_from_slice(body.get(..n)?);
    T::read_from_bytes(dst).ok()
}
