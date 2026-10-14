//! Types of the execution layer (README "使用本组件", "目标单元", "快照与恢复"): configuration,
//! inputs, outputs, the target cell and the section-21 snapshot rows. No runtime logic besides
//! constructors and constant tables.
// zerocopy's derive helpers carry no Debug impl.
#![allow(missing_debug_implementations)]

use core::mem::{align_of, offset_of, size_of};

use hft_strategy_sdk::{
    ExecUpdate, LegTarget, OrderIntent, OrderRef, ResidualPolicy, TakerSliceParams, TargetReason, TargetView,
};
use hft_types::{AccountId, GroupId, InstId, Notional, Qty, Rate, StrategyInstanceId, Tsc, VenueId};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

// ------------------------------------------------------------------------------- configuration

/// One row of `ExecConfig` (global default row or one account row; README "配置项").
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct ExecParams {
    pub band_floor_notional: Notional,
    /// The single definition of the dust line (already-filled notional ≤ it is dust).
    pub dust_notional: Notional,
    pub fix_tolerance_default: Rate,
    pub fix_max: u8,
    pub fix_cooldown_ms: u32,
    pub fix_deadline_ms: u32,
    pub broken_auto_fix_ms: u32,
    pub external_deadline_ms: u32,
    pub gate_naked_alarm_ms: u32,
    pub stuck_after_ms: u32,
    pub gated_backoff_ms: u32,
    pub max_slice_fails: u8,
    pub max_nofill: u8,
    pub urgent_cooldown_ms: u32,
    pub passive_cooldown_mult: u8,
    /// Slicing of engine orders (repair, residual trim, external default, auto-fix, L3, flatten).
    pub force_slice: TakerSliceParams,
}

impl Default for ExecParams {
    fn default() -> Self {
        ExecParams {
            band_floor_notional: Notional(20_000_000),
            dust_notional: Notional(5_000_000),
            fix_tolerance_default: Rate(20_000_000),
            fix_max: 3,
            fix_cooldown_ms: 1_000,
            fix_deadline_ms: 10_000,
            broken_auto_fix_ms: 120_000,
            external_deadline_ms: 5_000,
            gate_naked_alarm_ms: 300_000,
            stuck_after_ms: 2_000,
            gated_backoff_ms: 500,
            max_slice_fails: 3,
            max_nofill: 5,
            urgent_cooldown_ms: 100,
            passive_cooldown_mult: 4,
            force_slice: TakerSliceParams {
                depth_frac: Rate(300_000_000),
                depth_levels: 5,
                _pad0: [0; 7],
                max_cross: Rate(5_000_000),
                max_chunk_notional: Notional(1_000_000_000),
                min_chunk_notional: Notional(0),
                cooldown_ms: 200,
                _pad1: 0,
            },
        }
    }
}

/// One account of the process: its id, whether it is a Sim account (Sim cells advance after the
/// Live ones) and its optional override row.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct ExecAccount {
    pub account: AccountId,
    pub sim: bool,
    /// `None` = the global default row applies.
    pub params: Option<ExecParams>,
}

/// `EngineConfig` section `exec`: the global default row plus one entry per account.
#[derive(Clone, Eq, PartialEq, Debug, Default)]
pub struct ExecConfig {
    pub default: ExecParams,
    pub accounts: Vec<ExecAccount>,
}

/// The capacities this crate allocates by (projected by the sequencer from ECF1 section 3).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Capacities {
    /// Grouped cells of the whole process (the per-account ungrouped cells come on top).
    pub max_cells_total: u32,
    /// Cells one inst may be referenced by.
    pub max_subs_per_inst: u16,
}

/// `Exec::new` rejections.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ExecConfigError {
    /// `force_slice` outside the target parameter ranges.
    ForceSlice,
    /// `fix_max`, `max_slice_fails`, `max_nofill` or `passive_cooldown_mult` is 0.
    ZeroLimit,
    /// `max_cells_total` + accounts > 65 535 (cix is a u16), or a duplicate account.
    Capacity,
}

/// `hedge_fix_cmd` rejections; nothing is changed when one is returned.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ExecCmdErr {
    Hard,
    UnknownGroup,
    NotQuiescent,
    NothingToFix,
    QuoteUnavailable,
    /// Optional chain feature.
    ChainLeg,
}

