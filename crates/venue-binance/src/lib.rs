//! `venue-binance`: the Binance USDT-M perpetual (`Usdm`) and spot (`Spot`) protocols
//! (README "概要").
//!
//! M1.5 part (README "已知限制与后续工作"): routes, endpoints and the listenKey bootstrap; market
//! dispatch (`classify`), ids (`peek`), sequence rules and control hints of every core class;
//! `bookTicker` decoding of both lines; `session.logon` with Ed25519; subscriptions; order frames
//! (place, cancel, amend, query, spot cancel-all) with request ids and client ids; WS API
//! responses (ack, reject and outcome-unknown tables, cancel/amend acks, query results, rate
//! limits); rate-rule, capability, expiry, dead-man and heartbeat defaults. The other market
//! classes' decoding, the user-data streams, account queries, REST signing and the read-only
//! queries arrive with M3. Pure functions: no I/O, no threads, no global state.
//! Specification: `crates/venue-binance/README.md`.
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
mod bbo;
mod caps;
mod cid;
mod errors;
mod funding;
mod json;
mod line;
mod market;
mod md;
mod orders;
mod readonly;
mod reports;
mod rest_orders;
mod session;
mod sign;
mod user_stream;
mod wr;

pub use cid::{decode_req_id, encode_req_id};
pub use errors::{CodeClass, classify_code};
pub use line::Line;
pub use orders::Templates;
pub use sign::session_seed;

use hft_event::{GwCmd, GwOp};
use hft_refdata::{RefData, VenueCaps};
use hft_types::{AccountId, AssetId, ClientOrderId, InstId, MarketKind, VenueId};
use hft_venue_proto::{
    Align, BootCall, ControlHint, Credential, DeadManOut, DeadManSpec, DecodeErr, EncodeErr, EndpointSpec,
    HeartbeatSpec, LoginResult, LoginStep, MdOutcome, MdSink, MsgOp, OrderChannel, Peek, ProtoErr, RateRule,
    RawSeq, ReportOutcome, ReportSink, RestOrigin, RestReqBuf, ResyncMethod, Role, RouteId, RouteSpec,
    RxMeta, SeqRule, SnapshotInfo, StaleGuard, SubFrames, SubSpec, TargetRef, VenueCfg, VenueProtocol,
    WallClock,
};

/// USDT-M perpetual (`binance-usdm`).
#[derive(Copy, Clone, Debug)]
pub struct Usdm;

/// Spot (`binance-spot`).
#[derive(Copy, Clone, Debug)]
pub struct Spot;

/// USDT-M dispatch classes (README "使用本组件").
pub mod usdm {
    #[derive(Copy, Clone, Eq, PartialEq, Debug)]
    pub enum MdClass {
        BookTicker,
        /// TopN snapshot stream (`public-topn` route).
        DepthTopN,
        /// Incremental depth (optional extension, `public-depth` route).
        DepthUpdate,
        AggTrade,
        MarkPrice,
        ForceOrder,
        SubAck,
        ServerShutdown,
        Unknown,
    }

    #[derive(Copy, Clone, Eq, PartialEq, Debug)]
    pub enum ReportClass {
        WsResp,
        OrderTradeUpdate,
        AccountUpdate,
        MarginCall,
        AccountConfigUpdate,
        ListenKeyExpired,
        /// logon and subscription answers.
        Control,
        Ignorable,
        Unknown,
    }
}

/// Spot dispatch classes (README "使用本组件"; the SBE classes are optional and not built).
pub mod spot {
    #[derive(Copy, Clone, Eq, PartialEq, Debug)]
    pub enum MdClass {
        JsonBookTicker,
        /// Combined-stream TopN snapshot (`spot-topn` route).
        JsonDepthTopN,
        /// Incremental depth (optional extension).
        JsonDepth,
        JsonTrade,
        SubAck,
        Unknown,
    }

    #[derive(Copy, Clone, Eq, PartialEq, Debug)]
    pub enum ReportClass {
        WsResp,
        ExecutionReport,
        OutboundAccountPosition,
        EventStreamTerminated,
        Control,
        Ignorable,
        Unknown,
    }
}

/// Unit of the `E`/`T` fields of a market connection (decided by the connection URL, README
/// "2. 解码"; never guessed from the value).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum TimeUnit {
    Ms,
    Us,
}

