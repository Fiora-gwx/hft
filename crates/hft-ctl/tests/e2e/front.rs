//! The gateway side of the E2E rig. Linux: the real `Gateway` task (socket, connection state
//! machine, `ResultSource`) and one `CtlClient` per issuer; the client connects to the rig's
//! listener, which hands each connection to the gateway with `Gateway::adopt` and the issuer's G0
//! uid (the credential injection the README gives harnesses; production reads `SO_PEERCRED`) and
//! relays the bytes. Elsewhere: `Core` called with the bytes the client would send (the client and
//! the socket task are Linux-only).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use hft_ctl::{ParsedConfig, Reply};
use hft_ctl_proto::{CmdRequest, CmdResponse, EngineInfo, Uuid, parse_request};
use hft_event::CmdResult;
use hft_refdata::RefData;
use hft_rt::{MailReceiver, MailSender};
use hft_types::Tsc;
use serde_json::Value;

use super::io::{CTL_RING_BYTES, CtlQ, RigRing};
use super::world::Set;
use crate::common::{clock, config, new_id, refdata_check, uid_of};

/// One client call (the `CtlClient` methods).
#[derive(Clone, Debug)]
pub enum Call {
    /// `send` of a request (JSON value of the G0 conventions).
    Send(Value),
    /// `preview` of a request.
    Preview(Value),
    /// `confirm` of a request with the preview's `ctl_state_version`.
    Confirm(Value, Option<u64>),
    /// `result(cmd_id)` from the client of `issuer`.
    Result(String, Uuid),
}

fn parse(v: &Value) -> CmdRequest {
    let b = serde_json::to_vec(v).unwrap_or_default();
    parse_request(&b).unwrap_or_else(|e| panic!("request does not parse: {e:?}"))
}

/// The request a `CtlClient` method puts on the wire (non-Linux path; Linux uses the client).
fn client_request(c: &Call) -> Option<CmdRequest> {
    Some(match c {
        Call::Send(v) => parse(v),
        Call::Preview(v) => {
            let mut r = parse(v);
            r.preview = true;
            r.confirmed = false;
            r.expect_state_version = None;
            r
        }
        Call::Confirm(v, sv) => {
            let mut r = parse(v);
            r.cmd_id = new_id();
            r.preview = false;
            r.confirmed = true;
            r.expect_state_version = *sv;
            r
        }
        Call::Result(..) => return None,
    })
}

fn gateway_parts(
    rd: &RefData,
    tsc: &Rc<Cell<u64>>,
    ctl: &Rc<RefCell<CtlQ>>,
) -> (RigRing, hft_ctl::GatewayConfig) {
    let ring = RigRing {
        q: ctl.clone(),
        tsc: tsc.clone(),
    };
    let _ = rd;
    (ring, config(std::env::temp_dir().join("ctl-e2e-unused.sock")))
}

#[cfg(not(target_os = "linux"))]
pub use core_front::{Front, Ticket};

#[cfg(target_os = "linux")]
pub use sock_front::{Front, Ticket};

#[cfg(not(target_os = "linux"))]
mod core_front {
    use std::collections::HashMap;

    use hft_ctl::Core;

    use super::*;

    /// `Core` called directly.
    pub struct Front {
        pub core: Core<Set, RigRing>,
        next_conn: u64,
        delivered: HashMap<u64, CmdResponse>,
        sent: HashMap<Uuid, Vec<u8>>,
    }

    /// A call in progress.
    #[derive(Debug)]
    pub struct Ticket {
        conn: u64,
        now: Option<CmdResponse>,
    }

    impl Front {
        pub fn new(
            rd: RefData,
            tsc: Rc<Cell<u64>>,
            ctl: Rc<RefCell<CtlQ>>,
            parsed: MailSender<ParsedConfig<Set>>,
            retired: MailReceiver<ParsedConfig<Set>>,
            active: Vec<ParsedConfig<Set>>,
            prev: Option<Front>,
        ) -> Front {
            let (ring, cfg) = gateway_parts(&rd, &tsc, &ctl);
            // The clients outlive a gateway restart (their sent requests for `result()`).
            let sent = prev.map(|p| p.sent).unwrap_or_default();
            let core = Core::new(
                cfg,
                rd,
                ring,
                parsed,
                retired,
                refdata_check,
                super::super::world::risk_decode,
                active,
                clock(),
                EngineInfo::default(),
            );
            let _ = CTL_RING_BYTES;
            Front {
                core,
                next_conn: 1,
                delivered: HashMap::new(),
                sent,
            }
        }

        pub fn core_mut(&mut self) -> &mut Core<Set, RigRing> {
            &mut self.core
        }

        pub fn poll(&mut self, now: Tsc) -> bool {
            let mut did = false;
            for (c, r) in self.core.on_tick(now) {
                self.delivered.insert(c, r);
                did = true;
            }
            did |= self.core.drain_retired() > 0;
            did
        }

        pub fn result(&mut self, r: CmdResult) {
            if let Some((c, resp)) = self.core.on_result(&r) {
                self.delivered.insert(c, resp);
            }
        }

