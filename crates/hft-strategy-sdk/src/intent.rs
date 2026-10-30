//! Order intents and the order-intent vocabulary (README "Ctx 与订单意图"): `OrderIntent`,
//! `IntentClass`, `LegLink`/`LegLinkOpt`, `HedgePlan` and its enums, `OrderRef`, `IntentHandle`,
//! `IntentOutcome` and `TargetReject`.
//!
//! The types are defined here; the semantics of links and hedges belong to hft-oms and hft-exec.
//! Every `#[repr(C)]` struct has explicit `_pad` fields and compile-time layout assertions; the
//! enums' discriminants equal the wire codes (append-only).
// zerocopy's derive helpers carry no Debug impl.
#![allow(missing_debug_implementations)]

use core::mem::{align_of, offset_of, size_of};

use hft_types::{
    AccountId, AssetExp, GroupId, InstId, Notional, Px, Qty, Rate, RejectBits, Side, Tif, VenueId,
};
use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes};

/// Category the strategy declares for an order intent; the final classification is hft-risk
/// `classify()`'s. Discriminants equal the `OrderDecision` class codes.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, TryFromBytes, IntoBytes, KnownLayout, Immutable)]
pub enum IntentClass {
    Open = 1,
    Reduce = 2,
    Hedge = 3,
    Flatten = 4,
}

/// Role of an order in a two-leg link; discriminant = `OrderDecision.link_role` wire code
/// (0 = no link appears only in that record field and is not a value of this enum).
/// Strategies only ever fill `First`.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, TryFromBytes, IntoBytes, KnownLayout, Immutable)]
pub enum LinkRole {
    First = 1,
    Second = 2,
    SecondRetry = 3,
    HedgeFix = 4,
    Reverse = 5,
}

/// How the second leg of a link is produced (by hft-oms; the strategy only places the first leg).
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, TryFromBytes, IntoBytes, KnownLayout, Immutable)]
pub enum HedgeKind {
    None = 0,
    /// Second leg per fill increment of the first leg.
    OnFill = 1,
    /// Second leg in the same event as the first leg's `place`.
    Simultaneous = 2,
}

/// Second-leg pricing mode.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, TryFromBytes, IntoBytes, KnownLayout, Immutable)]
pub enum PxMode {
    Cross = 1,
    Mid = 2,
    Limit = 3,
}

/// What hft-oms does when the second leg fails. `OnFill` defaults to `Retry`; `Simultaneous` only
/// takes `Reverse | Hold`.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, TryFromBytes, IntoBytes, KnownLayout, Immutable)]
pub enum OnFail {
    Retry = 1,
    Reverse = 2,
    Hold = 3,
}

/// Residual rule read by hft-exec.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, TryFromBytes, IntoBytes, KnownLayout, Immutable)]
pub enum ResidualPolicy {
    Defer = 1,
    TrimExcess = 2,
    Hold = 3,
}

/// Second-leg plan of a link (48 B). Semantics (sizing caps, `max_slip` soft threshold) belong to
/// hft-oms.
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, TryFromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct HedgePlan {
    pub kind: HedgeKind,
    pub px_mode: PxMode,
    pub on_fail: OnFail,
    /// Second leg `Ioc` (`Simultaneous` may use `Fok`).
    pub tif: Tif,
    pub venue: VenueId,
    pub account: AccountId,
    pub side: Side,
    /// 1 = closing second leg. Risk validates the reduction; the venue flag requires capability.
    pub reduce_only: u8,
    pub inst: InstId,
    pub ratio_num: u32,
    pub ratio_den: u32,
    pub residual: ResidualPolicy,
    pub _pad: [u8; 3],
    pub px_param: Rate,
    /// Soft threshold: beyond it the second leg is priced at the risk `rr_max_cross` cap and still sent.
    pub max_slip: Rate,
    pub limit_px: Px,
}
const _: () = assert!(size_of::<HedgePlan>() == 48);
const _: () = assert!(align_of::<HedgePlan>() == 8);
const _: () = assert!(offset_of!(HedgePlan, reduce_only) == 7);
const _: () = assert!(offset_of!(HedgePlan, inst) == 8);
const _: () = assert!(offset_of!(HedgePlan, ratio_num) == 12);
const _: () = assert!(offset_of!(HedgePlan, ratio_den) == 16);
const _: () = assert!(offset_of!(HedgePlan, residual) == 20);
const _: () = assert!(offset_of!(HedgePlan, px_param) == 24);
const _: () = assert!(offset_of!(HedgePlan, max_slip) == 32);
const _: () = assert!(offset_of!(HedgePlan, limit_px) == 40);

