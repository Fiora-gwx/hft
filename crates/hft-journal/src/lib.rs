//! `hft-journal`: the journal file format of `docs/subsystems/journal-format.md` — RAM hot-segment
//! writer (`writer` feature), segment preparation, persist to cold copies, retirement, recovery,
//! readers, retention, verification and `DurableMeta`.
//!
//! Hot paths (`append*`, `publish`, `end_event`) make no allocation and no system call. Linux-only
//! facilities (hugetlbfs/tmpfs mappings, `statfs`, `fallocate`, ACL-friendly directory modes) compile
//! everywhere; off Linux they return `io::ErrorKind::Unsupported` (surfacing as `OpenErr::Io` /
//! `OpenErr::DiskBacked`). The heap segment backend (feature `mem-backend`) runs the same logic on
//! any platform for tests. `unsafe` lives only in `layout.rs`.
//! Specification: `crates/hft-journal/README.md`.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented,
    clippy::undocumented_unsafe_blocks
)]
#![cfg_attr(not(feature = "writer"), allow(dead_code))]

mod config;
mod fs;
mod held;
mod layout;
mod meta;
mod oplog;
mod reader;
mod retire;
mod scan;
mod verify;

#[cfg(feature = "writer")]
mod persist;
#[cfg(feature = "writer")]
mod prepare;
#[cfg(feature = "writer")]
mod recover;
#[cfg(feature = "writer")]
mod roll;
#[cfg(feature = "writer")]
mod writer;

#[cfg(feature = "mem-backend")]
pub mod testing;

pub use config::*;
pub use held::held_by_instance;
pub use hft_rt::PageSize;
pub use layout::{
    FORMAT_VERSION, HEADER_LEN, RECOVERY_GAP, SEAL_MAGIC, SEGMENT_MAGIC, SchemaEntry, SealRecord,
    SegmentHeader, SegmentHeaderFixed,
};
pub use meta::DurableMeta;
pub use reader::{EndAt, JournalReader, LogicalEvent, Next, OnGap, ReaderConfig, StartAt};
pub use retire::cold_retire;
pub use verify::verify;

#[cfg(feature = "writer")]
pub use persist::Persister;
#[cfg(feature = "writer")]
pub use prepare::{PrepEnv, SegmentPreparer};
#[cfg(feature = "writer")]
pub use recover::RecoveryReport;
#[cfg(feature = "writer")]
pub use retire::HotRetirer;
#[cfg(feature = "writer")]
pub use roll::{BootWriter, RollInfo};
#[cfg(feature = "writer")]
pub use writer::{JournalWriter, PreparedSegment, RetiredSegment, VarSlot, WriterEnv};
