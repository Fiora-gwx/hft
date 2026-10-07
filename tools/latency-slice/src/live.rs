//! Main run network side (README "线程布局"): `NetConfig`, the conn-builder (`warm`), the
//! `feed-bbo` and `gw` runtimes of hft-venue and the ballast connections (`feed-md`).

use std::net::Ipv4Addr;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use hft_event::ConnRole;
use hft_net::{
    BuildReq, ConnArena, ConnId, ConnPool, Connector, Endpoint, EndpointId, ErrBits, NetConfig, PoolMode,
    RetireMsg, RotatePolicy,
};
use hft_refdata::RefData;
use hft_rt::{Consumer, Producer, TscClock};
use hft_types::{AccountId, InstId, MAX_BBO_CONN_SLOTS, VenueId};
use hft_venue::{Handover, MD_FIRST, MD_LAST, SessEvent, SessionCfg, SessionDriver};
use hft_venue_proto::{
    ArrayBuf, Credential, EndpointSpec, RouteId, RouteKind, SecretBuf, SignAlg, StreamKind, SubFrames,
    SubSpec, VenueCfg, VenueEnv, VenueProtocol,
};

use crate::config::{Config, Idle, Keys};
use crate::threads::{Stops, idle, utc_ns};

/// Endpoint indices of `NetConfig.endpoints`.
pub const EP_BBO: EndpointId = EndpointId(0);
pub const EP_DEPTH: EndpointId = EndpointId(1);
pub const EP_ORDER: EndpointId = EndpointId(2);

/// The route of `kind` in `P::routes()`.
pub fn route<P: VenueProtocol>(kind: RouteKind) -> Option<RouteId> {
    P::routes().iter().find(|r| r.kind == kind).map(|r| r.id)
}

fn path<P: VenueProtocol>(role: ConnRole, route: RouteId, vc: &VenueCfg) -> Result<String, String> {
    let mut ep = EndpointSpec::default();
    P::endpoint(role, route, vc, None, None, 0, &mut ep).map_err(|e| format!("endpoint {role:?}: {e:?}"))?;
    Ok(String::from_utf8_lossy(ep.path_query.as_slice()).into_owned())
}

/// `NetConfig` of the run: endpoints from the configuration hosts and `P::endpoint` paths.
pub fn net_config<P: VenueProtocol>(cfg: &Config) -> Result<NetConfig, String> {
    let vc = venue_cfg(VenueEnv::Prod);
    let bbo = route::<P>(RouteKind::Bbo).ok_or("no Bbo route")?;
    let depth = route::<P>(RouteKind::DepthTopN).ok_or("no DepthTopN route")?;
    let ep = |host: &str, port: u16, path: String, ips: &[Ipv4Addr]| Endpoint {
        host: host.to_string(),
        sni: host.to_string(),
        port,
        path,
        ips: ips.to_vec(),
        upgrade_headers: Vec::new(),
    };
    let order_path = path::<P>(ConnRole::Order, RouteId(0), &vc)?;
    let order_host = if cfg.orders_host.is_empty() {
        // Orders disabled without a host: an unused placeholder (never built).
        cfg.feed_host.clone()
    } else {
        cfg.orders_host.clone()
    };
    let src = cfg.src_ip.ok_or("net.src_ip is required")?;
    let dev = cfg.bind_device.clone().ok_or("net.bind_device is required")?;
    Ok(NetConfig {
        endpoints: vec![
            ep(
                &cfg.feed_host,
                cfg.feed_port,
                path::<P>(ConnRole::Md, bbo, &vc)?,
                &cfg.feed_ips,
            ),
            ep(
                &cfg.feed_host,
                cfg.feed_port,
                path::<P>(ConnRole::Md, depth, &vc)?,
                &cfg.feed_ips,
            ),
            ep(&order_host, cfg.orders_port, order_path, &cfg.orders_ips),
        ],
        src_ips: vec![src],
        bind_device: vec![(src, dev)],
        ts_expect_hw: cfg.ts_expect_hw,
        ..NetConfig::default()
    })
}

pub fn venue_cfg(env: VenueEnv) -> VenueCfg {
    VenueCfg {
        env,
        stale_guard_ms: 1_500,
    }
}

/// The arena: market-data, order, user-stream and REST slots.
pub fn arena(net: &NetConfig, clock: TscClock, md: u16) -> &'static ConnArena {
    ConnArena::new(
        &[
            (ConnRole::Md, md),
            (ConnRole::Order, 4),
            (ConnRole::UserStream, 2),
            (ConnRole::Rest, 1),
        ],
        net,
        clock,
    )
}

