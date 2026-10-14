//! Warm session setup: endpoint, login, verified identity handover, private subscription templates.
//! The driver can perform bootstrap/identity REST, or accept verified evidence per job.
//! Missing evidence for a protocol requiring a distinct session key is handed over as failed.

use std::collections::VecDeque;

use hft_event::ConnRole;
use hft_net::{
    BuildErr, BuildEvent, BuildReq, ConnId, ConnPhase, Connector, Handoff, JobId, ReadyHandle, RetireMsg,
};
use hft_types::{AccountId, MAX_BBO_CONN_SLOTS};
use hft_venue_proto::{
    Credential, EndpointSpec, LoginResult, LoginStep, ProtoErr, RestReqBuf, RouteId, VenueCfg, VenueProtocol,
};

use crate::conn_id::MD_LAST;

/// Driver configuration (hft-core assembly).
#[derive(Copy, Clone, Debug)]
pub struct SessionCfg {
    pub venue: VenueCfg,
    /// Readiness handles of the feed tasks with `feed.ready_poll` in effect: BBO slots and the
    /// feed-md range (README §7 step 4).
    pub ready_bbo: Option<ReadyHandle>,
    pub ready_md: Option<ReadyHandle>,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum SessErr {
    /// More sessions than the connector's jobs.
    Busy,
    Build(BuildErr),
    Proto(ProtoErr),
    /// The role needs a bootstrap REST call (M3).
    Bootstrap,
}

/// Outcome of a session.
#[derive(Copy, Clone, Debug)]
pub enum SessEvent {
    /// Logged in (when needed) and handed off; push it to the requesting task's hand-over ring.
    Ready(Handoff),
    /// Not handed off (the connection was reclaimed); `code` = the login rejection code or 0.
    Failed { conn: ConnId, code: u32 },
}

/// Ordered log of the driver's actions (observability and tests).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum SessionOp {
    Endpoint(ConnId),
    Submit(ConnId),
    LoginFrame(ConnId, u8),
    ReadyAdd(ConnId),
    Handoff(ConnId),
    ReadyDel(ConnId),
    Reclaim(ConnId),
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum Phase {
    Building,
    Login(u8),
    Identity { attempts: u8, retry_ns: i64 },
    IdentityDone,
}

#[derive(Copy, Clone)]
struct Queued {
    req: BuildReq,
    acct: Option<AccountId>,
}
#[derive(Copy, Clone)]
enum RestOwner {
    Bootstrap(Queued),
    Identity(usize),
}
#[derive(Copy, Clone)]
struct RestWork {
    owner: RestOwner,
    deadline_ns: i64,
}

#[derive(Copy, Clone, Debug)]
struct SJob {
    job: JobId,
    req: BuildReq,
    acct: Option<AccountId>,
    phase: Phase,
    identity: Option<[u8; 24]>,
}

const JOBS: usize = hft_net::MAX_JOBS;
const LOG_CAP: usize = 1024;

use hft_venue_proto::valid_uid;

pub struct SessionDriver<P: VenueProtocol, T: crate::AcctTransport = crate::AcctRestPools> {
    cfg: SessionCfg,
    creds: Vec<(AccountId, Credential)>,
    jobs: [Option<SJob>; JOBS],
    fds: Vec<(ConnId, i32)>,
    out: Vec<u8>,
    log: VecDeque<SessionOp>,
    rest: Option<T>,
    rest_account: Option<(AccountId, Credential)>,
    rest_cfg: crate::GwCfg,
    rest_work: Option<RestWork>,
    queued: VecDeque<Queued>,
    _p: core::marker::PhantomData<P>,
}

impl<P: VenueProtocol, T: crate::AcctTransport> core::fmt::Debug for SessionDriver<P, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SessionDriver").field("venue", &P::VENUE).finish()
    }
}