/// Two-leg link carried by the first leg (64 B). Invalid role/on_fail/tif combinations are rejected
/// `INVALID` by hft-oms `link_valid`.
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, TryFromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct LegLink {
    pub group: GroupId,
    /// Strategies only fill `First`.
    pub role: LinkRole,
    pub _pad: [u8; 3],
    /// Repair threshold of the group (leg difference / hedge quantity).
    pub tolerance: Rate,
    pub hedge: HedgePlan,
}
const _: () = assert!(size_of::<LegLink>() == 64);
const _: () = assert!(align_of::<LegLink>() == 8);
const _: () = assert!(offset_of!(LegLink, role) == 4);
const _: () = assert!(offset_of!(LegLink, tolerance) == 8);
const _: () = assert!(offset_of!(LegLink, hedge) == 16);

/// Optional link (72 B): `some = 0` = single leg (then `l` is ignored and should be zeroed).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, TryFromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct LegLinkOpt {
    pub some: u8,
    pub _pad: [u8; 7],
    pub l: LegLink,
}
const _: () = assert!(size_of::<LegLinkOpt>() == 72);
const _: () = assert!(align_of::<LegLinkOpt>() == 8);
const _: () = assert!(offset_of!(LegLinkOpt, some) == 0);
const _: () = assert!(offset_of!(LegLinkOpt, l) == 8);

impl HedgePlan {
    /// An all-default plan (`HedgeKind::None`); used as the filler of an absent link.
    pub const NONE: HedgePlan = HedgePlan {
        kind: HedgeKind::None,
        px_mode: PxMode::Cross,
        on_fail: OnFail::Retry,
        tif: Tif::Ioc,
        venue: VenueId(0),
        account: AccountId(0),
        side: Side::Buy,
        reduce_only: 0,
        inst: InstId(0),
        ratio_num: 0,
        ratio_den: 0,
        residual: ResidualPolicy::Defer,
        _pad: [0; 3],
        px_param: Rate(0),
        max_slip: Rate(0),
        limit_px: Px(0),
    };
}

impl LegLinkOpt {
    /// No link (single-leg order).
    pub const NONE: LegLinkOpt = LegLinkOpt {
        some: 0,
        _pad: [0; 7],
        l: LegLink {
            group: GroupId(0),
            role: LinkRole::First,
            _pad: [0; 3],
            tolerance: Rate(0),
            hedge: HedgePlan::NONE,
        },
    };

    /// A present link.
    pub const fn some(l: LegLink) -> LegLinkOpt {
        LegLinkOpt {
            some: 1,
            _pad: [0; 7],
            l,
        }
    }

    /// The link, if present.
    pub const fn get(&self) -> Option<&LegLink> {
        if self.some != 0 { Some(&self.l) } else { None }
    }
}

