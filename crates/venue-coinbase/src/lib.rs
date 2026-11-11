//! `venue-coinbase`: the Coinbase Exchange spot market-data protocol (`coinbase-spot`, README
//! "概要"). Read-only: `ticker` → `Bbo` on a raced route, `matches` → `Trade`, `heartbeat` as the
//! application-level liveness signal; no order entry, no keys, no account. Every order, report,
//! account and session-identity method returns the `Unsupported` / `UnknownClass` form the trait
//! allows. Pure functions: no I/O, no threads, no global state.
//! Specification: `crates/venue-coinbase/README.md`.
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

mod adv;
mod adv_book;
mod adv_md;
mod caps;
mod md;
mod session;
mod time;

pub use adv::{CoinbaseAdv, ROUTE_L2};
pub use adv_book::LEVELS as ADV_LEVELS;
pub use adv_md::{AdvClass, AdvFeedCtx, BOOKS as ADV_BOOKS};
pub use md::MdClass;
pub use session::{ROUTE_BBO, ROUTE_MD};
pub use time::iso8601_ns;

use hft_event::GwCmd;
use hft_refdata::{RefData, VenueCaps};
use hft_types::{AccountId, AssetId, ClientOrderId, InstId, MarketKind, VenueId};
use hft_venue_proto::account::{AccountProtocol, Query, State};
use hft_venue_proto::{
    Align, BootCall, ControlHint, Credential, DeadManSpec, DecodeErr, EncodeErr, EndpointSpec,
    FundingHistoryProtocol, FundingPage, FundingQuery, HeartbeatSpec, LoginResult, LoginStep, MdOutcome,
    MdSink, MsgOp, Peek, ProtoErr, RateRule, RawSeq, ReportOutcome, ReportSink, RestReqBuf, ResyncMethod,
    Role, RouteId, RouteSpec, RxMeta, SeqRule, SnapshotInfo, StaleGuard, StreamKind, SubFrames, SubSpec,
    TargetRef, VenueCfg, VenueProtocol, WallClock,
};

/// Coinbase Exchange spot (`coinbase-spot`), market data only.
#[derive(Copy, Clone, Debug)]
pub struct CoinbaseSpot;

/// Report classes: the venue has no private stream (contract 20 still names both forms).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ReportClass {
    Ignorable,
    Unknown,
}

/// Feed state: none. Dedup is the runtime's `seq_rule` on the exchange's own ids.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct FeedCtx;

pub(crate) const VENUE: VenueId = VenueId::COINBASE_SPOT;

impl VenueProtocol for CoinbaseSpot {
    const NAME: &'static str = "coinbase-spot";
    const VENUE: VenueId = VENUE;
    const MARKET: MarketKind = MarketKind::Spot;
    const ORDER_FRAME_CAP: usize = 2048;
    type MdClass = MdClass;
    type ReportClass = ReportClass;
    type FeedCtx = FeedCtx;
    type GwCtx = ();
    type Templates = ();

