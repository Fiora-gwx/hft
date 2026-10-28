//! The ctl-gateway warm task: Unix socket listener, one request and one response per
//! connection, `SO_PEERCRED`, journal tailing for `CmdResult`, and the `PollTask` wiring around
//! [`Core`]. Never blocks: every socket is non-blocking.

use std::io::{ErrorKind, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};

use hft_ctl_proto::{CmdResponse, encode_frame, frame_len};
use hft_event::{CmdResult, EventKind};
use hft_journal::{JournalReader, Next};
use hft_refdata::RefData;
use hft_rt::{Consumer, MailReceiver, MailSender, PollTask, Polled, UtcClock, WaitFds};
use hft_strategy_sdk::StrategySet;
use hft_types::Tsc;

use crate::config::{GatewayConfig, MIN_RING_BYTES};
use crate::core::{ConnId, Core, InternalCmd, RefdataAssemblyCheck, Reply, RiskDecode};
use crate::parsed::ParsedConfig;
use crate::ring::CtlRing;

/// Batch bounds of one `poll_once`.
const ACCEPT_BATCH: usize = 16;
const RESULT_BATCH: usize = 64;
const READ_CHUNK: usize = 64 * 1024;

/// Construction errors.
#[derive(Debug)]
pub enum GatewayError {
    /// ctl ring below 256 KiB.
    RingTooSmall(usize),
    /// Socket path unusable (bind, permissions).
    Socket(std::io::Error),
}

/// Source of `CmdResult`s (the seq journal tail in production).
pub trait ResultSource {
    /// Next `CmdResult` written since the last call; `None` = nothing new.
    fn next_result(&mut self) -> Option<CmdResult>;
}

/// The seq journal reader as a `CmdResult` source.
impl ResultSource for JournalReader {
    fn next_result(&mut self) -> Option<CmdResult> {
        loop {
            match self.poll() {
                Next::Record(r) if r.hdr.kind == EventKind::CMD_RESULT.0 => {
                    if let Ok(c) = r.read::<CmdResult>() {
                        return Some(c);
                    }
                }
                Next::Record(_) | Next::Event(_) | Next::Gap(_) => {}
                Next::Pending | Next::End | Next::Err(_) => return None,
            }
        }
    }
}

enum State {
    Reading(Vec<u8>),
    Waiting,
    Writing(Vec<u8>, usize),
}

struct Conn {
    id: ConnId,
    stream: UnixStream,
    uid: u32,
    state: State,
}

/// The gateway task.
pub struct Gateway<S: StrategySet, R: CtlRing, J: ResultSource> {
    core: Core<S, R>,
    listener: UnixListener,
    conns: Vec<Conn>,
    internal: Consumer<InternalCmd>,
    results: J,
    next_id: ConnId,
}

impl<S: StrategySet, R: CtlRing, J: ResultSource> core::fmt::Debug for Gateway<S, R, J> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Gateway")
            .field("conns", &self.conns.len())
            .finish_non_exhaustive()
    }
}

/// `SO_PEERCRED` uid of a connected socket.
pub fn peer_uid(s: &UnixStream) -> Option<u32> {
    let mut cred = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut len = size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: `cred` and `len` are valid for writes of the sizes passed; the fd is open.
    let rc = unsafe {
        libc::getsockopt(
            s.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&raw mut cred).cast(),
            &raw mut len,
        )
    };
    (rc == 0).then_some(cred.uid)
}

