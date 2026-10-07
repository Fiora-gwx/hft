//! `MemPeer`: an in-process rustls 0.23 server (suite TLS13_AES_128_GCM_SHA256) exchanging
//! ciphertext with the connection under test through its `ScriptIo`.
//!
//! - `pair()` builds a scripted `Conn`, runs the TLS handshake, and leaves it `Open` with the
//!   counters, error bits and queue-empty state of a freshly installed connection.
//! - Server → client: `records()` encrypts each given plaintext as exactly one record; `deliver()`
//!   queues bytes as one `recvmsg`.
//! - Client → server: `pump()` takes what the connection sent and decrypts it record by record,
//!   remembering each record's index (= sequence number under the first application key) and
//!   plaintext.

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

use aws_lc_rs::aead::{AES_128_GCM, Aad, LessSafeKey, Nonce, UnboundKey};
use aws_lc_rs::hkdf;
use hft_net::sock::{RecvStep, ScriptIo};
use hft_net::{Conn, ConnCfg, ConnId, ConnPhase, ConnRole};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};

use super::ws_ref::{self, ClientFrame};

pub const CA_PEM: &[u8] = include_bytes!("certs/ca.pem");
pub const SERVER_PEM: &[u8] = include_bytes!("certs/server.pem");
pub const SERVER_KEY: &[u8] = include_bytes!("certs/server.key");

/// TLS 1.3 traffic secrets captured through `KeyLog`.
#[derive(Debug, Default)]
pub struct Keys {
    pub server_app: Mutex<Vec<u8>>,
}

impl rustls::KeyLog for Keys {
    fn log(&self, label: &str, _client_random: &[u8], secret: &[u8]) {
        if label == "SERVER_TRAFFIC_SECRET_0" {
            *self.server_app.lock().unwrap() = secret.to_vec();
        }
    }
    fn will_log(&self, _label: &str) -> bool {
        true
    }
}

/// Server configuration of the test peers.
pub fn server_config(
    tickets: usize,
    versions: &[&'static rustls::SupportedProtocolVersion],
    suites: Option<Vec<rustls::SupportedCipherSuite>>,
    keys: Option<Arc<Keys>>,
) -> rustls::ServerConfig {
    use rustls::crypto::aws_lc_rs as p;
    let mut provider = p::default_provider();
    if let Some(s) = suites {
        provider.cipher_suites = s;
    }
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(SERVER_PEM)
        .map(|c| c.unwrap())
        .collect();
    let key = PrivateKeyDer::from_pem_slice(SERVER_KEY).unwrap();
    let mut cfg = rustls::ServerConfig::builder_with_provider(Arc::new(provider))
        .with_protocol_versions(versions)
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .unwrap();
    cfg.send_tls13_tickets = tickets;
    if let Some(k) = keys {
        cfg.key_log = k;
    }
    cfg
}

/// The 1 GHz test clock (1 tick = 1 ns).
pub fn clock() -> hft_rt::TscClock {
    hft_rt::TscClock::from_hz_milli(1_000_000_000_000, hft_rt::CalibPair::default())
}

/// TESTS.md defaults for `MemPeer`/`ScriptIo` cases.
pub fn cfg() -> ConnCfg {
    ConnCfg {
        rx_buf: 65_536,
        tx_buf: 16_384,
        max_msg: 65_536,
        ts_expect_hw: false,
        quickack: false,
    }
}

#[derive(Debug)]
pub struct SrvRec {
    pub index: u64,
    pub plain: Vec<u8>,
}

pub struct MemPeer {
    pub srv: rustls::ServerConnection,
    pub keys: Arc<Keys>,
    /// Client bytes not yet forming a whole record.
    pending: Vec<u8>,
    /// Records received after the server finished its handshake.
    pub recs: Vec<SrvRec>,
    /// Plaintext of `recs`, concatenated (client WS frames are parsed from it).
    pub client_plain: Vec<u8>,
    /// Records the server wrote under its first application key.
    srv_app_recs: u64,
    next_index: u64,
}

impl std::fmt::Debug for MemPeer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemPeer").field("recs", &self.recs.len()).finish()
    }
}

