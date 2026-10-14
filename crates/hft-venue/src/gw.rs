//! `GwRuntime<P, CAP>`: the gw loop body (README §4): per account the primary order connection
//! (every round, `drain_out` after every report frame), the user-stream connection (every round)
//! and the standby (every `gw.aux_every` rounds); `drain_out` with batch reordering, per-account
//! quotas, kill check, rate backstop, encode, send and exactly one `SendReport` per order-class
//! command; the slot mirror and client ids; reports into the rep ring.

use std::collections::VecDeque;
use std::sync::atomic::Ordering;

use hft_event::{
    ConnRole, ConnState, ConnStateKind, GwCmd, GwOp, OrderEvent, OrderEventKind, OutMsg, RawHdr, RepMsg,
    SendReport, SendStatus,
};
use hft_journal::JournalWriter;
use hft_net::{
    BuildReq, Conn, ConnId, ConnKey, ConnPool, EndpointId, ErrBits, Frame, MaskedFrame, Opcode, PoolMode,
    RetireMsg, RotatePolicy, SendErr, SendOk, TxTag,
};
use hft_refdata::{AccountKind, MarginData, RefData};
use hft_rt::{
    Backing, ByteConsumer, ByteProducer, Consumer, KillWord, PollTask, Polled, Producer, TscUtc, WaitFds,
    byte_spsc,
};
use hft_types::{AccountId, ClientOrderId, InstId, Px, Qty, SlotId, Tsc};
use hft_venue_proto::{
    Credential, DeadManOut, DecodeErr, ExchIdHint, MsgOp, OrderChannel, ReportCtl, ReportOutcome, ReportSink,
    ReqRef, RxMeta, Skip, TargetRef, VenueProtocol, WallClock,
};
use zerocopy::IntoBytes;

use crate::codes;
use crate::config::{GwCfg, InitErr, Ticks};
use crate::conn_id::{GW_FIRST, GW_LAST, IdRange};
use crate::hb::{Hb, HbSend};
use crate::hold::{Held, IdKey};
use crate::idtable::{IdTable, exch_id_value};
use crate::kill::kill_admit;
use crate::rate::RateCounters;
use crate::raw::{Effect, RawCounters, RawLog, what};
use crate::{Handover, LoopStat, min_deadline};

/// Rings, journal, kill word and clock of the gw (README "运行时入口").
#[derive(Debug)]
pub struct GwDeps {
    pub rd: &'static RefData,
    /// ← `out-<venue>`.
    pub cmds: Consumer<OutMsg>,
    /// → `rep-<venue>`.
    pub out: Producer<RepMsg>,
    /// ← `acct-<venue>` (acct-fetch records; applied from M3 on).
    pub acct: ByteConsumer,
    /// raw-gw journal; the gw is its single writer.
    pub raw: JournalWriter,
    pub kill: &'static KillWord,
    /// `GLOBAL | VENUE | HARD`.
    pub kill_mask: u32,
    /// TX stamp pairing (optional).
    pub tx_tags: Option<Producer<TxTag>>,
    pub handoff: Consumer<Handover>,
    pub retire: Producer<RetireMsg>,
    pub build: Producer<BuildReq>,
    pub clock: TscUtc,
}

/// One Live account served by this gw. The account owns its REST credential separately from
/// the non-secret templates; neither its credential nor signed wire content may be logged.
pub struct GwAccountSpec<P: VenueProtocol> {
    pub account: AccountId,
    /// Index into the instance's source addresses (the account's own egress IP).
    pub src_ip_idx: u8,
    pub rest_credential: Credential,
    pub tpl: P::Templates,
    pub ctx: P::GwCtx,
}

impl<P: VenueProtocol> core::fmt::Debug for GwAccountSpec<P> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GwAccountSpec")
            .field("account", &self.account)
            .finish()
    }
}

/// Slot mirror entry (README "出环": written on New and `MirrorRestore`, cleared on terminal reports).
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct SlotMirror {
    pub r#gen: u32,
    pub epoch: u16,
    pub account: AccountId,
    pub live: bool,
    pub inst: InstId,
    pub exch_order_id: u64,
}

impl Default for SlotMirror {
    fn default() -> Self {
        SlotMirror {
            r#gen: 0,
            epoch: 0,
            account: AccountId(0),
            live: false,
            inst: InstId(0),
            exch_order_id: 0,
        }
    }
}

/// gw counters.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct GwCounters {
    pub cmd_bad_account: u64,
    /// `drain_out` calls (instrumentation of README §4 / T13).
    pub drain_calls: u64,
    /// Standby socket reads and the `drain_out` right after each.
    pub standby_reads: u64,
    pub standby_drains: u64,
    pub user_evt_ignored: u64,
    pub user_evt_unknown: u64,
    pub report_decode_err: u64,
    pub outcome_unknown: u64,
    /// Events dropped because one frame produced more than the sink holds.
    pub sink_over: u64,
    /// Operations needing a REST pool (M3) or rep-bulk (account reports): not executed.
    pub rest_unavailable: u64,
    pub maint_ignored: u64,
    pub mirror_out_of_range: u64,
    pub rep_deferred: u64,
    pub rep_lost: u64,
    pub ring_full: u64,
    pub install_failed: u64,
    pub fee_rule_deviation: u64,
    pub hold_evicted: u64,
    pub private_unowned: u64,
}

const NO_CONN: ConnId = ConnId(0);
/// rep records waiting for ring space (never expected: the reserve covers one batch).
const REP_PEND_CAP: usize = 256;
/// Requests in flight per gw.
const INFLIGHT_CAP: usize = 4096;
/// Events one report frame may produce.
const SINK_CAP: usize = 64;
/// Orders per `encode_batch` call of the CancelAll expansion.
const BATCH_CAP: usize = 64;

#[derive(Copy, Clone, Debug)]
struct Inflight {
    conn: ConnId,
    slot: u32,
    r#gen: u32,
    epoch: u16,
    op: GwOp,
    sent: u64,
}

/// Per-connection state (order and user-stream connections).
#[derive(Copy, Clone, Debug)]
struct CMeta {
    id: ConnId,
    role: ConnRole,
    fd: i32,
    hb: Hb,
    last_data: u64,
    pong_seen: u64,
    /// Consecutive report decode failures and the first one's time.
    dec_fail: u8,
    dec_first: u64,
    buffered: bool,
}

#[path = "gw_rest.rs"]
mod rest_io;
struct Acct<P: VenueProtocol> {
    account: AccountId,
    kill_ix: u8,
    acct_bits: u32,
    src_ip_idx: u8,
    pool: ConnPool<3>,
    user: Option<ConnPool<2>>,
    metas: Vec<CMeta>,
    rest_credential: Credential,
    tpl: P::Templates,
    ctx: P::GwCtx,
    ids: IdTable,
    hold: VecDeque<Held>,
    rate: RateCounters,
    rest_weight: RateCounters,
    http: Option<rest_io::Pools>,
    sched: crate::AcctSched,
    private: crate::private::Private,
    quota: u16,
    rotate: bool,
    rotate_since: Option<u64>,
    rotate_bits: u32,
    /// Old primary retired at this time after a planned switch.
    retire_at: Option<(ConnId, u64)>,
}

/// One per order venue (README "运行时入口"). `CAP` ≥ `P::ORDER_FRAME_CAP`.
pub struct GwRuntime<P: VenueProtocol, const CAP: usize = 2048> {
    cfg: GwCfg,
    rd: &'static RefData,
    t: Ticks,
    accts: Vec<Acct<P>>,
    rest_ip: Vec<(u8, RateCounters)>,
    mirror: Vec<SlotMirror>,
    cmds: Consumer<OutMsg>,
    out: Producer<RepMsg>,
    pend_rep: VecDeque<RepMsg>,
    bulk: Option<ByteProducer>,
    bulk_tx: ByteProducer,
    bulk_rx: ByteConsumer,
    acct: ByteConsumer,
    acct_req: Option<Producer<crate::AcctReq>>,
    acct_queries: [&'static [hft_venue_proto::AcctQuery]; 2],
    raw: RawLog,
    kill: &'static KillWord,
    kill_mask: u32,
    tx_tags: Option<Producer<TxTag>>,
    handoff: Consumer<Handover>,
    retire: Producer<RetireMsg>,
    retire_pend: VecDeque<RetireMsg>,
    build: Producer<BuildReq>,
    building: Vec<Option<(usize, ConnRole, usize)>>,
    ids: IdRange,
    frame: Box<MaskedFrame<CAP>>,
    private_frames: hft_venue_proto::SubFrames,
    txcopy: Box<[u8]>,
    staged: VecDeque<GwCmd>,
    batch: Vec<GwCmd>,
    inflight: Vec<Inflight>,
    sink_evs: Vec<RepMsg>,
    sink_bulk: Vec<u8>,
    sink_spans: [(usize, usize); SINK_CAP],
    sink_keys: [IdKey; SINK_CAP],
    dm: DeadManOut,
    insts_buf: Vec<InstId>,
    epoch: u16,
    accept_prev: bool,
    prev_epoch: u16,
    round: u64,
    cmds_done: u64,
    venue_insts: Box<[InstId]>,
    n: GwCounters,
    last: LoopStat,
    rest: Option<(
        Producer<crate::order_rest::Request>,
        Consumer<crate::order_rest::Reply>,
    )>,
}

impl<P: VenueProtocol, const CAP: usize> core::fmt::Debug for GwRuntime<P, CAP> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GwRuntime")
            .field("venue", &P::VENUE)
            .field("accounts", &self.accts.len())
            .finish()
    }
}

fn is_order(op: GwOp) -> bool {
    matches!(
        op,
        GwOp::Place | GwOp::Cancel | GwOp::Amend | GwOp::CancelAll | GwOp::Query
    )
}

/// Report sink of one frame: buffers events, records exchange ids in the account's `IdTable`.
struct GwSink<'a> {
    evs: &'a mut Vec<RepMsg>,
    bulk: &'a mut Vec<u8>,
    spans: &'a mut [(usize, usize); SINK_CAP],
    keys: &'a mut [IdKey; SINK_CAP],
    ids: &'a mut IdTable,
    mirror: &'a mut [SlotMirror],
    account: AccountId,
    now: u64,
    epoch: u16,
    prev: Option<u16>,
    over: bool,
    unsupported: u32,
    key: [u8; 36],
    key_len: usize,
}

impl GwSink<'_> {
    fn push(&mut self, m: RepMsg) {
        if self.evs.len() < SINK_CAP {
            self.spans[self.evs.len()] = (0, 0);
            self.keys[self.evs.len()] = IdKey::new(&self.key[..self.key_len]);
            self.evs.push(m);
        } else {
            self.over = true;
        }
    }
    fn push_bulk(&mut self, msg: RepMsg, bytes: &[u8]) {
        if self.bulk.len() + bytes.len() > self.bulk.capacity() || self.evs.len() == SINK_CAP {
            self.over = true;
            return;
        }
        let ix = self.evs.len();
        self.push(msg);
        let start = self.bulk.len();
        self.bulk.extend_from_slice(bytes);
        self.spans[ix] = (start, bytes.len());
    }
    fn remember(&mut self, key: &[u8]) {
        self.key_len = 0;
        if key.len() > self.key.len() {
            return;
        }
        let n = key.len();
        if let (Some(d), Some(s)) = (self.key.get_mut(..n), key.get(..n)) {
            d.copy_from_slice(s);
            self.key_len = n;
        }
    }
}

