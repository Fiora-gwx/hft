//! Bounded HTTP/1.1 on handed-off TLS connections. No allocation or connection management on poll.
use hft_rt::TscClock;
use hft_types::Tsc;

use crate::{
    BuildWhy, Conn, ConnId, ConnPhase, ConnRole, ErrBits, Handoff, NetConfig, PoolErr, RetireMsg, SendErr,
    Timestamps,
};

pub type ReqTag = u64;
pub use httparse::Header;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Delete,
}
impl Method {
    fn bytes(self) -> &'static [u8] {
        match self {
            Self::Get => b"GET",
            Self::Post => b"POST",
            Self::Put => b"PUT",
            Self::Delete => b"DELETE",
        }
    }
}

/// Header values and the target must not contain CR/LF. Framing headers belong to the pool.
#[derive(Clone, Copy)]
pub struct RestReq<'a> {
    pub method: Method,
    pub path_query: &'a [u8],
    pub headers: &'a [(&'a [u8], &'a [u8])],
    pub body: &'a [u8],
    pub tag: ReqTag,
}

/// Header storage is inline; only `headers[..header_count]` contains response headers.
pub struct RestResp<'a> {
    pub tag: ReqTag,
    pub status: u16,
    pub headers: [Header<'a>; 64],
    pub header_count: usize,
    pub body: &'a [u8],
    pub ts: Timestamps,
}

impl core::fmt::Debug for RestReq<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RestReq")
            .field("method", &self.method)
            .field("tag", &self.tag)
            .field("body_len", &self.body.len())
            .finish_non_exhaustive()
    }
}
impl core::fmt::Debug for RestResp<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RestResp")
            .field("status", &self.status)
            .field("tag", &self.tag)
            .field("body_len", &self.body.len())
            .field("ts", &self.ts)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestErr {
    Busy,
    NoConn,
    ReqTooLarge,
    Send(SendErr),
    Timeout,
    TooLarge,
    Closed,
    Proto,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RestFail {
    pub tag: ReqTag,
    pub err: RestErr,
}

#[derive(Clone, Copy, Debug)]
struct Complete {
    head: usize,
    body: usize,
    close: bool,
}

#[derive(Debug)]
struct Lane {
    h: Option<Handoff>,
    retired: Option<RetireMsg>,
    pending: Option<(ReqTag, u64)>,
    result: Option<Result<Complete, RestErr>>,
    buf: Box<[u8]>,
    len: usize,
    ts: Option<Timestamps>,
}

pub struct RestPool<const N: usize = 2> {
    lanes: [Lane; N],
    host: Box<[u8]>,
    tx: Box<[u8]>,
    clock: TscClock,
    timeout_ns: u64,
    building: bool,
}

impl<const N: usize> core::fmt::Debug for RestPool<N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RestPool")
            .field("capacity", &N)
            .field(
                "pending",
                &self.lanes.iter().filter(|l| l.pending.is_some()).count(),
            )
            .field("building", &self.building)
            .finish()
    }
}

impl<const N: usize> RestPool<N> {
    /// Startup only. Capacity includes HTTP headers; reserve one TLS record's 22-byte overhead.
    pub fn new(host: &[u8], cfg: &NetConfig, clock: TscClock) -> Self {
        Self {
            lanes: std::array::from_fn(|_| Lane {
                h: None,
                retired: None,
                pending: None,
                result: None,
                buf: vec![0; cfg.rx_buf.rest.saturating_sub(22) as usize].into_boxed_slice(),
                len: 0,
                ts: None,
            }),
            host: host.into(),
            tx: vec![0; cfg.tx_buf.saturating_sub(22).min(16_384) as usize].into_boxed_slice(),
            clock,
            timeout_ns: u64::from(cfg.rest_timeout_ms) * 1_000_000,
            building: false,
        }
    }

