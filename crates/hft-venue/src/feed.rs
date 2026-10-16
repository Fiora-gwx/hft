//! `FeedRuntime<P>`: one feed task's loop body (README §2 fixed order, §3 dedup, §3a BBO connection
//! health, §3c TopN depth, §1 connection numbers and full rings).

use hft_rt::latency_trace::{self as lt, Stage as TraceStage};
use std::collections::VecDeque;

use hft_event::{ConnRole, ConnState, ConnStateKind, EventKind, FeedCmd, FeedMsg, L2SnapHdr};
use hft_journal::JournalWriter;
use hft_net::{
    BuildReq, Conn, ConnId, ConnPool, ConnSet, EndpointId, ErrBits, Frame, Opcode, PoolMode, ReadyPoll,
    RetireMsg, RotatePolicy, SysCount,
};
use hft_refdata::RefData;
use hft_rt::{ByteConsumer, ByteProducer, Consumer, PollTask, Polled, Producer, TscUtc, WaitFds};
use hft_types::{AccountId, InstId, MAX_BBO_CONN_SLOTS, Tsc, VenueId};
use hft_venue_proto::{
    ControlHint, DecodeErr, MsgOp, PingKind, RouteId, RouteKind, RouteSpec, RxMeta, StreamKind, SubFrames,
    SubSpec, VenueProtocol,
};
use zerocopy::FromBytes;

use crate::codes;
use crate::config::{DepthMode, FeedCfg, FeedRoutes, InitErr, RawMode, Ticks};
use crate::conn_id::{BboSlots, IdRange, MD_FIRST, MD_LAST};
use crate::hb::{Hb, HbSend};
use crate::raw::{Effect, RawCounters, RawLog, what};
use crate::seq::{InstSeq, Verdict, judge, slot_of};
use crate::sink::FeedSink;
use crate::snap::{SnapRecHdr, SnapReq};
use crate::{FEEDCMD_PER_POLL, FRAME_EVENTS_MAX, Handover, LoopStat, min_deadline, streams};

/// Rings, journal and clock of one feed task (README "运行时入口").
#[derive(Debug)]
pub struct FeedDeps {
    pub rd: &'static RefData,
    /// → `bbo-<venue>` (Bbo and the BBO connections' `ConnState`); routes ∈ {Bbo, All}.
    pub bbo: Option<Producer<FeedMsg>>,
    /// → `md-<venue>`; routes ∈ {Md, All}.
    pub md: Option<Producer<FeedMsg>>,
    /// → `md-<venue>-bulk` (variable `L2Snapshot` bodies); routes ∈ {Md, All}.
    pub bulk: Option<ByteProducer>,
    /// ← `feedctl-<venue>` or `feedctl-<venue>-bbo`.
    pub cmds: Consumer<FeedCmd>,
    /// → snapreq ring (BBO refresh); existence rule README §1.
    pub snapreq: Option<Producer<SnapReq>>,
    /// ← snap ring.
    pub snap: Option<ByteConsumer>,
    /// Readiness probe, only when `feed.ready_poll` is in effect.
    pub ready: Option<ReadyPoll>,
    /// raw-feed journal; this task is its single writer.
    pub raw: JournalWriter,
    pub handoff: Consumer<Handover>,
    pub retire: Producer<RetireMsg>,
    pub build: Producer<BuildReq>,
    pub clock: TscUtc,
}

/// Counters of one feed task (summed into telemetry each second by hft-core).
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct FeedCounters {
    /// Frames of insts this task does not carry (`peek` → inst 0 or not subscribed).
    pub unsubscribed: u64,
    /// Classes a route does not know.
    pub unknown_class: u64,
    /// Duplicates dropped by the dedup rule.
    pub dup: u64,
    pub gap: u64,
    /// Decode failures by route kind: Bbo, DepthTopN, Md.
    pub decode_err: [u64; 3],
    pub frame_events_over: u64,
    pub cmd_bad_inst: u64,
    pub cmd_ignored: u64,
    pub ring_full: u64,
    /// Subscription answers without an outstanding subscription frame.
    pub stray_ack: u64,
    /// A group had no room for a new inst (more than `max_sub_insts`).
    pub no_group: u64,
    /// Status records waiting for ring space.
    pub status_deferred: u64,
    pub status_lost: u64,
    pub sub_queue_full: u64,
    pub install_failed: u64,
    pub snapshot_stale: u64,
    pub snapshot_failed: u64,
}

const NONE8: u8 = u8::MAX;
const REFRESH_NONE: u8 = 0;
const REFRESH_NEEDED: u8 = 1;
const REFRESH_ACTIVE: u8 = 2;
/// Status records waiting for ring space (bounded; overflow is counted, never expected).
const PEND_CAP: usize = 512;
/// Subscription items queued or awaiting answers per connection.
const SUBQ_CAP: usize = 4096;
/// Subscription entries per `subscribe` call.
const SUBS_PER_CALL: usize = 64;

/// Per-inst state of this task.
#[derive(Copy, Clone, Debug)]
struct Inst {
    id: InstId,
    /// Stream bits wanted (this task's share).
    want: u8,
    bbo_group: u8,
    depth_group: u8,
    md_group: [u8; MD_STREAMS.len()],
    /// BBO slots where the subscription is answered (member).
    members: u8,
    refresh: u8,
    refresh_due: u64,
    refresh_req: u32,
    refresh_attempt: u16,
    /// Per-task subscription ids (`RawHdr.stream`): BBO, depth, md.
    sub_id: [u32; 3],
    sample_tsc: Option<u64>,
}

#[derive(Copy, Clone, Debug)]
struct SubItem {
    ix: u32,
    stream: StreamKind,
    levels: u8,
    unsub: bool,
}

#[derive(Copy, Clone, Debug)]
struct Build {
    kind: RouteKind,
    group: u8,
    replaces: Option<ConnId>,
}

/// Mutable state of one connection (disjoint from its pool so frames can borrow the pool).
#[derive(Debug)]
struct Meta {
    id: ConnId,
    kind: RouteKind,
    route: RouteId,
    group: u8,
    fd: i32,
    installed: u64,
    replaces: Option<ConnId>,
    replaced_by: Option<ConnId>,
    rotate: bool,
    hint: bool,
    got_data: bool,
    last_data: u64,
    pend: VecDeque<SubItem>,
    acks: VecDeque<(u16, bool)>,
    ack_items: VecDeque<SubItem>,
    next_sub: u64,
    sub_period: u64,
    pub_tsc: u64,
    probe_tsc: u64,
    pong_seen: u64,
    hb: Option<Hb>,
    /// Per-inst membership on this connection (local index bitset).
    insts: Vec<u64>,
    left: u16,
    done: bool,
    buffered: bool,
}

#[derive(Debug)]
struct FConn {
    pool: ConnPool<1>,
    m: Meta,
}

/// Everything but the connections.
struct St<P: VenueProtocol> {
    cfg: FeedCfg,
    rd: &'static RefData,
    ctx: P::FeedCtx,
    t: Ticks,
    ids: Box<[InstId]>,
    insts: Vec<Inst>,
    seqs: Vec<InstSeq>,
    bbo: Option<Producer<FeedMsg>>,
    md: Option<Producer<FeedMsg>>,
    bulk: Option<ByteProducer>,
    pend_bbo: VecDeque<FeedMsg>,
    pend_md: VecDeque<FeedMsg>,
    raw: RawLog,
    sink: FeedSink,
    n: FeedCounters,
    next_sub_id: u32,
    bbo_route: Option<RouteSpec>,
    depth_route: Option<RouteSpec>,
    md_routes: Vec<Option<RouteSpec>>,
    topn: u8,
    spc: u32,
    max_groups: u8,
    bbo_load: Vec<u32>,
    depth_load: Vec<u32>,
    md_load: Vec<u32>,
}

/// One feed task (README "运行时入口").
pub struct FeedRuntime<P: VenueProtocol> {
    st: St<P>,
    conns: Vec<Option<FConn>>,
    building: Vec<Option<Build>>,
    slots: BboSlots,
    md_ids: IdRange,
    cmds: Consumer<FeedCmd>,
    /// BBO refresh requests (sent from M3 on, with snap-fetch).
    snapreq: Option<Producer<SnapReq>>,
    snapshot_supported: bool,
    next_snapshot: u32,
    snap: Option<ByteConsumer>,
    ready: Option<ReadyPoll>,
    ready_set: ConnSet,
    handoff: Consumer<Handover>,
    retire: Producer<RetireMsg>,
    retire_pend: VecDeque<RetireMsg>,
    build: Producer<BuildReq>,
    subframes: SubFrames,
    rr: usize,
    order: Vec<ConnId>,
    last: LoopStat,
}

impl<P: VenueProtocol> core::fmt::Debug for FeedRuntime<P> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FeedRuntime")
            .field("venue", &P::VENUE)
            .field("routes", &self.st.cfg.routes)
            .finish()
    }
}

