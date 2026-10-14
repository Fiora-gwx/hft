//! `Connector`: the warm-thread (`conn-builder`) build jobs — TCP connect → TLS handshake →
//! WS Upgrade (or HTTP ready) → `Ready` → hand-off; reclaim of retired connections (README §4, §5).

use std::net::Ipv4Addr;

use hft_event::ConnRole;
use hft_types::{ConnId, VenueId};

use crate::arena::{ConnArena, Handoff, RetireMsg};
use crate::config::{NetConfig, TlsMode};
use crate::conn::Conn;
use crate::sock::{SysCount, errno, opt};
use crate::tls::{self, ConfigErr, HsFail, HsStep};
use crate::types::{ConnPhase, EndpointId, TlsInfo};

/// Concurrent build jobs.
pub const MAX_JOBS: usize = 16;
/// Wait for NewSessionTicket after `Open` (README §4 step 5).
pub const TICKET_WAIT_NS: i64 = 20_000_000;
const WS_GUID: &[u8] = b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
/// Largest Upgrade response accepted (headers).
const UPGRADE_MAX: usize = 16 * 1024;

/// One build request (connection number and fields filled by hft-venue §1).
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct BuildReq {
    pub venue: VenueId,
    pub conn: ConnId,
    pub role: ConnRole,
    pub endpoint: EndpointId,
    /// Index into `src_ips[]`.
    pub src_ip_idx: u8,
    /// Target RX queue (only with `rss_plan`).
    pub rx_queue: u16,
    /// Deadline of the whole job; the earlier of it and the stage timeout applies.
    pub deadline_ns: i64,
}

/// Build-job handle (`index | generation << 8`).
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct JobId(pub u32);

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum BuildEvent {
    Ready {
        job: JobId,
        tls: TlsInfo,
        napi_id: u32,
    },
    Failed {
        job: JobId,
        stage: ConnPhase,
        why: BuildFail,
    },
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum BuildFail {
    Errno(i32),
    Timeout,
    TlsAlert(u8),
    Tls,
    HttpStatus(u16),
    BadAccept,
    Extensions,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum BuildErr {
    JobsFull,
    ArenaFull,
    BadEndpoint,
    BadSrcIp,
    NotReady,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum St {
    Init,
    Connecting,
    Tls,
    Upgrade,
    OpenWait,
    Ready,
}

#[derive(Clone, Debug)]
struct Job {
    generation: u8,
    slot: u16,
    req: BuildReq,
    st: St,
    stage_start: i64,
    open_ns: i64,
    ws_key: [u8; 24],
    info: TlsInfo,
    target: Option<(String, Vec<(String, String)>)>,
}

/// The builder (one per arena; warm thread).
pub struct Connector {
    arena: &'static ConnArena,
    cfg: NetConfig,
    tls: rustls::ClientConfig,
    jobs: [Option<Job>; MAX_JOBS],
    gens: [u8; MAX_JOBS],
    slot_busy: Vec<bool>,
    rr: usize,
    napi_cache: [(u32, u32); 16],
    napi_n: usize,
    sys: SysCount,
    retries: Vec<(u32, i64)>,
    now_ns: i64,
    #[cfg(feature = "test-io")]
    scripts: std::collections::VecDeque<crate::sock::ScriptIo>,
    #[cfg(feature = "test-io")]
    fixed_key: Option<[u8; 16]>,
}

impl std::fmt::Debug for Connector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connector")
            .field("arena", &self.arena)
            .field("jobs", &self.jobs.iter().filter(|j| j.is_some()).count())
            .finish()
    }
}

fn b64(input: &[u8], out: &mut [u8]) -> usize {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut w = 0;
    for c in input.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        let chars = [
            T[(n >> 18) as usize & 63],
            T[(n >> 12) as usize & 63],
            if c.len() > 1 {
                T[(n >> 6) as usize & 63]
            } else {
                b'='
            },
            if c.len() > 2 { T[n as usize & 63] } else { b'=' },
        ];
        if let Some(d) = out.get_mut(w..w + 4) {
            d.copy_from_slice(&chars);
        }
        w += 4;
    }
    w
}