        pub fn start(&mut self, c: Call, now: Tsc) -> Ticket {
            let (issuer, bytes) = match client_request(&c) {
                Some(r) => {
                    let b = r.to_json();
                    self.sent.insert(r.cmd_id, b.clone());
                    (r.issuer.clone(), b)
                }
                None => {
                    let Call::Result(issuer, id) = c else {
                        unreachable!()
                    };
                    let b = self
                        .sent
                        .get(&id)
                        .cloned()
                        .unwrap_or_else(|| panic!("result(): unknown cmd_id {id}"));
                    (issuer, b)
                }
            };
            let conn = self.next_conn;
            self.next_conn += 1;
            let uid = uid_of(&issuer);
            if let Some(r) = self.core.check_uid(uid) {
                return Ticket { conn, now: Some(r) };
            }
            match self.core.on_request(conn, uid, &bytes, now) {
                Reply::Now(r) => Ticket { conn, now: Some(*r) },
                Reply::Wait => Ticket { conn, now: None },
            }
        }

        pub fn done(&mut self, t: &mut Ticket) -> Option<CmdResponse> {
            if let Some(r) = t.now.take() {
                return Some(r);
            }
            self.delivered.remove(&t.conn)
        }

        pub fn idle_wait(&mut self) {}
    }
}

#[cfg(target_os = "linux")]
mod sock_front {
    use std::collections::HashMap;
    use std::io::{ErrorKind, Read, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread::JoinHandle;
    use std::time::Duration;

    use hft_ctl::client::{ClientError, CtlClient};
    use hft_ctl::gateway::{Gateway, ResultSource};
    use hft_ctl::{Core, InternalCmd};
    use hft_ctl_proto::frame_len;
    use hft_rt::{Backing, PollTask, Producer, spsc};

    use super::*;

    /// Queue of `CmdResult`s read from the journal tail.
    #[derive(Clone, Default)]
    pub struct ResQ(pub Rc<RefCell<std::collections::VecDeque<CmdResult>>>);

    impl ResultSource for ResQ {
        fn next_result(&mut self) -> Option<CmdResult> {
            self.0.borrow_mut().pop_front()
        }
    }

    static DIR_NO: AtomicU64 = AtomicU64::new(0);

    /// The real gateway task plus the relay that injects G0 uids.
    pub struct Front {
        pub gw: Gateway<Set, RigRing, ResQ>,
        q: ResQ,
        pub internal: Producer<InternalCmd>,
        pub dir: PathBuf,
        /// The gateway's own socket.
        pub socket: PathBuf,
        /// The socket the clients connect to (relayed with the issuer's uid).
        pub client_socket: PathBuf,
        listener: UnixListener,
        incoming: Vec<(UnixStream, Vec<u8>)>,
        relays: Vec<(UnixStream, UnixStream, Vec<u8>)>,
        clients: HashMap<String, Arc<CtlClient>>,
        /// Keep the directory (and the socket files) when dropped: a gateway that died.
        pub keep_dir: bool,
    }

    /// A call in progress (the client blocks in its own thread).
    pub struct Ticket {
        h: Option<JoinHandle<Result<CmdResponse, ClientError>>>,
    }

    impl std::fmt::Debug for Ticket {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("Ticket")
        }
    }

    impl Drop for Front {
        fn drop(&mut self) {
            if !self.keep_dir {
                let _ = std::fs::remove_dir_all(&self.dir);
            }
        }
    }