// ------------------------------------------------------------------------------------- inputs

/// Link phase of a `LinkNotice` (values of hft-oms `LinkState`).
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum LinkPhase {
    Open = 1,
    Hedging = 2,
    Done = 3,
    Broken = 4,
}

/// Why a link broke (values of hft-oms `LinkBroken`; `None` = not broken).
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum LinkBrokenCause {
    None = 0,
    RetryExhausted = 1,
    Deadline = 2,
    HedgeLimit = 3,
    Overfill = 4,
    ReverseFailed = 5,
    ResumeTimeout = 6,
}

/// Link state change projected by the sequencer from the OMS link output.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct LinkNotice {
    pub link_id: u32,
    pub account: AccountId,
    pub group: GroupId,
    pub first: OrderRef,
    pub state: LinkPhase,
    pub broken: LinkBrokenCause,
    /// [`LinkNotice::RESUME_HOLD`] (= hft-oms `LinkUpdate.flags`).
    pub flags: u8,
    pub first_net: Qty,
    pub hedge_filled: Qty,
    pub owed: Qty,
    /// The link's `LegLink.tolerance` (repair threshold of the group).
    pub tolerance: Rate,
    /// The link's `HedgePlan.residual`.
    pub residual: ResidualPolicy,
}

impl LinkNotice {
    pub const RESUME_HOLD: u8 = 1;
}

/// Result of one `ExecOrder` in the intent pipeline, fed back in the same event.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum PlaceResult {
    Placed(OrderRef),
    Rejected(hft_types::RejectBits),
    Shadowed,
}

// ------------------------------------------------------------------------------------ outputs

/// Cell index + per-cell monotonic sequence number of an order.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct ExecOrderId {
    pub cix: u16,
    pub n: u16,
}

impl ExecOrderId {
    /// `ExecDecision.exec_order_id` packing: `cix | n << 16`.
    pub const fn packed(self) -> u32 {
        (self.cix as u32) | ((self.n as u32) << 16)
    }
}

/// Who an order serves.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum ExecOrigin {
    /// A strategy target (goes through the mode gate).
    Target,
    /// Engine order (repair, residual, external default, auto-fix, L3, flatten, command; strategy
    /// targets of an account in `force_active`/`flatten_lock`).
    Engine,
}

/// `OrderDecision.reason` code of execution-layer orders (append-only).
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum ExecOrderKind {
    Slice = 1,
    PairFirst = 2,
    SimulFirst = 3,
    MakerFirst = 4,
    HedgeFix = 5,
    ResidualTrim = 6,
    ExternalDefault = 7,
    ForceStep = 8,
    CmdHedgeFix = 9,
    AutoFix = 10,
}

impl ExecOrderKind {
    pub const fn code(self) -> u8 {
        self as u8
    }
    pub const fn from_code(c: u8) -> Option<ExecOrderKind> {
        Some(match c {
            1 => ExecOrderKind::Slice,
            2 => ExecOrderKind::PairFirst,
            3 => ExecOrderKind::SimulFirst,
            4 => ExecOrderKind::MakerFirst,
            5 => ExecOrderKind::HedgeFix,
            6 => ExecOrderKind::ResidualTrim,
            7 => ExecOrderKind::ExternalDefault,
            8 => ExecOrderKind::ForceStep,
            9 => ExecOrderKind::CmdHedgeFix,
            10 => ExecOrderKind::AutoFix,
            _ => return None,
        })
    }
    /// Orders with a `hedge_cap_qty` (Hedge class).
    pub const fn is_hedge(self) -> bool {
        matches!(
            self,
            ExecOrderKind::HedgeFix
                | ExecOrderKind::ResidualTrim
                | ExecOrderKind::ExternalDefault
                | ExecOrderKind::CmdHedgeFix
                | ExecOrderKind::AutoFix
        )
    }
    /// Repair-family orders counted by `FixState`.
    pub const fn is_fix(self) -> bool {
        matches!(
            self,
            ExecOrderKind::HedgeFix | ExecOrderKind::ResidualTrim | ExecOrderKind::ExternalDefault
        )
    }
}