/// The session credential of the order account (Ed25519).
pub fn credential(k: &Keys) -> Result<Credential, String> {
    Ok(Credential {
        api_key: ArrayBuf::from_bytes(k.api_key.as_bytes()).ok_or("API key too long")?,
        secret: SecretBuf(ArrayBuf::from_bytes(&k.seed).ok_or("seed")?),
        passphrase: ArrayBuf::new(),
        alg: SignAlg::Ed25519,
    })
}

/// One task served by the conn-builder.
#[derive(Debug)]
pub struct Client {
    pub build: Consumer<BuildReq>,
    pub handoff: Producer<Handover>,
    pub retire: Consumer<RetireMsg>,
    pub acct: Option<AccountId>,
}

/// Owner of a connection id: BBO slots → feed-bbo, the market range → ballast, the rest → gw.
fn owner(id: ConnId, feed: usize, ballast: usize, gw: usize) -> usize {
    if id.0 < MAX_BBO_CONN_SLOTS {
        feed
    } else if id.0 <= MD_LAST {
        ballast
    } else {
        gw
    }
}

/// The conn-builder loop (`warm` thread). Clients: 0 = feed-bbo, 1 = gw, 2 = ballast (optional).
pub fn warm_loop<P: VenueProtocol>(
    mut c: Connector,
    mut d: SessionDriver<P>,
    mut clients: Vec<Client>,
    stops: Arc<Stops>,
    policy: Idle,
) -> u64 {
    let mut held_user_stream = 0u64;
    let mut idle_r = 0u32;
    let (feed, gw) = (0usize, 1usize);
    let ballast = if clients.len() > 2 { 2 } else { feed };
    loop {
        if stops.warm.load(Ordering::Acquire) {
            break;
        }
        let now = utc_ns();
        let mut did = false;
        for cl in clients.iter_mut() {
            while let Some(req) = cl.build.try_pop() {
                did = true;
                if req.role == ConnRole::UserStream {
                    // README "已知限制与后续工作": the USDT-M listenKey stream is not built; the
                    // request stays pending (no hand-over, no failure).
                    held_user_stream += 1;
                    continue;
                }
                let acct = cl.acct;
                if d.start(&mut c, req, acct, now).is_err() {
                    let _ = cl.handoff.try_push(Handover::Failed(req.conn));
                }
            }
            while let Some(m) = cl.retire.try_pop() {
                did = true;
                d.retire(&mut c, m);
            }
        }
        for _ in 0..16 {
            let Some(ev) = d.drive(&mut c, now) else { break };
            did = true;
            let (id, h) = match ev {
                SessEvent::Ready(h) => (h.id(), Handover::Ready(h)),
                SessEvent::Failed { conn, code } => {
                    if code != 0 {
                        eprintln!("latency-slice: login of conn {} rejected: code {code:#x}", conn.0);
                    }
                    (conn, Handover::Failed(conn))
                }
            };
            let k = owner(id, feed, ballast, gw).min(clients.len().saturating_sub(1));
            let _ = clients[k].handoff.try_push(h);
        }
        if did {
            idle_r = 0;
        } else {
            idle(policy, &mut idle_r);
        }
    }
    held_user_stream
}

/// Builds the `SessionDriver` of the run.
pub fn driver<P: VenueProtocol>(
    keys: Option<&Keys>,
    acct: AccountId,
    ready_bbo: Option<hft_net::ReadyHandle>,
) -> Result<SessionDriver<P>, String> {
    let creds = match keys {
        Some(k) => vec![(acct, credential(k)?)],
        None => Vec::new(),
    };
    Ok(SessionDriver::new(
        SessionCfg {
            venue: venue_cfg(VenueEnv::Testnet),
            ready_bbo,
            ready_md: None,
        },
        creds,
    ))
}

// ------------------------------------------------------------------------------------ ballast

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum BState {
    Idle,
    Building,
    Up,
}

#[derive(Debug)]
struct BConn {
    id: ConnId,
    pool: ConnPool<2>,
    st: BState,
    frame_ix: usize,
    next_send_at: u64,
    retry_at: u64,
}

/// Ballast depth connections (README "线程布局" `feed-md`): subscribe every ballast inst's
/// `DepthTopN` stream and read without decoding.
#[derive(Debug)]
pub struct Ballast {
    conns: Vec<BConn>,
    frames: SubFrames,
    build: Producer<BuildReq>,
    handoff: Consumer<Handover>,
    retire: Producer<RetireMsg>,
    venue: VenueId,
    ticks_per_ms: u64,
    pub frames_rx: u64,
    pub bytes_rx: u64,
    pub drops: u64,
}

