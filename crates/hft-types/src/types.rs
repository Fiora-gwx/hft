//! Value types, ID newtypes, trading enums and their `const fn` helpers.

use core::ops::{Add, Neg, Sub};

/// Per-asset decimal exponents (`raw × 10^exp`); range `[-18, 6]` is checked by hft-refdata.
#[repr(C)]
#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Hash,
    Debug,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
pub struct AssetExp {
    pub px_exp: i8,
    pub qty_exp: i8,
}
const _: () = assert!(size_of::<AssetExp>() == 2);
const _: () = assert!(align_of::<AssetExp>() == 1);
const _: () = assert!(core::mem::offset_of!(AssetExp, px_exp) == 0);
const _: () = assert!(core::mem::offset_of!(AssetExp, qty_exp) == 1);

/// Price in quote currency per asset unit, exponent `px_exp` of the asset; may be negative.
#[repr(transparent)]
#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Debug,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
pub struct Px(pub i64);

/// Quantity in asset units, exponent `qty_exp` of the asset; signed (positions).
#[repr(transparent)]
#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Debug,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
pub struct Qty(pub i64);

/// Notional in the inst's quote asset, fixed exponent -6.
#[repr(transparent)]
#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Debug,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
pub struct Notional(pub i64);

/// Dimensionless ratio, fixed exponent -9 (1 bp = 100_000).
#[repr(transparent)]
#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Debug,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
pub struct Rate(pub i64);

impl Rate {
    /// Sentinel for ratios whose denominator is ≤ 0.
    pub const MAX: Rate = Rate(i64::MAX);
}

/// Upper bound (inclusive) of `|raw|` for every parsed or constructed `Px`/`Qty`.
pub const MAX_RAW: i64 = (1 << 62) - 1;

macro_rules! same_type_ops {
    ($t:ident) => {
        impl Add for $t {
            type Output = $t;
            #[inline]
            fn add(self, rhs: $t) -> $t {
                $t(self.0.saturating_add(rhs.0))
            }
        }
        impl Sub for $t {
            type Output = $t;
            #[inline]
            fn sub(self, rhs: $t) -> $t {
                $t(self.0.saturating_sub(rhs.0))
            }
        }
        impl Neg for $t {
            type Output = $t;
            #[inline]
            fn neg(self) -> $t {
                $t(self.0.saturating_neg())
            }
        }
    };
}
same_type_ops!(Px);
same_type_ops!(Qty);

/// Rounding direction for every fixed-point operation that drops digits.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum RoundMode {
    Floor = 1,
    Ceil = 2,
    TowardZero = 3,
    HalfEven = 4,
}
const _: () = assert!(size_of::<RoundMode>() == 1);

/// Order side (wire enum: 0 and unknown values fail `TryFromBytes`).
#[repr(u8)]
#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Hash,
    Debug,
    zerocopy::TryFromBytes,
    zerocopy::IntoBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
pub enum Side {
    Buy = 1,
    Sell = 2,
}
const _: () = assert!(size_of::<Side>() == 1);

/// Time in force (wire enum).
#[repr(u8)]
#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Hash,
    Debug,
    zerocopy::TryFromBytes,
    zerocopy::IntoBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
pub enum Tif {
    Gtc = 1,
    Ioc = 2,
    Fok = 3,
    PostOnly = 4,
}
const _: () = assert!(size_of::<Tif>() == 1);

/// Market segment of a `VenueId` (wire enum).
#[repr(u8)]
#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Hash,
    Debug,
    zerocopy::TryFromBytes,
    zerocopy::IntoBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
pub enum MarketKind {
    Spot = 1,
    LinearPerp = 2,
}
const _: () = assert!(size_of::<MarketKind>() == 1);

/// Host class: config `host.class` and ECF1 header `host_class` (wire enum, append-only).
#[repr(u8)]
#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Hash,
    Debug,
    zerocopy::TryFromBytes,
    zerocopy::IntoBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
pub enum HostClass {
    Dev2 = 1,
    C4 = 2,
    C8 = 3,
    Metal = 4,
    C16 = 5,
    C32 = 6,
}
const _: () = assert!(size_of::<HostClass>() == 1);