/// One order request for the sequencer's intent pipeline.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct ExecOrder {
    pub id: ExecOrderId,
    pub intent: OrderIntent,
    pub origin: ExecOrigin,
    pub kind: ExecOrderKind,
    pub reason: TargetReason,
    /// Quantity cap of Hedge-class orders (`naked_filled + 1 lot`); 0 for the rest.
    pub hedge_cap_qty: Qty,
}

/// `oms.cancel_resting_on(account, inst)` request.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct CancelResting {
    pub account: AccountId,
    pub inst: InstId,
}

/// `risk.mode.apply(Auto(Broken{account, group}))` request.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct BrokenReq {
    pub account: AccountId,
    pub group: GroupId,
}

/// `EventRef::Exec` for the owning strategy instance.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct ExecNotify {
    pub account: AccountId,
    pub owner: StrategyInstanceId,
    pub u: ExecUpdate,
}

/// Kinds of `Timer{owner = Exec}` (`timer_id = kind | account << 8`).
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum ExecTimerKind {
    Wake = 1,
    Fix = 2,
    External = 3,
    Continue = 4,
    AutoFix = 5,
    Alarm = 6,
}

impl ExecTimerKind {
    pub const fn from_code(c: u8) -> Option<ExecTimerKind> {
        Some(match c {
            1 => ExecTimerKind::Wake,
            2 => ExecTimerKind::Fix,
            3 => ExecTimerKind::External,
            4 => ExecTimerKind::Continue,
            5 => ExecTimerKind::AutoFix,
            6 => ExecTimerKind::Alarm,
            _ => return None,
        })
    }
}

/// Register (`cancel = false`, due at `at`) or cancel one `Timer{owner = Exec}`.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct ExecTimerReq {
    pub cancel: bool,
    pub timer_id: u32,
    pub group: GroupId,
    /// Due time (this run's TSC); 0 for a cancellation.
    pub at: Tsc,
}

impl ExecTimerReq {
    /// `timer_id` of `kind` for `account`.
    pub const fn id(kind: ExecTimerKind, account: AccountId) -> u32 {
        (kind as u32) | ((account.0 as u32) << 8)
    }
    pub const fn kind(&self) -> Option<ExecTimerKind> {
        ExecTimerKind::from_code((self.timer_id & 0xFF) as u8)
    }
    /// The u64 `on_timer` receives for this timer (`timer_id | group << 32`).
    pub const fn token(&self) -> u64 {
        (self.timer_id as u64) | ((self.group.0 as u64) << 32)
    }
}

/// Reason of an auto-fix failure (`AutoFixState.fail`; 3 and 5 reserved, never used).
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum AutoFixFail {
    ChainLeg = 1,
    MinOrder = 2,
    Rejected = 4,
    Residual = 6,
    Deadline = 7,
    SignFlip = 8,
    RiskReject = 9,
}

/// `AutoFixState.phase` codes.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum AutoFixPhase {
    None = 0,
    Primed = 1,
    WaitHard = 2,
    Running = 3,
    Done = 4,
    Failed = 5,
    Cancelled = 6,
    WaitHold = 7,
}

impl AutoFixPhase {
    pub const fn from_code(c: u8) -> Option<AutoFixPhase> {
        Some(match c {
            0 => AutoFixPhase::None,
            1 => AutoFixPhase::Primed,
            2 => AutoFixPhase::WaitHard,
            3 => AutoFixPhase::Running,
            4 => AutoFixPhase::Done,
            5 => AutoFixPhase::Failed,
            6 => AutoFixPhase::Cancelled,
            7 => AutoFixPhase::WaitHold,
            _ => return None,
        })
    }
}

// -------------------------------------------------------------------------------- cell parts

