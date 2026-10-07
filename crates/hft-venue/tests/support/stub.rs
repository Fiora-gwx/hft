//! `StubProto` (TESTS.md "夹具"): a `VenueProtocol` over `|`-separated text lines, with switches
//! (`StubCfg`, per test thread), hooks and a per-thread `CallLog`.
//!
//! Market data: `B|sym|u|bid|bq|ask|aq`, `T|sym|tid|px|qty|B/S`, `M|sym|ex_ms|px`, `I|sym|ex_ms|px`,
//! `F|sym|ex_ms|rate|next_ms`, `D|sym|seq|n|px qty|…` (n bids then n asks), `DT|sym|ex_ms|n|…`;
//! control `ACK|id|ok|code`, `PONG|x`, `BYE`. Subscriptions `SUB|i1,i2…` / `UNSUB|…` (inst ids,
//! `subs_per_frame` per frame).
//! Orders: `P|req|cid|inst|side|px|qty|exp|oflags`, `C|req|cid|exch|inst`, `A|req|cid|px|qty`,
//! `Q|req|cid|exch`, `CA|req|inst`, `BC|req|cid,cid…`, `DM|timeout|i1,i2…`; `req` = `slot-gen-op-epoch`.
//! Reports: `O|cid|exch|what|qty|cum|avg|fee|fee_asset|tid`, `R|req|code[|used|limit|window]`,
//! `E|name`, `Z|…` (known class, bad fields).
#![allow(dead_code)]

use std::cell::RefCell;
use std::thread::ThreadId;

use hft_event::{
    Bbo, GwCmd, GwOp, L2SnapHdr, L2SnapshotPart, Level, MarkPrice, MarketHdr, OrderEvent, OrderEventKind,
    Trade,
};
use hft_refdata::{MarginData, RefData, VenueCaps};
use hft_types::{AccountId, AssetId, ClientOrderId, InstId, MarketKind, Px, Qty, Side, SlotId, VenueId};
use hft_venue::exch_id_value;
use hft_venue_proto::{
    Align, BootCall, ControlHint, Credential, DeadManOut, DeadManSpec, DecodeErr, EncodeErr, EndpointSpec,
    ExchIdHint, HeartbeatSpec, LoginResult, LoginStep, MdOutcome, MdSink, MsgOp, OrderChannel, Peek,
    PingKind, ProtoErr, RateObs, RateRule, RawSeq, ReportCtl, ReportOutcome, ReportSink, ReqRef, RestReqBuf,
    ResyncMethod, Role, RouteId, RouteKind, RouteSpec, RxMeta, SeqIds, SeqRule, Skip, SnapshotInfo,
    StaleGuard, StreamKind, SubFrames, SubSpec, TargetRef, VenueCfg, VenueProtocol, Via, WallClock,
};
use zerocopy::FromZeros;

use super::rd::V7;

/// Variant bits of `Stub<FCAP, V>`.
pub const V_SWEEP: u32 = 1;
pub const V_SIGN: u32 = 2;
pub const V_REST_CANCEL_ALL: u32 = 4;
pub const V_USER_STREAM: u32 = 8;

pub struct Stub<const FCAP: usize, const V: u32>;
pub type StubProto = Stub<2048, 0>;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum LoginMode {
    None,
    One,
    Two,
}

#[derive(Clone, Debug)]
pub struct StubCfg {
    pub bbo_snapshot: bool,
    pub bbo_batch: usize,
    pub account_cancel_all: bool,
    pub inst_cancel_all: bool,
    pub query_by_cid: bool,
    pub dead_man: DeadManSpec,
    pub session_identity: bool,
    pub rest_query: bool,
    pub keepalive: bool,
    pub encode_batch_ok: bool,
    pub subs_per_frame: usize,
    pub bootstrap: bool,
    pub login: LoginMode,
    pub hb_md: HeartbeatSpec,
    pub hb_order: HeartbeatSpec,
    pub probe: PingKind,
    /// `encode` fails with this error (VEN-199.6).
    pub encode_err: Option<EncodeErr>,
}

pub const HB_OFF: HeartbeatSpec = HeartbeatSpec {
    ping: PingKind::WsPing,
    interval_ms: 0,
    unconditional: false,
    pong_timeout_ms: 0,
    idle_timeout_ms: 0,
};

impl Default for StubCfg {
    fn default() -> Self {
        StubCfg {
            bbo_snapshot: true,
            bbo_batch: 4,
            account_cancel_all: true,
            inst_cancel_all: true,
            query_by_cid: true,
            dead_man: DeadManSpec::CountdownPerInst {
                min_ms: 5000,
                timeout_ms: 30_000,
                refresh_ms: 10_000,
                via: Via::Ws,
            },
            session_identity: false,
            rest_query: false,
            keepalive: false,
            encode_batch_ok: true,
            subs_per_frame: 10,
            bootstrap: false,
            login: LoginMode::None,
            hb_md: HB_OFF,
            hb_order: HB_OFF,
            probe: PingKind::WsPing,
            encode_err: None,
        }
    }
}

