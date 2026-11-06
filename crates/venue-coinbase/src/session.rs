//! Routes, endpoint path and subscription frames (README "端点与路由", "订阅").

use hft_refdata::RefData;
use hft_types::InstId;
use hft_venue_proto::{
    EndpointSpec, ProtoErr, Role, RouteId, RouteKind, RouteSpec, StreamKind, SubFrames, SubSpec,
};

use crate::VENUE;

/// `ticker` + `heartbeat`: BBO, raced (`feed.bbo_conns`, default 2).
pub const ROUTE_BBO: RouteId = RouteId(1);
/// `matches` + `heartbeat`: trades, one connection (hft-venue Md routes).
pub const ROUTE_MD: RouteId = RouteId(2);

/// Products per frame; one frame covers at most this many leading specs.
const PRODUCTS_PER_FRAME: usize = 64;

static ROUTES: [RouteSpec; 2] = [
    RouteSpec {
        id: ROUTE_BBO,
        kind: RouteKind::Bbo,
        name: "ticker",
        race: true,
        max_streams: 64,
        max_in_msgs_per_s: 8,
        topn_levels: &[],
    },
    RouteSpec {
        id: ROUTE_MD,
        kind: RouteKind::Md,
        name: "matches",
        race: false,
        max_streams: 64,
        max_in_msgs_per_s: 8,
        topn_levels: &[],
    },
];

pub(crate) fn routes() -> &'static [RouteSpec] {
    &ROUTES
}

/// Only BBO and trades exist; every other stream has no route.
pub(crate) fn stream_route(stream: StreamKind) -> Option<RouteId> {
    match stream {
        StreamKind::Bbo => Some(ROUTE_BBO),
        StreamKind::Trade => Some(ROUTE_MD),
        _ => None,
    }
}

/// Both routes connect to the feed root `/`; no other role exists.
pub(crate) fn endpoint(role: Role, route: RouteId, out: &mut EndpointSpec) -> Result<(), ProtoErr> {
    out.path_query.clear();
    out.headers.clear();
    if role != Role::Md || (route != ROUTE_BBO && route != ROUTE_MD) {
        return Err(ProtoErr::Unsupported);
    }
    out.path_query.push(b"/")
}

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

/// Data channel of a stream.
fn channel(stream: StreamKind) -> Result<&'static [u8], ProtoErr> {
    match stream {
        StreamKind::Bbo => Ok(b"\"ticker\""),
        StreamKind::Trade => Ok(b"\"matches\""),
        _ => Err(ProtoErr::Unsupported),
    }
}

/// Fixed-capacity frame writer (no allocation).
struct Frame {
    buf: [u8; 2048],
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

/// One frame covering the leading specs of one stream kind:
/// `{"type":"subscribe","product_ids":[…],"channels":["ticker","heartbeat"]}`. Unsubscribing
/// names the data channel only, so the `heartbeat` subscription — and with it every answer's
/// non-empty `channels` list — stays (README "订阅").
pub(crate) fn subscribe(
    rd: &RefData,
    subs: &[SubSpec],
    unsub: bool,
    out: &mut SubFrames,
) -> Result<(), ProtoErr> {
    let first = subs.first().ok_or(ProtoErr::Cfg)?;
    let data = channel(first.stream)?;
    let mut f = Frame { buf: [0; 2048], n: 0 };
    f.push(if unsub {
        b"{\"type\":\"unsubscribe\",\"product_ids\":["
    } else {
        b"{\"type\":\"subscribe\",\"product_ids\":["
    })?;
    let mut covered = 0usize;
    for (i, s) in subs.iter().enumerate() {
        if i >= PRODUCTS_PER_FRAME || s.stream != first.stream {
            break;
        }
        let p = product(rd, s.inst)?;
        if i > 0 {
            f.push(b",")?;
        }
        f.push(b"\"")?;
        f.push(p)?;
        f.push(b"\"")?;
        covered = i + 1;
    }
    f.push(b"],\"channels\":[")?;
    f.push(data)?;
    if !unsub {
        f.push(b",\"heartbeat\"")?;
    }
    f.push(b"]}")?;
    let covered = u16::try_from(covered).map_err(|_| ProtoErr::BufFull)?;
    out.push(f.buf.get(..f.n).unwrap_or(&[]), covered)
}