/// `Sec-WebSocket-Accept` for `key` (RFC 6455 §4.2.2).
pub fn ws_accept(key: &[u8]) -> [u8; 28] {
    let mut ctx = aws_lc_rs::digest::Context::new(&aws_lc_rs::digest::SHA1_FOR_LEGACY_USE_ONLY);
    ctx.update(key);
    ctx.update(WS_GUID);
    let d = ctx.finish();
    let mut out = [0u8; 28];
    b64(d.as_ref(), &mut out);
    out
}

fn random_seed() -> u64 {
    let mut b = [0u8; 8];
    let _ = aws_lc_rs::rand::fill(&mut b);
    let v = u64::from_ne_bytes(b);
    if v == 0 { 0x9E37_79B9_7F4A_7C15 } else { v }
}

impl Connector {
    /// Startup: validates the configuration and builds the TLS client configuration (reads
    /// `extra_ca` when `endpoint_override` is set). Optional extensions that are not built
    /// (`tls_mode = buffered`, scratch warm-up, TX timestamps, `rss_plan`) are refused.
    pub fn new(arena: &'static ConnArena, cfg: &NetConfig) -> Result<Connector, ConfigErr> {
        if cfg.tls_mode == TlsMode::Buffered {
            return Err(ConfigErr::BufferedTls);
        }
        if cfg.warm_interval_ns > 0 {
            return Err(ConfigErr::WarmUp);
        }
        if cfg.tx_timestamps {
            return Err(ConfigErr::TxTimestamps);
        }
        if cfg.rss_plan.is_some() {
            return Err(ConfigErr::RssPlan);
        }
        let ca = match (&cfg.extra_ca, cfg.endpoint_override) {
            (Some(p), true) => Some(std::fs::read(p).map_err(|_| ConfigErr::ExtraCa)?),
            _ => None,
        };
        let tls = tls::client_config(ca.as_deref())?;
        if cfg.reconnect_backoff_ms.0 == 0 || cfg.reconnect_backoff_ms.0 > cfg.reconnect_backoff_ms.1 {
            return Err(ConfigErr::Backoff);
        }
        Ok(Connector {
            arena,
            cfg: cfg.clone(),
            tls,
            jobs: std::array::from_fn(|_| None),
            gens: [0; MAX_JOBS],
            slot_busy: vec![false; arena.len()],
            rr: 0,
            napi_cache: [(0, 0); 16],
            napi_n: 0,
            sys: SysCount::default(),
            retries: vec![(0, 0); cfg.endpoints.len() * cfg.src_ips.len()],
            now_ns: 0,
            #[cfg(feature = "test-io")]
            scripts: std::collections::VecDeque::new(),
            #[cfg(feature = "test-io")]
            fixed_key: None,
        })
    }

    fn job_index(&self, job: JobId) -> Option<usize> {
        let i = (job.0 & 0xFF) as usize;
        let g = (job.0 >> 8) as u8;
        match self.jobs.get(i) {
            Some(Some(j)) if j.generation == g => Some(i),
            _ => None,
        }
    }

    #[allow(clippy::mut_from_ref)]
    fn conn(&self, slot: u16) -> Option<&mut Conn> {
        // SAFETY: slots marked busy by this Connector and not handed off are owned by it (the
        // Connector is the arena's only builder; module docs of `arena`). Callers never hold two
        // borrows of one slot.
        unsafe { self.arena.conn_mut(slot) }
    }