/// One order intent (112 B). `flags` bits: [`OrderIntent::REDUCE_ONLY`], [`OrderIntent::SPEND`].
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, TryFromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct OrderIntent {
    pub venue: VenueId,
    pub account: AccountId,
    pub side: Side,
    pub tif: Tif,
    /// Declared class; the final classification is hft-risk `classify()`'s.
    pub class: IntentClass,
    pub flags: u8,
    pub _pad: [u8; 2],
    pub inst: InstId,
    pub group: GroupId,
    pub px: Px,
    pub qty: Qty,
    /// Two-leg link (`some = 0` = single leg); semantics belong to hft-oms.
    pub link: LegLinkOpt,
    /// Strategy-defined channel tag.
    pub tag: u32,
    /// Longest resting time of a GTC/PostOnly order (OMS sweep sends `Aged`); 0 = no limit.
    pub max_rest_ms: u32,
}
const _: () = assert!(size_of::<OrderIntent>() == 112);
const _: () = assert!(align_of::<OrderIntent>() == 8);
const _: () = assert!(offset_of!(OrderIntent, class) == 4);
const _: () = assert!(offset_of!(OrderIntent, flags) == 5);
const _: () = assert!(offset_of!(OrderIntent, inst) == 8);
const _: () = assert!(offset_of!(OrderIntent, group) == 12);
const _: () = assert!(offset_of!(OrderIntent, px) == 16);
const _: () = assert!(offset_of!(OrderIntent, qty) == 24);
const _: () = assert!(offset_of!(OrderIntent, link) == 32);
const _: () = assert!(offset_of!(OrderIntent, tag) == 104);
const _: () = assert!(offset_of!(OrderIntent, max_rest_ms) == 108);

impl OrderIntent {
    /// `flags` bit: the order is reduce-only.
    pub const REDUCE_ONLY: u8 = 1;

    /// `flags` bit: BUY by spend amount (only a `Buy` with `Tif::Ioc`/`Tif::Fok` and no link,
    /// else the engine rejects the intent with `INVALID`): the order spends at most `px × qty` of
    /// quote and may receive more than `qty` when it fills below `px`; it ends when the venue says
    /// so. Same bit as hft-event `OrderFlags::SPEND`.
    pub const SPEND: u8 = 0x80;

    /// Whether [`OrderIntent::REDUCE_ONLY`] is set.
    pub const fn reduce_only(&self) -> bool {
        self.flags & Self::REDUCE_ONLY != 0
    }

    /// Whether [`OrderIntent::SPEND`] is set.
    pub const fn spend(&self) -> bool {
        self.flags & Self::SPEND != 0
    }

    /// A single-leg `Open` BUY by spend amount ([`OrderIntent::SPEND`]) at worst price `px`:
    /// spends at most `px × qty` of quote (`qty` from [`OrderIntent::spend_qty`]); `tif` must be
    /// `Ioc` or `Fok`.
    #[allow(clippy::too_many_arguments)]
    pub const fn spend_buy(
        venue: VenueId,
        account: AccountId,
        group: GroupId,
        inst: InstId,
        tif: Tif,
        px: Px,
        qty: Qty,
        tag: u32,
    ) -> OrderIntent {
        OrderIntent {
            venue,
            account,
            side: Side::Buy,
            tif,
            class: IntentClass::Open,
            flags: Self::SPEND,
            _pad: [0; 2],
            inst,
            group,
            px,
            qty,
            link: LegLinkOpt::NONE,
            tag,
            max_rest_ms: 0,
        }
    }

    /// The quantity whose cost at `px` is at most `quote` (a [`Notional`], exponent −6, in the
    /// inst's quote asset): `quote / px` rounded down to a multiple of `lot` (`lot ≤ 1` = no
    /// rounding). `Qty(0)` when `px ≤ 0`, `quote ≤ 0`, an exponent is out of range or the result
    /// does not fit.
    pub fn spend_qty(quote: Notional, px: Px, e: AssetExp, lot: i64) -> Qty {
        if px.0 <= 0 || quote.0 <= 0 {
            return Qty(0);
        }
        // qty = quote · 10^t / px with t = −6 − px_exp − qty_exp.
        let t = -6 - i32::from(e.px_exp) - i32::from(e.qty_exp);
        let Some(p) = 10i128.checked_pow(t.unsigned_abs()) else {
            return Qty(0);
        };
        let (num, den) = if t >= 0 {
            (i128::from(quote.0).checked_mul(p), Some(i128::from(px.0)))
        } else {
            (Some(i128::from(quote.0)), i128::from(px.0).checked_mul(p))
        };
        let (Some(num), Some(den)) = (num, den) else {
            return Qty(0);
        };
        let q = num.checked_div(den).unwrap_or(0);
        let q = if lot > 1 { q - q % i128::from(lot) } else { q };
        Qty(i64::try_from(q).unwrap_or(0))
    }
}

