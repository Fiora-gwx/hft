//! `SessionDriver<P>` on the warm `conn-builder` thread (README §7): endpoint, build, login (one or
//! more frames until `LoginResult::Ok`), readiness registration of the requesting feed task, hand-off;
//! on retirement de-registration before `Connector::reclaim`. Login frames and credentials stay on
//! this thread. M1.5 limits (README "已知限制与后续工作"): the bootstrap REST call, the session
//! identity request and a dynamic Upgrade target need the hft-net REST client / per-job targets
//! (M3) — `bootstrap = Some` is refused, `session_identity_request` is asked but not executed, the
//! `endpoint` output must equal the static endpoint configuration.

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
}

#[derive(Copy, Clone, Debug)]
struct SJob {
    job: JobId,
    req: BuildReq,
    acct: Option<AccountId>,
    phase: Phase,
}

const JOBS: usize = hft_net::MAX_JOBS;
const LOG_CAP: usize = 1024;

pub struct SessionDriver<P: VenueProtocol> {
    cfg: SessionCfg,
    creds: Vec<(AccountId, Credential)>,
    jobs: [Option<SJob>; JOBS],
    fds: Vec<(ConnId, i32)>,
    out: Vec<u8>,
    log: VecDeque<SessionOp>,
    _p: core::marker::PhantomData<P>,
}

impl<P: VenueProtocol> core::fmt::Debug for SessionDriver<P> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SessionDriver").field("venue", &P::VENUE).finish()
    }
}

impl<P: VenueProtocol> SessionDriver<P> {
    /// `creds` = the session credential of each account (Order/UserStream roles).
    pub fn new(cfg: SessionCfg, creds: Vec<(AccountId, Credential)>) -> Self {
        SessionDriver {
            cfg,
            creds,
            jobs: [None; JOBS],
            fds: Vec::with_capacity(256),
            out: Vec::with_capacity(4096),
            log: VecDeque::with_capacity(LOG_CAP),
            _p: core::marker::PhantomData,
        }
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
        if P::bootstrap(role, &cfg, cred.as_ref()).is_some() && boot.is_none() {
            return Err(SessErr::Bootstrap);
        }
        // Public market data (`Md`): the path is fixed per route and already in the connector's
        // endpoint table (BuildReq carries no route), so `P::endpoint` is not called (README §7 step 2).
        let mut target = None;
        if role != ConnRole::Md {
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
            });
        }
        Ok(job)
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
                Some(LoginResult::Ok) => return Some(self.finish(c, i, now_ns)),
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
        Some(self.finish(c, i, now_ns))
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
            Ok(LoginStep::NotNeeded) => Some(self.finish(c, i, now_ns)),
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

    /// Identity request (asked; executed from M3), readiness registration, hand-off.
    fn finish(&mut self, c: &mut Connector, i: usize, now_ns: i64) -> SessEvent {
        let Some(j) = self.jobs.get(i).copied().flatten() else {
            return SessEvent::Failed {
                conn: ConnId(0),
                code: 0,
            };
        };
        if j.req.role == ConnRole::Order
            && let Some(cred) = self.cred(j.acct).copied()
        {
            let mut req = RestReqBuf::default();
            let _ = P::session_identity_request(&self.cfg.venue, &cred, now_ns / 1_000_000, &mut req);
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