    /// Validates and queues a job. Jobs full → `JobsFull`; no free slot of the role → `ArenaFull`;
    /// unknown endpoint → `BadEndpoint`; `src_ip_idx` out of range or its address missing from
    /// `bind_device` → `BadSrcIp`.
    pub fn submit(&mut self, req: BuildReq) -> Result<JobId, BuildErr> {
        let Some(ji) = self.jobs.iter().position(|j| j.is_none()) else {
            return Err(BuildErr::JobsFull);
        };
        if req.endpoint.0 as usize >= self.cfg.endpoints.len() {
            return Err(BuildErr::BadEndpoint);
        }
        let Some(ip) = self.cfg.src_ips.get(req.src_ip_idx as usize).copied() else {
            return Err(BuildErr::BadSrcIp);
        };
        if !self.cfg.bind_device.iter().any(|(a, _)| *a == ip) {
            return Err(BuildErr::BadSrcIp);
        }
        let Some(slot) = (0..self.arena.len()).find(|&s| {
            !self.slot_busy.get(s).copied().unwrap_or(true) && self.arena.role(s as u16) == Some(req.role)
        }) else {
            return Err(BuildErr::ArenaFull);
        };
        if let Some(b) = self.slot_busy.get_mut(slot) {
            *b = true;
        }
        if let Some(c) = self.conn(slot as u16) {
            c.phase = ConnPhase::TcpConnecting;
        }
        let generation = self.gens[ji].wrapping_add(1);
        self.gens[ji] = generation;
        self.jobs[ji] = Some(Job {
            generation,
            slot: slot as u16,
            req,
            st: St::Init,
            stage_start: 0,
            open_ns: 0,
            ws_key: [0; 24],
            info: TlsInfo::default(),
            target: None,
        });
        Ok(JobId(ji as u32 | (generation as u32) << 8))
    }

    /// Warm-only per-job Upgrade target. Credentials never enter the shared endpoint table.
    pub fn submit_target(
        &mut self,
        req: BuildReq,
        path: String,
        headers: Vec<(String, String)>,
    ) -> Result<JobId, BuildErr> {
        if !path.starts_with('/')
            || path.bytes().any(|b| b <= 32 || b == 127)
            || headers
                .iter()
                .any(|(k, v)| k.is_empty() || k.contains(['\r', '\n', ':']) || v.contains(['\r', '\n']))
        {
            return Err(BuildErr::BadEndpoint);
        }
        let id = self.submit(req)?;
        if let Some(i) = self.job_index(id)
            && let Some(j) = self.jobs[i].as_mut()
        {
            j.target = Some((path, headers));
        }
        Ok(id)
    }

    /// Cancel a warm job, including a ready HTTP connection, without handing it to another thread.
    pub fn cancel(&mut self, id: JobId) {
        if let Some(i) = self.job_index(id) {
            let _ = self.fail(i, ConnPhase::Dead, BuildFail::Timeout);
        }
    }

    /// Advances every job one step (non-blocking); returns at most one event.
    pub fn drive(&mut self, now_ns: i64) -> Option<BuildEvent> {
        self.now_ns = now_ns;
        for k in 0..MAX_JOBS {
            let i = (self.rr + k) % MAX_JOBS;
            if let Some(ev) = self.step(i, now_ns) {
                self.rr = (i + 1) % MAX_JOBS;
                return Some(ev);
            }
        }
        None
    }

    /// The phase of a job's connection (`None` for an unknown or finished job).
    pub fn phase(&self, job: JobId) -> Option<ConnPhase> {
        let i = self.job_index(job)?;
        let slot = self.jobs[i].as_ref()?.slot;
        self.conn(slot).map(|c| c.phase())
    }

    /// The `Open` connection of a job (login and subscription on the warm thread); otherwise `None`.
    pub fn conn_mut(&mut self, job: JobId) -> Option<&mut Conn> {
        let i = self.job_index(job)?;
        let j = self.jobs[i].as_ref()?;
        if j.st != St::Ready {
            return None;
        }
        self.conn(j.slot)
    }

