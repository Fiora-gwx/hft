//! `FxErr`: bit flags returned by fixed-point parsing and arithmetic.

use core::ops::{BitOr, BitOrAssign};

/// Error bits; OR several together and test once (`OK` = no error).
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
pub struct FxErr(pub u8);

impl FxErr {
    pub const OK: FxErr = FxErr(0);
    pub const SYNTAX: FxErr = FxErr(1);
    pub const EMPTY: FxErr = FxErr(2);
    pub const PRECISION: FxErr = FxErr(4);
    pub const OVERFLOW: FxErr = FxErr(8);
    pub const NEGATIVE: FxErr = FxErr(16);
    pub const DIV_ZERO: FxErr = FxErr(32);
    pub const EXP_RANGE: FxErr = FxErr(64);
}

impl BitOr for FxErr {
    type Output = FxErr;
    #[inline]
    fn bitor(self, rhs: FxErr) -> FxErr {
        FxErr(self.0 | rhs.0)
    }
}

impl BitOrAssign for FxErr {
    #[inline]
    fn bitor_assign(&mut self, rhs: FxErr) {
        self.0 |= rhs.0;
    }
}

const _: () = assert!(size_of::<FxErr>() == 1);
