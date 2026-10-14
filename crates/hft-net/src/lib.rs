//! `hft-net`: every byte of a connection from the socket to the WS frame — non-blocking TCP and
//! its options, the TLS handshake (rustls, warm thread) and the in-place record layer after it,
//! WS framing and control frames, `recvmsg` receive stamps and `rx_empty_gap_ns`, the `Connector`
//! (conn-builder), `ConnArena`/`ConnPool` hand-off and `ReadyPoll`.
//!
//! It knows no exchange protocol, writes no journal and creates no thread. Hot entries
//! (`poll_rx`, `next_frame`, `send_frame`, `finish_poll`, `ReadyPoll::probe`) make no allocation
//! and only the system calls of hot-path-rules T7. Linux-only system calls compile everywhere;
//! off Linux they fail with `ENOSYS`. Feature `test-io` replaces sockets by scripts (tests).
//! Specification: `crates/hft-net/README.md`.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented,
    clippy::undocumented_unsafe_blocks
)]

mod arena;
pub mod cmsg;
mod config;
mod conn;
mod connector;
mod pool;
mod ready;
pub mod rest;
pub mod sock;
mod tls;
mod types;
pub mod ws;

pub use arena::{ConnArena, Handoff, RetireMsg};
pub use config::{ConnCfg, Endpoint, NetConfig, Quickack, RoleBytes, TlsMode};
pub use conn::{Conn, Frame, TEXT_CAP};
pub use connector::{
    BuildErr, BuildEvent, BuildFail, BuildReq, Connector, JobId, MAX_JOBS, TICKET_WAIT_NS, base64, ws_accept,
};
/// Connection role: the single owner is hft-event (event-model); re-exported.
pub use hft_event::ConnRole;
/// Connection number (hft-types).
pub use hft_types::ConnId;
pub use pool::{ActiveIter, BuildWhy, ConnPool, PoolErr, PoolMode, RotatePolicy};
pub use ready::{ReadyHandle, ReadyPoll};
pub use sock::SysCount;
pub use tls::ConfigErr;
pub use types::{
    ConnKey, ConnPhase, ConnSet, ConnStats, EndpointId, ErrBits, NetErr, Opcode, RttHist, SendErr, SendOk,
    Timestamps, TlsInfo, TsFlags, TxStamp, TxTag,
};
pub use ws::MaskedFrame;

/// The TLS client configuration `Connector` uses (tests drive scripted handshakes with it).
#[cfg(feature = "test-io")]
#[doc(hidden)]
pub fn client_tls_config(extra_ca: Option<&[u8]>) -> Result<rustls::ClientConfig, ConfigErr> {
    tls::client_config(extra_ca)
}

/// Field offsets for layout tests (`MaskedFrame` fields are private).
#[doc(hidden)]
pub mod layout {
    use crate::ws::MaskedFrame;

    /// `start`, `len`, `opcode`, `sealed`, `_pad`, `buf` of `MaskedFrame<2048>`.
    pub fn masked_frame_offsets() -> [usize; 6] {
        crate::ws::masked_frame_offsets::<2048>()
    }

    pub fn masked_frame_size<const C: usize>() -> usize {
        size_of::<MaskedFrame<C>>()
    }
}
