//! `hft-strategy-sdk`: the only stable interface between strategies and the engine.
//!
//! The fast part `Strategy` and the slow part `Model` (`strategy`), the parameter schema and its
//! single codec (`params`), the strategy descriptors (`desc`), the read-only views and view types
//! (`views`, `view_types`), the events delivered to strategies (`event`), order and target intents
//! (`intent`, `target`), the intent collector `Ctx` and `SubsBuilder` (`ctx`), Advice payloads
//! (`advice`), `GroupTable` and the group fingerprint (`group`), the checkpoint layout
//! description (`layout`), the snapshot read/write types (`snapshot`) and the assembly trait
//! `StrategySet` (`set`).
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

pub mod advice;
pub mod check;
pub mod ctx;
pub mod desc;
pub mod event;
pub mod group;
pub mod intent;
pub mod layout;
pub mod model_math;
pub mod params;
mod params_macro;
pub mod set;
mod set_macro;
pub mod snapshot;
pub mod strategy;
pub mod target;
#[cfg(feature = "testing")]
pub mod testing;
pub mod view_types;
pub mod views;

pub use advice::{
    AdviceBufs, AdviceFull, AdviceOut, AdvicePayload, PayloadError, PayloadFull, PayloadReader, PayloadWriter,
};
pub use ctx::{Ctx, CtxBufs, CtxFull, EvMask, OwnEvMask, Part, SubsBuilder, SubsErr, SubsLimits, TimerId};
pub use desc::{CmdDesc, CmdError, CmdView, NoteDesc, ReportAgg, ReportFieldDesc, ReportScope};
pub use event::{EventRef, ExecUpdate, ExecUpdateKind, GIX_NONE, MODEL_TICK, OrderUpdate, OrderUpdateKind};
pub use group::{GroupSpecExt, GroupTable};
pub use intent::{
    HedgeKind, HedgePlan, IntentClass, IntentHandle, IntentOutcome, LegLink, LegLinkOpt, LinkRole, OnFail,
    OrderIntent, OrderRef, PxMode, ResidualPolicy, TargetReject,
};
pub use layout::{ElemDesc, ElemTy, LenRule, ModelLayoutDesc, SectionDesc, SectionScope};
pub use params::{FieldDesc, MinorChange, ParamError, ParamSchemaDesc, Relax, Unit};
pub use set::{ResetPreview, StrategySet};
pub use snapshot::{
    PutTy, RestoreError, SectionView, SnapshotFull, SnapshotReader, SnapshotWriter, WriteRec,
};
pub use strategy::{
    GroupResets, Model, ModelParamEffect, NoModel, ParamSet, ResetCause, SnapshotProgress, Strategy,
    StrategyInstance,
};
pub use target::{
    ExecSpec, FirstLeg, LegTarget, MakerParams, PairParams, SimulOnFail, SimulParams, TakerSliceParams,
    TargetIntent, TargetReason, Urgency,
};
pub use view_types::{
    BboView, BrokenReason, CapBits, CapScopeView, CashView, EngineOrigin, ExternalCause, ExternalFillView,
    FeeView, FundingView, GroupPosView, InstRiskView, InventoryView, LegPos, LegTrack, LegView,
    LiquidationView, MarginLevel, MarginView, NoticeKind, OrderStateView, OrderView, PerpPosView, PnlSummary,
    PortfolioRead, RiskEvent, RiskStateView, RxTsSrc, StuckReason, TargetOrigin, TargetState, TargetView,
    TradeView, VenueEquityView, VenueNoticeView, VenueSet,
};
pub use views::{BookRead, BookStatus, EngineClockAnchor, ModelViews, Now, RefView, Views, time};

/// L0 types the strategy-facing API uses.
pub use hft_event::NoteLevel;

/// Everything a strategy crate needs: the L0 types and constants it uses, re-exported so its
/// `Cargo.toml` lists only the SDK and hft-strat-lib.
pub mod prelude {
    pub use hft_event::{ConnStateKind, GroupSpec, LegDef, Level, NoteLevel};
    pub use hft_refdata::{AccountMeta, AssetMeta, InstHot, InstMeta, VenueCaps};
    pub use hft_types::{
        AccountId, AssetId, GROUP_ALL, GROUP_NONE, GroupId, INST_ALL, INST_NONE, InstId, MAX_BBO_CONN_SLOTS,
        MarketKind, Notional, Px, Qty, Rate, RejectBits, RunMode, Side, StrategyId, Tif, Tsc, VenueId,
    };

    pub use crate::*;
}

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
