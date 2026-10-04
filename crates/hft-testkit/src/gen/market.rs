//! `market_stream`: legal core market-data sequences (`Bbo`, `Trade`, `Funding`, `MarkPrice`,
//! `L2Snapshot{origin = TopN}`) together with the BBO/market connection inputs the feed task
//! produces (`ConnState` Up/member Up/Alive and the silence-recovery sequence), in one global
//! virtual-time order.
//!
//! Model (README "2. proptest 生成器"): one BBO connection (slot 0) carries every inst's `Bbo`;
//! one market connection (`MAX_BBO_CONN_SLOTS`) carries trades, mark, funding and depth. At
//! t = 0 both connections come up and every inst joins the BBO connection. Each connection is
//! probed every `bbo_ping_ms`; a probe answer yields `ConnState{Alive}` when the connection
//! published nothing in the last `bbo_ping_ms / 2` (hft-venue §3a rule 3). The stream is built by
//! a deterministic interpreter over proptest-generated steps, so it shrinks with the steps.

use hft_event::{
    Bbo, Body, ConnRole, ConnState, ConnStateKind, EventKind, Funding, L2SnapHdr, Level, MarkPrice,
    MarketHdr, Trade,
};
use hft_types::{AccountId, ConnId, InstId, MAX_BBO_CONN_SLOTS, Px, Qty, Rate, Side, VenueId};
use proptest::prelude::*;
use zerocopy::IntoBytes;

/// The BBO connection's slot.
pub const BBO_CONN: u8 = 0;
/// The market (non-BBO) connection's id.
pub const MD_CONN: u8 = MAX_BBO_CONN_SLOTS;
/// `ConnState.reason` codes used here (owner: hft-venue's reason table).
pub const REASON_BBO_REFRESH: u32 = 6;
pub const REASON_BBO_SILENT: u32 = 7;

/// `Bbo.update_id` per inst (core meaning; sequence gaps of incremental depth are the optional
/// extension and not generated).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum GapPolicy {
    /// The venue has no sequence: `update_id = 0`.
    NoSeq,
    /// `1, 2, 3, …` per inst.
    Contiguous,
    /// Strictly increasing with forward jumps of `1..=max_skip + 1` (venues skip ids).
    Skips { max_skip: u32 },
}

/// Quiet behaviour; windows are `[from_ms, to_ms)` from the stream start, `0 < from_ms < to_ms`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum QuietPolicy {
    None,
    /// `inst`'s book does not change inside the window (no `Bbo` for it); the BBO connection
    /// keeps answering probes, so `Alive` follows the ordinary rule.
    Still {
        inst: InstId,
        from_ms: u64,
        to_ms: u64,
    },
    /// The BBO connection answers nothing inside the window: no event carries its id there. At
    /// `to_ms` the recovery sequence follows, stamped `to_ms`: `Degraded{BBO_SILENT}`,
    /// `Down{inst = 0}`, `Up{inst = 0}`, then per inst `Up{inst}` and `Resync{inst, BBO_REFRESH}`;
    /// only after it does a `Bbo` appear on that connection again.
    Silent {
        from_ms: u64,
        to_ms: u64,
    },
}

/// Every parameter of `market_stream_with`; `new` fills the non-signature fields with the values
/// documented on each field.
#[derive(Clone, Debug)]
pub struct MarketStreamCfg {
    pub venue: VenueId,
    /// 1 or more insts, none equal to `InstId(0)` (0 = whole connection in `ConnState`).
    pub insts: Vec<InstId>,
    /// Number of generated market steps (the output also carries connection events).
    pub n: usize,
    pub gap: GapPolicy,
    pub quiet: QuietPolicy,
    /// Probe period of both connections (`new`: 1000).
    pub bbo_ping_ms: u32,
    /// `rx_tsc = start_tsc + ⌊t_ns × tsc_hz / 10^9⌋` (`new`: 1 000 000 000 and 0).
    pub tsc_hz: u64,
    pub start_tsc: u64,
    /// UTC of t = 0 (`new`: 1 700 000 000 000 000 000).
    pub start_utc_ns: i64,
    /// Time between steps, inclusive range in ms (`new`: 0..=20).
    pub dt_ms: (u32, u32),
    /// Starting best bid, price tick and lot (raw; `new`: 100 000, 1, 1).
    pub start_bid: Px,
    pub tick: Px,
    pub lot: Qty,
    /// Starting funding settlement period (`new`: 28 800 s).
    pub funding_interval_s: u32,
}

