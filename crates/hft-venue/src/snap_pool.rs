//! Bounded keep-alive BBO snapshots. The feed remains the only raw-journal writer.
use crate::{InitErr, LoopStat, SNAP_BBO_BATCH_MAX, SnapCfg, SnapDeps, SnapRecHdr, SnapReq};
use hft_event::{Bbo, FeedMsg};
use hft_net::{ConnId, RestErr, RestPool};
use hft_rt::{PollTask, Polled, WaitFds};
use hft_types::{AccountId, InstId, Tsc};
use hft_venue_proto::{ProtoErr, RestReqBuf, RxMeta, VenueProtocol};
use std::collections::VecDeque;
use zerocopy::IntoBytes;

#[derive(Clone, Copy)]
struct Request {
    lane: usize,
    req: SnapReq,
}
struct Batch {
    tag: u64,
    requests: Vec<Request>,
    deadline: u64,
    conn: ConnId,
}
/// Connection ownership channels; all producers to the builder use the configured no-wake policy.
#[derive(Debug)]
pub struct SnapConnections {
    pub handoff: hft_rt::Consumer<crate::Handover>,
    pub retire: hft_rt::Producer<hft_net::RetireMsg>,
    pub build: hft_rt::Producer<hft_net::BuildReq>,
    pub endpoint: hft_net::EndpointId,
    pub src_ip_idx: u8,
}
struct DirectConnections {
    connector: hft_net::Connector,
    endpoint: hft_net::EndpointId,
    src_ip_idx: u8,
    building: Option<(hft_net::JobId, ConnId)>,
}
/// Startup-allocated worker; pool handovers/reclamation belong to the same warm task.
pub struct PooledSnapFetchTask<P: VenueProtocol, const N: usize = 2> {
    cfg: SnapCfg,
    deps: SnapDeps,
    ctx: P::FeedCtx,
    pool: RestPool<N>,
    rate: crate::RateCounters,
    queue: VecDeque<Request>,
    batches: Vec<Batch>,
    completed: Option<usize>,
    emitted: usize,
    body: Vec<u8>,
    sink: crate::sink::FeedSink,
    status: u8,
    http: u16,
    rx: RxMeta,
    rr: usize,
    tag: u64,
    next: u64,
    idle: u64,
    last: LoopStat,
    connections: Option<SnapConnections>,
    direct: Option<DirectConnections>,
    ids: crate::IdRange,
    retired: Option<hft_net::RetireMsg>,
}
impl<P: VenueProtocol, const N: usize> core::fmt::Debug for PooledSnapFetchTask<P, N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PooledSnapFetchTask")
            .field("cfg", &self.cfg)
            .finish_non_exhaustive()
    }
}
impl<P: VenueProtocol, const N: usize> PooledSnapFetchTask<P, N> {
    pub fn new(
        cfg: SnapCfg,
        mut deps: SnapDeps,
        ctx: P::FeedCtx,
        pool: RestPool<N>,
    ) -> Result<Self, InitErr> {
        if cfg.parallel == 0
            || usize::from(cfg.parallel) > N
            || N > usize::from(crate::SNAP_LAST - crate::SNAP_FIRST + 1)
        {
            return Err(InitErr::Cfg("snap.parallel"));
        }
        if cfg.timeout_ms == 0
            || cfg.rest_idle_check_ms == 0
            || cfg.request_gap_ms == 0
            || cfg.max_record_bytes < (size_of::<SnapRecHdr>() + size_of::<Bbo>()) as u32
        {
            return Err(InitErr::Cfg("snap.bounds"));
        }
        for lane in 0..2 {
            if deps.reqs[lane].is_some() != deps.out[lane].is_some() {
                return Err(InitErr::DepsMismatch);
            }
            if let Some(out) = &mut deps.out[lane]
                && out.reserve(cfg.max_record_bytes).is_none()
            {
                return Err(InitErr::DepsMismatch);
            }
        }
        if deps.reqs.iter().all(Option::is_none) {
            return Err(InitErr::DepsMismatch);
        }
        let age = deps.rd.venue(P::VENUE).ws_max_conn_age_s;
        if age != 0 && cfg.rotate_margin_s >= age {
            return Err(InitErr::RotateMargin);
        }
        let t = crate::config::Ticks::of(&deps.clock);
        let rate = crate::RateCounters::rest(deps.rd.rate_limits(P::VENUE), |ms| t.ms(ms));
        Ok(Self {
            cfg,
            deps,
            ctx,
            pool,
            rate,
            queue: VecDeque::with_capacity(SNAP_BBO_BATCH_MAX),
            batches: (0..usize::from(cfg.parallel))
                .map(|_| Batch {
                    tag: 0,
                    requests: Vec::with_capacity(SNAP_BBO_BATCH_MAX),
                    deadline: 0,
                    conn: ConnId(0),
                })
                .collect(),
            completed: None,
            emitted: 0,
            body: Vec::with_capacity(cfg.max_record_bytes as usize),
            sink: crate::sink::FeedSink::new(SNAP_BBO_BATCH_MAX as u16),
            status: 0,
            http: 0,
            rx: RxMeta::default(),
            rr: 0,
            tag: 0,
            next: 0,
            idle: 0,
            last: LoopStat::default(),
            connections: None,
            direct: None,
            ids: crate::IdRange::new(crate::SNAP_FIRST, crate::SNAP_LAST),
            retired: None,
        })
    }
    pub fn pool_mut(&mut self) -> &mut RestPool<N> {
        &mut self.pool
    }
    pub fn set_connections(&mut self, deps: SnapConnections) -> Result<(), InitErr> {
        if self.connections.is_some() || self.direct.is_some() {
            return Err(InitErr::DepsMismatch);
        }
        self.connections = Some(deps);
        Ok(())
    }
    /// Own connection setup on this same warm thread, using the configured market-data source IP.
    pub fn attach_connector(
        &mut self,
        connector: hft_net::Connector,
        endpoint: hft_net::EndpointId,
        src_ip_idx: u8,
    ) -> Result<(), InitErr> {
        if self.connections.is_some() || self.direct.is_some() {
            return Err(InitErr::DepsMismatch);
        }
        self.direct = Some(DirectConnections {
            connector,
            endpoint,
            src_ip_idx,
            building: None,
        });
        Ok(())
    }
    fn connections(&mut self, now: u64) -> bool {
        if let Some(c) = &mut self.direct {
            let mut work = false;
            let ns = self.deps.clock.utc_ns(Tsc(now));
            if let Some(ev) = c.connector.drive(ns) {
                match ev {
                    hft_net::BuildEvent::Ready { job, .. } => {
                        let pending = c.building.take();
                        if let Ok(h) = c.connector.handoff(job) {
                            if self.pool.install(h).is_err() {
                                self.ids.free(h.id());
                                c.connector.reclaim(h.retire(hft_net::ErrBits::empty()));
                                self.pool.build_failed();
                            }
                        } else {
                            c.connector.cancel(job);
                            if let Some((_, id)) = pending {
                                self.ids.free(id);
                            }
                            self.pool.build_failed();
                        }
                    }
                    hft_net::BuildEvent::Failed { .. } => {
                        if let Some((_, id)) = c.building.take() {
                            self.ids.free(id);
                        }
                        self.pool.build_failed();
                    }
                }
                work = true;
            }
            while let Some(m) = self.pool.take_retired() {
                self.ids.free(m.id());
                c.connector.reclaim(m);
                work = true;
            }
            if self.pool.next_build(now).is_some() {
                if let Some(id) = self.ids.alloc() {
                    let req = hft_net::BuildReq {
                        venue: P::VENUE,
                        conn: id,
                        role: hft_net::ConnRole::Rest,
                        endpoint: c.endpoint,
                        src_ip_idx: c.src_ip_idx,
                        rx_queue: 0,
                        deadline_ns: ns.saturating_add(i64::from(self.cfg.timeout_ms) * 1_000_000),
                        account: hft_types::AccountId(0),
                    };
                    match c.connector.submit(req) {
                        Ok(job) => c.building = Some((job, id)),
                        Err(_) => {
                            self.ids.free(id);
                            self.pool.build_failed();
                        }
                    }
                    work = true;
                } else {
                    self.pool.build_failed();
                }
            }
            return work;
        }
        let Some(c) = &mut self.connections else {
            return false;
        };
        let mut work = false;
        if self.retired.is_none()
            && let Some(h) = c.handoff.try_pop()
        {
            match h {
                crate::Handover::Ready(h) => {
                    if self.pool.install(h).is_err() {
                        self.retired = Some(h.retire(hft_net::ErrBits::empty()));
                        self.pool.build_failed();
                    }
                }
                crate::Handover::Failed(id) => {
                    self.pool.build_failed();
                    self.ids.free(id);
                }
            }
            work = true;
        }
        if self.retired.is_none() {
            self.retired = self.pool.take_retired();
        }
        if let Some(m) = self.retired.take() {
            let id = m.id();
            match c.retire.try_push(m) {
                Ok(()) => {
                    self.ids.free(id);
                    work = true;
                }
                Err(m) => self.retired = Some(m),
            }
        }
        if self.retired.is_none() && self.pool.next_build(now).is_some() {
            if let Some(id) = self.ids.alloc() {
                let req = hft_net::BuildReq {
                    venue: P::VENUE,
                    conn: id,
                    role: hft_net::ConnRole::Rest,
                    endpoint: c.endpoint,
                    src_ip_idx: c.src_ip_idx,
                    rx_queue: 0,
                    deadline_ns: self
                        .deps
                        .clock
                        .utc_ns(Tsc(now))
                        .saturating_add(i64::from(self.cfg.timeout_ms) * 1_000_000),
                    account: hft_types::AccountId(0),
                };
                if c.build.try_push(req).is_ok() {
                    work = true;
                } else {
                    self.ids.free(id);
                    self.pool.build_failed();
                }
            } else {
                self.pool.build_failed();
            }
        }
        work
    }
    fn ticks(&self) -> crate::config::Ticks {
        crate::config::Ticks::of(&self.deps.clock)
    }
    fn complete(&mut self, ix: usize, status: u8, now: u64) {
        self.completed = Some(ix);
        self.emitted = 0;
        let bytes = size_of::<SnapRecHdr>()
            + self.body.len()
            + if status == SnapRecHdr::OK {
                size_of::<Bbo>()
            } else {
                0
            };
        self.status = if bytes > self.cfg.max_record_bytes as usize {
            self.body.clear();
            self.sink.begin(SNAP_BBO_BATCH_MAX);
            SnapRecHdr::TOO_LARGE
        } else {
            status
        };
        self.rx.rx_tsc = now;
        self.rx.conn = self.batches[ix].conn;
    }
    fn emit(&mut self) -> bool {
        let Some(ix) = self.completed else { return false };
        let batch = &mut self.batches[ix];
        let Some(r) = batch.requests.get(self.emitted) else {
            return false;
        };
        let quote = if self.status == SnapRecHdr::OK {
            self.sink.evs[..self.sink.n].iter().find_map(|e| match e {
                FeedMsg::Bbo(b, _) if b.h.inst == r.req.inst => Some(b),
                _ => None,
            })
        } else {
            None
        };
        let raw = if self.emitted == 0 {
            self.body.as_slice()
        } else {
            &[]
        };
        let mut h = SnapRecHdr {
            rx_tsc: self.rx.rx_tsc,
            sw_rx_ns: self.rx.sw_rx_ns,
            req_id: r.req.req_id,
            inst: r.req.inst,
            body_len: raw.len() as u32,
            status: self.status,
            kind: r.req.kind,
            conn: batch.conn,
            http_status: self.http,
            _pad: 0,
            _pad2: 0,
            ts_flags: self.rx.ts_flags,
        };
        let len = size_of::<SnapRecHdr>() + raw.len() + quote.map_or(0, |_| size_of::<Bbo>());
        let oversized = len > self.cfg.max_record_bytes as usize;
        let len = if oversized {
            h.status = SnapRecHdr::TOO_LARGE;
            h.body_len = 0;
            size_of::<SnapRecHdr>()
        } else {
            len
        };
        let Some(dst) = self.deps.out[r.lane]
            .as_mut()
            .and_then(|out| out.reserve(len as u32))
        else {
            return false;
        };
        dst[..size_of::<SnapRecHdr>()].copy_from_slice(h.as_bytes());
        if !oversized {
            dst[size_of::<SnapRecHdr>()..size_of::<SnapRecHdr>() + raw.len()].copy_from_slice(raw);
            if let Some(b) = quote {
                dst[size_of::<SnapRecHdr>() + raw.len()..].copy_from_slice(b.as_bytes());
            }
        }
        if let Some(out) = self.deps.out[r.lane].as_mut() {
            out.commit();
        }
        self.emitted += 1;
        if self.emitted == batch.requests.len() {
            batch.requests.clear();
            self.completed = None;
            self.body.clear();
            self.sink.begin(SNAP_BBO_BATCH_MAX);
        }
        true
    }
    pub fn poll_once(&mut self, now: u64) -> LoopStat {
        let mut stat = LoopStat::default();
        let idle_period = self.ticks().ms(u64::from(self.cfg.rest_idle_check_ms));
        if now >= self.idle.saturating_add(idle_period) {
            self.pool.idle_check(now);
            self.idle = now;
        }
        stat.did_work |= self.connections(now);
        if self.completed.is_some() {
            stat.did_work |= self.emit();
            if self.completed.is_some() {
                self.last = stat;
                return stat;
            }
        }
        if let Some(ix) = self
            .batches
            .iter()
            .position(|b| !b.requests.is_empty() && now >= b.deadline)
        {
            self.pool.cancel(self.batches[ix].tag);
            self.body.clear();
            self.sink.begin(SNAP_BBO_BATCH_MAX);
            self.http = 0;
            self.rx = RxMeta::default();
            self.complete(ix, SnapRecHdr::TIMEOUT, now);
            stat.did_work |= self.emit();
        } else if let Some(result) = self.pool.poll(now) {
            let tag = match &result {
                Ok(r) => r.tag,
                Err(e) => e.tag,
            };
            if let Some(ix) = self
                .batches
                .iter()
                .position(|b| !b.requests.is_empty() && b.tag == tag)
            {
                self.body.clear();
                self.sink.begin(SNAP_BBO_BATCH_MAX);
                self.http = 0;
                self.rx = RxMeta::default();
                let mut status = SnapRecHdr::HTTP;
                match result {
                    Err(e) => {
                        status = match e.err {
                            RestErr::Timeout => SnapRecHdr::TIMEOUT,
                            RestErr::TooLarge => SnapRecHdr::TOO_LARGE,
                            _ => SnapRecHdr::HTTP,
                        }
                    }
                    Ok(r) => {
                        self.http = r.status;
                        self.rx = RxMeta {
                            rx_tsc: now,
                            hw_rx_ns: r.ts.best_rx().0,
                            sw_rx_ns: r.ts.sw_rx_ns,
                            conn: r.conn,
                            account: AccountId(0),
                            ts_flags: r.ts.flags,
                            ..RxMeta::default()
                        };
                        if r.body.len() > self.cfg.max_record_bytes as usize {
                            status = SnapRecHdr::TOO_LARGE;
                        } else {
                            self.body.extend_from_slice(r.body);
                            if r.status == 429 || r.status == 418 {
                                status = SnapRecHdr::RATE;
                            } else if (200..300).contains(&r.status) {
                                let mut insts = [InstId(0); SNAP_BBO_BATCH_MAX];
                                let n = self.batches[ix].requests.len();
                                for (dst, req) in insts.iter_mut().zip(&self.batches[ix].requests) {
                                    *dst = req.req.inst;
                                }
                                self.sink.rx_flags = crate::latency::rx_flags(self.rx.ts_flags);
                                let decoded = P::decode_bbo_snapshot(
                                    self.deps.rd,
                                    &mut self.ctx,
                                    &insts[..n],
                                    r.body,
                                    &self.rx,
                                    &mut self.sink,
                                );
                                let valid = decoded == Ok(n as u16) && self.sink.n == n && !self.sink.over
                                    && insts[..n].iter().all(|i| self.sink.evs[..self.sink.n].iter().filter(|e| matches!(e, FeedMsg::Bbo(b, _) if b.h.inst == *i && b.h.venue == P::VENUE)).count() == 1);
                                status = if valid { SnapRecHdr::OK } else { SnapRecHdr::DECODE };
                            }
                        }
                    }
                }
                self.complete(ix, status, now);
                stat.did_work = true;
                self.emit();
            }
        }
        // A completed batch owns its response buffer until every record has been committed.
        if self.completed.is_none()
            && now >= self.next
            && let Some(ix) = self.batches.iter().position(|b| b.requests.is_empty())
        {
            for _ in self.queue.len()..SNAP_BBO_BATCH_MAX {
                let mut found = false;
                for _ in 0..2 {
                    let lane = self.rr;
                    self.rr = (self.rr + 1) % 2;
                    if let Some(req) = self.deps.reqs[lane].as_mut().and_then(|r| r.try_pop()) {
                        self.queue.push_back(Request { lane, req });
                        found = true;
                        stat.did_work = true;
                        break;
                    }
                }
                if !found {
                    break;
                }
            }
            if !self.queue.is_empty() {
                let mut insts = [InstId(0); SNAP_BBO_BATCH_MAX];
                let mut n = 0;
                for r in self
                    .queue
                    .iter()
                    .take(usize::from(P::BBO_SNAPSHOT_BATCH).min(SNAP_BBO_BATCH_MAX))
                {
                    if r.req.kind != SnapReq::BBO || insts[..n].contains(&r.req.inst) {
                        break;
                    }
                    insts[n] = r.req.inst;
                    n += 1;
                }
                let (encoded, wire) = loop {
                    let mut wire = RestReqBuf::default();
                    let r = if n == 0 {
                        Err(ProtoErr::Cfg)
                    } else {
                        P::bbo_snapshot_request(self.deps.rd, &self.ctx, &insts[..n], &mut wire)
                    };
                    if r == Err(ProtoErr::BufFull) && n > 1 {
                        n /= 2;
                    } else {
                        break (r, wire);
                    }
                };
                let valid = encoded == Ok(true)
                    && wire.endpoint == 0
                    && !wire.redacted
                    && wire.method == hft_venue_proto::Method::Get
                    && wire.headers.is_empty()
                    && wire.body.is_empty();
                if !valid || self.rate.available(now) {
                    self.tag = self.tag.wrapping_add(1);
                    let sent = if valid {
                        crate::rest::submit(&mut self.pool, &wire, self.tag, false, now).map(|(conn, _)| conn)
                    } else {
                        Err(RestErr::Proto)
                    };
                    if !matches!(sent, Err(RestErr::Busy | RestErr::NoConn)) {
                        if valid {
                            self.rate.charge();
                        }
                        let batch = &mut self.batches[ix];
                        batch.tag = self.tag;
                        batch.conn = sent.unwrap_or(ConnId(crate::SNAP_FIRST));
                        for _ in 0..n.max(1) {
                            if let Some(r) = self.queue.pop_front() {
                                batch.requests.push(r);
                            }
                        }
                        batch.deadline = now.saturating_add(
                            crate::config::Ticks::of(&self.deps.clock).ms(u64::from(self.cfg.timeout_ms)),
                        );
                        self.next = now.saturating_add(self.ticks().ms(u64::from(self.cfg.request_gap_ms)));
                        if sent.is_err() {
                            self.http = 0;
                            self.body.clear();
                            self.sink.begin(SNAP_BBO_BATCH_MAX);
                            self.rx = RxMeta::default();
                            self.complete(ix, SnapRecHdr::REQUEST, now);
                            self.emit();
                        }
                        stat.did_work = true;
                    }
                }
            }
        }
        stat.next_deadline = Some(self.idle.saturating_add(idle_period));
        if !self.queue.is_empty() && self.next > now {
            stat.next_deadline = crate::min_deadline(stat.next_deadline, self.next);
        }
        for b in &self.batches {
            if !b.requests.is_empty() {
                stat.next_deadline = crate::min_deadline(stat.next_deadline, b.deadline);
            }
        }
        self.last = stat;
        stat
    }
}
impl<P: VenueProtocol, const N: usize> Drop for PooledSnapFetchTask<P, N> {
    fn drop(&mut self) {
        if let Some(c) = &mut self.direct {
            if let Some((job, _)) = c.building.take() {
                c.connector.cancel(job);
            }
            self.pool.retire_all();
            while let Some(m) = self.pool.take_retired() {
                c.connector.reclaim(m);
            }
        }
    }
}
impl<P: VenueProtocol, const N: usize> PollTask for PooledSnapFetchTask<P, N> {
    fn poll_once(&mut self, now: Tsc) -> Polled {
        Polled {
            did_work: self.poll_once(now.0).did_work,
        }
    }
    fn has_pending(&self) -> bool {
        self.completed.is_some()
            || !self.queue.is_empty()
            || self.pool.has_pending()
            || self.deps.reqs.iter().flatten().any(|r| r.len_hint() > 0)
    }
    fn wait_fds(&self, out: &mut WaitFds) {
        self.pool.wait_fds(out);
    }
    fn next_deadline(&self) -> Option<Tsc> {
        self.last.next_deadline.map(Tsc)
    }
}
