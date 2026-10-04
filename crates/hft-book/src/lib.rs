//! `hft-book`: per-inst dense-array L2 books and the BBO cache owned by the sequencer.
//!
//! `L2Book` applies already-normalised snapshots and increments (generic `prev_id == last_id`
//! check, de-duplication, structural checks), keeps the best levels through an occupancy
//! bitmap, holds levels outside the window in a bounded overflow area and exports keyframes.
//! `Top` caches the best level of the `Bbo` stream; `BookSet` is the dense per-inst set and
//! writes/restores snapshot sections 6 (`BookMeta`) and 7 (`Tops`).
//!
//! No allocation after construction, no system calls, no clocks; nothing panics on any input.
//! Specification: `crates/hft-book/README.md`.
#![forbid(unsafe_code)]
#![deny(
    clippy::float_arithmetic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

#[cfg(target_endian = "big")]
compile_error!("hft-book: snapshot rows are little-endian only");

mod bitmap;
mod export;
mod l2;
mod overflow;
mod recenter;
mod set;
mod snapshot;
mod top;

use hft_refdata::InstHot;

pub use export::LevelIter;
pub use l2::L2Book;
pub use set::BookSet;
pub use snapshot::{BookMetaRow, ROW_VERSION, SECTION_BOOK_META, SECTION_TOPS, TopRow};
pub use top::{Top, TopOutcome};

/// Book shape: price step, window half-width W (window length L = 2W rounded up to 64 ticks),
/// overflow capacity per side and the number of consecutive crossed messages tolerated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BookParams {
    pub tick: i64,
    pub window: u32,
    pub ovf_cap: u16,
    pub crossed_tolerance: u16,
}

impl BookParams {
    /// Default overflow capacity per side.
    pub const OVF_CAP_DEFAULT: u16 = 64;

    /// `tick`, `window = book_window` and `crossed_tolerance` from refdata; `ovf_cap` = 64.
    pub fn from_inst(h: &InstHot) -> BookParams {
        BookParams {
            tick: h.tick,
            window: h.book_window,
            ovf_cap: Self::OVF_CAP_DEFAULT,
            crossed_tolerance: h.crossed_tolerance,
        }
    }

    /// `tick > 0`, `window ∈ [64, 65536]`, `ovf_cap > 0`.
    pub fn is_valid(&self) -> bool {
        self.tick > 0 && (64..=65_536).contains(&self.window) && self.ovf_cap > 0
    }
}

/// Construction errors.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BookErr {
    /// `mem.len() < bytes_needed`.
    MemTooSmall = 1,
    /// `mem` does not start on a 64 B boundary.
    MemAlign = 2,
    /// `tick ≤ 0`, `window ∉ [64, 65536]` or `ovf_cap = 0`.
    BadParams = 3,
    /// `l2_insts` names an inst refdata does not have.
    UnknownInst = 4,
    /// `l2_insts` lists an inst twice.
    DupInst = 5,
}

/// Book state machine (README "状态机").
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BookState {
    Uninit = 0,
    Live = 1,
    Stale = 2,
    Resyncing = 3,
}

impl BookState {
    pub fn from_u8(v: u8) -> Option<BookState> {
        match v {
            0 => Some(BookState::Uninit),
            1 => Some(BookState::Live),
            2 => Some(BookState::Stale),
            3 => Some(BookState::Resyncing),
            _ => None,
        }
    }
}

/// Why a book left `Live`.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StaleReason {
    Gap = 1,
    Crossed = 2,
    ConnDown = 3,
    Split = 4,
    BadLevel = 5,
    Refdata = 6,
}

impl StaleReason {
    /// `None` for 0 ("no reason") and unknown codes.
    pub fn from_u8(v: u8) -> Option<StaleReason> {
        match v {
            1 => Some(StaleReason::Gap),
            2 => Some(StaleReason::Crossed),
            3 => Some(StaleReason::ConnDown),
            4 => Some(StaleReason::Split),
            5 => Some(StaleReason::BadLevel),
            6 => Some(StaleReason::Refdata),
            _ => None,
        }
    }
}

/// Result bits of every mutating `L2Book` call.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct Outcome(pub u16);

impl Outcome {
    pub const NONE: Outcome = Outcome(0);
    pub const APPLIED: Outcome = Outcome(1);
    /// The call changed the price or quantity of the best bid.
    pub const BID_TOP: Outcome = Outcome(2);
    /// The call changed the price or quantity of the best ask.
    pub const ASK_TOP: Outcome = Outcome(4);
    pub const DUPLICATE: Outcome = Outcome(8);
    pub const NOT_LIVE: Outcome = Outcome(16);
    pub const GAP: Outcome = Outcome(32);
    pub const CROSSED: Outcome = Outcome(64);
    pub const TRUNCATED: Outcome = Outcome(128);
    pub const RECENTERED: Outcome = Outcome(256);
    pub const INVALID: Outcome = Outcome(512);
    /// The call made the book leave `Live`.
    pub const WENT_STALE: Outcome = Outcome(1024);
    /// The call moved the book from a non-`Live` state to `Live`.
    pub const WENT_LIVE: Outcome = Outcome(2048);

    /// All bits of `o` are set.
    #[inline]
    pub const fn contains(self, o: Outcome) -> bool {
        self.0 & o.0 == o.0
    }
    #[inline]
    pub const fn bits(self) -> u16 {
        self.0
    }
    #[inline]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl core::ops::BitOr for Outcome {
    type Output = Outcome;
    #[inline]
    fn bitor(self, o: Outcome) -> Outcome {
        Outcome(self.0 | o.0)
    }
}

impl core::ops::BitOrAssign for Outcome {
    #[inline]
    fn bitor_assign(&mut self, o: Outcome) {
        self.0 |= o.0;
    }
}

impl core::ops::BitAnd for Outcome {
    type Output = Outcome;
    #[inline]
    fn bitand(self, o: Outcome) -> Outcome {
        Outcome(self.0 & o.0)
    }
}