/// Options of `pair_with`.
#[derive(Clone, Debug)]
pub struct Opts {
    pub cfg: ConnCfg,
    pub role: ConnRole,
    pub id: ConnId,
    pub tickets: usize,
    pub script: ScriptIo,
}

impl Default for Opts {
    fn default() -> Self {
        Opts {
            cfg: cfg(),
            role: ConnRole::Md,
            id: ConnId(1),
            tickets: 0,
            script: ScriptIo::new(),
        }
    }
}

/// The server side only (Connector tests drive the client through `ScriptIo`).
pub fn server_only(tickets: usize) -> MemPeer {
    let keys = Arc::new(Keys::default());
    let sc = server_config(
        tickets,
        &[&rustls::version::TLS13],
        Some(vec![
            rustls::crypto::aws_lc_rs::cipher_suite::TLS13_AES_128_GCM_SHA256,
        ]),
        Some(keys.clone()),
    );
    MemPeer {
        srv: rustls::ServerConnection::new(Arc::new(sc)).unwrap(),
        keys,
        pending: Vec::new(),
        recs: Vec::new(),
        client_plain: Vec::new(),
        srv_app_recs: 0,
        next_index: 0,
    }
}

pub fn pair() -> (Box<Conn>, MemPeer) {
    pair_with(Opts::default())
}

pub fn pair_cfg(cfg: ConnCfg) -> (Box<Conn>, MemPeer) {
    pair_with(Opts {
        cfg,
        ..Opts::default()
    })
}

pub fn pair_with(o: Opts) -> (Box<Conn>, MemPeer) {
    let keys = Arc::new(Keys::default());
    let sc = server_config(
        o.tickets,
        &[&rustls::version::TLS13],
        Some(vec![
            rustls::crypto::aws_lc_rs::cipher_suite::TLS13_AES_128_GCM_SHA256,
        ]),
        Some(keys.clone()),
    );
    let srv = rustls::ServerConnection::new(Arc::new(sc)).unwrap();
    let mut peer = MemPeer {
        srv,
        keys,
        pending: Vec::new(),
        recs: Vec::new(),
        client_plain: Vec::new(),
        srv_app_recs: 0,
        next_index: 0,
    };
    let mut conn = Conn::standalone(o.cfg, o.role, o.id, clock(), o.script);
    let ccfg = hft_net::client_tls_config(Some(CA_PEM)).unwrap();
    assert!(conn.test_start_tls(&ccfg, "a.test"));
    for _ in 0..50 {
        let r = conn.test_handshake_step();
        let out = conn.script().unwrap().take_sent();
        peer.feed_handshake(&out);
        let back = peer.drain();
        if !back.is_empty() {
            conn.script().unwrap().push_recv(RecvStep::data(0, 0, back));
        }
        match r {
            Some(Ok(_)) => break,
            Some(Err(())) => panic!("scripted handshake failed: {:?}", conn.err()),
            None => {}
        }
    }
    assert!(!peer.srv.is_handshaking(), "server handshake incomplete");
    // Tickets (if any) were queued above; leave them for the test to deliver or drop.
    conn.script().unwrap().recv.clear();
    conn.test_set_phase(ConnPhase::Open);
    conn.test_reset_stats();
    conn.test_on_install();
    (conn, peer)
}

impl MemPeer {
    pub fn feed_handshake(&mut self, mut b: &[u8]) {
        while !b.is_empty() {
            let n = self.srv.read_tls(&mut b).unwrap();
            if n == 0 {
                break;
            }
            self.srv.process_new_packets().unwrap();
        }
    }

    /// Everything the server has to send.
    pub fn drain(&mut self) -> Vec<u8> {
        let counting = !self.srv.is_handshaking();
        let mut out = Vec::new();
        while self.srv.wants_write() {
            self.srv.write_tls(&mut out).unwrap();
        }
        if counting {
            self.srv_app_recs += count_records(&out);
        }
        out
    }

