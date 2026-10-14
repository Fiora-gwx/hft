//! Routes, endpoints, the listenKey bootstrap, `session.logon`, subscriptions (README "端点与路由",
//! "下单:WS API", "7. 订阅节奏").

use hft_refdata::RefData;
use hft_types::{AccountId, InstId};
use hft_venue_proto::{
    BootCall, Credential, EndpointSpec, LoginResult, LoginStep, Method, ProtoErr, Role, RouteId, RouteKind,
    RouteSpec, SignAlg, StreamKind, SubFrames, SubSpec, VenueCfg,
};
use serde::Deserialize;

use crate::Line;
use crate::cid::{IdKind, id_kind};
use crate::errors::raw_code;
use crate::json::{ErrBody, from};
use crate::sign::{ed25519_b64, logon_payload};
use crate::wr::Wr;

pub(crate) fn identity_request(
    line: Line,
    cfg: &VenueCfg,
    cred: &Credential,
    now_ms: i64,
    out: &mut hft_venue_proto::RestReqBuf,
) -> Result<bool, ProtoErr> {
    let mut request = hft_venue_proto::RestReqBuf {
        endpoint: u8::from(line == Line::Usdm),
        ..Default::default()
    };
    request.path_query.push(b"/api/v3/account")?;
    crate::account::sign_request(cfg, cred, now_ms, &mut request)?;
    *out = request;
    Ok(true)
}

pub(crate) fn decode_identity(
    status: u16,
    body: &[u8],
    uid: &mut [u8; 24],
) -> Result<(), hft_venue_proto::DecodeErr> {
    use hft_venue_proto::DecodeErr;
    if !(200..300).contains(&status) {
        return Err(DecodeErr::Shape);
    }
    #[derive(Deserialize)]
    struct Identity<'a> {
        #[serde(borrow)]
        uid: Uid<'a>,
    }
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Uid<'a> {
        Number(u64),
        Text(&'a str),
    }
    let identity: Identity<'_> = from(body).map_err(|_| DecodeErr::Shape)?;
    let mut text = [0u8; 24];
    match identity.uid {
        Uid::Number(0) => return Err(DecodeErr::Shape),
        Uid::Number(n) => {
            let mut writer = Wr::new(&mut text);
            writer.u64(n);
            writer.finish().ok_or(DecodeErr::Shape)?;
        }
        Uid::Text(s) => {
            if s.is_empty()
                || s.len() > text.len()
                || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
            {
                return Err(DecodeErr::Shape);
            }
            text[..s.len()].copy_from_slice(s.as_bytes());
        }
    }
    *uid = text;
    Ok(())
}

/// USDT-M routes.
pub(crate) const ROUTE_PUBLIC_BBO: RouteId = RouteId(1);
pub(crate) const ROUTE_PUBLIC_TOPN: RouteId = RouteId(2);
pub(crate) const ROUTE_MARKET: RouteId = RouteId(3);
pub(crate) const ROUTE_PUBLIC_DEPTH: RouteId = RouteId(4);
/// Spot routes.
pub(crate) const ROUTE_SPOT_BBO: RouteId = RouteId(1);
pub(crate) const ROUTE_SPOT_TOPN: RouteId = RouteId(2);
pub(crate) const ROUTE_SPOT_MD: RouteId = RouteId(3);

const TOPN: &[u8] = &[5, 10, 20];

static USDM_ROUTES: [RouteSpec; 4] = [
    RouteSpec {
        id: ROUTE_PUBLIC_BBO,
        kind: RouteKind::Bbo,
        name: "public-bbo",
        race: true,
        max_streams: 1024,
        max_in_msgs_per_s: 10,
        topn_levels: &[],
    },
    RouteSpec {
        id: ROUTE_PUBLIC_TOPN,
        kind: RouteKind::DepthTopN,
        name: "public-topn",
        race: false,
        max_streams: 1024,
        max_in_msgs_per_s: 10,
        topn_levels: TOPN,
    },
    RouteSpec {
        id: ROUTE_MARKET,
        kind: RouteKind::Md,
        name: "market",
        race: false,
        max_streams: 1024,
        max_in_msgs_per_s: 10,
        topn_levels: &[],
    },
    RouteSpec {
        id: ROUTE_PUBLIC_DEPTH,
        kind: RouteKind::Md,
        name: "public-depth",
        race: false,
        max_streams: 1024,
        max_in_msgs_per_s: 10,
        topn_levels: &[],
    },
];

