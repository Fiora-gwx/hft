//! Built-in fake protocol of TESTS.md "夹具" — M1.5 part: `FakePerp`'s market-data path (BBO,
//! trade, mark price, 20-level TopN snapshot) and `fake_rd()`. Messages are simple JSON text:
//!
//! ```text
//! {"e":"bbo","i":201,"u":7,"b":"100.5","B":"1.25","a":"100.6","A":"2","T":1700000000000}
//! {"e":"trade","i":201,"t":99,"p":"100.5","q":"0.1","m":1,"T":1700000000000}
//! {"e":"mark","i":201,"p":"100.55","T":1700000000000}
//! {"e":"depth","i":201,"u":5,"T":1700000000000,"b":[["100.5","1"],…],"a":[["100.6","2"],…]}
//! ```
//!
//! `FakeSpot`, the session/order/account members and the contract-violating variants arrive with M3.
#![allow(dead_code)]

use hft_event::{Bbo, L2SnapHdr, L2SnapshotPart, Level, MarkPrice, MarketHdr, Trade};
use hft_event::{GwCmd, GwOp};
use hft_refdata::{
    AssetClass, AssetSpec, CapFlags, DeadManCap, Exchange, FeeAssetRule, FundingSettleKind, InstKind,
    InstSpec, InstStatus, LiqMarker, MarginData, MarginMode, PositionMode, RdbBuilder, RefData,
    StaleGuardKind, TestnetKind, TsUnit, VenueCaps, VenueSpec,
};
use hft_types::{AccountId, ClientOrderId};
use hft_types::{AssetExp, AssetId, InstId, MarketKind, Notional, Px, Qty, Side, VenueId};
use hft_venue_proto::{
    Align, BootCall, ControlHint, Credential, DeadManOut, DeadManSpec, DecodeErr, EncodeErr, EndpointSpec,
    HeartbeatSpec, LoginResult, LoginStep, MdOutcome, MdSink, MsgOp, Peek, PingKind, ProtoErr, RateRule,
    RawSeq, ReportOutcome, ReportSink, RestReqBuf, ResyncMethod, Role, RouteId, RouteSpec, RxMeta, SeqRule,
    SnapshotInfo, StaleGuard, SubFrames, SubSpec, TargetRef, VenueCfg, VenueProtocol, WallClock,
};
use zerocopy::FromZeros;

/// Venue of the fake refdata (a spot venue row: the M1.5 fixture only exercises decoding).
pub const FAKE_VENUE: VenueId = VenueId::BINANCE_SPOT;

