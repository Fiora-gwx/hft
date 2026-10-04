//! `RecordHeader` (24 B), `RecordFlags` and `record_len`.
// zerocopy's derive helpers inside `event_body!` expansions carry no Debug impl.
#![allow(missing_debug_implementations)]

use crate::bytes::round8;
use crate::macros::layout;

crate::event_body! {
    /// Header of every journal record (event-model "记录头"); `len` includes header, body,
    /// optional CRC and zero padding to 8 B.
    #[repr(align(8))]
    pub struct RecordHeader : plain, FromBytes {
        pub len: u32,
        pub kind: u16,
        pub flags: u16,
        pub seq: u64,
        pub tsc: u64,
    }
}
layout!(RecordHeader, 24, 8; len @ 0, kind @ 4, flags @ 6, seq @ 8, tsc @ 16);

/// Record flag bits (unknown bits are written 0 and ignored by readers).
#[repr(transparent)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct RecordFlags(pub u16);

impl RecordFlags {
    /// Same-inst BBO backlog conflated (replay does the same).
    pub const CONFLATED: Self = RecordFlags(1 << 0);
    /// Intent recorded under Shadow, not sent.
    pub const SHADOW: Self = RecordFlags(1 << 1);
    /// `hw_rx_ns` from the NIC.
    pub const HW_TS: Self = RecordFlags(1 << 2);
    /// Fell back to a software timestamp.
    pub const SW_TS: Self = RecordFlags(1 << 3);
    /// Non-final part of a multi-part record.
    pub const PART: Self = RecordFlags(1 << 4);
    /// The last 4 bytes are a CRC32C over `[0, len − 4)`.
    pub const CRC: Self = RecordFlags(1 << 5);
    /// Last record of one input event (journal-format commit cursor).
    pub const EVENT_END: Self = RecordFlags(1 << 6);

    #[inline]
    pub const fn contains(self, o: Self) -> bool {
        self.0 & o.0 == o.0
    }

    #[inline]
    pub const fn union(self, o: Self) -> Self {
        RecordFlags(self.0 | o.0)
    }
}

impl core::ops::BitOr for RecordFlags {
    type Output = Self;
    #[inline]
    fn bitor(self, o: Self) -> Self {
        self.union(o)
    }
}

impl core::ops::BitOrAssign for RecordFlags {
    #[inline]
    fn bitor_assign(&mut self, o: Self) {
        self.0 |= o.0;
    }
}

/// Record length for a body of `body_len` bytes: `24 + body (+ 4 CRC)` rounded up to 8
/// (saturates at `u32::MAX` for absurd inputs).
#[inline]
pub const fn record_len(body_len: usize, crc: bool) -> u32 {
    let raw = 24usize
        .saturating_add(body_len)
        .saturating_add(if crc { 4 } else { 0 });
    let n = round8(raw);
    if n > u32::MAX as usize { u32::MAX } else { n as u32 }
}
