//! Feed-task harness: `FeedRuntime<StubProto>` with the `F0` configuration (TESTS.md "基准配置"),
//! its rings (`Taps`), a raw journal, `ScriptNet` and `BuildStub`.
#![allow(dead_code)]

use hft_event::{ConnState, ConnStateKind, FeedCmd, FeedMsg};
use hft_net::{BuildReq, ConnId, RetireMsg};
use hft_rt::{
    Backing, ByteConsumer, ByteProducer, CalibPair, Consumer, Producer, TscClock, TscUtc, byte_spsc, spsc,
};
use hft_types::InstId;
use hft_venue::{FeedCfg, FeedDeps, FeedRoutes, FeedRuntime, Handover, LoopStat, SnapReq, streams};
use hft_venue_proto::VenueProtocol;

use super::jrn::{RawJ, raw_journal};
use super::net::Net;
use super::rd::rd_stub;
use super::stub::StubProto;

/// 1 tick = 1 ns, `utc_ns = tsc`.
pub fn clock() -> TscUtc {
    let c = TscClock::from_hz_milli(1_000_000_000_000, CalibPair::default());
    c.tsc_utc(&CalibPair::default())
}

/// `F0`.
pub fn f0() -> FeedCfg {
    FeedCfg {
        routes: FeedRoutes::All,
        ready: false,
        max_sub_insts: 50,
        ..FeedCfg::default()
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Caps {
    pub bbo: usize,
    pub md: usize,
    pub bulk: usize,
    pub feedctl: usize,
    pub snapreq: usize,
    pub snap: usize,
}

impl Default for Caps {
    fn default() -> Self {
        Caps {
            bbo: 1024,
            md: 1024,
            bulk: 1 << 20,
            feedctl: 256,
            snapreq: 64,
            snap: 256 * 1024,
        }
    }
}

pub struct Fh<P: VenueProtocol<FeedCtx = super::stub::FeedCtx> = StubProto> {
    pub rt: FeedRuntime<P>,
    pub net: Net,
    pub bbo: Option<Consumer<FeedMsg>>,
    pub md: Option<Consumer<FeedMsg>>,
    pub bulk: Option<ByteConsumer>,
    pub cmd: Producer<FeedCmd>,
    pub snapreq: Option<Consumer<SnapReq>>,
    pub snap: Option<ByteProducer>,
    pub handoff: Producer<Handover>,
    pub retire: Consumer<RetireMsg>,
    pub build: Consumer<BuildReq>,
    pub raw: RawJ,
    /// Every `BuildReq` seen and not yet completed/failed.
    pub builds: Vec<BuildReq>,
    pub all_builds: Vec<BuildReq>,
    pub retired: Vec<ConnId>,
    pub last: LoopStat,
}

pub fn feed(cfg: FeedCfg) -> Fh {
    feed_with::<StubProto>(cfg, Caps::default())
}

pub fn feed_with<P: VenueProtocol<FeedCtx = super::stub::FeedCtx>>(cfg: FeedCfg, caps: Caps) -> Fh<P> {
    feed_net(cfg, caps, Net::new(64, 4))
}

pub fn feed_net<P: VenueProtocol<FeedCtx = super::stub::FeedCtx>>(
    cfg: FeedCfg,
    caps: Caps,
    net: Net,
) -> Fh<P> {
    try_feed(cfg, caps, net, rd_stub()).unwrap()
}

/// Builds the harness on `rd`; a constructor failure is returned.
pub fn try_feed<P: VenueProtocol<FeedCtx = super::stub::FeedCtx>>(
    cfg: FeedCfg,
    caps: Caps,
    net: Net,
    rd: &'static hft_refdata::RefData,
) -> Result<Fh<P>, hft_venue::InitErr> {
    try_feed_deps(cfg, caps, net, rd, |_| {})
}
pub fn try_feed_deps<P: VenueProtocol<FeedCtx = super::stub::FeedCtx>>(
    cfg: FeedCfg,
    caps: Caps,
    mut net: Net,
    rd: &'static hft_refdata::RefData,
    edit: impl FnOnce(&mut FeedDeps),
) -> Result<Fh<P>, hft_venue::InitErr> {
    let (w, raw) = raw_journal(
        if cfg.routes == FeedRoutes::Bbo {
            "raw-feed-v7-bbo"
        } else {
            "raw-feed-v7"
        },
        4,
    );
    let (bbo_p, bbo_c) = spsc::<FeedMsg>(caps.bbo, Backing::Heap).unwrap();
    let (md_p, md_c) = spsc::<FeedMsg>(caps.md, Backing::Heap).unwrap();
    let (bulk_p, bulk_c) = byte_spsc(caps.bulk, Backing::Heap).unwrap();
    let (cmd_p, cmd_c) = spsc::<FeedCmd>(caps.feedctl, Backing::Heap).unwrap();
    let (sr_p, sr_c) = spsc::<SnapReq>(caps.snapreq, Backing::Heap).unwrap();
    let (sn_p, sn_c) = byte_spsc(caps.snap, Backing::Heap).unwrap();
    let (h_p, h_c) = spsc::<Handover>(64, Backing::Heap).unwrap();
    let (r_p, r_c) = spsc::<RetireMsg>(64, Backing::Heap).unwrap();
    let (b_p, b_c) = spsc::<BuildReq>(64, Backing::Heap).unwrap();
    let has_bbo = cfg.routes.has_bbo();
    let has_md = cfg.routes.has_md();
    let snap = matches!(cfg.routes, FeedRoutes::Bbo | FeedRoutes::All);
    let ready = cfg.ready.then(|| net.ready_poll());
    let mut deps = FeedDeps {
        rd,
        bbo: has_bbo.then_some(bbo_p),
        md: has_md.then_some(md_p),
        bulk: has_md.then_some(bulk_p),
        cmds: cmd_c,
        snapreq: snap.then_some(sr_p),
        snap: snap.then_some(sn_c),
        ready,
        raw: w,
        handoff: h_c,
        retire: r_p,
        build: b_p,
        clock: clock(),
    };
    edit(&mut deps);
    let rt = FeedRuntime::<P>::new(cfg, deps, Default::default())?;
    Ok(Fh {
        rt,
        net,
        bbo: has_bbo.then_some(bbo_c),
        md: has_md.then_some(md_c),
        bulk: has_md.then_some(bulk_c),
        cmd: cmd_p,
        snapreq: snap.then_some(sr_c),
        snap: snap.then_some(sn_p),
        handoff: h_p,
        retire: r_c,
        build: b_c,
        raw,
        builds: Vec::new(),
        all_builds: Vec::new(),
        retired: Vec::new(),
        last: LoopStat::default(),
    })
}

impl<P: VenueProtocol<FeedCtx = super::stub::FeedCtx>> Drop for Fh<P> {
    fn drop(&mut self) {
        // Give every connection back so the connector (and its leaked arena) can be reused.
        self.rt.retire_all(0);
        self.net.reap(&mut self.retire);
    }
}

impl<P: VenueProtocol<FeedCtx = super::stub::FeedCtx>> Fh<P> {
    /// Delivers due events, runs one `poll_once`, collects builds and retirements.
    pub fn poll(&mut self, now: u64) -> LoopStat {
        self.net.flush(&mut self.rt, now);
        let ls = self.rt.poll_once(now);
        self.last = ls;
        while let Some(b) = self.build.try_pop() {
            self.builds.push(b);
            self.all_builds.push(b);
        }
        self.retired.extend(self.net.reap(&mut self.retire));
        ls
    }

    pub fn sub(&mut self, inst: InstId, s: u8) {
        self.cmd
            .try_push(FeedCmd::Subscribe { inst, streams: s })
            .unwrap();
    }

    pub fn sub_bbo(&mut self, insts: &[InstId]) {
        for &i in insts {
            self.sub(i, streams::BBO);
        }
    }

    /// Completes every outstanding build (hand-over in the next poll).
    pub fn complete_builds(&mut self) -> Vec<ConnId> {
        let mut ids = Vec::new();
        for b in std::mem::take(&mut self.builds) {
            let h = self.net.complete(b);
            ids.push(b.conn);
            self.handoff.try_push(Handover::Ready(h)).unwrap();
        }
        ids
    }

    /// Completes the outstanding build for `conn` only.
    pub fn complete(&mut self, conn: ConnId) {
        let i = self
            .builds
            .iter()
            .position(|b| b.conn == conn)
            .expect("no such build");
        let b = self.builds.remove(i);
        let h = self.net.complete(b);
        self.handoff.try_push(Handover::Ready(h)).unwrap();
    }

    pub fn fail(&mut self, conn: ConnId) {
        let i = self
            .builds
            .iter()
            .position(|b| b.conn == conn)
            .expect("no such build");
        self.builds.remove(i);
        self.handoff.try_push(Handover::Failed(conn)).unwrap();
    }

    /// Answers every subscription frame sent on `conn` with `ACK|n|1|0`.
    pub fn ack(&mut self, conn: ConnId) -> usize {
        let frames = self.net.sent_text(&mut self.rt, conn);
        let mut n = 0;
        for f in frames {
            if f.starts_with("SUB|") || f.starts_with("UNSUB|") {
                n += 1;
                self.net.push(conn, format!("ACK|{n}|1|0").as_bytes());
            }
        }
        n
    }

    /// Subscribes, builds, acks until every connection is up and every subscription answered;
    /// time advances 1 µs per round (subscription pacing is not exercised: few insts per frame).
    pub fn up(&mut self, insts: &[InstId], s: u8, now: &mut u64) {
        for &i in insts {
            self.sub(i, s);
        }
        for _ in 0..20_000 {
            self.poll(*now);
            let built = self.complete_builds();
            let ids: Vec<ConnId> = self.rt.conn_ids().collect();
            let mut acked = 0;
            for id in ids {
                acked += self.ack(id);
            }
            if built.is_empty() && acked == 0 && self.builds.is_empty() && self.rt.pending_subs() == 0 {
                return;
            }
            *now += 1_000;
        }
        panic!("feed did not settle");
    }

    pub fn drain_bbo(&mut self) -> Vec<FeedMsg> {
        let mut v = Vec::new();
        if let Some(c) = self.bbo.as_mut() {
            while let Some(m) = c.try_pop() {
                v.push(m);
            }
        }
        v
    }

    pub fn drain_md(&mut self) -> Vec<FeedMsg> {
        let mut v = Vec::new();
        if let Some(c) = self.md.as_mut() {
            while let Some(m) = c.try_pop() {
                v.push(m);
            }
        }
        v
    }

    /// Clears every tap.
    pub fn clear(&mut self) {
        self.drain_bbo();
        self.drain_md();
        if let Some(b) = self.bulk.as_mut() {
            while b.peek().is_some() {
                b.advance();
            }
        }
    }
}

/// `F0` with `routes = Bbo` and `f` applied; `insts` subscribed (BBO) and every connection up and
/// answered; the taps are cleared. Returns the harness and the time reached.
pub fn bbo_up(f: impl FnOnce(&mut FeedCfg), insts: &[InstId]) -> (Fh, u64) {
    let mut cfg = FeedCfg {
        routes: FeedRoutes::Bbo,
        ..f0()
    };
    f(&mut cfg);
    let mut h = feed(cfg);
    let mut now = 100;
    h.up(insts, streams::BBO, &mut now);
    h.clear();
    super::stub::take_log();
    (h, now)
}

/// Helpers over ring elements.
pub fn states(v: &[FeedMsg]) -> Vec<ConnState> {
    v.iter()
        .filter_map(|m| match m {
            FeedMsg::ConnState(s) => Some(*s),
            _ => None,
        })
        .collect()
}

pub fn bbos(v: &[FeedMsg]) -> Vec<hft_event::Bbo> {
    v.iter()
        .filter_map(|m| match m {
            FeedMsg::Bbo(b, _) => Some(*b),
            _ => None,
        })
        .collect()
}

pub fn has_state(v: &[FeedMsg], conn: u8, state: ConnStateKind, inst: InstId, reason: u32) -> bool {
    states(v)
        .iter()
        .any(|s| s.conn == conn && s.state == state && s.inst == inst && s.reason == reason)
}

/// `B|<sym>|<u>|…` with fixed prices.
pub fn b(sym: &str, u: u64) -> Vec<u8> {
    format!("B|{sym}|{u}|100.00|1.000|100.01|2.000").into_bytes()
}
