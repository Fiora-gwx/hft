//! Warm, bounded account query execution. Only the gateway publishes reports to the sequencer.
use crate::acct_wire::{self, Reports};
use crate::{ACCT_RAW_CHUNK, AcctFail, AcctRecHdr, AcctReq, GwCfg, InitErr, LoopStat};
use hft_net::{ConnId, RestErr, Timestamps};
use hft_refdata::RefData;
use hft_rt::{ByteProducer, Consumer, PollTask, Polled, UtcClock, WaitFds};
use hft_types::{AccountId, Tsc};
use hft_venue_proto::{AcctProtocol, AcctQuery, AcctQueryKind, RestReqBuf, WallClock};

#[derive(Debug)]
pub struct AcctResponse<'a> {
    pub status: u16,
    pub body: &'a [u8],
    pub conn: ConnId,
    pub ts: Timestamps,
}
/// A production implementation uses endpoint-indexed hft-net pools; tests inject wire responses.
pub trait AcctTransport {
    /// Bounded connection handover/reclamation on the owning warm task.
    fn maintain(&mut self, _now: u64) -> bool {
        false
    }
    fn submit(&mut self, req: &RestReqBuf, tag: u64, now: u64) -> Result<ConnId, RestErr>;
    fn poll(&mut self, now: u64) -> Option<Result<AcctResponse<'_>, RestErr>>;
    fn idle_check(&mut self, now: u64);
    /// Must retire the timed-out connection and discard its response before accepting another request.
    fn abort(&mut self);
    fn wait_fds(&self, _out: &mut WaitFds) {}
    fn has_pending(&self) -> bool {
        false
    }
}