/// `fake_rd()`: dense insts 1..=202 at price/quantity exponent −8; inst 101 = `FAKE-SPOT-1`,
/// 201 = `FAKE-PERP-1`, 202 = `FAKE-PERP-2`.
pub fn fake_rd() -> RefData {
    let mut b = RdbBuilder::new(1);
    b.created_utc_ns(1_759_276_800_000_000_000);
    for (id, name, class) in [(1u16, "BTC", AssetClass::Crypto), (2, "USDT", AssetClass::Stable)] {
        b.asset(AssetSpec {
            id: AssetId(id),
            name: name.into(),
            class,
            exp: AssetExp {
                px_exp: -8,
                qty_exp: -8,
            },
            px_max: Px(1_000_000_000_000_000),
            qty_max: Qty(1_000_000_000_000_000),
            dust: Qty(1_000),
        });
    }
    b.venue(VenueSpec {
        id: FAKE_VENUE,
        exchange: Exchange::Binance,
        product: InstKind::Spot,
        ws_max_streams_per_conn: 1024,
        ws_max_in_msgs_per_s: 10,
        ws_max_conn_age_s: 86_400,
        ws_new_conn_per_5min_ip: 300,
        app_ping_interval_s: 0,
        ts_unit: TsUnit::Ms,
        client_id_max_len: 36,
        client_id_prefix: "".into(),
        max_batch: 5,
        max_inflight_amend: 1,
        crossed_tolerance_default: 0,
        stale_guard: StaleGuardKind::RecvWindow,
    });
    b.caps(VenueCaps {
        venue: FAKE_VENUE,
        market: MarketKind::Spot,
        fee_asset_rule: FeeAssetRule::ReceivedAsset,
        reduce_only: false,
        required_position_mode: PositionMode::NotApplicable,
        required_margin_mode: MarginMode::NotApplicable,
        required_multi_asset: None,
        required_spot_borrow: None,
        funding_settle: FundingSettleKind::NotApplicable,
        margin_data: MarginData::NotApplicable,
        own_liquidation_marker: LiqMarker::NotIdentifiable,
        liq_marker_arg: "".into(),
        ignorable_user_events: Vec::new().into(),
        flags: CapFlags::POST_ONLY | CapFlags::IOC,
        amend_qty_down_only: false,
        dms: DeadManCap::None,
        dms_min_ms: 0,
        dms_max_ms: 0,
        dms_timeout_ms: 0,
        dms_refresh_ms: 0,
        testnet: TestnetKind::Testnet,
        _pad: 0,
    });
    for id in 1..=202u32 {
        let symbol = match id {
            101 => "FAKE-SPOT-1".to_string(),
            201 => "FAKE-PERP-1".to_string(),
            202 => "FAKE-PERP-2".to_string(),
            n => format!("FAKE-FILL-{n}"),
        };
        b.inst(InstSpec {
            id: InstId(id),
            venue: FAKE_VENUE,
            symbol,
            status: InstStatus::Trading,
            kind: InstKind::Spot,
            asset: AssetId(1),
            quote: AssetId(2),
            tick: 1,
            lot: 1,
            min_qty: Qty(1_000),
            max_qty: Qty(100_000_000_000),
            min_notional: Notional(5_000_000),
            book_window: 64,
            l2_depth: 20,
            crossed_tolerance: 0,
            tick_str: "0.00000001".into(),
            lot_str: "0.00000001".into(),
            min_qty_str: "0.00001".into(),
            ..InstSpec::default()
        });
    }
    let blob = b.finish();
    match RefData::load(&blob, None) {
        Ok(r) => r,
        Err(e) => panic!("fake refdata rejected: {e:?}"),
    }
}

/// Market-data classes of the fake protocol.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum FakeMd {
    Bbo,
    Trade,
    Mark,
    Depth,
    Unknown,
}

/// The fake linear-perpetual protocol.
#[derive(Copy, Clone, Debug)]
pub struct Fake<const SPOT: bool = false, const MUT: u8 = 0>;
pub type FakePerp = Fake;
pub type FakeSpot = Fake<true>;

/// Feed state (none needed by the M1.5 messages).
#[derive(Clone, Debug, Default)]
pub struct FakeFeedCtx;

struct Wire<'a> {
    out: &'a mut [u8],
    n: usize,
}
impl core::fmt::Write for Wire<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let dst = self
            .out
            .get_mut(self.n..self.n + s.len())
            .ok_or(core::fmt::Error)?;
        dst.copy_from_slice(s.as_bytes());
        self.n += s.len();
        Ok(())
    }
}
fn blank_report(rx: &RxMeta) -> hft_event::OrderEvent {
    use hft_event::{OrderEvent, OrderEventKind};
    OrderEvent {
        venue: FAKE_VENUE,
        account: rx.account,
        what: OrderEventKind::Ack,
        detail: 0,
        slot: hft_types::SlotId(42),
        r#gen: 7,
        inst: InstId(201),
        px: Px(0),
        qty: Qty(0),
        cum_filled: Qty(0),
        fee: Qty(0),
        fee_asset: AssetId(0),
        epoch: 4,
        reject_code: 0,
        exch_order_id: 0,
        ex_ts_ns: 0,
        hw_rx_ns: rx.hw_rx_ns,
        rx_tsc: rx.rx_tsc,
        avg_px: Px(0),
        cash_qty: Qty(0),
        cash_sum: Qty(0),
        trade_id: 0,
    }
}

