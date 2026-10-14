//! `ScriptNet` + `BuildStub` (TESTS.md "夹具"): a scripted `Connector` (hft-net `test-io`) whose
//! connections finish a real TLS 1.3 handshake and WS Upgrade against in-process `MemPeer`s; events
//! scheduled per connection are delivered when the test's clock reaches them (all data due for one
//! connection in one `recvmsg`, pre/post TSC = now); `sent` decrypts what the runtime sent.
#![allow(dead_code)]

use std::collections::VecDeque;
use std::net::Ipv4Addr;

use hft_net::sock::{RecvKind, RecvStep, ScriptIo};
use hft_net::{
    BuildReq, Conn, ConnArena, ConnId, ConnRole, ConnSet, Connector, Endpoint, Handoff, NetConfig, RetireMsg,
    RoleBytes,
};
use hft_rt::{Backing, Consumer, Producer, spsc};

use super::mempeer::{self, MemPeer};
use super::ws_ref::{self, ClientFrame};

/// Access to the installed connections of a runtime.
pub trait Rt {
    fn conn_mut(&mut self, id: ConnId) -> Option<&mut Conn>;
}

impl<P: hft_venue_proto::VenueProtocol> Rt for hft_venue::FeedRuntime<P> {
    fn conn_mut(&mut self, id: ConnId) -> Option<&mut Conn> {
        self.conn(id)
    }
}

impl<P: hft_venue_proto::VenueProtocol, const C: usize> Rt for hft_venue::GwRuntime<P, C> {
    fn conn_mut(&mut self, id: ConnId) -> Option<&mut Conn> {
        self.conn(id)
    }
}

/// What the peer does.
#[derive(Clone, Debug)]
pub enum Ev {
    Text(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    /// Raw WS frame bytes (e.g. a huge or malformed frame).
    Frame(Vec<u8>),
    Eof,
    Rst,
}

pub const ECONNRESET: i32 = 104;

pub fn net_cfg() -> NetConfig {
    let ep = Endpoint {
        host: "a.test".into(),
        sni: "a.test".into(),
        port: 443,
        path: "/ws".into(),
        ips: vec![Ipv4Addr::LOCALHOST],
        upgrade_headers: Vec::new(),
    };
    NetConfig {
        endpoints: vec![ep.clone(), ep.clone(), ep.clone(), ep],
        src_ips: vec![
            Ipv4Addr::new(10, 0, 0, 5),
            Ipv4Addr::new(10, 0, 0, 6),
            Ipv4Addr::new(10, 0, 0, 7),
        ],
        bind_device: vec![
            (Ipv4Addr::new(10, 0, 0, 5), "eth1".into()),
            (Ipv4Addr::new(10, 0, 0, 6), "eth1".into()),
            (Ipv4Addr::new(10, 0, 0, 7), "eth1".into()),
        ],
        extra_ca: Some(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../hft-net/tests/support/certs/ca.pem"
            )
            .into(),
        ),
        endpoint_override: true,
        connect_timeout_ms: 3000,
        tls_timeout_ms: 3000,
        upgrade_timeout_ms: 3000,
        ts_expect_hw: false,
        rx_buf: RoleBytes::all(65_536),
        tx_buf: 16_384,
        max_msg: 65_536,
        ..NetConfig::default()
    }
}

pub struct Net {
    cc: Option<Box<Connector>>,
    peers: Vec<Option<MemPeer>>,
    sched: Vec<(u64, ConnId, Ev)>,
    t: i64,
    pub ready: ConnSet,
    ready_tx: Option<Producer<ConnSet>>,
    /// Hand-overs prepared by `complete` that the test pushes itself.
    pub done: VecDeque<Handoff>,
}

