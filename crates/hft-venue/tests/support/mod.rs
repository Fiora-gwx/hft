//! Fixtures of the hft-venue tests (TESTS.md "夹具"): `StubProto`, `ScriptNet`/`BuildStub`, `Taps`,
//! `rd_stub()`, raw journals. `MemPeer` and the WS reference come from hft-net's test fixtures.
#![allow(dead_code, unused_imports)]

#[path = "../../../hft-net/tests/support/mempeer.rs"]
pub mod mempeer;
#[path = "../../../hft-net/tests/support/ws_ref.rs"]
pub mod ws_ref;

pub mod feedh;
pub mod gwh;
pub mod jrn;
#[cfg(target_os = "linux")]
#[path = "../../../hft-net/tests/support/looppeer.rs"]
pub mod looppeer;
pub mod net;
pub mod rd;
pub mod stub;