    fn default_caps(asset: impl Fn(&str) -> Option<AssetId>) -> VenueCaps {
        caps::default_caps(asset)
    }
    fn bootstrap(_role: Role, _cfg: &VenueCfg, _cred: Option<&Credential>) -> Option<BootCall> {
        None
    }
    fn routes() -> &'static [RouteSpec] {
        session::routes()
    }
    fn stream_route(stream: StreamKind) -> Option<RouteId> {
        session::stream_route(stream)
    }
    fn endpoint(
        role: Role,
        route: RouteId,
        _cfg: &VenueCfg,
        _cred: Option<&Credential>,
        _boot: Option<&[u8]>,
        _now_ms: i64,
        out: &mut EndpointSpec,
    ) -> Result<(), ProtoErr> {
        session::endpoint(role, route, out)
    }
    fn login(
        role: Role,
        _cfg: &VenueCfg,
        _cred: &Credential,
        _step: u8,
        _now_ms: i64,
        _out: &mut Vec<u8>,
    ) -> Result<LoginStep, ProtoErr> {
        if role == Role::Md {
            Ok(LoginStep::NotNeeded)
        } else {
            Err(ProtoErr::Unsupported)
        }
    }
    fn login_result(_payload: &[u8]) -> LoginResult {
        LoginResult::NotMine
    }
    fn subscribe(
        rd: &RefData,
        subs: &[SubSpec],
        unsub: bool,
        _cfg: &VenueCfg,
        out: &mut SubFrames,
    ) -> Result<(), ProtoErr> {
        session::subscribe(rd, subs, unsub, out)
    }
    fn private_subscribe(
        _cfg: &VenueCfg,
        _acct: AccountId,
        _unsub: bool,
        _out: &mut SubFrames,
    ) -> Result<(), ProtoErr> {
        Err(ProtoErr::Unsupported)
    }
    fn heartbeat(role: Role) -> HeartbeatSpec {
        caps::heartbeat(role)
    }
    fn app_ping(_role: Role, _now_ms: i64, _out: &mut [u8]) -> usize {
        0
    }
    fn session_identity_request(
        _cfg: &VenueCfg,
        _cred: &Credential,
        _now_ms: i64,
        _out: &mut RestReqBuf,
    ) -> Result<bool, ProtoErr> {
        Err(ProtoErr::Unsupported)
    }
    fn decode_identity(_status: u16, _body: &[u8], _uid: &mut [u8; 24]) -> Result<(), DecodeErr> {
        Err(DecodeErr::UnknownClass)
    }
    fn classify(route: RouteId, op: MsgOp, head: &[u8]) -> MdClass {
        md::classify(route, op, head)
    }
    fn peek(rd: &RefData, _ctx: &FeedCtx, c: MdClass, payload: &[u8]) -> Result<Peek, DecodeErr> {
        md::peek(rd, c, payload)
    }
    fn seq_rule(c: MdClass) -> SeqRule {
        md::seq_rule(c)
    }
    fn decode_md(
        rd: &RefData,
        _ctx: &mut FeedCtx,
        c: MdClass,
        payload: &[u8],
        rx: &RxMeta,
        out: &mut impl MdSink,
    ) -> Result<MdOutcome, DecodeErr> {
        md::decode_md(rd, c, payload, rx, out)
    }
    fn control(c: MdClass, payload: &[u8]) -> Option<ControlHint> {
        md::control(c, payload)
    }
    fn resync_method(_c: MdClass) -> ResyncMethod {
        ResyncMethod::None
    }
    fn align_first(_snap_id: u64, _raw: &RawSeq) -> Align {
        // No incremental depth (README "已知限制与后续工作").
        Align::Gap
    }
    fn snapshot_request(
        _rd: &RefData,
        _ctx: &FeedCtx,
        _inst: InstId,
        _out: &mut RestReqBuf,
    ) -> Result<(), ProtoErr> {
        Err(ProtoErr::Unsupported)
    }
    fn decode_snapshot(
        _rd: &RefData,
        _ctx: &mut FeedCtx,
        _inst: InstId,
        _body: &[u8],
        _rx: &RxMeta,
        _out: &mut impl MdSink,
    ) -> Result<SnapshotInfo, DecodeErr> {
        Err(DecodeErr::UnknownClass)
    }
    // `bbo_snapshot_request` keeps the default `Ok(false)`: a `ticker` subscription pushes the
    // current ticker at once (README "订阅").
    fn build_templates(
        _cfg: &VenueCfg,
        _rd: &RefData,
        _acct: AccountId,
        _rest: &Credential,
    ) -> Result<(), ProtoErr> {
        Err(ProtoErr::Unsupported)
    }
    fn encode(
        _rd: &RefData,
        _t: &(),
        c: &GwCmd,
        _tgt: &TargetRef,
        _clk: &WallClock,
        _body: &mut [u8],
    ) -> Result<usize, EncodeErr> {
        Err(EncodeErr::Unsupported(c.op))
    }
    fn render_cid(_id: &ClientOrderId, _out: &mut [u8; 48]) -> usize {
        0
    }
    fn parse_cid(_bytes: &[u8]) -> Option<ClientOrderId> {
        None
    }
    fn classify_report(_op: MsgOp, _head: &[u8]) -> ReportClass {
        ReportClass::Unknown
    }
    fn decode_report(
        _rd: &RefData,
        _ctx: &mut (),
        _c: ReportClass,
        _payload: &[u8],
        _rx: &RxMeta,
        _out: &mut impl ReportSink,
    ) -> Result<ReportOutcome, DecodeErr> {
        Err(DecodeErr::UnknownClass)
    }
    fn rate_rules() -> &'static [RateRule] {
        caps::rate_rules()
    }
    fn stale_guard() -> StaleGuard {
        StaleGuard::None
    }
    fn dead_man() -> DeadManSpec {
        DeadManSpec::None
    }
}

impl FundingHistoryProtocol for CoinbaseSpot {
    fn funding_request(
        _cfg: &VenueCfg,
        _cred: &Credential,
        _query: &FundingQuery,
        _clock: &WallClock,
        _out: &mut RestReqBuf,
    ) -> Result<(), ProtoErr> {
        // Spot: no funding.
        Err(ProtoErr::Unsupported)
    }
    fn funding_decode(
        _rd: &RefData,
        _query: &FundingQuery,
        _status: u16,
        _body: &[u8],
    ) -> Result<FundingPage, DecodeErr> {
        Err(DecodeErr::UnknownClass)
    }
}

/// No account: hft-core builds an account client only for a venue with `accounts`, and refuses
/// accounts on this venue (README "账户").
impl AccountProtocol for CoinbaseSpot {
    type Funding = CoinbaseSpot;
    fn account_request(
        _cfg: &VenueCfg,
        _cred: &Credential,
        _query: &Query,
        _now_ms: i64,
        _out: &mut RestReqBuf,
    ) -> Result<(), ProtoErr> {
        Err(ProtoErr::Unsupported)
    }
    fn account_decode(
        _rd: &RefData,
        _account: AccountId,
        _query: &Query,
        _status: u16,
        _body: &[u8],
        _now_ms: i64,
        _state: &mut State,
    ) -> Result<(), DecodeErr> {
        Err(DecodeErr::UnknownClass)
    }
}
