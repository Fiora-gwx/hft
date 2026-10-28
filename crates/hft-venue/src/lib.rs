//! `hft-venue`: the generic venue runtime — feed tasks (`FeedRuntime`: BBO race dedup and BBO
//! connection health, TopN depth, the other market streams, raw-feed), the gw task (`GwRuntime`:
//! `drain_out` with kill check, rate backstop, encode, send and `SendReport`; slot mirror and client
//! ids; reports into the rep ring), `SessionDriver` on the warm `conn-builder` thread and
//! `kill_admit`. It runs a `VenueProtocol` (hft-venue-proto) on hft-net connections; it creates no
//! thread, changes no order state and knows no exchange.
//!
//! M1.5 part (README "已知限制与后续工作"): the REST work of snap-fetch, acct-fetch and the gw
//! (BBO refresh requests, account queries, REST order operations, dead-man REST, session
//! keepalive), the unmatched-fill hold, the dedup-ring reseed, private-channel ownership and the
//! optional incremental depth arrive with M3.
//! Specification: `crates/hft-venue/README.md`.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

pub mod account;
mod acct;
mod acct_fetch;
mod acct_wire;
mod latency;
mod private;
mod rest;
pub use private::prepare as prepare_private;
pub mod codes;
mod config;
mod conn_id;
mod feed;
mod funding;
mod gw;
mod hb;
mod hold;
mod idtable;
mod kill;
pub mod order_rest;
mod rate;
mod raw;
mod seq;
mod session;
mod sink;
mod snap;
mod snap_pool;

pub use acct::{AcctReq, AcctSched, AcctState};
pub use acct_fetch::{AcctAccount, AcctFetchTask, AcctResponse, AcctTransport};
pub use acct_wire::{AcctFail, AcctRecHdr};
pub use config::{
    DepthMode, FeedCfg, FeedCmdDest, FeedEndpoints, FeedRoutes, GwCfg, InitErr, RawMode, ReadyPollMode,
    SnapCfg, feed_cmd_dest, ready_poll_on,
};
pub use conn_id::{
    ACCT_FIRST, ACCT_LAST, BboSlots, GW_FIRST, GW_LAST, IdRange, MD_FIRST, MD_LAST, SNAP_FIRST, SNAP_LAST,
};
pub use feed::{FeedCounters, FeedDeps, FeedRuntime};
pub use funding::{FundingPoll, FundingPollCfg, FundingPollErr, write_funding_record};
pub use gw::{GwAccountSpec, GwCounters, GwDeps, GwRuntime, SlotMirror};
pub use idtable::{IdCounters, IdEntry, IdErr, IdTable, exch_id_value};
pub use kill::kill_admit;
pub use rate::RateCounters;
pub use raw::{Effect, RawCounters, RawLog, what as raw_what};
pub use rest::{AcctConnections, AcctRestPools};
pub use seq::{InstSeq, Verdict, judge};
pub use session::{SessErr, SessEvent, SessionCfg, SessionDriver, SessionOp};
pub use snap::{SnapDeps, SnapFetchTask, SnapRecHdr, SnapReq};
pub use snap_pool::{PooledSnapFetchTask, SnapConnections};

/// Max `FeedCmd`s taken per `poll_once`.
pub const FEEDCMD_PER_POLL: usize = 8;
/// Max events one frame of a non-Bbo route may produce; more → the whole frame is a decode error.
pub const FRAME_EVENTS_MAX: usize = 64;
/// Max BBO refresh requests snap-fetch merges into one REST request (also `P::BBO_SNAPSHOT_BATCH`).
pub const SNAP_BBO_BATCH_MAX: usize = 32;
/// gw REST endpoint pools per account.
pub const MAX_REST_ENDPOINTS: usize = 4;
/// Most connections of one gw REST pool (`gw.rest_conns`).
pub const GW_REST_CONNS_MAX: usize = 16;
/// Cancels per account waiting for a free gw REST connection (README §4 "撤单优先").
pub const GW_REST_CANCEL_QUEUE: usize = 32;
/// A gw REST pool: `GW_REST_CONNS_MAX` lanes, `gw.rest_conns` of them used.
pub type GwRestPool = hft_net::RestPool<GW_REST_CONNS_MAX>;
/// Largest chunk of one acct record's raw response.
pub const ACCT_RAW_CHUNK: usize = 16_384;

/// `FeedCmd.streams` bits (equal to the SDK `EvMask` values).
pub mod streams {
    pub const BBO: u8 = 1;
    pub const BOOK: u8 = 2;
    pub const TRADE: u8 = 4;
    pub const FUNDING: u8 = 8;
    pub const MARK: u8 = 16;
    pub const INDEX: u8 = 32;
    pub const LIQUIDATION: u8 = 64;
    /// Everything carried by feed-md (Md and DepthTopN routes).
    pub const MD_ALL: u8 = BOOK | TRADE | FUNDING | MARK | INDEX | LIQUIDATION;
}

/// One `poll_once`'s work. `did_work` = any frame, command, record, connection event or timed
/// action; a round that stopped on a full output ring is idle (`did_work = false`).
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct LoopStat {
    pub did_work: bool,
    pub frames: u16,
    pub cmds: u16,
    pub ring_full: u16,
    /// Earliest heartbeat / retry / timeout TSC.
    pub next_deadline: Option<u64>,
}

/// conn-builder → task hand-over ring element: an open connection, or the failure of the build the
/// task asked for (the task frees the connection number and builds again).
#[derive(Copy, Clone, Debug)]
pub enum Handover {
    Ready(hft_net::Handoff),
    Failed(hft_net::ConnId),
}

pub(crate) fn min_deadline(a: Option<u64>, b: u64) -> Option<u64> {
    Some(a.map_or(b, |x| x.min(b)))
}
