//! Routes, endpoints, `login`, subscriptions (README "端点", "实现说明" 4).

use hft_refdata::RefData;
use hft_types::{AccountId, InstId};
use hft_venue_proto::{
    Credential, EndpointSpec, LoginResult, LoginStep, ProtoErr, Role, RouteId, RouteKind, RouteSpec, SignAlg,
    StreamKind, SubFrames, SubSpec,
};
use serde::Deserialize;

use crate::Line;
use crate::json::{Code, from};
use crate::sign::hmac_b64;
use crate::wr::Wr;

/// Routes (same ids on both lines; spot has no `public-ticker`).
pub(crate) const ROUTE_BOOKS1: RouteId = RouteId(0);
pub(crate) const ROUTE_TOPN: RouteId = RouteId(1);
pub(crate) const ROUTE_TRADE: RouteId = RouteId(2);
pub(crate) const ROUTE_TICKER: RouteId = RouteId(3);
/// Incremental `books` (optional extension, `feed.depth = incremental`).
pub(crate) const ROUTE_BOOKS: RouteId = RouteId(4);

/// `books5` / `books50` (UTA; Classic V2 had `books15`).
pub(crate) const TOPN: &[u8] = &[5, 50];

const fn route(id: RouteId, kind: RouteKind, name: &'static str, topn: &'static [u8]) -> RouteSpec {
    RouteSpec {
        id,
        kind,
        name,
        race: matches!(kind, RouteKind::Bbo),
        max_streams: 1000,
        max_in_msgs_per_s: 10,
        topn_levels: topn,
    }
}

static FUT_ROUTES: [RouteSpec; 5] = [
    route(ROUTE_BOOKS1, RouteKind::Bbo, "books1", &[]),
    route(ROUTE_TOPN, RouteKind::DepthTopN, "public-topn", TOPN),
    route(ROUTE_TRADE, RouteKind::Md, "public-trade", &[]),
    route(ROUTE_TICKER, RouteKind::Md, "public-ticker", &[]),
    route(ROUTE_BOOKS, RouteKind::Md, "public-books", &[]),
];

static SPOT_ROUTES: [RouteSpec; 4] = [
    route(ROUTE_BOOKS1, RouteKind::Bbo, "books1", &[]),
    route(ROUTE_TOPN, RouteKind::DepthTopN, "public-topn", TOPN),
    route(ROUTE_TRADE, RouteKind::Md, "public-trade", &[]),
    route(ROUTE_BOOKS, RouteKind::Md, "public-books", &[]),
];

pub(crate) fn routes(line: Line) -> &'static [RouteSpec] {
    match line {
        Line::Fut => &FUT_ROUTES,
        Line::Spot => &SPOT_ROUTES,
    }
}

pub(crate) fn has_route(line: Line, r: RouteId) -> bool {
    routes(line).iter().any(|s| s.id == r)
}

pub(crate) fn endpoint(
    line: Line,
    role: Role,
    route: RouteId,
    out: &mut EndpointSpec,
) -> Result<(), ProtoErr> {
    out.path_query.clear();
    out.headers.clear();
    let path: &[u8] = match role {
        Role::Md if has_route(line, route) => b"/v3/ws/public",
        // Private channels and trade frames share the logged-in connection.
        Role::Order => b"/v3/ws/private",
        _ => return Err(ProtoErr::Unsupported),
    };
    out.path_query.push(path)
}

/// `{"op":"login","args":[{"apiKey":…,"passphrase":…,"timestamp":"<s>","sign":…}]}` on the Order
/// role; `sign = Base64(HMAC-SHA256(secret, ts_s + "GET" + "/user/verify"))`.
pub(crate) fn login(
    role: Role,
    cred: &Credential,
    step: u8,
    now_ms: i64,
    out: &mut Vec<u8>,
) -> Result<LoginStep, ProtoErr> {
    if role != Role::Order {
        out.clear();
        return Ok(LoginStep::NotNeeded);
    }
    if step != 0 || cred.alg != SignAlg::HmacSha256 || cred.api_key.is_empty() || cred.secret.0.is_empty() {
        return Err(ProtoErr::Cfg);
    }
    let mut t = [0u8; 24];
    let mut w = Wr::new(&mut t);
    w.i64(now_ms.div_euclid(1000));
    let tn = w.finish().ok_or(ProtoErr::BufFull)?;
    let ts = t.get(..tn).unwrap_or(&[]);
    let sig = hmac_b64(cred.secret.0.as_slice(), &[ts, b"GET", b"/user/verify"]).ok_or(ProtoErr::Cfg)?;
    out.clear();
    out.extend_from_slice(b"{\"op\":\"login\",\"args\":[{\"apiKey\":\"");
    out.extend_from_slice(cred.api_key.as_slice());
    out.extend_from_slice(b"\",\"passphrase\":\"");
    out.extend_from_slice(cred.passphrase.as_slice());
    out.extend_from_slice(b"\",\"timestamp\":\"");
    out.extend_from_slice(ts);
    out.extend_from_slice(b"\",\"sign\":\"");
    out.extend_from_slice(&sig);
    out.extend_from_slice(b"\"}]}");
    Ok(LoginStep::Frame)
}

/// `{"event":…,"code":…}` of control frames.
#[derive(Deserialize)]
pub(crate) struct Event<'a> {
    #[serde(borrow)]
    pub event: &'a str,
    #[serde(default)]
    pub code: Code,
}