impl MarketStreamCfg {
    pub fn new(venue: VenueId, insts: &[InstId], n: usize, gap: GapPolicy) -> Self {
        MarketStreamCfg {
            venue,
            insts: insts.to_vec(),
            n,
            gap,
            quiet: QuietPolicy::None,
            bbo_ping_ms: 1000,
            tsc_hz: 1_000_000_000,
            start_tsc: 0,
            start_utc_ns: 1_700_000_000_000_000_000,
            dt_ms: (0, 20),
            start_bid: Px(100_000),
            tick: Px(1),
            lot: Qty(1),
            funding_interval_s: 28_800,
        }
    }

    fn validate(&self) {
        assert!(!self.insts.is_empty(), "market_stream: no insts");
        assert!(
            self.insts.iter().all(|i| i.0 != 0),
            "market_stream: InstId(0) is reserved"
        );
        assert!(
            self.bbo_ping_ms > 0 && self.tsc_hz > 0,
            "market_stream: zero ping or tsc_hz"
        );
        assert!(self.dt_ms.0 <= self.dt_ms.1, "market_stream: empty dt range");
        assert!(
            self.tick.0 > 0 && self.lot.0 > 0 && self.start_bid.0 > 0,
            "market_stream: bad px/qty"
        );
        assert!(
            self.funding_interval_s > 0,
            "market_stream: zero funding interval"
        );
        match self.quiet {
            QuietPolicy::None => {}
            QuietPolicy::Still { inst, from_ms, to_ms } => {
                assert!(
                    self.insts.contains(&inst),
                    "market_stream: Still inst not in insts"
                );
                assert!(0 < from_ms && from_ms < to_ms, "market_stream: bad Still window");
            }
            QuietPolicy::Silent { from_ms, to_ms } => {
                assert!(0 < from_ms && from_ms < to_ms, "market_stream: bad Silent window");
            }
        }
    }
}

/// An owned `L2Snapshot` (one logical record; `bids` price-descending, `asks` ascending).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedL2Snapshot {
    pub hdr: L2SnapHdr,
    pub bids: Vec<Level>,
    pub asks: Vec<Level>,
}

/// One generated market input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MdEvent {
    Bbo(Bbo),
    Trade(Trade),
    Funding(Funding),
    Mark(MarkPrice),
    L2Snapshot(OwnedL2Snapshot),
    Conn(ConnState),
}

impl MdEvent {
    pub fn kind(&self) -> EventKind {
        match self {
            MdEvent::Bbo(_) => Bbo::KIND,
            MdEvent::Trade(_) => Trade::KIND,
            MdEvent::Funding(_) => Funding::KIND,
            MdEvent::Mark(_) => MarkPrice::KIND,
            MdEvent::L2Snapshot(_) => L2SnapHdr::KIND,
            MdEvent::Conn(_) => ConnState::KIND,
        }
    }

    fn hdr(&self) -> Option<&MarketHdr> {
        match self {
            MdEvent::Bbo(b) => Some(&b.h),
            MdEvent::Trade(b) => Some(&b.h),
            MdEvent::Funding(b) => Some(&b.h),
            MdEvent::Mark(b) => Some(&b.h),
            MdEvent::L2Snapshot(s) => Some(&s.hdr.h),
            MdEvent::Conn(_) => None,
        }
    }

    /// Connection id (`MarketHdr.conn` or `ConnState.conn`).
    pub fn conn(&self) -> u8 {
        match self {
            MdEvent::Conn(c) => c.conn,
            e => e.hdr().map_or(0, |h| h.conn.0),
        }
    }

    pub fn rx_tsc(&self) -> u64 {
        match self {
            MdEvent::Conn(c) => c.rx_tsc,
            e => e.hdr().map_or(0, |h| h.rx_tsc),
        }
    }

    /// `InstId(0)` for connection-level `ConnState`.
    pub fn inst(&self) -> InstId {
        match self {
            MdEvent::Conn(c) => c.inst,
            e => e.hdr().map_or(InstId(0), |h| h.inst),
        }
    }

