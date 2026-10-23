//! `venue-polymarket`: the Polymarket CLOB protocol for binary outcome tokens (README "概要").
//!
//! Built: the market channel (`book` → `L2Snapshot`, `price_change` → `L2Batch`,
//! `last_trade_price` → `Trade`) with synthetic per-inst sequence ids, the text `PING`/`PONG`
//! heartbeat, `subscribe`/`unsubscribe` frames rendered from the token binding table of
//! `FeedCtx`, capability and rate defaults; order entry over REST (Deposit Wallet signed,
//! post-only GTD orders, cancels, order query), the authenticated user channel (`order` events →
//! fills and cancels) and the warm account baseline. Pure functions: no I/O, no threads, no
//! global state. Specification: `crates/venue-polymarket/README.md`.
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

mod account;
mod bind;
mod caps;
mod dedup;
mod md;
mod orders;
mod reports;
mod session;
pub mod sign;

pub use bind::{BindErr, FeedCtx, TOKEN_MAX};
pub use orders::{GTD_S, Templates};
pub use reports::GwCtx;

use hft_event::{GwCmd, GwOp};
use hft_refdata::{RefData, VenueCaps};
use hft_types::{AccountId, AssetId, ClientOrderId, InstId, MarketKind, VenueId};
use hft_venue_proto::account::{AccountProtocol, Query, State};
use hft_venue_proto::{
    Align, BootCall, ControlHint, Credential, DeadManSpec, DecodeErr, EncodeErr, EndpointSpec,
    FundingHistoryProtocol, FundingPage, FundingQuery, HeartbeatSpec, LoginResult, LoginStep, MdOutcome,
    MdSink, MsgOp, OrderChannel, Peek, PingKind, ProtoErr, RateRule, RawSeq, ReportOutcome, ReportSink,
    RestOrigin, RestReqBuf, ResyncMethod, Role, RouteId, RouteSpec, RxMeta, SeqRule, SnapshotInfo,
    StaleGuard, StreamKind, SubFrames, SubSpec, TargetRef, VenueCfg, VenueProtocol, WallClock,
};

/// Polymarket CLOB (`polymarket-clob`).
#[derive(Copy, Clone, Debug)]
pub struct PolymarketClob;

/// Market-data dispatch classes (README "使用本组件").
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum MdClass {
    /// A JSON object or array: `book`, `price_change`, `last_trade_price` or another event type
    /// (the type sits past the first 64 B, so `peek` tells them apart).
    Push,
    Pong,
    Unknown,
}

/// Report dispatch classes of the user channel (README "用户流事件表").
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ReportClass {
    /// A JSON object or array: `order` / `trade` events (the type sits at the end of the frame).
    Push,
    /// `PONG`.
    Ignorable,
    Unknown,
}

pub(crate) const VENUE: VenueId = VenueId::POLYMARKET_CLOB;

impl VenueProtocol for PolymarketClob {
    const NAME: &'static str = "polymarket-clob";
    const VENUE: VenueId = VENUE;
    const MARKET: MarketKind = MarketKind::Spot;
    const ORDER_FRAME_CAP: usize = 2048;
    type MdClass = MdClass;
    type ReportClass = ReportClass;
    type FeedCtx = FeedCtx;
    type GwCtx = GwCtx;
    type Templates = Templates;
    const SUB_ACKS: bool = false;
    const REST_PLACE_BINDS_ID: bool = true;

