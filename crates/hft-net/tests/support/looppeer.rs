//! `LoopPeer`: a TLS + WS/HTTP test server on loopback TCP (Linux). Its certificate is signed by
//! the test CA (`certs/ca.pem`), which clients load through `extra_ca` with `endpoint_override`.
//! Each accepted connection is served on its own thread up to the end of the handshake (and the
//! Upgrade), then handed to the test as a blocking `PeerConn`.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use hft_net::{
    BuildEvent, BuildReq, ConnArena, ConnId, ConnPool, ConnRole, Connector, Endpoint, EndpointId, Handoff,
    JobId, NetConfig, PoolMode, RotatePolicy,
};
use hft_types::VenueId;

use super::mempeer::server_config;
use super::ws_ref;

/// How the server answers the WS Upgrade.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Upgrade {
    Ok,
    /// 101 with this `Sec-WebSocket-Accept` value.
    Accept(String),
    /// 101 without `Sec-WebSocket-Accept`.
    NoAccept,
    /// This status instead of 101.
    Status(u16),
    /// 101 plus `Sec-WebSocket-Extensions: permessage-deflate`.
    Extensions,
}

#[derive(Clone)]
pub struct LoopOpts {
    pub ws: bool,
    pub upgrade: Upgrade,
    /// Answer the ClientHello with this plaintext fatal alert and close.
    pub alert_after_hello: Option<u8>,
    pub versions: Vec<&'static rustls::SupportedProtocolVersion>,
    pub suites: Option<Vec<rustls::SupportedCipherSuite>>,
    pub kx_x25519_only: bool,
    pub tickets: usize,
    /// Wait this long before answering the Upgrade (lets a test observe `WsUpgrade` between drives).
    pub upgrade_delay_ms: u64,
}

impl Default for LoopOpts {
    fn default() -> Self {
        LoopOpts {
            ws: true,
            upgrade: Upgrade::Ok,
            alert_after_hello: None,
            versions: vec![&rustls::version::TLS13, &rustls::version::TLS12],
            suites: None,
            kx_x25519_only: false,
            tickets: 0,
            upgrade_delay_ms: 0,
        }
    }
}

/// One accepted connection after its handshake (and Upgrade).
pub struct PeerConn {
    pub s: rustls::StreamOwned<rustls::ServerConnection, TcpStream>,
    /// The Upgrade (or first HTTP) request as received.
    pub request: Vec<u8>,
    pub client_hello: Vec<u8>,
}

impl PeerConn {
    pub fn write_frame(&mut self, f: &[u8]) {
        self.s.write_all(f).unwrap();
        self.s.flush().unwrap();
    }

    pub fn text(&mut self, p: &[u8]) {
        self.write_frame(&ws_ref::text(p));
    }

    /// Reads client WS frames until `n` arrived (5 s limit).
    pub fn read_frames(&mut self, n: usize, acc: &mut Vec<u8>) -> Vec<ws_ref::ClientFrame> {
        let mut out = Vec::new();
        let end = Instant::now() + Duration::from_secs(5);
        self.s
            .sock
            .set_read_timeout(Some(Duration::from_millis(20)))
            .unwrap();
        while out.len() < n && Instant::now() < end {
            let mut b = [0u8; 65536];
            match self.s.read(&mut b) {
                Ok(0) => break,
                Ok(k) => acc.extend_from_slice(&b[..k]),
                Err(_) => {}
            }
            let (f, used) = ws_ref::parse_client(acc);
            acc.drain(..used);
            out.extend(f);
        }
        out
    }

    /// Whether the peer sees EOF (FIN, or RST) within `ms`.
    pub fn sees_eof(&mut self, ms: u64) -> bool {
        self.s
            .sock
            .set_read_timeout(Some(Duration::from_millis(ms)))
            .unwrap();
        let mut b = [0u8; 1024];
        let end = Instant::now() + Duration::from_millis(ms);
        loop {
            match self.s.read(&mut b) {
                Ok(0) => return true,
                Ok(_) => {}
                Err(e) => match e.kind() {
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => {
                        if Instant::now() >= end {
                            return false;
                        }
                    }
                    _ => return true,
                },
            }
        }
    }
}

pub struct LoopPeer {
    pub port: u16,
    rx: mpsc::Receiver<PeerConn>,
    pub hellos: Arc<Mutex<Vec<Vec<u8>>>>,
}