    /// Each plaintext as exactly one server record (≤ 16 384 B each).
    pub fn records(&mut self, recs: &[&[u8]]) -> Vec<u8> {
        let mut out = Vec::new();
        for r in recs {
            assert!(!r.is_empty() && r.len() <= 16_384);
            self.srv.writer().write_all(r).unwrap();
            out.extend(self.drain());
        }
        out
    }

    /// One record per frame.
    pub fn frames(&mut self, frames: &[Vec<u8>]) -> Vec<u8> {
        let v: Vec<&[u8]> = frames.iter().map(|f| f.as_slice()).collect();
        self.records(&v)
    }

    /// Plaintext `p` split into records of the given sizes (the last takes the rest).
    pub fn split(&mut self, p: &[u8], sizes: &[usize]) -> Vec<u8> {
        let mut parts = Vec::new();
        let mut off = 0;
        for &s in sizes {
            if off >= p.len() {
                break;
            }
            let e = (off + s).min(p.len());
            parts.push(&p[off..e]);
            off = e;
        }
        if off < p.len() {
            for c in p[off..].chunks(16_384) {
                parts.push(c);
            }
        }
        self.records(&parts)
    }

    /// Server KeyUpdate (update_requested).
    pub fn key_update(&mut self) -> Vec<u8> {
        self.srv.refresh_traffic_keys().unwrap();
        self.drain()
    }

    pub fn close_notify(&mut self) -> Vec<u8> {
        self.srv.send_close_notify();
        self.drain()
    }

    /// A fatal alert record encrypted under the server's first application key (crafted: rustls
    /// has no API to send an arbitrary alert).
    pub fn fatal_alert(&mut self, desc: u8) -> Vec<u8> {
        let secret = self.keys.server_app.lock().unwrap().clone();
        assert!(!secret.is_empty());
        let key = expand_label(&secret, b"key", 16);
        let iv = expand_label(&secret, b"iv", 12);
        let k = LessSafeKey::new(UnboundKey::new(&AES_128_GCM, &key).unwrap());
        let mut nonce = [0u8; 12];
        nonce.copy_from_slice(&iv);
        for (a, b) in nonce[4..].iter_mut().zip(self.srv_app_recs.to_be_bytes()) {
            *a ^= b;
        }
        let mut body = vec![2u8, desc, 21];
        let hdr = [23u8, 3, 3, 0, (body.len() + 16) as u8];
        k.seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::from(hdr), &mut body)
            .unwrap();
        self.srv_app_recs += 1;
        let mut out = hdr.to_vec();
        out.extend_from_slice(&body);
        out
    }

    /// Takes what the client sent and decrypts it record by record.
    pub fn pump(&mut self, conn: &mut Conn) -> usize {
        let b = conn.script().unwrap().take_sent();
        self.feed_client(&b)
    }

    /// Feeds client bytes; returns the number of whole records processed.
    pub fn feed_client(&mut self, b: &[u8]) -> usize {
        self.pending.extend_from_slice(b);
        let mut n = 0;
        loop {
            if self.pending.len() < 5 {
                break;
            }
            let len = u16::from_be_bytes([self.pending[3], self.pending[4]]) as usize;
            if self.pending.len() < 5 + len {
                break;
            }
            let rec: Vec<u8> = self.pending.drain(..5 + len).collect();
            let mut s = &rec[..];
            while !s.is_empty() {
                self.srv.read_tls(&mut s).unwrap();
            }
            self.srv
                .process_new_packets()
                .expect("server failed to process a client record");
            let mut plain = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                match self.srv.reader().read(&mut buf) {
                    Ok(0) => break,
                    Ok(k) => plain.extend_from_slice(&buf[..k]),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(e) => panic!("{e}"),
                }
            }
            self.client_plain.extend_from_slice(&plain);
            self.recs.push(SrvRec {
                index: self.next_index,
                plain,
            });
            self.next_index += 1;
            n += 1;
        }
        n
    }

    /// Complete client WS frames received so far (consumed).
    pub fn client_frames(&mut self) -> Vec<ClientFrame> {
        let (f, used) = ws_ref::parse_client(&self.client_plain);
        self.client_plain.drain(..used);
        f
    }
}