impl<P: VenueProtocol, T: crate::AcctTransport> SessionDriver<P, T> {
    /// `creds` = the session credential of each account (Order/UserStream roles).
    pub fn new(cfg: SessionCfg, creds: Vec<(AccountId, Credential)>) -> Self {
        SessionDriver {
            cfg,
            creds,
            jobs: [None; JOBS],
            fds: Vec::with_capacity(256),
            out: Vec::with_capacity(4096),
            log: VecDeque::with_capacity(LOG_CAP),
            rest: None,
            rest_account: None,
            rest_cfg: crate::GwCfg::default(),
            rest_work: None,
            queued: VecDeque::with_capacity(JOBS),
            _p: core::marker::PhantomData,
        }
    }

    /// Startup only. This transport is scoped to one account's REST credential and source IP.
    pub fn set_rest(
        &mut self,
        rest: T,
        cfg: crate::GwCfg,
        account: AccountId,
        credential: Credential,
    ) -> Result<(), SessErr> {
        if self.rest.is_some() || cfg.acct_query_timeout_ms == 0 || cfg.acct_query_degraded_after == 0 {
            return Err(SessErr::Proto(ProtoErr::Cfg));
        }
        self.rest = Some(rest);
        self.rest_account = Some((account, credential));
        self.rest_cfg = cfg;
        Ok(())
    }
    pub fn rest_mut(&mut self) -> Option<&mut T> {
        self.rest.as_mut()
    }
    pub fn job(&self, conn: ConnId) -> Option<JobId> {
        self.jobs
            .iter()
            .flatten()
            .find(|j| j.req.conn == conn)
            .map(|j| j.job)
    }
    /// Queue a connection that may require bootstrap. No socket is handed off before bootstrap,
    /// login and the bounded session-key identity attempt have completed.
    pub fn enqueue(&mut self, req: BuildReq, acct: Option<AccountId>) -> Result<(), SessErr> {
        if self.rest.is_none() {
            return Err(SessErr::Bootstrap);
        }
        if acct.is_some() && self.rest_account.as_ref().map(|(a, _)| *a) != acct {
            return Err(SessErr::Proto(ProtoErr::Cfg));
        }
        if self.queued.len() + self.jobs.iter().flatten().count() + usize::from(self.rest_work.is_some())
            >= JOBS
        {
            return Err(SessErr::Busy);
        }
        if self.queued.iter().any(|q| q.req.conn == req.conn)
            || self.jobs.iter().flatten().any(|j| j.req.conn == req.conn)
            || matches!(self.rest_work, Some(RestWork { owner: RestOwner::Bootstrap(q), .. }) if q.req.conn == req.conn)
        {
            return Err(SessErr::Busy);
        }
        self.queued.push_back(Queued { req, acct });
        Ok(())
    }
    /// Drive REST using counter ticks and connector/login deadlines using injected UTC nanoseconds.
    pub fn drive_at(&mut self, c: &mut Connector, now_ns: i64, now_tsc: u64) -> Option<SessEvent> {
        if let Some(w) = self.rest_work {
            let result = self.rest.as_mut().and_then(|r| r.poll(now_tsc));
            if result.is_some() || now_ns >= w.deadline_ns {
                let mut boot = None;
                let mut uid = [0; 24];
                let ok = match result {
                    Some(Ok(r)) if (200..300).contains(&r.status) => match w.owner {
                        RestOwner::Bootstrap(_) => {
                            boot = Some(r.body.to_vec());
                            true
                        }
                        RestOwner::Identity(_) => {
                            P::decode_identity(r.status, r.body, &mut uid).is_ok() && valid_uid(&uid)
                        }
                    },
                    _ => false,
                };
                if !ok && let Some(r) = &mut self.rest {
                    r.abort();
                }
                self.rest_work = None;
                match w.owner {
                    RestOwner::Bootstrap(q) => {
                        let started = ok
                            && self
                                .start_prepared(c, q.req, q.acct, now_ns, boot.as_deref())
                                .is_ok();
                        if let Some(b) = &mut boot {
                            b.fill(0);
                        }
                        if !started {
                            return Some(SessEvent::Failed {
                                conn: q.req.conn,
                                code: 0,
                            });
                        }
                    }
                    RestOwner::Identity(i) => {
                        let Some(j) = self.jobs[i].as_mut() else {
                            return self.drive(c, now_ns);
                        };
                        let Phase::Identity { attempts, .. } = j.phase else {
                            return self.drive(c, now_ns);
                        };
                        if ok || attempts >= self.rest_cfg.acct_query_degraded_after {
                            j.identity = ok.then_some(uid);
                            j.phase = Phase::IdentityDone;
                            return Some(self.finish(c, i, now_ns));
                        }
                        let seconds = (1i64 << attempts.saturating_sub(1).min(5)).min(30);
                        j.phase = Phase::Identity {
                            attempts,
                            retry_ns: now_ns.saturating_add(seconds * 1_000_000_000),
                        };
                    }
                }
            }
        }
        if self.rest_work.is_none() {
            let identity = self.jobs.iter().position(|j| matches!(j, Some(SJob { phase: Phase::Identity { retry_ns, .. }, .. }) if now_ns >= *retry_ns));
            let mut wire = RestReqBuf::default();
            let owner = if let Some(i) = identity {
                let j = self.jobs[i]?;
                let cred = self.cred(j.acct).copied();
                if cred.as_ref().is_none_or(|cred| {
                    P::session_identity_request(&self.cfg.venue, cred, now_ns / 1_000_000, &mut wire)
                        != Ok(true)
                }) {
                    if let Some(j) = &mut self.jobs[i] {
                        j.phase = Phase::IdentityDone;
                    }
                    return Some(self.finish(c, i, now_ns));
                }
                Some(RestOwner::Identity(i))
            } else if let Some(q) = self.queued.front().copied() {
                if now_ns >= q.req.deadline_ns {
                    self.queued.pop_front();
                    return Some(SessEvent::Failed {
                        conn: q.req.conn,
                        code: 0,
                    });
                }
                let credential = self
                    .rest_account
                    .as_ref()
                    .filter(|(a, _)| Some(*a) == q.acct)
                    .map(|(_, k)| k);
                match P::bootstrap(q.req.role, &self.cfg.venue, credential) {
                    Some(r) => {
                        wire = r;
                        Some(RestOwner::Bootstrap(q))
                    }
                    None => {
                        self.queued.pop_front();
                        if self.start(c, q.req, q.acct, now_ns).is_err() {
                            return Some(SessEvent::Failed {
                                conn: q.req.conn,
                                code: 0,
                            });
                        }
                        None
                    }
                }
            } else {
                None
            };
            if let Some(owner) = owner {
                let result = self.rest.as_mut().map(|r| r.submit(&wire, 1, now_tsc));
                wire.headers.clear();
                wire.path_query.clear();
                wire.body.clear();
                if !matches!(result, Some(Err(hft_net::RestErr::Busy))) {
                    match owner {
                        RestOwner::Bootstrap(_) => {
                            self.queued.pop_front();
                        }
                        RestOwner::Identity(i) => {
                            if let Some(j) = &mut self.jobs[i]
                                && let Phase::Identity { attempts, retry_ns } = j.phase
                            {
                                j.phase = Phase::Identity {
                                    attempts: attempts.saturating_add(1),
                                    retry_ns,
                                };
                            }
                        }
                    }
                    self.rest_work = Some(RestWork {
                        owner,
                        deadline_ns: if matches!(result, Some(Ok(_))) {
                            now_ns.saturating_add(i64::from(self.rest_cfg.acct_query_timeout_ms) * 1_000_000)
                        } else {
                            now_ns
                        },
                    });
                }
            }
        }
        self.drive(c, now_ns)
    }