fn status(
    venue: VenueId,
    conn: ConnId,
    state: ConnStateKind,
    inst: InstId,
    reason: u32,
    rx_tsc: u64,
) -> ConnState {
    ConnState {
        venue,
        conn: conn.0,
        state,
        role: ConnRole::Md,
        inst,
        reason,
        account: AccountId(0),
        _pad2: [0; 3],
        rx_tsc,
    }
}

impl<P: VenueProtocol> St<P> {
    fn local(&self, id: InstId) -> Option<usize> {
        self.ids.binary_search(&id).ok()
    }

    /// Status record of a BBO connection (`bbo` ring) or another connection (`md` ring), in order.
    fn emit(&mut self, to_bbo: bool, m: FeedMsg) {
        let (ring, pend) = if to_bbo {
            (&mut self.bbo, &mut self.pend_bbo)
        } else {
            (&mut self.md, &mut self.pend_md)
        };
        let Some(r) = ring.as_mut() else { return };
        self.raw.note(if to_bbo { Effect::Bbo } else { Effect::Md });
        if pend.is_empty() && r.try_push(m).is_ok() {
            crate::latency::feed(TraceStage::FeedPush, &m);
            return;
        }
        if pend.len() < PEND_CAP {
            pend.push_back(m);
            self.n.status_deferred += 1;
        } else {
            self.n.status_lost += 1;
        }
    }

    fn flush_pending(&mut self) {
        for (ring, pend) in [
            (&mut self.bbo, &mut self.pend_bbo),
            (&mut self.md, &mut self.pend_md),
        ] {
            let Some(r) = ring.as_mut() else { continue };
            while let Some(m) = pend.front().copied() {
                if r.try_push(m).is_err() {
                    break;
                }
                crate::latency::feed(TraceStage::FeedPush, &m);
                pend.pop_front();
            }
        }
    }

    fn cs(&mut self, to_bbo: bool, conn: ConnId, state: ConnStateKind, inst: InstId, reason: u32, rx: u64) {
        let s = status(P::VENUE, conn, state, inst, reason, rx);
        self.emit(to_bbo, FeedMsg::ConnState(s));
    }

    fn raw_mode(&self, kind: RouteKind) -> RawMode {
        if kind == RouteKind::Bbo {
            self.cfg.raw_bbo
        } else {
            self.cfg.raw_md
        }
    }

    /// Room for one more frame of `kind` (README §1 "每取一帧之前确认目标环至少有…空位").
    fn has_room(&mut self, kind: RouteKind) -> bool {
        if kind == RouteKind::Bbo {
            return self.pend_bbo.is_empty() && self.bbo.as_ref().is_some_and(|r| r.free_hint() >= 1);
        }
        if !self.pend_md.is_empty()
            || !self
                .md
                .as_ref()
                .is_some_and(|r| r.free_hint() >= FRAME_EVENTS_MAX)
        {
            return false;
        }
        let part = size_of::<L2SnapHdr>() + usize::from(self.cfg.snapshot_part_levels) * 16;
        match self.bulk.as_mut() {
            Some(b) => b.reserve(part as u32).is_some(),
            None => true,
        }
    }

    /// Raw data frame per the route's mode (`dup` frames only with `feed.raw_dups` and `on`).
    #[allow(clippy::too_many_arguments)]
    fn raw_data(
        &mut self,
        kind: RouteKind,
        conn: ConnId,
        op: Opcode,
        stream: u32,
        ix: Option<usize>,
        ts: &hft_net::Timestamps,
        payload: &[u8],
        dup: bool,
    ) {
        let mode = self.raw_mode(kind);
        let mut rflags = 0u8;
        match mode {
            RawMode::Off => return,
            RawMode::On => {
                if dup && !self.cfg.raw_dups {
                    return;
                }
            }
            RawMode::Sampled => {
                if dup {
                    return;
                }
                let every = self.t.ms(u64::from(self.cfg.raw_sample_ms));
                if let Some(i) = ix.and_then(|i| self.insts.get_mut(i)) {
                    if i.sample_tsc
                        .is_some_and(|last| ts.rx_tsc.saturating_sub(last) < every)
                    {
                        return;
                    }
                    i.sample_tsc = Some(ts.rx_tsc);
                }
                rflags = hft_event::RawHdr::SAMPLED;
            }
        }
        let w = if op == Opcode::Binary {
            what::WS_BINARY
        } else {
            what::WS_TEXT
        };
        self.raw.rx(
            conn,
            w,
            rflags,
            stream,
            ts.best_rx().0,
            ts.flags,
            payload,
            ts.rx_tsc,
        );
    }

    /// Publishes the sink's events; `false` = no room (the caller checked, so this is not expected).
    fn publish(&mut self) {
        let mut bulk_off = 0usize;
        for k in 0..self.sink.n {
            let Some(m) = self.sink.evs.get(k).copied() else {
                break;
            };
            crate::latency::feed(TraceStage::DecodedEvent, &m);
            match m {
                FeedMsg::Bbo(_, _) => self.emit(true, m),
                FeedMsg::Bulk { kind, len, .. } => {
                    let end = bulk_off + len as usize;
                    if let (Some(b), Some(src)) = (self.bulk.as_mut(), self.sink.bulk.get(bulk_off..end)) {
                        crate::latency::market_bytes(TraceStage::DecodedEvent, kind, src);
                        if let Some(dst) = b.reserve(len) {
                            dst.copy_from_slice(src);
                            b.commit();
                            crate::latency::market_bytes(TraceStage::FeedPush, kind, src);
                        } else {
                            self.n.status_lost += 1;
                            bulk_off = end;
                            continue;
                        }
                    }
                    bulk_off = end;
                    self.emit(false, m);
                }
                _ => self.emit(false, m),
            }
        }
    }

    /// README §2 step 5 for one data/control frame of connection `m`.
    fn frame(&mut self, m: &mut Meta, f: &Frame<'_>) {
        lt::stamp(TraceStage::DecodeStart, [0; 4]);
        let payload = f.payload;
        let ts = f.ts;
        let op = if f.opcode == Opcode::Binary {
            MsgOp::Binary
        } else {
            MsgOp::Text
        };
        let head = payload.get(..payload.len().min(64)).unwrap_or(payload);
        let c = P::classify(m.route, op, head);
        let to_bbo = m.kind == RouteKind::Bbo;
        if let Some(h) = P::control(c, payload) {
            let w = match h {
                ControlHint::SubAck { ok, code } => {
                    self.on_ack(m, ok, code, ts.rx_tsc);
                    what::SUB_ACK
                }
                ControlHint::Pong => {
                    self.on_pong(m, ts.rx_tsc);
                    what::PONG
                }
                ControlHint::ServerShutdown | ControlHint::Maintenance | ControlHint::SessionExpired => {
                    m.hint = true;
                    what::WS_TEXT
                }
            };
            self.raw
                .rx(m.id, w, 0, 0, ts.best_rx().0, ts.flags, payload, ts.rx_tsc);
            return;
        }
        let kix = match m.kind {
            RouteKind::Bbo => 0,
            RouteKind::DepthTopN => 1,
            RouteKind::Md => 2,
        };
        m.last_data = ts.rx_tsc;
        let pk = match P::peek(self.rd, &self.ctx, c, payload) {
            Ok(p) => p,
            Err(DecodeErr::UnknownClass) => {
                self.n.unknown_class += 1;
                self.raw_data(m.kind, m.id, f.opcode, 0, None, &ts, payload, false);
                return;
            }
            Err(_) => {
                if let Some(d) = self.n.decode_err.get_mut(kix) {
                    *d += 1;
                }
                self.raw_data(m.kind, m.id, f.opcode, 0, None, &ts, payload, false);
                return;
            }
        };
        let bit = stream_bit(pk.stream);
        let ix = self
            .local(pk.inst)
            .filter(|&i| self.insts.get(i).is_some_and(|s| s.want & bit != 0));
        let Some(ix) = ix else {
            self.n.unsubscribed += 1;
            self.raw_data(m.kind, m.id, f.opcode, 0, None, &ts, payload, false);
            return;
        };
        let (sub_id, refreshing, inst) = match self.insts.get(ix) {
            Some(s) => (
                s.sub_id[kix],
                pk.stream == StreamKind::Bbo && s.refresh == REFRESH_ACTIVE,
                s.id,
            ),
            None => return,
        };
        let sl = slot_of(pk.stream);
        let last = self
            .seqs
            .get(ix)
            .and_then(|q| q.last_fwd.get(sl).copied())
            .unwrap_or(0);
        match judge(P::seq_rule(c), last, &pk.ids, refreshing) {
            Verdict::Duplicate => {
                self.n.dup += 1;
                self.raw_data(m.kind, m.id, f.opcode, sub_id, Some(ix), &ts, payload, true);
                return;
            }
            Verdict::Gap => {
                self.n.gap += 1;
                self.raw_data(m.kind, m.id, f.opcode, sub_id, Some(ix), &ts, payload, false);
                return;
            }
            Verdict::Forward => {}
        }
        let rx = RxMeta {
            rx_tsc: ts.rx_tsc,
            hw_rx_ns: ts.best_rx().0,
            sw_rx_ns: ts.sw_rx_ns,
            prev_override: 0,
            conn: m.id,
            account: AccountId(0),
            _pad: [0; 2],
            ts_flags: ts.flags,
            rx_empty_gap_ns: ts.rx_empty_gap_ns,
            _pad2: 0,
        };
        self.sink.begin(if to_bbo { 1 } else { FRAME_EVENTS_MAX });
        self.sink.rx_flags = crate::latency::rx_flags(rx.ts_flags);
        let depth = pk.stream == StreamKind::DepthTopN;
        match P::decode_md(self.rd, &mut self.ctx, c, payload, &rx, &mut self.sink) {
            Ok(_) if !self.sink.over => {
                lt::stamp(TraceStage::DecodeEnd, [0; 4]);
                self.publish();
                if let Some(q) = self.seqs.get_mut(ix)
                    && let Some(v) = q.last_fwd.get_mut(sl)
                {
                    *v = pk.ids.last_id;
                }
                m.got_data = true;
                m.pub_tsc = ts.rx_tsc;
                if to_bbo
                    && let Some(s) = self.insts.get_mut(ix)
                    && s.refresh == REFRESH_ACTIVE
                {
                    s.refresh = REFRESH_NONE;
                    s.refresh_req = 0;
                }
                if depth
                    && let Some(q) = self.seqs.get_mut(ix)
                    && q.l2_state == InstSeq::L2_AWAIT_FIRST
                {
                    q.l2_state = InstSeq::L2_LIVE;
                    self.cs(false, m.id, ConnStateKind::Up, inst, 0, ts.rx_tsc);
                }
            }
            r => {
                if r.is_ok() {
                    self.n.frame_events_over += 1;
                }
                if let Some(d) = self.n.decode_err.get_mut(kix) {
                    *d += 1;
                }
                if depth {
                    if let Some(q) = self.seqs.get_mut(ix) {
                        q.l2_state = InstSeq::L2_AWAIT_FIRST;
                    }
                    self.cs(false, m.id, ConnStateKind::Resync, inst, codes::DECODE, ts.rx_tsc);
                }
            }
        }
        self.raw_data(m.kind, m.id, f.opcode, sub_id, Some(ix), &ts, payload, false);
    }

