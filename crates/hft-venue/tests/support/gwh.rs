//! gw harness: `GwRuntime<StubProto>` with the `G0` configuration (TESTS.md "基准配置"): accounts
//! A1 (ix 0) and A2 (ix 11), a leaked kill word, rings, a raw-gw journal, `ScriptNet`/`BuildStub`.
#![allow(dead_code)]

use std::sync::atomic::{AtomicU32, Ordering};

use hft_event::{GwCmd, GwOp, OrderFlags, OutMsg, RepMsg, SendReport, SendStatus};
use hft_net::{BuildReq, ConnId, RetireMsg, TxTag};
use hft_rt::{Backing, ByteProducer, CachePadded, Consumer, KillWord, Producer, byte_spsc, spsc};
use hft_types::{AccountId, InstId, KillBits, Px, Qty, Side, SlotId, Tif, Tsc};
use hft_venue::{GwAccountSpec, GwCfg, GwDeps, GwRuntime, Handover, LoopStat};
use hft_venue_proto::{Credential, VenueCfg, VenueEnv, VenueProtocol};

use super::feedh::clock;
use super::jrn::{RawJ, raw_journal};
use super::net::Net;
use super::rd::{A1, A2, rd_stub};
use super::stub::{GwCtx, Tpl};

pub fn g0() -> GwCfg {
    GwCfg::default()
}

pub struct Gh<P: VenueProtocol<Templates = Tpl, GwCtx = GwCtx>, const C: usize = 2048> {
    pub rt: GwRuntime<P, C>,
    pub net: Net,
    pub kill: &'static KillWord,
    pub out: Option<Producer<OutMsg>>,
    pub rep: Consumer<RepMsg>,
    pub acct: ByteProducer,
    pub handoff: Producer<Handover>,
    pub retire: Consumer<RetireMsg>,
    pub build: Consumer<BuildReq>,
    pub tags: Option<Consumer<TxTag>>,
    pub raw: RawJ,
    pub builds: Vec<BuildReq>,
    pub retired: Vec<ConnId>,
    pub last: LoopStat,
    pub seq: u64,
}

#[derive(Copy, Clone, Debug)]
pub struct GhOpts {
    pub rep_cap: usize,
    pub out_cap: usize,
    pub tx_tags: bool,
}

impl Default for GhOpts {
    fn default() -> Self {
        GhOpts {
            rep_cap: 1024,
            out_cap: 1024,
            tx_tags: false,
        }
    }
}

pub fn spec<P: VenueProtocol<Templates = Tpl, GwCtx = GwCtx>>(
    a: AccountId,
    stale_ms: u32,
) -> GwAccountSpec<P> {
    let vc = VenueCfg {
        env: VenueEnv::Prod,
        stale_guard_ms: stale_ms,
    };
    GwAccountSpec {
        account: a,
        src_ip_idx: 0,
        tpl: P::build_templates(&vc, rd_stub(), a, &Credential::default()).unwrap(),
        ctx: GwCtx,
    }
}

pub fn kill_word() -> &'static KillWord {
    Box::leak(Box::new(CachePadded(AtomicU32::new(0))))
}

/// The gw with `accts` and the given config, before any connection.
pub fn gw_with<P: VenueProtocol<Templates = Tpl, GwCtx = GwCtx>, const C: usize>(
    cfg: GwCfg,
    accts: &[AccountId],
    o: GhOpts,
) -> Result<Gh<P, C>, hft_venue::InitErr> {
    gw_rd::<P, C>(cfg, accts, o, rd_stub())
}

pub fn gw_rd<P: VenueProtocol<Templates = Tpl, GwCtx = GwCtx>, const C: usize>(
    cfg: GwCfg,
    accts: &[AccountId],
    o: GhOpts,
    rd: &'static hft_refdata::RefData,
) -> Result<Gh<P, C>, hft_venue::InitErr> {
    let net = Net::new(4, 40);
    let (w, raw) = raw_journal("raw-gw-v7", 4);
    let (out_p, out_c) = spsc::<OutMsg>(o.out_cap, Backing::Heap).unwrap();
    let (rep_p, rep_c) = spsc::<RepMsg>(o.rep_cap, Backing::Heap).unwrap();
    let (acct_p, acct_c) = byte_spsc(1 << 20, Backing::Heap).unwrap();
    let (h_p, h_c) = spsc::<Handover>(64, Backing::Heap).unwrap();
    let (r_p, r_c) = spsc::<RetireMsg>(64, Backing::Heap).unwrap();
    let (b_p, b_c) = spsc::<BuildReq>(64, Backing::Heap).unwrap();
    let (t_p, t_c) = spsc::<TxTag>(1024, Backing::Heap).unwrap();
    let kill = kill_word();
    let deps = GwDeps {
        rd,
        cmds: out_c,
        out: rep_p,
        acct: acct_c,
        raw: w,
        kill,
        kill_mask: KillBits::GLOBAL.0 | KillBits::VENUE.0 | KillBits::HARD.0,
        tx_tags: o.tx_tags.then_some(t_p),
        handoff: h_c,
        retire: r_p,
        build: b_p,
        clock: clock(),
    };
    let specs = accts.iter().map(|&a| spec::<P>(a, cfg.stale_guard_ms)).collect();
    let rt = GwRuntime::<P, C>::new(cfg, deps, specs)?;
    Ok(Gh {
        rt,
        net,
        kill,
        out: Some(out_p),
        rep: rep_c,
        acct: acct_p,
        handoff: h_p,
        retire: r_c,
        build: b_c,
        tags: o.tx_tags.then_some(t_c),
        raw,
        builds: Vec::new(),
        retired: Vec::new(),
        last: LoopStat::default(),
        seq: 1000,
    })
}