pub(crate) fn login_result(payload: &[u8]) -> LoginResult {
    if !payload.starts_with(b"{") {
        return LoginResult::NotMine;
    }
    let Ok(e) = from::<Event>(payload) else {
        return LoginResult::NotMine;
    };
    match e.event {
        "login" if e.code == Code::Absent || e.code.is_zero() => LoginResult::Ok,
        "login" | "error" => LoginResult::Rejected(e.code.raw(0)),
        _ => LoginResult::NotMine,
    }
}

/// Args per subscription frame (README "实现说明" 4).
const ARGS_PER_FRAME: usize = 50;

/// Topic of a stream on this line; `None` = not supported.
fn topic(line: Line, s: &SubSpec) -> Option<&'static [u8]> {
    match (line, s.stream) {
        (_, StreamKind::Bbo) => Some(b"books1"),
        (_, StreamKind::DepthTopN) => match s.levels {
            5 => Some(b"books5"),
            50 => Some(b"books50"),
            _ => None,
        },
        (_, StreamKind::DepthIncr) => Some(b"books"),
        (_, StreamKind::Trade) => Some(b"publicTrade"),
        (Line::Fut, StreamKind::Mark | StreamKind::Index | StreamKind::Funding) => Some(b"ticker"),
        _ => None,
    }
}

fn is_ticker(k: StreamKind) -> bool {
    matches!(k, StreamKind::Mark | StreamKind::Index | StreamKind::Funding)
}

/// Symbol bytes of an inst of this line (empty when unknown).
fn symbol_of(line: Line, rd: &RefData, inst: InstId) -> &[u8] {
    if inst.0 == 0 || rd.inst(inst).venue != line.venue() {
        return &[];
    }
    rd.inst_meta(inst).symbol.as_bytes()
}

fn arg(w: &mut Wr<'_>, inst_type: &[u8], topic: &[u8], symbol: &[u8]) {
    w.bytes(b"{\"instType\":\"")
        .bytes(inst_type)
        .bytes(b"\",\"topic\":\"")
        .bytes(topic);
    if !symbol.is_empty() {
        w.bytes(b"\",\"symbol\":\"").bytes(symbol);
    }
    w.bytes(b"\"}");
}

/// Subscription frames (symbols from `rd`); ≤ 50 args per frame, no `id`.
pub(crate) fn subscribe(
    line: Line,
    rd: &RefData,
    subs: &[SubSpec],
    unsub: bool,
    out: &mut SubFrames,
) -> Result<(), ProtoErr> {
    // Validate everything first: a refused spec writes no frame at all.
    for s in subs {
        if symbol_of(line, rd, s.inst).is_empty() {
            return Err(ProtoErr::Cfg);
        }
        if topic(line, s).is_none() {
            return Err(ProtoErr::Unsupported);
        }
    }
    let mut frame = [0u8; 8192];
    let mut start = 0usize;
    while start < subs.len() {
        let mut w = Wr::new(&mut frame);
        w.bytes(if unsub {
            b"{\"op\":\"unsubscribe\",\"args\":["
        } else {
            b"{\"op\":\"subscribe\",\"args\":["
        });
        let mut names = 0usize;
        let mut end = start;
        while end < subs.len() && names < ARGS_PER_FRAME {
            let s = subs.get(end).ok_or(ProtoErr::BufFull)?;
            let dup = is_ticker(s.stream)
                && subs
                    .get(start..end)
                    .unwrap_or(&[])
                    .iter()
                    .any(|p| p.inst == s.inst && is_ticker(p.stream));
            if !dup {
                if names > 0 {
                    w.byte(b',');
                }
                let t = topic(line, s).ok_or(ProtoErr::Unsupported)?;
                arg(&mut w, line.ws_type(), t, symbol_of(line, rd, s.inst));
                names += 1;
            }
            end += 1;
        }
        w.bytes(b"]}");
        let n = w.finish().ok_or(ProtoErr::BufFull)?;
        let covered = u16::try_from(end - start).map_err(|_| ProtoErr::BufFull)?;
        out.push(frame.get(..n).unwrap_or(&[]), covered)?;
        start = end;
    }
    Ok(())
}

/// The private channels of this line in one frame: `order` and `account` (both), `position`
/// (perpetual only). UTA pushes cover the whole unified account; `decode_report` keeps this
/// line's category.
pub(crate) fn private_subscribe(
    line: Line,
    _acct: AccountId,
    unsub: bool,
    out: &mut SubFrames,
) -> Result<(), ProtoErr> {
    let mut frame = [0u8; 512];
    let mut w = Wr::new(&mut frame);
    w.bytes(if unsub {
        b"{\"op\":\"unsubscribe\",\"args\":["
    } else {
        b"{\"op\":\"subscribe\",\"args\":["
    });
    arg(&mut w, b"UTA", b"order", b"");
    if line == Line::Fut {
        w.byte(b',');
        arg(&mut w, b"UTA", b"position", b"");
    }
    w.byte(b',');
    arg(&mut w, b"UTA", b"account", b"");
    w.bytes(b"]}");
    let n = w.finish().ok_or(ProtoErr::BufFull)?;
    out.push(frame.get(..n).unwrap_or(&[]), 0)
}