    fn on_pong(&mut self, m: &mut Meta, rx: u64) {
        if let Some(h) = m.hb.as_mut() {
            h.on_pong();
        }
        let half = self.t.ms(u64::from(self.cfg.bbo_ping_ms)) / 2;
        if rx.saturating_sub(m.pub_tsc) >= half {
            m.pub_tsc = rx;
            self.cs(
                m.kind == RouteKind::Bbo,
                m.id,
                ConnStateKind::Alive,
                InstId(0),
                0,
                rx,
            );
        }
    }

    /// A subscription answer: matched to the oldest outstanding frame of this connection.
    fn on_ack(&mut self, m: &mut Meta, ok: bool, _code: u32, rx: u64) {
        let Some((count, unsub)) = m.acks.pop_front() else {
            self.n.stray_ack += 1;
            return;
        };
        let to_bbo = m.kind == RouteKind::Bbo;
        for _ in 0..count {
            let Some(it) = m.ack_items.pop_front() else { break };
            let ix = it.ix as usize;
            let Some(inst) = self.insts.get(ix).map(|s| s.id) else {
                continue;
            };
            if !ok {
                self.cs(
                    to_bbo,
                    m.id,
                    ConnStateKind::Degraded,
                    inst,
                    codes::SUB_REJECTED,
                    rx,
                );
                continue;
            }
            let w = ix / 64;
            if unsub {
                if let Some(b) = m.insts.get_mut(w) {
                    *b &= !(1u64 << (ix % 64));
                }
                if to_bbo {
                    if let Some(s) = self.insts.get_mut(ix) {
                        s.members &= !(1u8 << m.id.0);
                    }
                    m.pub_tsc = rx;
                    self.cs(true, m.id, ConnStateKind::Down, inst, 0, rx);
                }
                continue;
            }
            if let Some(b) = m.insts.get_mut(w) {
                *b |= 1u64 << (ix % 64);
            }
            if to_bbo {
                let mut refresh = false;
                if let Some(s) = self.insts.get_mut(ix) {
                    s.members |= 1u8 << m.id.0;
                    if s.refresh == REFRESH_NEEDED {
                        s.refresh = REFRESH_ACTIVE;
                        s.refresh_due =
                            rx.saturating_add(self.t.ms(u64::from(self.cfg.bbo_refresh_grace_ms)));
                        s.refresh_req = 0;
                        s.refresh_attempt = 0;
                        refresh = true;
                    }
                }
                m.pub_tsc = rx;
                self.cs(true, m.id, ConnStateKind::Up, inst, 0, rx);
                if refresh {
                    self.cs(true, m.id, ConnStateKind::Resync, inst, codes::BBO_REFRESH, rx);
                }
            }
        }
    }
}

fn stream_bit(s: StreamKind) -> u8 {
    match s {
        StreamKind::Bbo => streams::BBO,
        StreamKind::DepthTopN | StreamKind::DepthIncr => streams::BOOK,
        StreamKind::Trade => streams::TRADE,
        StreamKind::Mark => streams::MARK,
        StreamKind::Index => streams::INDEX,
        StreamKind::Funding => streams::FUNDING,
        StreamKind::Liquidation => streams::LIQUIDATION,
    }
}

/// Md-route streams of a bit mask, in a fixed order.
const MD_STREAMS: [(u8, StreamKind); 5] = [
    (streams::TRADE, StreamKind::Trade),
    (streams::MARK, StreamKind::Mark),
    (streams::INDEX, StreamKind::Index),
    (streams::FUNDING, StreamKind::Funding),
    (streams::LIQUIDATION, StreamKind::Liquidation),
];

impl<P: VenueProtocol> FeedRuntime<P> {
    /// Startup (may allocate): validates the configuration (README "配置项", §1) and preallocates
    /// every per-inst and per-connection structure.
    pub fn new(cfg: FeedCfg, deps: FeedDeps, ctx: P::FeedCtx) -> Result<Self, InitErr> {
        for (v, k) in [
            (u32::from(cfg.bbo_conns), "feed.bbo_conns"),
            (u32::from(cfg.bbo_frames_per_poll), "feed.bbo_frames_per_poll"),
            (u32::from(cfg.frames_per_poll), "feed.frames_per_poll"),
            (cfg.bbo_ping_ms, "feed.bbo_ping_ms"),
            (cfg.bbo_dead_ms, "feed.bbo_dead_ms"),
        ] {
            if v == 0 {
                return Err(InitErr::Cfg(k));
            }
        }
        let rd = deps.rd;
        let vm = rd.venue(P::VENUE);
        let spc = u32::from(vm.ws_max_streams_per_conn.max(1));
        let routes = cfg.routes;
        // Rings and journals must match the routes (README §1 "存在条件").
        let snap_needed = match routes {
            FeedRoutes::Bbo | FeedRoutes::All => true,
            FeedRoutes::Md => cfg.depth == DepthMode::Incremental,
        };
        if deps.bbo.is_some() != routes.has_bbo()
            || deps.md.is_some() != routes.has_md()
            || deps.bulk.is_some() != routes.has_md()
            || deps.snapreq.is_some() != snap_needed
            || deps.snap.is_some() != snap_needed
            || deps.ready.is_some() != cfg.ready
        {
            return Err(InitErr::DepsMismatch);
        }
        let groups = cfg.max_sub_insts.div_ceil(spc);
        if routes.has_bbo() && u32::from(cfg.bbo_conns) * groups + 1 > u32::from(MAX_BBO_CONN_SLOTS) {
            return Err(InitErr::BboSlots);
        }
        let half = u32::from(vm.ws_max_in_msgs_per_s) / 2;
        let probes = 1000u32.div_ceil(cfg.bbo_ping_ms);
        if routes.has_bbo() && half.saturating_sub(probes) < 1 {
            return Err(InitErr::SubRate);
        }
        if routes.has_md() && half < 1 {
            return Err(InitErr::SubRate);
        }
        if vm.ws_max_conn_age_s != 0 && cfg.rotate_margin_s >= vm.ws_max_conn_age_s {
            return Err(InitErr::RotateMargin);
        }
        let t = Ticks { clock: deps.clock };
        let mut ids: Vec<InstId> = rd.insts_of_venue(P::VENUE).to_vec();
        ids.sort_unstable();
        ids.dedup();
        let n = ids.len();
        let insts = ids
            .iter()
            .map(|&id| Inst {
                id,
                want: 0,
                bbo_group: NONE8,
                depth_group: NONE8,
                md_group: [NONE8; MD_STREAMS.len()],
                members: 0,
                refresh: REFRESH_NONE,
                refresh_due: 0,
                refresh_req: 0,
                refresh_attempt: 0,
                sub_id: [0; 3],
                sample_tsc: None,
            })
            .collect();
        let pick = |k: RouteKind| P::routes().iter().find(|r| r.kind == k).copied();
        let depth_route = pick(RouteKind::DepthTopN);
        let topn = depth_route.map_or(cfg.topn_levels, |r| {
            r.topn_levels
                .iter()
                .copied()
                .find(|&l| l >= cfg.topn_levels)
                .or_else(|| r.topn_levels.iter().copied().max())
                .unwrap_or(cfg.topn_levels)
        });
        let max_groups = u8::try_from(groups.max(1)).unwrap_or(u8::MAX);
        let md_groups = (MD_LAST - MD_FIRST + 1) as usize;
        let st = St {
            cfg,
            rd,
            ctx,
            t,
            ids: ids.into_boxed_slice(),
            insts,
            seqs: vec![InstSeq::new(); n],
            bbo: deps.bbo,
            md: deps.md,
            bulk: deps.bulk,
            pend_bbo: VecDeque::with_capacity(PEND_CAP),
            pend_md: VecDeque::with_capacity(PEND_CAP),
            raw: RawLog::new(deps.raw, P::VENUE, t),
            sink: FeedSink::new(cfg.snapshot_part_levels),
            n: FeedCounters::default(),
            next_sub_id: 1,
            bbo_route: pick(RouteKind::Bbo),
            depth_route,
            md_routes: vec![None; md_groups],
            topn,
            spc,
            max_groups,
            bbo_load: vec![0; usize::from(max_groups)],
            depth_load: vec![0; md_groups],
            md_load: vec![0; md_groups],
        };
        let mut conns = Vec::with_capacity(64);
        conns.resize_with(64, || None);
        let mut building = Vec::with_capacity(64);
        building.resize_with(64, || None);
        let snapshot_supported = deps.rd.insts_of_venue(P::VENUE).first().is_some_and(|inst| {
            P::bbo_snapshot_request(
                deps.rd,
                &st.ctx,
                &[*inst],
                &mut hft_venue_proto::RestReqBuf::default(),
            ) == Ok(true)
        });
        Ok(FeedRuntime {
            st,
            conns,
            building,
            slots: BboSlots::default(),
            md_ids: IdRange::new(MD_FIRST, MD_LAST),
            cmds: deps.cmds,
            snapshot_supported,
            next_snapshot: 1,
            snapreq: deps.snapreq,
            snap: deps.snap,
            ready: deps.ready,
            ready_set: ConnSet::default(),
            handoff: deps.handoff,
            retire: deps.retire,
            retire_pend: VecDeque::with_capacity(64),
            build: deps.build,
            subframes: SubFrames::with_capacity(64 * 1024, 64),
            rr: 0,
            order: Vec::with_capacity(64),
            last: LoopStat::default(),
        })
    }

