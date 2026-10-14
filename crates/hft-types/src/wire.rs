//! Cross-crate wire codes: `KillBits` (the single 32-bit kill-word layout), `RunMode`,
//! `KillLevel`, `CapSource`, `RejectBits`. Semantics belong to hft-risk.

/// Value stored in a gw's `KillWord` (`AtomicU32` in hft-rt).
#[repr(transparent)]
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
pub struct KillBits(pub u32);
const _: () = assert!(size_of::<KillBits>() == 4);
const _: () = assert!(align_of::<KillBits>() == 4);

impl KillBits {
    pub const GLOBAL: Self = KillBits(1 << 0);
    pub const VENUE: Self = KillBits(1 << 1);
    pub const HARD: Self = KillBits(1 << 2);
    /// Start of the account-kill field (bits 8..19).
    pub const ACCOUNT0: Self = KillBits(1 << 8);
    /// Start of the account-HARD field (bits 20..31).
    pub const ACCOUNT_HARD0: Self = KillBits(1 << 20);
    pub const ACCOUNT_MASK: Self = KillBits(0x000F_FF00);
    pub const ACCOUNT_HARD_MASK: Self = KillBits(0xFFF0_0000);
    /// Live accounts per venue gw (refdata load check).
    pub const MAX_ACCOUNTS_PER_GW: u8 = 12;

    /// `1 << (8 + ix)`; `ix ≥ 12` → debug_assert, release returns `KillBits(0)`.
    #[inline]
    pub const fn account(ix: u8) -> Self {
        debug_assert!(ix < Self::MAX_ACCOUNTS_PER_GW, "account index out of range");
        if ix >= Self::MAX_ACCOUNTS_PER_GW {
            return KillBits(0);
        }
        KillBits(1u32.wrapping_shl(8u32.wrapping_add(ix as u32)))
    }

    /// `1 << (20 + ix)`; `ix ≥ 12` → debug_assert, release returns `KillBits(0)`.
    #[inline]
    pub const fn account_hard(ix: u8) -> Self {
        debug_assert!(ix < Self::MAX_ACCOUNTS_PER_GW, "account index out of range");
        if ix >= Self::MAX_ACCOUNTS_PER_GW {
            return KillBits(0);
        }
        KillBits(1u32.wrapping_shl(20u32.wrapping_add(ix as u32)))
    }

    /// `GLOBAL | VENUE | HARD | account(ix) | account_hard(ix)`.
    #[inline]
    pub const fn gw_mask(ix: u8) -> Self {
        KillBits(
            Self::GLOBAL.0 | Self::VENUE.0 | Self::HARD.0 | Self::account(ix).0 | Self::account_hard(ix).0,
        )
    }

    /// `HARD | account_hard(ix)`.
    #[inline]
    pub const fn hard_mask(ix: u8) -> Self {
        KillBits(Self::HARD.0 | Self::account_hard(ix).0)
    }
}

/// Strategy run mode (2 retired, never reused).
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
pub enum RunMode {
    Shadow = 1,
    CloseOnly = 3,
    Live = 4,
}
const _: () = assert!(size_of::<RunMode>() == 1);

/// Kill command level (1 retired, never reused); HARD is a separate command bit.
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
pub enum KillLevel {
    Venue = 2,
    Account = 3,
    Global = 4,
}
const _: () = assert!(size_of::<KillLevel>() == 1);

/// Source of a mode cap (scopes, setters and release conditions belong to hft-risk).
#[repr(transparent)]
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
pub struct CapSource(pub u8);

