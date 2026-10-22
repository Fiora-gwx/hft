//! Route, endpoint, subscription frames and heartbeat (README "端点", "订阅").

use hft_refdata::RefData;
use hft_venue_proto::{
    Credential, EndpointSpec, HeartbeatSpec, LoginStep, PingKind, ProtoErr, Role, RouteId, RouteKind,
    RouteSpec, StreamKind, SubFrames, SubSpec,
};

use crate::FeedCtx;
use crate::bind::TOKEN_MAX;

/// The market channel: books, level changes and trades of every subscribed token. Declared as the
/// depth route (whole book, at most 99 price levels per side, so "top 100" is the full book);
/// trades arrive on the same connection, so `Trade` has no route of its own.
pub(crate) const ROUTE_MARKET: RouteId = RouteId(0);

static ROUTES: [RouteSpec; 1] = [RouteSpec {
    id: ROUTE_MARKET,
    kind: RouteKind::DepthTopN,
    name: "market",
    race: false,
    // Polymarket closes a connection whose send buffer fills (WS 1013); one market (two tokens)
    // per connection keeps each connection's burst small. Effective value: refdata.
    max_streams: 2,
    max_in_msgs_per_s: 10,
    topn_levels: &[100],
}];

pub(crate) fn routes() -> &'static [RouteSpec] {
    &ROUTES
}

pub(crate) fn stream_route(stream: StreamKind) -> Option<RouteId> {
    match stream {
        StreamKind::DepthTopN => Some(ROUTE_MARKET),
        _ => None,
    }
}

pub(crate) fn endpoint(role: Role, route: RouteId, out: &mut EndpointSpec) -> Result<(), ProtoErr> {
    out.path_query.clear();
    out.headers.clear();
    match role {
        Role::Md if route == ROUTE_MARKET => out.path_query.push(b"/ws/market"),
        // The user channel is the order connection: orders themselves go over REST.
        Role::Order => out.path_query.push(b"/ws/user"),
        _ => Err(ProtoErr::Unsupported),
    }
}

/// Order connection: `{"auth":{"apiKey","secret","passphrase"},"type":"user"}` subscribes every
/// market of the account and is never answered (`LoginStep::Sent`). The runtime keeps login
/// frames out of the raw journal (contract 7).
pub(crate) fn login(role: Role, cred: &Credential, out: &mut Vec<u8>) -> Result<LoginStep, ProtoErr> {
    out.clear();
    if role != Role::Order {
        return Ok(LoginStep::NotNeeded);
    }
    let k = crate::orders::keys(cred)?;
    let ok = |b: &[u8]| {
        b.iter()
            .all(|c| c.is_ascii_graphic() && *c != b'"' && *c != b'\\')
    };
    if !ok(cred.api_key.as_slice()) || !ok(k.api_secret.as_bytes()) || !ok(cred.passphrase.as_slice()) {
        return Err(ProtoErr::Cfg);
    }
    out.extend_from_slice(b"{\"auth\":{\"apiKey\":\"");
    out.extend_from_slice(cred.api_key.as_slice());
    out.extend_from_slice(b"\",\"secret\":\"");
    out.extend_from_slice(k.api_secret.as_bytes());
    out.extend_from_slice(b"\",\"passphrase\":\"");
    out.extend_from_slice(cred.passphrase.as_slice());
    out.extend_from_slice(b"\"},\"type\":\"user\"}");
    Ok(LoginStep::Sent)
}

/// Text `PING` every 10 s; the server answers `PONG` and drops silent connections. Market
/// channel: no idle timeout (a market that has not opened yet, or has closed, is legitimately
/// silent for minutes; liveness is the `PONG`). User channel (order connection): the gw counts any
/// complete frame, `PONG` included, as activity but matches pongs only at the WS level, so the
/// liveness test there is 30 s without any frame (three missed `PONG`s) instead of a pong timeout.
pub(crate) fn heartbeat(role: Role) -> HeartbeatSpec {
    let order = role == Role::Order;
    HeartbeatSpec {
        ping: PingKind::AppText,
        interval_ms: 10_000,
        unconditional: true,
        pong_timeout_ms: if order { 0 } else { 10_000 },
        idle_timeout_ms: if order { 30_000 } else { 0 },
    }
}

pub(crate) const PING: &[u8] = b"PING";

/// Tokens per subscription frame.
const TOKENS_PER_FRAME: usize = 100;

/// `{"type":"market","operation":"subscribe","assets_ids":["<token>",…]}` (or `unsubscribe`).
/// Book and trade streams of one inst share the token: the inst is written once. The exchange
/// sends no answer to these frames. An unsubscription names the inst's previous token after a
/// rebinding. Insts without a token are skipped (counted as covered); none at all → `Cfg`;
/// another stream refuses the whole call.
pub(crate) fn subscribe(
    _rd: &RefData,
    ctx: &FeedCtx,
    subs: &[SubSpec],
    unsub: bool,
    out: &mut SubFrames,
) -> Result<(), ProtoErr> {
    let token = |inst| {
        if unsub {
            ctx.unsub_token_of(inst)
        } else {
            ctx.token_of(inst)
        }
    };
    for s in subs {
        if !matches!(
            s.stream,
            StreamKind::DepthTopN | StreamKind::DepthIncr | StreamKind::Trade
        ) {
            return Err(ProtoErr::Unsupported);
        }
    }
    if subs.iter().all(|s| token(s.inst).is_none()) {
        return Err(ProtoErr::Cfg);
    }
    let op: &[u8] = if unsub { b"unsubscribe" } else { b"subscribe" };
    let mut frame = [0u8; 64 + TOKENS_PER_FRAME * (TOKEN_MAX + 3)];
    let mut start = 0usize;
    while start < subs.len() {
        let mut n = 0usize;
        let mut len = 0usize;
        let mut put = |b: &[u8], len: &mut usize| -> Result<(), ProtoErr> {
            let end = *len + b.len();
            frame
                .get_mut(*len..end)
                .ok_or(ProtoErr::BufFull)?
                .copy_from_slice(b);
            *len = end;
            Ok(())
        };
        put(b"{\"type\":\"market\",\"operation\":\"", &mut len)?;
        put(op, &mut len)?;
        put(b"\",\"assets_ids\":[", &mut len)?;
        let mut tokens = 0usize;
        let mut k = start;
        while let Some(s) = subs.get(k) {
            let seen = subs
                .get(start..k)
                .is_some_and(|prev| prev.iter().any(|p| p.inst == s.inst));
            if let (false, Some(t)) = (seen, token(s.inst)) {
                if tokens == TOKENS_PER_FRAME {
                    break;
                }
                if tokens > 0 {
                    put(b",", &mut len)?;
                }
                put(b"\"", &mut len)?;
                put(t, &mut len)?;
                put(b"\"", &mut len)?;
                tokens += 1;
            }
            n += 1;
            k += 1;
        }
        put(b"]}", &mut len)?;
        out.push(frame.get(..len).ok_or(ProtoErr::BufFull)?, n as u16)?;
        start += n;
    }
    Ok(())
}