    impl Front {
        pub fn new(
            rd: RefData,
            tsc: Rc<Cell<u64>>,
            ctl: Rc<RefCell<CtlQ>>,
            parsed: MailSender<ParsedConfig<Set>>,
            retired: MailReceiver<ParsedConfig<Set>>,
            active: Vec<ParsedConfig<Set>>,
            prev: Option<Front>,
        ) -> Front {
            // A restart keeps the socket paths and the clients (their sent requests).
            let (dir, clients) = match prev {
                Some(mut p) => (p.dir.clone(), std::mem::take(&mut p.clients)),
                None => (
                    std::env::temp_dir().join(format!(
                        "ctl-e2e-{}-{}",
                        std::process::id(),
                        DIR_NO.fetch_add(1, Ordering::Relaxed)
                    )),
                    HashMap::new(),
                ),
            };
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));
            let socket = dir.join("ctl.sock");
            let client_socket = dir.join("client.sock");
            let (ring, mut cfg) = gateway_parts(&rd, &tsc, &ctl);
            cfg.socket_path = socket.clone();
            let (i_prod, i_cons) = spsc::<InternalCmd>(16, Backing::Heap).unwrap_or_else(|e| panic!("{e:?}"));
            let q = ResQ::default();
            let gw = Gateway::new(
                cfg,
                rd,
                ring,
                CTL_RING_BYTES,
                parsed,
                retired,
                i_cons,
                refdata_check,
                super::super::world::risk_decode,
                active,
                q.clone(),
                clock(),
                EngineInfo::default(),
            )
            .unwrap_or_else(|e| panic!("{e:?}"));
            let listener = UnixListener::bind(&client_socket).unwrap_or_else(|e| panic!("{e}"));
            listener.set_nonblocking(true).unwrap_or_else(|e| panic!("{e}"));
            Front {
                gw,
                q,
                internal: i_prod,
                dir,
                socket,
                client_socket,
                listener,
                incoming: Vec::new(),
                relays: Vec::new(),
                clients,
                keep_dir: false,
            }
        }

        pub fn core_mut(&mut self) -> &mut Core<Set, RigRing> {
            self.gw.core_mut()
        }

        fn relay(&mut self) -> bool {
            let mut did = false;
            while let Ok((s, _)) = self.listener.accept() {
                let _ = s.set_nonblocking(true);
                self.incoming.push((s, Vec::new()));
                did = true;
            }
            let mut i = 0;
            while i < self.incoming.len() {
                let (s, buf) = &mut self.incoming[i];
                let mut chunk = [0u8; 65_536];
                match s.read(&mut chunk) {
                    Ok(n) if n > 0 => {
                        buf.extend_from_slice(&chunk[..n]);
                        did = true;
                    }
                    _ => {}
                }
                let full = buf
                    .get(..4)
                    .and_then(|h| frame_len(&[h[0], h[1], h[2], h[3]]).ok())
                    .is_some_and(|n| buf.len() >= 4 + n);
                if !full {
                    i += 1;
                    continue;
                }
                let (client, frame) = self.incoming.remove(i);
                let issuer = serde_json::from_slice::<Value>(&frame[4..])
                    .ok()
                    .and_then(|v| v["issuer"].as_str().map(str::to_owned))
                    .unwrap_or_default();
                let (mut a, b) = UnixStream::pair().unwrap_or_else(|e| panic!("{e}"));
                self.gw.adopt(b, uid_of(&issuer));
                a.write_all(&frame).unwrap_or_else(|e| panic!("{e}"));
                let _ = a.set_nonblocking(true);
                self.relays.push((client, a, Vec::new()));
            }
            let mut i = 0;
            while i < self.relays.len() {
                let (_, a, buf) = &mut self.relays[i];
                let mut chunk = [0u8; 65_536];
                let closed = match a.read(&mut chunk) {
                    Ok(0) => true,
                    Ok(n) => {
                        buf.extend_from_slice(&chunk[..n]);
                        did = true;
                        false
                    }
                    Err(e) if e.kind() == ErrorKind::WouldBlock => false,
                    Err(_) => true,
                };
                if closed {
                    let (mut client, _, buf) = self.relays.remove(i);
                    let _ = client.set_nonblocking(false);
                    let _ = client.write_all(&buf);
                    did = true;
                } else {
                    i += 1;
                }
            }
            did
        }

        pub fn poll(&mut self, now: Tsc) -> bool {
            let mut did = self.relay();
            did |= self.gw.poll_once(now).did_work;
            did |= self.relay();
            did
        }

        pub fn result(&mut self, r: CmdResult) {
            self.q.0.borrow_mut().push_back(r);
        }

        fn client(&mut self, issuer: &str) -> Arc<CtlClient> {
            let path = self.client_socket.clone();
            self.clients
                .entry(issuer.to_owned())
                .or_insert_with(|| Arc::new(CtlClient::new(path, issuer)))
                .clone()
        }

        pub fn start(&mut self, c: Call, _now: Tsc) -> Ticket {
            let issuer = match &c {
                Call::Send(v) | Call::Preview(v) | Call::Confirm(v, _) => {
                    v["issuer"].as_str().unwrap_or("").to_owned()
                }
                Call::Result(i, _) => i.clone(),
            };
            let cl = self.client(&issuer);
            let h = std::thread::spawn(move || match c {
                Call::Send(v) => cl.send(&parse(&v), Duration::from_secs(30)),
                Call::Preview(v) => cl.preview(&parse(&v)),
                Call::Confirm(v, sv) => cl.confirm(&parse(&v), sv),
                Call::Result(_, id) => cl.result(id),
            });
            Ticket { h: Some(h) }
        }

        pub fn done(&mut self, t: &mut Ticket) -> Option<CmdResponse> {
            if !t.h.as_ref().is_some_and(JoinHandle::is_finished) {
                return None;
            }
            let h = t.h.take()?;
            match h.join() {
                Ok(Ok(r)) => Some(r),
                Ok(Err(e)) => panic!("client error: {e:?}"),
                Err(_) => panic!("client thread panicked"),
            }
        }

        pub fn idle_wait(&mut self) {
            std::thread::sleep(Duration::from_micros(200));
        }
    }

    /// The `ClientError` of connecting to a socket nobody listens on (CTL-156).
    pub fn connect_error(path: &std::path::Path) -> Option<ClientError> {
        let cl = CtlClient::new(path, "cli:tom");
        let req = cl.request(
            hft_ctl_proto::AccountSel::Missing,
            hft_ctl_proto::CmdBody::Annotate {
                kind: hft_ctl_proto::AnnotateKind::OpsNote,
                text: "x".into(),
                digest: None,
                account: Some("*".into()),
            },
        );
        cl.send(&req, Duration::from_millis(500)).err()
    }
}

#[cfg(target_os = "linux")]
pub use sock_front::connect_error;
