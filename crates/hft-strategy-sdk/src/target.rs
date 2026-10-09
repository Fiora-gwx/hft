//! Target intents (README "目标意图"): `TargetIntent`, `LegTarget`, `TargetReason`, `Urgency`,
//! `ExecSpec` and the algorithm parameter structs.
//!
//! The types are defined here; the execution semantics (slicing, depth, residuals, repair,
//! risk caps) belong to hft-exec. Discriminants equal the `ExecDecision` wire codes.
// zerocopy's derive helpers carry no Debug impl.
#![allow(missing_debug_implementations)]

use core::mem::{align_of, offset_of, size_of};

use hft_types::{AccountId, GroupId, Notional, Qty, Rate};
use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes};

pub use crate::intent::{PxMode, ResidualPolicy};

/// Why the target exists; discriminant = `ExecDecision.reason`. `MarginStress`/`Hard` force
/// perp-first leg order in hft-exec.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, TryFromBytes, IntoBytes, KnownLayout, Immutable)]
pub enum TargetReason {
    Rebalance = 1,
    Exit = 2,
    MarginStress = 3,
    Hard = 4,
    Manual = 5,
}

/// Urgency; discriminant = `ExecDecision.urgency`. Its effect on cooldowns and Stuck timing is
/// hft-exec's.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, TryFromBytes, IntoBytes, KnownLayout, Immutable)]
pub enum Urgency {
    Passive = 1,
    Normal = 2,
    Urgent = 3,
}

/// Which leg of a pair goes first.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, TryFromBytes, IntoBytes, KnownLayout, Immutable)]
pub enum FirstLeg {
    ByLiquidity = 1,
    PerpFirst = 2,
    SpotFirst = 3,
    /// `PairParams.first_leg_ix` gives the leg.
    Fixed = 4,
}

/// Failure handling of `Simultaneous`.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, TryFromBytes, IntoBytes, KnownLayout, Immutable)]
pub enum SimulOnFail {
    Reverse = 1,
    Hold = 2,
}

/// Target net quantity of one leg (16 B; signed, asset units).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, TryFromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct LegTarget {
    pub leg_ix: u8,
    pub _pad: [u8; 7],
    pub qty: Qty,
}
const _: () = assert!(size_of::<LegTarget>() == 16);
const _: () = assert!(align_of::<LegTarget>() == 8);
const _: () = assert!(offset_of!(LegTarget, qty) == 8);

impl LegTarget {
    pub const ZERO: LegTarget = LegTarget {
        leg_ix: 0,
        _pad: [0; 7],
        qty: Qty(0),
    };
    pub const fn new(leg_ix: u8, qty: Qty) -> LegTarget {
        LegTarget {
            leg_ix,
            _pad: [0; 7],
            qty,
        }
    }
}

/// Independent taker slicing per leg (48 B).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, TryFromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct TakerSliceParams {
    /// One slice ≤ weaker side's visible depth × `depth_frac`.
    pub depth_frac: Rate,
    /// Book levels counted as visible depth (valid range and reject code are hft-exec's).
    pub depth_levels: u8,
    pub _pad0: [u8; 7],
    /// IOC limit = touch × (1 ± max_cross), then capped by `Views::risk_for`.
    pub max_cross: Rate,
    pub max_chunk_notional: Notional,
    /// Per-slice floor (the last slice of a target is exempt, see hft-exec).
    pub min_chunk_notional: Notional,
    /// Minimum interval between two slices of the same cell.
    pub cooldown_ms: u32,
    pub _pad1: u32,
}
const _: () = assert!(size_of::<TakerSliceParams>() == 48);
const _: () = assert!(align_of::<TakerSliceParams>() == 8);
const _: () = assert!(offset_of!(TakerSliceParams, depth_levels) == 8);
const _: () = assert!(offset_of!(TakerSliceParams, max_cross) == 16);
const _: () = assert!(offset_of!(TakerSliceParams, max_chunk_notional) == 24);
const _: () = assert!(offset_of!(TakerSliceParams, min_chunk_notional) == 32);
const _: () = assert!(offset_of!(TakerSliceParams, cooldown_ms) == 40);

/// Two legs: slice `first_leg`, hedge the other leg IOC per fill (96 B).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, TryFromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct PairParams {
    /// First-leg slicing.
    pub slice: TakerSliceParams,
    pub first_leg: FirstLeg,
    /// Leg index when `first_leg = Fixed`; otherwise 0.
    pub first_leg_ix: u8,
    pub hedge_px_mode: PxMode,
    pub _pad0: [u8; 5],
    /// → `HedgePlan.px_param`.
    pub hedge_cross: Rate,
    /// → `HedgePlan.max_slip`.
    pub hedge_max_slip: Rate,
    /// Second-leg quantity = first-leg net fill × num / den.
    pub ratio_num: u32,
    pub ratio_den: u32,
    /// → `LegLink.tolerance`.
    pub fix_tolerance: Rate,
    pub residual: ResidualPolicy,
    pub _pad1: [u8; 7],
}
const _: () = assert!(size_of::<PairParams>() == 96);
const _: () = assert!(align_of::<PairParams>() == 8);
const _: () = assert!(offset_of!(PairParams, first_leg) == 48);
const _: () = assert!(offset_of!(PairParams, first_leg_ix) == 49);
const _: () = assert!(offset_of!(PairParams, hedge_px_mode) == 50);
const _: () = assert!(offset_of!(PairParams, hedge_cross) == 56);
const _: () = assert!(offset_of!(PairParams, hedge_max_slip) == 64);
const _: () = assert!(offset_of!(PairParams, ratio_num) == 72);
const _: () = assert!(offset_of!(PairParams, fix_tolerance) == 80);
const _: () = assert!(offset_of!(PairParams, residual) == 88);

