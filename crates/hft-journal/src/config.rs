//! Names, configuration, code tables and error types shared by writer, reader and tools.

use std::path::PathBuf;
use std::str::FromStr;

use hft_rt::PageSize;
use hft_types::VenueId;

/// Default soft roll threshold (README "配置项").
pub const DEFAULT_SIZE_IDLE_PCT: u8 = 75;
/// Default hard roll threshold.
pub const DEFAULT_SIZE_VALVE_PCT: u8 = 90;
/// Grace between setting `retired = 1` and `munmap` + `unlink`.
pub const RETIRE_GRACE_MS: u64 = 2000;
/// Default reader reassembly limit (256 MiB).
pub const DEFAULT_REASSEMBLY_CAP: usize = 256 << 20;
/// Default minimum interval between re-reads of `committed_len` after `Pending`.
pub const DEFAULT_TAIL_POLL_US: u32 = 50;

/// Journal name: `"seq"`, `"raw-feed-binance-usdm"`, `"raw-gw-okx-swap"`, `"chain-bsc"`, …
/// (ASCII `[a-z0-9-]`, 1–32 bytes; checked by `JournalWriter::open`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct JournalName(pub String);

impl JournalName {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 1–32 bytes of `[a-z0-9-]`.
    pub fn is_valid(&self) -> bool {
        let b = self.0.as_bytes();
        !b.is_empty()
            && b.len() <= 32
            && b.iter()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
    }

    /// Right-padded to 32 bytes (header `journal_name`).
    pub fn to_header(&self) -> [u8; 32] {
        let mut o = [0u8; 32];
        for (d, s) in o.iter_mut().zip(self.0.as_bytes()) {
            *d = *s;
        }
        o
    }
}

impl From<&str> for JournalName {
    fn from(s: &str) -> JournalName {
        JournalName(s.to_string())
    }
}

/// hft-core instance name: `Prod` = "prod", `Sim` = "sim"; no other value exists.
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum InstanceName {
    Prod = 1,
    Sim = 2,
}

impl InstanceName {
    pub const fn as_str(self) -> &'static str {
        match self {
            InstanceName::Prod => "prod",
            InstanceName::Sim => "sim",
        }
    }

    /// Right-padded to 16 bytes (header `instance`).
    pub fn to_header(self) -> [u8; 16] {
        let mut o = [0u8; 16];
        for (d, s) in o.iter_mut().zip(self.as_str().as_bytes()) {
            *d = *s;
        }
        o
    }
}

/// Error of `InstanceName::from_str` (only `"prod"` and `"sim"` parse).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct BadInstanceName;

impl FromStr for InstanceName {
    type Err = BadInstanceName;
    fn from_str(s: &str) -> Result<InstanceName, BadInstanceName> {
        match s {
            "prod" => Ok(InstanceName::Prod),
            "sim" => Ok(InstanceName::Sim),
            _ => Err(BadInstanceName),
        }
    }
}

/// `SegmentHeader.journal_kind`.
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum JournalKind {
    Raw = 1,
    Seq = 2,
    Chain = 3,
}

impl JournalKind {
    pub fn from_u8(v: u8) -> Option<JournalKind> {
        match v {
            1 => Some(JournalKind::Raw),
            2 => Some(JournalKind::Seq),
            3 => Some(JournalKind::Chain),
            _ => None,
        }
    }

    pub(crate) fn mask(self) -> hft_event::JournalMask {
        match self {
            JournalKind::Raw => hft_event::JournalMask::RAW,
            JournalKind::Seq => hft_event::JournalMask::SEQ,
            JournalKind::Chain => hft_event::JournalMask::CHAIN,
        }
    }

    /// `seq`/`chain` segments carry a boot block ending in `BootDone`.
    pub(crate) fn has_boot(self) -> bool {
        self != JournalKind::Raw
    }
}

/// `SegmentRoll.reason` and `SegmentHeader.roll_reason` (hft-event code table `RollReason`).
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum RollReason {
    Hourly = 1,
    Size = 2,
    Startup = 3,
    Recovery = 4,
    Manual = 5,
    RecoverFromSnapshot = 6,
}

impl RollReason {
    pub fn from_u8(v: u8) -> Option<RollReason> {
        Some(match v {
            1 => RollReason::Hourly,
            2 => RollReason::Size,
            3 => RollReason::Startup,
            4 => RollReason::Recovery,
            5 => RollReason::Manual,
            6 => RollReason::RecoverFromSnapshot,
            _ => return None,
        })
    }
}

/// `SegmentRoll.mark`.
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum RollMark {
    End = 1,
    Begin = 2,
    BootDone = 3,
}

/// `SegmentHeader.state`.
#[repr(u32)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum SegState {
    Preparing = 0,
    Open = 1,
    Sealed = 2,
}

/// Errors of `JournalWriter::open`, `open_offline` and `JournalReader::open`.
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum OpenErr {
    DiskBacked = 1,
    OfflineTarget = 2,
    BadConfig = 3,
    Io = 4,
    ChainBroken = 5,
    ColdHotConflict = 6,
    SeqUnavailable = 7,
    NotFound = 8,
}

