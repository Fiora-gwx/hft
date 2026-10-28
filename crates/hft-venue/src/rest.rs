//! Endpoint-indexed keep-alive transport shared by bounded warm tasks.
use crate::{AcctResponse, AcctTransport};
use hft_net::{ConnId, RestErr, RestPool, RestReq};
use hft_venue_proto::{Method, RestReqBuf};

#[derive(Debug)]
pub struct AcctRestPools<const N: usize = 2> {
    pools: Vec<RestPool<N>>,
    active: Option<usize>,
    tag: u64,
    connections: Option<AcctConnections>,
    ids: crate::IdRange,
    building: Vec<(ConnId, usize)>,
    retired: Option<hft_net::RetireMsg>,
}
/// One account's connection-builder channels. `first_conn..=last_conn` must be disjoint from all
/// other accounts and within the account REST range. Endpoints follow `RestReqBuf.endpoint`.
#[derive(Debug)]
pub struct AcctConnections {
    pub venue: hft_types::VenueId,
    pub account: hft_types::AccountId,
    pub endpoints: Vec<hft_net::EndpointId>,
    pub src_ip_idx: u8,
    pub first_conn: u8,
    pub last_conn: u8,
    /// The published TSC → UTC line: `BuildReq.deadline_ns` (the conn-builder's `now` reads the
    /// same line).
    pub clock: hft_rt::UtcClock,
    pub timeout_ms: u32,
    pub handoff: hft_rt::Consumer<crate::Handover>,
    pub retire: hft_rt::Producer<hft_net::RetireMsg>,
    pub build: hft_rt::Producer<hft_net::BuildReq>,
}
impl<const N: usize> AcctRestPools<N> {
    pub fn new(pools: Vec<RestPool<N>>) -> Result<Self, crate::InitErr> {
        if pools.is_empty() || pools.len() > crate::MAX_REST_ENDPOINTS {
            return Err(crate::InitErr::DepsMismatch);
        }
        Ok(Self {
            pools,
            active: None,
            tag: 0,
            connections: None,
            ids: crate::IdRange::new(crate::ACCT_FIRST, crate::ACCT_LAST),
            building: Vec::with_capacity(crate::MAX_REST_ENDPOINTS),
            retired: None,
        })
    }
    pub fn attach_connections(&mut self, c: AcctConnections) -> Result<(), crate::InitErr> {
        if self.connections.is_some()
            || c.endpoints.len() != self.pools.len()
            || c.first_conn < crate::ACCT_FIRST
            || c.last_conn > crate::ACCT_LAST
            || c.first_conn > c.last_conn
            || c.timeout_ms == 0
            || c.account.0 == 0
            || usize::from(c.last_conn - c.first_conn + 1) < N * self.pools.len()
        {
            return Err(crate::InitErr::DepsMismatch);
        }
        self.ids = crate::IdRange::new(c.first_conn, c.last_conn);
        self.connections = Some(c);
        Ok(())
    }
    fn maintain_connections(&mut self, now: u64) -> bool {
        let Some(c) = &mut self.connections else {
            return false;
        };
        let mut work = false;
        // A retired handle is retained until the builder accepts ownership.
        if self.retired.is_none() {
            self.retired = self.pools.iter_mut().find_map(RestPool::take_retired);
        }
        if let Some(r) = self.retired.take() {
            let id = r.id();
            if let Err(r) = c.retire.try_push(r) {
                self.retired = Some(r);
            } else {
                self.ids.free(id);
                work = true;
            }
        }
        if self.retired.is_some() {
            return work;
        }
        if let Some(h) = c.handoff.try_pop() {
            let id = match h {
                crate::Handover::Ready(h) => h.id(),
                crate::Handover::Failed(id) => id,
            };
            if let Some(pos) = self.building.iter().position(|(i, _)| *i == id) {
                let (_, ep) = self.building.swap_remove(pos);
                match h {
                    crate::Handover::Ready(h) => {
                        if self.pools[ep].install(h).is_err() {
                            self.pools[ep].build_failed();
                            self.retired = Some(h.retire(hft_net::ErrBits::PEER_EOF));
                        }
                    }
                    crate::Handover::Failed(_) => {
                        self.pools[ep].build_failed();
                        self.ids.free(id);
                    }
                }
            } else if let crate::Handover::Ready(h) = h {
                self.retired = Some(h.retire(hft_net::ErrBits::PEER_EOF));
            }
            work = true;
        }
        if self.retired.is_some() {
            return work;
        }
        for (ep, pool) in self.pools.iter_mut().enumerate() {
            let Some(_) = pool.next_build(now) else { continue };
            let Some(conn) = self.ids.alloc() else {
                pool.build_failed();
                break;
            };
            let req = hft_net::BuildReq {
                venue: c.venue,
                role: hft_event::ConnRole::Rest,
                conn,
                endpoint: c.endpoints[ep],
                src_ip_idx: c.src_ip_idx,
                rx_queue: 0,
                deadline_ns: c
                    .clock
                    .utc_ns(hft_types::Tsc(now))
                    .saturating_add(i64::from(c.timeout_ms) * 1_000_000),
            };
            if c.build.try_push(req).is_err() {
                pool.build_failed();
                self.ids.free(conn);
            } else {
                self.building.push((conn, ep));
                work = true;
            }
        }
        work
    }
    /// Connection-builder handover and reclamation run on the owning warm task.
    pub fn pool_mut(&mut self, endpoint: u8) -> Option<&mut RestPool<N>> {
        self.pools.get_mut(usize::from(endpoint))
    }
}

