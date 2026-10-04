//! `hft-strategy-sdk`: the only stable interface between strategies and the engine.
//!
//! M1 part: the parameter schema and its single codec (`params`: SPB1 body layout, encode/decode,
//! text parsing and formatting, canonical schema JSON and digest, `minor_change_check`, strategy
//! command payloads), the strategy descriptors (`StrategyDesc`, `NoteDesc`, `ReportFieldDesc`,
//! `CmdDesc`), the checkpoint layout description and the snapshot read/write types shared by the
//! engine L1 components. The strategy traits, views, `Ctx` and assembly arrive in M2.
//!
//! No I/O, no threads, no clock; nothing panics on any input. Specification:
//! `crates/hft-strategy-sdk/README.md`.
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
compile_error!("hft-strategy-sdk: the parameter body and snapshot encodings are little-endian only");

pub mod desc;
pub mod layout;
pub mod params;
pub mod snapshot;

pub use desc::{CmdDesc, NoteDesc, ReportAgg, ReportFieldDesc, ReportScope};
pub use layout::{ElemDesc, ElemTy, LenRule, ModelLayoutDesc, SectionDesc, SectionScope};
pub use params::{FieldDesc, MinorChange, ParamError, ParamSchemaDesc, Relax, Unit};
pub use snapshot::{RestoreError, SectionView, SnapshotFull, SnapshotReader, SnapshotWriter};

/// L0 types the strategy-facing API uses (the full `prelude` arrives with the M2 API).
pub use hft_event::NoteLevel;

/// Static metadata of one strategy kind. `kind_id` is globally stable and append-only (registry
/// in `strategies/registry`); `name` is the crate name. `schema`, `notes`, `report` and `cmds` are
/// exported together in the canonical schema JSON (`params::schema_json`).
#[derive(Clone, Copy, Debug)]
pub struct StrategyDesc {
    pub name: &'static str,
    pub kind_id: u16,
    /// Fast-part snapshot layout and decision-semantics version (`StrategyRowHdr.desc_version`).
    pub version: u16,
    /// `true` = may only be bound to accounts with `AccountMeta.is_prod = false`.
    pub non_prod_only: bool,
    pub schema: &'static ParamSchemaDesc,
    /// Every Note code the strategy writes through `Ctx::note`.
    pub notes: &'static [NoteDesc],
    /// Strategy-specific daily-report fields (aggregated from Notes).
    pub report: &'static [ReportFieldDesc],
    /// Accepted `StrategyCmd` payloads; empty = no strategy commands.
    pub cmds: &'static [CmdDesc],
}