fn read_record(s: &mut TcpStream) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut b = [0u8; 4096];
    loop {
        if buf.len() >= 5 {
            let len = u16::from_be_bytes([buf[3], buf[4]]) as usize;
            if buf.len() >= 5 + len {
                return buf;
            }
        }
        match s.read(&mut b) {
            Ok(0) | Err(_) => return buf,
            Ok(k) => buf.extend_from_slice(&b[..k]),
        }
    }
}

fn serve(mut tcp: TcpStream, o: LoopOpts, tx: mpsc::Sender<PeerConn>, hellos: Arc<Mutex<Vec<Vec<u8>>>>) {
    tcp.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let hello = read_record(&mut tcp);
    hellos.lock().unwrap().push(hello.clone());
    if let Some(desc) = o.alert_after_hello {
        let _ = tcp.write_all(&[21, 3, 3, 0, 2, 2, desc]);
        std::thread::sleep(Duration::from_millis(200));
        return;
    }
    let mut provider = rustls::crypto::aws_lc_rs::default_provider();
    if let Some(s) = o.suites.clone() {
        provider.cipher_suites = s;
    }
    if o.kx_x25519_only {
        provider.kx_groups = vec![rustls::crypto::aws_lc_rs::kx_group::X25519];
    }
    let mut cfg = server_config(o.tickets, &o.versions, None, None);
    let base = cfg.clone();
    cfg = rustls::ServerConfig::builder_with_provider(Arc::new(provider))
        .with_protocol_versions(&o.versions)
        .unwrap()
        .with_no_client_auth()
        .with_cert_resolver(base.cert_resolver.clone());
    cfg.send_tls13_tickets = o.tickets;
    let mut sc = rustls::ServerConnection::new(Arc::new(cfg)).unwrap();
    let mut h = &hello[..];
    while !h.is_empty() {
        if sc.read_tls(&mut h).is_err() {
            return;
        }
    }
    if sc.process_new_packets().is_err() {
        let _ = sc.write_tls(&mut tcp);
        return;
    }
    let mut s = rustls::StreamOwned::new(sc, tcp);
    while s.conn.is_handshaking() {
        if s.conn.complete_io(&mut s.sock).is_err() {
            let _ = s.conn.write_tls(&mut s.sock);
            return;
        }
    }
    let mut request = Vec::new();
    if o.ws {
        let mut b = [0u8; 4096];
        while !request.windows(4).any(|w| w == b"\r\n\r\n") {
            match s.read(&mut b) {
                Ok(0) | Err(_) => return,
                Ok(k) => request.extend_from_slice(&b[..k]),
            }
        }
        let text = String::from_utf8_lossy(&request).to_string();
        let key = text
            .lines()
            .find_map(|l| l.strip_prefix("Sec-WebSocket-Key: "))
            .unwrap_or("")
            .trim()
            .to_string();
        let accept = String::from_utf8(hft_net::ws_accept(key.as_bytes()).to_vec()).unwrap();
        if o.upgrade_delay_ms > 0 {
            std::thread::sleep(Duration::from_millis(o.upgrade_delay_ms));
        }
        let resp = match &o.upgrade {
            Upgrade::Ok => format!(
                "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
            ),
            Upgrade::Accept(a) => format!(
                "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {a}\r\n\r\n"
            ),
            Upgrade::NoAccept => {
                "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\r\n"
                    .to_string()
            }
            Upgrade::Status(c) => format!("HTTP/1.1 {c} X\r\nContent-Length: 0\r\n\r\n"),
            Upgrade::Extensions => format!(
                "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\nSec-WebSocket-Extensions: permessage-deflate\r\n\r\n"
            ),
        };
        if s.write_all(resp.as_bytes()).is_err() || s.flush().is_err() {
            return;
        }
    }
    let _ = tx.send(PeerConn {
        s,
        request,
        client_hello: hello,
    });
}

impl LoopPeer {
    pub fn start(o: LoopOpts) -> LoopPeer {
        let l = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = l.local_addr().unwrap().port();
        let (tx, rx) = mpsc::channel();
        let hellos = Arc::new(Mutex::new(Vec::new()));
        let h2 = hellos.clone();
        std::thread::spawn(move || {
            for c in l.incoming() {
                let Ok(c) = c else { break };
                let (o, tx, h) = (o.clone(), tx.clone(), h2.clone());
                std::thread::spawn(move || serve(c, o, tx, h));
            }
        });
        LoopPeer { port, rx, hellos }
    }

    pub fn accept(&self) -> PeerConn {
        self.rx
            .recv_timeout(Duration::from_secs(10))
            .expect("LoopPeer: no connection")
    }
}