pub(crate) fn submit<const N: usize>(
    pool: &mut RestPool<N>,
    req: &RestReqBuf,
    tag: u64,
    now: u64,
) -> Result<ConnId, RestErr> {
    let mut headers = [(&[][..], &[][..]); 32];
    let mut count = 0;
    for line in req.headers.as_slice().split(|b| *b == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.is_empty() {
            continue;
        }
        let colon = line.iter().position(|b| *b == b':').ok_or(RestErr::Proto)?;
        let (name, rest) = line.split_at(colon);
        let value = rest.get(1..).ok_or(RestErr::Proto)?;
        let value = value.strip_prefix(b" ").unwrap_or(value);
        let row = headers.get_mut(count).ok_or(RestErr::ReqTooLarge)?;
        *row = (name, value);
        count += 1;
    }
    let r = RestReq {
        method: match req.method {
            Method::Get => hft_net::Method::Get,
            Method::Post => hft_net::Method::Post,
            Method::Put => hft_net::Method::Put,
            Method::Delete => hft_net::Method::Delete,
        },
        path_query: req.path_query.as_slice(),
        headers: &headers[..count],
        body: req.body.as_slice(),
        tag,
    };
    pool.submit(&r, now)?;
    pool.pending_conn(tag).ok_or(RestErr::NoConn)
}
impl<const N: usize> AcctTransport for AcctRestPools<N> {
    fn maintain(&mut self, now: u64) -> bool {
        self.maintain_connections(now)
    }
    fn submit(&mut self, req: &RestReqBuf, tag: u64, now: u64) -> Result<ConnId, RestErr> {
        if self.active.is_some() {
            return Err(RestErr::Busy);
        }
        let ix = usize::from(req.endpoint);
        let pool = self.pools.get_mut(ix).ok_or(RestErr::Proto)?;
        let conn = submit(pool, req, tag, now)?;
        self.active = Some(ix);
        self.tag = tag;
        Ok(conn)
    }
    fn poll(&mut self, now: u64) -> Option<Result<AcctResponse<'_>, RestErr>> {
        let ix = self.active?;
        let result = self.pools.get_mut(ix)?.poll(now)?;
        self.active = None;
        Some(
            result
                .map(|r| AcctResponse {
                    status: r.status,
                    body: r.body,
                    conn: r.conn,
                    ts: r.ts,
                })
                .map_err(|e| e.err),
        )
    }
    fn idle_check(&mut self, now: u64) {
        for pool in &mut self.pools {
            pool.idle_check(now);
        }
    }
    fn abort(&mut self) {
        if let Some(i) = self.active.take() {
            self.pools[i].cancel(self.tag);
        }
    }
    fn wait_fds(&self, out: &mut hft_rt::WaitFds) {
        for p in &self.pools {
            p.wait_fds(out);
        }
    }
    fn has_pending(&self) -> bool {
        self.pools.iter().any(RestPool::has_pending)
    }
}
