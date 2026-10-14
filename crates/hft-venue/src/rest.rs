//! Endpoint-indexed keep-alive transport shared by bounded warm tasks.
use crate::{AcctResponse, AcctTransport};
use hft_net::{ConnId, RestErr, RestPool, RestReq};
use hft_venue_proto::{Method, RestReqBuf};

#[derive(Debug)]
pub struct AcctRestPools<const N: usize = 2> {
    pools: Vec<RestPool<N>>,
    active: Option<usize>,
    tag: u64,
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
        })
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