impl<S: StrategySet, R: CtlRing, J: ResultSource> Gateway<S, R, J> {
    /// Binds the socket (mode 0660) and builds the core. `ring_bytes` = capacity of the ctl ring
    /// behind `ctl` (≥ 256 KiB).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cfg: GatewayConfig,
        rd: RefData,
        ctl: R,
        ring_bytes: usize,
        parsed: MailSender<ParsedConfig<S>>,
        retired: MailReceiver<ParsedConfig<S>>,
        internal: Consumer<InternalCmd>,
        refdata_check: RefdataAssemblyCheck,
        risk_decode: RiskDecode,
        active_params: Vec<ParsedConfig<S>>,
        journal: J,
        clock: UtcClock,
        engine: hft_ctl_proto::EngineInfo,
    ) -> Result<Gateway<S, R, J>, GatewayError> {
        if ring_bytes < MIN_RING_BYTES {
            return Err(GatewayError::RingTooSmall(ring_bytes));
        }
        let _ = std::fs::remove_file(&cfg.socket_path);
        let listener = UnixListener::bind(&cfg.socket_path).map_err(GatewayError::Socket)?;
        listener.set_nonblocking(true).map_err(GatewayError::Socket)?;
        std::fs::set_permissions(&cfg.socket_path, std::fs::Permissions::from_mode(0o660))
            .map_err(GatewayError::Socket)?;
        let core = Core::new(
            cfg,
            rd,
            ctl,
            parsed,
            retired,
            refdata_check,
            risk_decode,
            active_params,
            clock,
            engine,
        );
        Ok(Gateway {
            core,
            listener,
            conns: Vec::new(),
            internal,
            results: journal,
            next_id: 1,
        })
    }

    pub fn core(&self) -> &Core<S, R> {
        &self.core
    }
    pub fn core_mut(&mut self) -> &mut Core<S, R> {
        &mut self.core
    }
    pub fn results_mut(&mut self) -> &mut J {
        &mut self.results
    }
    /// Open connections.
    pub fn conn_count(&self) -> usize {
        self.conns.len()
    }

    /// Adds an accepted connection with a known peer uid (the accept path after `SO_PEERCRED`;
    /// harnesses use it with socket pairs to inject credentials).
    pub fn adopt(&mut self, stream: UnixStream, uid: u32) {
        let _ = stream.set_nonblocking(true);
        let id = self.next_id;
        self.next_id += 1;
        let state = match self.core.check_uid(uid) {
            Some(r) => State::Writing(frame(&r), 0),
            None => State::Reading(Vec::new()),
        };
        self.conns.push(Conn {
            id,
            stream,
            uid,
            state,
        });
    }

    fn accept(&mut self) -> bool {
        let mut did = false;
        for _ in 0..ACCEPT_BATCH {
            match self.listener.accept() {
                Ok((s, _)) => {
                    did = true;
                    match peer_uid(&s) {
                        Some(uid) => self.adopt(s, uid),
                        None => drop(s),
                    }
                }
                Err(_) => break,
            }
        }
        did
    }

    fn read_conns(&mut self, now: Tsc) -> bool {
        let mut did = false;
        for i in 0..self.conns.len() {
            let c = &mut self.conns[i];
            let State::Reading(buf) = &mut c.state else {
                continue;
            };
            let mut chunk = vec![0u8; READ_CHUNK];
            let mut closed = false;
            match c.stream.read(&mut chunk) {
                Ok(0) => closed = true,
                Ok(n) => {
                    did = true;
                    buf.extend_from_slice(&chunk[..n]);
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => {}
                Err(_) => closed = true,
            }
            let State::Reading(buf) = &self.conns[i].state else {
                continue;
            };
            let ready = match buf.get(..4) {
                Some(h) => match frame_len(&[h[0], h[1], h[2], h[3]]) {
                    Ok(n) => (buf.len() >= 4 + n).then(|| Ok(buf[4..4 + n].to_vec())),
                    Err(_) => Some(Err(())),
                },
                None => None,
            };
            match ready {
                Some(Ok(body)) => {
                    did = true;
                    let (id, uid) = (self.conns[i].id, self.conns[i].uid);
                    self.conns[i].state = match self.core.on_request(id, uid, &body, now) {
                        Reply::Now(r) => State::Writing(frame(&r), 0),
                        Reply::Wait => State::Waiting,
                    };
                }
                Some(Err(())) => {
                    let (id, uid) = (self.conns[i].id, self.conns[i].uid);
                    // An over-long frame is a schema error; the body is never read.
                    let r = match self.core.on_request(id, uid, b"", now) {
                        Reply::Now(r) => r,
                        Reply::Wait => continue,
                    };
                    self.conns[i].state = State::Writing(frame(&r), 0);
                }
                None if closed => self.conns[i].state = State::Writing(Vec::new(), 0),
                None => {}
            }
        }
        did
    }

    fn deliver(&mut self, conn: ConnId, r: &CmdResponse) {
        if let Some(c) = self.conns.iter_mut().find(|c| c.id == conn) {
            c.state = State::Writing(frame(r), 0);
        }
    }

    fn write_conns(&mut self) -> bool {
        let mut did = false;
        let mut i = 0;
        while i < self.conns.len() {
            let c = &mut self.conns[i];
            let done = match &mut c.state {
                State::Writing(out, off) => {
                    while *off < out.len() {
                        match c.stream.write(&out[*off..]) {
                            Ok(0) => break,
                            Ok(n) => {
                                did = true;
                                *off += n;
                            }
                            Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                            Err(_) => {
                                *off = out.len();
                            }
                        }
                    }
                    *off >= out.len()
                }
                _ => false,
            };
            if done {
                let c = self.conns.remove(i);
                self.core.forget_conn(c.id);
                let _ = c.stream.shutdown(std::net::Shutdown::Both);
            } else {
                i += 1;
            }
        }
        did
    }
}

fn frame(r: &CmdResponse) -> Vec<u8> {
    let mut out = Vec::new();
    encode_frame(&r.to_json(), &mut out);
    out
}

impl<S: StrategySet, R: CtlRing, J: ResultSource> PollTask for Gateway<S, R, J> {
    fn poll_once(&mut self, now: Tsc) -> Polled {
        let mut did = self.accept();
        did |= self.read_conns(now);
        while let Some(cmd) = self.internal.try_pop() {
            did = true;
            self.core.on_internal(cmd, now);
        }
        for _ in 0..RESULT_BATCH {
            let Some(res) = self.results.next_result() else {
                break;
            };
            did = true;
            if let Some((conn, r)) = self.core.on_result(&res) {
                self.deliver(conn, &r);
            }
        }
        for (conn, r) in self.core.on_tick(now) {
            did = true;
            self.deliver(conn, &r);
        }
        did |= self.core.drain_retired() > 0;
        did |= self.write_conns();
        Polled { did_work: did }
    }

    fn next_deadline(&self) -> Option<Tsc> {
        self.core.next_deadline()
    }

    fn wait_fds(&self, out: &mut WaitFds) {
        out.push(self.listener.as_raw_fd());
        for c in &self.conns {
            out.push(c.stream.as_raw_fd());
        }
    }

    fn has_pending(&self) -> bool {
        self.conns
            .iter()
            .any(|c| matches!(c.state, State::Reading(_) | State::Writing(..)))
    }
}