impl<const SPOT: bool, const MUT: u8> VenueProtocol for Fake<SPOT, MUT> {
    const NAME: &'static str = "fake-perp";
    const VENUE: VenueId = FAKE_VENUE;
    const MARKET: MarketKind = if SPOT {
        MarketKind::Spot
    } else {
        MarketKind::LinearPerp
    };
    const ORDER_FRAME_CAP: usize = 2048;
    type MdClass = FakeMd;
    type ReportClass = u8;
    type FeedCtx = FakeFeedCtx;
    type GwCtx = ();
    type Templates = ();

    // Only methods with independent contract fixtures implement this synthetic wire protocol.
    fn default_caps(asset: impl Fn(&str) -> Option<AssetId>) -> VenueCaps {
        let mut caps = VenueCaps::none(FAKE_VENUE);
        caps.market = Self::MARKET;
        caps.reduce_only = !SPOT;
        caps.fee_asset_rule = if SPOT {
            FeeAssetRule::ReceivedAssetUnlessDiscount {
                token: asset("FKT").unwrap_or(AssetId(0)),
            }
        } else {
            FeeAssetRule::Quote
        };
        if !SPOT {
            caps.dms = DeadManCap::CountdownAccount;
            caps.dms_min_ms = 10_000;
            caps.dms_max_ms = 120_000;
            caps.dms_timeout_ms = 60_000;
            caps.dms_refresh_ms = 15_000;
        }
        caps
    }
    fn bootstrap(_r: Role, _c: &VenueCfg, _k: Option<&Credential>) -> Option<BootCall> {
        None
    }
    fn routes() -> &'static [RouteSpec] {
        &[]
    }
    fn endpoint(
        _r: Role,
        _route: RouteId,
        _c: &VenueCfg,
        _k: Option<&Credential>,
        _b: Option<&[u8]>,
        _n: i64,
        _o: &mut EndpointSpec,
    ) -> Result<(), ProtoErr> {
        Err(ProtoErr::Unsupported)
    }
    fn login(
        _r: Role,
        _c: &VenueCfg,
        _k: &Credential,
        _s: u8,
        _n: i64,
        _o: &mut Vec<u8>,
    ) -> Result<LoginStep, ProtoErr> {
        Ok(LoginStep::NotNeeded)
    }
    fn login_result(_p: &[u8]) -> LoginResult {
        LoginResult::NotMine
    }
    fn subscribe(
        _rd: &RefData,
        _s: &[SubSpec],
        _u: bool,
        _c: &VenueCfg,
        _o: &mut SubFrames,
    ) -> Result<(), ProtoErr> {
        Err(ProtoErr::Unsupported)
    }
    fn private_subscribe(_c: &VenueCfg, _a: AccountId, _u: bool, _o: &mut SubFrames) -> Result<(), ProtoErr> {
        Err(ProtoErr::Unsupported)
    }
    fn heartbeat(_r: Role) -> HeartbeatSpec {
        HeartbeatSpec {
            ping: PingKind::WsPing,
            interval_ms: 0,
            unconditional: false,
            pong_timeout_ms: 0,
            idle_timeout_ms: 0,
        }
    }
    fn app_ping(_r: Role, _n: i64, _o: &mut [u8]) -> usize {
        0
    }
    fn session_identity_request(
        _c: &VenueCfg,
        _k: &Credential,
        _n: i64,
        out: &mut RestReqBuf,
    ) -> Result<bool, ProtoErr> {
        if SPOT {
            return Ok(false);
        }
        out.path_query.push(b"/identity").map_err(|_| ProtoErr::BufFull)?;
        out.headers
            .push(b"Authorization: fixture-signature\r\n")
            .map_err(|_| ProtoErr::BufFull)?;
        out.redacted = true;
        Ok(true)
    }
    fn decode_identity(s: u16, b: &[u8], u: &mut [u8; 24]) -> Result<(), DecodeErr> {
        if s != 200 || b.is_empty() || b.len() > u.len() || !b.iter().all(u8::is_ascii_graphic) {
            return Err(DecodeErr::Shape);
        }
        u.fill(0);
        u[..b.len()].copy_from_slice(b);
        Ok(())
    }
    fn peek(_rd: &RefData, _ctx: &FakeFeedCtx, c: FakeMd, p: &[u8]) -> Result<Peek, DecodeErr> {
        let (stream, key) = match c {
            FakeMd::Bbo => (hft_venue_proto::StreamKind::Bbo, "u"),
            FakeMd::Trade => (hft_venue_proto::StreamKind::Trade, "t"),
            FakeMd::Mark => (hft_venue_proto::StreamKind::Mark, "T"),
            FakeMd::Depth => (hft_venue_proto::StreamKind::DepthTopN, "u"),
            FakeMd::Unknown => return Err(DecodeErr::UnknownClass),
        };
        Ok(Peek {
            inst: InstId(int_field(p, "i")? as u32),
            stream,
            ids: hft_venue_proto::SeqIds {
                prev_id: 0,
                last_id: int_field(p, key)? as u64,
            },
            raw: RawSeq::default(),
        })
    }
    fn seq_rule(_c: FakeMd) -> SeqRule {
        SeqRule::None
    }
    fn control(_c: FakeMd, _p: &[u8]) -> Option<ControlHint> {
        None
    }
    fn resync_method(_c: FakeMd) -> ResyncMethod {
        ResyncMethod::None
    }
    fn align_first(_s: u64, _r: &RawSeq) -> Align {
        Align::Gap
    }
    fn snapshot_request(
        _rd: &RefData,
        _c: &FakeFeedCtx,
        _i: InstId,
        _o: &mut RestReqBuf,
    ) -> Result<(), ProtoErr> {
        Err(ProtoErr::Unsupported)
    }
    fn decode_snapshot(
        _rd: &RefData,
        _c: &mut FakeFeedCtx,
        _i: InstId,
        _b: &[u8],
        _rx: &RxMeta,
        _o: &mut impl MdSink,
    ) -> Result<SnapshotInfo, DecodeErr> {
        Err(DecodeErr::UnknownClass)
    }
    fn build_templates(_c: &VenueCfg, _rd: &RefData, _a: AccountId, _k: &Credential) -> Result<(), ProtoErr> {
        Ok(())
    }
    fn encode(
        rd: &RefData,
        _t: &(),
        c: &GwCmd,
        tg: &TargetRef,
        k: &WallClock,
        b: &mut [u8],
    ) -> Result<usize, EncodeErr> {
        if rd.inst(c.inst).venue != Self::VENUE {
            return Err(EncodeErr::UnknownInst);
        }
        let ro = c.oflags & hft_event::OrderFlags::REDUCE_ONLY.0 != 0;
        if SPOT && ro && MUT != 1 {
            return Err(EncodeErr::Unsupported(c.op));
        }
        if c.op == GwOp::Amend && tg.exch_order_id == 0 {
            return Err(EncodeErr::UnknownTarget);
        }
        let mut cid = [0; 48];
        let n = Self::render_cid(&tg.id, &mut cid);
        let cid = core::str::from_utf8(&cid[..n]).map_err(|_| EncodeErr::FieldOverflow)?;
        use core::fmt::Write;
        let mut w = Wire { out: b, n: 0 };
        write!(
            w,
            "{{\"op\":{},\"cid\":\"{}\",\"time\":{}",
            c.op as u8, cid, k.unix_ms
        )
        .map_err(|_| EncodeErr::FieldOverflow)?;
        if ro && !SPOT {
            w.write_str(",\"ro\":true")
                .map_err(|_| EncodeErr::FieldOverflow)?;
        }
        w.write_str("}").map_err(|_| EncodeErr::FieldOverflow)?;
        Ok(w.n)
    }
    fn render_cid(id: &ClientOrderId, o: &mut [u8; 48]) -> usize {
        let mut text = [0; 24];
        id.encode(&mut text);
        o[..2].copy_from_slice(b"hf");
        o[2..26].copy_from_slice(&text);
        26
    }
    fn parse_cid(b: &[u8]) -> Option<ClientOrderId> {
        ClientOrderId::decode(b.strip_prefix(b"hf")?).ok()
    }
    fn classify_report(_op: MsgOp, h: &[u8]) -> u8 {
        match str_field(h, "e") {
            Some(b"response") => 1,
            Some(b"listenKeyExtended") => 2,
            Some(b"ended") => 3,
            Some(b"shutdown") => 4,
            _ => 0,
        }
    }
    fn decode_report(
        _rd: &RefData,
        _c: &mut (),
        k: u8,
        p: &[u8],
        rx: &RxMeta,
        out: &mut impl ReportSink,
    ) -> Result<ReportOutcome, DecodeErr> {
        let mut o = ReportOutcome::default();
        match k {
            0 => o.skip = hft_venue_proto::Skip::Unknown,
            2 => o.skip = hft_venue_proto::Skip::Listed,
            3 => o.ctl = hft_venue_proto::ReportCtl::UserStreamEnded,
            4 => o.ctl = hft_venue_proto::ReportCtl::ServerShutdown,
            _ => {
                let r = hft_venue_proto::ReqRef {
                    slot: 42,
                    r#gen: 7,
                    op: GwOp::Place,
                };
                o.req = Some(r);
                let code = str_field(p, "code").ok_or(DecodeErr::Shape)?;
                o.raw_code = if code.starts_with(b"HTTP") {
                    hft_venue_proto::raw_code_http(503)
                } else {
                    hft_venue_proto::raw_code_str(code)
                };
                if [b"-2010".as_slice(), b"51008", b"-1021", b"-1015"].contains(&code) {
                    let mut e = blank_report(rx);
                    e.what = hft_event::OrderEventKind::Reject;
                    e.reject_code = o.raw_code;
                    out.order(&e);
                    o.events = 1;
                    o.ctl = match code {
                        b"-1021" => hft_venue_proto::ReportCtl::ClockSkew,
                        b"-1015" => hft_venue_proto::ReportCtl::RateLimited,
                        _ => hft_venue_proto::ReportCtl::None,
                    };
                } else {
                    o.outcome_unknown = Some(r);
                }
            }
        }
        Ok(o)
    }
    fn rate_rules() -> &'static [RateRule] {
        const NORMAL: RateRule = RateRule {
            scope: hft_refdata::LimitScope::Account,
            bucket: hft_refdata::LimitBucket::Orders,
            window_ms: 1000,
            limit: 10,
        };
        if MUT == 2 {
            &[RateRule {
                window_ms: 0,
                ..NORMAL
            }]
        } else if MUT == 3 {
            &[RateRule { limit: 0, ..NORMAL }]
        } else {
            &[NORMAL]
        }
    }
    fn stale_guard() -> StaleGuard {
        if SPOT {
            StaleGuard::None
        } else {
            StaleGuard::RecvWindow { default_ms: 5000 }
        }
    }
    fn dead_man() -> DeadManSpec {
        if SPOT {
            DeadManSpec::None
        } else {
            DeadManSpec::CountdownAccount {
                min_ms: 10_000,
                max_ms: 120_000,
                timeout_ms: 60_000,
                refresh_ms: 15_000,
                via: hft_venue_proto::Via::Ws,
            }
        }
    }
    fn dead_man_refresh(
        _rd: &RefData,
        _t: &(),
        _ms: u32,
        _i: &[InstId],
        _k: &WallClock,
        _o: &mut DeadManOut,
    ) -> Result<(), EncodeErr> {
        Err(EncodeErr::Unsupported(GwOp::DeadManRefresh))
    }

    fn classify(_route: RouteId, op: MsgOp, head: &[u8]) -> FakeMd {
        if op != MsgOp::Text {
            return FakeMd::Unknown;
        }
        match str_field(head, "e") {
            Some(b"bbo") => FakeMd::Bbo,
            Some(b"trade") => FakeMd::Trade,
            Some(b"mark") => FakeMd::Mark,
            Some(b"depth") => FakeMd::Depth,
            _ => FakeMd::Unknown,
        }
    }

    fn decode_md(
        rd: &RefData,
        _ctx: &mut FakeFeedCtx,
        c: FakeMd,
        p: &[u8],
        rx: &RxMeta,
        out: &mut impl MdSink,
    ) -> Result<MdOutcome, DecodeErr> {
        if c == FakeMd::Unknown {
            return Err(DecodeErr::UnknownClass);
        }
        let inst = InstId(u32::try_from(int_field(p, "i")?).map_err(|_| DecodeErr::Overflow)?);
        let row = rd.inst(inst);
        let (pe, qe) = (row.px_exp, row.qty_exp);
        let mut h = MarketHdr::new_zeroed();
        h.venue = Self::VENUE;
        h.conn = rx.conn;
        h.inst = inst;
        h.ex_ts_ns = int_field(p, "T")?.saturating_mul(1_000_000);
        h.ex_send_ns = h.ex_ts_ns;
        h.hw_rx_ns = rx.hw_rx_ns;
        h.rx_tsc = rx.rx_tsc;
        h.sw_rx_ns = rx.sw_rx_ns;
        h.rx_empty_gap_ns = rx.rx_empty_gap_ns;
        let px = |k: &str| fixed(str_field(p, k).ok_or(DecodeErr::Truncated)?, pe).map(Px);
        let qty = |k: &str| fixed(str_field(p, k).ok_or(DecodeErr::Truncated)?, qe).map(Qty);
        match c {
            FakeMd::Bbo => {
                let mut e = Bbo::new_zeroed();
                e.h = h;
                e.update_id = int_field(p, "u")? as u64;
                e.bid_px = px("b")?;
                e.bid_qty = qty("B")?;
                e.ask_px = px("a")?;
                e.ask_qty = qty("A")?;
                out.bbo(&e);
            }
            FakeMd::Trade => {
                let e = Trade {
                    h,
                    px: px("p")?,
                    qty: qty("q")?,
                    trade_id: int_field(p, "t")? as u64,
                    taker_side: if int_field(p, "m")? == 1 {
                        Side::Sell
                    } else {
                        Side::Buy
                    },
                    _pad: [0; 7],
                };
                out.trade(&e);
            }
            FakeMd::Mark => {
                let mut e = MarkPrice::new_zeroed();
                e.h = h;
                e.px = px("p")?;
                out.mark(&e);
            }
            FakeMd::Depth => {
                let bids = levels(p, "b", pe, qe)?;
                let asks = levels(p, "a", pe, qe)?;
                let mut s = L2SnapHdr::new_zeroed();
                s.h = h;
                s.snap_id = int_field(p, "u")? as u64;
                s.origin = L2SnapHdr::TOP_N;
                s.n_bids = bids.len() as u16;
                s.n_asks = asks.len() as u16;
                out.l2_snapshot(&L2SnapshotPart {
                    hdr: s,
                    bids: &bids,
                    asks: &asks,
                    part: 0,
                    parts: 1,
                });
            }
            FakeMd::Unknown => return Err(DecodeErr::UnknownClass),
        }
        Ok(MdOutcome { events: 1, inst })
    }
}