    fn default_caps(asset: impl Fn(&str) -> Option<AssetId>) -> VenueCaps {
        caps::default_caps(VENUE, asset)
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
        cred: &Credential,
        _step: u8,
        _now_ms: i64,
        out: &mut Vec<u8>,
    ) -> Result<LoginStep, ProtoErr> {
        session::login(role, cred, out)
    }
    fn login_result(_payload: &[u8]) -> LoginResult {
        LoginResult::NotMine
    }
    fn subscribe(
        _rd: &RefData,
        _subs: &[SubSpec],
        _unsub: bool,
        _cfg: &VenueCfg,
        _out: &mut SubFrames,
    ) -> Result<(), ProtoErr> {
        // Wire ids live in the binding table, not refdata: only `subscribe_ctx` can render them.
        Err(ProtoErr::Cfg)
    }
    fn subscribe_ctx(
        rd: &RefData,
        ctx: &FeedCtx,
        subs: &[SubSpec],
        unsub: bool,
        _cfg: &VenueCfg,
        out: &mut SubFrames,
    ) -> Result<(), ProtoErr> {
        session::subscribe(rd, ctx, subs, unsub, out)
    }
    fn bind_wire(ctx: &mut FeedCtx, inst: InstId, id: &[u8; 32]) -> Result<(), ProtoErr> {
        ctx.bind_wire(inst, id).map_err(|_| ProtoErr::Cfg)
    }
    fn bind_order_wire(ctx: &mut GwCtx, inst: InstId, id: &[u8; 32]) -> Result<(), ProtoErr> {
        ctx.bind(inst, id)
    }
    fn private_subscribe(
        _cfg: &VenueCfg,
        _acct: AccountId,
        _unsub: bool,
        out: &mut SubFrames,
    ) -> Result<(), ProtoErr> {
        // The login frame subscribes the whole account; there is nothing else to send.
        out.clear();
        Ok(())
    }
    fn is_private_report(class: ReportClass) -> bool {
        class == ReportClass::Push
    }
    fn heartbeat(role: Role) -> HeartbeatSpec {
        session::heartbeat(role)
    }
    fn app_ping(_role: Role, _now_ms: i64, out: &mut [u8]) -> usize {
        match out.get_mut(..session::PING.len()) {
            Some(d) => {
                d.copy_from_slice(session::PING);
                session::PING.len()
            }
            None => 0,
        }
    }
    fn bbo_probe() -> PingKind {
        PingKind::AppText
    }
    fn session_identity_request(
        _cfg: &VenueCfg,
        _cred: &Credential,
        _now_ms: i64,
        _out: &mut RestReqBuf,
    ) -> Result<bool, ProtoErr> {
        // The user channel and REST use the same API key (README "账户配置核验").
        Ok(false)
    }
    fn decode_identity(_status: u16, _body: &[u8], _uid: &mut [u8; 24]) -> Result<(), DecodeErr> {
        Err(DecodeErr::UnknownClass)
    }
    fn classify(route: RouteId, op: MsgOp, head: &[u8]) -> MdClass {
        md::classify(route, op, head)
    }
    fn peek(rd: &RefData, ctx: &FeedCtx, c: MdClass, payload: &[u8]) -> Result<Peek, DecodeErr> {
        md::peek(rd, ctx, c, payload)
    }
    fn seq_rule(c: MdClass) -> SeqRule {
        md::seq_rule(c)
    }
    fn decode_md(
        rd: &RefData,
        ctx: &mut FeedCtx,
        c: MdClass,
        payload: &[u8],
        rx: &RxMeta,
        out: &mut impl MdSink,
    ) -> Result<MdOutcome, DecodeErr> {
        md::decode_md(VENUE, rd, ctx, c, payload, rx, out)
    }
    fn control(c: MdClass, payload: &[u8]) -> Option<ControlHint> {
        md::control(c, payload)
    }
    fn resync_method(c: MdClass) -> ResyncMethod {
        md::resync_method(c)
    }
    fn align_first(snap_id: u64, raw: &RawSeq) -> Align {
        md::align_first(snap_id, raw)
    }
    fn snapshot_request(
        _rd: &RefData,
        _ctx: &FeedCtx,
        _inst: InstId,
        _out: &mut RestReqBuf,
    ) -> Result<(), ProtoErr> {
        // The stream resyncs by resubscribing (the exchange sends a fresh `book`).
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
    fn build_templates(
        _cfg: &VenueCfg,
        _rd: &RefData,
        acct: AccountId,
        rest: &Credential,
    ) -> Result<Templates, ProtoErr> {
        orders::build_templates(acct, rest)
    }
    fn order_channel(_op: GwOp) -> OrderChannel {
        OrderChannel::Rest
    }
    fn encode_rest_ctx(
        rd: &RefData,
        t: &Templates,
        ctx: &mut GwCtx,
        cred: &Credential,
        c: &GwCmd,
        tgt: &TargetRef,
        clk: &WallClock,
        out: &mut RestReqBuf,
    ) -> Result<(), EncodeErr> {
        orders::encode(rd, t, ctx, cred, c, tgt, clk, out)
    }
    fn encode_rest_signed(
        _rd: &RefData,
        _t: &Templates,
        _cred: &Credential,
        c: &GwCmd,
        _tgt: &TargetRef,
        _clk: &WallClock,
        _out: &mut RestReqBuf,
    ) -> Result<(), EncodeErr> {
        // Token ids and order hashes live in `GwCtx`: only `encode_rest_ctx` can encode.
        Err(EncodeErr::Unsupported(c.op))
    }
    fn decode_rest(
        rd: &RefData,
        ctx: &mut GwCtx,
        req: &RestOrigin,
        status: u16,
        body: &[u8],
        rx: &RxMeta,
        out: &mut impl ReportSink,
    ) -> Result<ReportOutcome, DecodeErr> {
        reports::decode_rest(rd, ctx, req, status, body, rx, out)
    }
    fn encode(
        _rd: &RefData,
        _t: &Templates,
        c: &GwCmd,
        _tgt: &TargetRef,
        _clk: &WallClock,
        _body: &mut [u8],
    ) -> Result<usize, EncodeErr> {
        Err(EncodeErr::Unsupported(c.op))
    }
    fn render_cid(id: &ClientOrderId, out: &mut [u8; 48]) -> usize {
        let mut t = [0u8; 24];
        id.encode(&mut t);
        if let Some(d) = out.get_mut(..24) {
            d.copy_from_slice(&t);
        }
        24
    }
    fn parse_cid(bytes: &[u8]) -> Option<ClientOrderId> {
        if bytes.len() != 24 {
            return None;
        }
        ClientOrderId::decode(bytes).ok()
    }
    fn classify_report(op: MsgOp, head: &[u8]) -> ReportClass {
        reports::classify_report(op, head)
    }
    fn report_event_name(payload: &[u8]) -> &[u8] {
        reports::event_name(payload)
    }
    fn decode_report(
        rd: &RefData,
        ctx: &mut GwCtx,
        c: ReportClass,
        payload: &[u8],
        rx: &RxMeta,
        out: &mut impl ReportSink,
    ) -> Result<ReportOutcome, DecodeErr> {
        reports::decode_report(rd, ctx, c, payload, rx, out)
    }
    fn rate_rules() -> &'static [RateRule] {
        caps::rate_rules()
    }
    fn stale_guard() -> StaleGuard {
        caps::stale_guard()
    }
    fn dead_man() -> DeadManSpec {
        caps::dead_man()
    }
}

impl FundingHistoryProtocol for PolymarketClob {
    fn funding_request(
        _cfg: &VenueCfg,
        _cred: &Credential,
        _query: &FundingQuery,
        _clock: &WallClock,
        _out: &mut RestReqBuf,
    ) -> Result<(), ProtoErr> {
        // Outcome tokens have no funding.
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

impl AccountProtocol for PolymarketClob {
    type Funding = PolymarketClob;
    fn account_request(
        _cfg: &VenueCfg,
        cred: &Credential,
        query: &Query,
        now_ms: i64,
        out: &mut RestReqBuf,
    ) -> Result<(), ProtoErr> {
        account::request(cred, query, now_ms, out)
    }
    fn account_decode(
        rd: &RefData,
        account: AccountId,
        query: &Query,
        status: u16,
        body: &[u8],
        now_ms: i64,
        state: &mut State,
    ) -> Result<(), DecodeErr> {
        account::decode(VENUE, rd, account, query, status, body, now_ms, state)
    }
}
