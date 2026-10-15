//! `hft-rt`: runtime primitives shared by hot and cold processes — SPSC rings (in-process and
//! shared memory), the variable-length byte ring, ownership hand-off (`Mailbox`), single-writer
//! value cells (`SeqCell`), the kill word, TSC reading and calibration, CPU pinning and host
//! checks, huge-page regions, heartbeats and the task loop with its idle policy.
//!
//! The library never spawns threads and never decides which thread runs on which core; that is
//! assembled in `apps/hft-core`. Every `unsafe` block carries a `// SAFETY:` comment; every
//! atomic ordering names the operation it pairs with.
//!
//! Linux-only facilities (mmap, futex, epoll, sched_setaffinity, mlockall, PHC) compile on every
//! platform; off Linux they return `RtErr::Os(ENOSYS)` / `ShmErr::Io` / `ClockErr::Os(ENOSYS)`.
//! Specification: `crates/hft-rt/README.md`.
// Clippy 1.97 does not honor a local allowance on thread_local! expansions.
// hot-check enforces the macro ban here, except measurement-only latency TLS.
#![allow(clippy::disallowed_macros)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented,
    clippy::undocumented_unsafe_blocks
)]

mod calib;
mod config;
mod cpu;
mod hb_block;
mod host;
#[cfg(target_arch = "aarch64")]
mod host_aarch64;
#[cfg(not(target_arch = "aarch64"))]
mod host_x86;
mod idle;
pub mod latency_trace;
mod mailbox;
mod mem;
mod ring;
#[cfg(not(loom))]
mod swap;
mod sync;
mod sys;
mod tsc;
mod wait;

pub use calib::{ClockVerdict, DriftTracker, Phc, PhcOffset, SysClock, sample_pair, sample_pair_with};
pub use config::{CalibConfig, HostMode, RtConfig};
pub use cpu::{CpuSet, name_current_thread, pin_current_thread};
pub use hb_block::{HeartbeatBlock, HkLine};
pub use hft_types::{HostClass, Tsc};
pub use host::{
    HeldBySelf, HostCheck, HostNeeds, HostReport, Isa, SysRoot, check_host, required_cpu_features,
};
#[doc(hidden)]
pub use idle::ParkState;
pub use idle::{IdleAction, IdlePolicy, ParkBackend, Parker, PollTask, Polled, TaskLoop, Unparker, WaitFds};
pub use mailbox::{KillWord, MailReceiver, MailSender, Mailbox, SeqCell};
pub use mem::{Arena, HugeRegion, OpenMode, PageSize, lock_all};
pub use ring::bytes::{ByteConsumer, ByteProducer, byte_spsc};
#[cfg(not(loom))]
pub use ring::shm::{
    PeerLine, PeerStatus, RingSide, ShmConsumer, ShmEnd, ShmErr, ShmProducer, ShmRingHdr, ShmSpsc,
};
pub use ring::spsc::{Backing, Consumer, Producer, ReadBatch, WriteBatch, spsc};
#[cfg(not(loom))]
pub use swap::{EpochSwap, SwapPublisher, SwapReader, SwapReclaimer};
pub use tsc::{CalibPair, ClockErr, ClockReads, TscClock, TscUtc, tsc_hz_nominal, tsc_now, tsc_ordered};
pub use wait::{Backoff, CachePadded, HbKind, Heartbeat, HeartbeatWatch, Progress, cpu_relax};

/// The one error type of this crate's startup and warm entries (hot entries never return it).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RtErr {
    /// Ring capacity breaks its rule (power of two, minimum size).
    Capacity,
    /// Allocation, mmap or huge pages short.
    NoMem,
    /// `Arena` has too little space left.
    ArenaFull,
    /// `sched_getcpu` after pinning disagrees.
    PinMismatch { want: u16, got: u16 },
    /// Shared block (`HeartbeatBlock`) magic mismatch.
    BadMagic,
    /// Shared block layout version mismatch.
    BadVersion(u32),
    /// Any other failed system call (errno).
    Os(i32),
}