/// Raw bytes after `"k":` up to the next `,`, `}` or `]` (quotes stripped).
fn raw_field<'a>(p: &'a [u8], k: &str) -> Option<&'a [u8]> {
    let key = format!("\"{k}\":");
    let at = p.windows(key.len()).position(|w| w == key.as_bytes())? + key.len();
    let rest = &p[at..];
    let end = rest
        .iter()
        .position(|&b| b == b',' || b == b'}' || b == b']')
        .unwrap_or(rest.len());
    Some(&rest[..end])
}

fn str_field<'a>(p: &'a [u8], k: &str) -> Option<&'a [u8]> {
    raw_field(p, k).map(|v| {
        v.strip_prefix(b"\"")
            .unwrap_or(v)
            .strip_suffix(b"\"")
            .unwrap_or(v)
    })
}

fn int_field(p: &[u8], k: &str) -> Result<i64, DecodeErr> {
    let v = str_field(p, k).ok_or(DecodeErr::Truncated)?;
    std::str::from_utf8(v)
        .ok()
        .and_then(|s| s.parse().ok())
        .ok_or(DecodeErr::Number)
}

/// Decimal text → integer at exponent `exp` (exact; more fraction digits than `-exp` → Number).
fn fixed(s: &[u8], exp: i8) -> Result<i64, DecodeErr> {
    let scale = u32::try_from(-i32::from(exp)).map_err(|_| DecodeErr::Number)?;
    let (int, frac) = match s.iter().position(|&b| b == b'.') {
        Some(i) => (&s[..i], &s[i + 1..]),
        None => (s, &s[..0]),
    };
    if int.is_empty() || frac.len() > scale as usize || !int.iter().chain(frac).all(u8::is_ascii_digit) {
        return Err(DecodeErr::Number);
    }
    let mut v: i64 = 0;
    for &d in int.iter().chain(frac) {
        v = v
            .checked_mul(10)
            .and_then(|v| v.checked_add(i64::from(d - b'0')))
            .ok_or(DecodeErr::Overflow)?;
    }
    v.checked_mul(10i64.pow(scale - frac.len() as u32))
        .ok_or(DecodeErr::Overflow)
}

