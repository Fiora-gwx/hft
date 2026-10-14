//! `hft-venue-proto`: the `VenueProtocol` trait and the pure types in its signatures.
//!
//! This build carries the M1.5 part of the README: the identity constants, the session, market-data,
//! snapshot, order/report, rate-limit, expiry and dead-man members of the trait with their helper
//! types (`types.rs`), `MdSink` and `ReportSink`, and under feature `testkit` the `VecSink` recorder
//! and the contract-3 part of `check_md_basics`. The account-query and read-only query members,
//! `TradeIdRing`/`dedup_key`, the raw-code encoders and the other contract checks arrive with M3
//! (README "已知限制与后续工作"). No I/O, no threads, no global state.
//! Specification: `crates/hft-venue-proto/README.md`.
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

mod funding;
mod sink;
mod types;

pub use funding::{FundingBill, FundingHistoryProtocol, FundingPage, FundingQuery};

#[cfg(feature = "testkit")]
pub mod testkit;

pub use sink::{ExchIdHint, MdSink, ReportSink};
pub use types::{
    AcctHint, Align, ArrayBuf, BootCall, ControlHint, Credential, DeadManOut, DeadManSpec, DecodeErr,
    EncodeErr, EndpointSpec, HeartbeatSpec, LoginResult, LoginStep, MdOutcome, Method, MsgOp, OrderChannel,
    Peek, PingKind, ProtoErr, RateObs, RateRule, RawSeq, ReportCtl, ReportOutcome, ReqRef, RestOrigin,
    RestReqBuf, RestWhat, ResyncMethod, Role, RouteId, RouteKind, RouteSpec, RxMeta, SecretBuf, SeqIds,
    SeqRule, SignAlg, Skip, SnapshotInfo, StaleGuard, StreamKind, SubFrame, SubFrames, SubSpec, TargetRef,
    VenueCfg, VenueEnv, Via, WallClock,
};

use hft_event::{GwCmd, GwOp};
use hft_refdata::{RefData, VenueCaps};
use hft_types::{AccountId, AssetId, ClientOrderId, InstId, MarketKind, VenueId};

