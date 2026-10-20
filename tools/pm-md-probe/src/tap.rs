//! `Tap`: venue-polymarket's protocol with a diagnostic on the side (README "实现说明"): every
//! `peek`/`decode_md` error other than an unknown class prints the frame head (first
//! `TAP_PRINT` of them) to stderr, so frames the runtime only counts can be inspected.

use std::sync::atomic::{AtomicU32, Ordering};

use hft_event::GwCmd;
use hft_refdata::{RefData, VenueCaps};
use hft_types::{AccountId, AssetId, ClientOrderId, InstId, MarketKind, VenueId};
use hft_venue_proto::{
    Align, BootCall, ControlHint, Credential, DeadManSpec, DecodeErr, EncodeErr, EndpointSpec, HeartbeatSpec,
    LoginResult, LoginStep, MdOutcome, MdSink, MsgOp, Peek, PingKind, ProtoErr, RateRule, RawSeq,
    ReportOutcome, ReportSink, RestReqBuf, ResyncMethod, Role, RouteId, RouteSpec, RxMeta, SeqRule,
    SnapshotInfo, StaleGuard, StreamKind, SubFrames, SubSpec, TargetRef, VenueCfg, VenueProtocol, WallClock,
};
use venue_polymarket::{FeedCtx, GwCtx, MdClass, PolymarketClob as Pm, ReportClass, Templates};

const TAP_PRINT: u32 = 20;
static PRINTED: AtomicU32 = AtomicU32::new(0);

fn report(stage: &str, e: DecodeErr, payload: &[u8]) {
    if e == DecodeErr::UnknownClass || PRINTED.fetch_add(1, Ordering::Relaxed) >= TAP_PRINT {
        return;
    }
    let head = payload.get(..payload.len().min(600)).unwrap_or(payload);
    eprintln!(
        "pm-md-probe tap: {stage} {e:?} len {}: {}",
        payload.len(),
        String::from_utf8_lossy(head)
    );
}

#[derive(Copy, Clone, Debug)]
pub struct Tap;

impl VenueProtocol for Tap {
    const NAME: &'static str = Pm::NAME;
    const VENUE: VenueId = Pm::VENUE;
    const MARKET: MarketKind = Pm::MARKET;
    const ORDER_FRAME_CAP: usize = Pm::ORDER_FRAME_CAP;
    type MdClass = MdClass;
    type ReportClass = ReportClass;
    type FeedCtx = FeedCtx;
    type GwCtx = GwCtx;
    type Templates = Templates;
    const SUB_ACKS: bool = Pm::SUB_ACKS;