/// Two legs sent together; failures per `on_fail` (72 B).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, TryFromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct SimulParams {
    pub slice: TakerSliceParams,
    pub on_fail: SimulOnFail,
    pub _pad0: [u8; 7],
    pub fix_tolerance: Rate,
    pub residual: ResidualPolicy,
    pub _pad1: [u8; 7],
}
const _: () = assert!(size_of::<SimulParams>() == 72);
const _: () = assert!(align_of::<SimulParams>() == 8);
const _: () = assert!(offset_of!(SimulParams, on_fail) == 48);
const _: () = assert!(offset_of!(SimulParams, fix_tolerance) == 56);
const _: () = assert!(offset_of!(SimulParams, residual) == 64);

/// Reserved (optional): rest first, hedge on fill (128 B).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, TryFromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct MakerParams {
    pub offset: Rate,
    pub requote: Rate,
    pub max_rest_ms: u32,
    pub _pad0: u32,
    pub chunk_notional: Notional,
    pub hedge: PairParams,
}
const _: () = assert!(size_of::<MakerParams>() == 128);
const _: () = assert!(align_of::<MakerParams>() == 8);
const _: () = assert!(offset_of!(MakerParams, max_rest_ms) == 16);
const _: () = assert!(offset_of!(MakerParams, chunk_notional) == 24);
const _: () = assert!(offset_of!(MakerParams, hedge) == 32);

/// Execution algorithm (136 B: discriminant byte + 7 B alignment padding of the `repr(C, u8)`
/// layout + 128 B payload). Discriminants equal `ExecDecision.algo`. Neither this type nor
/// `TargetIntent` is ever written as raw bytes (decisions and snapshots copy field by field), so
/// neither derives `IntoBytes`.
// `cfg_attr` wrapper: `cargo xtask hot-check` sizes every `repr(u8)` enum as 1 byte, which is
// wrong for this data enum (it would report false implicit padding in `TargetIntent`); the
// layout is still exactly `repr(C, u8)` (asserted below).
#[cfg_attr(all(), repr(C, u8))]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum ExecSpec {
    TakerSlice(TakerSliceParams) = 1,
    PairOnFill(PairParams) = 2,
    Simultaneous(SimulParams) = 3,
    /// Reserved (optional).
    MakerFirst(MakerParams) = 4,
}
const _: () = assert!(size_of::<ExecSpec>() == 136);
const _: () = assert!(align_of::<ExecSpec>() == 8);

impl ExecSpec {
    /// `ExecDecision.algo` code.
    pub const fn algo(&self) -> u8 {
        match self {
            ExecSpec::TakerSlice(_) => 1,
            ExecSpec::PairOnFill(_) => 2,
            ExecSpec::Simultaneous(_) => 3,
            ExecSpec::MakerFirst(_) => 4,
        }
    }
    /// The slicing parameters every algorithm carries (MakerFirst: its hedge's).
    pub const fn slice(&self) -> &TakerSliceParams {
        match self {
            ExecSpec::TakerSlice(p) => p,
            ExecSpec::PairOnFill(p) => &p.slice,
            ExecSpec::Simultaneous(p) => &p.slice,
            ExecSpec::MakerFirst(p) => &p.hedge.slice,
        }
    }
}

/// Target position of one (account, group) (232 B). Stays in force until replaced or cleared;
/// an intent field-for-field equal to the current target is not a replacement (hft-exec).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct TargetIntent {
    pub account: AccountId,
    pub _pad0: [u8; 3],
    pub group: GroupId,
    /// First `n_legs` valid; the rest zero.
    pub legs: [LegTarget; 4],
    pub n_legs: u8,
    pub reason: TargetReason,
    pub urgency: Urgency,
    pub _pad1: [u8; 5],
    /// No-trade band: |target − actual| / max(|target|, floor) ≤ band leaves the leg alone.
    pub band: Rate,
    pub exec: ExecSpec,
    /// Carried into slice orders, `ExecDecision` and `ExecUpdate`.
    pub tag: u32,
    pub _pad2: u32,
}
const _: () = assert!(size_of::<TargetIntent>() == 232);
const _: () = assert!(align_of::<TargetIntent>() == 8);
const _: () = assert!(offset_of!(TargetIntent, group) == 4);
const _: () = assert!(offset_of!(TargetIntent, legs) == 8);
const _: () = assert!(offset_of!(TargetIntent, n_legs) == 72);
const _: () = assert!(offset_of!(TargetIntent, reason) == 73);
const _: () = assert!(offset_of!(TargetIntent, urgency) == 74);
const _: () = assert!(offset_of!(TargetIntent, band) == 80);
const _: () = assert!(offset_of!(TargetIntent, exec) == 88);
const _: () = assert!(offset_of!(TargetIntent, tag) == 224);

impl TargetIntent {
    /// The valid legs (`legs[..n_legs]`, at most 4).
    pub fn legs(&self) -> &[LegTarget] {
        let n = usize::from(self.n_legs).min(4);
        self.legs.get(..n).unwrap_or(&[])
    }
}