    /// Appends `kind (u16 LE) ‖ body bytes` (snapshot: prefix then bid and ask levels).
    pub fn write_bytes(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.kind().0.to_le_bytes());
        match self {
            MdEvent::Bbo(b) => out.extend_from_slice(b.as_bytes()),
            MdEvent::Trade(b) => out.extend_from_slice(b.as_bytes()),
            MdEvent::Funding(b) => out.extend_from_slice(b.as_bytes()),
            MdEvent::Mark(b) => out.extend_from_slice(b.as_bytes()),
            MdEvent::L2Snapshot(s) => {
                out.extend_from_slice(s.hdr.as_bytes());
                out.extend_from_slice(s.bids.as_bytes());
                out.extend_from_slice(s.asks.as_bytes());
            }
            MdEvent::Conn(c) => out.extend_from_slice(c.as_bytes()),
        }
    }
}

/// One interpreter step: (dt_ms, inst index, kind roll 0..100, price move, spread ticks, qty a,
/// qty b, extra bits).
type Step = (u32, usize, u8, i8, u8, u16, u16, u32);

/// `market_stream(venue, insts, n, gap)` with `QuietPolicy::None` and `MarketStreamCfg::new`.
pub fn market_stream(
    venue: VenueId,
    insts: &[InstId],
    n: usize,
    gap: GapPolicy,
) -> impl Strategy<Value = Vec<MdEvent>> + use<> {
    market_stream_with(MarketStreamCfg::new(venue, insts, n, gap))
}

/// The generator with every parameter explicit.
pub fn market_stream_with(cfg: MarketStreamCfg) -> impl Strategy<Value = Vec<MdEvent>> + use<> {
    cfg.validate();
    let k = cfg.insts.len();
    let step = (
        cfg.dt_ms.0..=cfg.dt_ms.1,
        0..k,
        0u8..100,
        -3i8..=3,
        1u8..=5,
        1u16..=1000,
        1u16..=1000,
        any::<u32>(),
    );
    proptest::collection::vec(step, cfg.n..=cfg.n).prop_map(move |steps| build(&cfg, &steps))
}

#[derive(Clone, Debug)]
struct InstState {
    bid_ticks: i64,
    spread_ticks: i64,
    update_id: u64,
    trade_id: u64,
    snap_id: u64,
    interval_s: u32,
}

struct Builder<'a> {
    cfg: &'a MarketStreamCfg,
    out: Vec<MdEvent>,
    ping_ns: u64,
    next_probe_ns: u64,
    /// Last publish time per connection (0 = BBO, 1 = market).
    last_pub: [u64; 2],
    recovered: bool,
    insts: Vec<InstState>,
}

fn ms(v: u64) -> u64 {
    v.saturating_mul(1_000_000)
}