    /// One round (README §2). Non-blocking; bounded by `FEEDCMD_PER_POLL`, the per-connection frame
    /// caps and the connection count.
    pub fn poll_once(&mut self, now: u64) -> LoopStat {
        let mut ls = LoopStat::default();
        self.st.flush_pending();
        self.flush_retire();
        // 1. commands.
        for _ in 0..FEEDCMD_PER_POLL {
            let Some(c) = self.cmds.try_pop() else { break };
            ls.cmds += 1;
            self.command(c, now);
        }
        // Hand-overs before reading (a connection installed this round is read next round).
        while let Some(h) = self.handoff.try_pop() {
            ls.did_work = true;
            self.handover(h, now);
        }
        // 2. readiness.
        let probing = self.ready.is_some();
        if let Some(r) = self.ready.as_mut() {
            r.probe(&mut self.ready_set);
        }
        // 3./4. read and frames.
        let (frames, full) = self.read_all(now, probing);
        ls.frames = frames;
        ls.ring_full = full;
        ls.did_work |= self.snapshots(now);
        // 7./8. errors, liveness, heartbeats, subscriptions, builds.
        let maint = self.maintain(now);
        ls.did_work |= frames > 0 || ls.cmds > 0 || maint;
        let mut dl = self.deadline(now);
        if full > 0 {
            self.st.n.ring_full += u64::from(full);
            dl = min_deadline(
                dl,
                now.saturating_add(self.st.t.us(u64::from(self.st.cfg.ring_full_park_us))),
            );
        }
        ls.next_deadline = dl;
        self.last = ls;
        ls
    }

    fn flush_retire(&mut self) {
        while let Some(m) = self.retire_pend.front().copied() {
            if self.retire.try_push(m).is_err() {
                break;
            }
            self.retire_pend.pop_front();
        }
    }

    /// Stream quotes win over delayed REST responses. A REST quote never proves WS liveness.
    fn snapshots(&mut self, now: u64) -> bool {
        let mut did = false;
        let grace = self.st.t.ms(u64::from(self.st.cfg.bbo_refresh_grace_ms));
        if self.st.pend_bbo.is_empty()
            && self.st.bbo.as_ref().is_some_and(|r| r.free_hint() > 0)
            && let Some(ring) = &mut self.snap
            && let Some(bytes) = ring.peek()
        {
            if let Ok((h, tail)) = SnapRecHdr::read_from_prefix(bytes) {
                let quote = tail
                    .get(h.body_len as usize..)
                    .and_then(|b| hft_event::Bbo::read_from_bytes(b).ok());
                if let Some(raw) = tail.get(..h.body_len as usize) {
                    self.st.raw.rx(
                        h.conn,
                        what::REST_RESPONSE,
                        0,
                        h.req_id,
                        quote.map_or(0, |e| e.h.hw_rx_ns),
                        if quote.is_some() { h.ts_flags } else { 0 },
                        raw,
                        h.rx_tsc,
                    );
                }
                let ix = self.st.local(h.inst);
                if let Some(ix) = ix
                    && self.st.insts.get(ix).is_some_and(|s| {
                        s.refresh == REFRESH_ACTIVE && s.refresh_req == h.req_id && s.members != 0
                    })
                {
                    let last = self.st.seqs[ix].last_fwd[slot_of(StreamKind::Bbo)];
                    if h.status == SnapRecHdr::OK
                        && let Some(e) = quote
                        && e.h.inst == h.inst
                        && e.h.venue == P::VENUE
                        && (e.update_id == 0 || e.update_id >= last)
                    {
                        use hft_venue_proto::MdSink;
                        self.st.sink.begin(1);
                        self.st.sink.rx_flags = crate::latency::rx_flags(h.ts_flags);
                        self.st.sink.bbo(&e);
                        self.st.publish();
                        if e.update_id != 0 {
                            self.st.seqs[ix].last_fwd[slot_of(StreamKind::Bbo)] = e.update_id;
                        }
                        self.st.insts[ix].refresh = REFRESH_NONE;
                    } else {
                        self.st.n.snapshot_failed += 1;
                        self.st.insts[ix].refresh_due = now.saturating_add(grace);
                    }
                    self.st.insts[ix].refresh_req = 0;
                } else {
                    self.st.n.snapshot_stale += 1;
                }
            } else {
                self.st.n.snapshot_failed += 1;
            }
            ring.advance();
            did = true;
        }
        if self.snapshot_supported
            && let Some(out) = &mut self.snapreq
        {
            for s in &mut self.st.insts {
                if s.refresh != REFRESH_ACTIVE || s.members == 0 || s.refresh_req != 0 || now < s.refresh_due
                {
                    continue;
                }
                let req = SnapReq {
                    kind: SnapReq::BBO,
                    _pad: 0,
                    attempt: s.refresh_attempt,
                    inst: s.id,
                    req_id: self.next_snapshot,
                    _pad2: 0,
                };
                if out.try_push(req).is_err() {
                    break;
                }
                s.refresh_req = self.next_snapshot;
                s.refresh_attempt = s.refresh_attempt.saturating_add(1);
                self.next_snapshot = self.next_snapshot.wrapping_add(1).max(1);
                did = true;
            }
        }
        did
    }

    fn command(&mut self, c: FeedCmd, now: u64) {
        match c {
            FeedCmd::Subscribe { inst, streams } => self.subscribe(inst, streams, now),
            FeedCmd::Unsubscribe { inst } => self.subscribe(inst, 0, now),
            FeedCmd::Resync { inst } => {
                if !self.st.cfg.routes.has_md() {
                    self.st.n.cmd_ignored += 1;
                    return;
                }
                let Some(ix) = self.st.local(inst) else {
                    self.st.n.cmd_bad_inst += 1;
                    return;
                };
                if self.st.insts.get(ix).is_some_and(|s| s.want & streams::BOOK != 0) {
                    if let Some(q) = self.st.seqs.get_mut(ix) {
                        q.l2_state = InstSeq::L2_AWAIT_FIRST;
                    }
                    let conn = self.depth_conn_of(ix).unwrap_or(ConnId(0));
                    self.st
                        .cs(false, conn, ConnStateKind::Resync, inst, codes::REQUESTED, now);
                }
            }
            FeedCmd::Rotate { conn } => match self.conns.get_mut(conn.0 as usize).and_then(Option::as_mut) {
                Some(fc) => fc.m.rotate = true,
                None => self.st.n.cmd_ignored += 1,
            },
        }
    }

    fn depth_conn_of(&self, ix: usize) -> Option<ConnId> {
        let g = self.st.insts.get(ix)?.depth_group;
        self.conns
            .iter()
            .flatten()
            .find(|c| c.m.kind == RouteKind::DepthTopN && c.m.group == g)
            .map(|c| c.m.id)
    }

