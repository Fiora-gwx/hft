//! Warm public BBO REST worker. One bounded request per lane; the feed owns retries.
#![allow(clippy::disallowed_macros)] // allow-list: warm HTTP and record encoding.
use crate::account::Transport;
use crate::config::{InitErr, SnapCfg};
use crate::{LoopStat, SNAP_FIRST};
use hft_event::{Bbo, FeedMsg};
use hft_net::{BuildReq, ConnId, ConnRole, EndpointId};
use hft_refdata::RefData;
use hft_rt::{ByteProducer, Consumer, PollTask, Polled, UtcClock};
use hft_types::{AccountId, InstId, Tsc};
use hft_venue_proto::{RestReqBuf, RxMeta, VenueProtocol};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

#[repr(C)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct SnapReq {
    pub kind: u8,
    pub _pad: u8,
    pub attempt: u16,
    pub inst: InstId,
    pub req_id: u32,
    pub _pad2: u32,
}
const _: () = assert!(core::mem::size_of::<SnapReq>() == 16);
impl SnapReq {
    pub const L2: u8 = 1;
    pub const BBO: u8 = 2;
}

/// In-process BBO record, then raw response bytes and (only for OK) one Bbo body.
#[repr(C)]
#[derive(Copy, Clone, Debug, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct SnapRecHdr {
    pub rx_tsc: u64,
    pub sw_rx_ns: i64,
    pub req_id: u32,
    pub inst: InstId,
    pub body_len: u32,
    pub status: u8,
    pub kind: u8,
    pub conn: ConnId,
    pub _pad: u8,
    pub http_status: u16,
    pub _pad2: u16,
    pub ts_flags: u32,
}
impl SnapRecHdr {
    pub const OK: u8 = 1;
    pub const HTTP: u8 = 2;
    pub const TIMEOUT: u8 = 3;
    pub const DECODE: u8 = 4;
    pub const TOO_LARGE: u8 = 5;
    pub const RATE: u8 = 6;
    pub const REQUEST: u8 = 7;
}
#[derive(Debug)]
pub struct SnapDeps {
    pub rd: &'static RefData,
    pub reqs: [Option<Consumer<SnapReq>>; 2],
    pub out: [Option<ByteProducer>; 2],
    /// The published TSC → UTC line (hft-rt `UtcClock`): REST deadlines, record times.
    pub clock: UtcClock,
}
pub struct SnapFetchTask<P: VenueProtocol, T: Transport = hft_net::rest::Client> {
    cfg: SnapCfg,
    deps: SnapDeps,
    ctx: P::FeedCtx,
    transport: Option<T>,
    host: String,
    active: Option<(usize, SnapReq)>,
    pending: Option<(usize, Vec<u8>)>,
    next: i64,
    rr: usize,
    sink: crate::sink::FeedSink,
    waiting: Option<(usize, SnapReq)>,
    rate: crate::RateCounters,
}
impl<P: VenueProtocol, T: Transport> core::fmt::Debug for SnapFetchTask<P, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SnapFetchTask").field("cfg", &self.cfg).finish()
    }
}
impl<P: VenueProtocol, T: Transport> SnapFetchTask<P, T> {
    /// Startup conversion to bounded keep-alive pools; no request may have been accepted yet.
    pub fn into_pooled<const N: usize>(
        self,
        pool: hft_net::RestPool<N>,
    ) -> Result<crate::PooledSnapFetchTask<P, N>, InitErr> {
        if self.transport.is_some()
            || self.active.is_some()
            || self.pending.is_some()
            || self.waiting.is_some()
        {
            return Err(InitErr::DepsMismatch);
        }
        crate::PooledSnapFetchTask::new(self.cfg, self.deps, self.ctx, pool)
    }
    pub fn new(cfg: SnapCfg, deps: SnapDeps, ctx: P::FeedCtx) -> Result<Self, InitErr> {
        if cfg.parallel == 0 {
            return Err(InitErr::Cfg("snap.parallel"));
        }
        if cfg.timeout_ms == 0
            || cfg.request_gap_ms == 0
            || cfg.max_record_bytes < (size_of::<SnapRecHdr>() + size_of::<Bbo>()) as u32
        {
            return Err(InitErr::Cfg("snap.bounds"));
        }
        let vm = deps.rd.venue(P::VENUE);
        if vm.ws_max_conn_age_s != 0 && cfg.rotate_margin_s >= vm.ws_max_conn_age_s {
            return Err(InitErr::RotateMargin);
        }
        let ticks = crate::config::Ticks::of(&deps.clock);
        let rate = crate::RateCounters::rest(deps.rd.rate_limits(P::VENUE), |ms| ticks.ms(ms));
        Ok(Self {
            cfg,
            deps,
            ctx,
            transport: None,
            host: String::new(),
            active: None,
            pending: None,
            next: 0,
            rr: 0,
            sink: crate::sink::FeedSink::new(1),
            waiting: None,
            rate,
        })
    }
    /// Startup only. The transport uses the configured public REST endpoint and source IP.
    pub fn attach(&mut self, transport: T, host: String) -> Result<(), InitErr> {
        if host.is_empty()
            || host.bytes().any(|b| b.is_ascii_control() || b == b' ')
            || self.transport.is_some()
        {
            return Err(InitErr::Cfg("snap.host"));
        }
        self.transport = Some(transport);
        self.host = host;
        Ok(())
    }
    fn record(
        &mut self,
        lane: usize,
        req: SnapReq,
        result: Result<hft_net::rest::Response, hft_net::rest::Error>,
        now: u64,
    ) {
        let ns = self.deps.clock.utc_ns(Tsc(now));
        let mut hdr = SnapRecHdr {
            rx_tsc: now,
            sw_rx_ns: 0,
            req_id: req.req_id,
            inst: req.inst,
            body_len: 0,
            status: SnapRecHdr::HTTP,
            kind: req.kind,
            conn: ConnId(SNAP_FIRST),
            _pad: 0,
            http_status: 0,
            _pad2: 0,
            ts_flags: 0,
        };
        let mut body = Vec::new();
        let mut quote = None;
        match result {
            Err(hft_net::rest::Error::Timeout) => hdr.status = SnapRecHdr::TIMEOUT,
            Err(hft_net::rest::Error::TooLarge) => hdr.status = SnapRecHdr::TOO_LARGE,
            Err(hft_net::rest::Error::Request) => hdr.status = SnapRecHdr::REQUEST,
            Err(_) => {}
            Ok(r) => {
                hdr.http_status = r.status;
                body = r.body;
                if r.status == 429 || r.status == 418 {
                    hdr.status = SnapRecHdr::RATE;
                } else if (200..300).contains(&r.status) {
                    let rx = RxMeta {
                        rx_tsc: now,
                        hw_rx_ns: 0,
                        sw_rx_ns: 0,
                        conn: hdr.conn,
                        account: AccountId(0),
                        prev_override: 0,
                        _pad: [0; 2],
                        ts_flags: 0,
                        rx_empty_gap_ns: 0,
                        _pad2: 0,
                    };
                    self.sink.begin(1);
                    hdr.status = SnapRecHdr::DECODE;
                    if P::decode_bbo_snapshot(
                        self.deps.rd,
                        &mut self.ctx,
                        &[req.inst],
                        &body,
                        &rx,
                        &mut self.sink,
                    ) == Ok(1)
                        && !self.sink.over
                        && self.sink.n == 1
                        && let Some(FeedMsg::Bbo(e, _)) = self.sink.evs.first()
                        && e.h.inst == req.inst
                        && e.h.venue == P::VENUE
                    {
                        quote = Some(*e);
                        hdr.status = SnapRecHdr::OK;
                    }
                }
            }
        }
        let len = size_of::<SnapRecHdr>() + body.len() + quote.as_ref().map_or(0, |_| size_of::<Bbo>());
        if len > self.cfg.max_record_bytes as usize {
            body.clear();
            quote = None;
            hdr.status = SnapRecHdr::TOO_LARGE;
        }
        hdr.body_len = body.len() as u32;
        let mut rec = Vec::with_capacity(size_of::<SnapRecHdr>() + body.len() + size_of::<Bbo>());
        rec.extend_from_slice(hdr.as_bytes());
        rec.extend_from_slice(&body);
        if let Some(e) = quote {
            rec.extend_from_slice(e.as_bytes());
        }
        self.pending = Some((lane, rec));
        self.next = ns.saturating_add(i64::from(self.cfg.request_gap_ms) * 1_000_000);
    }
    pub fn poll_once(&mut self, now: u64) -> LoopStat {
        let mut stat = LoopStat::default();
        if let Some((lane, rec)) = &self.pending {
            if let Some(out) = self.deps.out[*lane].as_mut()
                && let Some(dst) = out.reserve(rec.len() as u32)
            {
                dst.copy_from_slice(rec);
                out.commit();
                self.pending = None;
                stat.did_work = true;
            } else {
                return stat;
            }
        }
        let ns = self.deps.clock.utc_ns(Tsc(now));
        let Some(transport) = &mut self.transport else {
            return stat;
        };
        if self.active.is_some() {
            if let Some(result) = transport.poll(ns)
                && let Some((lane, req)) = self.active.take()
            {
                self.record(lane, req, result, now);
                stat.did_work = true;
            }
            return stat;
        }
        if ns < self.next {
            return stat;
        }
        for offset in 0..2 {
            let lane = self.waiting.as_ref().map_or((self.rr + offset) % 2, |r| r.0);
            if self.deps.out[lane].is_none() {
                continue;
            }
            let Some(req) = self
                .waiting
                .take()
                .map(|r| r.1)
                .or_else(|| self.deps.reqs[lane].as_mut().and_then(Consumer::try_pop))
            else {
                continue;
            };
            if !self.rate.take(now) {
                self.waiting = Some((lane, req));
                return stat;
            }
            self.rr = (lane + 1) % 2;
            let mut r = RestReqBuf::default();
            let result = if req.kind != SnapReq::BBO {
                Err(hft_net::rest::Error::Request)
            } else {
                match P::bbo_snapshot_request(self.deps.rd, &self.ctx, &[req.inst], &mut r) {
                    Ok(true)
                        if r.endpoint == 0
                            && r.method == hft_venue_proto::Method::Get
                            && !r.redacted
                            && r.headers.is_empty()
                            && r.body.is_empty() =>
                    {
                        match core::str::from_utf8(r.path_query.as_slice()) {
                            Ok(path)
                                if path.starts_with('/')
                                    && !path.bytes().any(|b| b.is_ascii_control() || b == b' ') =>
                            {
                                let wire = format!(
                                    "GET {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nAccept-Encoding: identity\r\nContent-Length: 0\r\n\r\n",
                                    self.host
                                );
                                transport.start(
                                    BuildReq {
                                        venue: P::VENUE,
                                        conn: ConnId(SNAP_FIRST),
                                        role: ConnRole::Rest,
                                        endpoint: EndpointId(0),
                                        src_ip_idx: 0,
                                        rx_queue: 0,
                                        deadline_ns: ns
                                            .saturating_add(i64::from(self.cfg.timeout_ms) * 1_000_000),
                                        account: hft_types::AccountId(0),
                                    },
                                    wire.as_bytes(),
                                )
                            }
                            _ => Err(hft_net::rest::Error::Request),
                        }
                    }
                    _ => Err(hft_net::rest::Error::Request),
                }
            };
            if let Err(err) = result {
                self.record(lane, req, Err(err), now);
            } else {
                self.active = Some((lane, req));
            }
            stat.did_work = true;
            break;
        }
        stat
    }
}
impl<P: VenueProtocol, T: Transport> PollTask for SnapFetchTask<P, T> {
    fn poll_once(&mut self, now: Tsc) -> Polled {
        Polled {
            did_work: SnapFetchTask::poll_once(self, now.0).did_work,
        }
    }
    fn has_pending(&self) -> bool {
        self.pending.is_some()
            || self.active.is_some()
            || self.waiting.is_some()
            || self.deps.reqs.iter().flatten().any(|r| r.len_hint() > 0)
    }
}
