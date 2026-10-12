//! `hft-event`: the single Rust source of the record header, the `EventKind` registry, every
//! event body, the strategy payload containers (Advice, SPB1), control commands, ring element
//! types and variable-length record prefixes, with zero-copy codecs.
//!
//! Stateless; no I/O, no threads, no allocation on any codec path. Little-endian only.
//! Specification: `crates/hft-event/README.md`.
#![forbid(unsafe_code)]
#![deny(
    clippy::float_arithmetic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented,
    clippy::indexing_slicing
)]

#[cfg(target_endian = "big")]
compile_error!("hft-event: the wire format is little-endian only; big-endian targets are not supported");

// `event_body!` expands to `$crate::…` paths; make them resolve inside this crate too.
extern crate self as hft_event;

mod macros;

mod bytes;
mod chain;
mod cmd;
mod codec;
pub mod codes;
mod control;
#[cfg(feature = "dev-kinds")]
pub mod dev;
mod exec;
mod header;
#[doc(hidden)]
pub mod kinds;
mod market;
mod ring;
pub mod schema;
mod strategy;
mod system;
mod timer;
mod var;

pub use chain::{ChainHead, PoolQuote, SwapDecision, TxLifecycle, TxState};
pub use cmd::{BlobRef, CmdHdr, CmdKind, CmdScope, ControlCmd, STRATEGY_CMD_MAX_PAYLOAD, WARMUP_MAX_PAYLOAD};
pub use codec::{
    DecodeErr, EncodeErr, MAX_BODY, MAX_RECORD, RecordView, classify, decode_record, encode_fixed,
    encode_part, encode_var,
};
pub use control::{
    CapRecord, CmdPreview, CmdResult, RECONCILE_BATCH_MAX_ITEMS, ReconcileBatchHdr, ReconcileBatchView,
    ReconcileReport,
};
pub use exec::{
    AccountConfigReportHdr, BalanceReport, ExecDecision, ExternalCause, ExternalFill, FundingSettle, GwOp,
    LeverageItem, MarginItem, MarginReportHdr, OrderAction, OrderDecision, OrderEvent, OrderEventKind,
    OrderFlags, PositionReport, SendReport, SendStatus, TargetOriginWire,
};
pub use header::{RecordFlags, RecordHeader, record_len};
pub use kinds::{
    CrcPolicy, Domain, EventKind, JournalMask, KINDS, KindInfo, Layout, MEASURE_MASK, Role, kind_info,
};
pub use market::{Bbo, Funding, IndexPrice, L2BatchHdr, L2Delta, Liquidation, MarkPrice, MarketHdr, Trade};
pub use ring::{AdviceMsg, CtlRecord, CtlRecordHdr, FeedCmd, FeedMsg, GwCmd, HkMsg, OutMsg, RepMsg};
pub use schema::{EnumDesc, EnumItem, FieldDesc, FieldTy, IdKind};
pub use strategy::{
    ADVICE_MAX_PAYLOAD, AdviceHdr, AdviceRecord, GroupSpec, LegDef, ParamSource, ParamsErr, SpbView,
    StrategyParamsHdr, parse_spb1,
};
pub use system::{
    ClockStatus, ConnRole, ConnState, ConnStateKind, EngineClock, HostProfile, LatencySample, NOTE_NAMES,
    Note, NoteLevel, NoteName, NoteSource, SegmentRoll, Timer, VField, note_code, note_name,
};
pub use timer::{OmsTimer, SysTimer, TIMER_OWNER_STRATEGY_BASE, TimerCode, TimerErr};
pub use var::{
    BlobHdr, BlobKind, L2BatchView, L2SnapHdr, L2SnapshotPart, Level, PART_MAX_CHUNK, PartHdr, PartIter,
    RawHdr, Reassembler, SectionHdr, SnapshotHdr, multi_view,
};

/// A fixed body or Var/Multi prefix: `#[repr(C)]`, padding-free, size a multiple of 8.
pub trait Body:
    zerocopy::TryFromBytes + zerocopy::IntoBytes + zerocopy::KnownLayout + zerocopy::Immutable + Copy
{
    const KIND: EventKind;
    const SCHEMA_VERSION: u16;
}