/// One trait call.
#[derive(Clone, Debug)]
pub struct Call {
    pub name: &'static str,
    pub thread: ThreadId,
    pub arg: String,
    /// `decode_md`: the `RxMeta` it got.
    pub rx: Option<RxMeta>,
    /// `encode`: the wall clock it got.
    pub clk: Option<WallClock>,
}

type Hook = Box<dyn FnMut(&GwCmd)>;
type ReportHook = Box<dyn FnMut(&[u8])>;

thread_local! {
    static CFG: RefCell<StubCfg> = RefCell::new(StubCfg::default());
    static LOG: RefCell<Vec<Call>> = const { RefCell::new(Vec::new()) };
    static ENCODE_HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) };
    static REPORT_HOOK: RefCell<Option<ReportHook>> = const { RefCell::new(None) };
    static LOG_ON: RefCell<bool> = const { RefCell::new(true) };
}

pub fn set_cfg(f: impl FnOnce(&mut StubCfg)) {
    CFG.with(|c| f(&mut c.borrow_mut()));
}
pub fn cfg() -> StubCfg {
    CFG.with(|c| c.borrow().clone())
}
pub fn set_encode_hook(h: Option<Hook>) {
    ENCODE_HOOK.with(|x| *x.borrow_mut() = h);
}
pub fn set_report_hook(h: Option<ReportHook>) {
    REPORT_HOOK.with(|x| *x.borrow_mut() = h);
}
/// Takes this thread's call log.
pub fn take_log() -> Vec<Call> {
    LOG.with(|l| std::mem::take(&mut *l.borrow_mut()))
}
/// Calls of `name` in this thread's log (not taken).
pub fn calls(name: &str) -> Vec<Call> {
    LOG.with(|l| l.borrow().iter().filter(|c| c.name == name).cloned().collect())
}
pub fn log_enabled(on: bool) {
    LOG_ON.with(|x| *x.borrow_mut() = on);
}
fn log(name: &'static str, arg: impl FnOnce() -> String) {
    log_full(name, arg, None, None);
}
fn log_full(name: &'static str, arg: impl FnOnce() -> String, rx: Option<RxMeta>, clk: Option<WallClock>) {
    if !LOG_ON.with(|x| *x.borrow()) {
        return;
    }
    let arg = arg();
    LOG.with(|l| {
        l.borrow_mut().push(Call {
            name,
            thread: std::thread::current().id(),
            arg,
            rx,
            clk,
        })
    });
}

/// Writes formatted text into a byte slice (no allocation).
struct SliceW<'a> {
    b: &'a mut [u8],
    n: usize,
}

impl core::fmt::Write for SliceW<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let end = self.n + s.len();
        let d = self.b.get_mut(self.n..end).ok_or(core::fmt::Error)?;
        d.copy_from_slice(s.as_bytes());
        self.n = end;
        Ok(())
    }
}

/// Canonical client id text of `id` (24 lowercase hex), allocation-free.
fn canon(id: &ClientOrderId) -> [u8; 24] {
    let mut b = [0u8; 24];
    id.encode(&mut b);
    b
}

/// Field `i` of a `|`-separated line.
fn field(p: &[u8], i: usize) -> Option<&[u8]> {
    p.split(|&b| b == b'|').nth(i)
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Md {
    Bbo,
    Trade,
    Mark,
    Index,
    Funding,
    Depth,
    DepthT,
    Ack,
    Pong,
    Bye,
    Unknown,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Rep {
    Order,
    Resp,
    Event,
    Ignorable,
    Bad,
    Unknown,
}

#[derive(Debug)]
pub struct Tpl {
    pub account: AccountId,
    pub stale_ms: u32,
}

#[derive(Debug, Default)]
pub struct GwCtx;

/// Fields of a `|`-separated line (fixed array, no allocation).
struct Fields<'a> {
    v: [&'a [u8]; 48],
    n: usize,
}

impl<'a> Fields<'a> {
    fn get(&self, i: usize) -> Option<&&'a [u8]> {
        if i < self.n { self.v.get(i) } else { None }
    }
    fn first(&self) -> Option<&&'a [u8]> {
        self.get(0)
    }
    fn len(&self) -> usize {
        self.n
    }
}

fn fields(p: &[u8]) -> Fields<'_> {
    let mut f = Fields { v: [&[]; 48], n: 0 };
    for (i, x) in p.split(|&b| b == b'|').enumerate().take(48) {
        f.v[i] = x;
        f.n = i + 1;
    }
    f
}