impl TimeUnit {
    /// Nanoseconds per unit.
    pub const fn ns(self) -> i64 {
        match self {
            TimeUnit::Ms => 1_000_000,
            TimeUnit::Us => 1_000,
        }
    }
}

/// Feed-task state (no refdata, contract 25).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct FeedCtx {
    pub time_unit: TimeUnit,
    fund: [(InstId, i64, u32); 256],
}

impl FeedCtx {
    /// `fstream` connections (no `timeUnit` parameter): milliseconds.
    pub const USDM: FeedCtx = FeedCtx {
        time_unit: TimeUnit::Ms,
        fund: [(InstId(0), 0, 0); 256],
    };
    /// Spot connections opened with `timeUnit=MICROSECOND`.
    pub const SPOT: FeedCtx = FeedCtx {
        time_unit: TimeUnit::Us,
        fund: [(InstId(0), 0, 0); 256],
    };

    fn funding_interval(&mut self, inst: InstId, next_ms: i64) -> u32 {
        for slot in &mut self.fund {
            if slot.0 == inst {
                if next_ms != slot.1 {
                    slot.2 = next_ms
                        .checked_sub(slot.1)
                        .filter(|d| *d > 0 && *d % 1000 == 0)
                        .and_then(|d| u32::try_from(d / 1000).ok())
                        .unwrap_or(0);
                    slot.1 = next_ms;
                }
                return slot.2;
            }
            if slot.0.0 == 0 {
                *slot = (inst, next_ms, 0);
                return 0;
            }
        }
        0
    }
}

/// Per-account bounded duplicate windows; retained across connection replacement.
#[derive(Clone, Debug)]
pub struct GwCtx {
    trades: [(u32, u64, u64, i64); 1024],
    trade_next: usize,
    funding: [(i64, i64); 64],
    funding_next: usize,
}
impl Default for GwCtx {
    fn default() -> Self {
        Self {
            trades: [(0, 0, 0, 0); 1024],
            trade_next: 0,
            funding: [(0, 0); 64],
            funding_next: 0,
        }
    }
}
impl GwCtx {
    pub(crate) fn trade(&mut self, inst: u32, id: u64, order: u64, cum: i64) -> bool {
        let key = if id > 0 {
            (inst, id, 0, 0)
        } else {
            (inst, 0, order, cum)
        };
        if self.trades.contains(&key) {
            return false;
        }
        if let Some(slot) = self.trades.get_mut(self.trade_next) {
            *slot = key;
        }
        self.trade_next = (self.trade_next + 1) % self.trades.len();
        true
    }
    pub(crate) fn funding(&mut self, ts: i64, wallet: i64) -> bool {
        let key = (ts, wallet);
        if self.funding.contains(&key) {
            return false;
        }
        if let Some(slot) = self.funding.get_mut(self.funding_next) {
            *slot = key;
        }
        self.funding_next = (self.funding_next + 1) % self.funding.len();
        true
    }
}

