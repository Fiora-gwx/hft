//! `venue-bitget`: the Bitget UTA (V3) USDT-M perpetual (`BitgetUsdtFutures`) and spot
//! (`BitgetSpot`) protocols (README "概要").
//!
//! Built: routes and endpoints; market dispatch by route; `books1` → `Bbo`, `books5`/`books50` →
//! TopN `L2Snapshot`, incremental `books` (`seq`/`pseq` chain) → `L2Snapshot`/`L2Batch`,
//! `publicTrade` → `Trade`, perpetual `ticker` → `MarkPrice`/`IndexPrice`/`Funding`; the
//! unconditional 25 s text `ping`; `login` (HMAC-SHA256); subscriptions; WS trade frames
//! (`place-order`, `cancel-order`, `modify-order`) with request and client ids; trade answers and
//! the code tables; the private `order` (fills, external fills), `position` and `account` channels;
//! the signed REST order query; rate-rule, capability, expiry and dead-man defaults. Not built:
//! account queries (configuration, margin), read-only queries, BBO REST refresh, the `mock`
//! feature (README "已知限制与后续工作"). Pure functions: no I/O, no threads, no global state.
//! Specification: `crates/venue-bitget/README.md`.
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

mod caps;
mod cid;
pub mod codes;
mod json;
mod line;
mod md;
mod orders;
mod reports;
mod session;
mod sign;
mod wr;

pub use cid::{decode_req_id, encode_req_id};
pub use line::Line;
pub use orders::Templates;
pub use sign::rest_sign;

use hft_event::{GwCmd, GwOp};
use hft_refdata::{RefData, VenueCaps};
use hft_types::{AccountId, AssetId, ClientOrderId, InstId, MarketKind, VenueId};
use hft_venue_proto::{
    Align, BootCall, ControlHint, Credential, DeadManOut, DeadManSpec, DecodeErr, EncodeErr, EndpointSpec,
    HeartbeatSpec, LoginResult, LoginStep, MdOutcome, MdSink, MsgOp, OrderChannel, Peek, PingKind, ProtoErr,
    RateRule, RawSeq, ReportOutcome, ReportSink, RestOrigin, RestReqBuf, ResyncMethod, Role, RouteId,
    RouteSpec, RxMeta, SeqRule, SnapshotInfo, StaleGuard, SubFrames, SubSpec, TargetRef, VenueCfg,
    VenueProtocol, WallClock,
};

/// USDT-M perpetual (`bitget-usdt-futures`).
#[derive(Copy, Clone, Debug)]
pub struct BitgetUsdtFutures;

/// Spot (`bitget-spot`).
#[derive(Copy, Clone, Debug)]
pub struct BitgetSpot;

/// Market-data dispatch classes of both lines (README "使用本组件").
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum MdClass {
    Books1,
    /// `books5`, `books50` (`public-topn` route).
    BooksTopN,
    /// Incremental `books` (optional extension, `public-books` route).
    BooksIncr,
    Trade,
    /// Perpetual only.
    Ticker,
    /// `event = subscribe / unsubscribe / error`.
    SubAck,
    Pong,
    Unknown,
}

/// Report dispatch classes (README "使用本组件").
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ReportClass {
    /// Trade-frame answers (`event = trade`, or `error` with our request id).
    WsResp,
    Orders,
    /// `fill` / `fast-fill` (not subscribed).
    Fills,
    Positions,
    Account,
    /// A private push whose topic lies beyond the first 64 B: `decode_report` reads `arg`.
    Push,
    /// `event = login / subscribe / error`.
    Control,
    Pong,
    Ignorable,
    Unknown,
}

/// Funding-period tracking of one inst (README "行情频道 → 事件": `interval_s` = the difference of
/// two consecutive distinct `nextFundingTime`s).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
struct FundSlot {
    inst: InstId,
    next_ms: u64,
    interval_s: u32,
}

const FUND_SLOTS: usize = 256;

/// Feed-task state: the funding-period table (no refdata, contract 25).
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct FeedCtx {
    fund: [FundSlot; FUND_SLOTS],
}

impl Default for FeedCtx {
    fn default() -> Self {
        Self::new()
    }
}

impl FeedCtx {
    pub const fn new() -> FeedCtx {
        FeedCtx {
            fund: [FundSlot {
                inst: InstId(0),
                next_ms: 0,
                interval_s: 0,
            }; FUND_SLOTS],
        }
    }