    /// Queues `it` on every live connection of (`kind`, `group`).
    fn enqueue(&mut self, kind: RouteKind, group: u8, it: SubItem) {
        for fc in self.conns.iter_mut().flatten() {
            if fc.m.kind == kind && fc.m.group == group && fc.m.replaced_by.is_none() {
                if fc.m.pend.len() < SUBQ_CAP {
                    fc.m.pend.push_back(it);
                } else {
                    self.st.n.sub_queue_full += 1;
                }
            }
        }
    }

    fn subscribe(&mut self, inst: InstId, want: u8, now: u64) {
        let Some(ix) = self.st.local(inst) else {
            self.st.n.cmd_bad_inst += 1;
            return;
        };
        let mask = match self.st.cfg.routes {
            FeedRoutes::Bbo => streams::BBO,
            FeedRoutes::Md => streams::MD_ALL,
            FeedRoutes::All => streams::BBO | streams::MD_ALL,
        };
        let Some(old) = self.st.insts.get(ix).map(|s| s.want) else {
            return;
        };
        let new = want & mask;
        if new == old {
            return;
        }
        let add = new & !old;
        let del = old & !new;
        let ix32 = ix as u32;
        if let Some(s) = self.st.insts.get_mut(ix) {
            s.want = new;
        }
        // BBO.
        if add & streams::BBO != 0 && self.st.bbo_route.is_some() {
            let g = (0..self.st.max_groups)
                .find(|&g| self.st.bbo_load.get(g as usize).is_some_and(|&l| l < self.st.spc));
            match g {
                Some(g) => {
                    if let Some(l) = self.st.bbo_load.get_mut(g as usize) {
                        *l += 1;
                    }
                    let id = self.st.next_sub_id;
                    self.st.next_sub_id += 1;
                    if let Some(s) = self.st.insts.get_mut(ix) {
                        s.bbo_group = g;
                        s.sub_id[0] = id;
                    }
                    self.enqueue(
                        RouteKind::Bbo,
                        g,
                        SubItem {
                            ix: ix32,
                            stream: StreamKind::Bbo,
                            levels: 0,
                            unsub: false,
                        },
                    );
                }
                None => self.st.n.no_group += 1,
            }
        }
        if del & streams::BBO != 0 {
            let g = self.st.insts.get(ix).map_or(NONE8, |s| s.bbo_group);
            if g != NONE8 {
                if let Some(l) = self.st.bbo_load.get_mut(g as usize) {
                    *l = l.saturating_sub(1);
                }
                self.enqueue(
                    RouteKind::Bbo,
                    g,
                    SubItem {
                        ix: ix32,
                        stream: StreamKind::Bbo,
                        levels: 0,
                        unsub: true,
                    },
                );
                if let Some(s) = self.st.insts.get_mut(ix) {
                    s.bbo_group = NONE8;
                    s.refresh = REFRESH_NONE;
                }
            }
        }
        // Depth (TopN).
        if add & streams::BOOK != 0 && self.st.depth_route.is_some() {
            let g = self.assign(RouteKind::DepthTopN, 1);
            if let Some(g) = g {
                let id = self.st.next_sub_id;
                self.st.next_sub_id += 1;
                if let Some(s) = self.st.insts.get_mut(ix) {
                    s.depth_group = g;
                    s.sub_id[1] = id;
                }
                if let Some(q) = self.st.seqs.get_mut(ix) {
                    q.l2_state = InstSeq::L2_AWAIT_FIRST;
                }
                let levels = self.st.topn;
                self.enqueue(
                    RouteKind::DepthTopN,
                    g,
                    SubItem {
                        ix: ix32,
                        stream: StreamKind::DepthTopN,
                        levels,
                        unsub: false,
                    },
                );
                self.st
                    .cs(false, ConnId(0), ConnStateKind::Resync, inst, codes::NEW_SUB, now);
            }
        }
        if del & streams::BOOK != 0 {
            let g = self.st.insts.get(ix).map_or(NONE8, |s| s.depth_group);
            if g != NONE8 {
                if let Some(l) = self.st.depth_load.get_mut(g as usize) {
                    *l = l.saturating_sub(1);
                }
                self.enqueue(
                    RouteKind::DepthTopN,
                    g,
                    SubItem {
                        ix: ix32,
                        stream: StreamKind::DepthTopN,
                        levels: 0,
                        unsub: true,
                    },
                );
                if let Some(s) = self.st.insts.get_mut(ix) {
                    s.depth_group = NONE8;
                }
            }
        }
        // Each MD stream belongs to its protocol route. Groups never change route while
        // connections may still be installed/reconnecting, including after unsubscribe.
        for (stream_ix, (bit, stream)) in MD_STREAMS.iter().copied().enumerate() {
            if add & bit != 0 {
                let Some(route) = P::stream_route(stream).and_then(|id| {
                    P::routes()
                        .iter()
                        .find(|r| r.id == id && r.kind == RouteKind::Md)
                        .copied()
                }) else {
                    continue;
                };
                let Some(g) = self.assign_md(route) else {
                    self.st.n.no_group += 1;
                    continue;
                };
                let id = self.st.next_sub_id;
                self.st.next_sub_id += 1;
                if let Some(s) = self.st.insts.get_mut(ix) {
                    s.md_group[stream_ix] = g;
                    s.sub_id[2] = id;
                }
                self.enqueue(
                    RouteKind::Md,
                    g,
                    SubItem {
                        ix: ix32,
                        stream,
                        levels: 0,
                        unsub: false,
                    },
                );
            }
            if del & bit != 0 {
                let g = self.st.insts.get(ix).map_or(NONE8, |s| s.md_group[stream_ix]);
                if g != NONE8 {
                    if let Some(l) = self.st.md_load.get_mut(g as usize) {
                        *l = l.saturating_sub(1);
                    }
                    self.enqueue(
                        RouteKind::Md,
                        g,
                        SubItem {
                            ix: ix32,
                            stream,
                            levels: 0,
                            unsub: true,
                        },
                    );
                    if let Some(s) = self.st.insts.get_mut(ix) {
                        s.md_group[stream_ix] = NONE8;
                    }
                }
            }
        }
    }

    fn assign_md(&mut self, route: RouteSpec) -> Option<u8> {
        let g = self
            .st
            .md_routes
            .iter()
            .zip(&self.st.md_load)
            .position(|(r, load)| r.is_some_and(|r| r.id == route.id) && *load < self.st.spc)
            .or_else(|| self.st.md_routes.iter().position(Option::is_none))?;
        *self.st.md_routes.get_mut(g)? = Some(route);
        *self.st.md_load.get_mut(g)? += 1;
        u8::try_from(g).ok()
    }

    /// First group of a non-BBO route with room for `need` more streams (`need = 0`: room for one).
    fn assign(&mut self, kind: RouteKind, need: u32) -> Option<u8> {
        let spc = self.st.spc;
        let load = if kind == RouteKind::DepthTopN {
            &mut self.st.depth_load
        } else {
            &mut self.st.md_load
        };
        let g = load.iter().position(|&l| l + need.max(1) <= spc)?;
        if let Some(l) = load.get_mut(g)
            && need > 0
        {
            *l += need;
        }
        u8::try_from(g).ok()
    }