    /// Hands the `Open` connection over (the slot number travels through an SPSC ring).
    pub fn handoff(&mut self, job: JobId) -> Result<Handoff, BuildErr> {
        let i = self.job_index(job).ok_or(BuildErr::NotReady)?;
        let (slot, id, role) = match &self.jobs[i] {
            Some(j) if j.st == St::Ready => (j.slot, j.req.conn, j.req.role),
            _ => return Err(BuildErr::NotReady),
        };
        if let Some(j) = &self.jobs[i] {
            let k = usize::from(j.req.endpoint.0) * self.cfg.src_ips.len() + usize::from(j.req.src_ip_idx);
            if j.req.role != ConnRole::Rest
                && let Some(r) = self.retries.get_mut(k)
            {
                *r = (0, 0);
            }
        }
        self.jobs[i] = None;
        Ok(Handoff {
            arena: self.arena,
            slot,
            id,
            role,
        })
    }

    /// NAPI id → interface index (netdev netlink, kernel ≥ 6.9; cached). `napi_id = 0` → `None`
    /// without a system call.
    pub fn napi_ifindex(&mut self, napi_id: u32) -> Option<u32> {
        if napi_id == 0 {
            return None;
        }
        if let Some((_, idx)) = self
            .napi_cache
            .iter()
            .take(self.napi_n)
            .find(|(n, _)| *n == napi_id)
        {
            return Some(*idx);
        }
        let idx = crate::sock::napi_ifindex(napi_id)?;
        if self.napi_n < self.napi_cache.len() {
            self.napi_cache[self.napi_n] = (napi_id, idx);
            self.napi_n += 1;
        }
        Some(idx)
    }

    /// `shutdown(SHUT_RDWR)` + `close`; the slot is free again.
    pub fn reclaim(&mut self, m: RetireMsg) {
        let slot = m.h.slot;
        if let Some(c) = self.conn(slot) {
            c.sock.shutdown_close();
            c.phase = ConnPhase::Free;
        }
        if let Some(b) = self.slot_busy.get_mut(slot as usize) {
            *b = false;
        }
    }

    /// System calls made by the Connector itself (resolver); per-socket calls are in `Conn::sys`.
    pub fn sys(&self) -> &SysCount {
        &self.sys
    }

    // ------------------------------------------------------------------ job steps

    fn fail(&mut self, i: usize, stage: ConnPhase, why: BuildFail) -> Option<BuildEvent> {
        let j = self.jobs[i].take()?;
        if j.req.role != ConnRole::Rest {
            let k = usize::from(j.req.endpoint.0) * self.cfg.src_ips.len() + usize::from(j.req.src_ip_idx);
            if let Some(r) = self.retries.get_mut(k) {
                r.0 = r.0.saturating_add(1);
                let (lo, hi) = self.cfg.reconnect_backoff_ms;
                let delay = u64::from(lo)
                    .saturating_mul(1u64 << r.0.saturating_sub(1).min(32))
                    .min(u64::from(hi));
                r.1 = self.now_ns.saturating_add(delay as i64 * 1_000_000);
            }
        }
        if let Some(c) = self.conn(j.slot) {
            c.sock.shutdown_close();
            c.phase = ConnPhase::Free;
        }
        if let Some(b) = self.slot_busy.get_mut(j.slot as usize) {
            *b = false;
        }
        Some(BuildEvent::Failed {
            job: JobId(i as u32 | (j.generation as u32) << 8),
            stage,
            why,
        })
    }

    fn timeout_ns(&self, st: St) -> i64 {
        let ms = match st {
            St::Init | St::Connecting => self.cfg.connect_timeout_ms,
            St::Tls => self.cfg.tls_timeout_ms,
            St::Upgrade => self.cfg.upgrade_timeout_ms,
            St::OpenWait | St::Ready => return i64::MAX,
        };
        ms as i64 * 1_000_000
    }

    fn stage_phase(st: St) -> ConnPhase {
        match st {
            St::Init | St::Connecting => ConnPhase::TcpConnecting,
            St::Tls => ConnPhase::TlsHandshake,
            St::Upgrade => ConnPhase::WsUpgrade,
            St::OpenWait | St::Ready => ConnPhase::Open,
        }
    }