fn num(b: Option<&&[u8]>) -> Result<i64, DecodeErr> {
    let b = b.ok_or(DecodeErr::Truncated)?;
    if b.is_empty() {
        return Err(DecodeErr::Truncated);
    }
    std::str::from_utf8(b)
        .ok()
        .and_then(|s| s.parse().ok())
        .ok_or(DecodeErr::Number)
}

fn fx(b: Option<&&[u8]>, exp: i8) -> Result<i64, DecodeErr> {
    let b = b.ok_or(DecodeErr::Truncated)?;
    hft_types::parse_exact(b, exp).map_err(|e| {
        if e == hft_types::FxErr::OVERFLOW {
            DecodeErr::Overflow
        } else {
            DecodeErr::Number
        }
    })
}

fn inst_of(rd: &RefData, sym: Option<&&[u8]>) -> InstId {
    sym.and_then(|s| rd.symbols(V7).get(s)).unwrap_or(InstId(0))
}

fn hdr(rx: &RxMeta, inst: InstId) -> MarketHdr {
    let mut h = MarketHdr::new_zeroed();
    h.venue = V7;
    h.conn = rx.conn;
    h.inst = inst;
    h.hw_rx_ns = rx.hw_rx_ns;
    h.rx_tsc = rx.rx_tsc;
    h.sw_rx_ns = rx.sw_rx_ns;
    h.rx_empty_gap_ns = rx.rx_empty_gap_ns;
    h
}

static ROUTES: [RouteSpec; 3] = [
    RouteSpec {
        id: RouteId(0),
        kind: RouteKind::Bbo,
        name: "bbo",
        race: true,
        max_streams: 200,
        max_in_msgs_per_s: 20,
        topn_levels: &[],
    },
    RouteSpec {
        id: RouteId(1),
        kind: RouteKind::DepthTopN,
        name: "depth",
        race: false,
        max_streams: 200,
        max_in_msgs_per_s: 20,
        topn_levels: &[5, 20],
    },
    RouteSpec {
        id: RouteId(2),
        kind: RouteKind::Md,
        name: "md",
        race: false,
        max_streams: 200,
        max_in_msgs_per_s: 20,
        topn_levels: &[],
    },
];

/// `"st-"` + canonical text.
pub fn cid_text(id: &ClientOrderId) -> String {
    let mut b = [0u8; 24];
    id.encode(&mut b);
    format!("st-{}", String::from_utf8_lossy(&b))
}

fn op_code(op: GwOp) -> u8 {
    op as u8
}