/// One implementation for both lines; `Usdm` and `Spot` forward with their `Line`.
macro_rules! venue_impl {
    ($ty:ty, $line:expr, $name:literal, $venue:expr, $market:expr, $md:ty, $rep:ty, $user:expr) => {
        impl VenueProtocol for $ty {
            const NAME: &'static str = $name;
            const VENUE: VenueId = $venue;
            const MARKET: MarketKind = $market;
            const BBO_SNAPSHOT_BATCH: u16 = if matches!($line, Line::Spot) {
                u16::MAX
            } else {
                1
            };
            const ORDER_FRAME_CAP: usize = 2048;
            type MdClass = $md;
            type ReportClass = $rep;
            type FeedCtx = FeedCtx;
            type GwCtx = GwCtx;
            type Templates = Templates;
            const USER_STREAM: bool = $user;

            fn default_caps(asset: impl Fn(&str) -> Option<AssetId>) -> VenueCaps {
                caps::default_caps($line, asset)
            }
            fn bootstrap(role: Role, cfg: &VenueCfg, cred: Option<&Credential>) -> Option<BootCall> {
                session::bootstrap($line, role, cfg, cred)
            }
            fn routes() -> &'static [RouteSpec] {
                session::routes($line)
            }
            fn endpoint(
                role: Role,
                route: RouteId,
                cfg: &VenueCfg,
                cred: Option<&Credential>,
                boot: Option<&[u8]>,
                now_ms: i64,
                out: &mut EndpointSpec,
            ) -> Result<(), ProtoErr> {
                session::endpoint($line, role, route, cfg, cred, boot, now_ms, out)
            }
            fn login(
                role: Role,
                cfg: &VenueCfg,
                cred: &Credential,
                step: u8,
                now_ms: i64,
                out: &mut Vec<u8>,
            ) -> Result<LoginStep, ProtoErr> {
                session::login($line, role, cfg, cred, step, now_ms, out)
            }
            fn login_result(payload: &[u8]) -> LoginResult {
                session::login_result(payload)
            }
            fn subscribe(
                rd: &RefData,
                subs: &[SubSpec],
                unsub: bool,
                cfg: &VenueCfg,
                out: &mut SubFrames,
            ) -> Result<(), ProtoErr> {
                session::subscribe($line, rd, subs, unsub, cfg, out)
            }
            fn private_channels() -> u64 {
                if $line == Line::Spot { 1 } else { 0 }
            }
            fn private_ack(payload: &[u8]) -> Option<(u8, bool)> {
                session::private_ack($line, payload)
            }
            fn is_private_report(class: Self::ReportClass) -> bool {
                {
                    use $rep as R;
                    !matches!(class, R::WsResp | R::Control | R::Unknown)
                }
            }
            fn private_subscribe(
                cfg: &VenueCfg,
                acct: AccountId,
                unsub: bool,
                out: &mut SubFrames,
            ) -> Result<(), ProtoErr> {
                session::private_subscribe($line, cfg, acct, unsub, out)
            }
            fn heartbeat(role: Role) -> HeartbeatSpec {
                caps::heartbeat($line, role)
            }
            fn app_ping(_role: Role, _now_ms: i64, _out: &mut [u8]) -> usize {
                0
            }
            fn session_identity_request(
                cfg: &VenueCfg,
                cred: &Credential,
                now_ms: i64,
                out: &mut RestReqBuf,
            ) -> Result<bool, ProtoErr> {
                session::identity_request($line, cfg, cred, now_ms, out)
            }
            fn decode_identity(status: u16, body: &[u8], uid: &mut [u8; 24]) -> Result<(), DecodeErr> {
                session::decode_identity(status, body, uid)
            }
            fn classify(route: RouteId, op: MsgOp, head: &[u8]) -> $md {
                md::Classify::classify(route, op, head)
            }
            fn peek(rd: &RefData, ctx: &FeedCtx, c: $md, payload: &[u8]) -> Result<Peek, DecodeErr> {
                md::peek($line, rd, ctx, md::Class::from(c), payload)
            }
            fn seq_rule(c: $md) -> SeqRule {
                md::seq_rule(md::Class::from(c))
            }
            fn decode_md(
                rd: &RefData,
                ctx: &mut FeedCtx,
                c: $md,
                payload: &[u8],
                rx: &RxMeta,
                out: &mut impl MdSink,
            ) -> Result<MdOutcome, DecodeErr> {
                md::decode_md($line, rd, ctx, md::Class::from(c), payload, rx, out)
            }
            fn control(c: $md, payload: &[u8]) -> Option<ControlHint> {
                md::control(md::Class::from(c), payload)
            }
            fn resync_method(c: $md) -> ResyncMethod {
                md::resync_method(md::Class::from(c))
            }
            fn align_first(_snap_id: u64, _raw: &RawSeq) -> Align {
                // Incremental depth is an optional extension (README "3. 快照对齐"); not built.
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
            fn bbo_snapshot_request(
                rd: &RefData,
                _ctx: &Self::FeedCtx,
                insts: &[InstId],
                out: &mut RestReqBuf,
            ) -> Result<bool, ProtoErr> {
                bbo::request($line, rd, insts, out)
            }
            fn decode_bbo_snapshot(
                rd: &RefData,
                _ctx: &mut Self::FeedCtx,
                insts: &[InstId],
                body: &[u8],
                rx: &RxMeta,
                out: &mut impl MdSink,
            ) -> Result<u16, DecodeErr> {
                bbo::decode($line, rd, insts, body, rx, out)
            }
            fn build_templates(
                cfg: &VenueCfg,
                rd: &RefData,
                acct: AccountId,
                rest: &Credential,
            ) -> Result<Templates, ProtoErr> {
                orders::build_templates($line, cfg, rd, acct, rest)
            }
            fn order_channel(op: GwOp) -> OrderChannel {
                orders::order_channel($line, op)
            }
            fn encode(
                rd: &RefData,
                t: &Templates,
                c: &GwCmd,
                tgt: &TargetRef,
                clk: &WallClock,
                body: &mut [u8],
            ) -> Result<usize, EncodeErr> {
                orders::encode($line, rd, t, c, tgt, clk, body)
            }
            fn render_cid(id: &ClientOrderId, out: &mut [u8; 48]) -> usize {
                cid::render_cid(id, out)
            }
            fn parse_cid(bytes: &[u8]) -> Option<ClientOrderId> {
                cid::parse_cid(bytes)
            }
            fn classify_report(op: MsgOp, head: &[u8]) -> $rep {
                reports::ClassifyReport::classify_report(op, head)
            }
            fn report_event_name(payload: &[u8]) -> &[u8] {
                reports::report_event_name(payload)
            }
            fn decode_report(
                rd: &RefData,
                ctx: &mut GwCtx,
                c: $rep,
                payload: &[u8],
                rx: &RxMeta,
                out: &mut impl ReportSink,
            ) -> Result<ReportOutcome, DecodeErr> {
                reports::decode_report($line, rd, ctx, reports::RClass::from(c), payload, rx, out)
            }
            fn ro_request_signed(
                rd: &RefData,
                t: &Templates,
                cred: &Credential,
                acct: AccountId,
                q: &hft_venue_proto::RoQuery,
                clk: &WallClock,
                out: &mut RestReqBuf,
            ) -> Result<(), EncodeErr> {
                readonly::request($line, rd, t, cred, acct, q, clk, out)
            }
            fn ro_decode(
                rd: &RefData,
                acct: AccountId,
                q: &hft_venue_proto::RoQuery,
                status: u16,
                body: &[u8],
                out: &mut impl hft_venue_proto::RoSink,
            ) -> Result<hft_venue_proto::RoPage, DecodeErr> {
                readonly::decode::<Self>($line, rd, acct, q, status, body, out)
            }
            fn rate_rules() -> &'static [RateRule] {
                caps::rate_rules($line)
            }
            fn stale_guard() -> StaleGuard {
                caps::stale_guard()
            }
            fn dead_man() -> DeadManSpec {
                caps::dead_man($line)
            }
            fn dead_man_refresh_signed(
                rd: &RefData,
                t: &Templates,
                cred: &Credential,
                timeout_ms: u32,
                insts: &[InstId],
                clk: &WallClock,
                out: &mut DeadManOut,
            ) -> Result<(), EncodeErr> {
                rest_orders::deadman($line, rd, t, cred, timeout_ms, insts, clk, out)
            }
            fn encode_rest_signed(
                rd: &RefData,
                t: &Templates,
                cred: &Credential,
                c: &GwCmd,
                _tgt: &TargetRef,
                clk: &WallClock,
                out: &mut RestReqBuf,
            ) -> Result<(), EncodeErr> {
                rest_orders::cancel_all($line, rd, t, cred, c, clk, out)
            }
            fn session_keepalive_request_signed(
                _rd: &RefData,
                t: &Templates,
                cred: &Credential,
                clk: &WallClock,
                out: &mut RestReqBuf,
            ) -> Result<bool, EncodeErr> {
                if $line == Line::Spot {
                    return Ok(false);
                }
                account::request(
                    true,
                    &VenueCfg {
                        env: hft_venue_proto::VenueEnv::Prod,
                        stale_guard_ms: t.recv_window_ms,
                    },
                    cred,
                    &hft_venue_proto::account::Query::Keepalive,
                    clk.unix_ms,
                    out,
                )
                .map_err(|_| EncodeErr::FieldOverflow)?;
                Ok(true)
            }
            fn decode_rest(
                _rd: &RefData,
                _ctx: &mut Self::GwCtx,
                req: &RestOrigin,
                status: u16,
                body: &[u8],
                _rx: &RxMeta,
                _out: &mut impl ReportSink,
            ) -> Result<ReportOutcome, DecodeErr> {
                rest_orders::decode(req, status, body)
            }
        }
    };
}

venue_impl!(
    Usdm,
    Line::Usdm,
    "binance-usdm",
    VenueId::BINANCE_USDM,
    MarketKind::LinearPerp,
    usdm::MdClass,
    usdm::ReportClass,
    true
);
venue_impl!(
    Spot,
    Line::Spot,
    "binance-spot",
    VenueId::BINANCE_SPOT,
    MarketKind::Spot,
    spot::MdClass,
    spot::ReportClass,
    false
);