    /// `Funding.interval_s` after seeing `next_ms` for `inst` (0 = unknown).
    pub(crate) fn funding_interval(&mut self, inst: InstId, next_ms: u64) -> u32 {
        let home = (inst.0 as usize) % FUND_SLOTS;
        for k in 0..FUND_SLOTS {
            let Some(s) = self.fund.get_mut((home + k) % FUND_SLOTS) else {
                return 0;
            };
            if s.inst == inst {
                if next_ms > s.next_ms {
                    let d = (next_ms - s.next_ms) / 1000;
                    s.interval_s = u32::try_from(d).unwrap_or(0);
                    s.next_ms = next_ms;
                } else if next_ms < s.next_ms {
                    s.interval_s = 0;
                    s.next_ms = next_ms;
                }
                return s.interval_s;
            }
            if s.inst.0 == 0 {
                *s = FundSlot {
                    inst,
                    next_ms,
                    interval_s: 0,
                };
                return 0;
            }
        }
        0
    }
}

/// Per-order cumulative state of the `order` channel (README "累计量与费用来源"): the last
/// cumulative quantity, value (`Notional`, −1 = unknown) and fee seen, so fills and fees are
/// emitted once as differences.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub(crate) struct OrderState {
    pub id: u64,
    pub cum: i64,
    pub value: i64,
    pub fee: i64,
    pub terminal: bool,
    pub stamp: u32,
}

const GW_SLOTS: usize = 4096;
const GW_PROBE: usize = 32;

/// gw-task state: the per-order table (fixed capacity, allocated once by `new`). A full probe
/// window reuses a terminal entry, else the oldest; a reused live order restarts from zero and the
/// OMS ignores the repeated cumulative quantity (contract 9).
#[derive(Clone, Eq, PartialEq)]
pub struct GwCtx {
    tab: Box<[OrderState]>,
    clock: u32,
}

impl core::fmt::Debug for GwCtx {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GwCtx").field("clock", &self.clock).finish()
    }
}

impl Default for GwCtx {
    fn default() -> Self {
        Self::new()
    }
}

impl GwCtx {
    /// Startup (allocates the table).
    pub fn new() -> GwCtx {
        GwCtx {
            tab: vec![OrderState::default(); GW_SLOTS].into_boxed_slice(),
            clock: 0,
        }
    }

    fn home(id: u64) -> usize {
        (id.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 52) as usize
    }

    pub(crate) fn get(&self, id: u64) -> Option<OrderState> {
        let h = Self::home(id);
        for k in 0..GW_PROBE {
            let e = self.tab.get((h + k) % GW_SLOTS)?;
            if e.id == id {
                return Some(*e);
            }
            if e.id == 0 {
                return None;
            }
        }
        None
    }

    pub(crate) fn put(&mut self, id: u64, cum: i64, value: i64, fee: i64, terminal: bool) {
        self.clock = self.clock.wrapping_add(1);
        let h = Self::home(id);
        // The entry itself, else the first empty slot (end of the chain), else the first terminal
        // entry, else the oldest.
        let (mut terminal_at, mut oldest) = (None, None::<(usize, u32)>);
        let mut pick = None;
        for k in 0..GW_PROBE {
            let i = (h + k) % GW_SLOTS;
            let Some(e) = self.tab.get(i) else { break };
            if e.id == id || e.id == 0 {
                pick = Some(i);
                break;
            }
            if e.terminal && terminal_at.is_none() {
                terminal_at = Some(i);
            }
            let age = self.clock.wrapping_sub(e.stamp);
            if oldest.is_none_or(|(_, a)| age > a) {
                oldest = Some((i, age));
            }
        }
        let Some(i) = pick.or(terminal_at).or(oldest.map(|(i, _)| i)) else {
            return;
        };
        if let Some(e) = self.tab.get_mut(i) {
            *e = OrderState {
                id,
                cum,
                value,
                fee,
                terminal,
                stamp: self.clock,
            };
        }
    }
}

