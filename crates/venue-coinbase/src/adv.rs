//! `coinbase-adv`: Coinbase Advanced Trade public market data of a derivatives product (README
//! "Advanced Trade：level2 → Bbo"). Read-only like `coinbase-spot`: the `level2` channel keeps a
//! book per connection and yields a `Bbo` whenever the best bid or ask changes; `heartbeats` is
//! the liveness signal. No order entry, no keys, no account.

use hft_event::GwCmd;
use hft_refdata::{DeadManCap, FeeAssetRule, FundingSettleKind, LiqMarker, RefData, TestnetKind, VenueCaps};
use hft_types::{AccountId, AssetId, ClientOrderId, InstId, MarketKind, VenueId};
use hft_venue_proto::account::{AccountProtocol, Query, State};
use hft_venue_proto::{
    Align, BootCall, ControlHint, Credential, DeadManSpec, DecodeErr, EncodeErr, EndpointSpec,
    FundingHistoryProtocol, FundingPage, FundingQuery, HeartbeatSpec, LoginResult, LoginStep, MdOutcome,
    MdSink, MsgOp, Peek, PingKind, ProtoErr, RateRule, RawSeq, ReportOutcome, ReportSink, RestReqBuf,
    ResyncMethod, Role, RouteId, RouteKind, RouteSpec, RxMeta, SeqRule, SnapshotInfo, StaleGuard, StreamKind,
    SubFrames, SubSpec, TargetRef, VenueCfg, VenueProtocol, WallClock,
};

use crate::ReportClass;
use crate::adv_md::{self, AdvClass, AdvFeedCtx};

/// Coinbase Advanced Trade derivatives market data (`coinbase-adv`).
#[derive(Copy, Clone, Debug)]
pub struct CoinbaseAdv;

pub(crate) const VENUE: VenueId = VenueId::COINBASE_ADV;

/// `level2` (+ `heartbeats`): the best quotes, raced (`feed.bbo_conns`, default 2).
pub const ROUTE_L2: RouteId = RouteId(1);

static ROUTES: [RouteSpec; 1] = [RouteSpec {
    id: ROUTE_L2,
    kind: RouteKind::Bbo,
    name: "level2",
    race: true,
    max_streams: 2,
    max_in_msgs_per_s: 8,
    topn_levels: &[],
}];

/// The keep-alive frame (README "订阅与保活"): sent as the BBO probe once per `feed.bbo_ping_ms`;
/// the first one subscribes the `heartbeats` channel, every one is answered by `subscriptions`.
const KEEPALIVE: &[u8] = b"{\"type\":\"subscribe\",\"channel\":\"heartbeats\"}";

/// Product id of an inst of this venue: `[A-Z0-9-]+` only (it is written into JSON unescaped).
fn product(rd: &RefData, inst: InstId) -> Result<&[u8], ProtoErr> {
    if inst.0 == 0 || rd.inst(inst).venue != VENUE {
        return Err(ProtoErr::Cfg);
    }
    let s = rd.inst_meta(inst).symbol.as_bytes();
    if s.is_empty()
        || !s
            .iter()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || *b == b'-')
    {
        return Err(ProtoErr::Cfg);
    }
    Ok(s)
}

/// One frame for the leading `Bbo` specs:
/// `{"type":"subscribe","product_ids":[…],"channel":"level2"}` (`unsubscribe` likewise).
fn subscribe(rd: &RefData, subs: &[SubSpec], unsub: bool, out: &mut SubFrames) -> Result<(), ProtoErr> {
    let first = subs.first().ok_or(ProtoErr::Cfg)?;
    if first.stream != StreamKind::Bbo {
        return Err(ProtoErr::Unsupported);
    }
    let mut buf = [0u8; 512];
    let mut n = 0usize;
    let mut push = |b: &[u8]| -> Result<(), ProtoErr> {
        let end = n.checked_add(b.len()).ok_or(ProtoErr::BufFull)?;
        buf.get_mut(n..end).ok_or(ProtoErr::BufFull)?.copy_from_slice(b);
        n = end;
        Ok(())
    };
    push(if unsub {
        b"{\"type\":\"unsubscribe\",\"product_ids\":["
    } else {
        b"{\"type\":\"subscribe\",\"product_ids\":["
    })?;
    let mut covered = 0u16;
    for (i, s) in subs.iter().enumerate() {
        if s.stream != StreamKind::Bbo || i >= usize::from(ROUTES[0].max_streams) {
            break;
        }
        let p = product(rd, s.inst)?;
        if i > 0 {
            push(b",")?;
        }
        push(b"\"")?;
        push(p)?;
        push(b"\"")?;
        covered += 1;
    }
    push(b"],\"channel\":\"level2\"}")?;
    out.push(buf.get(..n).unwrap_or(&[]), covered)
}

/// Read-only perpetual-style market: no capability bit, no dead man, no account requirement.
fn default_caps() -> VenueCaps {
    let mut c = VenueCaps::none(VENUE);
    c.market = MarketKind::LinearPerp;
    c.fee_asset_rule = FeeAssetRule::Quote;
    c.reduce_only = false;
    // A perpetual row names its funding settlement (refdata step 5); nothing is ever settled here.
    c.funding_settle = FundingSettleKind::RateOnly;
    c.own_liquidation_marker = LiqMarker::NotIdentifiable;
    c.testnet = TestnetKind::None;
    c.dms = DeadManCap::None;
    c
}

