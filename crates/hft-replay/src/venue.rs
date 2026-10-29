//! The simulated venue of the replayed accounts (README "模拟场所"): `hft_sim::SimExchange` fed
//! from the journal, the order commands of the masked accounts, `Deliver` settlement as the live gw
//! reports it, and the deterministic reseeding of every account's latency sub-stream.

use hft_event::{
    EventKind, ExternalCause, ExternalFill, GwCmd, GwOp, Level, RecordHeader, RepMsg, SendReport,
};
use hft_refdata::{AccountKind, RefData, SimAccountCfg, SimBalance};
use hft_sim::glue::{Fed, feed_record, reply_parts, written};
use hft_sim::{
    Cond, LatencyModel, MakerFill, MarginModel, MmrSource, SimExchange, SimSetup, SimTuning, VenueModel,
};
use hft_types::{AccountId, AssetId, InstId, Px, Qty, Rate, Side, Tsc, VenueId};

/// Simulation parameters of one run (README "命令行" `--sim-*`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimParams {
    pub latency: LatencyModel,
    pub maker_fill: MakerFill,
    /// Base seed; the sub-stream of an account in a reseed interval is
    /// `splitmix(seed ⊕ interval ⊕ account)` (README "确定性").
    pub seed: u64,
    /// Reseed interval in UTC seconds (0 = never: one stream for the whole run).
    pub reseed_s: u32,
    pub consume_decay_ms: u32,
    pub take_avail_bp: u16,
    /// hft-sim venue-behaviour options (README "模拟场所"; default = none).
    pub venue: VenueModel,
    /// Thresholds of the latency condition (used when `venue.pools` is set).
    pub cond: CondCfg,
    /// `--own`: hide the journal's own resting orders of the masked accounts from the simulated
    /// book (and their mirror on the complementary token).
    pub own: bool,
    /// SHA-256 of the calibration table file the options came from (empty = none).
    pub calib_file_sha: String,
}

/// Latency-condition buckets (README "条件延迟"): venue load of the previous UTC minute
/// (the minute median of per-second md-lag p90 of the masked accounts' venues, against
/// `load_thr_ns`), the reference mid move over the last 1 s (≤ 0.25 / ≤ 1 / > 1 bp) and the
/// window phase (within the first `phase_s` of each `window_s`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CondCfg {
    pub load_thr_ns: Vec<i64>,
    pub window_s: u32,
    pub phase_s: u32,
}

impl Default for CondCfg {
    fn default() -> Self {
        CondCfg {
            load_thr_ns: Vec::new(),
            window_s: 300,
            phase_s: 5,
        }
    }
}

impl Default for SimParams {
    fn default() -> Self {
        SimParams {
            latency: LatencyModel::Fixed {
                entry_us: 40_000,
                resp_us: 0,
            },
            maker_fill: MakerFill::Queue,
            seed: 1,
            reseed_s: 300,
            consume_decay_ms: 1_000,
            take_avail_bp: 10_000,
            venue: VenueModel::default(),
            cond: CondCfg::default(),
            own: false,
            calib_file_sha: String::new(),
        }
    }
}

/// The condition tracker of `CondCfg`.
#[derive(Debug, Default)]
struct CondTrack {
    /// Venues of the masked accounts (bit per `VenueId`).
    venues: u64,
    sec: i64,
    lags: Vec<i64>,
    minute: i64,
    p90s: Vec<i64>,
    /// Load bucket of the previous complete minute (1 until one is known).
    load: u8,
    ref_inst: Option<InstId>,
    /// `(record tsc, bid + ask)` of the reference over the last 2 s.
    ref_hist: std::collections::VecDeque<(u64, i128)>,
}

/// One logged own order of a masked account (`--own`).
#[derive(Clone, Copy, Debug)]
struct OwnOrder {
    inst: InstId,
    side: Side,
    px: Px,
    qty: i64,
    cum: i64,
    hidden: i64,
}

/// A simulated report ready to be handed as an Input.
#[derive(Clone, Debug)]
pub struct SimInput {
    pub kind: EventKind,
    pub due: Tsc,
    pub body: Vec<u8>,
}