/// Auto bounded close-out state of a Broken cell (56 B; also the bytes of `ExecCellRow.auto_fix`).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct AutoFixState {
    /// `AutoFixPhase` code.
    pub phase: u8,
    pub leg_ix: u8,
    pub n_slices: u8,
    /// `AutoFixFail` code; 0 = none.
    pub fail: u8,
    /// Sign of `naked_sum` at start (+1 / −1).
    pub start_sign: i8,
    pub _pad: [u8; 3],
    /// Accumulated executable time of this attempt (ms).
    pub exec_ms: u32,
    pub _pad2: u32,
    pub due_tsc: Tsc,
    pub started_tsc: Tsc,
    /// Start of the current executable stretch; 0 = waiting (not accumulating).
    pub run_mark_tsc: Tsc,
    /// Quantity cap fixed at start.
    pub budget: Qty,
    pub done_qty: Qty,
}
const _: () = assert!(size_of::<AutoFixState>() == 56);
const _: () = assert!(align_of::<AutoFixState>() == 8);
const _: () = assert!(offset_of!(AutoFixState, exec_ms) == 8);
const _: () = assert!(offset_of!(AutoFixState, due_tsc) == 16);
const _: () = assert!(offset_of!(AutoFixState, started_tsc) == 24);
const _: () = assert!(offset_of!(AutoFixState, run_mark_tsc) == 32);
const _: () = assert!(offset_of!(AutoFixState, budget) == 40);
const _: () = assert!(offset_of!(AutoFixState, done_qty) == 48);

impl AutoFixState {
    pub fn phase(&self) -> AutoFixPhase {
        AutoFixPhase::from_code(self.phase).unwrap_or(AutoFixPhase::None)
    }
}

/// Repair counters (`count`, first trigger, earliest next repair).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct FixState {
    pub count: u8,
    pub first_tsc: Tsc,
    pub next_tsc: Tsc,
}

impl FixState {
    pub const ZERO: FixState = FixState {
        count: 0,
        first_tsc: Tsc(0),
        next_tsc: Tsc(0),
    };
}

/// Leg kind codes of `LegRef.kind`.
pub const LEG_SPOT: u8 = 1;
pub const LEG_PERP: u8 = 2;
pub const LEG_CHAIN: u8 = 3;

/// One leg of a cell (32 B; also the `ExecCellRow.legs` row element).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct LegRef {
    pub venue: VenueId,
    /// `LEG_SPOT` | `LEG_PERP` | `LEG_CHAIN`.
    pub kind: u8,
    /// 1 = the venue supports reduce-only orders (`VenueCaps.reduce_only`).
    pub reduce_only_cap: u8,
    pub _pad: u8,
    pub inst: InstId,
    pub lot: Qty,
    pub min_qty: Qty,
    /// Ungrouped cell: OMS slot of the leg's tracked slice (`TargetCell.slot`); `min_notional` is
    /// read from refdata.
    pub slot: u32,
    pub _pad2: u32,
}
const _: () = assert!(size_of::<LegRef>() == 32);
const _: () = assert!(offset_of!(LegRef, inst) == 4);
const _: () = assert!(offset_of!(LegRef, lot) == 8);
const _: () = assert!(offset_of!(LegRef, min_qty) == 16);
const _: () = assert!(offset_of!(LegRef, slot) == 24);

/// One leg of a cell (24 B; `min_notional` is read from the inst row).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct CellLeg {
    pub venue: VenueId,
    /// `LEG_SPOT` | `LEG_PERP` | `LEG_CHAIN`.
    pub kind: u8,
    /// 1 = the venue supports reduce-only orders.
    pub reduce_only_cap: u8,
    pub inst: InstId,
    pub lot: Qty,
    pub min_qty: Qty,
}

impl CellLeg {
    pub const ZERO: CellLeg = CellLeg {
        venue: VenueId(0),
        kind: 0,
        reduce_only_cap: 0,
        inst: InstId(0),
        lot: Qty(0),
        min_qty: Qty(0),
    };
}

/// `ActiveSliceRow.kind` codes.
pub const ACT_NONE: u8 = 0;
pub const ACT_SLICE: u8 = 1;
pub const ACT_PAIR: u8 = 2;
pub const ACT_SIMUL: u8 = 3;
pub const ACT_MAKER: u8 = 4;
pub const ACT_SERIAL: u8 = 5;
pub const ACT_AUTOFIX: u8 = 6;
pub const ACT_FORCE: u8 = 7;