impl Builder<'_> {
    fn rx_tsc(&self, t_ns: u64) -> u64 {
        let ticks = t_ns as u128 * self.cfg.tsc_hz as u128 / 1_000_000_000;
        self.cfg
            .start_tsc
            .saturating_add(ticks.min(u64::MAX as u128) as u64)
    }

    fn utc(&self, t_ns: u64) -> i64 {
        self.cfg
            .start_utc_ns
            .saturating_add(t_ns.min(i64::MAX as u64) as i64)
    }

    fn silent_at(&self, t_ns: u64) -> bool {
        matches!(self.cfg.quiet, QuietPolicy::Silent { from_ms, to_ms } if ms(from_ms) <= t_ns && t_ns < ms(to_ms))
    }

    fn still_at(&self, inst: InstId, t_ns: u64) -> bool {
        matches!(self.cfg.quiet, QuietPolicy::Still { inst: q, from_ms, to_ms }
            if q == inst && ms(from_ms) <= t_ns && t_ns < ms(to_ms))
    }

    fn conn_state(&mut self, t_ns: u64, conn: u8, state: ConnStateKind, inst: InstId, reason: u32) {
        let ev = ConnState {
            venue: self.cfg.venue,
            conn,
            state,
            role: ConnRole::Md,
            inst,
            reason,
            account: AccountId(0),
            _pad2: [0; 3],
            rx_tsc: self.rx_tsc(t_ns),
        };
        self.out.push(MdEvent::Conn(ev));
        self.last_pub[usize::from(conn != BBO_CONN)] = t_ns;
    }

    fn hdr(&self, t_ns: u64, conn: u8, inst: InstId, lat_ns: u32) -> MarketHdr {
        let ex = self.utc(t_ns).saturating_sub(i64::from(lat_ns));
        MarketHdr {
            venue: self.cfg.venue,
            conn: ConnId(conn),
            _pad: 0,
            inst,
            ex_ts_ns: ex,
            ex_send_ns: ex,
            hw_rx_ns: 0,
            rx_tsc: self.rx_tsc(t_ns),
            sw_rx_ns: 0,
            rx_empty_gap_ns: 0,
            _rsv: 0,
        }
    }

    fn setup(&mut self) {
        self.conn_state(0, BBO_CONN, ConnStateKind::Up, InstId(0), 0);
        for i in 0..self.cfg.insts.len() {
            self.conn_state(0, BBO_CONN, ConnStateKind::Up, self.cfg.insts[i], 0);
        }
        self.conn_state(0, MD_CONN, ConnStateKind::Up, InstId(0), 0);
    }

    fn recover(&mut self, t_ns: u64) {
        self.conn_state(
            t_ns,
            BBO_CONN,
            ConnStateKind::Degraded,
            InstId(0),
            REASON_BBO_SILENT,
        );
        self.conn_state(t_ns, BBO_CONN, ConnStateKind::Down, InstId(0), 0);
        self.conn_state(t_ns, BBO_CONN, ConnStateKind::Up, InstId(0), 0);
        for i in 0..self.cfg.insts.len() {
            let inst = self.cfg.insts[i];
            self.conn_state(t_ns, BBO_CONN, ConnStateKind::Up, inst, 0);
            self.conn_state(t_ns, BBO_CONN, ConnStateKind::Resync, inst, REASON_BBO_REFRESH);
        }
        self.recovered = true;
    }

    fn probe(&mut self, p_ns: u64) {
        let half = self.ping_ns / 2;
        if !self.silent_at(p_ns) && p_ns - self.last_pub[0].min(p_ns) >= half {
            self.conn_state(p_ns, BBO_CONN, ConnStateKind::Alive, InstId(0), 0);
        }
        if p_ns - self.last_pub[1].min(p_ns) >= half {
            self.conn_state(p_ns, MD_CONN, ConnStateKind::Alive, InstId(0), 0);
        }
    }

    /// Emits every probe answer and the recovery sequence due at or before `t_ns`, in time order
    /// (recovery before a probe at the same instant).
    fn advance_to(&mut self, t_ns: u64) {
        loop {
            let end = match self.cfg.quiet {
                QuietPolicy::Silent { to_ms, .. } if !self.recovered => ms(to_ms),
                _ => u64::MAX,
            };
            let next = end.min(self.next_probe_ns);
            if next > t_ns {
                return;
            }
            if end <= self.next_probe_ns {
                self.recover(end);
            } else {
                let p = self.next_probe_ns;
                self.probe(p);
                self.next_probe_ns = p.saturating_add(self.ping_ns);
            }
        }
    }

    fn step(&mut self, t_ns: u64, s: &Step) {
        let &(_, ix, roll, mv, spread, qa, qb, extra) = s;
        let inst = self.cfg.insts[ix];
        let tick = self.cfg.tick.0;
        let lot = self.cfg.lot.0;
        let lat = extra % 2_000_000;
        match roll {
            0..50 => {
                if self.silent_at(t_ns) || self.still_at(inst, t_ns) {
                    return;
                }
                let st = &mut self.insts[ix];
                st.bid_ticks = (st.bid_ticks + i64::from(mv)).max(1);
                st.spread_ticks = i64::from(spread);
                st.update_id = match self.cfg.gap {
                    GapPolicy::NoSeq => 0,
                    GapPolicy::Contiguous => st.update_id + 1,
                    GapPolicy::Skips { max_skip } => {
                        st.update_id + 1 + u64::from(extra % max_skip.saturating_add(1))
                    }
                };
                let (bid, ask, uid) = (st.bid_ticks, st.bid_ticks + st.spread_ticks, st.update_id);
                let ev = Bbo {
                    h: self.hdr(t_ns, BBO_CONN, inst, lat),
                    bid_px: Px(bid * tick),
                    bid_qty: Qty(i64::from(qa) * lot),
                    ask_px: Px(ask * tick),
                    ask_qty: Qty(i64::from(qb) * lot),
                    update_id: uid,
                };
                self.out.push(MdEvent::Bbo(ev));
                self.last_pub[0] = t_ns;
                return;
            }
            50..70 => {
                let st = &mut self.insts[ix];
                st.trade_id += 1;
                let off = i64::from(extra >> 21) % (st.spread_ticks + 1);
                let (px, id) = (st.bid_ticks + off, st.trade_id);
                let ev = Trade {
                    h: self.hdr(t_ns, MD_CONN, inst, lat),
                    px: Px(px * tick),
                    qty: Qty(i64::from(qa) * lot),
                    trade_id: id,
                    taker_side: if extra & 1 == 0 { Side::Buy } else { Side::Sell },
                    _pad: [0; 7],
                };
                self.out.push(MdEvent::Trade(ev));
            }
            70..85 => {
                let st = &mut self.insts[ix];
                st.snap_id += 1;
                let depth = 1 + (extra >> 24) as i64 % 5;
                let nb = depth.min(st.bid_ticks);
                let qty = |i: i64, q: u16| Qty(((i64::from(q) + 7 * i) % 1000 + 1) * lot);
                let bids = (0..nb)
                    .map(|i| Level {
                        px: Px((st.bid_ticks - i) * tick),
                        qty: qty(i, qa),
                    })
                    .collect::<Vec<_>>();
                let ask0 = st.bid_ticks + st.spread_ticks;
                let asks = (0..depth)
                    .map(|i| Level {
                        px: Px((ask0 + i) * tick),
                        qty: qty(i, qb),
                    })
                    .collect::<Vec<_>>();
                let snap_id = st.snap_id;
                let hdr = L2SnapHdr {
                    h: self.hdr(t_ns, MD_CONN, inst, lat),
                    snap_id,
                    origin: L2SnapHdr::TOP_N,
                    _pad: [0; 3],
                    n_bids: bids.len() as u16,
                    n_asks: asks.len() as u16,
                };
                self.out
                    .push(MdEvent::L2Snapshot(OwnedL2Snapshot { hdr, bids, asks }));
            }
            85..95 => {
                let st = &self.insts[ix];
                let mid = st.bid_ticks + st.spread_ticks / 2 + i64::from(mv);
                let ev = MarkPrice {
                    h: self.hdr(t_ns, MD_CONN, inst, lat),
                    px: Px(mid.max(1) * tick),
                };
                self.out.push(MdEvent::Mark(ev));
            }
            _ => {
                if extra & 7 == 0 {
                    self.insts[ix].interval_s = [3_600, 14_400, 28_800][(extra >> 3) as usize % 3];
                }
                let interval_s = self.insts[ix].interval_s;
                let h = self.hdr(t_ns, MD_CONN, inst, lat);
                let period = i64::from(interval_s) * 1_000_000_000;
                let next = (h.ex_ts_ns.div_euclid(period) + 1).saturating_mul(period);
                let ev = Funding {
                    h,
                    rate: Rate(i64::from(qa) - 500),
                    next_funding_ns: next,
                    interval_s,
                    _pad: 0,
                };
                self.out.push(MdEvent::Funding(ev));
            }
        }
        self.last_pub[1] = t_ns;
    }
}

fn build(cfg: &MarketStreamCfg, steps: &[Step]) -> Vec<MdEvent> {
    let start_ticks = (cfg.start_bid.0 / cfg.tick.0).max(1);
    let st = InstState {
        bid_ticks: start_ticks,
        spread_ticks: 1,
        update_id: 0,
        trade_id: 0,
        snap_id: 0,
        interval_s: cfg.funding_interval_s,
    };
    let ping_ns = ms(u64::from(cfg.bbo_ping_ms));
    let mut b = Builder {
        cfg,
        out: Vec::with_capacity(steps.len() + 16),
        ping_ns,
        next_probe_ns: ping_ns,
        last_pub: [0; 2],
        recovered: false,
        insts: vec![st; cfg.insts.len()],
    };
    b.setup();
    let mut t_ns = 0u64;
    for s in steps {
        t_ns = t_ns.saturating_add(ms(u64::from(s.0)));
        b.advance_to(t_ns);
        b.step(t_ns, s);
    }
    b.out
}