pub struct AcctAccount<P: AcctProtocol, T: AcctTransport> {
    pub account: AccountId,
    pub templates: P::Templates,
    pub transport: T,
}
impl<P: AcctProtocol, T: AcctTransport> core::fmt::Debug for AcctAccount<P, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AcctAccount")
            .field("account", &self.account)
            .finish_non_exhaustive()
    }
}
struct Worker<P: AcctProtocol, T: AcctTransport> {
    spec: AcctAccount<P, T>,
    credential: hft_venue_proto::Credential,
    ctx: P::AcctCtx,
    scratch: P::AcctScratch,
    req: Option<AcctReq>,
    queries: &'static [AcctQuery],
    next: usize,
    active: Option<AcctQuery>,
    waiting: bool,
    raw: bool,
    raw_pos: usize,
    body: Vec<u8>,
    meta: [u8; 1040],
    meta_len: usize,
    hdr: Option<AcctRecHdr>,
    fail: AcctFail,
    reports: Reports,
    finished: bool,
    deadline: u64,
    sent: u64,
    submitted_conn: ConnId,
}
pub struct AcctFetchTask<P: AcctProtocol, T: AcctTransport> {
    rd: &'static RefData,
    cfg: GwCfg,
    t: crate::config::Ticks,
    /// Signing timestamps of the account queries.
    utc: UtcClock,
    reqs: Consumer<AcctReq>,
    pending_req: Option<AcctReq>,
    out: ByteProducer,
    workers: Vec<Worker<P, T>>,
    last_idle: u64,
    last: LoopStat,
}
impl<P: AcctProtocol, T: AcctTransport> core::fmt::Debug for AcctFetchTask<P, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AcctFetchTask")
            .field("accounts", &self.workers.len())
            .finish()
    }
}
impl<P: AcctProtocol, T: AcctTransport> AcctFetchTask<P, T> {
    pub fn new(
        rd: &'static RefData,
        cfg: GwCfg,
        clock: UtcClock,
        reqs: Consumer<AcctReq>,
        mut out: ByteProducer,
        accounts: Vec<AcctAccount<P, T>>,
    ) -> Result<Self, InitErr> {
        if cfg.acct_query_timeout_ms == 0
            || cfg.rest_idle_check_ms == 0
            || cfg.acct_body_max == 0
            || cfg.acct_body_max as usize > ACCT_RAW_CHUNK * 255
        {
            return Err(InitErr::Cfg("gw.acct_body_max"));
        }
        if out.reserve((acct_wire::HEADER + 65536) as u32).is_none() {
            return Err(InitErr::DepsMismatch);
        }
        for kind in [AcctQueryKind::Config, AcctQueryKind::Margin] {
            if P::acct_queries(kind)
                .iter()
                .any(|q| q.kind != kind || q.every == 0)
            {
                return Err(InitErr::Cfg("acct_queries"));
            }
        }
        let vm = rd.venue(P::VENUE);
        if vm.ws_max_conn_age_s != 0 && cfg.rotate_margin_s >= vm.ws_max_conn_age_s {
            return Err(InitErr::RotateMargin);
        }
        let mut workers = Vec::with_capacity(accounts.len());
        for spec in accounts {
            if matches!(rd.account(spec.account).kind, hft_refdata::AccountKind::Sim(_)) {
                return Err(InitErr::SimAccount);
            }
            if rd.venue_account_ix(P::VENUE, spec.account).is_none()
                || workers
                    .iter()
                    .any(|w: &Worker<P, T>| w.spec.account == spec.account)
            {
                return Err(InitErr::DepsMismatch);
            }
            workers.push(Worker {
                spec,
                credential: hft_venue_proto::Credential::default(),
                ctx: P::AcctCtx::default(),
                scratch: P::AcctScratch::default(),
                req: None,
                queries: &[],
                next: 0,
                active: None,
                waiting: false,
                raw: false,
                raw_pos: 0,
                body: Vec::with_capacity(cfg.acct_body_max as usize),
                meta: [0; 1040],
                meta_len: 0,
                hdr: None,
                fail: AcctFail::None,
                reports: Reports::new(65536),
                finished: false,
                deadline: 0,
                sent: 0,
                submitted_conn: ConnId(0),
            });
        }
        Ok(Self {
            rd,
            cfg,
            t: crate::config::Ticks::of(&clock),
            utc: clock,
            reqs,
            pending_req: None,
            out,
            workers,
            last_idle: 0,
            last: LoopStat::default(),
        })
    }
    pub fn transport_mut(&mut self, account: AccountId) -> Option<&mut T> {
        Some(
            &mut self
                .workers
                .iter_mut()
                .find(|w| w.spec.account == account)?
                .spec
                .transport,
        )
    }
    /// Startup only; the credential belongs to the warm worker, never to protocol state.
    pub fn set_credential(
        &mut self,
        account: AccountId,
        credential: hft_venue_proto::Credential,
    ) -> Result<(), InitErr> {
        let w = self
            .workers
            .iter_mut()
            .find(|w| w.spec.account == account)
            .ok_or(InitErr::DepsMismatch)?;
        if w.req.is_some() {
            return Err(InitErr::DepsMismatch);
        }
        w.credential = credential;
        Ok(())
    }
    pub fn poll_once(&mut self, now: u64) -> LoopStat {
        let mut stat = LoopStat::default();
        // Never pop a second request for a busy account: the producer keeps one active sequence.
        if self.pending_req.is_none() {
            self.pending_req = self.reqs.try_pop();
        }
        if let Some(r) = self.pending_req
            && !self.workers.iter().any(|w| w.spec.account == r.account)
            && let Some(dst) = self.out.reserve(acct_wire::HEADER as u32)
        {
            AcctRecHdr {
                seq_no: r.seq_no,
                account: r.account,
                what: acct_wire::FAILED,
                kind: r.kind,
                step: 0,
                part: 0,
                parts: 1,
                fail: AcctFail::Decode,
                conn: ConnId(0),
                http_status: 0,
                sent_tsc: now,
                ts: Timestamps::default(),
                req_len: 0,
                body_len: 0,
            }
            .write(dst);
            self.out.commit();
            self.pending_req = None;
            stat.did_work = true;
        }
        if let Some(r) = self.pending_req
            && let Some(w) = self.workers.iter_mut().find(|w| w.spec.account == r.account)
            && w.req.is_none()
        {
            self.pending_req = None;
            w.req = Some(r);
            w.submitted_conn = ConnId(0);
            w.sent = 0;
            w.queries = P::acct_queries(r.kind);
            w.next = 0;
            w.active = None;
            w.scratch = P::AcctScratch::default();
            w.reports.clear();
            w.fail = AcctFail::None;
            w.finished = false;
            w.deadline = now.saturating_add(self.t.ms(u64::from(self.cfg.acct_query_timeout_ms)));
            stat.did_work = true;
        }
        let idle = now.saturating_sub(self.last_idle) >= self.t.ms(u64::from(self.cfg.rest_idle_check_ms));
        if idle {
            self.last_idle = now;
        }
        for w in &mut self.workers {
            stat.did_work |= w.spec.transport.maintain(now);
            if idle {
                w.spec.transport.idle_check(now);
            }
            let Some(req) = w.req else { continue };
            if !w.raw && !w.finished {
                stat.next_deadline = Some(stat.next_deadline.map_or(w.deadline, |d| d.min(w.deadline)));
            }
            if w.raw {
                let left = w.body.len().saturating_sub(w.raw_pos);
                let n = left.min(ACCT_RAW_CHUNK);
                let metadata = if w.raw_pos == 0 { w.meta_len } else { 0 };
                let len = acct_wire::HEADER + metadata + n;
                if let Some(dst) = self.out.reserve(len as u32) {
                    if let Some(mut h) = w.hdr {
                        h.part = (w.raw_pos / ACCT_RAW_CHUNK) as u8;
                        h.parts = w.body.len().div_ceil(ACCT_RAW_CHUNK).max(1) as u8;
                        h.req_len = metadata as u32;
                        h.body_len = n as u32;
                        h.write(dst);
                    }
                    dst[acct_wire::HEADER..acct_wire::HEADER + metadata].copy_from_slice(&w.meta[..metadata]);
                    dst[acct_wire::HEADER + metadata..].copy_from_slice(&w.body[w.raw_pos..w.raw_pos + n]);
                    self.out.commit();
                    w.raw_pos += n;
                    stat.did_work = true;
                    if w.raw_pos == w.body.len() {
                        w.raw = false;
                        w.body.clear();
                        w.deadline = now.saturating_add(self.t.ms(u64::from(self.cfg.acct_query_timeout_ms)));
                    }
                }
                continue;
            }
            if w.waiting {
                if let Some(result) = w.spec.transport.poll(now) {
                    w.waiting = false;
                    w.body.clear();
                    w.raw_pos = 0;
                    let mut status = 0;
                    let mut ts = Timestamps::default();
                    let mut conn = w.submitted_conn;
                    match result {
                        Ok(r) => {
                            status = r.status;
                            ts = r.ts;
                            conn = r.conn;
                            if r.body.len() > self.cfg.acct_body_max as usize {
                                w.fail = AcctFail::TooLarge;
                            } else {
                                w.body.extend_from_slice(r.body);
                                w.fail = if status == 429 {
                                    AcctFail::RateLimited
                                } else if !(200..300).contains(&status) {
                                    AcctFail::Http
                                } else if let Some(q) = w.active {
                                    if P::acct_decode(
                                        self.rd,
                                        &mut w.ctx,
                                        req.account,
                                        q,
                                        status,
                                        r.body,
                                        &mut w.scratch,
                                    )
                                    .is_err()
                                    {
                                        AcctFail::Decode
                                    } else {
                                        AcctFail::None
                                    }
                                } else {
                                    AcctFail::Decode
                                };
                            }
                        }
                        Err(e) => {
                            w.fail = match e {
                                RestErr::Timeout => AcctFail::Timeout,
                                RestErr::TooLarge => AcctFail::TooLarge,
                                _ => AcctFail::Http,
                            }
                        }
                    }
                    w.hdr = Some(AcctRecHdr {
                        seq_no: req.seq_no,
                        account: req.account,
                        what: acct_wire::RAW,
                        kind: req.kind,
                        step: w.active.map_or(0, |q| q.step),
                        part: 0,
                        parts: 1,
                        fail: AcctFail::None,
                        conn,
                        http_status: status,
                        sent_tsc: w.sent,
                        ts,
                        req_len: 0,
                        body_len: 0,
                    });
                    w.raw = true;
                    stat.did_work = true;
                } else if now >= w.deadline {
                    w.spec.transport.abort();
                    w.waiting = false;
                    w.fail = AcctFail::Timeout;
                    w.body.clear();
                    w.raw_pos = 0;
                    w.raw = true;
                    w.hdr = Some(AcctRecHdr {
                        seq_no: req.seq_no,
                        account: req.account,
                        what: acct_wire::RAW,
                        kind: req.kind,
                        step: w.active.map_or(0, |q| q.step),
                        part: 0,
                        parts: 1,
                        fail: AcctFail::None,
                        conn: w.submitted_conn,
                        http_status: 0,
                        sent_tsc: w.sent,
                        ts: Timestamps::default(),
                        req_len: 0,
                        body_len: 0,
                    });
                    stat.did_work = true;
                }
                stat.next_deadline = Some(stat.next_deadline.map_or(w.deadline, |d| d.min(w.deadline)));
                continue;
            }
            if w.fail != AcctFail::None || w.finished {
                let bytes = if w.fail == AcctFail::None {
                    w.reports.bytes.as_slice()
                } else {
                    &[]
                };
                if let Some(dst) = self.out.reserve((acct_wire::HEADER + bytes.len()) as u32) {
                    AcctRecHdr {
                        seq_no: req.seq_no,
                        account: req.account,
                        what: if w.fail == AcctFail::None {
                            acct_wire::DONE
                        } else {
                            acct_wire::FAILED
                        },
                        kind: req.kind,
                        step: 0,
                        part: 0,
                        parts: 1,
                        fail: w.fail,
                        conn: w.submitted_conn,
                        http_status: 0,
                        sent_tsc: w.sent,
                        ts: Timestamps::default(),
                        req_len: 0,
                        body_len: bytes.len() as u32,
                    }
                    .write(dst);
                    dst[acct_wire::HEADER..].copy_from_slice(bytes);
                    self.out.commit();
                    w.req = None;
                    w.finished = false;
                    stat.did_work = true;
                }
                continue;
            }
            let fallback = req.kind == AcctQueryKind::Margin
                && self.rd.caps(P::VENUE).margin_data == hft_refdata::MarginData::Push;
            while w.next < w.queries.len() && !w.queries[w.next].selected(req.poll_no, fallback) {
                w.next += 1;
            }
            if let Some(q) = w.queries.get(w.next).copied() {
                let mut wire = RestReqBuf::default();
                let utc = self.utc.utc_ns(hft_types::Tsc(now));
                let clk = WallClock {
                    unix_ms: utc / 1_000_000,
                    unix_us: utc / 1000,
                };
                if P::acct_request_signed(
                    self.rd,
                    &w.spec.templates,
                    &w.credential,
                    req.account,
                    q,
                    &clk,
                    &mut wire,
                )
                .is_err()
                {
                    w.fail = AcctFail::Decode;
                    continue;
                }
                let result = w.spec.transport.submit(&wire, u64::from(req.seq_no), now);
                // Only method and path, never the query string or headers (which may contain keys).
                let path = wire
                    .path_query
                    .as_slice()
                    .split(|b| *b == b'?')
                    .next()
                    .unwrap_or(&[]);
                w.meta[0] = wire.method as u8;
                w.meta[1..1 + path.len()].copy_from_slice(path);
                w.meta_len = 1 + path.len();
                wire.headers.clear();
                wire.body.clear();
                wire.path_query.clear();
                match result {
                    Ok(conn) => {
                        w.submitted_conn = conn;
                        w.active = Some(q);
                        w.next += 1;
                        w.waiting = true;
                        w.sent = now;
                        w.deadline = now.saturating_add(self.t.ms(u64::from(self.cfg.acct_query_timeout_ms)));
                        stat.did_work = true;
                    }
                    Err(RestErr::Busy | RestErr::NoConn) => {
                        if now >= w.deadline {
                            w.fail = AcctFail::Timeout;
                        }
                    }
                    Err(_) => w.fail = AcctFail::Http,
                }
            } else {
                if P::acct_finish(
                    self.rd,
                    &mut w.ctx,
                    req.account,
                    req.kind,
                    &w.scratch,
                    &mut w.reports,
                )
                .is_err()
                    || w.reports.failed
                {
                    w.fail = AcctFail::Decode;
                }
                w.reports.session(req, self.rd);
                if w.reports.failed {
                    w.fail = AcctFail::Decode;
                }
                w.finished = true;
                stat.did_work = true;
            }
        }
        self.last = stat;
        stat
    }
}
impl<P: AcctProtocol, T: AcctTransport> PollTask for AcctFetchTask<P, T> {
    fn poll_once(&mut self, now: Tsc) -> Polled {
        Polled {
            did_work: self.poll_once(now.0).did_work,
        }
    }
    fn next_deadline(&self) -> Option<Tsc> {
        self.last.next_deadline.map(Tsc)
    }
    fn wait_fds(&self, out: &mut WaitFds) {
        for w in &self.workers {
            w.spec.transport.wait_fds(out);
        }
    }
    fn has_pending(&self) -> bool {
        self.pending_req.is_some()
            || self.reqs.len_hint() > 0
            || self.workers.iter().any(|w| {
                w.raw || w.finished || (w.req.is_some() && !w.waiting) || w.spec.transport.has_pending()
            })
    }
}