    fn default_caps(asset: impl Fn(&str) -> Option<AssetId>) -> VenueCaps {
        Pm::default_caps(asset)
    }
    fn bootstrap(role: Role, cfg: &VenueCfg, cred: Option<&Credential>) -> Option<BootCall> {
        Pm::bootstrap(role, cfg, cred)
    }
    fn routes() -> &'static [RouteSpec] {
        Pm::routes()
    }
    fn stream_route(stream: StreamKind) -> Option<RouteId> {
        Pm::stream_route(stream)
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
        Pm::endpoint(role, route, cfg, cred, boot, now_ms, out)
    }
    fn login(
        role: Role,
        cfg: &VenueCfg,
        cred: &Credential,
        step: u8,
        now_ms: i64,
        out: &mut Vec<u8>,
    ) -> Result<LoginStep, ProtoErr> {
        Pm::login(role, cfg, cred, step, now_ms, out)
    }
    fn login_result(payload: &[u8]) -> LoginResult {
        Pm::login_result(payload)
    }
    fn subscribe(
        rd: &RefData,
        subs: &[SubSpec],
        unsub: bool,
        cfg: &VenueCfg,
        out: &mut SubFrames,
    ) -> Result<(), ProtoErr> {
        Pm::subscribe(rd, subs, unsub, cfg, out)
    }
    fn subscribe_ctx(
        rd: &RefData,
        ctx: &FeedCtx,
        subs: &[SubSpec],
        unsub: bool,
        cfg: &VenueCfg,
        out: &mut SubFrames,
    ) -> Result<(), ProtoErr> {
        Pm::subscribe_ctx(rd, ctx, subs, unsub, cfg, out)
    }
    fn private_subscribe(
        cfg: &VenueCfg,
        acct: AccountId,
        unsub: bool,
        out: &mut SubFrames,
    ) -> Result<(), ProtoErr> {
        Pm::private_subscribe(cfg, acct, unsub, out)
    }
    fn heartbeat(role: Role) -> HeartbeatSpec {
        Pm::heartbeat(role)
    }
    fn app_ping(role: Role, now_ms: i64, out: &mut [u8]) -> usize {
        Pm::app_ping(role, now_ms, out)
    }
    fn bbo_probe() -> PingKind {
        Pm::bbo_probe()
    }
    fn session_identity_request(
        cfg: &VenueCfg,
        cred: &Credential,
        now_ms: i64,
        out: &mut RestReqBuf,
    ) -> Result<bool, ProtoErr> {
        Pm::session_identity_request(cfg, cred, now_ms, out)
    }
    fn decode_identity(status: u16, body: &[u8], uid: &mut [u8; 24]) -> Result<(), DecodeErr> {
        Pm::decode_identity(status, body, uid)
    }
    fn classify(route: RouteId, op: MsgOp, head: &[u8]) -> MdClass {
        Pm::classify(route, op, head)
    }
    fn peek(rd: &RefData, ctx: &FeedCtx, c: MdClass, payload: &[u8]) -> Result<Peek, DecodeErr> {
        let r = Pm::peek(rd, ctx, c, payload);
        if let Err(e) = r {
            report("peek", e, payload);
        }
        r
    }
    fn seq_rule(c: MdClass) -> SeqRule {
        Pm::seq_rule(c)
    }
    fn decode_md(
        rd: &RefData,
        ctx: &mut FeedCtx,
        c: MdClass,
        payload: &[u8],
        rx: &RxMeta,
        out: &mut impl MdSink,
    ) -> Result<MdOutcome, DecodeErr> {
        let r = Pm::decode_md(rd, ctx, c, payload, rx, out);
        if let Err(e) = r {
            report("decode", e, payload);
        }
        r
    }
    fn control(c: MdClass, payload: &[u8]) -> Option<ControlHint> {
        Pm::control(c, payload)
    }
    fn resync_method(c: MdClass) -> ResyncMethod {
        Pm::resync_method(c)
    }
    fn align_first(snap_id: u64, raw: &RawSeq) -> Align {
        Pm::align_first(snap_id, raw)
    }
    fn snapshot_request(
        rd: &RefData,
        ctx: &FeedCtx,
        inst: InstId,
        out: &mut RestReqBuf,
    ) -> Result<(), ProtoErr> {
        Pm::snapshot_request(rd, ctx, inst, out)
    }
    fn decode_snapshot(
        rd: &RefData,
        ctx: &mut FeedCtx,
        inst: InstId,
        body: &[u8],
        rx: &RxMeta,
        out: &mut impl MdSink,
    ) -> Result<SnapshotInfo, DecodeErr> {
        Pm::decode_snapshot(rd, ctx, inst, body, rx, out)
    }
    fn build_templates(
        cfg: &VenueCfg,
        rd: &RefData,
        acct: AccountId,
        rest: &Credential,
    ) -> Result<Templates, ProtoErr> {
        Pm::build_templates(cfg, rd, acct, rest)
    }
    fn encode(
        rd: &RefData,
        t: &Templates,
        c: &GwCmd,
        tgt: &TargetRef,
        clk: &WallClock,
        body: &mut [u8],
    ) -> Result<usize, EncodeErr> {
        Pm::encode(rd, t, c, tgt, clk, body)
    }
    fn render_cid(id: &ClientOrderId, out: &mut [u8; 48]) -> usize {
        Pm::render_cid(id, out)
    }
    fn parse_cid(bytes: &[u8]) -> Option<ClientOrderId> {
        Pm::parse_cid(bytes)
    }
    fn classify_report(op: MsgOp, head: &[u8]) -> ReportClass {
        Pm::classify_report(op, head)
    }
    fn decode_report(
        rd: &RefData,
        ctx: &mut GwCtx,
        c: ReportClass,
        payload: &[u8],
        rx: &RxMeta,
        out: &mut impl ReportSink,
    ) -> Result<ReportOutcome, DecodeErr> {
        Pm::decode_report(rd, ctx, c, payload, rx, out)
    }
    fn rate_rules() -> &'static [RateRule] {
        Pm::rate_rules()
    }
    fn stale_guard() -> StaleGuard {
        Pm::stale_guard()
    }
    fn dead_man() -> DeadManSpec {
        Pm::dead_man()
    }
}