/// The cell's in-flight execution-layer slice(s) (32 B; also the `ExecCellRow.active` row).
/// `ext` = `[order kind (ExecOrderKind code), in-flight leg mask, sell-side leg mask, filled leg
/// mask]` of the in-flight slices (per-leg concurrency of `TakerSlice` and serial legs).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct ActiveSliceRow {
    pub kind: u8,
    pub leg_ix: u8,
    /// `ExecOrderId.n` of the latest slice.
    pub n: u16,
    /// 0 = no link.
    pub link_id: u32,
    pub first: OrderRefRow,
    pub ext: [u8; 4],
    pub planned: Qty,
}
const _: () = assert!(size_of::<ActiveSliceRow>() == 32);
const _: () = assert!(offset_of!(ActiveSliceRow, link_id) == 4);
const _: () = assert!(offset_of!(ActiveSliceRow, first) == 8);
const _: () = assert!(offset_of!(ActiveSliceRow, ext) == 20);
const _: () = assert!(offset_of!(ActiveSliceRow, planned) == 24);

/// `OrderRef` bytes inside snapshot rows (all-integer twin of the SDK type, 12 B).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct OrderRefRow {
    pub venue: u8,
    pub account: u8,
    pub _pad: [u8; 2],
    pub slot: u32,
    pub r#gen: u32,
}
const _: () = assert!(size_of::<OrderRefRow>() == 12);

impl OrderRefRow {
    pub const fn of(r: OrderRef) -> OrderRefRow {
        OrderRefRow {
            venue: r.venue.0,
            account: r.account.0,
            _pad: [0; 2],
            slot: r.slot,
            r#gen: r.r#gen,
        }
    }
    pub const fn get(&self) -> OrderRef {
        OrderRef::new(
            VenueId(self.venue),
            AccountId(self.account),
            self.slot,
            self.r#gen,
        )
    }
}

impl ActiveSliceRow {
    /// Order kind of the in-flight slices.
    pub fn order_kind(&self) -> Option<ExecOrderKind> {
        ExecOrderKind::from_code(self.ext[0])
    }
    pub const fn leg_mask(&self) -> u8 {
        self.ext[1]
    }
}

/// Cell state (`TargetState` wire codes; reasons in `TargetCell.view.state`).
pub use hft_strategy_sdk::TargetState as CellState;

/// `TargetCell.hold` bits.
pub const MANUAL_HOLD: u8 = 1;
pub const LINK_HOLD: u8 = 2;
pub const MANUAL_HOLD_DEFERRED: u8 = 4;

/// `TargetCell.cap_block` bits.
pub const CAP_BROKEN: u8 = 1;
pub const CAP_REORG: u8 = 2;

/// `TargetCell.serial` value.
pub const SERIAL_FALLBACK: u8 = 1;

/// `TargetCell.deferred` bits: records and work a multi-cell entry left for `advance`.
pub const D_ENGINE_SET: u8 = 1;
pub const D_ENGINE_CLEARED: u8 = 2;
pub const D_CLEARED_BY_CMD: u8 = 4;
pub const D_STATE: u8 = 8;
pub const D_TIMERS: u8 = 16;
pub const D_ALARM: u8 = 32;
pub const D_HOLD_REL: u8 = 64;
pub const D_EXT_CLR: u8 = 128;

/// `TargetCell.aux16` layout: bits 0..7 registered timer kinds (bit k = kind k), bits 8..11
/// in-flight slice of leg i is `Unknown`, bits 12..13 residual policy of the latest link (0 = none).
pub const AUX_UNKNOWN_SHIFT: u16 = 8;
pub const AUX_RESID_SHIFT: u16 = 12;

