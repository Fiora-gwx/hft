//! Route, endpoint, subscription frames and heartbeat (README "端点", "订阅").

use hft_refdata::RefData;
use hft_venue_proto::{
    EndpointSpec, HeartbeatSpec, PingKind, ProtoErr, Role, RouteId, RouteKind, RouteSpec, StreamKind,
    SubFrames, SubSpec,
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
        _ => Err(ProtoErr::Unsupported),
    }
}

/// Text `PING` every 10 s; the server answers `PONG` and drops silent connections.
pub(crate) fn heartbeat(_role: Role) -> HeartbeatSpec {
    HeartbeatSpec {
        ping: PingKind::AppText,
        interval_ms: 10_000,
        unconditional: true,
        pong_timeout_ms: 10_000,
        idle_timeout_ms: 30_000,
    }
}

pub(crate) const PING: &[u8] = b"PING";

/// Tokens per subscription frame.
const TOKENS_PER_FRAME: usize = 100;

/// `{"type":"market","operation":"subscribe","assets_ids":["<token>",…]}` (or `unsubscribe`).
/// Book and trade streams of one inst share the token: the inst is written once. The exchange
/// sends no answer to these frames. An unbound inst or another stream refuses the whole call.
pub(crate) fn subscribe(
    _rd: &RefData,
    ctx: &FeedCtx,
    subs: &[SubSpec],
    unsub: bool,
    out: &mut SubFrames,
) -> Result<(), ProtoErr> {
    for s in subs {
        if !matches!(
            s.stream,
            StreamKind::DepthTopN | StreamKind::DepthIncr | StreamKind::Trade
        ) {
            return Err(ProtoErr::Unsupported);
        }
        if ctx.token_of(s.inst).is_none() {
            return Err(ProtoErr::Cfg);
        }
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
            if !seen {
                if tokens == TOKENS_PER_FRAME {
                    break;
                }
                let t = ctx.token_of(s.inst).ok_or(ProtoErr::Cfg)?;
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