    fn handover(&mut self, h: Handover, now: u64) {
        match h {
            Handover::Failed(id) => {
                if self
                    .building
                    .get_mut(id.0 as usize)
                    .and_then(Option::take)
                    .is_some()
                {
                    self.free_id(id);
                }
            }
            Handover::Ready(h) => {
                let id = h.id();
                let Some(b) = self.building.get_mut(id.0 as usize).and_then(Option::take) else {
                    self.st.n.install_failed += 1;
                    return;
                };
                let mut pool = ConnPool::<1>::new(PoolMode::Race, 1, RotatePolicy::default());
                let age = u64::from(self.st.rd.venue(P::VENUE).ws_max_conn_age_s);
                let rot = (age != 0).then(|| {
                    now.saturating_add(
                        self.st
                            .t
                            .ms(age.saturating_sub(u64::from(self.st.cfg.rotate_margin_s)) * 1000),
                    )
                });
                if pool.install(h, rot).is_err() {
                    self.st.n.install_failed += 1;
                    self.free_id(id);
                    return;
                }
                let fd = pool.get(id).map_or(-1, |c| c.raw_fd());
                let route = match b.kind {
                    RouteKind::Bbo => self.st.bbo_route,
                    RouteKind::DepthTopN => self.st.depth_route,
                    RouteKind::Md => self.st.md_routes.get(b.group as usize).copied().flatten(),
                }
                .map_or(RouteId(0), |r| r.id);
                let vm = self.st.rd.venue(P::VENUE);
                let half = u64::from(vm.ws_max_in_msgs_per_s) / 2;
                let rate = if b.kind == RouteKind::Bbo {
                    half.saturating_sub(u64::from(1000u32.div_ceil(self.st.cfg.bbo_ping_ms)))
                } else {
                    half
                }
                .max(1);
                let hb = (b.kind != RouteKind::Bbo).then(|| {
                    Hb::new(
                        &P::heartbeat(ConnRole::Md),
                        &self.st.t,
                        now,
                        b.kind == RouteKind::DepthTopN,
                    )
                });
                let mut m = Meta {
                    id,
                    kind: b.kind,
                    route,
                    group: b.group,
                    fd,
                    installed: now,
                    replaces: b.replaces,
                    replaced_by: None,
                    rotate: false,
                    hint: false,
                    got_data: false,
                    last_data: now,
                    pend: VecDeque::with_capacity(SUBQ_CAP),
                    acks: VecDeque::with_capacity(SUBQ_CAP),
                    ack_items: VecDeque::with_capacity(SUBQ_CAP),
                    next_sub: now,
                    sub_period: self.st.t.ms(1000) / rate,
                    pub_tsc: now,
                    probe_tsc: now.saturating_add(self.st.t.ms(u64::from(self.st.cfg.bbo_ping_ms))),
                    pong_seen: 0,
                    hb,
                    insts: vec![0u64; self.st.insts.len().div_ceil(64).max(1)],
                    left: 0,
                    done: false,
                    buffered: false,
                };
                // The group's subscriptions.
                for (ix, s) in self.st.insts.iter().enumerate() {
                    let ix = ix as u32;
                    match b.kind {
                        RouteKind::Bbo if s.bbo_group == b.group && s.want & streams::BBO != 0 => {
                            m.pend.push_back(SubItem {
                                ix,
                                stream: StreamKind::Bbo,
                                levels: 0,
                                unsub: false,
                            });
                        }
                        RouteKind::DepthTopN if s.depth_group == b.group && s.want & streams::BOOK != 0 => {
                            m.pend.push_back(SubItem {
                                ix,
                                stream: StreamKind::DepthTopN,
                                levels: self.st.topn,
                                unsub: false,
                            });
                        }
                        RouteKind::Md => {
                            for (stream_ix, (bit, sk)) in MD_STREAMS.iter().copied().enumerate() {
                                if s.want & bit != 0 && s.md_group[stream_ix] == b.group {
                                    m.pend.push_back(SubItem {
                                        ix,
                                        stream: sk,
                                        levels: 0,
                                        unsub: false,
                                    });
                                }
                            }
                        }
                        _ => {}
                    }
                }
                let to_bbo = b.kind == RouteKind::Bbo;
                self.st.cs(to_bbo, id, ConnStateKind::Up, InstId(0), 0, now);
                self.st
                    .raw
                    .rx(id, what::CONNECT, 0, 0, 0, 0, &[0, ConnRole::Md as u8], now);
                if let Some(old) = b.replaces
                    && let Some(o) = self.conns.get_mut(old.0 as usize).and_then(Option::as_mut)
                {
                    o.m.replaced_by = Some(id);
                }
                if let Some(slot) = self.conns.get_mut(id.0 as usize) {
                    *slot = Some(FConn { pool, m });
                }
            }
        }
    }

    fn free_id(&mut self, id: ConnId) {
        if id.0 < MAX_BBO_CONN_SLOTS {
            self.slots.free(id);
        } else {
            self.md_ids.free(id);
        }
    }

    /// Steps 3–5 over every connection; returns (frames, full rings).
    fn read_all(&mut self, now: u64, probing: bool) -> (u16, u16) {
        let mut frames = 0u16;
        let mut full_bbo = false;
        let mut full_md = false;
        self.order.clear();
        for (i, c) in self.conns.iter_mut().enumerate() {
            if let Some(fc) = c.as_mut() {
                fc.m.left = if fc.m.kind == RouteKind::Bbo {
                    self.st.cfg.bbo_frames_per_poll
                } else {
                    self.st.cfg.frames_per_poll
                };
                fc.m.done = false;
                self.order.push(ConnId(i as u8));
            }
        }
        // Receive: one poll_rx per ready (or every) live connection whose ring has room.
        for k in 0..self.order.len() {
            let Some(&id) = self.order.get(k) else { break };
            let Some(fc) = self.conns.get_mut(id.0 as usize).and_then(Option::as_mut) else {
                continue;
            };
            let bbo = fc.m.kind == RouteKind::Bbo;
            if !self.st.has_room(fc.m.kind) {
                if bbo {
                    full_bbo = true;
                } else {
                    full_md = true;
                }
                fc.m.done = true;
                continue;
            }
            let Some(conn) = fc.pool.primary() else { continue };
            if !probing || self.ready_set.contains(id) {
                if bbo {
                    conn.poll_rx_if_needed(now);
                } else {
                    conn.poll_rx(now);
                }
            } else {
                conn.note_empty(now);
            }
        }
        // Bbo routes: frame round robin, the start rotating each round.
        let nb = self
            .order
            .iter()
            .filter(|id| {
                self.conns
                    .get(id.0 as usize)
                    .and_then(Option::as_ref)
                    .is_some_and(|c| c.m.kind == RouteKind::Bbo)
            })
            .count();
        if nb > 0 {
            let start = self.rr % nb;
            self.rr = self.rr.wrapping_add(1);
            let mut bbo_ids = [ConnId(0); MAX_BBO_CONN_SLOTS as usize];
            let mut cnt = 0usize;
            for id in self.order.iter() {
                if self
                    .conns
                    .get(id.0 as usize)
                    .and_then(Option::as_ref)
                    .is_some_and(|c| c.m.kind == RouteKind::Bbo)
                    && let Some(s) = bbo_ids.get_mut(cnt)
                {
                    *s = *id;
                    cnt += 1;
                }
            }
            loop {
                let mut any = false;
                for j in 0..cnt {
                    let Some(&id) = bbo_ids.get((start + j) % cnt) else {
                        continue;
                    };
                    if full_bbo {
                        break;
                    }
                    let Some(fc) = self.conns.get_mut(id.0 as usize).and_then(Option::as_mut) else {
                        continue;
                    };
                    if fc.m.done || fc.m.left == 0 {
                        continue;
                    }
                    if !self.st.has_room(RouteKind::Bbo) {
                        full_bbo = true;
                        break;
                    }
                    let FConn { pool, m } = fc;
                    let Some(conn) = pool.primary() else {
                        m.done = true;
                        continue;
                    };
                    match conn.next_frame() {
                        Some(f) => {
                            self.st.frame(m, &f);
                            m.left -= 1;
                            frames += 1;
                            any = true;
                        }
                        None => m.done = true,
                    }
                }
                if !any || full_bbo {
                    break;
                }
            }
        }
        // Other routes: per connection, up to frames_per_poll.
        for k in 0..self.order.len() {
            let Some(&id) = self.order.get(k) else { break };
            let Some(fc) = self.conns.get_mut(id.0 as usize).and_then(Option::as_mut) else {
                continue;
            };
            if fc.m.kind == RouteKind::Bbo || fc.m.done {
                continue;
            }
            let FConn { pool, m } = fc;
            while m.left > 0 {
                if !self.st.has_room(m.kind) {
                    full_md = true;
                    break;
                }
                let Some(conn) = pool.primary() else { break };
                let Some(f) = conn.next_frame() else { break };
                self.st.frame(m, &f);
                m.left -= 1;
                frames += 1;
            }
        }
        // WS pongs (liveness), finish_poll, buffered state.
        for k in 0..self.order.len() {
            let Some(&id) = self.order.get(k) else { break };
            let Some(fc) = self.conns.get_mut(id.0 as usize).and_then(Option::as_mut) else {
                continue;
            };
            let FConn { pool, m } = fc;
            let Some(conn) = pool.primary() else { continue };
            let p = conn.last_pong_tsc();
            conn.finish_poll(now);
            m.buffered = conn.rx_buffered() > 0;
            if p > m.pong_seen {
                m.pong_seen = p;
                self.st.on_pong(m, p);
            }
        }
        (frames, u16::from(full_bbo) + u16::from(full_md))
    }