static SPOT_ROUTES: [RouteSpec; 3] = [
    RouteSpec {
        id: ROUTE_SPOT_BBO,
        kind: RouteKind::Bbo,
        name: "spot-bbo",
        race: true,
        max_streams: 1024,
        max_in_msgs_per_s: 5,
        topn_levels: &[],
    },
    RouteSpec {
        id: ROUTE_SPOT_TOPN,
        kind: RouteKind::DepthTopN,
        name: "spot-topn",
        race: false,
        max_streams: 1024,
        max_in_msgs_per_s: 5,
        topn_levels: TOPN,
    },
    RouteSpec {
        id: ROUTE_SPOT_MD,
        kind: RouteKind::Md,
        name: "spot-md",
        race: false,
        max_streams: 1024,
        max_in_msgs_per_s: 5,
        topn_levels: &[],
    },
];

pub(crate) fn routes(line: Line) -> &'static [RouteSpec] {
    match line {
        Line::Usdm => &USDM_ROUTES,
        Line::Spot => &SPOT_ROUTES,
    }
}

/// `X-MBX-APIKEY: <key>\r\n`.
fn api_key_header<const N: usize>(
    cred: &Credential,
    out: &mut hft_venue_proto::ArrayBuf<N>,
) -> Result<(), ProtoErr> {
    out.push(b"X-MBX-APIKEY: ")?;
    out.push(cred.api_key.as_slice())?;
    out.push(b"\r\n")
}

pub(crate) fn bootstrap(
    line: Line,
    role: Role,
    _cfg: &VenueCfg,
    cred: Option<&Credential>,
) -> Option<BootCall> {
    if line != Line::Usdm || role != Role::UserStream {
        return None;
    }
    let cred = cred?;
    let mut b = BootCall {
        endpoint: 0,
        method: Method::Post,
        ..BootCall::default()
    };
    b.path_query.push(b"/fapi/v1/listenKey").ok()?;
    api_key_header(cred, &mut b.headers).ok()?;
    Some(b)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn endpoint(
    line: Line,
    role: Role,
    route: RouteId,
    _cfg: &VenueCfg,
    _cred: Option<&Credential>,
    boot: Option<&[u8]>,
    _now_ms: i64,
    out: &mut EndpointSpec,
) -> Result<(), ProtoErr> {
    out.path_query.clear();
    out.headers.clear();
    let path: &[u8] = match (line, role) {
        (Line::Usdm, Role::Md) => match route {
            ROUTE_PUBLIC_BBO | ROUTE_PUBLIC_TOPN | ROUTE_PUBLIC_DEPTH => b"/public/ws",
            ROUTE_MARKET => b"/market/ws",
            _ => return Err(ProtoErr::Unsupported),
        },
        (Line::Usdm, Role::Order) => b"/ws-fapi/v1",
        (Line::Usdm, Role::UserStream) => {
            #[derive(Deserialize)]
            struct Lk<'a> {
                #[serde(rename = "listenKey", borrow)]
                key: &'a str,
            }
            let body = boot.ok_or(ProtoErr::Cfg)?;
            let lk: Lk = from(body).map_err(|_| ProtoErr::Cfg)?;
            if lk.key.is_empty() || !lk.key.bytes().all(|c| c.is_ascii_alphanumeric()) {
                return Err(ProtoErr::Cfg);
            }
            out.path_query.push(b"/private/ws/")?;
            return out.path_query.push(lk.key.as_bytes());
        }
        (Line::Spot, Role::Md) => match route {
            ROUTE_SPOT_BBO | ROUTE_SPOT_MD => b"/ws?timeUnit=MICROSECOND",
            ROUTE_SPOT_TOPN => b"/stream?timeUnit=MICROSECOND",
            _ => return Err(ProtoErr::Unsupported),
        },
        (Line::Spot, Role::Order) => b"/ws-api/v3?timeUnit=MICROSECOND",
        _ => return Err(ProtoErr::Unsupported),
    };
    out.path_query.push(path)
}