impl Ballast {
    /// `n` connections; `insts` = the ballast insts (DepthTopN 20 levels each).
    #[allow(clippy::too_many_arguments)]
    pub fn new<P: VenueProtocol>(
        n: u8,
        rd: &RefData,
        insts: &[InstId],
        build: Producer<BuildReq>,
        handoff: Consumer<Handover>,
        retire: Producer<RetireMsg>,
        ticks_per_ms: u64,
    ) -> Result<Ballast, String> {
        let route = route::<P>(RouteKind::DepthTopN).ok_or("no DepthTopN route")?;
        let subs: Vec<SubSpec> = insts
            .iter()
            .map(|&inst| SubSpec {
                inst,
                stream: StreamKind::DepthTopN,
                levels: 20,
                route,
            })
            .collect();
        let mut frames = SubFrames::with_capacity(1 << 20, 64);
        let mut start = 0usize;
        let mut one = SubFrames::with_capacity(16 * 1024, 4);
        while start < subs.len() {
            one.clear();
            P::subscribe(rd, &subs[start..], false, &venue_cfg(VenueEnv::Prod), &mut one)
                .map_err(|e| format!("ballast subscribe: {e:?}"))?;
            let (t, covered) = one.get(0).ok_or("ballast: no frame")?;
            frames.push(t, covered).map_err(|e| format!("{e:?}"))?;
            start += usize::from(covered.max(1));
        }
        let conns = (0..n)
            .map(|k| BConn {
                id: ConnId(MD_FIRST + k),
                pool: ConnPool::new(PoolMode::Race, 1, RotatePolicy::default()),
                st: BState::Idle,
                frame_ix: 0,
                next_send_at: 0,
                retry_at: 0,
            })
            .collect();
        Ok(Ballast {
            conns,
            frames,
            build,
            handoff,
            retire,
            venue: P::VENUE,
            ticks_per_ms,
            frames_rx: 0,
            bytes_rx: 0,
            drops: 0,
        })
    }

    /// One round; whether anything was read or changed.
    pub fn poll(&mut self, now: u64) -> bool {
        let mut did = false;
        while let Some(h) = self.handoff.try_pop() {
            did = true;
            match h {
                Handover::Ready(h) => {
                    if let Some(c) = self.conns.iter_mut().find(|c| c.id == h.id())
                        && c.pool.install(h, None).is_ok()
                    {
                        c.st = BState::Up;
                        c.frame_ix = 0;
                        c.next_send_at = now;
                    }
                }
                Handover::Failed(id) => {
                    if let Some(c) = self.conns.iter_mut().find(|c| c.id == id) {
                        c.st = BState::Idle;
                        c.retry_at = now + 2_000 * self.ticks_per_ms;
                    }
                }
            }
        }
        for c in &mut self.conns {
            match c.st {
                BState::Idle if now >= c.retry_at => {
                    let req = BuildReq {
                        venue: self.venue,
                        conn: c.id,
                        role: ConnRole::Md,
                        endpoint: EP_DEPTH,
                        src_ip_idx: 0,
                        rx_queue: 0,
                        deadline_ns: utc_ns() + 10_000_000_000,
                    };
                    if self.build.try_push(req).is_ok() {
                        c.st = BState::Building;
                    }
                }
                BState::Up => {
                    let id = c.id;
                    let mut fatal = ErrBits::empty();
                    if let Some(conn) = c.pool.get(id) {
                        if c.frame_ix < self.frames.len() && now >= c.next_send_at {
                            if let Some((t, _)) = self.frames.get(c.frame_ix)
                                && conn.send_text(t, now).is_ok()
                            {
                                c.frame_ix += 1;
                            }
                            // ≤ 4 subscription frames per second (exchange inbound limit / 2).
                            c.next_send_at = now + 250 * self.ticks_per_ms;
                        }
                        if conn.poll_rx(now) > 0 {
                            did = true;
                        }
                        while let Some(f) = conn.next_frame() {
                            self.frames_rx += 1;
                            self.bytes_rx += f.payload.len() as u64;
                        }
                        conn.finish_poll(now);
                        fatal = conn.err() & ErrBits::FATAL;
                    }
                    if !fatal.is_empty() {
                        self.drops += 1;
                        if let Ok(m) = c.pool.retire(id, fatal) {
                            let _ = self.retire.try_push(m);
                        }
                        c.st = BState::Idle;
                        c.retry_at = now + 2_000 * self.ticks_per_ms;
                    }
                }
                _ => {}
            }
        }
        did
    }

    /// Retires every connection (end of run).
    pub fn retire_all(&mut self) {
        for c in &mut self.conns {
            if c.st == BState::Up
                && let Ok(m) = c.pool.retire(c.id, ErrBits::empty())
            {
                let _ = self.retire.try_push(m);
            }
            c.st = BState::Idle;
        }
    }
}