impl MemPeer {
    /// Answers a WS Upgrade request already read by the server with `101` and the right accept.
    pub fn answer_upgrade(&mut self) -> Vec<u8> {
        let mut req = Vec::new();
        let mut b = [0u8; 4096];
        while let Ok(k) = self.srv.reader().read(&mut b) {
            if k == 0 {
                break;
            }
            req.extend_from_slice(&b[..k]);
        }
        let t = String::from_utf8_lossy(&req).to_string();
        let key = t
            .lines()
            .find_map(|l| l.strip_prefix("Sec-WebSocket-Key: "))
            .unwrap_or("")
            .trim()
            .to_string();
        let acc = String::from_utf8(hft_net::ws_accept(key.as_bytes()).to_vec()).unwrap();
        let resp = format!(
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {acc}\r\n\r\n"
        );
        self.srv.writer().write_all(resp.as_bytes()).unwrap();
        self.drain()
    }
}

/// Builds a WS connection through a scripted `Connector` job against `peer`; returns at `Ready`.
pub fn ws_build(
    c: &mut hft_net::Connector,
    job: hft_net::JobId,
    peer: &mut MemPeer,
    t0: i64,
) -> hft_net::BuildEvent {
    assert_eq!(c.drive(t0), None);
    let out = c.script(job).unwrap().take_sent();
    peer.feed_handshake(&out);
    let back = peer.drain();
    c.script(job).unwrap().push_recv(RecvStep::data(0, 0, back));
    assert_eq!(c.drive(t0 + 1), None);
    let out = c.script(job).unwrap().take_sent();
    peer.feed_handshake(&out);
    let back = peer.answer_upgrade();
    c.script(job).unwrap().push_recv(RecvStep::data(0, 0, back));
    let _ = c.drive(t0 + 2);
    c.drive(t0 + 2 + hft_net::TICKET_WAIT_NS).expect("Ready")
}

/// Queues `bytes` as one `recvmsg` with the given TSCs and stamps.
pub fn deliver(conn: &mut Conn, pre: u64, post: u64, bytes: Vec<u8>) {
    conn.script().unwrap().push_recv(RecvStep::data(pre, post, bytes));
}

pub fn deliver_stamped(conn: &mut Conn, pre: u64, post: u64, bytes: Vec<u8>, hw: i64, sw: i64) {
    conn.script()
        .unwrap()
        .push_recv(RecvStep::stamped(pre, post, bytes, hw, sw));
}

/// Number of whole records in a byte stream.
pub fn count_records(mut b: &[u8]) -> u64 {
    let mut n = 0;
    while b.len() >= 5 {
        let len = u16::from_be_bytes([b[3], b[4]]) as usize;
        if b.len() < 5 + len {
            break;
        }
        b = &b[5 + len..];
        n += 1;
    }
    n
}

struct Len(usize);
impl hkdf::KeyType for Len {
    fn len(&self) -> usize {
        self.0
    }
}

/// TLS 1.3 HKDF-Expand-Label(secret, label, "", len) with SHA-256.
pub fn expand_label(secret: &[u8], label: &[u8], len: usize) -> Vec<u8> {
    let prk = hkdf::Prk::new_less_safe(hkdf::HKDF_SHA256, secret);
    let mut info = Vec::new();
    info.extend_from_slice(&(len as u16).to_be_bytes());
    info.push((6 + label.len()) as u8);
    info.extend_from_slice(b"tls13 ");
    info.extend_from_slice(label);
    info.push(0);
    let info_parts = [info.as_slice()];
    let okm = prk.expand(&info_parts, Len(len)).unwrap();
    let mut out = vec![0u8; len];
    okm.fill(&mut out).unwrap();
    out
}