/// Logon frame (Order role with an Ed25519 credential); every other role needs none.
pub(crate) fn login(
    _line: Line,
    role: Role,
    _cfg: &VenueCfg,
    cred: &Credential,
    step: u8,
    now_ms: i64,
    out: &mut Vec<u8>,
) -> Result<LoginStep, ProtoErr> {
    if role != Role::Order {
        return Ok(LoginStep::NotNeeded);
    }
    if step != 0 || cred.alg != SignAlg::Ed25519 || cred.api_key.is_empty() {
        return Err(ProtoErr::Cfg);
    }
    let key = cred.api_key.as_slice();
    let mut msg = [0u8; 192];
    let n = logon_payload(key, now_ms, &mut msg).ok_or(ProtoErr::BufFull)?;
    let sig = ed25519_b64(cred.secret.0.as_slice(), msg.get(..n).unwrap_or(&[])).ok_or(ProtoErr::Cfg)?;
    out.clear();
    out.extend_from_slice(
        b"{\"id\":\"l0000000000000000\",\"method\":\"session.logon\",\"params\":{\"apiKey\":\"",
    );
    out.extend_from_slice(key);
    out.extend_from_slice(b"\",\"signature\":\"");
    out.extend_from_slice(&sig);
    out.extend_from_slice(b"\",\"timestamp\":");
    let mut t = [0u8; 24];
    let mut w = Wr::new(&mut t);
    w.i64(now_ms);
    let tn = w.finish().ok_or(ProtoErr::BufFull)?;
    out.extend_from_slice(t.get(..tn).unwrap_or(&[]));
    out.extend_from_slice(b"}}");
    Ok(LoginStep::Frame)
}

pub(crate) fn login_result(payload: &[u8]) -> LoginResult {
    #[derive(Deserialize)]
    struct Ans<'a> {
        #[serde(borrow)]
        id: &'a str,
        status: u16,
        #[serde(default)]
        error: Option<ErrBody>,
    }
    if !payload.starts_with(b"{\"id\":\"l") {
        return LoginResult::NotMine;
    }
    let Ok(a) = from::<Ans>(payload) else {
        return LoginResult::NotMine;
    };
    if id_kind(a.id.as_bytes()) != IdKind::Control || !a.id.starts_with('l') {
        return LoginResult::NotMine;
    }
    if a.status == 200 {
        LoginResult::Ok
    } else {
        LoginResult::Rejected(raw_code(a.error.map(|e| e.code), a.status))
    }
}

/// Streams per subscription frame (README "7. 订阅节奏").
const STREAMS_PER_FRAME: usize = 200;

/// Writes one stream name (with quotes) for `s`; `Ok(false)` = covered by an earlier entry (the
/// `markPrice` stream carries Mark, Index and Funding once per inst).
fn stream_name(line: Line, rd_sym: &[u8], s: &SubSpec, w: &mut Wr<'_>) -> Result<(), ProtoErr> {
    w.byte(b'"').lower(rd_sym);
    let ok = match (line, s.stream) {
        (_, StreamKind::Bbo) => {
            w.bytes(b"@bookTicker");
            true
        }
        (_, StreamKind::DepthTopN) if TOPN.contains(&s.levels) => {
            w.bytes(b"@depth").u64(u64::from(s.levels)).bytes(b"@100ms");
            true
        }
        (_, StreamKind::DepthIncr) => {
            w.bytes(b"@depth@100ms");
            true
        }
        (Line::Usdm, StreamKind::Trade) => {
            w.bytes(b"@aggTrade");
            true
        }
        (Line::Spot, StreamKind::Trade) => {
            w.bytes(b"@trade");
            true
        }
        (Line::Usdm, StreamKind::Mark | StreamKind::Index | StreamKind::Funding) => {
            w.bytes(b"@markPrice@1s");
            true
        }
        (Line::Usdm, StreamKind::Liquidation) => {
            w.bytes(b"@forceOrder");
            true
        }
        _ => false,
    };
    if !ok {
        return Err(ProtoErr::Unsupported);
    }
    w.byte(b'"');
    Ok(())
}