    /// Steps 7–8: error bits, liveness, heartbeats, subscriptions, rotation, builds. Returns whether
    /// anything was done.
    fn maintain(&mut self, now: u64) -> bool {
        let mut did = false;
        let ping = self.st.t.ms(u64::from(self.st.cfg.bbo_ping_ms));
        let dead = self.st.t.ms(u64::from(self.st.cfg.bbo_dead_ms));
        for i in 0..self.conns.len() {
            let Some(fc) = self.conns.get_mut(i).and_then(Option::as_mut) else {
                continue;
            };
            let FConn { pool, m } = fc;
            let id = m.id;
            let Some(conn) = pool.primary() else { continue };
            // Timeouts.
            let mut bits = conn.err();
            if m.kind == RouteKind::Bbo {
                let last = conn.last_rx_tsc().max(m.installed);
                if now.saturating_sub(last) > dead {
                    self.st.cs(
                        true,
                        id,
                        ConnStateKind::Degraded,
                        InstId(0),
                        codes::BBO_SILENT,
                        now,
                    );
                    conn.mark(ErrBits::IDLE_TIMEOUT);
                    bits = conn.err();
                }
            } else if let Some(h) = m.hb.as_ref() {
                let t = h.check(now, m.last_data);
                if !t.is_empty() {
                    conn.mark(t);
                    bits = conn.err();
                }
            }
            if m.hint {
                conn.mark(ErrBits::SERVER_HINT);
                m.hint = false;
                bits = conn.err();
            }
            let fatal = bits.intersects(ErrBits::FATAL | ErrBits::PING_TIMEOUT | ErrBits::IDLE_TIMEOUT);
            if fatal {
                self.retire_conn(ConnId(i as u8), bits, now);
                did = true;
                continue;
            }
            if bits.intersects(ErrBits::SERVER_HINT | ErrBits::ROTATE_DUE) {
                m.rotate = true;
            }
            conn.clear_err();
            if pool.rotate_due(now).is_some() {
                m.rotate = true;
            }
            let Some(conn) = pool.primary() else { continue };
            // BBO probe.
            if m.kind == RouteKind::Bbo && now >= m.probe_tsc {
                m.probe_tsc = m.probe_tsc.saturating_add(ping).max(now.saturating_add(1));
                match P::bbo_probe() {
                    PingKind::WsPing => {
                        let _ = conn.send_ping(now);
                    }
                    PingKind::AppText | PingKind::AppJson => {
                        let mut buf = [0u8; 256];
                        let ms = self.st.t.utc_ns(now) / 1_000_000;
                        let n = P::app_ping(ConnRole::Md, ms, &mut buf);
                        match buf.get(..n) {
                            Some(b) if n > 0 => {
                                let _ = conn.send_text(b, now);
                            }
                            _ => {
                                let _ = conn.send_ping(now);
                            }
                        }
                    }
                }
                did = true;
            }
            // Heartbeat of the other connections.
            if let Some(h) = m.hb.as_mut() {
                match h.due(now, m.last_data) {
                    HbSend::None => {}
                    HbSend::WsPing => {
                        let _ = conn.send_ping(now);
                        did = true;
                    }
                    HbSend::App => {
                        let mut buf = [0u8; 256];
                        let ms = self.st.t.utc_ns(now) / 1_000_000;
                        let n = P::app_ping(ConnRole::Md, ms, &mut buf);
                        match buf.get(..n) {
                            Some(b) if n > 0 => {
                                let _ = conn.send_text(b, now);
                            }
                            _ => {
                                let _ = conn.send_ping(now);
                            }
                        }
                        did = true;
                    }
                }
            }
            // Subscriptions (paced).
            if !m.pend.is_empty() && now >= m.next_sub {
                did |= send_sub::<P>(&mut self.st, m, conn, &mut self.subframes, now);
            }
            conn.finish_poll(now);
        }
        // Rotation: a replacement that answered all its subscriptions and got data retires the old one.
        for i in 0..self.conns.len() {
            let Some(fc) = self.conns.get(i).and_then(Option::as_ref) else {
                continue;
            };
            let (Some(old), true) = (fc.m.replaces, fc.m.pend.is_empty() && fc.m.acks.is_empty()) else {
                continue;
            };
            let ready = fc.m.got_data || fc.m.kind != RouteKind::Bbo;
            if ready {
                if let Some(f) = self.conns.get_mut(i).and_then(Option::as_mut) {
                    f.m.replaces = None;
                }
                if self.conns.get(old.0 as usize).is_some_and(Option::is_some) {
                    self.retire_conn(old, ErrBits::ROTATE_DUE, now);
                    did = true;
                }
            }
        }
        did |= self.builds(now);
        did
    }

    /// Retires connection `id` (README §2 step 7, §3a): `Down{inst = 0}` (then the slot is free),
    /// `RACE_SINGLE` when one BBO connection of the group is left, `CONN_LOST` for TopN insts.
    fn retire_conn(&mut self, id: ConnId, why: ErrBits, now: u64) {
        let Some(mut fc) = self.conns.get_mut(id.0 as usize).and_then(Option::take) else {
            return;
        };
        if let Ok(msg) = fc.pool.retire(id, why)
            && self.retire.try_push(msg).is_err()
        {
            self.retire_pend.push_back(msg);
        }
        let to_bbo = fc.m.kind == RouteKind::Bbo;
        self.st.raw.rx(id, what::DISCONNECT, 0, 0, 0, 0, &[], now);
        self.st
            .cs(to_bbo, id, ConnStateKind::Down, InstId(0), why.bits(), now);
        if let Some(n) = fc.m.replaced_by
            && let Some(r) = self.conns.get_mut(n.0 as usize).and_then(Option::as_mut)
        {
            r.m.replaces = None;
        }
        if let Some(o) = fc.m.replaces
            && let Some(r) = self.conns.get_mut(o.0 as usize).and_then(Option::as_mut)
        {
            r.m.replaced_by = None;
        }
        match fc.m.kind {
            RouteKind::Bbo => {
                let bit = 1u8 << id.0;
                for s in self.st.insts.iter_mut() {
                    if s.members & bit != 0 {
                        s.members &= !bit;
                        if s.members == 0 && s.want & streams::BBO != 0 {
                            s.refresh = REFRESH_NEEDED;
                            s.refresh_req = 0;
                        }
                    }
                }
                self.slots.free(id);
                if self.st.cfg.bbo_conns >= 2 {
                    let g = fc.m.group;
                    let mut left = None;
                    let mut cnt = 0;
                    for c in self.conns.iter().flatten() {
                        if c.m.kind == RouteKind::Bbo && c.m.group == g {
                            cnt += 1;
                            left = Some(c.m.id);
                        }
                    }
                    if cnt == 1
                        && let Some(l) = left
                    {
                        self.st.cs(
                            true,
                            l,
                            ConnStateKind::Degraded,
                            InstId(0),
                            codes::RACE_SINGLE,
                            now,
                        );
                    }
                }
            }
            RouteKind::DepthTopN => {
                for (w, bits) in fc.m.insts.iter().enumerate() {
                    let mut b = *bits;
                    while b != 0 {
                        let k = b.trailing_zeros() as usize;
                        b &= b - 1;
                        let ix = w * 64 + k;
                        let Some(inst) = self.st.insts.get(ix).map(|s| s.id) else {
                            continue;
                        };
                        if let Some(q) = self.st.seqs.get_mut(ix) {
                            q.l2_state = InstSeq::L2_AWAIT_FIRST;
                        }
                        self.st
                            .cs(false, id, ConnStateKind::Resync, inst, codes::CONN_LOST, now);
                    }
                }
                self.md_ids.free(id);
            }
            RouteKind::Md => self.md_ids.free(id),
        }
    }

    /// Builds missing connections and rotation replacements (README §2 step 8).
    fn builds(&mut self, now: u64) -> bool {
        let mut did = false;
        // Rotation replacements.
        for i in 0..self.conns.len() {
            let Some(fc) = self.conns.get(i).and_then(Option::as_ref) else {
                continue;
            };
            if !fc.m.rotate || fc.m.replaced_by.is_some() || fc.m.replaces.is_some() {
                continue;
            }
            if self
                .building
                .iter()
                .flatten()
                .any(|b| b.replaces == Some(fc.m.id))
            {
                continue;
            }
            let (kind, group, old) = (fc.m.kind, fc.m.group, fc.m.id);
            did |= self.request(kind, group, Some(old), now);
        }
        // Missing BBO connections per group.
        if self.st.bbo_route.is_some() {
            for g in 0..self.st.max_groups {
                if self.st.bbo_load.get(g as usize).copied().unwrap_or(0) == 0 {
                    continue;
                }
                let live = self
                    .conns
                    .iter()
                    .flatten()
                    .filter(|c| c.m.kind == RouteKind::Bbo && c.m.group == g && c.m.replaced_by.is_none())
                    .count();
                let bld = self
                    .building
                    .iter()
                    .flatten()
                    .filter(|b| b.kind == RouteKind::Bbo && b.group == g && b.replaces.is_none())
                    .count();
                for _ in live + bld..usize::from(self.st.cfg.bbo_conns) {
                    if !self.request(RouteKind::Bbo, g, None, now) {
                        break;
                    }
                    did = true;
                }
            }
        }
        for kind in [RouteKind::DepthTopN, RouteKind::Md] {
            let n = if kind == RouteKind::DepthTopN {
                self.st.depth_load.len()
            } else {
                self.st.md_load.len()
            };
            for g in 0..n {
                let load = if kind == RouteKind::DepthTopN {
                    &self.st.depth_load
                } else {
                    &self.st.md_load
                };
                if load.get(g).copied().unwrap_or(0) == 0 {
                    continue;
                }
                let g8 = g as u8;
                let have = self
                    .conns
                    .iter()
                    .flatten()
                    .any(|c| c.m.kind == kind && c.m.group == g8)
                    || self
                        .building
                        .iter()
                        .flatten()
                        .any(|b| b.kind == kind && b.group == g8);
                if !have {
                    did |= self.request(kind, g8, None, now);
                }
            }
        }
        did
    }