    fn enter(&mut self, i: usize, st: St, now: i64) {
        let Some(j) = self.jobs[i].as_mut() else { return };
        j.st = st;
        j.stage_start = now;
        let p = Self::stage_phase(st);
        let slot = j.slot;
        if let Some(c) = self.conn(slot) {
            c.phase = p;
            c.sock.phase = p as u8;
        }
    }

    fn step(&mut self, i: usize, now: i64) -> Option<BuildEvent> {
        let (st, slot, req) = {
            let j = self.jobs[i].as_ref()?;
            (j.st, j.slot, j.req)
        };
        if st == St::Ready {
            return None;
        }
        let ev = match st {
            St::Init => self.step_init(i, slot, req, now),
            St::Connecting => self.step_connecting(i, slot, now),
            St::Tls => self.step_tls(i, slot, req, now),
            St::Upgrade => self.step_upgrade(i, slot, now),
            St::OpenWait => self.step_open_wait(i, slot, now),
            St::Ready => None,
        };
        if ev.is_some() {
            return ev;
        }
        let j = self.jobs[i].as_ref()?;
        if matches!(j.st, St::Connecting | St::Tls | St::Upgrade)
            && (now.saturating_sub(j.stage_start) >= self.timeout_ns(j.st) || now >= j.req.deadline_ns)
        {
            let stage = Self::stage_phase(j.st);
            return self.fail(i, stage, BuildFail::Timeout);
        }
        None
    }

    fn resolve(&mut self, ep: usize) -> Option<(Ipv4Addr, u16)> {
        let e = self.cfg.endpoints.get(ep)?;
        if let Some(ip) = e.ips.first() {
            return Some((*ip, e.port));
        }
        self.sys.getaddrinfo += 1;
        use std::net::ToSocketAddrs;
        let addrs = (e.host.as_str(), e.port).to_socket_addrs().ok()?;
        for a in addrs {
            if let std::net::SocketAddr::V4(v4) = a {
                return Some((*v4.ip(), v4.port()));
            }
        }
        None
    }

    fn step_init(&mut self, i: usize, slot: u16, req: BuildReq, now: i64) -> Option<BuildEvent> {
        if now >= req.deadline_ns {
            return self.fail(i, ConnPhase::TcpConnecting, BuildFail::Timeout);
        }
        let k = usize::from(req.endpoint.0) * self.cfg.src_ips.len() + usize::from(req.src_ip_idx);
        if req.role != ConnRole::Rest && self.retries.get(k).is_some_and(|r| now < r.1) {
            return None;
        }
        let seed = random_seed();
        #[cfg(feature = "test-io")]
        let script = self.scripts.pop_front();
        let src = self.cfg.src_ips.get(req.src_ip_idx as usize).copied()?;
        let dev = self
            .cfg
            .bind_device
            .iter()
            .find(|(a, _)| *a == src)
            .map(|(_, d)| d.clone())?;
        let target = self.resolve(req.endpoint.0 as usize);
        let cfg = self.cfg.clone();
        let c = self.conn(slot)?;
        c.reset(req.conn, req.venue, seed);
        c.sock.phase = ConnPhase::TcpConnecting as u8;
        #[cfg(feature = "test-io")]
        {
            c.sock.script = script.map(Box::new);
        }
        let Some((ip, port)) = target else {
            return self.fail(i, ConnPhase::TcpConnecting, BuildFail::Errno(errno::ENOENT));
        };
        let r = (|| -> Result<bool, i32> {
            c.sock.open_tcp()?;
            c.sock.setsockopt_i32(opt::IPPROTO_TCP, opt::TCP_NODELAY, 1)?;
            c.sock.setsockopt_i32(
                opt::IPPROTO_TCP,
                opt::TCP_USER_TIMEOUT,
                cfg.tcp_user_timeout_ms as i32,
            )?;
            c.sock.setsockopt_i32(opt::SOL_SOCKET, opt::SO_BUSY_POLL, 0)?;
            c.sock
                .setsockopt_i32(opt::SOL_SOCKET, opt::SO_TIMESTAMPING, opt::TS_RX_FLAGS)?;
            c.sock.setsockopt_i32(opt::SOL_SOCKET, opt::SO_REUSEADDR, 1)?;
            c.sock
                .setsockopt_i32(opt::SOL_SOCKET, opt::SO_RCVBUF, cfg.so_rcvbuf as i32)?;
            c.sock
                .setsockopt_i32(opt::SOL_SOCKET, opt::SO_SNDBUF, cfg.so_sndbuf as i32)?;
            c.sock
                .setsockopt_bytes(opt::SOL_SOCKET, opt::SO_BINDTODEVICE, dev.as_bytes())?;
            c.sock.bind(src, 0)?;
            c.sock.connect(ip, port)
        })();
        match r {
            Ok(true) => self.begin_tls(i, slot, req, now),
            Ok(false) => {
                self.enter(i, St::Connecting, now);
                None
            }
            Err(e) => self.fail(i, ConnPhase::TcpConnecting, BuildFail::Errno(e)),
        }
    }