/// One venue product line's protocol (static dispatch; README "`VenueProtocol`(静态分派)").
/// Every method is an associated function or takes its context explicitly; no global state.
pub trait VenueProtocol: Sized + 'static {
    // ---- identity and constants ----
    /// Venue name, e.g. `"binance-usdm"` (hft-core `venues.<v>.protocol`, `fixtures/<NAME>/`).
    const NAME: &'static str;
    /// Product-line `VenueId`.
    const VENUE: VenueId;
    /// `Spot` or `LinearPerp`.
    const MARKET: MarketKind;
    /// Frame capacity the gw needs (2048 / 8192).
    const ORDER_FRAME_CAP: usize;
    /// Market-data dispatch result (venue-private enum).
    type MdClass: Copy + Eq;
    /// Report dispatch result (must contain an ignorable and an unknown class, contract 20).
    type ReportClass: Copy + Eq;
    /// Venue-private feed state (never caches refdata, contract 25 / "实现说明").
    type FeedCtx;
    /// Venue-private gw state: request ids, cumulative-quantity synthesis, dedup rings, margin cache.
    type GwCtx;
    /// Frame templates and the REST signing credential of one (venue, account) (contract 7).
    type Templates;
    /// After `CancelAll` the gw queries the open-order list once (contract 29).
    const CANCEL_ALL_SWEEP: bool = false;
    /// Insts one BBO refresh request may cover (contract 24); `u16::MAX` = request length only.
    const BBO_SNAPSHOT_BATCH: u16 = 1;
    /// The private channels live on a separate user-data connection (Binance USDT-M listenKey
    /// stream) instead of the order connection (hft-venue §4b).
    const USER_STREAM: bool = false;

    // ---- capability defaults (cold; hft-cfg only, never called by the runtime) ----
    /// Contract 15; asset names resolve through `asset` (unknown → `ASSET_NONE`).
    fn default_caps(asset: impl Fn(&str) -> Option<AssetId>) -> VenueCaps;

    // ---- session (warm thread conn-builder; may allocate and sign) ----
    /// REST request run before connecting; `cred` = the account's REST credential (`None` for Md).
    fn bootstrap(role: Role, cfg: &VenueCfg, cred: Option<&Credential>) -> Option<BootCall>;
    /// Market-data route table (Bbo, DepthTopN and Md routes).
    fn routes() -> &'static [RouteSpec];
    /// Upgrade request target and extra headers.
    fn endpoint(
        role: Role,
        route: RouteId,
        cfg: &VenueCfg,
        cred: Option<&Credential>,
        boot: Option<&[u8]>,
        now_ms: i64,
        out: &mut EndpointSpec,
    ) -> Result<(), ProtoErr>;
    /// Login frame number `step` (0 first; challenge logins continue after `LoginResult::Continue`).
    fn login(
        role: Role,
        cfg: &VenueCfg,
        cred: &Credential,
        step: u8,
        now_ms: i64,
        out: &mut Vec<u8>,
    ) -> Result<LoginStep, ProtoErr>;
    fn login_result(payload: &[u8]) -> LoginResult;
    /// Subscription (`unsub = false`) or unsubscription frames; each frame records how many leading
    /// `subs` it covers. Called on the feed thread after `install`: writes into the preallocated
    /// `out` without allocating. `rd` gives the wire symbols of the insts.
    fn subscribe(
        rd: &RefData,
        subs: &[SubSpec],
        unsub: bool,
        cfg: &VenueCfg,
        out: &mut SubFrames,
    ) -> Result<(), ProtoErr>;
    /// Private channel frames; only on the owning connection (hft-venue §4b).
    fn private_subscribe(
        cfg: &VenueCfg,
        acct: AccountId,
        unsub: bool,
        out: &mut SubFrames,
    ) -> Result<(), ProtoErr>;
    fn heartbeat(role: Role) -> HeartbeatSpec;
    /// Application ping into `out`; returns its length (0 = use a WS Ping).
    fn app_ping(role: Role, now_ms: i64, out: &mut [u8]) -> usize;
    /// BBO connection liveness probe (contract 24).
    fn bbo_probe() -> PingKind {
        PingKind::WsPing
    }
    /// Identity check of the session credential (contract 23): `Ok(true)` with a request signed by
    /// the session credential, `Ok(false)` when it is the REST credential.
    fn session_identity_request(
        cfg: &VenueCfg,
        cred: &Credential,
        now_ms: i64,
        out: &mut RestReqBuf,
    ) -> Result<bool, ProtoErr>;
    /// ASCII UID, right-padded with 0.
    fn decode_identity(status: u16, body: &[u8], uid: &mut [u8; 24]) -> Result<(), DecodeErr>;

    // ---- market data (feed tasks; hot) ----
    /// Dispatch from the first 64 B of a frame; `route` = the connection's route.
    fn classify(route: RouteId, op: MsgOp, head: &[u8]) -> Self::MdClass;
    /// Inst, stream and normalized ids without parsing prices; a class without data (control,
    /// unknown) → `Err(DecodeErr::UnknownClass)`.
    fn peek(rd: &RefData, ctx: &Self::FeedCtx, c: Self::MdClass, payload: &[u8]) -> Result<Peek, DecodeErr>;
    fn seq_rule(c: Self::MdClass) -> SeqRule;
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
    /// Subscription answers, server shutdown notices, application pongs.
    fn control(c: Self::MdClass, payload: &[u8]) -> Option<ControlHint>;
    fn resync_method(c: Self::MdClass) -> ResyncMethod;
    /// First increment after a snapshot (incremental depth).
    fn align_first(snap_id: u64, raw: &RawSeq) -> Align;

    // ---- snapshots (warm task snap-fetch, its own FeedCtx; contract 25) ----
    fn snapshot_request(
        rd: &RefData,
        ctx: &Self::FeedCtx,
        inst: InstId,
        out: &mut RestReqBuf,
    ) -> Result<(), ProtoErr>;
    fn decode_snapshot(
        rd: &RefData,
        ctx: &mut Self::FeedCtx,
        inst: InstId,
        body: &[u8],
        rx: &RxMeta,
        out: &mut impl MdSink,
    ) -> Result<SnapshotInfo, DecodeErr>;
    /// BBO refresh (contract 24); default: the venue pushes the first frame on subscribe.
    fn bbo_snapshot_request(
        rd: &RefData,
        ctx: &Self::FeedCtx,
        insts: &[InstId],
        out: &mut RestReqBuf,
    ) -> Result<bool, ProtoErr> {
        let _ = (rd, ctx, insts, out);
        Ok(false)
    }
    /// Returns the number of `Bbo`s produced.
    fn decode_bbo_snapshot(
        rd: &RefData,
        ctx: &mut Self::FeedCtx,
        insts: &[InstId],
        body: &[u8],
        rx: &RxMeta,
        out: &mut impl MdSink,
    ) -> Result<u16, DecodeErr> {
        let _ = (rd, ctx, insts, body, rx, out);
        Err(DecodeErr::UnknownClass)
    }

    // ---- orders and reports (gw task; hot) ----
    /// Startup, once per (venue, account).
    fn build_templates(
        cfg: &VenueCfg,
        rd: &RefData,
        acct: AccountId,
        rest: &Credential,
    ) -> Result<Self::Templates, ProtoErr>;
    /// WS frame or REST for an order-class op (contract 29).
    fn order_channel(op: GwOp) -> OrderChannel {
        let _ = op;
        OrderChannel::Ws
    }
    /// Fills Place/Cancel/Amend/CancelAll/Query into `body`; returns the length.
    fn encode(
        rd: &RefData,
        t: &Self::Templates,
        c: &GwCmd,
        tgt: &TargetRef,
        clk: &WallClock,
        body: &mut [u8],
    ) -> Result<usize, EncodeErr>;
    /// REST form of an order operation (contract 29).
    fn encode_rest(
        rd: &RefData,
        t: &Self::Templates,
        c: &GwCmd,
        tgt: &TargetRef,
        clk: &WallClock,
        out: &mut RestReqBuf,
    ) -> Result<(), EncodeErr> {
        let _ = (rd, t, tgt, clk, out);
        Err(EncodeErr::Unsupported(c.op))
    }
    /// Session keepalive request (`GwOp::SessionKeepalive`); `Ok(false)` = not needed.
    fn session_keepalive_request(
        rd: &RefData,
        t: &Self::Templates,
        clk: &WallClock,
        out: &mut RestReqBuf,
    ) -> Result<bool, EncodeErr> {
        let _ = (rd, t, clk, out);
        Ok(false)
    }
    /// Responses of the gw's REST requests (contract 29).
    #[allow(clippy::too_many_arguments)]
    fn decode_rest(
        rd: &RefData,
        ctx: &mut Self::GwCtx,
        req: &RestOrigin,
        status: u16,
        body: &[u8],
        rx: &RxMeta,
        out: &mut impl ReportSink,
    ) -> Result<ReportOutcome, DecodeErr> {
        let _ = (rd, ctx, req, status, body, rx, out);
        Err(DecodeErr::UnknownClass)
    }
    /// Optional: several decisions of one account in one frame; default unsupported.
    fn encode_batch(
        rd: &RefData,
        t: &Self::Templates,
        cs: &[GwCmd],
        tgts: &[TargetRef],
        clk: &WallClock,
        body: &mut [u8],
    ) -> Result<usize, EncodeErr> {
        let _ = (rd, t, cs, tgts, clk, body);
        Err(EncodeErr::Unsupported(GwOp::Place))
    }
    /// The canonical 24-hex text in this venue's shape (prefix only); returns the length.
    fn render_cid(id: &ClientOrderId, out: &mut [u8; 48]) -> usize;
    /// Strips the venue shape and decodes; not ours → `None`.
    fn parse_cid(bytes: &[u8]) -> Option<ClientOrderId>;
    fn classify_report(op: MsgOp, head: &[u8]) -> Self::ReportClass;
    fn decode_report(
        rd: &RefData,
        ctx: &mut Self::GwCtx,
        c: Self::ReportClass,
        payload: &[u8],
        rx: &RxMeta,
        out: &mut impl ReportSink,
    ) -> Result<ReportOutcome, DecodeErr>;

    // ---- rate limits, expiry, dead man ----
    /// Official defaults (protocol constants); effective values come from refdata.
    fn rate_rules() -> &'static [RateRule];
    fn stale_guard() -> StaleGuard;
    fn dead_man() -> DeadManSpec;
    /// WS frame or REST requests; `dead_man() = None` → `Err(Unsupported(DeadManRefresh))`.
    fn dead_man_refresh(
        rd: &RefData,
        t: &Self::Templates,
        timeout_ms: u32,
        insts: &[InstId],
        clk: &WallClock,
        out: &mut DeadManOut,
    ) -> Result<(), EncodeErr>;
    /// Hyperliquid: every order frame carries a signature.
    fn needs_per_order_signing() -> bool {
        false
    }
}