fn is_mark(k: StreamKind) -> bool {
    matches!(k, StreamKind::Mark | StreamKind::Index | StreamKind::Funding)
}

/// Symbol bytes of an inst of this line (empty when unknown).
fn symbol_of(line: Line, rd: &RefData, inst: InstId) -> &[u8] {
    if inst.0 == 0 || rd.inst(inst).venue != line.venue() {
        return &[];
    }
    rd.inst_meta(inst).symbol.as_bytes()
}

/// Subscription frames (symbols from `rd`).
pub(crate) fn subscribe(
    line: Line,
    rd: &RefData,
    subs: &[SubSpec],
    unsub: bool,
    _cfg: &VenueCfg,
    out: &mut SubFrames,
) -> Result<(), ProtoErr> {
    // Validate everything first: a refused spec writes no frame at all.
    let mut probe = [0u8; 96];
    for s in subs {
        let sym = symbol_of(line, rd, s.inst);
        if sym.is_empty() {
            return Err(ProtoErr::Cfg);
        }
        let mut w = Wr::new(&mut probe);
        stream_name(line, sym, s, &mut w)?;
    }
    let mut frame = [0u8; 8192];
    let mut id: u64 = 0;
    let mut start = 0usize;
    while start < subs.len() {
        id = id.wrapping_add(1);
        let mut w = Wr::new(&mut frame);
        w.bytes(if unsub {
            b"{\"method\":\"UNSUBSCRIBE\",\"params\":["
        } else {
            b"{\"method\":\"SUBSCRIBE\",\"params\":["
        });
        let mut names = 0usize;
        let mut end = start;
        while end < subs.len() && names < STREAMS_PER_FRAME {
            let s = subs.get(end).ok_or(ProtoErr::BufFull)?;
            let dup = is_mark(s.stream)
                && subs
                    .get(start..end)
                    .unwrap_or(&[])
                    .iter()
                    .any(|p| p.inst == s.inst && is_mark(p.stream));
            if !dup {
                if names > 0 {
                    w.byte(b',');
                }
                stream_name(line, symbol_of(line, rd, s.inst), s, &mut w)?;
                names += 1;
            }
            end += 1;
        }
        w.bytes(b"],\"id\":").u64(id).byte(b'}');
        let n = w.finish().ok_or(ProtoErr::BufFull)?;
        let covered = u16::try_from(end - start).map_err(|_| ProtoErr::BufFull)?;
        out.push(frame.get(..n).unwrap_or(&[]), covered)?;
        start = end;
    }
    Ok(())
}

pub(crate) fn private_subscribe(
    line: Line,
    _cfg: &VenueCfg,
    _acct: AccountId,
    unsub: bool,
    out: &mut SubFrames,
) -> Result<(), ProtoErr> {
    match line {
        // The listenKey path selects the USDT-M user stream; no frame.
        Line::Usdm => Ok(()),
        Line::Spot => out.push(
            if unsub {
                b"{\"id\":\"u0000000000000000\",\"method\":\"userDataStream.unsubscribe\"}"
            } else {
                b"{\"id\":\"u0000000000000000\",\"method\":\"userDataStream.subscribe\"}"
            },
            0,
        ),
    }
}