    fn step_connecting(&mut self, i: usize, slot: u16, now: i64) -> Option<BuildEvent> {
        let req = self.jobs[i].as_ref()?.req;
        let c = self.conn(slot)?;
        match c.sock.writable() {
            Ok(false) => None,
            Ok(true) => match c.sock.getsockopt_i32(opt::SOL_SOCKET, 4) {
                Ok(0) => self.begin_tls(i, slot, req, now),
                Ok(e) | Err(e) => self.fail(i, ConnPhase::TcpConnecting, BuildFail::Errno(e)),
            },
            Err(e) => self.fail(i, ConnPhase::TcpConnecting, BuildFail::Errno(e)),
        }
    }

    fn begin_tls(&mut self, i: usize, slot: u16, req: BuildReq, now: i64) -> Option<BuildEvent> {
        let sni = self.cfg.endpoints.get(req.endpoint.0 as usize)?.sni.clone();
        let t = tls::start(&self.tls, &sni);
        self.enter(i, St::Tls, now);
        let Some(t) = t else {
            return self.fail(i, ConnPhase::TlsHandshake, BuildFail::Tls);
        };
        self.conn(slot)?.tls = t;
        self.step_tls(i, slot, req, now)
    }

    fn step_tls(&mut self, i: usize, slot: u16, req: BuildReq, now: i64) -> Option<BuildEvent> {
        let c = self.conn(slot)?;
        c.recv_once();
        let step = c.handshake_io();
        let dead = c.phase == ConnPhase::Dead;
        let eno = c.last_errno();
        match step {
            HsStep::Done(info) => {
                if let Some(j) = self.jobs[i].as_mut() {
                    j.info = info;
                }
                let next = if req.role == ConnRole::Rest {
                    St::OpenWait
                } else {
                    St::Upgrade
                };
                self.enter(i, next, now);
                let c = self.conn(slot)?;
                let napi = c
                    .sock
                    .getsockopt_i32(opt::SOL_SOCKET, opt::SO_INCOMING_NAPI_ID)
                    .unwrap_or(0);
                let mss = c
                    .sock
                    .getsockopt_i32(opt::IPPROTO_TCP, opt::TCP_MAXSEG)
                    .unwrap_or(0);
                c.napi_id = napi.max(0) as u32;
                if mss > 0 {
                    c.mss = mss as u32;
                }
                if next == St::OpenWait {
                    if let Some(j) = self.jobs[i].as_mut() {
                        j.open_ns = now;
                    }
                    self.conn(slot)?.opened_tsc = hft_rt::tsc_now().0;
                    return self.step_open_wait(i, slot, now);
                }
                self.send_upgrade(i, slot, now)
            }
            HsStep::Failed(HsFail::Alert(d)) => self.fail(i, ConnPhase::TlsHandshake, BuildFail::TlsAlert(d)),
            HsStep::Failed(HsFail::Tls) => {
                let why = if eno != 0 {
                    BuildFail::Errno(eno)
                } else {
                    BuildFail::Tls
                };
                self.fail(i, ConnPhase::TlsHandshake, why)
            }
            HsStep::NeedRead if dead => {
                let why = if eno != 0 {
                    BuildFail::Errno(eno)
                } else {
                    BuildFail::Tls
                };
                self.fail(i, ConnPhase::TlsHandshake, why)
            }
            HsStep::NeedRead => None,
        }
    }