    fn note(&mut self, op: SessionOp) {
        if self.log.len() == LOG_CAP {
            self.log.pop_front();
        }
        self.log.push_back(op);
    }

    /// The action log, oldest first.
    pub fn log(&self) -> impl Iterator<Item = SessionOp> + '_ {
        self.log.iter().copied()
    }

    fn cred(&self, a: Option<AccountId>) -> Option<&Credential> {
        let a = a?;
        self.creds.iter().find(|(x, _)| *x == a).map(|(_, c)| c)
    }

    /// Starts a session for `req` (steps 1–2): bootstrap check, `endpoint`, `Connector::submit`.
    pub fn start(
        &mut self,
        c: &mut Connector,
        req: BuildReq,
        acct: Option<AccountId>,
        now_ns: i64,
    ) -> Result<JobId, SessErr> {
        self.start_prepared(c, req, acct, now_ns, None)
    }

    /// Bootstrap body supplied by the warm REST task; never recorded in a raw journal.
    pub fn start_prepared(
        &mut self,
        c: &mut Connector,
        req: BuildReq,
        acct: Option<AccountId>,
        now_ns: i64,
        boot: Option<&[u8]>,
    ) -> Result<JobId, SessErr> {
        let Some(free) = self.jobs.iter().position(Option::is_none) else {
            return Err(SessErr::Busy);
        };
        let role = req.role;
        let cfg = self.cfg.venue;
        let cred = self.cred(acct).copied();
        if boot.is_none() && P::bootstrap(role, &cfg, cred.as_ref()).is_some() {
            return Err(SessErr::Bootstrap);
        }
        // Public market data (`Md`): the path is fixed per route and already in the connector's
        // endpoint table (BuildReq carries no route), so `P::endpoint` is not called (README §7 step 2).
        let mut target = None;
        if !matches!(role, ConnRole::Md | ConnRole::Rest) {
            let mut ep = EndpointSpec::default();
            P::endpoint(
                role,
                RouteId(0),
                &cfg,
                cred.as_ref(),
                boot,
                now_ns / 1_000_000,
                &mut ep,
            )
            .map_err(SessErr::Proto)?;
            let path = std::str::from_utf8(ep.path_query.as_slice())
                .map_err(|_| SessErr::Proto(ProtoErr::Cfg))?
                .to_owned();
            let raw =
                std::str::from_utf8(ep.headers.as_slice()).map_err(|_| SessErr::Proto(ProtoErr::Cfg))?;
            let mut headers = Vec::new();
            for line in raw.split("\r\n").filter(|l| !l.is_empty()) {
                let (k, v) = line.split_once(':').ok_or(SessErr::Proto(ProtoErr::Cfg))?;
                headers.push((k.to_owned(), v.trim().to_owned()));
            }
            target = Some((path, headers));
            self.note(SessionOp::Endpoint(req.conn));
        }
        let job = match target {
            Some((path, headers)) => c.submit_target(req, path, headers),
            None => c.submit(req),
        }
        .map_err(SessErr::Build)?;
        self.note(SessionOp::Submit(req.conn));
        if let Some(s) = self.jobs.get_mut(free) {
            *s = Some(SJob {
                job,
                req,
                acct,
                phase: Phase::Building,
                identity: None,
            });
        }
        Ok(job)
    }

    /// Bind a successful warm session-key observation to this exact build job.
    /// The caller must have authenticated the UID with the session credential, not just REST.
    pub fn verified_identity(&mut self, job: JobId, uid: [u8; 24]) -> Result<(), SessErr> {
        if !valid_uid(&uid) {
            return Err(SessErr::Proto(ProtoErr::Cfg));
        }
        let i = self.find(job).ok_or(SessErr::Proto(ProtoErr::Cfg))?;
        if let Some(j) = &mut self.jobs[i] {
            j.identity = Some(uid);
        }
        Ok(())
    }

    /// Advances the connector and the logins; at most one event per call.
    pub fn drive(&mut self, c: &mut Connector, now_ns: i64) -> Option<SessEvent> {
        if let Some(ev) = c.drive(now_ns) {
            match ev {
                BuildEvent::Ready { job, .. } => {
                    if let Some(i) = self.find(job)
                        && let Some(e) = self.logged_in_or_login(c, i, now_ns)
                    {
                        return Some(e);
                    }
                }
                BuildEvent::Failed { job, .. } => {
                    if let Some(i) = self.find(job)
                        && let Some(j) = self.jobs.get_mut(i).and_then(Option::take)
                    {
                        return Some(SessEvent::Failed {
                            conn: j.req.conn,
                            code: 0,
                        });
                    }
                }
            }
        }
        // Login answers.
        for i in 0..JOBS {
            let Some(j) = self.jobs.get(i).copied().flatten() else {
                continue;
            };
            let Phase::Login(step) = j.phase else { continue };
            if now_ns >= j.req.deadline_ns {
                return Some(self.fail(c, i, 0));
            }
            let Some(conn) = c.conn_mut(j.job) else { continue };
            if conn.phase() != ConnPhase::Open {
                return Some(self.fail(c, i, 0));
            }
            conn.poll_rx(0);
            let mut res = None;
            while let Some(f) = conn.next_frame() {
                match P::login_result(f.payload) {
                    LoginResult::NotMine => continue,
                    r => {
                        res = Some(r);
                        break;
                    }
                }
            }
            match res {
                None => {}
                Some(LoginResult::Ok) => return self.identity_or_finish(c, i, now_ns),
                Some(LoginResult::Rejected(code)) => return Some(self.fail(c, i, code)),
                Some(LoginResult::Continue) => {
                    let next = step.saturating_add(1);
                    if let Some(e) = self.send_login(c, i, next, now_ns) {
                        return Some(e);
                    }
                }
                Some(LoginResult::NotMine) => {}
            }
        }
        None
    }

    fn find(&self, job: JobId) -> Option<usize> {
        self.jobs.iter().position(|j| j.is_some_and(|j| j.job == job))
    }

    fn logged_in_or_login(&mut self, c: &mut Connector, i: usize, now_ns: i64) -> Option<SessEvent> {
        let j = self.jobs.get(i).copied().flatten()?;
        if matches!(j.req.role, ConnRole::Order | ConnRole::UserStream) && self.cred(j.acct).is_some() {
            return self.send_login(c, i, 0, now_ns);
        }
        self.identity_or_finish(c, i, now_ns)
    }

    /// Sends login frame `step`; `NotNeeded` finishes the session.
    fn send_login(&mut self, c: &mut Connector, i: usize, step: u8, now_ns: i64) -> Option<SessEvent> {
        let j = self.jobs.get(i).copied().flatten()?;
        let cred = self.cred(j.acct).copied()?;
        self.out.clear();
        let r = P::login(
            j.req.role,
            &self.cfg.venue,
            &cred,
            step,
            now_ns / 1_000_000,
            &mut self.out,
        );
        match r {
            Ok(LoginStep::NotNeeded) => self.identity_or_finish(c, i, now_ns),
            Ok(LoginStep::Frame | LoginStep::Handshake) => {
                let sent = c.conn_mut(j.job).map(|conn| conn.send_text(&self.out, 0).is_ok());
                if sent != Some(true) {
                    return Some(self.fail(c, i, 0));
                }
                self.note(SessionOp::LoginFrame(j.req.conn, step));
                if let Some(Some(s)) = self.jobs.get_mut(i) {
                    s.phase = Phase::Login(step);
                }
                None
            }
            Err(_) => Some(self.fail(c, i, 0)),
        }
    }

    fn identity_or_finish(&mut self, c: &mut Connector, i: usize, now_ns: i64) -> Option<SessEvent> {
        let j = self.jobs[i]?;
        if self.rest.is_some()
            && self.rest_account.as_ref().map(|(a, _)| *a) == j.acct
            && j.identity.is_none()
            && matches!(j.req.role, ConnRole::Order | ConnRole::UserStream)
            && let Some(cred) = self.cred(j.acct)
        {
            let mut req = RestReqBuf::default();
            if P::session_identity_request(&self.cfg.venue, cred, now_ns / 1_000_000, &mut req) == Ok(true) {
                if let Some(j) = &mut self.jobs[i] {
                    j.phase = Phase::Identity {
                        attempts: 0,
                        retry_ns: now_ns,
                    };
                }
                return None;
            }
        }
        Some(self.finish(c, i, now_ns))
    }
    /// Merge the authenticated session observation before readiness registration and hand-off.
    fn finish(&mut self, c: &mut Connector, i: usize, now_ns: i64) -> SessEvent {
        let Some(j) = self.jobs.get(i).copied().flatten() else {
            return SessEvent::Failed {
                conn: ConnId(0),
                code: 0,
            };
        };
        let mut identity = 0;
        let mut uid = [0; 24];
        if matches!(j.req.role, ConnRole::Order | ConnRole::UserStream)
            && let Some(cred) = self.cred(j.acct).copied()
        {
            let mut req = RestReqBuf::default();
            match P::session_identity_request(&self.cfg.venue, &cred, now_ns / 1_000_000, &mut req) {
                Ok(false) => {}
                Ok(true) if j.identity.is_some() => {
                    identity = 1;
                    uid = j.identity.unwrap_or([0; 24]);
                }
                _ => identity = 2,
            }
        }
        if matches!(j.req.role, ConnRole::Order | ConnRole::UserStream)
            && let Some(account) = j.acct
        {
            let ok = c.conn_mut(j.job).is_some_and(|conn| {
                crate::private::prepare::<P>(conn, &self.cfg.venue, account, identity, uid).is_ok()
            });
            if !ok {
                return self.fail(c, i, 0);
            }
        }
        let id = j.req.conn;
        if let Some(h) = self.ready_of(id) {
            let ok = c.conn_mut(j.job).map(|conn| h.add(conn).is_ok());
            if ok != Some(true) {
                return self.fail(c, i, 0);
            }
            self.note(SessionOp::ReadyAdd(id));
        }
        let fd = c.conn_mut(j.job).map_or(-1, |conn| conn.raw_fd());
        match c.handoff(j.job) {
            Ok(h) => {
                if let Some(s) = self.jobs.get_mut(i) {
                    *s = None;
                }
                self.fds.retain(|(x, _)| *x != id);
                self.fds.push((id, fd));
                self.note(SessionOp::Handoff(id));
                SessEvent::Ready(h)
            }
            Err(_) => self.fail(c, i, 0),
        }
    }

    fn ready_of(&self, id: ConnId) -> Option<ReadyHandle> {
        if id.0 < MAX_BBO_CONN_SLOTS {
            self.cfg.ready_bbo
        } else if id.0 <= MD_LAST {
            self.cfg.ready_md
        } else {
            None
        }
    }

    /// Not handed off: the connection goes back to the connector.
    fn fail(&mut self, c: &mut Connector, i: usize, code: u32) -> SessEvent {
        let Some(j) = self.jobs.get_mut(i).and_then(Option::take) else {
            return SessEvent::Failed {
                conn: ConnId(0),
                code,
            };
        };
        let id = j.req.conn;
        c.cancel(j.job);
        self.note(SessionOp::Reclaim(id));
        SessEvent::Failed { conn: id, code }
    }

    /// A task retired a connection (step 7): de-register from the readiness set, then reclaim.
    pub fn retire(&mut self, c: &mut Connector, m: RetireMsg) {
        let id = m.id();
        if let Some(h) = self.ready_of(id) {
            let fd = self.fds.iter().find(|(x, _)| *x == id).map_or(-1, |(_, f)| *f);
            let _ = h.del_fd(fd);
            self.note(SessionOp::ReadyDel(id));
        }
        self.fds.retain(|(x, _)| *x != id);
        c.reclaim(m);
        self.note(SessionOp::Reclaim(id));
    }
}
