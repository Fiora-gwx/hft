//! Test kinds `0x7F01`–`0x7F06` (feature `dev-kinds`; dev-dependencies only). Bodies are opaque.
// zerocopy's derive helpers inside `event_body!` expansions carry no Debug impl.
#![allow(missing_debug_implementations)]

use crate::EventKind;

crate::event_body! {
    /// `TIn` (0x7F01, Input).
    pub struct TIn : fixed(EventKind::T_IN, 1), FromBytes { pub bytes: [u8; 80] }
}
crate::event_body! {
    /// `TOut` (0x7F02, Output).
    pub struct TOut : fixed(EventKind::T_OUT, 1), FromBytes { pub bytes: [u8; 56] }
}
crate::event_body! {
    /// `TMeta` (0x7F03, Meta).
    pub struct TMeta : fixed(EventKind::T_META, 1), FromBytes { pub bytes: [u8; 32] }
}
crate::event_body! {
    /// `TCrc` (0x7F04, Input, CRC always).
    pub struct TCrc : fixed(EventKind::T_CRC, 1), FromBytes { pub bytes: [u8; 40] }
}
crate::event_body! {
    /// `TVar` (0x7F06, Var) prefix: first 4 bytes = tail length L.
    pub struct TVarHdr : var(EventKind::T_VAR, 1, 65_000), FromBytes { pub len: u32, pub bytes: [u8; 12] }
}