thread_local! {
    /// Connectors whose arena slots are all free again (arenas are leaked by design; reuse them).
    #[allow(clippy::vec_box)] // moving a large Connector by pointer
    static SPARE: std::cell::RefCell<Vec<Box<Connector>>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Arena plan of every test connector.
const PLAN: [(ConnRole, u16); 4] = [
    (ConnRole::Md, 64),
    (ConnRole::Order, 16),
    (ConnRole::UserStream, 4),
    (ConnRole::Rest, 2),
];

impl Drop for Net {
    fn drop(&mut self) {
        let total: u16 = PLAN.iter().map(|p| p.1).sum();
        if let Some(c) = self.cc.take()
            && c.test_free_slots() == usize::from(total)
        {
            SPARE.with(|s| s.borrow_mut().push(c));
        }
    }
}

fn fresh_connector() -> Connector {
    connector_with(net_cfg())
}

fn connector_with(cfg: NetConfig) -> Connector {
    let arena = ConnArena::new(&PLAN, &cfg, mempeer::clock());
    let mut c = Connector::new(arena, &cfg).unwrap();
    c.test_fix_ws_key(Some([7u8; 16]));
    c
}

impl Net {
    /// A connector from this thread's spares, or a new one (the counts are fixed by `PLAN`).
    pub fn new(_md: u16, _order: u16) -> Net {
        let c = SPARE
            .with(|s| s.borrow_mut().pop())
            .unwrap_or_else(|| Box::new(fresh_connector()));
        Self::from_connector(c)
    }

    /// A separate test binary may resolve its fixture CA relative to its own manifest.
    pub fn with_config(cfg: NetConfig) -> Net {
        Self::from_connector(Box::new(connector_with(cfg)))
    }

    fn from_connector(c: Box<Connector>) -> Net {
        let mut peers = Vec::with_capacity(256);
        peers.resize_with(256, || None);
        Net {
            cc: Some(c),
            peers,
            sched: Vec::new(),
            t: 1,
            ready: ConnSet::default(),
            ready_tx: None,
            done: VecDeque::new(),
        }
    }

    /// A fresh (uncached) connector whose connections expect HW receive stamps (`TS_MISSING`).
    pub fn expecting_hw() -> Net {
        let mut n = Net::new(0, 0);
        let mut cfg = net_cfg();
        cfg.ts_expect_hw = true;
        n.cc = Some(Box::new(connector_with(cfg)));
        n
    }

    /// The connector.
    pub fn c(&mut self) -> &mut Connector {
        self.cc.as_mut().expect("connector")
    }

    /// A scripted `ReadyPoll` driven by `set_ready`.
    pub fn ready_poll(&mut self) -> hft_net::ReadyPoll {
        let (tx, rx): (Producer<ConnSet>, Consumer<ConnSet>) = spsc(64, Backing::Heap).unwrap();
        self.ready_tx = Some(tx);
        hft_net::ReadyPoll::scripted(rx)
    }

    pub fn set_ready(&mut self, id: ConnId, on: bool) {
        let i = (id.0 >> 6) as usize;
        if on {
            self.ready.insert(id);
        } else {
            self.ready.bits[i] &= !(1u64 << (id.0 & 63));
        }
        if let Some(tx) = self.ready_tx.as_mut() {
            let _ = tx.try_push(self.ready);
        }
    }

    /// Builds the requested connection through the connector (handshake + Upgrade against a fresh
    /// `MemPeer`) and returns its hand-off.
    pub fn complete(&mut self, req: BuildReq) -> Handoff {
        let mut io = ScriptIo::new();
        io.idle_tsc = 0;
        self.c().push_script(io);
        let job = self.c().submit(req).unwrap();
        let mut peer = mempeer::server_only(0);
        let t0 = self.t;
        self.t += 1_000_000_000;
        let c = self.cc.as_mut().expect("connector");
        let ev = mempeer::ws_build(c, job, &mut peer, t0);
        assert!(matches!(ev, hft_net::BuildEvent::Ready { .. }), "{ev:?}");
        let h = self.c().handoff(job).unwrap();
        if let Some(s) = self.peers.get_mut(req.conn.0 as usize) {
            *s = Some(peer);
        }
        h
    }

    /// Takes over the peer of a connection built elsewhere (e.g. by a `SessionDriver` on this
    /// net's connector).
    pub fn adopt(&mut self, id: ConnId, peer: MemPeer) {
        self.peers[id.0 as usize] = Some(peer);
    }

    /// The peer of connection `id`.
    pub fn peer(&mut self, id: ConnId) -> &mut MemPeer {
        self.peers[id.0 as usize].as_mut().expect("no peer")
    }

    /// Returns retired connections to the connector.
    pub fn reap(&mut self, rx: &mut Consumer<RetireMsg>) -> Vec<ConnId> {
        let mut v = Vec::new();
        while let Some(m) = rx.try_pop() {
            v.push(m.id());
            self.c().reclaim(m);
        }
        v
    }

    pub fn at(&mut self, id: ConnId, t: u64, ev: Ev) {
        self.sched.push((t, id, ev));
    }

    /// Text frame now (delivered by the next `flush`).
    pub fn push(&mut self, id: ConnId, payload: &[u8]) {
        self.sched.push((0, id, Ev::Text(payload.to_vec())));
    }

    /// Queues `payload` as one `recvmsg` right now with explicit TSCs and stamps.
    #[allow(clippy::too_many_arguments)]
    pub fn deliver_stamped<R: Rt>(
        &mut self,
        rt: &mut R,
        id: ConnId,
        payloads: &[&[u8]],
        pre: u64,
        post: u64,
        hw: i64,
        sw: i64,
    ) {
        let peer = self.peers[id.0 as usize].as_mut().unwrap();
        let mut bytes = Vec::new();
        for p in payloads {
            bytes.extend(peer.frames(&[ws_ref::text(p)]));
        }
        let c = rt.conn_mut(id).unwrap();
        c.script()
            .unwrap()
            .push_recv(RecvStep::stamped(pre, post, bytes, hw, sw));
    }

    /// Delivers every event due at `now`: one `recvmsg` per connection carrying all its due data
    /// (pre/post TSC = `now`), then EOF/RST steps; sets every script's idle TSC to `now`.
    pub fn flush<R: Rt>(&mut self, rt: &mut R, now: u64) {
        let mut due: Vec<(ConnId, Ev)> = Vec::new();
        let mut keep = Vec::new();
        for (t, id, ev) in self.sched.drain(..) {
            if t <= now {
                due.push((id, ev));
            } else {
                keep.push((t, id, ev));
            }
        }
        self.sched = keep;
        let mut ids: Vec<ConnId> = due.iter().map(|(i, _)| *i).collect();
        ids.dedup();
        let mut seen = Vec::new();
        for id in ids {
            if seen.contains(&id) {
                continue;
            }
            seen.push(id);
            let mut bytes = Vec::new();
            let mut tail = Vec::new();
            for (i, ev) in due.iter() {
                if *i != id {
                    continue;
                }
                let Some(peer) = self.peers.get_mut(id.0 as usize).and_then(Option::as_mut) else {
                    continue;
                };
                match ev {
                    Ev::Text(p) => bytes.extend(peer.frames(&[ws_ref::text(p)])),
                    Ev::Ping(p) => bytes.extend(peer.frames(&[ws_ref::server_frame(true, 0x9, p)])),
                    Ev::Pong(p) => bytes.extend(peer.frames(&[ws_ref::server_frame(true, 0xA, p)])),
                    Ev::Frame(f) => {
                        for chunk in f.chunks(16_384) {
                            bytes.extend(peer.records(&[chunk]));
                        }
                    }
                    Ev::Eof => tail.push(RecvKind::Eof),
                    Ev::Rst => tail.push(RecvKind::Errno(ECONNRESET)),
                }
            }
            let Some(conn) = rt.conn_mut(id) else { continue };
            let s = conn.script().unwrap();
            if !bytes.is_empty() {
                s.push_recv(RecvStep::data(now, now, bytes));
            }
            for k in tail {
                s.push_recv(RecvStep {
                    pre_tsc: now,
                    post_tsc: now,
                    kind: k,
                });
            }
        }
        self.idle(rt, now);
    }

    /// Sets every known connection's idle TSC (EAGAIN and send stamps) to `now`.
    pub fn idle<R: Rt>(&mut self, rt: &mut R, now: u64) {
        for i in 0..256usize {
            if self.peers[i].is_some()
                && let Some(c) = rt.conn_mut(ConnId(i as u8))
                && let Some(s) = c.script()
            {
                s.idle_tsc = now;
            }
        }
    }

    /// Text payloads the runtime sent on `id` since the last call (decrypted by the peer).
    pub fn sent<R: Rt>(&mut self, rt: &mut R, id: ConnId) -> Vec<ClientFrame> {
        let Some(conn) = rt.conn_mut(id) else {
            return Vec::new();
        };
        let b = conn.script().unwrap().take_sent();
        let peer = self.peers[id.0 as usize].as_mut().unwrap();
        peer.feed_client(&b);
        peer.client_frames()
    }

    /// Like `sent`, text payloads only (as strings).
    pub fn sent_text<R: Rt>(&mut self, rt: &mut R, id: ConnId) -> Vec<String> {
        self.sent(rt, id)
            .into_iter()
            .filter(|f| f.opcode == 1)
            .map(|f| String::from_utf8_lossy(&f.payload).into_owned())
            .collect()
    }

    /// Ciphertext bytes the peer received on `id` (sent and not yet taken).
    pub fn raw_sent_len<R: Rt>(&mut self, rt: &mut R, id: ConnId) -> usize {
        rt.conn_mut(id)
            .and_then(|c| c.script().map(|s| s.sent.len()))
            .unwrap_or(0)
    }
}
