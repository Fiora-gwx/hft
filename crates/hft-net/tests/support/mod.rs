//! Fixtures shared by the hft-net tests (TESTS.md "夹具"): `ScriptIo` lives in the crate behind
//! feature `test-io`; `MemPeer`, `LoopPeer` (Linux), the WS reference and the stamp model are here.
#![allow(dead_code, unused_imports)]

pub mod mempeer;
pub mod stamp_model;
pub mod ws_ref;
pub mod xorshift;

#[cfg(target_os = "linux")]
pub mod looppeer;