    fn request(&mut self, kind: RouteKind, group: u8, replaces: Option<ConnId>, _now: u64) -> bool {
        let id = if kind == RouteKind::Bbo {
            self.slots.alloc()
        } else {
            self.md_ids.alloc()
        };
        let Some(id) = id else { return false };
        let ep = &self.st.cfg.endpoints;
        let endpoint = match kind {
            RouteKind::Bbo => ep.bbo,
            RouteKind::DepthTopN => ep.depth,
            RouteKind::Md => self
                .st
                .md_routes
                .get(group as usize)
                .copied()
                .flatten()
                .and_then(|r| ep.md_routes[usize::from(r.id.0)])
                .unwrap_or(ep.md),
        };
        let req = BuildReq {
            venue: P::VENUE,
            conn: id,
            role: ConnRole::Md,
            endpoint: EndpointId(endpoint.0),
            src_ip_idx: ep.src_ip_idx,
            rx_queue: 0,
            deadline_ns: i64::MAX,
        };
        if self.build.try_push(req).is_err() {
            self.free_id(id);
            return false;
        }
        if let Some(s) = self.building.get_mut(id.0 as usize) {
            *s = Some(Build {
                kind,
                group,
                replaces,
            });
        }
        true
    }

    /// Earliest timed action.
    fn deadline(&self, now: u64) -> Option<u64> {
        let dead = self.st.t.ms(u64::from(self.st.cfg.bbo_dead_ms));
        let mut d = None;
        if self.snapshot_supported {
            for s in &self.st.insts {
                if s.refresh == REFRESH_ACTIVE && s.refresh_req == 0 && s.members != 0 {
                    d = min_deadline(d, s.refresh_due.max(now));
                }
            }
        }
        for fc in self.conns.iter().flatten() {
            let m = &fc.m;
            if m.kind == RouteKind::Bbo {
                d = min_deadline(d, m.probe_tsc);
                d = min_deadline(
                    d,
                    m.installed
                        .max(m.last_data)
                        .saturating_add(dead)
                        .saturating_add(1),
                );
            }
            if let Some(x) = m.hb.as_ref().and_then(|h| h.next_deadline(m.last_data)) {
                d = min_deadline(d, x);
            }
            if !m.pend.is_empty() {
                d = min_deadline(d, m.next_sub.max(now));
            }
        }
        d
    }

    // ---------------------------------------------------------------- views (tests, telemetry)

    pub fn counters(&self) -> FeedCounters {
        let mut n = self.st.n;
        n.unknown_class += self.st.sink.l2_batches;
        n
    }

    pub fn raw_counters(&self) -> RawCounters {
        self.st.raw.counters()
    }

    /// The installed connection `id` (tests drive its scripted socket; telemetry reads its stats).
    #[doc(hidden)]
    pub fn conn(&mut self, id: ConnId) -> Option<&mut Conn> {
        self.conns.get_mut(id.0 as usize)?.as_mut()?.pool.get(id)
    }

    /// Installed connection ids, ascending.
    pub fn conn_ids(&self) -> impl Iterator<Item = ConnId> + '_ {
        self.conns.iter().flatten().map(|c| c.m.id)
    }

    /// System calls of the readiness probe.
    pub fn ready_sys(&self) -> Option<&SysCount> {
        self.ready.as_ref().map(|r| r.sys())
    }

    /// `InstSeq` of `inst` (tests).
    #[doc(hidden)]
    pub fn inst_seq(&self, inst: InstId) -> Option<&InstSeq> {
        self.st.seqs.get(self.st.local(inst)?)
    }

    /// Sets `last_fwd` of `inst` (test preconditions).
    #[doc(hidden)]
    pub fn set_last_fwd(&mut self, inst: InstId, stream: StreamKind, v: u64) {
        if let Some(i) = self.st.local(inst)
            && let Some(q) = self.st.seqs.get_mut(i)
            && let Some(x) = q.last_fwd.get_mut(slot_of(stream))
        {
            *x = v;
        }
    }

    /// The protocol context (tests).
    #[doc(hidden)]
    pub fn ctx(&mut self) -> &mut P::FeedCtx {
        &mut self.st.ctx
    }

    /// Retires every connection (shutdown: hft-core drain); their `RetireMsg`s go to the retire
    /// ring. Returns how many.
    pub fn retire_all(&mut self, now: u64) -> usize {
        let ids: Vec<ConnId> = self.conn_ids().collect();
        for &id in &ids {
            self.retire_conn(id, ErrBits::empty(), now);
        }
        self.flush_retire();
        ids.len()
    }

    /// Shutdown after `retire_all`: seal the raw journal before the persister stops.
    pub fn seal_and_close(self) -> Result<(), hft_journal::RollErr> {
        self.st.raw.seal_and_close()
    }

    /// Subscription items queued or awaiting answers on all connections (tests).
    #[doc(hidden)]
    pub fn pending_subs(&self) -> usize {
        self.conns
            .iter()
            .flatten()
            .map(|c| c.m.pend.len() + c.m.ack_items.len())
            .sum()
    }

    /// Starts (`true`) or stops recording the effect trace (tests).
    #[doc(hidden)]
    pub fn trace(&mut self, on: bool) {
        self.st.raw.trace = on.then(Vec::new);
    }

    /// Takes the effect trace.
    #[doc(hidden)]
    pub fn take_trace(&mut self) -> Vec<Effect> {
        self.st.raw.trace.as_mut().map(std::mem::take).unwrap_or_default()
    }

    /// The last `LoopStat`.
    pub fn last_stat(&self) -> LoopStat {
        self.last
    }
}

/// Sends one subscription frame of `m` (README §2 step 8 pacing).
fn send_sub<P: VenueProtocol>(
    st: &mut St<P>,
    m: &mut Meta,
    conn: &mut Conn,
    frames: &mut SubFrames,
    now: u64,
) -> bool {
    let Some(first) = m.pend.front().copied() else {
        return false;
    };
    let mut specs = [SubSpec {
        inst: InstId(0),
        stream: StreamKind::Bbo,
        levels: 0,
        route: m.route,
    }; SUBS_PER_CALL];
    let mut k = 0usize;
    for it in m.pend.iter() {
        if it.unsub != first.unsub || k >= SUBS_PER_CALL {
            break;
        }
        if let (Some(s), Some(inst)) = (specs.get_mut(k), st.insts.get(it.ix as usize)) {
            *s = SubSpec {
                inst: inst.id,
                stream: it.stream,
                levels: it.levels,
                route: m.route,
            };
            k += 1;
        }
    }
    frames.clear();
    let ok = P::subscribe(
        st.rd,
        specs.get(..k).unwrap_or(&[]),
        first.unsub,
        &st.cfg.venue,
        frames,
    )
    .is_ok();
    let Some((text, covered)) = frames.get(0).filter(|_| ok) else {
        // Nothing the venue can subscribe: drop the batch so the queue cannot stall.
        for _ in 0..k {
            m.pend.pop_front();
        }
        return false;
    };
    let covered = usize::from(covered).clamp(1, k);
    if conn.send_text(text, now).is_err() {
        return false;
    }
    st.raw.note(Effect::Send(m.id));
    for _ in 0..covered {
        if let Some(it) = m.pend.pop_front()
            && m.ack_items.len() < SUBQ_CAP
        {
            m.ack_items.push_back(it);
        }
    }
    m.acks.push_back((covered as u16, first.unsub));
    m.next_sub = m
        .next_sub
        .max(now.saturating_sub(m.sub_period))
        .saturating_add(m.sub_period);
    if m.next_sub <= now {
        m.next_sub = now.saturating_add(m.sub_period);
    }
    true
}

impl<P: VenueProtocol> PollTask for FeedRuntime<P> {
    fn poll_once(&mut self, now: Tsc) -> Polled {
        Polled {
            did_work: FeedRuntime::poll_once(self, now.0).did_work,
        }
    }

    fn next_deadline(&self) -> Option<Tsc> {
        self.last.next_deadline.map(Tsc)
    }

    fn wait_fds(&self, out: &mut WaitFds) {
        for fc in self.conns.iter().flatten() {
            if fc.m.fd >= 0 {
                out.push(fc.m.fd);
            }
        }
    }

    /// Commands or snap records waiting, or unprocessed bytes in a connection whose ring has room
    /// (during a full ring the socket data does not count: README §1 "环满").
    fn has_pending(&self) -> bool {
        if self.cmds.len_hint() > 0 || self.handoff.len_hint() > 0 {
            return true;
        }
        if let Some(s) = self.snap.as_ref()
            && s.has_data()
        {
            return true;
        }
        let bbo_room =
            self.st.pend_bbo.is_empty() && self.st.bbo.as_ref().is_some_and(|r| r.free_hint() >= 1);
        let md_room = self.st.pend_md.is_empty()
            && self
                .st
                .md
                .as_ref()
                .is_some_and(|r| r.free_hint() >= FRAME_EVENTS_MAX);
        self.conns.iter().flatten().any(|c| {
            c.m.buffered
                && if c.m.kind == RouteKind::Bbo {
                    bbo_room
                } else {
                    md_room
                }
        })
    }
}

/// `EventKind` re-export check (bulk bodies are `L2Snapshot`).
const _: EventKind = EventKind::L2_SNAPSHOT;