/// `"k":[["px","qty"],…]` → levels.
fn levels(p: &[u8], k: &str, pe: i8, qe: i8) -> Result<Vec<Level>, DecodeErr> {
    let key = format!("\"{k}\":[");
    let at = p
        .windows(key.len())
        .position(|w| w == key.as_bytes())
        .ok_or(DecodeErr::Truncated)?
        + key.len();
    let mut rest = &p[at..];
    let mut out = Vec::new();
    while let Some(b'[') = rest.first() {
        let end = rest.iter().position(|&b| b == b']').ok_or(DecodeErr::Truncated)?;
        let pair = &rest[1..end];
        let comma = pair.iter().position(|&b| b == b',').ok_or(DecodeErr::Shape)?;
        let unq = |v: &[u8]| -> Vec<u8> { v.iter().copied().filter(|&b| b != b'"').collect() };
        out.push(Level {
            px: Px(fixed(&unq(&pair[..comma]), pe)?),
            qty: Qty(fixed(&unq(&pair[comma + 1..]), qe)?),
        });
        rest = &rest[end + 1..];
        rest = rest.strip_prefix(b",").unwrap_or(rest);
    }
    Ok(out)
}

/// 20-level depth sample text.
pub fn depth20_payload(inst: u32) -> String {
    let side = |base: u32, step: i32| -> String {
        (0..20)
            .map(|i| {
                let px = base as i32 + step * i;
                format!("[\"{px}.5\",\"{}\"]", i + 1)
            })
            .collect::<Vec<_>>()
            .join(",")
    };
    format!(
        "{{\"e\":\"depth\",\"i\":{inst},\"u\":5,\"T\":1700000000000,\"b\":[{}],\"a\":[{}]}}",
        side(100, -1),
        side(101, 1)
    )
}