/// The simulated venue.
pub struct ResearchVenue {
    ex: SimExchange,
    rd: RefData,
    /// Accounts whose logged reports are replaced by the simulation.
    mask: Vec<AccountId>,
    levels: Vec<Level>,
    params: SimParams,
    interval: i64,
    /// Inputs produced but not yet handed (SendReport Written, deliveries), in order.
    immediate: Vec<SimInput>,
    pub fed: u64,
    hz: u64,
    track: CondTrack,
    /// `(account, slot, gen)` → logged own order (`--own`).
    own: std::collections::BTreeMap<(u8, u32, u32), OwnOrder>,
    /// Engine UTC of the Input being processed (`on_utc`).
    utc_ns: i64,
}

impl core::fmt::Debug for ResearchVenue {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ResearchVenue")
            .field("mask", &self.mask)
            .field("params", &self.params)
            .finish_non_exhaustive()
    }
}

/// `splitmix64` (the sub-stream mixer of hft-sim).
pub fn splitmix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Report kinds of a masked account that the replay drops from the journal (their simulated
/// counterparts take their place, README "账户屏蔽").
pub fn masked_kind(kind: EventKind) -> bool {
    matches!(
        kind,
        EventKind::ORDER_EVENT
            | EventKind::SEND_REPORT
            | EventKind::EXTERNAL_FILL
            | EventKind::BALANCE_REPORT
            | EventKind::POSITION_REPORT
            | EventKind::MARGIN_REPORT
            | EventKind::FUNDING_SETTLE
    )
}

/// The account of a report body (`None` for other kinds).
pub fn report_account(kind: EventKind, body: &[u8]) -> Option<AccountId> {
    body.get(account_offset(kind)?).map(|a| AccountId(*a))
}

/// Offset of the `account` field of a report body (hft-event layouts).
fn account_offset(kind: EventKind) -> Option<usize> {
    use core::mem::offset_of;
    Some(match kind {
        EventKind::ORDER_EVENT => offset_of!(hft_event::OrderEvent, account),
        EventKind::EXTERNAL_FILL => offset_of!(hft_event::ExternalFill, account),
        EventKind::SEND_REPORT => offset_of!(hft_event::SendReport, account),
        EventKind::BALANCE_REPORT => offset_of!(hft_event::BalanceReport, account),
        EventKind::POSITION_REPORT => offset_of!(hft_event::PositionReport, account),
        EventKind::MARGIN_REPORT => offset_of!(hft_event::MarginReportHdr, account),
        EventKind::FUNDING_SETTLE => offset_of!(hft_event::FundingSettle, account),
        _ => return None,
    })
}

/// A large starting balance of every quote asset on the account's venues: the engine's own ledger
/// and risk checks govern; the simulated cash only has to never be the binding constraint.
fn balances(rd: &RefData, a: AccountId) -> Box<[SimBalance]> {
    let venues = rd.account(a).venues;
    let mut out: Vec<SimBalance> = Vec::new();
    for v in 1..64u8 {
        if venues & (1u64 << v) == 0 {
            continue;
        }
        for i in rd.insts_of_venue(VenueId(v)) {
            let asset: AssetId = rd.inst(*i).quote;
            if asset.0 == 0 || out.iter().any(|b| b.venue.0 == v && b.asset == asset) {
                continue;
            }
            let exp = rd.asset(asset).exp.qty_exp;
            let unit = 10i64.saturating_pow(u32::from(exp.unsigned_abs()));
            out.push(SimBalance {
                venue: VenueId(v),
                asset,
                qty: Qty(1_000_000_000i64.saturating_mul(unit)),
            });
        }
    }
    out.into()
}

impl ResearchVenue {
    /// A simulated exchange for `mask` (Live or Sim accounts of `rd`), clocked at `tsc_hz_milli`.
    pub fn new(rd: &RefData, mask: &[AccountId], tsc_hz_milli: u64, p: &SimParams) -> Result<Self, String> {
        let mut accounts = Vec::new();
        for a in mask {
            let m = rd.account(*a);
            let cfg = match &m.kind {
                AccountKind::Sim(c) => SimAccountCfg {
                    initial_balances: balances(rd, *a),
                    latency_model: 0,
                    fee_tier: c.fee_tier,
                    margin_model: 0,
                },
                AccountKind::Live => SimAccountCfg {
                    initial_balances: balances(rd, *a),
                    latency_model: 0,
                    fee_tier: 0,
                    margin_model: 0,
                },
            };
            accounts.push((*a, cfg));
        }
        let setup = SimSetup {
            tuning: SimTuning {
                maker_fill: p.maker_fill,
                consume_decay_ms: p.consume_decay_ms,
                take_avail_bp: p.take_avail_bp,
                rate_limit: true,
                ..SimTuning::with_seed(p.seed)
            },
            tsc_hz_milli,
            latency_models: vec![p.latency],
            margin_models: vec![MarginModel {
                mmr: MmrSource::Flat(Rate(5_000_000)),
                leverage: 1,
                liq_fee: Rate(0),
                liq_slippage_bp: 0,
            }],
            accounts,
        };
        let mut ex = SimExchange::new(&setup, rd).map_err(|e| format!("simulated venue: {e:?}"))?;
        ex.set_venue_model(p.venue.clone());
        let venues = mask.iter().fold(0u64, |v, a| v | rd.account(*a).venues);
        // Start-up replies (connection facts, account config) are not part of the replayed
        // world: the journal already carries the venue's own.
        ex.drain_due(Tsc(0), |_| {});
        Ok(ResearchVenue {
            ex,
            rd: rd.clone(),
            mask: mask.to_vec(),
            levels: Vec::with_capacity(8192),
            params: p.clone(),
            interval: -1,
            immediate: Vec::new(),
            fed: 0,
            hz: tsc_hz_milli,
            track: CondTrack {
                venues,
                load: 1,
                ..CondTrack::default()
            },
            own: Default::default(),
            utc_ns: 0,
        })
    }

    pub fn masked(&self, a: AccountId) -> bool {
        self.mask.contains(&a)
    }

    pub fn mask(&self) -> &[AccountId] {
        &self.mask
    }

    /// The engine UTC of the Input being processed: reseeds every account at the first command of
    /// each UTC interval of `reseed_s` seconds; the window phase of the latency condition.
    pub fn on_utc(&mut self, utc_ns: i64) {
        self.utc_ns = utc_ns;
        if self.params.reseed_s == 0 {
            return;
        }
        let k = utc_ns.div_euclid(i64::from(self.params.reseed_s) * 1_000_000_000);
        if k == self.interval {
            return;
        }
        self.interval = k;
        for a in &self.mask {
            let s = splitmix64(self.params.seed ^ splitmix64(k as u64) ^ u64::from(a.0));
            self.ex.reseed_account(*a, s);
        }
    }

    /// A journal record in journal order (market data reaches the replica, anything else only
    /// advances time).
    pub fn on_record(&mut self, r: &hft_sequencer::MemRecord) {
        let hdr = RecordHeader {
            len: r.body.len() as u32,
            kind: r.kind.0,
            flags: r.flags.0,
            seq: r.seq,
            tsc: r.tsc,
        };
        if feed_record(&mut self.ex, &hdr, r.kind, &r.body, &mut self.levels) == Fed::Market {
            self.fed += 1;
            if self.params.venue.pools.is_some() {
                self.track_market(r);
            }
        }
    }

    /// Condition inputs of one market record: md lag of the masked venues, the reference mid.
    fn track_market(&mut self, r: &hft_sequencer::MemRecord) {
        let Some(h) = hft_sequencer::types::read_body_plain::<hft_event::MarketHdr>(&r.body) else {
            return;
        };
        let t = &mut self.track;
        let on_venue = t.venues & (1u64 << (h.venue.0 & 63)) != 0;
        if on_venue
            && matches!(
                r.kind,
                EventKind::L2_BATCH | EventKind::L2_SNAPSHOT | EventKind::TRADE
            )
        {
            if h.sw_rx_ns > 0 && h.ex_ts_ns > 0 {
                let sec = h.sw_rx_ns.div_euclid(1_000_000_000);
                if sec != t.sec {
                    if !t.lags.is_empty() {
                        t.lags.sort_unstable();
                        let k = (t.lags.len() * 9 / 10).min(t.lags.len() - 1);
                        t.p90s.push(t.lags[k]);
                        t.lags.clear();
                    }
                    let minute = sec.div_euclid(60);
                    if minute != t.minute {
                        if !t.p90s.is_empty() {
                            t.p90s.sort_unstable();
                            let med = t.p90s[t.p90s.len() / 2];
                            t.load = self.params.cond.load_thr_ns.iter().filter(|x| med > **x).count() as u8;
                            t.p90s.clear();
                        }
                        t.minute = minute;
                    }
                    t.sec = sec;
                }
                t.lags.push(h.sw_rx_ns - h.ex_ts_ns);
            }
        } else if !on_venue && r.kind == EventKind::BBO {
            let Some(b) = hft_sequencer::types::read_body_plain::<hft_event::Bbo>(&r.body) else {
                return;
            };
            if t.ref_inst.is_none() {
                t.ref_inst = Some(b.h.inst);
            }
            if t.ref_inst == Some(b.h.inst) && b.bid_px.0 > 0 && b.ask_px.0 > 0 {
                t.ref_hist
                    .push_back((r.tsc, i128::from(b.bid_px.0) + i128::from(b.ask_px.0)));
                let keep = self.hz.saturating_mul(2) / 1_000;
                while t
                    .ref_hist
                    .front()
                    .is_some_and(|x| r.tsc.saturating_sub(x.0) > keep)
                {
                    t.ref_hist.pop_front();
                }
            }
        }
    }

    /// The latency condition at decision time `t` (README "条件延迟").
    fn condition(&self, t: Tsc) -> Cond {
        let tr = &self.track;
        let one_s = self.hz / 1_000;
        let mv = match (
            tr.ref_hist.iter().find(|x| t.0.saturating_sub(x.0) <= one_s),
            tr.ref_hist.back(),
        ) {
            (Some(a), Some(b)) if a.1 > 0 => {
                let d = (b.1 - a.1).abs();
                if d * 40_000 <= a.1 {
                    0
                } else if d * 10_000 <= a.1 {
                    1
                } else {
                    2
                }
            }
            _ => 0,
        };
        let c = &self.params.cond;
        let s = self.utc_ns.div_euclid(1_000_000_000);
        let phase = u8::from(s.rem_euclid(i64::from(c.window_s.max(1))) >= i64::from(c.phase_s));
        Cond {
            load: tr.load,
            mv,
            phase,
        }
    }

    /// `--own`: a logged record of a masked account (its own `OrderDecision` Outputs and the
    /// dropped `OrderEvent`s) keeps the hidden own volume of the simulated book in step.
    pub fn on_logged(&mut self, r: &hft_sequencer::MemRecord) {
        if !self.params.own {
            return;
        }
        match r.kind {
            EventKind::ORDER_DECISION => {
                let Some(d) = hft_sequencer::read_body::<hft_event::OrderDecision>(&r.body) else {
                    return;
                };
                if d.reject == 0 && d.action == hft_event::OrderAction::New && self.masked(d.account) {
                    self.own.insert(
                        (d.account.0, d.slot.0, d.r#gen),
                        OwnOrder {
                            inst: d.inst,
                            side: d.side,
                            px: d.px,
                            qty: d.qty.0,
                            cum: 0,
                            hidden: 0,
                        },
                    );
                }
            }
            EventKind::ORDER_EVENT => {
                use hft_event::OrderEventKind as K;
                let Some(e) = hft_sequencer::read_body::<hft_event::OrderEvent>(&r.body) else {
                    return;
                };
                let key = (e.account.0, e.slot.0, e.r#gen);
                let Some(mut o) = self.own.get(&key).copied() else {
                    return;
                };
                let terminal = matches!(e.what, K::Fill | K::CancelAck | K::Reject | K::Expired);
                if matches!(e.what, K::PartialFill | K::Fill) {
                    o.cum = o.cum.max(e.cum_filled.0);
                }
                let rest = if terminal {
                    0
                } else if e.what == K::Ack || o.hidden > 0 {
                    (o.qty - o.cum).max(0)
                } else {
                    o.hidden
                };
                if rest != o.hidden {
                    self.hide(o.inst, o.side, o.px, rest - o.hidden);
                    o.hidden = rest;
                }
                if terminal {
                    self.own.remove(&key);
                } else {
                    self.own.insert(key, o);
                }
            }
            _ => {}
        }
    }

    /// Hides `dq` at `(inst, side, px)` and its mirror on the complementary token.
    fn hide(&mut self, inst: InstId, side: Side, px: Px, dq: i64) {
        self.ex.hide_own(inst, side, px, Qty(dq));
        if let Some((c, one)) = self.ex.complement(inst) {
            let other = if side == Side::Buy { Side::Sell } else { Side::Buy };
            self.ex.hide_own(c, other, Px(one.0 - px.0), Qty(dq));
        }
    }

    /// An out-ring command of the engine at decision time `decided`. Commands of accounts outside
    /// the mask are ignored (their logged reports stay in the replay).
    pub fn on_command(&mut self, cmd: &GwCmd, decided: Tsc) {
        if !self.masked(cmd.account) {
            return;
        }
        match cmd.op {
            GwOp::Place | GwOp::Cancel | GwOp::Amend | GwOp::Query | GwOp::CancelAll => {
                let venue = self.rd.inst(cmd.inst).venue;
                if let Some(w) = written(cmd, venue, decided) {
                    self.push_immediate(EventKind::SEND_REPORT, decided, &RepMsg::SendReport(w));
                }
                if self.params.venue.pools.is_some() {
                    let c = self.condition(decided);
                    self.ex.set_condition(c);
                }
                self.ex.on_command(cmd, decided);
            }
            GwOp::SimSettle | GwOp::SimReset | GwOp::SimTransfer => self.ex.on_command(cmd, decided),
            GwOp::Deliver => self.deliver(cmd, decided),
            _ => {}
        }
    }

    /// hft-venue gw `Deliver`: the inventory reported as `ExternalFill{Delivery}` at the payout
    /// price (no fee); the simulated position is closed the same way.
    fn deliver(&mut self, c: &GwCmd, t: Tsc) {
        if c.qty.0 <= 0 || c.px.0 < 0 {
            return;
        }
        let _ = self.ex.external_fill(c.account, c.inst, Side::Sell, c.qty, c.px);
        let f = ExternalFill {
            venue: self.rd.inst(c.inst).venue,
            account: c.account,
            side: Side::Sell,
            cause: ExternalCause::Delivery,
            inst: c.inst,
            qty: c.qty,
            px: c.px,
            fee: Qty(0),
            fee_asset: self.rd.inst(c.inst).quote,
            _pad: [0; 6],
            exch_order_id: u64::from(c.arg2),
            trade_id: 0,
            ex_ts_ns: i64::from(c.arg2).saturating_mul(1_000_000_000),
            rx_tsc: t,
            fill_seq: c.qty.0 as u64,
            cash_qty: Qty(0),
            cash_amount: Qty(0),
        };
        self.push_immediate(EventKind::EXTERNAL_FILL, t, &RepMsg::ExternalFill(f));
    }

    fn push_immediate(&mut self, kind: EventKind, t: Tsc, m: &RepMsg) {
        let body = m.with_parts(&[], |_, p| p.concat());
        self.immediate.push(SimInput { kind, due: t, body });
    }

    /// Reports produced at the command itself (Written, deliveries), oldest first.
    pub fn take_immediate(&mut self) -> Vec<SimInput> {
        core::mem::take(&mut self.immediate)
    }

    /// Executes every command that arrives before `t`.
    pub fn advance(&mut self, t: Tsc) {
        self.ex.advance(t);
    }

    /// The replies due at or before `t`, as Input records in `(due, gen)` order; only order
    /// evidence (OrderEvent, ExternalFill, SendReport, FundingSettle) is kept — account truth and
    /// connection facts of the replayed accounts come from the journal and the engine's ledger.
    pub fn due(&mut self, t: Tsc, epoch: u16, out: &mut Vec<SimInput>) {
        let rd = &self.rd;
        self.ex.drain_due(t, |r| {
            let keep = matches!(
                r.msg,
                RepMsg::OrderEvent(..)
                    | RepMsg::ExternalFill(_)
                    | RepMsg::SendReport(_)
                    | RepMsg::FundingSettle(_)
            );
            if !keep {
                return;
            }
            reply_parts(rd, epoch, &r.msg, r.bulk, |kind, parts| {
                out.push(SimInput {
                    kind,
                    due: r.due_tsc,
                    body: parts.concat(),
                });
            });
        });
    }

    pub fn next_due(&self) -> Option<Tsc> {
        self.ex.next_due()
    }

    pub fn state_hash(&self) -> u64 {
        self.ex.state_hash()
    }
}

/// `SendReport` of a masked account is replaced too; kept public for the parity report.
pub fn is_written(r: &SendReport) -> bool {
    r.status == hft_event::SendStatus::Written
}