/// One implementation for both lines; the two types forward with their `Line`.
macro_rules! venue_impl {
    ($ty:ty, $line:expr, $name:literal, $venue:expr, $market:expr) => {
        impl VenueProtocol for $ty {
            const NAME: &'static str = $name;
            const VENUE: VenueId = $venue;
            const MARKET: MarketKind = $market;
            const ORDER_FRAME_CAP: usize = 2048;
            type MdClass = MdClass;
            type ReportClass = ReportClass;
            type FeedCtx = FeedCtx;
            type GwCtx = GwCtx;
            type Templates = Templates;
            const CANCEL_ALL_SWEEP: bool = false;
            const BBO_SNAPSHOT_BATCH: u16 = 1;
            const USER_STREAM: bool = false;

            fn default_caps(asset: impl Fn(&str) -> Option<AssetId>) -> VenueCaps {
                caps::default_caps($line, asset)
            }
            fn bootstrap(_role: Role, _cfg: &VenueCfg, _cred: Option<&Credential>) -> Option<BootCall> {
                None
            }
            fn routes() -> &'static [RouteSpec] {
                session::routes($line)
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
                session::endpoint($line, role, route, out)
            }
            fn login(
                role: Role,
                _cfg: &VenueCfg,
                cred: &Credential,
                step: u8,
                now_ms: i64,
                out: &mut Vec<u8>,
            ) -> Result<LoginStep, ProtoErr> {
                session::login(role, cred, step, now_ms, out)
            }
            fn login_result(payload: &[u8]) -> LoginResult {
                session::login_result(payload)
            }
            fn subscribe(
                rd: &RefData,
                subs: &[SubSpec],
                unsub: bool,
                _cfg: &VenueCfg,
                out: &mut SubFrames,
            ) -> Result<(), ProtoErr> {
                session::subscribe($line, rd, subs, unsub, out)
            }
            fn private_subscribe(
                _cfg: &VenueCfg,
                acct: AccountId,
                unsub: bool,
                out: &mut SubFrames,
            ) -> Result<(), ProtoErr> {
                session::private_subscribe($line, acct, unsub, out)
            }
            fn heartbeat(role: Role) -> HeartbeatSpec {
                caps::heartbeat(role)
            }
            fn app_ping(_role: Role, _now_ms: i64, out: &mut [u8]) -> usize {
                match out.get_mut(..caps::PING.len()) {
                    Some(d) => {
                        d.copy_from_slice(caps::PING);
                        caps::PING.len()
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
                // The WS login and REST requests use the same key (README "账户配置核验" step 2).
                Ok(false)
            }
            fn decode_identity(_status: u16, _body: &[u8], _uid: &mut [u8; 24]) -> Result<(), DecodeErr> {
                Err(DecodeErr::UnknownClass)
            }
            fn classify(route: RouteId, op: MsgOp, head: &[u8]) -> MdClass {
                md::classify($line, route, op, head)
            }
            fn peek(rd: &RefData, ctx: &FeedCtx, c: MdClass, payload: &[u8]) -> Result<Peek, DecodeErr> {
                md::peek($line, rd, ctx, c, payload)
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
                md::decode_md($line, rd, ctx, c, payload, rx, out)
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
                // The incremental `books` stream resyncs by resubscribing (snapshot inline).
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
                cfg: &VenueCfg,
                _rd: &RefData,
                acct: AccountId,
                rest: &Credential,
            ) -> Result<Templates, ProtoErr> {
                orders::build_templates(cfg, acct, rest)
            }
            fn order_channel(op: GwOp) -> OrderChannel {
                orders::order_channel(op)
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
            fn encode_rest(
                rd: &RefData,
                t: &Templates,
                c: &GwCmd,
                tgt: &TargetRef,
                clk: &WallClock,
                out: &mut RestReqBuf,
            ) -> Result<(), EncodeErr> {
                orders::encode_rest($line, rd, t, c, tgt, clk, out)
            }
            fn decode_rest(
                rd: &RefData,
                _ctx: &mut GwCtx,
                req: &RestOrigin,
                status: u16,
                body: &[u8],
                rx: &RxMeta,
                out: &mut impl ReportSink,
            ) -> Result<ReportOutcome, DecodeErr> {
                reports::decode_rest($line, rd, req, status, body, rx, out)
            }
            fn render_cid(id: &ClientOrderId, out: &mut [u8; 48]) -> usize {
                cid::render_cid(id, out)
            }
            fn parse_cid(bytes: &[u8]) -> Option<ClientOrderId> {
                cid::parse_cid(bytes)
            }
            fn classify_report(op: MsgOp, head: &[u8]) -> ReportClass {
                reports::classify_report(op, head)
            }
            fn decode_report(
                rd: &RefData,
                ctx: &mut GwCtx,
                c: ReportClass,
                payload: &[u8],
                rx: &RxMeta,
                out: &mut impl ReportSink,
            ) -> Result<ReportOutcome, DecodeErr> {
                reports::decode_report($line, rd, ctx, c, payload, rx, out)
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
            fn dead_man_refresh(
                _rd: &RefData,
                _t: &Templates,
                _timeout_ms: u32,
                _insts: &[InstId],
                _clk: &WallClock,
                _out: &mut DeadManOut,
            ) -> Result<(), EncodeErr> {
                Err(EncodeErr::Unsupported(GwOp::DeadManRefresh))
            }
        }
    };
}

venue_impl!(
    BitgetUsdtFutures,
    Line::Fut,
    "bitget-usdt-futures",
    VenueId::BITGET_UTA_USDT_FUT,
    MarketKind::LinearPerp
);
venue_impl!(
    BitgetSpot,
    Line::Spot,
    "bitget-spot",
    VenueId::BITGET_UTA_SPOT,
    MarketKind::Spot
);