/// Errors of the append entries.
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum AppendErr {
    Full = 1,
    TooLarge = 2,
    Sealed = 3,
    InBoot = 4,
}

/// Errors of `roll` / `seal_and_close`.
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum RollErr {
    NextNotReady = 1,
    Io = 2,
    InBoot = 3,
}

/// When the caller should roll (`needs_roll`).
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum RollAt {
    IdleRound = 1,
    EventBoundary = 2,
}

/// Reader errors.
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ReadErr {
    Corrupt = 1,
    BadCrc = 2,
    ReassemblyCap = 3,
    DevKind = 4,
}

/// Why the reader reports a gap.
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum GapReason {
    MissingSegment = 1,
    RecoveryGap = 2,
    ChainBroken = 3,
    Deleted = 4,
}

/// `SegmentPreparer::poll` result.
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum PrepState {
    Ready = 1,
    Prepared = 2,
    Failed = 3,
}

/// A place in a journal: `offset` is the byte offset after the record whose seq is `seq`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct Position {
    pub segment_no: u64,
    pub offset: u64,
    pub seq: u64,
}

/// A gap seen by the reader.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct GapInfo {
    /// Last delivered position.
    pub at: Position,
    pub expected_seq: u64,
    /// 0 = nothing to resume from.
    pub resume_segment_no: u64,
    pub reason: GapReason,
}

/// Writer configuration (README "写者"; values per host class from apps/hft-core).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JournalConfig {
    pub name: JournalName,
    pub kind: JournalKind,
    /// Raw journals only; otherwise `VenueId(0)`.
    pub venue: VenueId,
    pub instance: InstanceName,
    pub hot_dir: PathBuf,
    pub cold_root: PathBuf,
    pub segment_bytes: u64,
    pub page: PageSize,
    pub size_idle_pct: u8,
    pub size_valve_pct: u8,
    pub hot_keep: u8,
}

impl JournalConfig {
    /// README constraints: name and pct ranges, `segment_bytes` a page multiple and > 4 096.
    pub fn validate(&self) -> bool {
        let pg = self.page.bytes() as u64;
        self.name.is_valid()
            && self.size_idle_pct > 0
            && self.size_idle_pct < self.size_valve_pct
            && self.size_valve_pct < 100
            && self.segment_bytes > 4096
            && self.segment_bytes.is_multiple_of(pg)
            && usize::try_from(self.segment_bytes).is_ok()
    }

    /// `<hot_dir>/<instance>/<journal>/`.
    pub fn hot_journal_dir(&self) -> PathBuf {
        self.hot_dir.join(self.instance.as_str()).join(self.name.as_str())
    }

    /// `<cold_root>/<instance>/<journal>/`.
    pub fn cold_journal_dir(&self) -> PathBuf {
        self.cold_root
            .join(self.instance.as_str())
            .join(self.name.as_str())
    }
}

/// Persist thread parameters (README "配置项").
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct PersistConfig {
    pub chunk_bytes: u32,
    pub sync_bytes: u32,
    pub sync_interval_ms: u32,
}

impl Default for PersistConfig {
    fn default() -> PersistConfig {
        PersistConfig {
            chunk_bytes: 4 << 20,
            sync_bytes: 1 << 20,
            sync_interval_ms: 20,
        }
    }
}

/// Local retention of cold copies.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ColdRetention {
    pub keep_days: u16,
    pub require_uploaded: bool,
}

impl ColdRetention {
    /// Seq/Chain 7 days, Raw 3 days; `require_uploaded = true`.
    pub fn for_kind(k: JournalKind) -> ColdRetention {
        ColdRetention {
            keep_days: if k == JournalKind::Raw { 3 } else { 7 },
            require_uploaded: true,
        }
    }
}

/// What `cold_retire` removed.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub struct RetireReport {
    pub segments: u32,
    pub bytes: u64,
}

/// Unsynced bytes of one journal (`Persister::lag`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JournalLag {
    pub name: JournalName,
    pub bytes: u64,
    /// Estimated age of the oldest unsynced byte.
    pub ms: u64,
}

/// One `Persister::poll` round.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub struct PersistProgress {
    pub bytes_copied: u64,
    pub syncs: u32,
    pub sealed: u32,
    /// Failed copy/sync/seal operations this round (e.g. `ENOSPC`); retried next round.
    pub errors: u32,
}

/// `HotRetirer::audit`: hugetlbfs pages still mapped after their file was unlinked.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub struct HugeAudit {
    pub pages_2m: u64,
    pub pages_1g: u64,
}

/// `verify` options.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub struct VerifyOpts {
    /// Reject `0x7Fxx` kinds even when this binary knows them.
    pub production: bool,
}

/// One verification finding (`item` = journal-format "校验与缺段检测" items 1–5).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct VerifyIssue {
    pub segment_no: u64,
    pub offset: u64,
    pub item: u8,
}

/// `verify` result.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct VerifyReport {
    pub issues: Vec<VerifyIssue>,
    /// Missing `segment_no` ranges (inclusive).
    pub missing: Vec<(u64, u64)>,
    pub recovery_gaps: Vec<u64>,
    pub unsealed: Vec<u64>,
}