impl VenueProtocol for CoinbaseAdv {
    const NAME: &'static str = "coinbase-adv";
    const VENUE: VenueId = VENUE;
    const MARKET: MarketKind = MarketKind::LinearPerp;
    const ORDER_FRAME_CAP: usize = 2048;
    type MdClass = AdvClass;
    type ReportClass = ReportClass;
    type FeedCtx = AdvFeedCtx;
    type GwCtx = ();
    type Templates = ();

    fn default_caps(_asset: impl Fn(&str) -> Option<AssetId>) -> VenueCaps {
        default_caps()
    }
    fn bootstrap(_role: Role, _cfg: &VenueCfg, _cred: Option<&Credential>) -> Option<BootCall> {
        None
    }
    fn routes() -> &'static [RouteSpec] {
        &ROUTES
    }
    fn stream_route(stream: StreamKind) -> Option<RouteId> {
        (stream == StreamKind::Bbo).then_some(ROUTE_L2)
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
        out.path_query.clear();
        out.headers.clear();
        if role != Role::Md || route != ROUTE_L2 {
            return Err(ProtoErr::Unsupported);
        }
        out.path_query.push(b"/")
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
        subscribe(rd, subs, unsub, out)
    }
    fn private_subscribe(
        _cfg: &VenueCfg,
        _acct: AccountId,
        _unsub: bool,
        _out: &mut SubFrames,
    ) -> Result<(), ProtoErr> {
        Err(ProtoErr::Unsupported)
    }
    fn heartbeat(_role: Role) -> HeartbeatSpec {
        HeartbeatSpec {
            ping: PingKind::AppJson,
            interval_ms: 5_000,
            unconditional: false,
            pong_timeout_ms: 5_000,
            idle_timeout_ms: 0,
        }
    }
    fn app_ping(_role: Role, _now_ms: i64, out: &mut [u8]) -> usize {
        match out.get_mut(..KEEPALIVE.len()) {
            Some(dst) => {
                dst.copy_from_slice(KEEPALIVE);
                KEEPALIVE.len()
            }
            None => 0,
        }
    }
    fn bbo_probe() -> PingKind {
        PingKind::AppJson
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
    fn classify(route: RouteId, op: MsgOp, head: &[u8]) -> AdvClass {
        adv_md::classify(route, op, head)
    }
    fn peek(rd: &RefData, _ctx: &AdvFeedCtx, c: AdvClass, payload: &[u8]) -> Result<Peek, DecodeErr> {
        adv_md::peek(rd, c, payload)
    }
    fn seq_rule(_c: AdvClass) -> SeqRule {
        // No id shared by the two connections: every frame reaches the decoder, which keeps one
        // book per connection and drops the duplicate of a best-quote change itself.
        SeqRule::None
    }
    fn decode_md(
        rd: &RefData,
        ctx: &mut AdvFeedCtx,
        c: AdvClass,
        payload: &[u8],
        rx: &RxMeta,
        out: &mut impl MdSink,
    ) -> Result<MdOutcome, DecodeErr> {
        adv_md::decode_md(rd, ctx, c, payload, rx, out)
    }
    fn control(c: AdvClass, payload: &[u8]) -> Option<ControlHint> {
        adv_md::control(c, payload)
    }
    fn resync_method(_c: AdvClass) -> ResyncMethod {
        ResyncMethod::None
    }
    fn align_first(_snap_id: u64, _raw: &RawSeq) -> Align {
        Align::Gap
    }
    fn snapshot_request(
        _rd: &RefData,
        _ctx: &AdvFeedCtx,
        _inst: InstId,
        _out: &mut RestReqBuf,
    ) -> Result<(), ProtoErr> {
        Err(ProtoErr::Unsupported)
    }
    fn decode_snapshot(
        _rd: &RefData,
        _ctx: &mut AdvFeedCtx,
        _inst: InstId,
        _body: &[u8],
        _rx: &RxMeta,
        _out: &mut impl MdSink,
    ) -> Result<SnapshotInfo, DecodeErr> {
        Err(DecodeErr::UnknownClass)
    }
    // `bbo_snapshot_request` keeps the default `Ok(false)`: a `level2` subscription starts with
    // the book snapshot.
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
        crate::caps::rate_rules()
    }
    fn stale_guard() -> StaleGuard {
        StaleGuard::None
    }
    fn dead_man() -> DeadManSpec {
        DeadManSpec::None
    }
}

impl FundingHistoryProtocol for CoinbaseAdv {
    fn funding_request(
        _cfg: &VenueCfg,
        _cred: &Credential,
        _query: &FundingQuery,
        _clock: &WallClock,
        _out: &mut RestReqBuf,
    ) -> Result<(), ProtoErr> {
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

/// No account (as `coinbase-spot`).
impl AccountProtocol for CoinbaseAdv {
    type Funding = CoinbaseAdv;
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