impl Side {
    /// Buy → Floor, Sell → Ceil: limit prices round towards "not worse".
    #[inline]
    pub const fn price_round(self) -> RoundMode {
        match self {
            Side::Buy => RoundMode::Floor,
            Side::Sell => RoundMode::Ceil,
        }
    }

    /// Buy +1, Sell -1.
    #[inline]
    pub const fn sign(self) -> i64 {
        match self {
            Side::Buy => 1,
            Side::Sell => -1,
        }
    }

    #[inline]
    pub const fn opposite(self) -> Side {
        match self {
            Side::Buy => Side::Sell,
            Side::Sell => Side::Buy,
        }
    }
}

/// Instrument id (refdata, dense from 1; 0 = none/all).
#[repr(transparent)]
#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Debug,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
pub struct InstId(pub u32);

/// Asset id (refdata).
#[repr(transparent)]
#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Debug,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
pub struct AssetId(pub u16);

/// Venue (product line) id; constants in `venue_ids`.
#[repr(transparent)]
#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Debug,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
pub struct VenueId(pub u8);

/// Logical account id (refdata `AccountMeta`).
#[repr(transparent)]
#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Debug,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
pub struct AccountId(pub u8);

/// Strategy instance id, valid `1..=254`.
#[repr(transparent)]
#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Debug,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
pub struct StrategyId(pub u8);

/// Group id = `(StrategyId << 24) | n`; 0 = none, `u32::MAX` = all.
#[repr(transparent)]
#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Debug,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
pub struct GroupId(pub u32);

/// OMS slot index per (venue, account).
#[repr(transparent)]
#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Debug,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
pub struct SlotId(pub u32);

/// Connection id within a venue runtime.
#[repr(transparent)]
#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Debug,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
pub struct ConnId(pub u8);

/// Journal sequence number.
#[repr(transparent)]
#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Debug,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
pub struct Seq(pub u64);

/// TSC reading (same host, same boot only).
#[repr(transparent)]
#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Debug,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
pub struct Tsc(pub u64);

/// Same identity as `StrategyId` (a strategy instance).
pub type StrategyInstanceId = StrategyId;

pub const INST_NONE: InstId = InstId(0);
pub const INST_ALL: InstId = InstId(0);
pub const ASSET_NONE: AssetId = AssetId(0);
pub const GROUP_NONE: GroupId = GroupId(0);
pub const GROUP_ALL: GroupId = GroupId(u32::MAX);
pub const ACCOUNT_ALL: AccountId = AccountId(0);

/// BBO connection slots per venue (connection id = slot, `0..MAX_BBO_CONN_SLOTS`).
pub const MAX_BBO_CONN_SLOTS: u8 = 8;

impl SlotId {
    /// A report that could not be resolved to a slot.
    pub const UNRESOLVED: SlotId = SlotId(0);
}

impl GroupId {
    /// Upper bound of the per-instance counter.
    pub const N_MAX: u32 = 0xFF_FFFE;

    /// `s ∈ 1..=254` and `n ∈ 1..=N_MAX` → `Some((s << 24) | n)`, otherwise `None`.
    #[inline]
    pub const fn compose(s: StrategyId, n: u32) -> Option<GroupId> {
        if s.0 == 0 || s.0 == u8::MAX || n == 0 || n > Self::N_MAX {
            return None;
        }
        Some(GroupId(((s.0 as u32) << 24) | n))
    }

    /// High 8 bits.
    #[inline]
    pub const fn strategy(self) -> StrategyId {
        StrategyId((self.0 >> 24) as u8)
    }

    /// Low 24 bits.
    #[inline]
    pub const fn counter(self) -> u32 {
        self.0 & 0x00FF_FFFF
    }
}

/// Hedge pricing limits shared by risk and OMS (semantics owned by hft-risk).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct HedgePxLimits {
    /// Quote-connection health threshold in ms.
    pub conn_liveness_ms: u32,
    /// Max cross of a risk-reducing order relative to mid.
    pub rr_max_cross: Rate,
}