/// One (account, group) target cell (README "目标单元").
#[derive(Copy, Clone, Debug)]
pub struct TargetCell {
    /// Projection returned by `Exec::view` (current target, origin, state, progress).
    pub view: TargetView,
    pub account: AccountId,
    pub group: GroupId,
    pub owner: StrategyInstanceId,
    pub n_legs: u8,
    pub has_target: bool,
    /// `D_*` bits.
    pub deferred: u8,
    /// `state_from` code of the deferred `State` record.
    pub def_from: u8,
    pub cap_block: u8,
    pub serial: u8,
    pub hold: u8,
    pub fail_streak: u8,
    pub nofill_streak: u8,
    pub fund_shrink: u8,
    pub force_steps: u16,
    /// See `AUX_*`.
    pub aux16: u16,
    pub gated_bits: hft_types::RejectBits,
    pub legs: [CellLeg; 4],
    /// First-leg rounding unit of pair targets (0 = not a pair target); not snapshotted.
    pub pair_lot: Qty,
    pub active: ActiveSliceRow,
    pub wake_at: Tsc,
    pub gated_since: Tsc,
    pub stuck_since: Tsc,
    pub fix: FixState,
    /// 0 = no pending external fill.
    pub ext_deadline: Tsc,
    /// Repair threshold (latest link tolerance or target `fix_tolerance`); 0 = config default.
    pub tolerance: Rate,
    /// 0 = not Broken.
    pub broken_since: Tsc,
    pub auto_fix: AutoFixState,
    /// Grouped cells: signed fill of each leg's in-flight slice. Ungrouped cell: `own_infl`.
    pub aux: [Qty; 4],
    /// Ungrouped cell: OMS slot of each leg's tracked slice (0 = unknown, e.g. after `restore`);
    /// tells a released `Unknown` order's updates from the leg's newer slice. Not in the snapshot.
    pub slot: [u32; 4],
}

impl TargetCell {
    /// Registered timer kinds.
    pub const fn timer_mask(&self) -> u8 {
        (self.aux16 & 0xFF) as u8
    }
    /// Whether leg `i`'s in-flight slice reported `Unknown`.
    pub const fn leg_unknown(&self, i: usize) -> bool {
        i < 4 && self.aux16 & (1 << (AUX_UNKNOWN_SHIFT as usize + i)) != 0
    }
    /// Residual policy of the latest link of the group.
    pub fn link_residual(&self) -> Option<ResidualPolicy> {
        match (self.aux16 >> AUX_RESID_SHIFT) & 3 {
            1 => Some(ResidualPolicy::Defer),
            2 => Some(ResidualPolicy::TrimExcess),
            3 => Some(ResidualPolicy::Hold),
            _ => None,
        }
    }
    /// Whether the cell is the per-account ungrouped cell.
    pub const fn is_ungrouped(&self) -> bool {
        self.group.0 == 0
    }
    /// Target net quantity of leg `i` if the target lists it.
    pub fn target_of(&self, i: usize) -> Option<Qty> {
        if !self.has_target {
            return None;
        }
        self.view
            .intent
            .legs()
            .iter()
            .find(|l| usize::from(l.leg_ix) == i)
            .map(|l| l.qty)
    }
}

/// Per-account state (README "目标单元" `AcctState`).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct AcctState {
    pub force_active: bool,
    /// `force_req` kind: 0 none, 1 venues (proportional), 2 flatten.
    pub force_kind: u8,
    /// `FORCE_FULL` = the flatten request covers every cell (first `engine_flatten`).
    pub force_flags: u8,
    /// Venues of the pending proportional request; kept as the L3 venue set while `force_active`.
    pub force_venues: u64,
    pub force_frac: Rate,
    pub flatten_lock: bool,
    pub gate_open: bool,
    pub venue_open: u64,
    pub gate_closed_tsc: Tsc,
}

impl AcctState {
    pub const ZERO: AcctState = AcctState {
        force_active: false,
        force_kind: 0,
        force_flags: 0,
        force_venues: 0,
        force_frac: Rate(0),
        flatten_lock: false,
        gate_open: false,
        venue_open: 0,
        gate_closed_tsc: Tsc(0),
    };
}

/// `AcctState.force_kind` values.
pub const FORCE_NONE: u8 = 0;
pub const FORCE_VENUES: u8 = 1;
pub const FORCE_FLATTEN: u8 = 2;
/// `AcctState.force_flags` bit.
pub const FORCE_FULL: u8 = 1;

// ------------------------------------------------------------------------- snapshot rows (21)

/// Snapshot section number of this crate.
pub const SECTION_EXEC: u16 = 21;
/// Row version of section 21.
pub const ROW_VERSION: u16 = 2;
/// `ExecRowHdr.kind` values.
pub const ROW_CELL: u8 = 1;
pub const ROW_ACCT: u8 = 2;
pub const ROW_GATE: u8 = 3;