    pub fn install(&mut self, mut h: Handoff) -> Result<ConnId, PoolErr> {
        let lane = self
            .lanes
            .iter_mut()
            .find(|l| l.h.is_none() && l.retired.is_none() && l.pending.is_none())
            .ok_or(PoolErr::Full)?;
        if h.role() != ConnRole::Rest {
            return Err(PoolErr::WrongMode);
        }
        let c = h.conn().ok_or(PoolErr::NotOpen)?;
        if c.phase() != ConnPhase::Open {
            return Err(PoolErr::NotOpen);
        }
        c.on_install();
        let id = h.id();
        lane.h = Some(h);
        self.building = false;
        Ok(id)
    }

    pub fn next_build(&mut self, _now_tsc: u64) -> Option<BuildWhy> {
        if !self.building
            && self
                .lanes
                .iter()
                .any(|l| l.h.is_none() && l.retired.is_none() && l.pending.is_none())
        {
            self.building = true;
            Some(BuildWhy::Fill)
        } else {
            None
        }
    }
    pub fn build_failed(&mut self) {
        self.building = false;
    }
    pub fn take_retired(&mut self) -> Option<RetireMsg> {
        self.lanes.iter_mut().find_map(|l| l.retired.take())
    }
    pub fn get(&mut self, id: ConnId) -> Option<&mut Conn> {
        self.lanes
            .iter_mut()
            .filter_map(|l| l.h.as_mut())
            .find(|h| h.id() == id)?
            .conn()
    }

    pub fn submit(&mut self, req: &RestReq<'_>, now_tsc: u64) -> Result<ReqTag, RestErr> {
        let mut open = false;
        let idx = self
            .lanes
            .iter_mut()
            .position(|l| {
                let ready =
                    l.h.as_mut()
                        .and_then(Handoff::conn)
                        .is_some_and(|c| c.phase() == ConnPhase::Open);
                open |= ready;
                ready && l.pending.is_none()
            })
            .ok_or(if open { RestErr::Busy } else { RestErr::NoConn })?;
        let n = match encode(req, &self.host, &mut self.tx) {
            Ok(n) => n,
            Err(e) => {
                self.tx.fill(0);
                return Err(e);
            }
        };
        let lane = &mut self.lanes[idx];
        let c = lane.h.as_mut().and_then(Handoff::conn).ok_or(RestErr::NoConn)?;
        // Validate the actual handed-off connection's capacity before touching its TLS sequence.
        if n.saturating_add(22) > c.buf_caps().1 as usize {
            self.tx[..n].fill(0);
            return Err(RestErr::ReqTooLarge);
        }
        let sent = c.send_plain(&self.tx[..n]);
        self.tx[..n].fill(0);
        if let Err(e) = sent {
            retire(lane, ErrBits::IO);
            return Err(RestErr::Send(e));
        }
        lane.pending = Some((req.tag, now_tsc));
        lane.len = 0;
        lane.ts = None;
        lane.result = None;
        Ok(req.tag)
    }