/// Handle of one order slot in hft-oms (12 B); `gen` distinguishes reuses of the same slot.
#[repr(C)]
#[derive(
    Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, TryFromBytes, IntoBytes, KnownLayout, Immutable,
)]
pub struct OrderRef {
    pub venue: VenueId,
    pub account: AccountId,
    pub _pad: [u8; 2],
    pub slot: u32,
    pub r#gen: u32,
}
const _: () = assert!(size_of::<OrderRef>() == 12);
const _: () = assert!(align_of::<OrderRef>() == 4);
const _: () = assert!(offset_of!(OrderRef, slot) == 4);
const _: () = assert!(offset_of!(OrderRef, r#gen) == 8);

impl OrderRef {
    pub const fn new(venue: VenueId, account: AccountId, slot: u32, r#gen: u32) -> OrderRef {
        OrderRef {
            venue,
            account,
            _pad: [0; 2],
            slot,
            r#gen,
        }
    }
}

/// Index of one `Ctx` operation in the operation sequence of an `on_event` call; resolves to an
/// [`IntentOutcome`] through `Ctx::outcome` during the next call only.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct IntentHandle(pub u16);

/// Why a target intent was rejected (hft-exec `set_target`). Discriminants equal
/// `ExecDecision.detail` table 1 (registered in hft-exec); append-only.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum TargetReject {
    NotOwner = 1,
    BadLegs = 2,
    SpecInvalid = 3,
    AlgoUnsupported = 4,
    SpotShort = 5,
    CellBroken = 6,
    ForcedReduceActive = 7,
    TooManyLegs = 8,
    FlattenLocked = 9,
    LotMismatch = 10,
    /// A target set while handling a `reduce_only` command would increase some leg's |position|.
    ReduceOnlyCmd = 11,
}

impl TargetReject {
    /// Wire code (`ExecDecision.detail` table 1 `code`).
    pub const fn code(self) -> u8 {
        self as u8
    }
    /// Inverse of [`TargetReject::code`]; unknown codes → `None`.
    pub const fn from_code(c: u8) -> Option<TargetReject> {
        Some(match c {
            1 => TargetReject::NotOwner,
            2 => TargetReject::BadLegs,
            3 => TargetReject::SpecInvalid,
            4 => TargetReject::AlgoUnsupported,
            5 => TargetReject::SpotShort,
            6 => TargetReject::CellBroken,
            7 => TargetReject::ForcedReduceActive,
            8 => TargetReject::TooManyLegs,
            9 => TargetReject::FlattenLocked,
            10 => TargetReject::LotMismatch,
            11 => TargetReject::ReduceOnlyCmd,
            _ => return None,
        })
    }
}

/// Engine result of one intent of the previous `on_event` call (`Ctx::outcome`).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum IntentOutcome {
    /// Order intent passed every gate and got an OMS slot.
    Placed(OrderRef),
    /// Order intent rejected by a gate (bits never contain `MODE_SHADOW`).
    Rejected(RejectBits),
    /// Order intent only recorded (effective mode Shadow, or Live with `open_allowed = false`).
    Shadowed,
    TargetAccepted,
    /// Target accepted while the cell is shadowed (`ExecDecision` carries SHADOW).
    TargetShadowed,
    TargetRejected(TargetReject),
}