/// Header of every section-21 row (8 B).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct ExecRowHdr {
    pub kind: u8,
    pub _pad: [u8; 3],
    /// Row body bytes.
    pub len: u32,
}
const _: () = assert!(size_of::<ExecRowHdr>() == 8);
const _: () = assert!(offset_of!(ExecRowHdr, len) == 4);

/// Cell row (400 B; followed by `ExecTargetRow`).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct ExecCellRow {
    pub account: u8,
    /// `TargetState` wire code.
    pub state: u8,
    /// `StuckReason` code; 0 = none.
    pub stuck: u8,
    /// `BrokenReason` code; 0 = none.
    pub broken: u8,
    pub group: u32,
    pub owner: u8,
    pub n_legs: u8,
    pub has_target: u8,
    /// `TargetOriginWire` code.
    pub origin: u8,
    /// PENDING=1 SERIAL=2 MANUAL_HOLD=4 LINK_HOLD=8 MANUAL_HOLD_DEFERRED=16.
    pub flags: u8,
    /// `D_*` bits.
    pub deferred: u8,
    /// BROKEN=1 REORG=2.
    pub cap_block: u8,
    pub fix_count: u8,
    pub fail_streak: u8,
    pub nofill_streak: u8,
    pub fund_shrink: u8,
    /// `state_from` code of the deferred `State` record (0 = none).
    pub def_from: u8,
    pub gated_bits: u32,
    pub n_slices: u32,
    pub force_steps: u16,
    /// `TargetCell.aux16` (timer mask, Unknown legs, link residual policy).
    pub aux16: u16,
    pub set_seq: u64,
    pub set_tsc: Tsc,
    pub last_slice_tsc: Tsc,
    pub wake_at: Tsc,
    pub gated_since: Tsc,
    pub stuck_since: Tsc,
    pub fix_first_tsc: Tsc,
    pub fix_next_tsc: Tsc,
    pub ext_deadline: Tsc,
    pub broken_since: Tsc,
    pub tolerance: Rate,
    /// Grouped cells: `filled`; ungrouped cell: released `Unknown` remainder per leg.
    pub filled: [Qty; 4],
    /// Ungrouped cell: `own_infl`; grouped cells: fill of each leg's in-flight slice.
    pub own_infl: [Qty; 4],
    pub active: ActiveSliceRow,
    pub auto_fix: AutoFixState,
    pub legs: [LegRef; 4],
}
const _: () = assert!(size_of::<ExecCellRow>() == 400);
const _: () = assert!(align_of::<ExecCellRow>() == 8);
const _: () = assert!(offset_of!(ExecCellRow, state) == 1);
const _: () = assert!(offset_of!(ExecCellRow, group) == 4);
const _: () = assert!(offset_of!(ExecCellRow, owner) == 8);
const _: () = assert!(offset_of!(ExecCellRow, flags) == 12);
const _: () = assert!(offset_of!(ExecCellRow, deferred) == 13);
const _: () = assert!(offset_of!(ExecCellRow, cap_block) == 14);
const _: () = assert!(offset_of!(ExecCellRow, def_from) == 19);
const _: () = assert!(offset_of!(ExecCellRow, gated_bits) == 20);
const _: () = assert!(offset_of!(ExecCellRow, aux16) == 30);
const _: () = assert!(offset_of!(ExecCellRow, set_seq) == 32);
const _: () = assert!(offset_of!(ExecCellRow, set_tsc) == 40);
const _: () = assert!(offset_of!(ExecCellRow, fix_first_tsc) == 80);
const _: () = assert!(offset_of!(ExecCellRow, ext_deadline) == 96);
const _: () = assert!(offset_of!(ExecCellRow, broken_since) == 104);
const _: () = assert!(offset_of!(ExecCellRow, tolerance) == 112);
const _: () = assert!(offset_of!(ExecCellRow, filled) == 120);
const _: () = assert!(offset_of!(ExecCellRow, own_infl) == 152);
const _: () = assert!(offset_of!(ExecCellRow, active) == 184);
const _: () = assert!(offset_of!(ExecCellRow, auto_fix) == 216);
const _: () = assert!(offset_of!(ExecCellRow, legs) == 272);