    /// At most one recvmsg per pending connection. Completed responses wait until delivered.
    pub fn poll(&mut self, now_tsc: u64) -> Option<Result<RestResp<'_>, RestFail>> {
        for lane in &mut self.lanes {
            let Some((_, start)) = lane.pending else {
                continue;
            };
            if lane.result.is_some() {
                continue;
            }
            if self.clock.delta_ns(Tsc(start), Tsc(now_tsc)) >= self.timeout_ns {
                lane.result = Some(Err(RestErr::Timeout));
            } else {
                lane.result = read(lane, now_tsc, self.clock);
            }
            let why = match lane.result {
                Some(Err(RestErr::TooLarge)) => Some(ErrBits::RX_FULL),
                Some(Err(RestErr::Proto)) => Some(ErrBits::HTTP_PROTO),
                Some(Err(RestErr::Timeout)) => Some(ErrBits::IDLE_TIMEOUT),
                Some(Err(_)) | Some(Ok(Complete { close: true, .. })) => Some(ErrBits::PEER_EOF),
                _ => None,
            };
            if let Some(why) = why {
                retire(lane, why);
            }
        }
        let lane = self.lanes.iter_mut().find(|l| l.result.is_some())?;
        let result = lane.result.take()?;
        let (tag, _) = lane.pending.take()?;
        Some(match result {
            Err(err) => Err(RestFail { tag, err }),
            Ok(done) => {
                let mut headers = [httparse::EMPTY_HEADER; 64];
                let mut response = httparse::Response::new(&mut headers);
                // Already checked by parse; reborrow the immutable header bytes for the caller.
                if response.parse(&lane.buf[..done.head]).is_err() {
                    return Some(Err(RestFail {
                        tag,
                        err: RestErr::Proto,
                    }));
                }
                let status = response.code.unwrap_or(0);
                let header_count = response.headers.len();
                Ok(RestResp {
                    tag,
                    status,
                    headers,
                    header_count,
                    body: &lane.buf[done.head..done.head + done.body],
                    ts: lane.ts.unwrap_or_default(),
                })
            }
        })
    }

    pub fn idle_check(&mut self, now_tsc: u64) -> u8 {
        let mut count = 0u8;
        for lane in &mut self.lanes {
            if lane.pending.is_some() {
                continue;
            }
            let Some(c) = lane.h.as_mut().and_then(Handoff::conn) else {
                continue;
            };
            count = count.saturating_add(1);
            c.poll_rx(now_tsc);
            for _ in 0..64 {
                if c.has_plain() || c.open_one() != Some(true) {
                    break;
                }
            }
            let why = if c.has_plain() {
                ErrBits::HTTP_PROTO
            } else {
                c.err() & ErrBits::FATAL
            };
            if !why.is_empty() {
                retire(lane, why);
            }
        }
        count
    }
}

fn retire(lane: &mut Lane, why: ErrBits) {
    if let Some(mut h) = lane.h.take() {
        let why = why | h.conn().map(|c| c.err()).unwrap_or_default();
        lane.retired = Some(RetireMsg { h, why });
    }
}

fn read(lane: &mut Lane, now: u64, clock: TscClock) -> Option<Result<Complete, RestErr>> {
    let c = lane.h.as_mut().and_then(Handoff::conn)?;
    if let Err(e) = c.flush() {
        return Some(Err(RestErr::Send(e)));
    }
    c.poll_rx(now);
    for _ in 0..64 {
        let bytes = c.plaintext();
        if !bytes.is_empty() {
            if bytes.len() > lane.buf.len().saturating_sub(lane.len) {
                return Some(Err(RestErr::TooLarge));
            }
            let ts = c.plaintext_stamps();
            if let Some(first) = &mut lane.ts {
                if first.rx_empty_gap_ns != 0 {
                    first.rx_empty_gap_ns = first.rx_empty_gap_ns.saturating_add(
                        clock
                            .delta_ns(Tsc(first.rx_tsc), Tsc(ts.rx_tsc))
                            .min(u64::from(u32::MAX)) as u32,
                    );
                }
                first.rx_tsc = ts.rx_tsc;
            } else {
                lane.ts = Some(ts);
            }
            let n = bytes.len();
            lane.buf[lane.len..lane.len + n].copy_from_slice(bytes);
            lane.len += n;
            c.consume_plain(n);
        }
        if c.open_one() != Some(true) {
            break;
        }
    }
    match parse(&mut lane.buf, lane.len) {
        Ok(Some(mut done)) => {
            // The final body bytes may arrive with EOF. Deliver them once, then replace the lane.
            done.close |= c.phase() == ConnPhase::Dead || !(c.err() & ErrBits::FATAL).is_empty();
            Some(Ok(done))
        }
        Err(e) => Some(Err(e)),
        Ok(None) if c.err().contains(ErrBits::RX_FULL) || lane.len == lane.buf.len() => {
            Some(Err(RestErr::TooLarge))
        }
        Ok(None) if c.phase() == ConnPhase::Dead => Some(Err(RestErr::Closed)),
        Ok(None) => None,
    }
}