use hft_venue_proto::{AcctProtocol, AcctQuery, AcctQueryKind};
#[derive(Default)]
pub struct AcctScratch {
    pub bits: u8,
    pub amount: i64,
    pub qty: i64,
}
impl<const SPOT: bool, const MUT: u8> AcctProtocol for Fake<SPOT, MUT> {
    type AcctCtx = u32;
    type AcctScratch = AcctScratch;
    fn acct_queries(k: AcctQueryKind) -> &'static [AcctQuery] {
        const CONFIG: [AcctQuery; 3] = [
            AcctQuery {
                kind: AcctQueryKind::Config,
                step: 0,
                every: 1,
                push_fallback: false,
                weight: 2,
            },
            AcctQuery {
                kind: AcctQueryKind::Config,
                step: 1,
                every: 1,
                push_fallback: false,
                weight: 3,
            },
            AcctQuery {
                kind: AcctQueryKind::Config,
                step: 2,
                every: 1,
                push_fallback: false,
                weight: 1,
            },
        ];
        const MARGIN: [AcctQuery; 2] = [
            AcctQuery {
                kind: AcctQueryKind::Margin,
                step: 0,
                every: 1,
                push_fallback: true,
                weight: 5,
            },
            AcctQuery {
                kind: AcctQueryKind::Margin,
                step: 1,
                every: 2,
                push_fallback: true,
                weight: 5,
            },
        ];
        if k == AcctQueryKind::Config {
            &CONFIG
        } else {
            &MARGIN
        }
    }
    fn acct_request(
        _rd: &RefData,
        _t: &(),
        acct: AccountId,
        q: AcctQuery,
        _clk: &WallClock,
        out: &mut RestReqBuf,
    ) -> Result<(), EncodeErr> {
        let _ = acct;
        out.path_query
            .push(b"/account?signature=test-signed-value")
            .unwrap();
        out.redacted = true;
        out.endpoint = if q.kind == AcctQueryKind::Config {
            q.step % 2
        } else {
            0
        };
        Ok(())
    }
    fn acct_decode(
        _rd: &RefData,
        _ctx: &mut u32,
        _acct: AccountId,
        q: AcctQuery,
        status: u16,
        body: &[u8],
        s: &mut AcctScratch,
    ) -> Result<(), DecodeErr> {
        if status != 200 || body.len() < 8 {
            return Err(DecodeErr::Shape);
        }
        let value = i64::from_le_bytes(body[..8].try_into().unwrap());
        s.bits |= 1 << q.step;
        if q.kind == AcctQueryKind::Margin {
            if q.step == 0 {
                s.amount = value;
            } else {
                s.qty = value;
            }
        }
        Ok(())
    }
    fn acct_finish(
        _rd: &RefData,
        ctx: &mut u32,
        acct: AccountId,
        k: AcctQueryKind,
        s: &AcctScratch,
        out: &mut impl ReportSink,
    ) -> Result<(), DecodeErr> {
        *ctx += 1;
        if k == AcctQueryKind::Config {
            if s.bits != 7 {
                return Err(DecodeErr::Shape);
            }
            let mut h = hft_event::AccountConfigReportHdr::new_zeroed();
            h.venue = FAKE_VENUE;
            h.account = acct;
            h.uid_keys = if SPOT { 3 } else { 2 };
            h.venue_uid[..2].copy_from_slice(b"U1");
            h.n_lev = 1;
            out.account_config(
                &h,
                &[hft_event::LeverageItem {
                    inst: InstId(201),
                    _pad: 0,
                    leverage: hft_types::Rate(5_000_000_000),
                }],
            );
        } else {
            let mut h = hft_event::MarginReportHdr::new_zeroed();
            h.venue = FAKE_VENUE;
            h.account = acct;
            h.margin_balance = hft_types::Notional(s.amount);
            h.n = 1;
            out.margin(
                &h,
                &[hft_event::MarginItem {
                    inst: InstId(201),
                    adl_quantile: 255,
                    _pad: [0; 3],
                    liq_px: Px(0),
                }],
            );
            let mut p = hft_event::PositionReport::new_zeroed();
            p.venue = FAKE_VENUE;
            p.account = acct;
            p.inst = InstId(201);
            p.qty = Qty(s.qty);
            out.position(&p);
            p.inst = InstId(202);
            p.qty = Qty(-s.qty);
            out.position(&p);
        }
        Ok(())
    }
}