/// `ExecCellRow.flags` bits.
pub const RF_PENDING: u8 = 1;
pub const RF_SERIAL: u8 = 2;
pub const RF_MANUAL_HOLD: u8 = 4;
pub const RF_LINK_HOLD: u8 = 8;
pub const RF_MANUAL_HOLD_DEFERRED: u8 = 16;
/// Next-pass pending work bypasses the wake gate.
pub const RF_FORCED: u8 = 32;
/// Unvisited work in the current capacity-limited pass.
pub const RF_ADVANCING: u8 = 64;
pub const RF_ADVANCING_FORCED: u8 = 128;

/// Account row (24 B).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct ExecAcctRow {
    pub account: u8,
    pub force_active: u8,
    pub flatten_lock: u8,
    /// 0 = no `force_req`, 1 = venues (proportional), 2 = flatten.
    pub force_kind: u8,
    /// `FORCE_FULL` = the flatten request covers every cell.
    pub force_flags: u8,
    pub _pad: [u8; 3],
    pub force_venues: u64,
    pub force_frac: Rate,
}
const _: () = assert!(size_of::<ExecAcctRow>() == 24);
const _: () = assert!(offset_of!(ExecAcctRow, force_kind) == 3);
const _: () = assert!(offset_of!(ExecAcctRow, force_venues) == 8);
const _: () = assert!(offset_of!(ExecAcctRow, force_frac) == 16);

/// `ExecSpec` written field by field (136 B; padding and parameter tail always zero).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct ExecSpecRow {
    /// `ExecSpec` discriminant (= `ExecDecision.algo`).
    pub algo: u8,
    pub _pad: [u8; 7],
    pub params: [u8; 128],
}
const _: () = assert!(size_of::<ExecSpecRow>() == 136);
const _: () = assert!(offset_of!(ExecSpecRow, params) == 8);

/// Target row (232 B; all zero when the cell has no target).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct ExecTargetRow {
    pub account: u8,
    pub _pad0: [u8; 3],
    pub group: u32,
    pub legs: [LegTargetRow; 4],
    pub n_legs: u8,
    /// `TargetReason` code.
    pub reason: u8,
    /// `Urgency` code.
    pub urgency: u8,
    pub _pad1: [u8; 5],
    pub band: Rate,
    pub exec: ExecSpecRow,
    pub tag: u32,
    pub _pad2: u32,
}
const _: () = assert!(size_of::<ExecTargetRow>() == 232);
const _: () = assert!(offset_of!(ExecTargetRow, group) == 4);
const _: () = assert!(offset_of!(ExecTargetRow, legs) == 8);
const _: () = assert!(offset_of!(ExecTargetRow, n_legs) == 72);
const _: () = assert!(offset_of!(ExecTargetRow, reason) == 73);
const _: () = assert!(offset_of!(ExecTargetRow, urgency) == 74);
const _: () = assert!(offset_of!(ExecTargetRow, band) == 80);
const _: () = assert!(offset_of!(ExecTargetRow, exec) == 88);
const _: () = assert!(offset_of!(ExecTargetRow, tag) == 224);

/// SDK `LegTarget` bytes (16 B).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct LegTargetRow {
    pub leg_ix: u8,
    pub _pad: [u8; 7],
    pub qty: Qty,
}
const _: () = assert!(size_of::<LegTargetRow>() == 16);

impl LegTargetRow {
    pub const fn of(l: &LegTarget) -> LegTargetRow {
        LegTargetRow {
            leg_ix: l.leg_ix,
            _pad: [0; 7],
            qty: l.qty,
        }
    }
}

/// Gate row (24 B; one per account).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct ExecGateRow {
    pub account: u8,
    pub gate_open: u8,
    pub _pad: [u8; 6],
    pub venue_open: u64,
    pub gate_closed_tsc: Tsc,
}
const _: () = assert!(size_of::<ExecGateRow>() == 24);
const _: () = assert!(offset_of!(ExecGateRow, venue_open) == 8);
const _: () = assert!(offset_of!(ExecGateRow, gate_closed_tsc) == 16);

const _: () = assert!(size_of::<TargetCell>() <= 640);