fn append(dst: &mut [u8], pos: &mut usize, src: &[u8]) -> Result<(), RestErr> {
    let end = pos.checked_add(src.len()).ok_or(RestErr::ReqTooLarge)?;
    dst.get_mut(*pos..end)
        .ok_or(RestErr::ReqTooLarge)?
        .copy_from_slice(src);
    *pos = end;
    Ok(())
}
fn encode(req: &RestReq<'_>, host: &[u8], dst: &mut [u8]) -> Result<usize, RestErr> {
    if host.is_empty()
        || host.iter().any(|b| *b <= 32 || *b >= 127)
        || !req.path_query.starts_with(b"/")
        || req.path_query.iter().any(|b| *b <= 32 || *b == 127)
    {
        return Err(RestErr::Proto);
    }
    let mut n = 0;
    for part in [
        req.method.bytes(),
        b" ",
        req.path_query,
        b" HTTP/1.1\r\nHost: ",
        host,
        b"\r\nAccept-Encoding: identity\r\nConnection: keep-alive\r\n",
    ] {
        append(dst, &mut n, part)?;
    }
    for &(name, value) in req.headers {
        if name.is_empty()
            || !name
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(b))
            || value.iter().any(|b| (*b < 32 && *b != b'\t') || *b == 127)
            || [
                b"host".as_slice(),
                b"content-length",
                b"transfer-encoding",
                b"connection",
                b"accept-encoding",
            ]
            .iter()
            .any(|s| name.eq_ignore_ascii_case(s))
        {
            return Err(RestErr::Proto);
        }
        for part in [name, b": ", value, b"\r\n"] {
            append(dst, &mut n, part)?;
        }
    }
    let mut digits = [0u8; 20];
    let mut value = req.body.len();
    let mut at = digits.len();
    loop {
        at -= 1;
        digits[at] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    for part in [
        b"Content-Length: ".as_slice(),
        &digits[at..],
        b"\r\n\r\n",
        req.body,
    ] {
        append(dst, &mut n, part)?;
    }
    Ok(n)
}

/// Strict bounded Content-Length framing. Unsupported framing retires the connection.
fn parse(buf: &mut [u8], len: usize) -> Result<Option<Complete>, RestErr> {
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut response = httparse::Response::new(&mut headers);
    let head = match response.parse(&buf[..len]).map_err(|_| RestErr::Proto)? {
        httparse::Status::Partial => return Ok(None),
        httparse::Status::Complete(n) => n,
    };
    response.code.ok_or(RestErr::Proto)?;
    let (mut length, mut chunked, mut close) = (None, false, response.version == Some(0));
    for h in response.headers.iter() {
        if h.name.eq_ignore_ascii_case("content-length") {
            if length.is_some() || h.value.is_empty() || !h.value.iter().all(u8::is_ascii_digit) {
                return Err(RestErr::Proto);
            }
            length = Some(
                core::str::from_utf8(h.value)
                    .ok()
                    .and_then(|s| s.parse::<usize>().ok())
                    .ok_or(RestErr::TooLarge)?,
            );
        } else if h.name.eq_ignore_ascii_case("transfer-encoding") {
            if chunked || !h.value.eq_ignore_ascii_case(b"chunked") {
                return Err(RestErr::Proto);
            }
            chunked = true;
        } else if h.name.eq_ignore_ascii_case("content-encoding")
            && !h.value.eq_ignore_ascii_case(b"identity")
        {
            return Err(RestErr::Proto);
        } else if h.name.eq_ignore_ascii_case("connection") {
            close |= h
                .value
                .split(|b| *b == b',')
                .any(|s| s.trim_ascii().eq_ignore_ascii_case(b"close"));
        }
    }
    if chunked {
        return Err(RestErr::Proto);
    }
    if let Some(body) = length {
        let end = head.checked_add(body).ok_or(RestErr::TooLarge)?;
        if end > buf.len() {
            return Err(RestErr::TooLarge);
        }
        return if len > end {
            Err(RestErr::Proto)
        } else {
            Ok((len == end).then_some(Complete { head, body, close }))
        };
    }
    Err(RestErr::Proto)
}