/// WSL's mirrored networking routes 127.0.0.1 through `loopback0` (binding such a socket to
/// `lo` blackholes it); every other Linux host routes loopback through `lo`.
pub fn wsl_mirrored() -> bool {
    std::path::Path::new("/sys/class/net/loopback0").exists()
}

/// The interface traffic from 127.0.0.1 to 127.0.0.1 leaves through.
pub fn lo_dev() -> &'static str {
    if wsl_mirrored() { "loopback0" } else { "lo" }
}

/// Waits (test only) until every fd is readable, at most 5 s.
pub fn wait_readable(fds: &[i32]) {
    let end = Instant::now() + Duration::from_secs(5);
    let mut ps: Vec<libc::pollfd> = fds
        .iter()
        .map(|&fd| libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        })
        .collect();
    while Instant::now() < end {
        let n = unsafe { libc::poll(ps.as_mut_ptr(), ps.len() as libc::nfds_t, 10) };
        if n as usize == ps.len() {
            return;
        }
        for p in ps.iter_mut() {
            p.revents = 0;
        }
    }
    panic!("not every socket became readable");
}

/// Client configuration toward `port` (test CA trusted, source 127.0.0.1 bound to `lo`).
pub fn net_cfg(port: u16) -> NetConfig {
    NetConfig {
        endpoints: vec![Endpoint {
            host: "a.test".into(),
            sni: "a.test".into(),
            port,
            path: "/ws".into(),
            ips: vec![Ipv4Addr::LOCALHOST],
            upgrade_headers: Vec::new(),
        }],
        src_ips: vec![Ipv4Addr::LOCALHOST],
        bind_device: vec![(Ipv4Addr::LOCALHOST, lo_dev().into())],
        extra_ca: Some(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/certs/ca.pem").into()),
        endpoint_override: true,
        ts_expect_hw: false,
        ..NetConfig::default()
    }
}

pub fn clock() -> hft_rt::TscClock {
    static CLOCK: std::sync::OnceLock<hft_rt::TscClock> = std::sync::OnceLock::new();
    *CLOCK.get_or_init(|| {
        let cfg = hft_rt::CalibConfig {
            rounds: 1,
            ..hft_rt::CalibConfig::default()
        };
        hft_rt::TscClock::calibrate(&cfg).unwrap_or_else(|_| super::mempeer::clock())
    })
}

pub fn now_ns() -> i64 {
    static T0: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    T0.get_or_init(Instant::now).elapsed().as_nanos() as i64 + 1_000_000_000
}

pub fn req(conn: u8, role: ConnRole) -> BuildReq {
    BuildReq {
        venue: VenueId(1),
        conn: ConnId(conn),
        role,
        endpoint: EndpointId(0),
        src_ip_idx: 0,
        rx_queue: 0,
        deadline_ns: now_ns() + 10_000_000_000,
        account: hft_types::AccountId(0),
    }
}

/// Drives until the job's event (10 s); returns the event and the phases seen between drives.
pub fn drive_job(c: &mut Connector, job: JobId) -> (BuildEvent, Vec<hft_net::ConnPhase>) {
    let mut phases = Vec::new();
    let end = Instant::now() + Duration::from_secs(10);
    while Instant::now() < end {
        if let Some(p) = c.phase(job)
            && phases.last() != Some(&p)
        {
            phases.push(p);
        }
        if let Some(ev) = c.drive(now_ns()) {
            if let Some(p) = c.phase(job)
                && phases.last() != Some(&p)
            {
                phases.push(p);
            }
            return (ev, phases);
        }
        std::thread::sleep(Duration::from_micros(200));
    }
    panic!("no build event within 10 s");
}

/// A Connector over a fresh arena for `peer`.
pub fn connector(port: u16, plan: &[(ConnRole, u16)]) -> Connector {
    let cfg = net_cfg(port);
    let arena = ConnArena::new(plan, &cfg, clock());
    Connector::new(arena, &cfg).unwrap()
}

/// Builds one connection to `peer` and hands it over.
pub fn open(c: &mut Connector, peer: &LoopPeer, id: u8, role: ConnRole) -> (Handoff, PeerConn) {
    let job = c.submit(req(id, role)).unwrap();
    let (ev, _) = drive_job(c, job);
    assert!(matches!(ev, BuildEvent::Ready { .. }), "{ev:?}");
    let pc = peer.accept();
    (c.handoff(job).unwrap(), pc)
}

/// A pool holding many connections (tests look them up by id).
pub fn pool() -> Box<ConnPool<128>> {
    Box::new(ConnPool::new(PoolMode::Race, 2, RotatePolicy::default()))
}
