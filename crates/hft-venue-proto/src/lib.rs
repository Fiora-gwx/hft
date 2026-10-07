//! `hft-venue-proto`: the `VenueProtocol` trait and the pure types in its signatures.
//!
//! This build carries the M1.5 part of the README: `RxMeta` (fixed 48 B layout), the market-data
//! decode surface of the trait (`NAME`, `VENUE`, `MARKET`, `MdClass`, `FeedCtx`, `classify`,
//! `decode_md`), `MdSink`, and under feature `testkit` the `VecSink` recorder and the contract-3
//! part of `check_md_basics`. The session, order, account and read-only query members, the
//! remaining helper types, dedup and code encoding arrive with M3 (README "已知限制与后续工作").
//! No I/O, no threads, no global state. Specification: `crates/hft-venue-proto/README.md`.
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

mod sink;
mod types;

#[cfg(feature = "testkit")]
pub mod testkit;

pub use sink::MdSink;
pub use types::{DecodeErr, MdOutcome, MsgOp, Role, RouteId, RxMeta};

use hft_refdata::RefData;
use hft_types::{MarketKind, VenueId};

/// One venue product line's protocol (static dispatch; README "`VenueProtocol`(静态分派)").
/// M1.5 part: identity constants and the market-data decode path.
pub trait VenueProtocol: Sized + 'static {
    /// Venue name, e.g. `"binance-usdm"` (hft-core `venues.<v>.protocol`, `fixtures/<NAME>/`).
    const NAME: &'static str;
    /// Product-line `VenueId`.
    const VENUE: VenueId;
    /// `Spot` or `LinearPerp`.
    const MARKET: MarketKind;
    /// Market-data dispatch result (venue-private enum).
    type MdClass: Copy + Eq;
    /// Venue-private feed state (never caches refdata, contract 25 / "实现说明").
    type FeedCtx;

    /// Dispatch from the first 64 B of a frame; `route` = the connection's route.
    fn classify(route: RouteId, op: MsgOp, head: &[u8]) -> Self::MdClass;

    /// Decodes one market-data message into `out`. Contract 3: the receive time points of `rx`
    /// (`rx_tsc`, `hw_rx_ns`, `sw_rx_ns`), `rx_empty_gap_ns` and `conn` go into every event's
    /// `MarketHdr` unchanged; no HW → SW fallback and no record-header flags here.
    fn decode_md(
        rd: &RefData,
        ctx: &mut Self::FeedCtx,
        c: Self::MdClass,
        payload: &[u8],
        rx: &RxMeta,
        out: &mut impl MdSink,
    ) -> Result<MdOutcome, DecodeErr>;
}