    fn ws_key_raw(&mut self) -> [u8; 16] {
        #[cfg(feature = "test-io")]
        if let Some(k) = self.fixed_key {
            return k;
        }
        let mut k = [0u8; 16];
        let _ = aws_lc_rs::rand::fill(&mut k);
        k
    }

    fn send_upgrade(&mut self, i: usize, slot: u16, now: i64) -> Option<BuildEvent> {
        let raw = self.ws_key_raw();
        let mut key = [0u8; 24];
        b64(&raw, &mut key);
        let mut ep = self
            .cfg
            .endpoints
            .get(self.jobs[i].as_ref()?.req.endpoint.0 as usize)?
            .clone();
        if let Some((path, headers)) = self.jobs[i].as_ref()?.target.as_ref() {
            ep.path.clone_from(path);
            ep.upgrade_headers.clone_from(headers);
        }
        if let Some(j) = self.jobs[i].as_mut() {
            j.ws_key = key;
        }
        let mut r: Vec<u8> = Vec::with_capacity(256);
        r.extend_from_slice(b"GET ");
        r.extend_from_slice(if ep.path.is_empty() {
            b"/"
        } else {
            ep.path.as_bytes()
        });
        r.extend_from_slice(b" HTTP/1.1\r\nHost: ");
        r.extend_from_slice(ep.host.as_bytes());
        r.extend_from_slice(b"\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: ");
        r.extend_from_slice(&key);
        r.extend_from_slice(b"\r\nSec-WebSocket-Version: 13\r\n");
        for (k, v) in &ep.upgrade_headers {
            r.extend_from_slice(k.as_bytes());
            r.extend_from_slice(b": ");
            r.extend_from_slice(v.as_bytes());
            r.extend_from_slice(b"\r\n");
        }
        r.extend_from_slice(b"\r\n");
        let c = self.conn(slot)?;
        if c.send_plain(&r).is_err() {
            let e = c.last_errno();
            return self.fail(
                i,
                ConnPhase::WsUpgrade,
                if e != 0 {
                    BuildFail::Errno(e)
                } else {
                    BuildFail::Tls
                },
            );
        }
        self.step_upgrade(i, slot, now)
    }

    fn step_upgrade(&mut self, i: usize, slot: u16, now: i64) -> Option<BuildEvent> {
        let key = self.jobs[i].as_ref()?.ws_key;
        let c = self.conn(slot)?;
        c.recv_once();
        // Bounded by the buffered ciphertext: each iteration opens one record or stops.
        loop {
            let mut hdrs = [httparse::EMPTY_HEADER; 32];
            let mut resp = httparse::Response::new(&mut hdrs);
            match resp.parse(c.plaintext()) {
                Ok(httparse::Status::Complete(n)) => {
                    let code = resp.code.unwrap_or(0);
                    let ext = resp
                        .headers
                        .iter()
                        .any(|h| h.name.eq_ignore_ascii_case("Sec-WebSocket-Extensions"));
                    let accept = resp
                        .headers
                        .iter()
                        .find(|h| h.name.eq_ignore_ascii_case("Sec-WebSocket-Accept"))
                        .map(|h| h.value.trim_ascii());
                    let want = ws_accept(&key);
                    let why = if code != 101 {
                        Some(BuildFail::HttpStatus(code))
                    } else if ext {
                        Some(BuildFail::Extensions)
                    } else if accept != Some(&want[..]) {
                        Some(BuildFail::BadAccept)
                    } else {
                        None
                    };
                    if let Some(w) = why {
                        return self.fail(i, ConnPhase::WsUpgrade, w);
                    }
                    c.consume_plain(n);
                    c.opened_tsc = hft_rt::tsc_now().0;
                    if let Some(j) = self.jobs[i].as_mut() {
                        j.open_ns = now;
                    }
                    self.enter(i, St::OpenWait, now);
                    return self.step_open_wait(i, slot, now);
                }
                Ok(httparse::Status::Partial) => {
                    if c.plaintext().len() > UPGRADE_MAX {
                        return self.fail(i, ConnPhase::WsUpgrade, BuildFail::HttpStatus(0));
                    }
                }
                Err(_) => return self.fail(i, ConnPhase::WsUpgrade, BuildFail::HttpStatus(0)),
            }
            match c.open_one() {
                Some(true) => continue,
                Some(false) => break,
                None => {
                    return self.fail(i, ConnPhase::WsUpgrade, BuildFail::Tls);
                }
            }
        }
        if c.phase == ConnPhase::Dead {
            let e = c.last_errno();
            return self.fail(
                i,
                ConnPhase::WsUpgrade,
                if e != 0 {
                    BuildFail::Errno(e)
                } else {
                    BuildFail::Tls
                },
            );
        }
        None
    }