impl CapSource {
    pub const KILL: Self = CapSource(1);
    pub const DAILY_LOSS: Self = CapSource(2);
    pub const RECON: Self = CapSource(3);
    pub const UNKNOWN_TIMEOUT: Self = CapSource(4);
    pub const BROKEN: Self = CapSource(5);
    /// Optional chain leg.
    pub const REORG: Self = CapSource(6);
    pub const RECOVERY: Self = CapSource(7);
    pub const HALTED: Self = CapSource(8);
    /// Not clearable.
    pub const DRAINING: Self = CapSource(9);
    /// Conditional, released automatically.
    pub const CLOCK_BAD: Self = CapSource(10);
    /// Account: any group Broken.
    pub const HEDGE_FAILURE: Self = CapSource(11);
    /// Conditional: margin tier.
    pub const MARGIN_STRESS: Self = CapSource(12);
    /// Conditional: persist/disk degraded.
    pub const STORAGE: Self = CapSource(13);
    /// (venue, inst): `VenueNotice{Delisting}`.
    pub const DELISTING: Self = CapSource(14);
    /// Conditional: account config disagrees with `VenueCaps`.
    pub const ACCOUNT_CFG: Self = CapSource(15);

    /// `Resume{source}` accepts only 1..=8, 11 and 14.
    #[inline]
    pub const fn resumable(self) -> bool {
        matches!(self.0, 1..=8 | 11 | 14)
    }
}

/// Risk reject bits (`OrderDecision/SwapDecision.reject`); bit numbers are append-only.
#[repr(transparent)]
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
pub struct RejectBits(pub u32);
const _: () = assert!(size_of::<RejectBits>() == 4);

impl RejectBits {
    pub const INVALID: Self = RejectBits(1 << 0);
    pub const NOT_TRADABLE: Self = RejectBits(1 << 1);
    pub const VENUE_NOT_READY: Self = RejectBits(1 << 2);
    pub const KILLED: Self = RejectBits(1 << 3);
    pub const CLOSE_ONLY: Self = RejectBits(1 << 4);
    pub const HARD: Self = RejectBits(1 << 5);
    pub const UNKNOWN_PENDING: Self = RejectBits(1 << 6);
    pub const NO_MID: Self = RejectBits(1 << 7);
    /// Quote unavailable: the inst's BBO connection is unhealthy or resyncing.
    pub const STALE_MID: Self = RejectBits(1 << 8);
    pub const PX_BAND: Self = RejectBits(1 << 9);
    pub const NOTIONAL_ORDER: Self = RejectBits(1 << 10);
    pub const NOTIONAL_GROUP: Self = RejectBits(1 << 11);
    pub const NOTIONAL_ACCOUNT: Self = RejectBits(1 << 12);
    pub const NOTIONAL_VENUE: Self = RejectBits(1 << 13);
    pub const POSITION: Self = RejectBits(1 << 14);
    pub const REDUCE_INVALID: Self = RejectBits(1 << 15);
    pub const RATE: Self = RejectBits(1 << 16);
    pub const RATE_BACKOFF: Self = RejectBits(1 << 17);
    pub const DAILY_LOSS: Self = RejectBits(1 << 18);
    pub const EXPOSURE_GATE: Self = RejectBits(1 << 19);
    pub const RECON_DEVIATION: Self = RejectBits(1 << 20);
    // 1 << 21 retired, never reused.
    pub const CLOCK: Self = RejectBits(1 << 22);
    pub const DRAINING: Self = RejectBits(1 << 23);
    /// Not a rejection: the effective mode only records the intent.
    pub const MODE_SHADOW: Self = RejectBits(1 << 24);
    /// `SendReport` confirmed the frame was not written.
    pub const LOCAL_SEND_FAIL: Self = RejectBits(1 << 25);
    /// Hedge/repair quantity or notional above its limit.
    pub const HEDGE_LIMIT: Self = RejectBits(1 << 26);
    /// The intent's account is not bound to the issuing strategy instance.
    pub const NOT_BOUND: Self = RejectBits(1 << 27);
    /// Linked open: the second leg's BBO connection is unhealthy.
    pub const LINK_STALE: Self = RejectBits(1 << 28);
    /// Linked open: the second leg cannot fund the link.
    pub const LINK_FUNDS: Self = RejectBits(1 << 29);
    /// OMS order or link slots are exhausted; no order was created or sent.
    pub const CAPACITY: Self = RejectBits(1 << 30);
}