/// `G0` with A1, A2, both connections of both accounts open and `SetEpoch{5}` processed.
pub fn gw<P: VenueProtocol<Templates = Tpl, GwCtx = GwCtx>>() -> Gh<P> {
    gw_opts(GhOpts::default())
}

pub fn gw_opts<P: VenueProtocol<Templates = Tpl, GwCtx = GwCtx>>(o: GhOpts) -> Gh<P> {
    gw_full::<P>(g0(), &[A1, A2], o)
}

/// `cfg` with `accts`, every account's primary and standby open, `SetEpoch{5}` processed.
pub fn gw_full<P: VenueProtocol<Templates = Tpl, GwCtx = GwCtx>>(
    cfg: GwCfg,
    accts: &[AccountId],
    o: GhOpts,
) -> Gh<P> {
    let mut h = gw_with::<P, 2048>(cfg, accts, o).unwrap();
    let per = if P::USER_STREAM { 3 } else { 2 };
    h.open(1_000, per * accts.len());
    h.cmd(GwCmd::set_epoch(5, false, 0));
    h.poll(2_000);
    h.reps();
    h
}

impl<P: VenueProtocol<Templates = Tpl, GwCtx = GwCtx>, const C: usize> Drop for Gh<P, C> {
    fn drop(&mut self) {
        self.rt.retire_all(0);
        self.net.reap(&mut self.retire);
    }
}

impl<P: VenueProtocol<Templates = Tpl, GwCtx = GwCtx>, const C: usize> Gh<P, C> {
    pub fn poll(&mut self, now: u64) -> LoopStat {
        self.net.flush(&mut self.rt, now);
        let ls = self.rt.poll_once(now);
        self.last = ls;
        while let Some(b) = self.build.try_pop() {
            self.builds.push(b);
        }
        self.retired.extend(self.net.reap(&mut self.retire));
        ls
    }

    /// Completes builds until `n` connections were handed over.
    pub fn open(&mut self, now: u64, n: usize) {
        let mut done = 0;
        for _ in 0..100 {
            self.poll(now);
            for b in std::mem::take(&mut self.builds) {
                let h = self.net.complete(b);
                self.handoff.try_push(Handover::Ready(h)).unwrap();
                done += 1;
            }
            if done >= n {
                self.poll(now);
                return;
            }
        }
        panic!("gw did not open {n} connections");
    }

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

    pub fn cmd(&mut self, c: GwCmd) {
        self.out.as_mut().expect("out ring taken").try_push(c).unwrap();
    }

    pub fn reps(&mut self) -> Vec<RepMsg> {
        let mut v = Vec::new();
        while let Some(m) = self.rep.try_pop() {
            v.push(m);
        }
        v
    }

    pub fn set_kill(&mut self, w: u32) {
        self.kill.store(w, Ordering::Release);
    }

    pub fn next_seq(&mut self) -> u64 {
        self.seq += 1;
        self.seq
    }

    pub fn place(&mut self, account: AccountId, slot: u32, r#gen: u32, inst: InstId, rr: bool) -> GwCmd {
        let mut c = base(GwOp::Place, account);
        c.decision_seq = self.next_seq();
        c.slot = SlotId(slot);
        c.r#gen = r#gen;
        c.inst = inst;
        c.px = Px(10_000);
        c.qty = Qty(10_000);
        if rr {
            c.oflags = OrderFlags::RISK_REDUCING.0;
        }
        c
    }

    pub fn targeted(&mut self, op: GwOp, account: AccountId, target: u32) -> GwCmd {
        let mut c = base(op, account);
        c.decision_seq = self.next_seq();
        c.target_slot = SlotId(target);
        c
    }
}

pub fn base(op: GwOp, account: AccountId) -> GwCmd {
    GwCmd {
        decision_seq: 0,
        decided_tsc: Tsc(0),
        px: Px(0),
        qty: Qty(0),
        inst: InstId(0),
        slot: SlotId(0),
        r#gen: 0,
        target_slot: SlotId(0),
        op,
        account,
        side: Side::Buy,
        tif: Tif::Gtc,
        oflags: 0,
        _pad: [0; 2],
        arg: 0,
        arg2: 0,
    }
}

pub fn reports(v: &[RepMsg]) -> Vec<SendReport> {
    v.iter()
        .filter_map(|m| match m {
            RepMsg::SendReport(r) => Some(*r),
            _ => None,
        })
        .collect()
}

pub fn statuses(v: &[RepMsg]) -> Vec<SendStatus> {
    reports(v).iter().map(|r| r.status).collect()
}

pub fn events(v: &[RepMsg]) -> Vec<hft_event::OrderEvent> {
    v.iter()
        .filter_map(|m| match m {
            RepMsg::OrderEvent(e) => Some(*e),
            _ => None,
        })
        .collect()
}

pub fn conn_states(v: &[RepMsg]) -> Vec<hft_event::ConnState> {
    v.iter()
        .filter_map(|m| match m {
            RepMsg::ConnState(e) => Some(*e),
            _ => None,
        })
        .collect()
}