    fn step_open_wait(&mut self, i: usize, slot: u16, now: i64) -> Option<BuildEvent> {
        let (open_ns, info) = {
            let j = self.jobs[i].as_ref()?;
            (j.open_ns, j.info)
        };
        let c = self.conn(slot)?;
        if !c.has_plain() {
            c.recv_once();
            // Post-handshake records (tickets) until application data shows up.
            for _ in 0..64 {
                let before = c.has_plain();
                match c.open_one() {
                    Some(true) if !before && !c.has_plain() => continue,
                    _ => break,
                }
            }
        }
        if c.phase == ConnPhase::Dead {
            let e = c.last_errno();
            return self.fail(
                i,
                ConnPhase::Open,
                if e != 0 {
                    BuildFail::Errno(e)
                } else {
                    BuildFail::Tls
                },
            );
        }
        if c.tickets() == 0 && now < open_ns.saturating_add(TICKET_WAIT_NS) {
            return None;
        }
        let napi_id = c.napi_id;
        if let Some(j) = self.jobs[i].as_mut() {
            j.st = St::Ready;
        }
        Some(BuildEvent::Ready {
            job: JobId(i as u32 | (self.jobs[i].as_ref()?.generation as u32) << 8),
            tls: info,
            napi_id,
        })
    }
}

#[cfg(feature = "test-io")]
impl Connector {
    /// The next sockets created use these scripts, in order (instead of real sockets).
    #[doc(hidden)]
    pub fn push_script(&mut self, s: crate::sock::ScriptIo) {
        self.scripts.push_back(s);
    }

    /// The script of a job's socket.
    #[doc(hidden)]
    pub fn script(&mut self, job: JobId) -> Option<&mut crate::sock::ScriptIo> {
        let i = self.job_index(job)?;
        let slot = self.jobs[i].as_ref()?.slot;
        self.conn(slot)?.sock.script.as_deref_mut()
    }

    /// The connection of a job in any phase (tests).
    #[doc(hidden)]
    pub fn test_conn(&mut self, job: JobId) -> Option<&mut Conn> {
        let i = self.job_index(job)?;
        let slot = self.jobs[i].as_ref()?.slot;
        self.conn(slot)
    }

    /// Fixes the raw 16 bytes of `Sec-WebSocket-Key`.
    #[doc(hidden)]
    pub fn test_fix_ws_key(&mut self, k: Option<[u8; 16]>) {
        self.fixed_key = k;
    }

    /// Free slots of the arena.
    #[doc(hidden)]
    pub fn test_free_slots(&self) -> usize {
        self.slot_busy.iter().filter(|b| !**b).count()
    }
}

/// Base64 of `input` (tests and tools).
pub fn base64(input: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; input.len().div_ceil(3) * 4];
    let n = b64(input, &mut out);
    out.truncate(n);
    out
}