fn req_text(slot: u32, r#gen: u32, op: GwOp, epoch: u16) -> String {
    format!("{slot}-{gen}-{}-{epoch}", op_code(op), gen = r#gen)
}

/// `slot-gen-op-epoch` → parts.
pub fn parse_req(b: &[u8]) -> Option<(ReqRef, u16)> {
    let s = std::str::from_utf8(b).ok()?;
    let mut it = s.split('-');
    let slot = it.next()?.parse().ok()?;
    let r#gen = it.next()?.parse().ok()?;
    let op: u8 = it.next()?.parse().ok()?;
    let epoch = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
    let op = match op {
        1 => GwOp::Place,
        2 => GwOp::Cancel,
        3 => GwOp::Amend,
        4 => GwOp::CancelAll,
        5 => GwOp::Query,
        _ => return None,
    };
    Some((ReqRef { slot, r#gen, op }, epoch))
}

fn what_of(b: &[u8]) -> Option<OrderEventKind> {
    Some(match b {
        b"Ack" => OrderEventKind::Ack,
        b"Reject" => OrderEventKind::Reject,
        b"PartialFill" => OrderEventKind::PartialFill,
        b"Fill" => OrderEventKind::Fill,
        b"CancelAck" => OrderEventKind::CancelAck,
        b"CancelReject" => OrderEventKind::CancelReject,
        b"AmendAck" => OrderEventKind::AmendAck,
        b"AmendReject" => OrderEventKind::AmendReject,
        b"Expired" => OrderEventKind::Expired,
        b"QueryResult" => OrderEventKind::QueryResult,
        b"FeeOnly" => OrderEventKind::FeeOnly,
        _ => return None,
    })
}

fn blank_order(account: AccountId) -> OrderEvent {
    OrderEvent {
        venue: V7,
        account,
        what: OrderEventKind::Ack,
        detail: 0,
        slot: SlotId::UNRESOLVED,
        r#gen: 0,
        inst: InstId(0),
        px: Px(0),
        qty: Qty(0),
        cum_filled: Qty(0),
        fee: Qty(0),
        fee_asset: AssetId(0),
        epoch: 0,
        reject_code: 0,
        exch_order_id: 0,
        ex_ts_ns: 0,
        hw_rx_ns: 0,
        rx_tsc: 0,
        avg_px: Px(0),
    }
}

impl<const FCAP: usize, const V: u32> VenueProtocol for Stub<FCAP, V> {
    const NAME: &'static str = "stub-perp";
    const VENUE: VenueId = V7;
    const MARKET: MarketKind = MarketKind::LinearPerp;
    const ORDER_FRAME_CAP: usize = FCAP;
    type MdClass = Md;
    type ReportClass = Rep;
    type FeedCtx = ();
    type GwCtx = GwCtx;
    type Templates = Tpl;
    const CANCEL_ALL_SWEEP: bool = V & V_SWEEP != 0;
    const BBO_SNAPSHOT_BATCH: u16 = u16::MAX;
    const USER_STREAM: bool = V & V_USER_STREAM != 0;

    fn default_caps(_asset: impl Fn(&str) -> Option<AssetId>) -> VenueCaps {
        log("default_caps", String::new);
        let mut c = VenueCaps::none(V7);
        c.margin_data = MarginData::Poll { interval_ms: 5000 };
        c
    }

    fn bootstrap(role: Role, _cfg: &VenueCfg, cred: Option<&Credential>) -> Option<BootCall> {
        log("bootstrap", || format!("{role:?} cred={}", cred.is_some()));
        (cfg().bootstrap && role == Role::UserStream).then(RestReqBuf::default)
    }

    fn routes() -> &'static [RouteSpec] {
        &ROUTES
    }

    fn endpoint(
        role: Role,
        _route: RouteId,
        _cfg: &VenueCfg,
        cred: Option<&Credential>,
        boot: Option<&[u8]>,
        _now_ms: i64,
        out: &mut EndpointSpec,
    ) -> Result<(), ProtoErr> {
        log("endpoint", || {
            format!("{role:?} cred={} boot={}", cred.is_some(), boot.is_some())
        });
        out.path_query.push(b"/stub")?;
        if let Some(c) = cred {
            out.path_query.push(b"?key=")?;
            out.path_query.push(c.api_key.as_slice())?;
        }
        Ok(())
    }

    fn login(
        role: Role,
        _cfg: &VenueCfg,
        cred: &Credential,
        step: u8,
        _now_ms: i64,
        out: &mut Vec<u8>,
    ) -> Result<LoginStep, ProtoErr> {
        log("login", || format!("{role:?} step={step}"));
        let mode = cfg().login;
        match mode {
            LoginMode::None => Ok(LoginStep::NotNeeded),
            LoginMode::One | LoginMode::Two => {
                out.extend_from_slice(format!("LOGIN{step}|").as_bytes());
                out.extend_from_slice(cred.api_key.as_slice());
                Ok(if mode == LoginMode::One {
                    LoginStep::Frame
                } else {
                    LoginStep::Handshake
                })
            }
        }
    }

    fn login_result(payload: &[u8]) -> LoginResult {
        let f = fields(payload);
        match f.first().copied() {
            Some(b"LOGIN_OK") => LoginResult::Ok,
            Some(b"LOGIN_CONT") => LoginResult::Continue,
            Some(b"LOGIN_REJ") => LoginResult::Rejected(num(f.get(1)).unwrap_or(0) as u32),
            _ => LoginResult::NotMine,
        }
    }

    fn subscribe(
        _rd: &hft_refdata::RefData,
        subs: &[SubSpec],
        unsub: bool,
        _cfg: &VenueCfg,
        out: &mut SubFrames,
    ) -> Result<(), ProtoErr> {
        let per = cfg().subs_per_frame.max(1);
        for chunk in subs.chunks(per) {
            let ids: Vec<String> = chunk
                .iter()
                .map(|s| {
                    let k = match s.stream {
                        StreamKind::Bbo => "b",
                        StreamKind::DepthTopN => "d",
                        StreamKind::Trade => "t",
                        StreamKind::Mark => "m",
                        StreamKind::Index => "i",
                        StreamKind::Funding => "f",
                        _ => "x",
                    };
                    format!(
                        "{}{k}{}",
                        s.inst.0,
                        if s.levels > 0 {
                            format!("@{}", s.levels)
                        } else {
                            String::new()
                        }
                    )
                })
                .collect();
            let text = format!("{}|{}", if unsub { "UNSUB" } else { "SUB" }, ids.join(","));
            out.push(text.as_bytes(), chunk.len() as u16)?;
        }
        Ok(())
    }

    fn private_subscribe(
        _cfg: &VenueCfg,
        acct: AccountId,
        unsub: bool,
        out: &mut SubFrames,
    ) -> Result<(), ProtoErr> {
        log("private_subscribe", || format!("{} {unsub}", acct.0));
        out.push(format!("PSUB|{}", acct.0).as_bytes(), 1)
    }

    fn heartbeat(role: Role) -> HeartbeatSpec {
        let c = cfg();
        if role == Role::Md { c.hb_md } else { c.hb_order }
    }

    fn app_ping(_role: Role, now_ms: i64, out: &mut [u8]) -> usize {
        let t = format!("PING|{now_ms}");
        match out.get_mut(..t.len()) {
            Some(d) => {
                d.copy_from_slice(t.as_bytes());
                t.len()
            }
            None => 0,
        }
    }

    fn bbo_probe() -> PingKind {
        cfg().probe
    }

    fn session_identity_request(
        _cfg: &VenueCfg,
        _cred: &Credential,
        _now_ms: i64,
        out: &mut RestReqBuf,
    ) -> Result<bool, ProtoErr> {
        log("session_identity_request", String::new);
        if cfg().session_identity {
            out.path_query.push(b"/uid")?;
            out.redacted = true;
            return Ok(true);
        }
        Ok(false)
    }

    fn decode_identity(_status: u16, body: &[u8], uid: &mut [u8; 24]) -> Result<(), DecodeErr> {
        let n = body.len().min(24);
        uid[..n].copy_from_slice(&body[..n]);
        Ok(())
    }

    fn classify(_route: RouteId, _op: MsgOp, head: &[u8]) -> Md {
        log("classify", || {
            String::from_utf8_lossy(head.get(..head.len().min(24)).unwrap_or(&[])).into_owned()
        });
        let k = head.split(|&b| b == b'|').next().unwrap_or(&[]);
        match k {
            b"B" => Md::Bbo,
            b"T" => Md::Trade,
            b"M" => Md::Mark,
            b"I" => Md::Index,
            b"F" => Md::Funding,
            b"D" => Md::Depth,
            b"DT" => Md::DepthT,
            b"ACK" => Md::Ack,
            b"PONG" => Md::Pong,
            b"BYE" => Md::Bye,
            _ => Md::Unknown,
        }
    }

    fn peek(rd: &RefData, _ctx: &(), c: Md, p: &[u8]) -> Result<Peek, DecodeErr> {
        let f = fields(p);
        let inst = inst_of(rd, f.get(1));
        let (stream, id) = match c {
            Md::Bbo => (StreamKind::Bbo, num(f.get(2))?),
            Md::Trade => (StreamKind::Trade, num(f.get(2))?),
            Md::Mark => (StreamKind::Mark, num(f.get(2))?),
            Md::Index => (StreamKind::Index, num(f.get(2))?),
            Md::Funding => (StreamKind::Funding, num(f.get(2))?),
            Md::Depth | Md::DepthT => (StreamKind::DepthTopN, num(f.get(2))?),
            _ => return Err(DecodeErr::UnknownClass),
        };
        Ok(Peek {
            inst,
            stream,
            ids: SeqIds {
                prev_id: 0,
                last_id: u64::try_from(id).map_err(|_| DecodeErr::Number)?,
            },
            raw: RawSeq::default(),
        })
    }

    fn seq_rule(c: Md) -> SeqRule {
        match c {
            Md::Bbo | Md::Trade => SeqRule::Monotonic,
            Md::Mark | Md::Index | Md::Funding | Md::DepthT => SeqRule::ByTime,
            Md::Depth => SeqRule::Snapshot,
            _ => SeqRule::None,
        }
    }

    fn decode_md(
        rd: &RefData,
        _ctx: &mut (),
        c: Md,
        p: &[u8],
        rx: &RxMeta,
        out: &mut impl MdSink,
    ) -> Result<MdOutcome, DecodeErr> {
        log_full(
            "decode_md",
            || String::from_utf8_lossy(p.get(..p.len().min(48)).unwrap_or(&[])).into_owned(),
            Some(*rx),
            None,
        );
        let f = fields(p);
        let inst = inst_of(rd, f.get(1));
        if inst.0 == 0 {
            return Err(DecodeErr::Shape);
        }
        let row = rd.inst(inst);
        let (pe, qe) = (row.px_exp, row.qty_exp);
        let h = hdr(rx, inst);
        match c {
            Md::Bbo => {
                let e = Bbo {
                    h,
                    bid_px: Px(fx(f.get(3), pe)?),
                    bid_qty: Qty(fx(f.get(4), qe)?),
                    ask_px: Px(fx(f.get(5), pe)?),
                    ask_qty: Qty(fx(f.get(6), qe)?),
                    update_id: num(f.get(2))? as u64,
                };
                out.bbo(&e);
            }
            Md::Trade => {
                let e = Trade {
                    h,
                    px: Px(fx(f.get(3), pe)?),
                    qty: Qty(fx(f.get(4), qe)?),
                    trade_id: num(f.get(2))? as u64,
                    taker_side: if f.get(5).copied() == Some(b"S") {
                        Side::Sell
                    } else {
                        Side::Buy
                    },
                    _pad: [0; 7],
                };
                out.trade(&e);
            }
            Md::Mark => {
                let mut e = MarkPrice::new_zeroed();
                e.h = h;
                e.h.ex_ts_ns = num(f.get(2))?.saturating_mul(1_000_000);
                e.px = Px(fx(f.get(3), pe)?);
                out.mark(&e);
            }
            Md::Depth | Md::DepthT => {
                let n = num(f.get(3))? as usize;
                if n > 20 {
                    return Err(DecodeErr::Shape);
                }
                let mut lv = [Level {
                    px: Px(0),
                    qty: Qty(0),
                }; 40];
                for (k, slot) in lv.iter_mut().enumerate().take(2 * n) {
                    let pq = f.get(4 + k).ok_or(DecodeErr::Truncated)?;
                    let mut it = pq.split(|&b| b == b' ');
                    let px = fx(it.next().as_ref(), pe)?;
                    let qty = fx(it.next().as_ref(), qe)?;
                    *slot = Level {
                        px: Px(px),
                        qty: Qty(qty),
                    };
                }
                let mut s = L2SnapHdr::new_zeroed();
                s.h = h;
                s.snap_id = if c == Md::Depth { num(f.get(2))? as u64 } else { 0 };
                s.origin = L2SnapHdr::TOP_N;
                s.n_bids = n as u16;
                s.n_asks = n as u16;
                out.l2_snapshot(&L2SnapshotPart {
                    hdr: s,
                    bids: &lv[..n],
                    asks: &lv[n..2 * n],
                    part: 0,
                    parts: 1,
                });
            }
            _ => return Err(DecodeErr::UnknownClass),
        }
        Ok(MdOutcome { events: 1, inst })
    }

    fn control(c: Md, p: &[u8]) -> Option<ControlHint> {
        let f = fields(p);
        match c {
            Md::Ack => Some(ControlHint::SubAck {
                ok: f.get(2).copied() == Some(b"1"),
                code: num(f.get(3)).unwrap_or(0) as u32,
            }),
            Md::Pong => Some(ControlHint::Pong),
            Md::Bye => Some(ControlHint::ServerShutdown),
            _ => None,
        }
    }

    fn resync_method(_c: Md) -> ResyncMethod {
        ResyncMethod::None
    }

    fn align_first(_snap_id: u64, _raw: &RawSeq) -> Align {
        Align::Gap
    }

    fn snapshot_request(
        _rd: &RefData,
        _ctx: &(),
        _inst: InstId,
        _out: &mut RestReqBuf,
    ) -> Result<(), ProtoErr> {
        Err(ProtoErr::Unsupported)
    }

    fn decode_snapshot(
        _rd: &RefData,
        _ctx: &mut (),
        _inst: InstId,
        _body: &[u8],
        _rx: &RxMeta,
        _out: &mut impl MdSink,
    ) -> Result<SnapshotInfo, DecodeErr> {
        Err(DecodeErr::UnknownClass)
    }

    fn bbo_snapshot_request(
        _rd: &RefData,
        _ctx: &(),
        insts: &[InstId],
        out: &mut RestReqBuf,
    ) -> Result<bool, ProtoErr> {
        let c = cfg();
        if !c.bbo_snapshot {
            return Ok(false);
        }
        if insts.len() > c.bbo_batch {
            return Err(ProtoErr::BufFull);
        }
        out.path_query.push(b"/bbo")?;
        Ok(true)
    }

    fn build_templates(
        cfg: &VenueCfg,
        _rd: &RefData,
        acct: AccountId,
        _rest: &Credential,
    ) -> Result<Tpl, ProtoErr> {
        Ok(Tpl {
            account: acct,
            stale_ms: cfg.stale_guard_ms,
        })
    }

    fn order_channel(op: GwOp) -> OrderChannel {
        let c = cfg();
        if (op == GwOp::Query && c.rest_query) || (op == GwOp::CancelAll && V & V_REST_CANCEL_ALL != 0) {
            OrderChannel::Rest
        } else {
            OrderChannel::Ws
        }
    }

    fn encode(
        _rd: &RefData,
        t: &Tpl,
        c: &GwCmd,
        tgt: &TargetRef,
        clk: &WallClock,
        body: &mut [u8],
    ) -> Result<usize, EncodeErr> {
        ENCODE_HOOK.with(|h| {
            if let Some(f) = h.borrow_mut().as_mut() {
                f(c);
            }
        });
        log_full(
            "encode",
            || format!("{:?} slot={} inst={}", c.op, c.slot.0, c.inst.0),
            None,
            Some(*clk),
        );
        let k = cfg();
        if let Some(e) = k.encode_err {
            return Err(e);
        }
        let cb = canon(&tgt.id);
        let cid = std::str::from_utf8(&cb).map_err(|_| EncodeErr::FieldOverflow)?;
        let (slot, r#gen) = if c.op == GwOp::Place {
            (c.slot.0, c.r#gen)
        } else {
            (tgt.id.slot, tgt.id.r#gen)
        };
        let op = op_code(c.op);
        let ep = tgt.id.epoch;
        let mut w = SliceW { b: body, n: 0 };
        use core::fmt::Write as _;
        let r = match c.op {
            GwOp::Place => write!(
                w,
                "P|{slot}-{g}-{op}-{ep}|st-{cid}|{}|{}|{}|{}|{}|{}",
                c.inst.0,
                c.side as u8,
                c.px.0,
                c.qty.0,
                clk.unix_ms + i64::from(t.stale_ms),
                c.oflags,
                g = r#gen
            ),
            GwOp::Cancel => {
                if !k.query_by_cid && tgt.exch_order_id == 0 {
                    return Err(EncodeErr::UnknownTarget);
                }
                write!(
                    w,
                    "C|{slot}-{g}-{op}-{ep}|st-{cid}|{}|{}",
                    tgt.exch_order_id,
                    c.inst.0,
                    g = r#gen
                )
            }
            GwOp::Amend => write!(
                w,
                "A|{slot}-{g}-{op}-{ep}|st-{cid}|{}|{}",
                c.px.0,
                c.qty.0,
                g = r#gen
            ),
            GwOp::Query => {
                if !k.query_by_cid && tgt.exch_order_id == 0 {
                    return Err(EncodeErr::UnknownTarget);
                }
                write!(
                    w,
                    "Q|{slot}-{g}-{op}-{ep}|st-{cid}|{}",
                    tgt.exch_order_id,
                    g = r#gen
                )
            }
            GwOp::CancelAll => {
                if c.inst.0 == 0 && !k.account_cancel_all {
                    return Err(EncodeErr::Unsupported(GwOp::CancelAll));
                }
                if c.inst.0 != 0 && !k.inst_cancel_all {
                    return Err(EncodeErr::Unsupported(GwOp::CancelAll));
                }
                write!(w, "CA|0-0-4-0|{}", c.inst.0)
            }
            op => return Err(EncodeErr::Unsupported(op)),
        };
        r.map_err(|_| EncodeErr::FieldOverflow)?;
        if V & V_SIGN != 0 {
            w.write_str("|SIG=signed").map_err(|_| EncodeErr::FieldOverflow)?;
        }
        Ok(w.n)
    }

    fn encode_batch(
        _rd: &RefData,
        _t: &Tpl,
        cs: &[GwCmd],
        tgts: &[TargetRef],
        _clk: &WallClock,
        body: &mut [u8],
    ) -> Result<usize, EncodeErr> {
        log("encode_batch", || format!("{}", cs.len()));
        if !cfg().encode_batch_ok {
            return Err(EncodeErr::Unsupported(GwOp::Place));
        }
        let ids: Vec<String> = tgts.iter().map(|t| cid_text(&t.id)).collect();
        let text = format!("BC|{}|{}", req_text(0, 0, GwOp::CancelAll, 0), ids.join(","));
        let d = body.get_mut(..text.len()).ok_or(EncodeErr::FieldOverflow)?;
        d.copy_from_slice(text.as_bytes());
        Ok(text.len())
    }

    fn render_cid(id: &ClientOrderId, out: &mut [u8; 48]) -> usize {
        let t = cid_text(id);
        out[..t.len()].copy_from_slice(t.as_bytes());
        t.len()
    }

    fn parse_cid(bytes: &[u8]) -> Option<ClientOrderId> {
        ClientOrderId::decode(bytes.strip_prefix(b"st-")?).ok()
    }

    fn classify_report(_op: MsgOp, head: &[u8]) -> Rep {
        let f = fields(head);
        match f.first().copied() {
            Some(b"O") => Rep::Order,
            Some(b"R") => Rep::Resp,
            Some(b"E") if f.get(1).copied() == Some(b"listenKeyExtended") => Rep::Ignorable,
            Some(b"E") => Rep::Event,
            Some(b"Z") => Rep::Bad,
            _ => Rep::Unknown,
        }
    }

    fn decode_report(
        rd: &RefData,
        _ctx: &mut GwCtx,
        c: Rep,
        p: &[u8],
        rx: &RxMeta,
        out: &mut impl ReportSink,
    ) -> Result<ReportOutcome, DecodeErr> {
        REPORT_HOOK.with(|h| {
            if let Some(f) = h.borrow_mut().as_mut() {
                f(p);
            }
        });
        let f = fields(p);
        let mut o = ReportOutcome::default();
        match c {
            Rep::Order => {
                let mut e = blank_order(rx.account);
                e.rx_tsc = rx.rx_tsc;
                e.hw_rx_ns = rx.hw_rx_ns;
                e.what = what_of(f.get(3).copied().unwrap_or(&[])).ok_or(DecodeErr::Shape)?;
                let exch = f.get(2).copied().unwrap_or(&[]);
                let cid = f.get(1).copied().unwrap_or(&[]);
                if !exch.is_empty() {
                    e.exch_order_id = exch_id_value(exch);
                }
                if cid.is_empty() {
                    if !exch.is_empty()
                        && let ExchIdHint::Resolved { epoch, slot, r#gen } = out.exch_id(exch, None)
                    {
                        e.epoch = epoch;
                        e.slot = slot;
                        e.r#gen = r#gen;
                    }
                } else if let Some(id) = Self::parse_cid(cid) {
                    e.epoch = id.epoch;
                    e.slot = SlotId(id.slot);
                    e.r#gen = id.r#gen;
                    if !exch.is_empty() {
                        let _ = out.exch_id(exch, Some(&id));
                    }
                }
                e.qty = Qty(num(f.get(4)).unwrap_or(0));
                e.cum_filled = Qty(num(f.get(5)).unwrap_or(0));
                e.avg_px = Px(num(f.get(6)).unwrap_or(0));
                e.fee = Qty(num(f.get(7)).unwrap_or(0));
                out.order(&e);
                o.events = 1;
            }
            Rep::Resp => {
                let (r, epoch) = parse_req(f.get(1).copied().unwrap_or(&[])).ok_or(DecodeErr::Shape)?;
                let code = num(f.get(2))?;
                o.req = Some(r);
                if f.len() >= 6 {
                    o.rate = Some(RateObs {
                        bucket: hft_refdata::LimitBucket::Orders,
                        used: num(f.get(3))? as u32,
                        limit: num(f.get(4))? as u32,
                        window_ms: num(f.get(5))? as u32,
                    });
                }
                match code {
                    0 => {}
                    -2010 => {
                        let mut e = blank_order(rx.account);
                        e.what = OrderEventKind::Reject;
                        e.slot = SlotId(r.slot);
                        e.r#gen = r.r#gen;
                        e.epoch = epoch;
                        e.reject_code = code as i32 as u32;
                        e.rx_tsc = rx.rx_tsc;
                        out.order(&e);
                        o.raw_code = code as i32 as u32;
                        o.events = 1;
                    }
                    -1125 => o.ctl = ReportCtl::UserStreamEnded,
                    _ => {
                        o.outcome_unknown = Some(r);
                        o.raw_code = code as i32 as u32;
                    }
                }
            }
            Rep::Ignorable => o.skip = Skip::Listed,
            Rep::Event => {
                let name = f.get(1).copied().unwrap_or(&[]);
                if name == b"SHUTDOWN" {
                    o.ctl = ReportCtl::ServerShutdown;
                } else if rd
                    .caps(V7)
                    .ignorable_user_events
                    .iter()
                    .any(|x| x.as_bytes() == name)
                {
                    o.skip = Skip::Listed;
                } else {
                    o.skip = Skip::Unknown;
                }
            }
            Rep::Bad => return Err(DecodeErr::Shape),
            Rep::Unknown => o.skip = Skip::Unknown,
        }
        Ok(o)
    }

    fn rate_rules() -> &'static [RateRule] {
        &[]
    }

    fn stale_guard() -> StaleGuard {
        StaleGuard::RecvWindow { default_ms: 1500 }
    }

    fn dead_man() -> DeadManSpec {
        cfg().dead_man
    }

    fn dead_man_refresh(
        _rd: &RefData,
        _t: &Tpl,
        timeout_ms: u32,
        insts: &[InstId],
        _clk: &WallClock,
        out: &mut DeadManOut,
    ) -> Result<(), EncodeErr> {
        match cfg().dead_man {
            DeadManSpec::None => Err(EncodeErr::Unsupported(GwOp::DeadManRefresh)),
            _ => {
                let ids: Vec<String> = insts.iter().map(|i| i.0.to_string()).collect();
                out.frame
                    .push(format!("DM|{timeout_ms}|{}", ids.join(",")).as_bytes())
                    .map_err(|_| EncodeErr::FieldOverflow)
            }
        }
    }

    fn needs_per_order_signing() -> bool {
        V & V_SIGN != 0
    }
}
