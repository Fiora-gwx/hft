//! Route, endpoint path, market-data login and subscription frames (README "端点与路由",
//! "登录", "订阅").

use hft_refdata::RefData;
use hft_types::InstId;
use hft_venue_proto::{
    Credential, EndpointSpec, HeartbeatSpec, LoginResult, LoginStep, PingKind, ProtoErr, Role, RouteId,
    RouteKind, RouteSpec, StreamKind, SubFrames, SubSpec,
};

use crate::VENUE;

/// `price.crypto` + `price.crypto.twap`: one connection (hft-venue Md routes are not raced).
pub const ROUTE_PRICE: RouteId = RouteId(1);

static ROUTES: [RouteSpec; 1] = [RouteSpec {
    id: ROUTE_PRICE,
    kind: RouteKind::Md,
    name: "price",
    race: false,
    max_streams: 16,
    max_in_msgs_per_s: 10,
    topn_levels: &[],
}];

pub(crate) fn routes() -> &'static [RouteSpec] {
    &ROUTES
}

/// Chainlink price → `Index`, its 60 s TWAP → `Mark`; nothing else exists.
pub(crate) fn stream_route(stream: StreamKind) -> Option<RouteId> {
    match stream {
        StreamKind::Index | StreamKind::Mark => Some(ROUTE_PRICE),
        _ => None,
    }
}

/// The only path is `/ws`; no other role exists.
pub(crate) fn endpoint(role: Role, route: RouteId, out: &mut EndpointSpec) -> Result<(), ProtoErr> {
    out.path_query.clear();
    out.headers.clear();
    if role != Role::Md || route != ROUTE_PRICE {
        return Err(ProtoErr::Unsupported);
    }
    out.path_query.push(b"/ws")
}

/// A JSON string body written unescaped: printable ASCII without `"` and `\`.
fn plain(b: &[u8]) -> bool {
    !b.is_empty()
        && b.iter()
            .all(|c| c.is_ascii_graphic() && *c != b'"' && *c != b'\\')
}

/// The API secret: `Credential.secret` up to its first `|`. A Polymarket account secret file is
/// `<api secret>|<wallet key>|<deposit wallet>`; only the first part may leave the process.
pub fn api_secret(cred: &Credential) -> &[u8] {
    let s = cred.secret.0.as_slice();
    s.iter()
        .position(|&c| c == b'|')
        .and_then(|i| s.get(..i))
        .unwrap_or(s)
}

/// Market-data login: `{"op":"auth","rid":"a1","auth":{"apiKey":…,"secret":…,"passphrase":…}}`,
/// answered by `{"op":"authed"}` (README "登录"). Other roles have no session.
pub(crate) fn login(
    role: Role,
    cred: &Credential,
    step: u8,
    out: &mut Vec<u8>,
) -> Result<LoginStep, ProtoErr> {
    out.clear();
    if role != Role::Md {
        return Err(ProtoErr::Unsupported);
    }
    if step != 0 {
        return Err(ProtoErr::Cfg);
    }
    let (key, secret, pass) = (
        cred.api_key.as_slice(),
        api_secret(cred),
        cred.passphrase.as_slice(),
    );
    if !plain(key) || !plain(secret) || !plain(pass) {
        return Err(ProtoErr::Cfg);
    }
    out.extend_from_slice(b"{\"op\":\"auth\",\"rid\":\"a1\",\"auth\":{\"apiKey\":\"");
    out.extend_from_slice(key);
    out.extend_from_slice(b"\",\"secret\":\"");
    out.extend_from_slice(secret);
    out.extend_from_slice(b"\",\"passphrase\":\"");
    out.extend_from_slice(pass);
    out.extend_from_slice(b"\"}}");
    Ok(LoginStep::Frame)
}

/// `{"op":"authed",…}` → `Ok`; `{"op":"error",…}` → `Rejected` (code 1: the answer names a string
/// code, not a number); anything else is not the login answer.
pub(crate) fn login_result(payload: &[u8]) -> LoginResult {
    match crate::md::op_name(payload) {
        Some(b"authed") => LoginResult::Ok,
        Some(b"error") => LoginResult::Rejected(1),
        _ => LoginResult::NotMine,
    }
}

/// Symbol of an inst of this venue: `[a-z0-9]+` only (written into JSON unescaped).
fn symbol(rd: &RefData, inst: InstId) -> Result<&[u8], ProtoErr> {
    if inst.0 == 0 || rd.inst(inst).venue != VENUE {
        return Err(ProtoErr::Cfg);
    }
    let s = rd.inst_meta(inst).symbol.as_bytes();
    if s.is_empty() || !s.iter().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()) {
        return Err(ProtoErr::Cfg);
    }
    Ok(s)
}

/// Fixed-capacity frame writer (no allocation).
struct Frame {
    buf: [u8; 512],
    n: usize,
}

impl Frame {
    fn push(&mut self, b: &[u8]) -> Result<(), ProtoErr> {
        let end = self.n.checked_add(b.len()).ok_or(ProtoErr::BufFull)?;
        self.buf
            .get_mut(self.n..end)
            .ok_or(ProtoErr::BufFull)?
            .copy_from_slice(b);
        self.n = end;
        Ok(())
    }
}

/// One frame per spec (the server answers each subscription of a frame separately; one per frame
/// keeps answers and frames one to one): `{"op":"subscribe","rid":"s","subscriptions":[{"channel":
/// "price.crypto","filter":{"symbol":"btcusd"}}]}`; the TWAP filter adds `"window_seconds":60`.
pub(crate) fn subscribe(
    rd: &RefData,
    subs: &[SubSpec],
    unsub: bool,
    out: &mut SubFrames,
) -> Result<(), ProtoErr> {
    let s = subs.first().ok_or(ProtoErr::Cfg)?;
    let channel: &[u8] = match s.stream {
        StreamKind::Index => b"price.crypto",
        StreamKind::Mark => b"price.crypto.twap",
        _ => return Err(ProtoErr::Unsupported),
    };
    let mut f = Frame { buf: [0; 512], n: 0 };
    f.push(if unsub {
        b"{\"op\":\"unsubscribe\",\"rid\":\"u\",\"subscriptions\":[{\"channel\":\""
    } else {
        b"{\"op\":\"subscribe\",\"rid\":\"s\",\"subscriptions\":[{\"channel\":\""
    })?;
    f.push(channel)?;
    f.push(b"\",\"filter\":{\"symbol\":\"")?;
    f.push(symbol(rd, s.inst)?)?;
    f.push(if s.stream == StreamKind::Mark {
        b"\",\"window_seconds\":60}}]}"
    } else {
        b"\"}}]}"
    })?;
    out.push(f.buf.get(..f.n).unwrap_or(&[]), 1)
}

/// `{"op":"ping"}` every 5 s whether or not data flows, answered by `{"op":"pong"}`
/// (`ControlHint::Pong`) within 5 s; samples arrive once a second, so 10 s without any frame is
/// a dead connection (README "心跳").
pub(crate) fn heartbeat(_role: Role) -> HeartbeatSpec {
    HeartbeatSpec {
        ping: PingKind::AppJson,
        interval_ms: 5_000,
        unconditional: true,
        pong_timeout_ms: 5_000,
        idle_timeout_ms: 10_000,
    }
}

pub(crate) const PING: &[u8] = b"{\"op\":\"ping\",\"rid\":\"p\"}";