impl ReportSink for GwSink<'_> {
    fn order(&mut self, e: &OrderEvent) {
        self.push(RepMsg::OrderEvent(*e));
    }
    fn position(&mut self, e: &hft_event::PositionReport) {
        self.push(RepMsg::PositionReport(*e));
    }
    fn balance(&mut self, e: &hft_event::BalanceReport) {
        self.push(RepMsg::BalanceReport(*e));
    }
    fn funding_settle(&mut self, e: &hft_event::FundingSettle) {
        self.push(RepMsg::FundingSettle(*e));
    }
    fn external_fill(&mut self, e: &hft_event::ExternalFill) {
        self.push(RepMsg::ExternalFill(*e));
    }
    fn margin(&mut self, hdr: &hft_event::MarginReportHdr, items: &[hft_event::MarginItem]) {
        if usize::from(hdr.n) != items.len() || hdr.n > hft_event::MarginReportHdr::MAX_ITEMS {
            self.over = true;
            return;
        }
        self.push_bulk(RepMsg::MarginReport(*hdr), items.as_bytes());
    }
    fn account_config(&mut self, hdr: &hft_event::AccountConfigReportHdr, lev: &[hft_event::LeverageItem]) {
        if usize::from(hdr.n_lev) != lev.len() || hdr.n_lev > hft_event::AccountConfigReportHdr::MAX_ITEMS {
            self.over = true;
            return;
        }
        self.push_bulk(RepMsg::AccountConfigReport(*hdr), lev.as_bytes());
    }
    fn exch_id(&mut self, key: &[u8], owner: Option<&ClientOrderId>) -> ExchIdHint {
        self.remember(key);
        match owner {
            Some(id) => {
                if id.account != self.account || (id.epoch != self.epoch && Some(id.epoch) != self.prev) {
                    return ExchIdHint::Unknown;
                }
                let recorded = self
                    .ids
                    .insert(key, id.epoch, id.slot, id.r#gen, self.now)
                    .is_ok();
                if let Some(m) = self.mirror.get_mut(id.slot as usize)
                    && m.live
                    && m.r#gen == id.r#gen
                    && m.epoch == id.epoch
                    && m.account == self.account
                {
                    m.exch_order_id = exch_id_value(key);
                }
                if recorded {
                    ExchIdHint::Recorded
                } else {
                    ExchIdHint::Unknown
                }
            }
            None => match self.ids.get(key) {
                Some((epoch, slot, r#gen)) => ExchIdHint::Resolved {
                    epoch,
                    slot: SlotId(slot),
                    r#gen,
                },
                None => ExchIdHint::Unknown,
            },
        }
    }
}

impl<P: VenueProtocol, const CAP: usize> GwRuntime<P, CAP> {
    /// Startup (may allocate): checks the configuration and the accounts, builds the slot mirror and
    /// the per-account `IdTable`s. (The raw-gw dedup reseed, README §6a, arrives with M3.)
    pub fn new(cfg: GwCfg, deps: GwDeps, accts: Vec<GwAccountSpec<P>>) -> Result<Self, InitErr> {
        for (v, k) in [
            (u32::from(cfg.cmds_per_loop), "gw.cmds_per_loop"),
            (
                u32::from(cfg.cmds_per_account_per_loop),
                "gw.cmds_per_account_per_loop",
            ),
            (u32::from(cfg.aux_every), "gw.aux_every"),
            (u32::from(cfg.unmatched_hold_cap), "gw.unmatched_hold_cap"),
            (
                u32::from(cfg.acct_query_degraded_after),
                "gw.acct_query_degraded_after",
            ),
            (
                u32::from(cfg.report_decode_down_after),
                "gw.report_decode_down_after",
            ),
        ] {
            if v == 0 {
                return Err(InitErr::Cfg(k));
            }
        }
        let rd = deps.rd;
        if accts
            .iter()
            .any(|a| matches!(rd.account(a.account).kind, AccountKind::Sim(_)))
        {
            return Err(InitErr::SimAccount);
        }
        if CAP < P::ORDER_FRAME_CAP {
            return Err(InitErr::FrameCap);
        }
        let need = 2u64 * (u64::from(cfg.oms_slots) + u64::from(cfg.id_tomb_cap));
        if !cfg.id_table_cap.is_power_of_two() || u64::from(cfg.id_table_cap) < need {
            return Err(InitErr::IdTableCap);
        }
        if let MarginData::Poll { interval_ms } = rd.caps(P::VENUE).margin_data
            && interval_ms < cfg.margin_poll_min_ms
        {
            return Err(InitErr::MarginPoll);
        }
        let vm = rd.venue(P::VENUE);
        if vm.ws_max_conn_age_s != 0 && cfg.rotate_margin_s >= vm.ws_max_conn_age_s {
            return Err(InitErr::RotateMargin);
        }
        let t = Ticks { clock: deps.clock };
        let stagger = t.ms(3_600_000);
        let limits = rd.rate_limits(P::VENUE);
        let mut list = Vec::with_capacity(accts.len());
        for a in accts {
            let ix = rd
                .venue_account_ix(P::VENUE, a.account)
                .ok_or(InitErr::DepsMismatch)?;
            if list.iter().any(|x: &Acct<P>| x.account == a.account) {
                return Err(InitErr::DepsMismatch);
            }
            let bits = hft_types::KillBits::account(ix).0 | hft_types::KillBits::account_hard(ix).0;
            list.push(Acct {
                account: a.account,
                kill_ix: ix,
                acct_bits: bits,
                src_ip_idx: a.src_ip_idx,
                pool: ConnPool::new(PoolMode::PrimaryStandby, 2, RotatePolicy { stagger_tsc: stagger }),
                user: P::USER_STREAM
                    .then(|| ConnPool::new(PoolMode::Race, 1, RotatePolicy { stagger_tsc: stagger })),
                metas: Vec::with_capacity(8),
                rest_credential: a.rest_credential,
                tpl: a.tpl,
                ctx: a.ctx,
                ids: IdTable::new(cfg.id_table_cap, cfg.id_tomb_cap, t.ms(u64::from(cfg.id_tomb_ms))),
                hold: VecDeque::with_capacity(usize::from(cfg.unmatched_hold_cap)),
                rate: RateCounters::new(limits, |ms| t.ms(ms)),
                private: crate::private::Private::default(),
                http: None,
                rest_weight: RateCounters::account_rest(limits, |ms| t.ms(ms)),
                sched: crate::AcctSched::new(
                    a.account,
                    P::MARKET,
                    rd.caps(P::VENUE).margin_data,
                    &cfg,
                    |ms| t.ms(ms),
                ),
                quota: 0,
                rotate: false,
                rotate_since: None,
                rotate_bits: 0,
                retire_at: None,
            });
        }
        let mut rest_ip = Vec::new();
        for x in &list {
            if !rest_ip.iter().any(|(ip, _)| *ip == x.src_ip_idx) {
                rest_ip.push((x.src_ip_idx, RateCounters::rest(limits, |ms| t.ms(ms))));
            }
        }
        let mut building = Vec::with_capacity(256);
        building.resize(256, None);
        let mut venue_insts: Vec<InstId> = rd.insts_of_venue(P::VENUE).to_vec();
        venue_insts.sort_unstable();
        let insts_cap = venue_insts.len().max(1);
        let (bulk_tx, bulk_rx) = byte_spsc(1 << 19, Backing::Heap).map_err(|_| InitErr::DepsMismatch)?;
        Ok(GwRuntime {
            cfg,
            rd,
            t,
            accts: list,
            rest_ip,
            mirror: vec![SlotMirror::default(); cfg.oms_slots as usize],
            cmds: deps.cmds,
            out: deps.out,
            pend_rep: VecDeque::with_capacity(REP_PEND_CAP),
            bulk: None,
            bulk_tx,
            bulk_rx,
            acct: deps.acct,
            acct_req: None,
            acct_queries: [&[], &[]],
            raw: RawLog::new(deps.raw, P::VENUE, t),
            kill: deps.kill,
            kill_mask: deps.kill_mask,
            tx_tags: deps.tx_tags,
            handoff: deps.handoff,
            retire: deps.retire,
            retire_pend: VecDeque::with_capacity(64),
            build: deps.build,
            building,
            ids: IdRange::new(GW_FIRST, GW_LAST),
            frame: Box::new(MaskedFrame::new()),
            private_frames: hft_venue_proto::SubFrames::with_capacity(CAP * 8, 8),
            txcopy: vec![0u8; CAP].into_boxed_slice(),
            staged: VecDeque::with_capacity(usize::from(cfg.cmds_per_loop)),
            batch: Vec::with_capacity(usize::from(cfg.cmds_per_loop)),
            inflight: Vec::with_capacity(INFLIGHT_CAP),
            sink_evs: Vec::with_capacity(SINK_CAP),
            sink_bulk: Vec::with_capacity(65536),
            sink_spans: [(0, 0); SINK_CAP],
            sink_keys: [IdKey::default(); SINK_CAP],
            dm: DeadManOut {
                frame: Default::default(),
                reqs: Vec::with_capacity(insts_cap),
            },
            insts_buf: Vec::with_capacity(insts_cap),
            epoch: 0,
            accept_prev: false,
            prev_epoch: 0,
            round: 0,
            cmds_done: 0,
            venue_insts: venue_insts.into_boxed_slice(),
            n: GwCounters::default(),
            last: LoopStat::default(),
            rest: None,
        })
    }

    fn acct_ix(&self, a: AccountId) -> Option<usize> {
        self.accts.iter().position(|x| x.account == a)
    }

    fn wall(&self, now: u64) -> WallClock {
        let ns = self.t.utc_ns(now);
        WallClock {
            unix_ms: ns.div_euclid(1_000_000),
            unix_us: ns.div_euclid(1000),
        }
    }

    /// Startup: pair the variable report lane with the sequencer's rep consumer.
    pub fn set_rep_bulk(&mut self, bulk: ByteProducer) {
        self.bulk = Some(bulk);
    }

    fn flush_reports(&mut self) {
        while let Some(m) = self.pend_rep.front().copied() {
            if self.out.free_hint() == 0 {
                break;
            }
            if matches!(m, RepMsg::MarginReport(_) | RepMsg::AccountConfigReport(_)) {
                let Some(body) = self.bulk_rx.peek() else { break };
                let Some(dst) = self.bulk.as_mut().and_then(|b| b.reserve(body.len() as u32)) else {
                    break;
                };
                dst.copy_from_slice(body);
                if let Some(b) = &mut self.bulk {
                    b.commit();
                }
                self.bulk_rx.advance();
            }
            // Single producer: the free slot cannot disappear after the check above.
            if self.out.try_push(m).is_err() {
                break;
            }
            self.pend_rep.pop_front();
        }
    }

    fn push_rep(&mut self, m: RepMsg) {
        self.raw.note(Effect::Rep);
        if self.pend_rep.is_empty() && self.out.try_push(m).is_ok() {
            return;
        }
        if self.pend_rep.len() < REP_PEND_CAP {
            self.pend_rep.push_back(m);
            self.n.rep_deferred += 1;
        } else {
            self.n.rep_lost += 1;
        }
    }

    fn push_sink(&mut self, k: usize, m: RepMsg) {
        if !matches!(m, RepMsg::MarginReport(_) | RepMsg::AccountConfigReport(_)) {
            self.push_rep(m);
            return;
        }
        let (start, len) = self.sink_spans[k];
        let Some(bytes) = self.sink_bulk.get(start..start + len) else {
            return;
        };
        let Some(dst) = self.bulk_tx.reserve(len as u32) else {
            self.n.rep_lost += 1;
            return;
        };
        dst.copy_from_slice(bytes);
        self.bulk_tx.commit();
        self.raw.note(Effect::Rep);
        self.pend_rep.push_back(m);
        self.flush_reports();
    }

    fn conn_state(
        &mut self,
        account: AccountId,
        role: ConnRole,
        conn: ConnId,
        state: ConnStateKind,
        reason: u32,
        rx: u64,
    ) {
        self.push_rep(RepMsg::ConnState(ConnState {
            venue: P::VENUE,
            conn: conn.0,
            state,
            role,
            inst: InstId(0),
            reason,
            account,
            _pad2: [0; 3],
            rx_tsc: rx,
        }));
    }

    /// One round (README §4 "一圈的顺序").
    pub fn set_rest(
        &mut self,
        tx: Producer<crate::order_rest::Request>,
        rx: Consumer<crate::order_rest::Reply>,
    ) {
        self.rest = Some((tx, rx));
    }
    fn rest_reply(&mut self, r: crate::order_rest::Reply, now: u64) {
        // A queued request is not evidence of a write. A response proves it reached the wire;
        // transport errors leave the query outcome unknown.
        self.report(
            &r.cmd,
            self.target_gen(&r.cmd),
            NO_CONN,
            if r.status == 0 {
                SendStatus::WriteError
            } else {
                SendStatus::Written
            },
            0,
            0,
        );
        self.decode_rest_reply(
            r,
            NO_CONN,
            hft_net::Timestamps {
                rx_tsc: now,
                ..Default::default()
            },
            now,
        );
    }
    fn decode_rest_reply(
        &mut self,
        r: crate::order_rest::Reply,
        conn: ConnId,
        ts: hft_net::Timestamps,
        now: u64,
    ) {
        let Some(a) = self.accts.iter().position(|a| a.account == r.account) else {
            return;
        };
        let Some(x) = self.accts.get_mut(a) else { return };
        self.sink_evs.clear();
        self.sink_bulk.clear();
        let mut sink = GwSink {
            evs: &mut self.sink_evs,
            bulk: &mut self.sink_bulk,
            spans: &mut self.sink_spans,
            keys: &mut self.sink_keys,
            ids: &mut x.ids,
            mirror: &mut self.mirror,
            account: r.account,
            now,
            epoch: self.epoch,
            prev: self.accept_prev.then_some(self.prev_epoch),
            over: false,
            unsupported: 0,
            key: [0; 36],
            key_len: 0,
        };
        let rx = RxMeta {
            account: r.account,
            conn,
            rx_tsc: ts.rx_tsc,
            hw_rx_ns: ts.hw_rx_ns,
            sw_rx_ns: ts.sw_rx_ns,
            rx_empty_gap_ns: ts.rx_empty_gap_ns,
            ..RxMeta::default()
        };
        let result = if r.status == 0 {
            Err(DecodeErr::Shape)
        } else {
            P::decode_rest(
                self.rd,
                &mut x.ctx,
                &r.origin,
                r.status,
                r.body.get(..usize::from(r.len)).unwrap_or(&[]),
                &rx,
                &mut sink,
            )
        };
        let (key, n, over) = (sink.key, sink.key_len, sink.over);
        let outcome = match result {
            Ok(o) if !over => o,
            _ => {
                self.sink_evs.clear();
                self.sink_bulk.clear();
                self.n.report_decode_err += 1;
                ReportOutcome {
                    outcome_unknown: r.origin.req,
                    ..ReportOutcome::default()
                }
            }
        };
        self.apply_outcome(
            a,
            conn,
            ConnRole::Rest,
            &outcome,
            r.origin
                .req
                .zip(r.identity)
                .map(|(req, id)| (req, id, r.origin.inst)),
            now,
            0,
            key.get(..n).unwrap_or(&[]),
            now,
        );
    }
    pub fn poll_once(&mut self, now: u64) -> LoopStat {
        let mut ls = LoopStat::default();
        if self.pend_rep.is_empty()
            && self.out.free_hint() > SINK_CAP
            && let Some(reply) = self.rest.as_mut().and_then(|(_, rx)| rx.try_pop())
        {
            self.rest_reply(reply, now);
            ls.did_work = true;
        }
        self.flush_reports();
        while let Some(m) = self.retire_pend.front().copied() {
            if self.retire.try_push(m).is_err() {
                break;
            }
            self.retire_pend.pop_front();
        }
        while let Some(h) = self.handoff.try_pop() {
            ls.did_work = true;
            self.handover(h, now);
        }
        let aux = self.round.is_multiple_of(u64::from(self.cfg.aux_every));
        self.round = self.round.wrapping_add(1);
        let reserve = usize::from(self.cfg.cmds_per_loop);
        if self.out.free_hint() <= reserve || !self.pend_rep.is_empty() {
            ls.ring_full += 1;
            self.n.ring_full += 1;
        }
        for a in self.accts.iter_mut() {
            a.quota = 0;
        }
        let mut frames = 0u16;
        let cmds0 = self.cmds_done;
        for a in 0..self.accts.len() {
            // 1. primary.
            let pid = self
                .accts
                .get_mut(a)
                .and_then(|x| x.pool.primary().map(|c| c.id()));
            frames += self.read_conn(a, pid, ConnRole::Order, now);
            // 2. user stream.
            if P::USER_STREAM {
                let uid = self
                    .accts
                    .get_mut(a)
                    .and_then(|x| x.user.as_mut())
                    .and_then(|p| p.primary().map(|c| c.id()));
                let uid = self.accts[a].private.owner.or(uid);
                if uid.is_some() {
                    frames += self.read_conn(a, uid, ConnRole::UserStream, now);
                }
            }
            let candidate = self.accts[a].private.candidate.map(|c| c.id);
            let owner = self.accts[a].private.owner;
            if let Some(id) = candidate
                && Some(id) != pid
                && (!P::USER_STREAM || Some(id) != owner)
            {
                frames += self.read_conn(
                    a,
                    Some(id),
                    if P::USER_STREAM {
                        ConnRole::UserStream
                    } else {
                        ConnRole::Order
                    },
                    now,
                );
            }
            // 3. standby every aux round.
            if aux {
                let sid = self
                    .accts
                    .get_mut(a)
                    .and_then(|x| x.pool.standby().map(|c| c.id()));
                if sid.is_some() && sid != candidate {
                    self.n.standby_reads += 1;
                    frames += self.read_conn(a, sid, ConnRole::Order, now);
                    self.n.standby_drains += 1;
                }
            }
        }
        if aux {
            ls.did_work |= self.poll_http(now);
        }
        let cmds = self.cmds_done.wrapping_sub(cmds0);
        let maint = self.maintain(now);
        for a in 0..self.accts.len() {
            self.release_holds(a, now);
        }
        // Account decoding runs on the warm worker; only its bounded records reach this thread.
        if aux {
            ls.did_work |= self.schedule_accounts(now);
        }
        if self.out.free_hint() > reserve && self.pend_rep.is_empty() {
            ls.did_work |= self.apply_acct(now);
        }
        ls.frames = frames;
        ls.cmds = u16::try_from(cmds).unwrap_or(u16::MAX);
        ls.did_work |= frames > 0 || cmds > 0 || maint;
        ls.next_deadline = self.deadline();
        self.last = ls;
        ls
    }

    /// Startup-only wiring of the bounded account worker and its immutable protocol query table.
    pub fn set_account_queries(&mut self, tx: Producer<crate::AcctReq>)
    where
        P: hft_venue_proto::AcctProtocol,
    {
        self.acct_queries = [
            P::acct_queries(hft_venue_proto::AcctQueryKind::Config),
            P::acct_queries(hft_venue_proto::AcctQueryKind::Margin),
        ];
        self.acct_req = Some(tx);
    }

    pub fn account_session(&mut self, account: AccountId, status: u8, uid: [u8; 24]) {
        if let Some(x) = self.accts.iter_mut().find(|x| x.account == account) {
            x.sched.session(status, uid);
        }
    }

    fn rest_available(&mut self, a: usize, weight: u32, urgent: bool, now: u64) -> bool {
        let x = &mut self.accts[a];
        x.rest_weight.weight_available(weight, urgent, now)
            && self
                .rest_ip
                .iter_mut()
                .find(|(ip, _)| *ip == x.src_ip_idx)
                .is_some_and(|(_, r)| r.weight_available(weight, urgent, now))
    }
    fn rest_charge(&mut self, a: usize, weight: u32) {
        let x = &mut self.accts[a];
        x.rest_weight.charge_weight(weight);
        if let Some((_, r)) = self.rest_ip.iter_mut().find(|(ip, _)| *ip == x.src_ip_idx) {
            r.charge_weight(weight);
        }
    }
    fn schedule_accounts(&mut self, now: u64) -> bool {
        if self.acct_req.is_none() {
            return false;
        }
        let mut did = false;
        for a in 0..self.accts.len() {
            let x = &mut self.accts[a];
            let ready = x.pool.primary().is_some() && x.private.owner.is_some();
            if let Some((req, weight)) = x
                .sched
                .due(now, ready, self.acct_queries[0], self.acct_queries[1])
                && self.rest_available(a, weight, req.why == 2, now)
                && self.acct_req.as_mut().is_some_and(|tx| tx.try_push(req).is_ok())
            {
                self.rest_charge(a, weight);
                self.accts[a].sched.started(req);
                did = true;
            }
        }
        did
    }

    fn apply_acct(&mut self, now: u64) -> bool {
        let Some(record) = self.acct.peek() else {
            return false;
        };
        if let Some(e) = crate::funding::read_funding_record(record) {
            self.acct.advance();
            if e.venue == P::VENUE
                && e.synth == 0
                && self.acct_ix(e.account).is_some()
                && self.rd.inst(e.inst).venue == P::VENUE
            {
                self.push_rep(RepMsg::FundingSettle(e));
            } else {
                self.n.report_decode_err += 1;
            }
            return true;
        }
        let Some(h) = crate::AcctRecHdr::read(record) else {
            self.acct.advance();
            self.n.report_decode_err += 1;
            return true;
        };
        let Some(a) = self.accts.iter().position(|x| x.account == h.account) else {
            self.acct.advance();
            self.n.report_decode_err += 1;
            return true;
        };
        if !matches!(self.accts[a].sched.state(),crate::AcctState::Running(r) if r.seq_no==h.seq_no && r.kind==h.kind)
        {
            self.acct.advance();
            self.n.report_decode_err += 1;
            return true;
        }
        let mut invalid = false;
        match h.what {
            crate::acct_wire::RAW => {
                let start = crate::acct_wire::HEADER;
                let n = h.req_len as usize;
                if h.parts == 0
                    || h.part >= h.parts
                    || h.body_len as usize > crate::ACCT_RAW_CHUNK
                    || (h.part != 0 && n != 0)
                {
                    invalid = true;
                } else {
                    if n > 0 {
                        self.raw.tx(
                            h.conn,
                            what::REST_RESPONSE,
                            RawHdr::REDACTED,
                            &record[start..start + n],
                            h.sent_tsc,
                        );
                    }
                    self.raw.rx(
                        h.conn,
                        what::REST_RESPONSE,
                        0,
                        0,
                        h.ts.best_rx().0,
                        h.ts.flags,
                        &record[start + n..],
                        h.ts.rx_tsc,
                    );
                }
            }
            crate::acct_wire::DONE => {
                self.sink_evs.clear();
                self.sink_bulk.clear();
                let mut rest = &record[crate::acct_wire::HEADER..];
                while !rest.is_empty() {
                    let Some((m, tail, n)) = crate::acct_wire::report(rest) else {
                        invalid = true;
                        break;
                    };
                    let identity = match m {
                        RepMsg::MarginReport(e) => (e.venue, e.account),
                        RepMsg::AccountConfigReport(e) => (e.venue, e.account),
                        RepMsg::PositionReport(e) => (e.venue, e.account),
                        RepMsg::BalanceReport(e) => (e.venue, e.account),
                        RepMsg::FundingSettle(e) => (e.venue, e.account),
                        _ => {
                            invalid = true;
                            break;
                        }
                    };
                    let inst_ok = match m {
                        RepMsg::PositionReport(e) => self.rd.insts_of_venue(P::VENUE).contains(&e.inst),
                        RepMsg::FundingSettle(e) => {
                            e.inst.0 == 0 || self.rd.insts_of_venue(P::VENUE).contains(&e.inst)
                        }
                        RepMsg::MarginReport(_) | RepMsg::AccountConfigReport(_) => {
                            tail.chunks_exact(16).all(|row| {
                                let id = u32::from_le_bytes([row[0], row[1], row[2], row[3]]);
                                self.rd.insts_of_venue(P::VENUE).contains(&InstId(id))
                            })
                        }
                        _ => true,
                    };
                    if !inst_ok
                        || identity != (P::VENUE, h.account)
                        || self.sink_evs.len() == SINK_CAP
                        || self.sink_bulk.len() + tail.len() > self.sink_bulk.capacity()
                    {
                        invalid = true;
                        break;
                    }
                    let k = self.sink_evs.len();
                    self.sink_spans[k] = (self.sink_bulk.len(), tail.len());
                    self.sink_bulk.extend_from_slice(tail);
                    self.sink_evs.push(m);
                    rest = &rest[n..];
                }
            }
            crate::acct_wire::FAILED => {}
            _ => invalid = true,
        }
        self.acct.advance();
        if invalid || h.what == crate::acct_wire::FAILED {
            if invalid {
                self.n.report_decode_err += 1;
            }
            if self.accts[a]
                .sched
                .failed(h.seq_no, now, self.cfg.acct_query_degraded_after)
            {
                self.conn_state(
                    h.account,
                    ConnRole::Rest,
                    h.conn,
                    ConnStateKind::Degraded,
                    codes::ACCT_QUERY,
                    now,
                );
            }
        } else if h.what == crate::acct_wire::DONE {
            for k in 0..self.sink_evs.len() {
                let m = self.sink_evs[k];
                self.push_sink(k, m);
            }
            if self.accts[a].sched.done(h.seq_no, now) == Some(true) {
                self.conn_state(h.account, ConnRole::Rest, h.conn, ConnStateKind::Up, 0, now);
            }
        }
        true
    }

    /// Reads one connection of account `a`: `poll_rx`, then frame by frame with a `drain_out` after
    /// each; one `drain_out` when there was none. Returns the frames processed.
    fn read_conn(&mut self, a: usize, id: Option<ConnId>, role: ConnRole, now: u64) -> u16 {
        let reserve = usize::from(self.cfg.cmds_per_loop);
        let mut n = 0u16;
        if let Some(id) = id
            && self.out.free_hint() > reserve
            && self.pend_rep.is_empty()
        {
            if let Some(c) = self.conn_of(a, id) {
                c.poll_rx(now);
            }
            loop {
                if self.out.free_hint() <= reserve || !self.pend_rep.is_empty() {
                    break;
                }
                if !self.report_frame(a, id, role, now) {
                    break;
                }
                n += 1;
                self.drain_out(now);
            }
        }
        if n == 0 {
            self.drain_out(now);
        }
        if let Some(id) = id {
            let mut buffered = false;
            let mut pong = 0;
            if let Some(c) = self.conn_of(a, id) {
                c.finish_poll(now);
                buffered = c.rx_buffered() > 0;
                pong = c.last_pong_tsc();
            }
            if let Some(m) = self
                .accts
                .get_mut(a)
                .and_then(|x| x.metas.iter_mut().find(|m| m.id == id))
            {
                m.buffered = buffered;
                if pong > m.pong_seen {
                    m.pong_seen = pong;
                    m.hb.on_pong();
                }
            }
        }
        n
    }

    fn conn_of(&mut self, a: usize, id: ConnId) -> Option<&mut Conn> {
        let x = self.accts.get_mut(a)?;
        if x.metas
            .iter()
            .any(|m| m.id == id && m.role == ConnRole::UserStream)
        {
            return x.user.as_mut()?.get(id);
        }
        x.pool.get(id)
    }

    /// Decodes one report frame of `id`; `false` = no complete frame.
    fn report_frame(&mut self, a: usize, id: ConnId, role: ConnRole, now: u64) -> bool {
        let Some(x) = self.accts.get_mut(a) else {
            return false;
        };
        let accepted = x.private.accepts(id);
        let candidate = x.private.candidate.is_some_and(|c| c.id == id);
        let user = x
            .metas
            .iter()
            .any(|m| m.id == id && m.role == ConnRole::UserStream);
        let account = x.account;
        let Acct {
            pool,
            user: upool,
            ctx,
            ids,
            ..
        } = x;
        let conn = if user {
            upool.as_mut().and_then(|p| p.get(id))
        } else {
            pool.get(id)
        };
        let Some(conn) = conn else { return false };
        let Some(f) = conn.next_frame() else { return false };
        let private_ack = P::private_ack(f.payload);
        let class = P::classify_report(
            if f.opcode == Opcode::Binary {
                MsgOp::Binary
            } else {
                MsgOp::Text
            },
            f.payload.get(..64).unwrap_or(f.payload),
        );
        let private_push = P::is_private_report(class);
        let discard = private_push && !accepted;
        self.sink_evs.clear();
        self.sink_bulk.clear();
        let mut sink = GwSink {
            evs: &mut self.sink_evs,
            bulk: &mut self.sink_bulk,
            spans: &mut self.sink_spans,
            keys: &mut self.sink_keys,
            ids,
            mirror: &mut self.mirror,
            account,
            now,
            epoch: self.epoch,
            prev: self.accept_prev.then_some(self.prev_epoch),
            over: false,
            unsupported: 0,
            key: [0; 36],
            key_len: 0,
        };
        let res = if discard || private_ack.is_some() {
            Ok(ReportOutcome::default())
        } else {
            decode_one::<P>(self.rd, ctx, &f, id, account, &mut sink)
        };
        let (over, unsupported, key, key_len) = (sink.over, sink.unsupported, sink.key, sink.key_len);
        let ts = f.ts;
        self.raw.rx(
            id,
            what::WS_TEXT,
            0,
            0,
            ts.best_rx().0,
            ts.flags,
            f.payload,
            ts.rx_tsc,
        );
        if over {
            self.n.sink_over += 1;
        }
        self.n.rest_unavailable += u64::from(unsupported);
        if let Some(m) = self
            .accts
            .get_mut(a)
            .and_then(|x| x.metas.iter_mut().find(|m| m.id == id))
        {
            m.last_data = ts.rx_tsc;
        }
        if discard {
            self.n.private_unowned += 1;
        }
        if let Some((bit, ok)) = private_ack
            && candidate
        {
            if !ok {
                self.private_abort(a, id, now);
            } else if self.accts[a].private.ack(id, bit) {
                self.private_ready(a, id, now);
            }
        } else if candidate && private_push && res.is_ok() && self.accts[a].private.ready(id) {
            self.private_ready(a, id, now);
        }
        match if over { Err(DecodeErr::Overflow) } else { res } {
            Ok(o) => {
                if let Some(m) = self
                    .accts
                    .get_mut(a)
                    .and_then(|x| x.metas.iter_mut().find(|m| m.id == id))
                {
                    m.dec_fail = 0;
                }
                self.apply_outcome(
                    a,
                    id,
                    role,
                    &o,
                    None,
                    ts.rx_tsc,
                    ts.best_rx().0,
                    &key[..key_len.min(36)],
                    now,
                );
            }
            Err(_) => {
                self.n.report_decode_err += 1;
                self.conn_state(
                    account,
                    role,
                    id,
                    ConnStateKind::Degraded,
                    codes::REPORT_DECODE,
                    ts.rx_tsc,
                );
                let win = self.t.ms(60_000);
                let down_after = self.cfg.report_decode_down_after;
                let mut fatal = false;
                if let Some(m) = self
                    .accts
                    .get_mut(a)
                    .and_then(|x| x.metas.iter_mut().find(|m| m.id == id))
                {
                    if m.dec_fail == 0 || now.saturating_sub(m.dec_first) > win {
                        m.dec_fail = 1;
                        m.dec_first = now;
                    } else {
                        m.dec_fail = m.dec_fail.saturating_add(1);
                    }
                    fatal = m.dec_fail >= down_after;
                }
                if fatal && let Some(c) = self.conn_of(a, id) {
                    c.mark(ErrBits::IDLE_TIMEOUT);
                }
            }
        }
        true
    }

    /// Publishes the frame's events and applies the outcome (README §4 step 2).
    #[allow(clippy::too_many_arguments)]
    fn apply_outcome(
        &mut self,
        a: usize,
        id: ConnId,
        role: ConnRole,
        o: &ReportOutcome,
        rest_identity: Option<(ReqRef, ClientOrderId, InstId)>,
        rx: u64,
        hw: i64,
        _key: &[u8],
        now: u64,
    ) {
        let account = self.accts.get(a).map_or(AccountId(0), |x| x.account);
        let request_identity = rest_identity
            .filter(|(r, cid, _)| {
                cid.account == account && cid.venue == P::VENUE && cid.slot == r.slot && cid.r#gen == r.r#gen
            })
            .map(|(r, cid, inst)| (r, cid.epoch, inst))
            .or_else(|| {
                o.req
                    .or(o.outcome_unknown)
                    .and_then(|r| self.request_identity(id, account, r))
                    .map(|(r, m)| (r, m.epoch, m.inst))
            });
        for k in 0..self.sink_evs.len() {
            let Some(mut m) = self.sink_evs.get(k).copied() else {
                break;
            };
            if let RepMsg::OrderEvent(e) = &mut m {
                // Some request responses omit the client id (and therefore epoch).
                // Only an outstanding request on this connection can fill that gap.
                if e.epoch == 0
                    && let Some((r, epoch, inst)) = request_identity
                    && e.slot.0 == r.slot
                    && e.r#gen == r.r#gen
                    && e.account == account
                    && e.venue == P::VENUE
                    && (e.inst.0 == 0 || inst.0 == 0 || e.inst == inst)
                {
                    e.epoch = epoch;
                    if e.inst.0 == 0 {
                        e.inst = inst;
                    }
                }
                let accepted = e.epoch == self.epoch || (self.accept_prev && e.epoch == self.prev_epoch);
                if e.slot != SlotId::UNRESOLVED && !accepted {
                    e.slot = SlotId::UNRESOLVED;
                    e.r#gen = 0;
                }
                let key = self.sink_keys[k];
                if e.slot == SlotId::UNRESOLVED
                    && matches!(
                        e.what,
                        OrderEventKind::PartialFill | OrderEventKind::Fill | OrderEventKind::FeeOnly
                    )
                {
                    let cap = usize::from(self.cfg.unmatched_hold_cap);
                    let evicted = self
                        .accts
                        .get_mut(a)
                        .and_then(|x| (x.hold.len() == cap).then(|| x.hold.pop_front()).flatten());
                    if let Some(old) = evicted {
                        self.n.hold_evicted += 1;
                        self.push_rep(crate::hold::expired(old.event));
                    }
                    if let Some(x) = self.accts.get_mut(a) {
                        x.hold.push_back(Held {
                            event: *e,
                            key,
                            since: now,
                        });
                    }
                    continue;
                }
                self.order_terminal(a, e, key, now);
            }
            self.push_sink(k, m);
        }
        self.release_holds(a, now);
        if let Some(x) = self.accts.get_mut(a) {
            x.sched.hint(o.hint);
            if self
                .sink_evs
                .iter()
                .any(|m| matches!(m, RepMsg::MarginReport(_) | RepMsg::PositionReport(_)))
            {
                x.sched.pushed(now);
            }
        }
        if let Some(r) = o.req {
            self.inflight_done(id, r);
        }
        if let Some(obs) = o.rate
            && let Some(x) = self.accts.get_mut(a)
        {
            x.rate.observe_window(obs, now);
        }
        if let Some(r) = o.outcome_unknown {
            self.n.outcome_unknown += 1;
            let identity = request_identity.filter(|(req, _, _)| *req == r);
            let mut e = blank_event(P::VENUE, account);
            e.what = OrderEventKind::OutcomeUnknown;
            if let Some((_, epoch, inst)) = identity {
                e.slot = SlotId(r.slot);
                e.r#gen = r.r#gen;
                e.epoch = epoch;
                e.inst = inst;
            } else {
                e.slot = SlotId::UNRESOLVED;
            }
            e.reject_code = o.raw_code;
            e.rx_tsc = rx;
            e.hw_rx_ns = hw;
            self.push_rep(RepMsg::OrderEvent(e));
        }
        match o.skip {
            Skip::No => {}
            Skip::Listed => self.n.user_evt_ignored += 1,
            Skip::Unknown => self.n.user_evt_unknown += 1,
        }
        if o.fee_rule_deviation {
            self.n.fee_rule_deviation += 1;
        }
        match o.ctl {
            ReportCtl::None => {}
            ReportCtl::ClockSkew => {
                self.conn_state(account, role, id, ConnStateKind::Degraded, codes::CLOCK_SKEW, rx)
            }
            ReportCtl::RateLimited => {
                self.conn_state(account, role, id, ConnStateKind::Degraded, codes::RATE_LIMIT, rx)
            }
            ReportCtl::UserStreamEnded => {
                if P::USER_STREAM {
                    self.retire_conn(a, id, ErrBits::PEER_EOF, now);
                } else if self.accts[a].private.owner == Some(id) {
                    self.accts[a].private.owner = None;
                    self.conn_state(account, ConnRole::UserStream, id, ConnStateKind::Down, 0, now);
                    self.private_start(a, id, now);
                }
            }
            ReportCtl::ServerShutdown => {
                if role == ConnRole::UserStream
                    && let Some(c) = self.conn_of(a, id)
                {
                    c.mark(ErrBits::SERVER_HINT);
                }
                if let Some(x) = self.accts.get_mut(a) {
                    x.rotate = true;
                    x.rotate_bits |= ErrBits::SERVER_HINT.bits();
                }
            }
        }
    }

    fn order_terminal(&mut self, a: usize, e: &OrderEvent, key: IdKey, now: u64) {
        if e.slot == SlotId::UNRESOLVED
            || !matches!(
                e.what,
                OrderEventKind::Fill
                    | OrderEventKind::CancelAck
                    | OrderEventKind::Reject
                    | OrderEventKind::Expired
            )
        {
            return;
        }
        if let Some(mm) = self.mirror.get_mut(e.slot.0 as usize)
            && mm.r#gen == e.r#gen
            && mm.epoch == e.epoch
            && mm.account == e.account
        {
            mm.live = false;
        }
        if !key.bytes().is_empty()
            && exch_id_value(key.bytes()) == e.exch_order_id
            && let Some(x) = self.accts.get_mut(a)
        {
            x.ids.terminal(key.bytes(), now);
        }
    }

    fn release_holds(&mut self, a: usize, now: u64) {
        let timeout = self.t.ms(u64::from(self.cfg.unmatched_hold_ms));
        let mut i = 0;
        while self.pend_rep.is_empty() && self.out.free_hint() > usize::from(self.cfg.cmds_per_loop) {
            let Some(x) = self.accts.get_mut(a) else {
                break;
            };
            let Some(h) = x.hold.get(i).copied() else {
                break;
            };
            let owner = x.ids.get(h.key.bytes()).filter(|(epoch, _, _)| {
                *epoch == self.epoch || (self.accept_prev && *epoch == self.prev_epoch)
            });
            if owner.is_none() && now.saturating_sub(h.since) <= timeout {
                i += 1;
                continue;
            }
            x.hold.remove(i);
            if let Some((epoch, slot, r#gen)) = owner {
                let mut e = h.event;
                e.epoch = epoch;
                e.slot = SlotId(slot);
                e.r#gen = r#gen;
                self.order_terminal(a, &e, h.key, now);
                self.push_rep(RepMsg::OrderEvent(e));
            } else {
                self.push_rep(crate::hold::expired(h.event));
            }
        }
    }

    fn inflight_done(&mut self, conn: ConnId, r: ReqRef) {
        if let Some(i) = self
            .inflight
            .iter()
            .position(|x| x.conn == conn && x.slot == r.slot && x.r#gen == r.r#gen && x.op == r.op)
        {
            self.inflight.swap_remove(i);
        }
    }

    fn request_identity(&self, conn: ConnId, account: AccountId, r: ReqRef) -> Option<(ReqRef, SlotMirror)> {
        let mirror = *self.mirror.get(r.slot as usize)?;
        if !mirror.live || mirror.account != account || mirror.r#gen != r.r#gen {
            return None;
        }
        let mut requests = self
            .inflight
            .iter()
            .filter(|f| f.conn == conn && f.slot == r.slot && f.r#gen == r.r#gen && f.op == r.op);
        let request = requests.next()?;
        if requests.next().is_some() || request.epoch != mirror.epoch {
            return None;
        }
        Some((r, mirror))
    }

    // ---------------------------------------------------------------- drain_out

    /// README §4 step 1: staged commands of accounts with quota left, then up to
    /// min(`cmds_per_loop`, staged room, rep room) from the out ring, reordered within the batch.
    fn drain_out(&mut self, now: u64) {
        self.n.drain_calls += 1;
        let quota = self.cfg.cmds_per_account_per_loop;
        // Staged first.
        let mut k = 0;
        while k < self.staged.len() {
            if self.out.free_hint() == 0 {
                return;
            }
            let Some(c) = self.staged.get(k).copied() else {
                break;
            };
            let has_quota = self
                .acct_ix(c.account)
                .and_then(|a| self.accts.get(a))
                .is_none_or(|x| x.quota < quota);
            if has_quota {
                self.staged.remove(k);
                self.process(c, now);
            } else {
                k += 1;
            }
        }
        let cap = usize::from(self.cfg.cmds_per_loop);
        let room = cap.saturating_sub(self.staged.len()).min(self.out.free_hint());
        if room == 0 {
            return;
        }
        self.batch.clear();
        while self.batch.len() < room {
            let Some(c) = self.cmds.try_pop() else { break };
            self.batch.push(c);
        }
        if self.batch.is_empty() {
            return;
        }
        reorder(&mut self.batch);
        for i in 0..self.batch.len() {
            let Some(c) = self.batch.get(i).copied() else {
                break;
            };
            if is_order(c.op)
                && let Some(a) = self.acct_ix(c.account)
                && self.accts.get(a).is_some_and(|x| x.quota >= quota)
            {
                self.staged.push_back(c);
                continue;
            }
            self.process(c, now);
        }
    }

    fn process(&mut self, c: GwCmd, now: u64) {
        self.cmds_done += 1;
        if !is_order(c.op) {
            self.maintenance(c, now);
            return;
        }
        let Some(a) = self.acct_ix(c.account) else {
            self.n.cmd_bad_account += 1;
            self.report(&c, None, NO_CONN, SendStatus::NoConn, 0, 0);
            return;
        };
        if let Some(x) = self.accts.get_mut(a) {
            x.quota += 1;
        }
        let (bits, ix) = self.accts.get(a).map_or((0, 0), |x| (x.acct_bits, x.kill_ix));
        // Acquire pairs with the risk side's Release store of the kill word.
        let word = self.kill.load(Ordering::Acquire);
        if !kill_admit(word, self.kill_mask, bits, ix, c.op, c.oflags) {
            self.report(&c, self.target_gen(&c), NO_CONN, SendStatus::KillBlocked, 0, 0);
            return;
        }
        if c.op != GwOp::Query && !self.accts.get_mut(a).is_some_and(|x| x.rate.take(now)) {
            self.report(&c, self.target_gen(&c), NO_CONN, SendStatus::RateLimited, 0, 0);
            return;
        }
        if c.op == GwOp::CancelAll && P::order_channel(c.op) != OrderChannel::Rest {
            self.cancel_all(a, &c, now);
            return;
        }
        // Target from the slot mirror.
        let tgt = if matches!(c.op, GwOp::Place | GwOp::CancelAll) {
            TargetRef {
                id: self.cid(c.account, c.slot.0, c.r#gen, self.epoch),
                exch_order_id: 0,
            }
        } else {
            match self.mirror_target(&c) {
                Some(t) => t,
                None => {
                    let st = if c.op == GwOp::Query {
                        SendStatus::QueryUnsupported
                    } else {
                        SendStatus::EncodeError
                    };
                    self.report(&c, None, NO_CONN, st, 0, 0);
                    return;
                }
            }
        };
        let clk = self.wall(now);
        if P::order_channel(c.op) == OrderChannel::Rest {
            if self.accts[a].http.is_some() {
                self.order_http(a, c, tgt, now);
                return;
            }
            let sent = self.rest.as_mut().is_some_and(|(tx, _)| {
                tx.try_push(crate::order_rest::Request { cmd: c, target: tgt })
                    .is_ok()
            });
            if !sent {
                self.n.rest_unavailable += 1;
            }
            if !sent {
                self.report(&c, self.target_gen(&c), NO_CONN, SendStatus::NoConn, 0, 0);
            }
            return;
        }
        let x = match self.accts.get(a) {
            Some(x) => x,
            None => return,
        };
        self.frame.unseal();
        let r = P::encode(self.rd, &x.tpl, &c, &tgt, &clk, self.frame.body_mut());
        let len = match r {
            Ok(n) => n,
            Err(_) => {
                let st = if c.op == GwOp::Query {
                    SendStatus::QueryUnsupported
                } else {
                    SendStatus::EncodeError
                };
                self.report(&c, self.target_gen(&c), NO_CONN, st, 0, 0);
                return;
            }
        };
        let tg = self.target_gen(&c);
        match self.send_frame(a, len, now) {
            Ok((ok, conn)) => {
                if c.op == GwOp::Place {
                    self.write_mirror(&c);
                }
                let (slot, r#gen) = if c.op == GwOp::Place {
                    (c.slot.0, c.r#gen)
                } else {
                    (c.target_slot.0, tg.unwrap_or(0))
                };
                self.track(conn, slot, r#gen, c.op, ok.sent_tsc);
                self.tx_tag(conn, slot, r#gen, ok.sent_tsc);
                self.report(&c, tg, conn, SendStatus::Written, 0, ok.sent_tsc);
                self.raw_tx(conn, len, ok.sent_tsc);
            }
            Err((SendErr::Io(e), conn, at)) => {
                if c.op == GwOp::Place {
                    self.write_mirror(&c);
                }
                self.report(&c, tg, conn, SendStatus::WriteError, e, at);
            }
            Err((_, conn, at)) => self.report(&c, tg, conn, SendStatus::NoConn, 0, at),
        }
    }

    fn cid(&self, account: AccountId, slot: u32, r#gen: u32, epoch: u16) -> ClientOrderId {
        ClientOrderId {
            epoch,
            venue: P::VENUE,
            account,
            slot,
            r#gen,
        }
    }

    fn mirror_target(&self, c: &GwCmd) -> Option<TargetRef> {
        let m = self.mirror.get(c.target_slot.0 as usize)?;
        if !m.live || m.account != c.account {
            return None;
        }
        Some(TargetRef {
            id: self.cid(c.account, c.target_slot.0, m.r#gen, m.epoch),
            exch_order_id: m.exch_order_id,
        })
    }

    /// `target_gen` from the mirror (Cancel/Amend/Query), `None` for Place/CancelAll or absent.
    fn target_gen(&self, c: &GwCmd) -> Option<u32> {
        match c.op {
            GwOp::Cancel | GwOp::Amend | GwOp::Query => self
                .mirror
                .get(c.target_slot.0 as usize)
                .filter(|m| m.live && m.account == c.account)
                .map(|m| m.r#gen),
            _ => None,
        }
    }

    fn write_mirror(&mut self, c: &GwCmd) {
        match self.mirror.get_mut(c.slot.0 as usize) {
            Some(m) => {
                *m = SlotMirror {
                    r#gen: c.r#gen,
                    epoch: self.epoch,
                    account: c.account,
                    live: true,
                    inst: c.inst,
                    exch_order_id: 0,
                };
            }
            None => self.n.mirror_out_of_range += 1,
        }
    }

    /// Seals the frame and sends it on the primary; `NotOpen`/`TxBacklog` (nothing encrypted) →
    /// once more on the standby. Returns the connection used.
    fn send_frame(
        &mut self,
        a: usize,
        len: usize,
        now: u64,
    ) -> Result<(SendOk, ConnId), (SendErr, ConnId, u64)> {
        let len = len.min(CAP.saturating_sub(MaskedFrame::<CAP>::HDR_MAX));
        // Plaintext copy for RawTx (the frame is masked in place).
        if let (Some(d), Some(s)) = (self.txcopy.get_mut(..len), self.frame.body_mut().get(..len)) {
            d.copy_from_slice(s);
        }
        let Some(x) = self.accts.get_mut(a) else {
            return Err((SendErr::NotOpen(hft_net::ConnPhase::Free), NO_CONN, now));
        };
        let mut last = (SendErr::NotOpen(hft_net::ConnPhase::Free), NO_CONN);
        for which in 0..2 {
            let conn = if which == 0 {
                x.pool.primary()
            } else {
                x.pool.standby()
            };
            let Some(conn) = conn else { continue };
            let id = conn.id();
            if !self.frame.as_bytes().is_empty() {
                self.frame.unseal();
            }
            let key = conn.next_mask();
            self.frame.seal(Opcode::Text, len, key);
            match conn.send_frame(&*self.frame) {
                Ok(ok) => {
                    self.raw.note(Effect::Send(id));
                    return Ok((ok, id));
                }
                Err(e @ (SendErr::NotOpen(_) | SendErr::TxBacklog | SendErr::NotSealed)) => {
                    last = (e, id);
                    // Unmask for a retry on the other connection (seal is in place).
                    self.frame.unseal();
                    if let (Some(d), Some(s)) = (self.frame.body_mut().get_mut(..len), self.txcopy.get(..len))
                    {
                        d.copy_from_slice(s);
                    }
                }
                Err(e) => return Err((e, id, now)),
            }
        }
        Err((last.0, NO_CONN, now))
    }

    fn track(&mut self, conn: ConnId, slot: u32, r#gen: u32, op: GwOp, sent: u64) {
        if self.inflight.len() < INFLIGHT_CAP {
            self.inflight.push(Inflight {
                conn,
                slot,
                r#gen,
                epoch: self.mirror.get(slot as usize).map_or(self.epoch, |m| m.epoch),
                op,
                sent,
            });
        }
    }

    fn tx_tag(&mut self, conn: ConnId, slot: u32, r#gen: u32, sent: u64) {
        if let Some(t) = self.tx_tags.as_mut() {
            let _ = t.try_push(TxTag {
                key: ConnKey {
                    venue: P::VENUE,
                    conn,
                    _pad: 0,
                },
                tskey_end: 0,
                slot,
                r#gen,
                sent_tsc: sent,
            });
        }
    }

    fn raw_tx(&mut self, conn: ConnId, len: usize, sent: u64) {
        if P::needs_per_order_signing() {
            self.raw.tx(conn, what::WS_TEXT, RawHdr::REDACTED, &[], sent);
        } else {
            let p = self.txcopy.get(..len).unwrap_or(&[]);
            self.raw.tx(conn, what::WS_TEXT, 0, p, sent);
        }
    }

    /// The one `SendReport` of an order-class command (README §4 step 6; VEN-199 field rules).
    fn report(
        &mut self,
        c: &GwCmd,
        tg: Option<u32>,
        conn: ConnId,
        status: SendStatus,
        errno: i32,
        sent: u64,
    ) {
        let (slot, r#gen, ts, tgen) = match c.op {
            GwOp::Place => (c.slot, c.r#gen, SlotId(0), 0),
            GwOp::CancelAll => (SlotId(0), 0, SlotId(0), 0),
            _ => (c.target_slot, tg.unwrap_or(0), c.target_slot, tg.unwrap_or(0)),
        };
        self.push_rep(RepMsg::SendReport(SendReport {
            decision_seq: c.decision_seq,
            sent_tsc: Tsc(sent),
            slot,
            r#gen,
            target_slot: ts,
            target_gen: tgen,
            venue: P::VENUE,
            account: c.account,
            conn,
            status,
            op: c.op,
            _pad: [0; 3],
            os_errno: errno,
            _pad2: 0,
        }));
    }

    /// `CancelAll` expansion (README "出环" table): account level → per inst with live orders →
    /// per order (`encode_batch` when accepted, else one `encode` each); one `SendReport`.
    fn cancel_all(&mut self, a: usize, c: &GwCmd, now: u64) {
        let clk = self.wall(now);
        let none = TargetRef {
            id: self.cid(c.account, 0, 0, self.epoch),
            exch_order_id: 0,
        };
        let mut status = SendStatus::Written;
        let mut conn_used = NO_CONN;
        let mut sent_at = 0;
        // 1. account level.
        let mut c0 = *c;
        c0.inst = InstId(0);
        let r = match self.accts.get(a) {
            Some(x) => {
                self.frame.unseal();
                P::encode(self.rd, &x.tpl, &c0, &none, &clk, self.frame.body_mut())
            }
            None => return,
        };
        let mut level = 0u8;
        match r {
            Ok(len) => {
                self.cancel_send(a, len, now, &mut status, &mut conn_used, &mut sent_at);
            }
            Err(hft_venue_proto::EncodeErr::Unsupported(GwOp::CancelAll)) => level = 1,
            Err(_) => status = SendStatus::EncodeError,
        }
        if level == 1 {
            // 2. per inst with live orders of this account (ascending).
            self.insts_buf.clear();
            for m in self.mirror.iter() {
                if m.live
                    && m.account == c.account
                    && !self.insts_buf.contains(&m.inst)
                    && self.insts_buf.len() < self.insts_buf.capacity()
                {
                    self.insts_buf.push(m.inst);
                }
            }
            self.insts_buf.sort_unstable();
            for k in 0..self.insts_buf.len() {
                let Some(inst) = self.insts_buf.get(k).copied() else {
                    break;
                };
                let mut ci = *c;
                ci.inst = inst;
                let r = match self.accts.get(a) {
                    Some(x) => {
                        self.frame.unseal();
                        P::encode(self.rd, &x.tpl, &ci, &none, &clk, self.frame.body_mut())
                    }
                    None => return,
                };
                match r {
                    Ok(len) => self.cancel_send(a, len, now, &mut status, &mut conn_used, &mut sent_at),
                    Err(hft_venue_proto::EncodeErr::Unsupported(GwOp::CancelAll)) => {
                        level = 2;
                        break;
                    }
                    Err(_) => {
                        status = SendStatus::EncodeError;
                    }
                }
            }
        }
        if level == 2 {
            // 3. per order.
            let mut cs = [*c; BATCH_CAP];
            let mut tg = [none; BATCH_CAP];
            let mut k = 0usize;
            let n_slots = self.mirror.len();
            let mut s = 0usize;
            while s <= n_slots {
                let item = self.mirror.get(s).copied();
                let flush = item.is_none() || k == BATCH_CAP;
                if flush && k > 0 {
                    self.cancel_orders(
                        a,
                        &cs[..k],
                        &tg[..k],
                        &clk,
                        now,
                        &mut status,
                        &mut conn_used,
                        &mut sent_at,
                    );
                    k = 0;
                }
                let Some(m) = item else { break };
                if m.live && m.account == c.account {
                    let mut cc = *c;
                    cc.op = GwOp::Cancel;
                    cc.target_slot = SlotId(s as u32);
                    cc.inst = m.inst;
                    if let (Some(dc), Some(dt)) = (cs.get_mut(k), tg.get_mut(k)) {
                        *dc = cc;
                        *dt = TargetRef {
                            id: self.cid(c.account, s as u32, m.r#gen, m.epoch),
                            exch_order_id: m.exch_order_id,
                        };
                        k += 1;
                    }
                }
                s += 1;
            }
        }
        if P::CANCEL_ALL_SWEEP {
            self.sweep(a, *c, now);
        }
        self.report(c, None, conn_used, status, 0, sent_at);
    }

    #[allow(clippy::too_many_arguments)]
    fn cancel_orders(
        &mut self,
        a: usize,
        cs: &[GwCmd],
        tg: &[TargetRef],
        clk: &WallClock,
        now: u64,
        status: &mut SendStatus,
        conn_used: &mut ConnId,
        sent_at: &mut u64,
    ) {
        let r = match self.accts.get(a) {
            Some(x) => {
                self.frame.unseal();
                P::encode_batch(self.rd, &x.tpl, cs, tg, clk, self.frame.body_mut())
            }
            None => return,
        };
        if let Ok(len) = r {
            self.cancel_send(a, len, now, status, conn_used, sent_at);
            return;
        }
        for (c, t) in cs.iter().zip(tg.iter()) {
            let r = match self.accts.get(a) {
                Some(x) => {
                    self.frame.unseal();
                    P::encode(self.rd, &x.tpl, c, t, clk, self.frame.body_mut())
                }
                None => return,
            };
            match r {
                Ok(len) => self.cancel_send(a, len, now, status, conn_used, sent_at),
                Err(_) => {
                    if *status == SendStatus::Written {
                        *status = SendStatus::EncodeError;
                    }
                }
            }
        }
    }

    fn cancel_send(
        &mut self,
        a: usize,
        len: usize,
        now: u64,
        status: &mut SendStatus,
        conn_used: &mut ConnId,
        sent_at: &mut u64,
    ) {
        match self.send_frame(a, len, now) {
            Ok((ok, conn)) => {
                self.track(conn, 0, 0, GwOp::CancelAll, ok.sent_tsc);
                self.raw_tx(conn, len, ok.sent_tsc);
                if *conn_used == NO_CONN {
                    *conn_used = conn;
                    *sent_at = ok.sent_tsc;
                }
            }
            Err((SendErr::Io(_), conn, at)) => {
                if *status == SendStatus::Written {
                    *status = SendStatus::WriteError;
                    *conn_used = conn;
                    *sent_at = at;
                }
            }
            Err((_, _, at)) => {
                if *status == SendStatus::Written {
                    *status = SendStatus::NoConn;
                    *sent_at = at;
                }
            }
        }
    }

    /// Maintenance ops (README "出环" table).
    fn maintenance(&mut self, c: GwCmd, now: u64) {
        match c.op {
            GwOp::SetEpoch => {
                self.epoch = c.epoch();
                self.accept_prev = c.accept_prev();
                self.prev_epoch = c.prev_epoch();
            }
            GwOp::MirrorRestore => {
                let ok_inst = self.venue_insts.binary_search(&c.inst).is_ok();
                let Some(a) = self.acct_ix(c.account) else {
                    self.n.cmd_bad_account += 1;
                    return;
                };
                if !ok_inst || c.slot.0 as usize >= self.mirror.len() {
                    self.conn_state(
                        c.account,
                        ConnRole::Order,
                        NO_CONN,
                        ConnStateKind::Degraded,
                        codes::MIRROR,
                        now,
                    );
                    return;
                }
                if let Some(m) = self.mirror.get_mut(c.slot.0 as usize) {
                    *m = SlotMirror {
                        r#gen: c.r#gen,
                        epoch: c.epoch(),
                        account: c.account,
                        live: true,
                        inst: c.inst,
                        exch_order_id: c.exch_order_id(),
                    };
                }
                if c.exch_order_id() != 0
                    && let Some(x) = self.accts.get_mut(a)
                {
                    let mut buf = [0u8; 20];
                    let key = itoa(c.exch_order_id(), &mut buf);
                    let _ = x.ids.insert(key, c.epoch(), c.slot.0, c.r#gen, now);
                }
            }
            GwOp::ResetRate => {
                if let Some(x) = self.acct_ix(c.account).and_then(|a| self.accts.get_mut(a)) {
                    x.rate.reset();
                }
            }
            GwOp::Rotate => {
                if let Some(x) = self.acct_ix(c.account).and_then(|a| self.accts.get_mut(a)) {
                    x.rotate = true;
                }
            }
            GwOp::DeadManRefresh => self.dead_man(c, now),
            GwOp::SessionKeepalive => self.keepalive(c, now),
            GwOp::Resubscribe => {
                if let Some(a) = self.acct_ix(c.account)
                    && let Some(id) = self.accts[a].private.owner
                {
                    self.accts[a].private.owner = None;
                    self.conn_state(c.account, ConnRole::UserStream, id, ConnStateKind::Down, 0, now);
                    self.private_start(a, id, now);
                }
            }
            _ => self.n.maint_ignored += 1,
        }
    }

    fn dead_man(&mut self, c: GwCmd, now: u64) {
        let Some(a) = self.acct_ix(c.account) else {
            self.n.cmd_bad_account += 1;
            return;
        };
        if self.accts[a].http.is_none()
            && let Some((tx, _)) = &mut self.rest
        {
            let request = crate::order_rest::Request {
                cmd: c,
                target: TargetRef {
                    id: ClientOrderId {
                        epoch: self.epoch,
                        venue: P::VENUE,
                        account: c.account,
                        slot: 0,
                        r#gen: 0,
                    },
                    exch_order_id: 0,
                },
            };
            if tx.try_push(request).is_err() {
                self.conn_state(
                    c.account,
                    ConnRole::Order,
                    NO_CONN,
                    ConnStateKind::Degraded,
                    codes::DEADMAN,
                    now,
                );
            }
            return;
        }
        self.insts_buf.clear();
        for m in self.mirror.iter() {
            if m.live
                && m.account == c.account
                && !self.insts_buf.contains(&m.inst)
                && self.insts_buf.len() < self.insts_buf.capacity()
            {
                self.insts_buf.push(m.inst);
            }
        }
        let clk = self.wall(now);
        self.dm.frame.clear();
        self.dm.reqs.clear();
        let r = match self.accts.get(a) {
            Some(x) => P::dead_man_refresh_signed(
                self.rd,
                &x.tpl,
                &x.rest_credential,
                c.arg,
                &self.insts_buf,
                &clk,
                &mut self.dm,
            ),
            None => return,
        };
        let mut ok = r.is_ok();
        if ok && !self.dm.frame.is_empty() {
            let len = self
                .dm
                .frame
                .len()
                .min(CAP.saturating_sub(MaskedFrame::<CAP>::HDR_MAX));
            self.frame.unseal();
            if let (Some(d), Some(s)) = (
                self.frame.body_mut().get_mut(..len),
                self.dm.frame.as_slice().get(..len),
            ) {
                d.copy_from_slice(s);
            }
            match self.send_frame(a, len, now) {
                Ok((s, conn)) => self.raw_tx(conn, len, s.sent_tsc),
                Err(_) => ok = false,
            }
        }
        if ok && !self.dm.reqs.is_empty() {
            for k in 0..self.dm.reqs.len() {
                let wire = self.dm.reqs[k];
                let origin = hft_venue_proto::RestOrigin {
                    what: hft_venue_proto::RestWhat::DeadMan,
                    req: None,
                    inst: InstId(0),
                };
                if self.submit_http(a, c, origin, &wire, now).is_err() {
                    ok = false;
                }
            }
        }
        if !ok {
            let account = c.account;
            self.conn_state(
                account,
                ConnRole::Order,
                NO_CONN,
                ConnStateKind::Degraded,
                codes::DEADMAN,
                now,
            );
        }
    }

    fn private_send(&mut self, a: usize, id: ConnId, unsub: bool, now: u64) -> bool {
        self.private_frames.clear();
        // Use disjoint borrows: the frame store is immutable while the connection writes.
        let x = &mut self.accts[a];
        let conn = if P::USER_STREAM {
            x.user.as_mut().and_then(|u| u.get(id))
        } else {
            x.pool.get(id)
        };
        let Some(conn) = conn else { return false };
        if !crate::private::copy_frames(conn.handoff_data(), unsub, &mut self.private_frames) {
            return P::private_channels() == 0;
        }
        for k in 0..self.private_frames.len() {
            let Some((bytes, _)) = self.private_frames.get(k) else {
                return false;
            };
            if conn.send_text(bytes, now).is_err() {
                return false;
            }
            self.raw.tx(id, what::WS_TEXT, RawHdr::REDACTED, &[], now);
        }
        true
    }
    fn private_start(&mut self, a: usize, id: ConnId, now: u64) {
        if self.accts[a].private.candidate.is_some() {
            return;
        }
        self.accts[a].private.begin(id, P::private_channels(), now);
        if !self.private_send(a, id, false, now) {
            self.private_abort(a, id, now);
            return;
        }
        // An implicit subscription is authenticated by the connection's bootstrap. Replacement
        // user streams still wait for their first private event before retiring the old stream.
        if P::private_channels() == 0 && (!P::USER_STREAM || self.accts[a].private.owner.is_none()) {
            self.private_ready(a, id, now);
        }
    }
    fn private_ready(&mut self, a: usize, id: ConnId, now: u64) {
        let old = self.accts[a].private.owner.replace(id);
        self.accts[a].private.candidate = None;
        if self.accts[a].private.ever_ready {
            self.accts[a].sched.hint(hft_venue_proto::AcctHint::MarginNow);
            self.accts[a].sched.hint(hft_venue_proto::AcctHint::ConfigRecheck);
        }
        self.accts[a].private.ever_ready = true;
        if let Some(old) = old
            && old != id
        {
            if !P::USER_STREAM {
                self.accts[a].private.retiring = Some(old);
            }
            self.private_send(a, old, true, now);
            if P::USER_STREAM {
                self.retire_conn(a, old, ErrBits::ROTATE_DUE, now);
            }
        }
        self.conn_state(
            self.accts[a].account,
            ConnRole::UserStream,
            id,
            ConnStateKind::Up,
            0,
            now,
        );
    }
    fn private_abort(&mut self, a: usize, id: ConnId, now: u64) {
        self.private_send(a, id, true, now);
        self.accts[a].private.candidate = None;
        self.accts[a].private.retry_at =
            now.saturating_add(self.t.ms(u64::from(self.cfg.rotate_wait_max_ms)));
        self.conn_state(
            self.accts[a].account,
            ConnRole::UserStream,
            id,
            ConnStateKind::Degraded,
            codes::SUB_REJECTED,
            now,
        );
        if P::USER_STREAM {
            self.retire_conn(a, id, ErrBits::IDLE_TIMEOUT, now);
        }
    }
    pub fn private_owner(&self, account: AccountId) -> Option<ConnId> {
        self.accts.iter().find(|a| a.account == account)?.private.owner
    }

    // ---------------------------------------------------------------- connections and timers

    fn handover(&mut self, h: Handover, now: u64) {
        match h {
            Handover::Failed(id) => {
                if let Some((a, role, endpoint)) = self.building.get_mut(id.0 as usize).and_then(Option::take)
                {
                    self.ids.free(id);
                    if let Some(x) = self.accts.get_mut(a) {
                        match role {
                            ConnRole::Rest => {
                                if let Some((_, p)) =
                                    x.http.as_mut().and_then(|r| r.endpoints.get_mut(endpoint))
                                {
                                    p.build_failed();
                                }
                            }
                            ConnRole::UserStream => {
                                if let Some(u) = x.user.as_mut() {
                                    u.build_failed();
                                }
                            }
                            _ => x.pool.build_failed(),
                        }
                    }
                }
            }
            Handover::Ready(h) => {
                let id = h.id();
                let Some((a, role, endpoint)) = self.building.get_mut(id.0 as usize).and_then(Option::take)
                else {
                    self.n.install_failed += 1;
                    return;
                };
                if role == ConnRole::Rest {
                    let ok = self.accts[a]
                        .http
                        .as_mut()
                        .and_then(|r| r.endpoints.get_mut(endpoint))
                        .is_some_and(|(_, p)| p.install(h).is_ok());
                    if !ok {
                        self.n.install_failed += 1;
                        self.ids.free(id);
                    }
                    return;
                }
                let age = u64::from(self.rd.venue(P::VENUE).ws_max_conn_age_s);
                let rot = (age != 0).then(|| {
                    now.saturating_add(
                        self.t
                            .ms(age.saturating_sub(u64::from(self.cfg.rotate_margin_s)) * 1000),
                    )
                });
                let hb_role = if role == ConnRole::UserStream {
                    ConnRole::UserStream
                } else {
                    ConnRole::Order
                };
                let spec = P::heartbeat(hb_role);
                let t = self.t;
                let Some(x) = self.accts.get_mut(a) else { return };
                let res = match role {
                    ConnRole::UserStream => match x.user.as_mut() {
                        Some(u) => u.install(h, rot).map(|_| u.get(id).map_or(-1, |c| c.raw_fd())),
                        None => Err(hft_net::PoolErr::WrongMode),
                    },
                    _ => x
                        .pool
                        .install(h, rot)
                        .map(|_| x.pool.get(id).map_or(-1, |c| c.raw_fd())),
                };
                let Ok(fd) = res else {
                    self.n.install_failed += 1;
                    self.ids.free(id);
                    return;
                };
                x.metas.push(CMeta {
                    id,
                    role,
                    fd,
                    hb: Hb::new(&spec, &t, now, role == ConnRole::Order),
                    last_data: now,
                    pong_seen: 0,
                    dec_fail: 0,
                    dec_first: 0,
                    buffered: false,
                });
                let account = x.account;
                let mut session_failed = false;
                let conn = if role == ConnRole::UserStream {
                    x.user.as_mut().and_then(|u| u.get(id))
                } else {
                    x.pool.get(id)
                };
                if let Some(data) = conn.map(|c| c.handoff_data())
                    && data.get(..8) == Some(b"PRIV0001")
                    && data.len() >= 33
                {
                    let mut uid = [0; 24];
                    uid.copy_from_slice(&data[9..33]);
                    if data[8] != 0 {
                        x.sched.session(data[8], uid);
                        session_failed = data[8] == 2;
                    }
                }
                if session_failed {
                    self.conn_state(
                        account,
                        ConnRole::Rest,
                        id,
                        ConnStateKind::Degraded,
                        codes::ACCT_QUERY,
                        now,
                    );
                }
                if role == ConnRole::Order {
                    self.conn_state(account, role, id, ConnStateKind::Up, 0, now);
                }
                let primary = self.accts[a].pool.primary().map(|c| c.id());
                if (P::USER_STREAM && role == ConnRole::UserStream)
                    || (!P::USER_STREAM && primary == Some(id))
                {
                    self.private_start(a, id, now);
                }
                self.raw
                    .rx(id, what::CONNECT, 0, 0, 0, 0, &[account.0, role as u8], now);
            }
        }
    }

    /// Retires connection `id` of account `a` and pushes `Down` (README §4 step 4).
    fn retire_conn(&mut self, a: usize, id: ConnId, why: ErrBits, now: u64) {
        let Some(x) = self.accts.get_mut(a) else { return };
        let role = x
            .metas
            .iter()
            .find(|m| m.id == id)
            .map_or(ConnRole::Order, |m| m.role);
        let r = match role {
            ConnRole::UserStream => x.user.as_mut().map(|u| u.retire(id, why)),
            _ => Some(x.pool.retire(id, why)),
        };
        x.metas.retain(|m| m.id != id);
        let owner = x.private.owner == Some(id);
        if owner {
            x.private.owner = None;
        }
        if x.private.retiring == Some(id) {
            x.private.retiring = None;
        }
        if x.private.candidate.is_some_and(|c| c.id == id) {
            x.private.candidate = None;
        }
        let account = x.account;
        if let Some(Ok(msg)) = r
            && self.retire.try_push(msg).is_err()
        {
            self.retire_pend.push_back(msg);
        }
        self.ids.free(id);
        self.inflight.retain(|f| f.conn != id);
        self.raw
            .rx(id, what::DISCONNECT, 0, 0, 0, 0, &[account.0, role as u8], now);
        if role != ConnRole::UserStream || owner {
            self.conn_state(account, role, id, ConnStateKind::Down, why.bits(), now);
        }
        if owner {
            if role != ConnRole::UserStream {
                self.conn_state(
                    account,
                    ConnRole::UserStream,
                    id,
                    ConnStateKind::Down,
                    why.bits(),
                    now,
                );
            }
            let next = if P::USER_STREAM {
                self.accts[a]
                    .user
                    .as_mut()
                    .and_then(|u| u.primary().map(|c| c.id()))
            } else {
                self.accts[a].pool.primary().map(|c| c.id())
            };
            if let Some(next) = next {
                self.private_start(a, next, now);
            }
        }
    }

    /// Error bits, heartbeats, in-flight timeouts, planned switches, builds (README §4 steps 4–6).
    fn maintain(&mut self, now: u64) -> bool {
        let mut did = false;
        // In-flight timeouts.
        let to = self.t.ms(u64::from(self.cfg.inflight_timeout_ms));
        let mut i = 0;
        while i < self.inflight.len() {
            let Some(f) = self.inflight.get(i).copied() else {
                break;
            };
            if now.saturating_sub(f.sent) > to {
                self.inflight.swap_remove(i);
                let account = self
                    .accts
                    .iter_mut()
                    .find(|x| x.metas.iter().any(|m| m.id == f.conn))
                    .map_or(AccountId(0), |x| x.account);
                self.conn_state(
                    account,
                    ConnRole::Order,
                    f.conn,
                    ConnStateKind::Degraded,
                    codes::INFLIGHT_TIMEOUT,
                    now,
                );
                did = true;
            } else {
                i += 1;
            }
        }
        for a in 0..self.accts.len() {
            let ids: [Option<(ConnId, ConnRole)>; 8] = {
                let mut v = [None; 8];
                if let Some(x) = self.accts.get(a) {
                    for (k, m) in x.metas.iter().take(8).enumerate() {
                        if let Some(s) = v.get_mut(k) {
                            *s = Some((m.id, m.role));
                        }
                    }
                }
                v
            };
            for (id, role) in ids.into_iter().flatten() {
                let (mut bits, send) = {
                    let Some(x) = self.accts.get_mut(a) else { continue };
                    let Some(m) = x.metas.iter_mut().find(|m| m.id == id) else {
                        continue;
                    };
                    let t = m.hb.check(now, m.last_data);
                    let send = m.hb.due(now, m.last_data);
                    (t, send)
                };
                let now_ms = self.t.utc_ns(now) / 1_000_000;
                let Some(c) = self.conn_of(a, id) else { continue };
                if !bits.is_empty() {
                    c.mark(bits);
                }
                bits = c.err();
                match send {
                    HbSend::None => {}
                    HbSend::WsPing => {
                        let _ = c.send_ping(now);
                    }
                    HbSend::App => {
                        let mut buf = [0u8; 256];
                        let n = P::app_ping(role, now_ms, &mut buf);
                        match buf.get(..n) {
                            Some(b) if n > 0 => {
                                let _ = c.send_text(b, now);
                            }
                            _ => {
                                let _ = c.send_ping(now);
                            }
                        }
                    }
                }
                if bits.intersects(ErrBits::FATAL | ErrBits::PING_TIMEOUT | ErrBits::IDLE_TIMEOUT) {
                    self.retire_conn(a, id, bits, now);
                    did = true;
                    continue;
                }
                if bits.intersects(ErrBits::SERVER_HINT | ErrBits::ROTATE_DUE)
                    && let Some(x) = self.accts.get_mut(a)
                {
                    x.rotate = true;
                    x.rotate_bits |= (bits & (ErrBits::SERVER_HINT | ErrBits::ROTATE_DUE)).bits();
                }
                if let Some(c) = self.conn_of(a, id) {
                    if role != ConnRole::UserStream {
                        c.clear_err();
                    }
                    c.finish_poll(now);
                }
            }
            if let Some(c) = self.accts[a].private.candidate
                && now.saturating_sub(c.since) >= self.t.ms(5000)
            {
                self.private_abort(a, c.id, now);
                did = true;
            }
            if self.accts[a].private.owner.is_none()
                && self.accts[a].private.candidate.is_none()
                && now >= self.accts[a].private.retry_at
            {
                let id = if P::USER_STREAM {
                    self.accts[a]
                        .user
                        .as_mut()
                        .and_then(|u| u.primary().map(|c| c.id()))
                } else {
                    self.accts[a].pool.primary().map(|c| c.id())
                };
                if let Some(id) = id {
                    self.private_start(a, id, now);
                    did = true;
                }
            }
            did |= self.switch(a, now);
        }
        did |= self.builds(now);
        did |= self.build_http(now);
        did
    }

    /// Planned switch (README §4 step 5): standby open and nothing in flight on the primary.
    fn switch(&mut self, a: usize, now: u64) -> bool {
        let wait = self.t.ms(u64::from(self.cfg.rotate_wait_max_ms));
        let one_s = self.t.ms(1000);
        let mut did = false;
        if let Some((old, at)) = self.accts.get(a).and_then(|x| x.retire_at)
            && now >= at
        {
            if let Some(x) = self.accts.get_mut(a) {
                x.retire_at = None;
            }
            self.retire_conn(a, old, ErrBits::ROTATE_DUE, now);
            did = true;
        }
        if let Some(x) = self.accts.get_mut(a)
            && x.pool.rotate_due(now).is_some()
        {
            x.rotate = true;
            x.rotate_bits |= ErrBits::ROTATE_DUE.bits();
        }
        let Some(x) = self.accts.get_mut(a) else {
            return did;
        };
        if !x.rotate || x.retire_at.is_some() || now < x.private.retry_at {
            return did;
        }
        let since = *x.rotate_since.get_or_insert(now);
        if now < since {
            return did;
        }
        let pid = x.pool.primary().map(|c| c.id());
        let standby_open = x
            .pool
            .standby()
            .is_some_and(|c| c.phase() == hft_net::ConnPhase::Open);
        let busy = pid.is_some_and(|p| self.inflight.iter().any(|f| f.conn == p));
        if !standby_open || busy {
            if now.saturating_sub(since) > wait {
                // Give up this time; wait again.
                x.rotate_since = Some(now.saturating_add(wait));
            }
            return did;
        }
        let sid = x.pool.standby().map(|c| c.id());
        if !P::USER_STREAM
            && let Some(sid) = sid
            && x.private.owner != Some(sid)
        {
            if x.private.candidate.is_none() {
                self.private_start(a, sid, now);
            }
            return true;
        }
        let x = &mut self.accts[a];
        let Ok((newp, old)) = x.pool.swap_primary() else {
            return did;
        };
        let bits = x.rotate_bits;
        x.rotate = false;
        x.rotate_bits = 0;
        x.rotate_since = None;
        x.retire_at = Some((old, now.saturating_add(one_s)));
        let account = x.account;
        self.conn_state(account, ConnRole::Order, newp, ConnStateKind::Rotated, bits, now);
        true
    }

    /// At most one build in flight per gw (stagger).
    fn builds(&mut self, now: u64) -> bool {
        if self.building.iter().any(Option::is_some) {
            return false;
        }
        for a in 0..self.accts.len() {
            for role in [ConnRole::Order, ConnRole::UserStream] {
                let want = {
                    let Some(x) = self.accts.get_mut(a) else { continue };
                    if role == ConnRole::UserStream && now < x.private.retry_at {
                        continue;
                    }
                    match role {
                        ConnRole::UserStream => x.user.as_mut().and_then(|u| u.next_build(now)),
                        _ => x.pool.next_build(now),
                    }
                };
                if want.is_none() {
                    continue;
                }
                let src = self.accts.get(a).map_or(0, |x| x.src_ip_idx);
                let ok = match self.ids.alloc() {
                    Some(id) => {
                        let endpoint = if role == ConnRole::UserStream {
                            self.cfg.user_endpoint
                        } else {
                            self.cfg.order_endpoint
                        };
                        let req = BuildReq {
                            venue: P::VENUE,
                            conn: id,
                            role,
                            endpoint: EndpointId(endpoint.0),
                            src_ip_idx: src,
                            rx_queue: 0,
                            deadline_ns: i64::MAX,
                        };
                        if self.build.try_push(req).is_ok() {
                            if let Some(s) = self.building.get_mut(id.0 as usize) {
                                *s = Some((a, role, 0));
                            }
                            true
                        } else {
                            self.ids.free(id);
                            false
                        }
                    }
                    None => false,
                };
                if !ok && let Some(x) = self.accts.get_mut(a) {
                    match role {
                        ConnRole::UserStream => {
                            if let Some(u) = x.user.as_mut() {
                                u.build_failed();
                            }
                        }
                        _ => x.pool.build_failed(),
                    }
                }
                return ok;
            }
        }
        false
    }

    fn deadline(&self) -> Option<u64> {
        let to = self.t.ms(u64::from(self.cfg.inflight_timeout_ms));
        let mut d = None;
        for f in self.inflight.iter() {
            d = min_deadline(d, f.sent.saturating_add(to).saturating_add(1));
        }
        for x in self.accts.iter() {
            if let Some(c) = x.private.candidate {
                d = min_deadline(d, c.since.saturating_add(self.t.ms(5000)));
            }
            if let Some(h) = x.hold.front() {
                d = min_deadline(
                    d,
                    h.since
                        .saturating_add(self.t.ms(u64::from(self.cfg.unmatched_hold_ms)))
                        .saturating_add(1),
                );
            }
            for m in x.metas.iter() {
                if let Some(t) = m.hb.next_deadline(m.last_data) {
                    d = min_deadline(d, t);
                }
            }
            if let Some((_, at)) = x.retire_at {
                d = min_deadline(d, at);
            }
        }
        d
    }

    // ---------------------------------------------------------------- views (tests, telemetry)

    pub fn counters(&self) -> GwCounters {
        self.n
    }

    #[doc(hidden)]
    pub fn held(&self, account: AccountId) -> usize {
        self.accts
            .iter()
            .find(|x| x.account == account)
            .map_or(0, |x| x.hold.len())
    }

    pub fn raw_counters(&self) -> RawCounters {
        self.raw.counters()
    }

    /// The installed connection `id` of any account.
    #[doc(hidden)]
    pub fn conn(&mut self, id: ConnId) -> Option<&mut Conn> {
        for x in self.accts.iter_mut() {
            if x.metas
                .iter()
                .any(|m| m.id == id && m.role == ConnRole::UserStream)
            {
                return x.user.as_mut()?.get(id);
            }
            if x.metas.iter().any(|m| m.id == id) {
                return x.pool.get(id);
            }
            if let Some(h) = &mut x.http {
                for (_, p) in &mut h.endpoints {
                    if let Some(c) = p.get(id) {
                        return Some(c);
                    }
                }
            }
        }
        None
    }

    /// Primary order connection of `account`.
    pub fn primary(&mut self, account: AccountId) -> Option<ConnId> {
        let a = self.acct_ix(account)?;
        self.accts.get_mut(a)?.pool.primary().map(|c| c.id())
    }

    pub fn standby(&mut self, account: AccountId) -> Option<ConnId> {
        let a = self.acct_ix(account)?;
        self.accts.get_mut(a)?.pool.standby().map(|c| c.id())
    }

    /// Requests in flight on `conn`.
    pub fn inflight(&self, conn: ConnId) -> usize {
        self.inflight.iter().filter(|f| f.conn == conn).count()
    }

    pub fn mirror(&self, slot: u32) -> Option<SlotMirror> {
        self.mirror.get(slot as usize).copied()
    }

    /// The `IdTable` of `account`.
    pub fn id_table(&self, account: AccountId) -> Option<&IdTable> {
        Some(&self.accts.get(self.acct_ix(account)?)?.ids)
    }

    /// The protocol context of `account` (tests).
    #[doc(hidden)]
    pub fn ctx(&mut self, account: AccountId) -> Option<&mut P::GwCtx> {
        let a = self.acct_ix(account)?;
        Some(&mut self.accts.get_mut(a)?.ctx)
    }

    /// Current epoch, `accept_prev`, `prev_epoch`.
    pub fn epoch(&self) -> (u16, bool, u16) {
        (self.epoch, self.accept_prev, self.prev_epoch)
    }

    pub fn last_stat(&self) -> LoopStat {
        self.last
    }

    /// Retires every connection (shutdown: hft-core drain); returns how many.
    pub fn retire_all(&mut self, now: u64) -> usize {
        let mut n = 0;
        for a in 0..self.accts.len() {
            let ids: Vec<ConnId> = self
                .accts
                .get(a)
                .map(|x| x.metas.iter().map(|m| m.id).collect())
                .unwrap_or_default();
            for id in ids {
                self.retire_conn(a, id, ErrBits::empty(), now);
                n += 1;
            }
        }
        for a in &mut self.accts {
            if let Some(h) = &mut a.http {
                for (_, p) in &mut h.endpoints {
                    p.retire_all();
                    while let Some(m) = p.take_retired() {
                        n += 1;
                        if self.retire.try_push(m).is_err() {
                            self.retire_pend.push_back(m);
                        }
                    }
                }
            }
        }
        while let Some(m) = self.retire_pend.front().copied() {
            if self.retire.try_push(m).is_err() {
                break;
            }
            self.retire_pend.pop_front();
        }
        n
    }

    /// Shutdown after `retire_all`: seal the raw journal before the persister stops.
    pub fn seal_and_close(self) -> Result<(), hft_journal::RollErr> {
        self.raw.seal_and_close()
    }

    /// Starts (`true`) or stops recording the effect trace (tests).
    #[doc(hidden)]
    pub fn trace(&mut self, on: bool) {
        self.raw.trace = on.then(Vec::new);
    }

    /// Takes the effect trace.
    #[doc(hidden)]
    pub fn take_trace(&mut self) -> Vec<Effect> {
        self.raw.trace.as_mut().map(std::mem::take).unwrap_or_default()
    }
}

fn decode_one<P: VenueProtocol>(
    rd: &RefData,
    ctx: &mut P::GwCtx,
    f: &Frame<'_>,
    id: ConnId,
    account: AccountId,
    sink: &mut GwSink<'_>,
) -> Result<ReportOutcome, DecodeErr> {
    let op = if f.opcode == Opcode::Binary {
        MsgOp::Binary
    } else {
        MsgOp::Text
    };
    let head = f.payload.get(..f.payload.len().min(64)).unwrap_or(f.payload);
    let c = P::classify_report(op, head);
    let ts = f.ts;
    let rx = RxMeta {
        rx_tsc: ts.rx_tsc,
        hw_rx_ns: ts.best_rx().0,
        sw_rx_ns: ts.sw_rx_ns,
        prev_override: 0,
        conn: id,
        account,
        _pad: [0; 2],
        ts_flags: ts.flags,
        rx_empty_gap_ns: ts.rx_empty_gap_ns,
        _pad2: 0,
    };
    P::decode_report(rd, ctx, c, f.payload, &rx, sink)
}

fn blank_event(venue: hft_types::VenueId, account: AccountId) -> OrderEvent {
    OrderEvent {
        venue,
        account,
        what: OrderEventKind::Ack,
        detail: 0,
        slot: SlotId(0),
        r#gen: 0,
        inst: InstId(0),
        px: Px(0),
        qty: Qty(0),
        cum_filled: Qty(0),
        fee: Qty(0),
        fee_asset: hft_types::AssetId(0),
        epoch: 0,
        reject_code: 0,
        exch_order_id: 0,
        ex_ts_ns: 0,
        hw_rx_ns: 0,
        rx_tsc: 0,
        avg_px: Px(0),
        cash_qty: hft_types::Qty(0),
        cash_sum: hft_types::Qty(0),
        trade_id: 0,
    }
}

/// Stable reorder of one batch (README §4 step 1): risk-reducing `Place`/`Amend` first, except an
/// `Amend` whose target is the slot of an earlier `Place` in the same batch.
fn reorder(b: &mut [GwCmd]) {
    let n = b.len();
    if n < 2 {
        return;
    }
    let mut front = [false; 256];
    let rr = hft_event::OrderFlags::RISK_REDUCING.0;
    for i in 0..n.min(256) {
        let Some(c) = b.get(i) else { break };
        let is_rr = c.oflags & rr != 0 && matches!(c.op, GwOp::Place | GwOp::Amend);
        let pinned = c.op == GwOp::Amend
            && b.get(..i)
                .is_some_and(|p| p.iter().any(|q| q.op == GwOp::Place && q.slot == c.target_slot));
        if let Some(f) = front.get_mut(i) {
            *f = is_rr && !pinned;
        }
    }
    // Stable partition without allocating: rotate each front element into place.
    let mut w = 0usize;
    for i in 0..n.min(256) {
        if front.get(i).copied().unwrap_or(false) {
            if let Some(s) = b.get_mut(w..=i) {
                s.rotate_right(1);
            }
            w += 1;
        }
    }
}

/// Decimal text of `v` (MirrorRestore keys of numeric exchange ids).
fn itoa(mut v: u64, buf: &mut [u8; 20]) -> &[u8] {
    let mut i = buf.len();
    loop {
        i -= 1;
        if let Some(d) = buf.get_mut(i) {
            *d = b'0' + (v % 10) as u8;
        }
        v /= 10;
        if v == 0 || i == 0 {
            break;
        }
    }
    buf.get(i..).unwrap_or(&[])
}

impl<P: VenueProtocol, const CAP: usize> PollTask for GwRuntime<P, CAP> {
    fn poll_once(&mut self, now: Tsc) -> Polled {
        Polled {
            did_work: GwRuntime::poll_once(self, now.0).did_work,
        }
    }

    fn next_deadline(&self) -> Option<Tsc> {
        self.last.next_deadline.map(Tsc)
    }

    fn wait_fds(&self, out: &mut WaitFds) {
        for x in self.accts.iter() {
            if let Some(h) = &x.http {
                for (_, p) in &h.endpoints {
                    p.wait_fds(out);
                }
            }
            for m in x.metas.iter() {
                if m.fd >= 0 {
                    out.push(m.fd);
                }
            }
        }
    }

    /// Commands, acct records or hand-overs waiting, or unprocessed bytes in a connection while the
    /// rep ring has more room than the command reserve (README §1 "环满").
    fn has_pending(&self) -> bool {
        if self.cmds.len_hint() > 0 || self.handoff.len_hint() > 0 || self.acct.has_data() {
            return true;
        }
        let room = self.out.free_hint() > usize::from(self.cfg.cmds_per_loop);
        room && self.accts.iter().any(|x| x.metas.iter().any(|m| m.buffered))
    }
}
