//! Fast part of `binary-maker` (sequencer thread, hot). README "规则": one post-only bid at the
//! best bid on the Up and the Down slot of the current window while the reference volatility is
//! high enough and the window is not about to end; a quote stays when its level clears, is
//! cancelled when undercut or when the reference moves its fair value against it, and a bid that
//! completes pairs rests no higher than the pair-cost limit; with `fv_limit`, bids are also capped
//! by the strategy's own fair value (anchored to the Up mid, moved by the reference, the inventory
//! and a direction forecast); with `comp_skip`/`comp_dir`, a composite direction score per token
//! skips the token it is against and lets the one it favours hold more inventory (`comp`); with
//! `load_gate_ns`/`mdslow_gate_ns`, the previous minute's feed lag stops quoting (`lag`). No
//! allocation after `new`/`on_params`; snapshots are keyed by `GroupId`.
#![deny(clippy::float_arithmetic)]

pub mod comp;
pub mod lag;

use hft_strategy_sdk::prelude::*;

use comp::{BidSide, Own, ScoreIn, SecHist, SlotHist, Trades};
use lag::LagGate;

use crate::norm::{cdf, inv_cdf};
use crate::params::BinaryMakerParams;
use crate::vol::{ONE, Vol, isqrt};

/// The settlement averages the reference over the last 60 s of the window: the variance left
/// `t` seconds before the end (t ≥ 60 s) is that of `t − 40` seconds of the spot.
pub const TWAP_TAIL_MS: i64 = 40_000;

/// Order tag of the quote of slot group `gix` (`gix + 1`).
pub fn tag_of(gix: u16) -> u32 {
    u32::from(gix) + 1
}

/// Book and trades of every slot leg (the trades drive the simulator's queue fills; the strategy
/// ignores them) and the BBO of the reference leg (the last group).
pub fn subscribe(groups: &GroupTable, out: &mut SubsBuilder<'_>) {
    let n = groups.len();
    for (gix, g) in groups.iter() {
        let Some(l) = g.legs.first() else { continue };
        let mask = if gix + 1 == n {
            EvMask::BBO
        } else {
            EvMask(EvMask::BOOK.0 | EvMask::TRADE.0)
        };
        let _ = out.market(l.inst, gix, mask, Part::Fast);
    }
}

/// √5 and √30 × 1e6 (forecast return scale and drift horizon, README "规则" 9).
const SQRT5_MICRO: i128 = 2_236_068;
const SQRT30_MICRO: i128 = 5_477_226;

/// Fair-value state of the current window (README "规则" 9): the reference 2 × mid at the window's
/// first event and the anchor offset (`Rate`), with the engine time of its last update.
#[derive(Clone, Copy, Debug, Default)]
pub struct FvAnchor {
    pub win: i64,
    pub b_open: i64,
    pub off: i64,
    pub off_ok: bool,
    pub off_ens: i64,
}

/// Composite scores of the current window's two tokens (README "规则" 10), cached for
/// `comp_every_ms`.
#[derive(Clone, Copy, Debug, Default)]
pub struct CompCache {
    pub ok: bool,
    pub win: i64,
    pub utc_ns: i64,
    /// Up, Down (`Rate` standard deviations).
    pub z: [i64; 2],
}

/// Quote state of one slot leg.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum QState {
    #[default]
    None,
    /// Placed in the previous event; its outcome is read on the next one.
    Placing,
    /// Has an OMS slot (`r`).
    Live,
    /// Cancel sent; reset to `None` when the slot pair's window changes or after `stuck_reset_ms`
    /// without a terminal update (README "规则" 8).
    Cancelling,
}

#[derive(Clone, Copy, Debug)]
pub struct Quote {
    pub id: GroupId,
    pub inst: InstId,
    pub venue: VenueId,
    pub state: QState,
    pub handle: u16,
    pub r: Option<OrderRef>,
    pub px: i64,
    /// Engine time of the last place or cancel.
    pub last_ens: i64,
    /// Start (UTC seconds) of the window the quote was placed in.
    pub win: i64,
    /// At placement (README "规则" 5): reference 2 × mid; Φ⁻¹(Up mid) and implied σ ×
    /// √(seconds left − 40), both 1e-9.
    pub b0: i64,
    pub d0: i64,
    pub den: i64,
    /// Engine time before which this leg places nothing (after a fair-value cancel or, with
    /// `nochase_ms`, after an undercut cancel).
    pub cool_until: i64,
    /// Engine times of this leg's last fills (ring of 8; `n_fill` counts all fills).
    pub fill_ens: [i64; 8],
    pub n_fill: u32,
}

impl Quote {
    /// Streak pause (README "规则" 7): the last `k` fills within `window` ns and the last fill
    /// less than `pause` ns ago.
    pub fn streak_paused(&self, now: i64, k: u32, window: i64, pause: i64) -> bool {
        if k < 2 || self.n_fill < k {
            return false;
        }
        let at = |back: u32| self.fill_ens[((self.n_fill - back) % 8) as usize];
        now.saturating_sub(at(k)) < window && now.saturating_sub(at(1)) < pause
    }
}

#[derive(Debug)]
pub struct Fast {
    pub account: AccountId,
    pub p: BinaryMakerParams,
    /// One entry per slot group (gix order); capacity `GroupTable::capacity()`.
    pub quotes: Vec<Quote>,
    scratch: Vec<Quote>,
    pub reference: InstId,
    pub vol: Vol,
    /// Last reference 2 × mid (0 = none yet).
    pub ref_mid_x2: i64,
    /// Level-1 imbalance of the last reference quote, (bid − ask quantity) / (bid + ask) (`Rate`).
    pub ref_imb: i64,
    /// Fair-value anchor of the current window (not in the snapshot: rebuilt on the next event).
    pub fv: FvAnchor,
    /// Per slot leg (quote order): per-second mid and best bid, consumed market-data stamps.
    pub hist: Vec<SlotHist>,
    /// Reference 2 × mid per second (score).
    pub ref_x2: Box<SecHist>,
    /// Large prints of the slot legs (score).
    pub trades: Box<Trades>,
    pub comp: CompCache,
    /// Feed-lag statistics (venue-load gates).
    pub lag: LagGate,
}

impl Fast {
    pub fn new(account: AccountId, p: &BinaryMakerParams, groups: &GroupTable) -> Fast {
        let cap = usize::from(groups.capacity());
        let mut f = Fast {
            account,
            p: p.clone(),
            quotes: Vec::with_capacity(cap),
            scratch: Vec::with_capacity(cap),
            reference: INST_NONE,
            vol: Vol::new(p.defaults().vol_window_s as usize),
            ref_mid_x2: 0,
            ref_imb: 0,
            fv: FvAnchor {
                win: i64::MIN,
                ..FvAnchor::default()
            },
            hist: Vec::with_capacity(cap),
            ref_x2: Box::default(),
            trades: Box::default(),
            comp: CompCache::default(),
            lag: LagGate::new(),
        };
        f.on_params(p, groups);
        f
    }

    /// New parameters: slot groups keep their quote state by `GroupId` and leg; the volatility
    /// ring restarts when its length changes.
    pub fn on_params(&mut self, p: &BinaryMakerParams, groups: &GroupTable) {
        let w = p.defaults().vol_window_s as usize;
        if w != self.vol.capacity() {
            self.vol = Vol::new(w);
        }
        self.p = p.clone();
        self.scratch.clear();
        self.scratch.extend_from_slice(&self.quotes);
        self.quotes.clear();
        let n = groups.len();
        for (gix, g) in groups.iter() {
            let Some(l) = g.legs.first() else { continue };
            if gix + 1 == n {
                self.reference = l.inst;
                continue;
            }
            let fresh = Quote {
                id: g.id,
                inst: l.inst,
                venue: l.venue,
                state: QState::None,
                handle: 0,
                r: None,
                px: 0,
                last_ens: i64::MIN / 2,
                win: i64::MIN,
                b0: 0,
                d0: 0,
                den: 0,
                cool_until: i64::MIN / 2,
                fill_ens: [i64::MIN / 2; 8],
                n_fill: 0,
            };
            let kept = self
                .scratch
                .iter()
                .find(|q| q.id == g.id && q.inst == l.inst)
                .copied()
                .unwrap_or(fresh);
            self.quotes.push(kept);
        }
        // Score histories follow the slot insts (rebuilt when they change).
        if !self
            .hist
            .iter()
            .map(|h| h.inst)
            .eq(self.quotes.iter().map(|q| q.inst))
        {
            self.hist.clear();
            self.hist
                .extend(self.quotes.iter().map(|q| SlotHist::new(q.inst)));
        }
        self.comp.ok = false;
    }

    /// Composite score on (`comp_skip` or `comp_dir` set).
    pub fn comp_on(&self) -> bool {
        let d = self.p.defaults();
        d.comp_skip.0 > 0 || d.comp_dir.0 > 0
    }

    /// Venue-load gates on.
    pub fn lag_on(&self) -> bool {
        let d = self.p.defaults();
        d.load_gate_ns > 0 || d.mdslow_gate_ns > 0
    }

    /// Before the rules (README "规则" 10, 11): the reference 2 × mid of the second, every slot
    /// book's 2 × mid and best bid of the second, large prints; the slot insts' new market-data
    /// stamps into the lag statistics.
    fn observe<V: Views>(&mut self, ev: &EventRef<'_>, v: &V, utc_ns: i64) {
        let sec = utc_ns.div_euclid(1_000_000_000);
        if self.comp_on() {
            match *ev {
                EventRef::Bbo { inst, b, .. } if inst == self.reference => {
                    if b.bid_px.0 > 0 && b.ask_px.0 > 0 {
                        self.ref_x2.put(sec, b.bid_px.0.saturating_add(b.ask_px.0));
                    }
                }
                EventRef::Trade { inst, t, .. } => {
                    let unit_qty = 10i64.saturating_pow(u32::from(v.inst(inst).qty_exp.unsigned_abs()));
                    let min = i64::from(self.p.defaults().comp_flow_min_shares).saturating_mul(unit_qty);
                    if t.qty.0 >= min && self.hist.iter().any(|h| h.inst == inst) {
                        let q = if t.side == Side::Buy { t.qty.0 } else { -t.qty.0 };
                        self.trades.push(utc_ns, inst, q);
                    }
                }
                _ => {}
            }
            for h in self.hist.iter_mut() {
                let b = v.book(h.inst);
                if b.status() != BookStatus::Live {
                    continue;
                }
                if let (Some(x), Some(y)) = (b.best(Side::Buy), b.best(Side::Sell))
                    && x.px.0 < y.px.0
                {
                    h.mid_x2.put(sec, x.px.0.saturating_add(y.px.0));
                    h.bid.put(sec, x.px.0);
                }
            }
        }
        if self.lag_on() {
            // New stamps of all slot insts, in receive order (at most MD_STAMPS_KEPT per inst).
            const MAX: usize = MD_STAMPS_KEPT as usize * 16;
            let mut new = [(0i64, 0i64); MAX];
            let mut n = 0usize;
            for h in self.hist.iter_mut() {
                let count = v.md_count(h.inst);
                let from = h
                    .md_seen
                    .saturating_add(1)
                    .max(count.saturating_sub(MD_STAMPS_KEPT - 1));
                for k in from..=count {
                    let Some(m) = v.md_stamp(h.inst, k) else { continue };
                    let ex_ms = m.ex_ts_ns / 1_000_000;
                    if ex_ms == 0 || m.sw_rx_ns <= 0 {
                        continue;
                    }
                    if let Some(x) = new.get_mut(n) {
                        *x = (
                            m.sw_rx_ns,
                            m.sw_rx_ns.saturating_sub(ex_ms.saturating_mul(1_000_000)),
                        );
                        n += 1;
                    }
                }
                h.md_seen = count;
            }
            if let Some(new) = new.get_mut(..n) {
                new.sort_unstable();
                for (rx, lag) in new.iter() {
                    self.lag.push(*rx, *lag);
                }
            }
            self.lag.tick(utc_ns);
        }
    }

    /// Our resting bid of quote `i` in the current window `start` as the venue book shows it
    /// (`comp_excl_own`): price and remaining quantity of an acknowledged order.
    fn own_bid<V: Views>(&self, v: &V, i: usize, start: i64) -> Own {
        if !self.p.defaults().comp_excl_own {
            return None;
        }
        let q = self.quotes.get(i)?;
        if !matches!(q.state, QState::Live | QState::Cancelling) || q.win != start {
            return None;
        }
        let o = v.order(q.r?)?;
        matches!(
            o.state,
            OrderStateView::Live | OrderStateView::PendingCancel | OrderStateView::PendingAmend
        )
        .then_some((o.px.0, o.qty.0.saturating_sub(o.filled.0).max(0)))
    }

    /// Composite score of token quote `x` against its complement `o` (README "规则" 10).
    pub fn comp_score<V: Views>(&self, v: &V, x: usize, o: usize, start: i64, utc_ns: i64) -> Option<i64> {
        let d = self.p.defaults();
        let (qx, qo) = (self.quotes.get(x)?, self.quotes.get(o)?);
        let hx = self.hist.get(x)?;
        let hot = v.inst(qx.inst);
        let unit_px = 10i64.saturating_pow(u32::from(hot.px_exp.unsigned_abs()));
        let unit_qty = 10i64.saturating_pow(u32::from(hot.qty_exp.unsigned_abs()));
        let bx = BidSide::read(&v.book(qx.inst), self.own_bid(v, x, start));
        let bo = BidSide::read(&v.book(qo.inst), self.own_bid(v, o, start));
        let implied = self
            .vol
            .sigma(d.vol_min_samples as usize)
            .map(|s| (i128::from(s) * i128::from(d.sigma_scale.0) / i128::from(ONE)) as i64);
        let f = comp::features(&ScoreIn {
            x: &bx,
            o: &bo,
            hx,
            ref_x2: &self.ref_x2,
            trades: &self.trades,
            x_inst: qx.inst,
            o_inst: qo.inst,
            is_up: x.is_multiple_of(2),
            utc_ns,
            implied,
            unit_px,
            unit_qty,
        });
        Some(comp::score(&f, &self.comp_mu(), &self.comp_sd()))
    }

    /// `comp_*_mu` in feature order.
    pub fn comp_mu(&self) -> [i64; comp::N_FEAT] {
        let d = self.p.defaults();
        [
            d.comp_imb1_mu.0,
            d.comp_imb5_mu.0,
            d.comp_mom30_mu.0,
            d.comp_mom60_mu.0,
            d.comp_bret15_mu.0,
            d.comp_bret60_mu.0,
            d.comp_flow30_mu.0,
            d.comp_bdrop1_mu.0,
        ]
    }

    /// `comp_*_sd` in feature order.
    pub fn comp_sd(&self) -> [i64; comp::N_FEAT] {
        let d = self.p.defaults();
        [
            d.comp_imb1_sd.0,
            d.comp_imb5_sd.0,
            d.comp_mom30_sd.0,
            d.comp_mom60_sd.0,
            d.comp_bret15_sd.0,
            d.comp_bret60_sd.0,
            d.comp_flow30_sd.0,
            d.comp_bdrop1_sd.0,
        ]
    }

    /// Scores of the current window's tokens (`up` = Up quote index), recomputed at most every
    /// `comp_every_ms` while quoting; cleared otherwise.
    fn comp_update<V: Views>(&mut self, v: &V, quoting: bool, start: i64, up: usize, utc_ns: i64) {
        let dn = up.wrapping_add(1);
        let books = [up, dn].iter().all(|&i| {
            self.quotes
                .get(i)
                .is_some_and(|q| v.book(q.inst).status() != BookStatus::Uninit)
        });
        if !(self.comp_on() && quoting && books) {
            self.comp.ok = false;
            return;
        }
        let every = i64::from(self.p.defaults().comp_every_ms) * 1_000_000;
        if self.comp.ok && self.comp.win == start && utc_ns.saturating_sub(self.comp.utc_ns) < every {
            return;
        }
        match (
            self.comp_score(v, up, dn, start, utc_ns),
            self.comp_score(v, dn, up, start, utc_ns),
        ) {
            (Some(zu), Some(zd)) => {
                self.comp = CompCache {
                    ok: true,
                    win: start,
                    utc_ns,
                    z: [zu, zd],
                };
            }
            _ => self.comp.ok = false,
        }
    }

    pub fn on_event<V: Views>(&mut self, ev: &EventRef<'_>, v: &V, ctx: &mut Ctx<'_>) {
        self.outcomes(ctx);
        let now = v.now();
        if now.clock_ok {
            self.observe(ev, v, now.utc_ns);
        }
        match *ev {
            EventRef::Bbo { inst, b, .. } if inst == self.reference => {
                if b.bid_px.0 > 0 && b.ask_px.0 > 0 {
                    let mid_x2 = b.bid_px.0.saturating_add(b.ask_px.0);
                    self.ref_mid_x2 = mid_x2;
                    let (bq, aq) = (i128::from(b.bid_qty.0), i128::from(b.ask_qty.0));
                    if bq >= 0 && aq >= 0 && bq + aq > 0 {
                        self.ref_imb = ((bq - aq) * i128::from(ONE) / (bq + aq)) as i64;
                    }
                    let now = v.now();
                    if now.clock_ok {
                        self.vol
                            .on_mid(i128::from(mid_x2), now.utc_ns.div_euclid(1_000_000_000));
                    }
                }
                self.run(v, ctx);
            }
            EventRef::Book { .. } => self.run(v, ctx),
            EventRef::Order { u, .. } => {
                let now = v.now().engine_ns;
                if let Some(q) = self.quotes.iter_mut().find(|q| q.r == Some(u.r)) {
                    if let OrderUpdateKind::Filled { d, .. } = u.kind
                        && d.0 > 0
                    {
                        q.fill_ens[(q.n_fill % 8) as usize] = now;
                        q.n_fill = q.n_fill.wrapping_add(1);
                    }
                    match u.kind {
                        OrderUpdateKind::Filled { done: true, .. }
                        | OrderUpdateKind::Cancelled
                        | OrderUpdateKind::Rejected { .. } => {
                            q.state = QState::None;
                            q.r = None;
                        }
                        // Still resting after `max_rest_ms`: cancel it (the OMS does not).
                        OrderUpdateKind::Aged if q.state == QState::Live => {
                            let _ = ctx.cancel(u.r);
                            q.state = QState::Cancelling;
                            q.last_ens = now;
                        }
                        _ => {}
                    }
                }
                self.run(v, ctx);
            }
            _ => {}
        }
    }

    /// Outcomes of the previous event's places.
    fn outcomes(&mut self, ctx: &Ctx<'_>) {
        for q in self.quotes.iter_mut().filter(|q| q.state == QState::Placing) {
            match ctx.outcome(IntentHandle(q.handle)) {
                Some(IntentOutcome::Placed(r)) => {
                    q.state = QState::Live;
                    q.r = Some(r);
                }
                _ => {
                    q.state = QState::None;
                    q.r = None;
                }
            }
        }
    }

    /// Change of the Up probability (1e-9) since quote `q` was placed: the reference move since
    /// then through the digital delta, Φ(d0 + move / den) − Φ(d0) (README "规则" 5); 0 without
    /// an anchor.
    pub fn dfv(&self, q: &Quote) -> i64 {
        if q.den <= 0 || q.b0 <= 0 || self.ref_mid_x2 <= 0 {
            return 0;
        }
        let one = i128::from(ONE);
        let moved = (i128::from(self.ref_mid_x2) - i128::from(q.b0)) * one / i128::from(q.b0);
        let shift = moved * one / i128::from(q.den);
        let z = (i128::from(q.d0) + shift).clamp(-6 * one, 6 * one) as i64;
        cdf(z) - cdf(q.d0)
    }

    /// Direction forecast in standard deviations of the next 30 s (`Rate`, README "规则" 9):
    /// `pred_w_imb` × level-1 imbalance + `pred_w_ret` × (sum of the last 5 one-second returns) /
    /// (σ √5) + `pred_w_ser` × (sum of the last 15) / (sum of their absolute values); clamped to ±6.
    pub fn forecast(&self, sigma: i64) -> i64 {
        let d = self.p.defaults();
        let one = i128::from(ONE);
        let (s5, _) = self.vol.recent(5);
        let (s15, a15) = self.vol.recent(15);
        let sd5 = i128::from(sigma) * SQRT5_MICRO / 1_000_000;
        let ret = if sd5 > 0 {
            (i128::from(s5) * one / sd5).clamp(-6 * one, 6 * one)
        } else {
            0
        };
        let ser = if a15 > 0 {
            i128::from(s15) * one / i128::from(a15)
        } else {
            0
        };
        let z = (i128::from(d.pred_w_imb.0) * i128::from(self.ref_imb)
            + i128::from(d.pred_w_ret.0) * ret
            + i128::from(d.pred_w_ser.0) * ser)
            / one;
        z.clamp(-6 * one, 6 * one) as i64
    }

    /// Fair-value limits (Up, Down) of the current window as fractions of one (README "规则" 9),
    /// updating the anchor; `None` before the window has a reference price, an anchor or σ.
    /// `up` is the Up slot index, `den` = implied σ × √(seconds left − 40), `net` = Up − Down raw
    /// quantity.
    fn fair_limits<V: Views>(
        &mut self,
        v: &V,
        start: i64,
        up: usize,
        den: i64,
        net: i64,
        now_ens: i64,
    ) -> Option<(i64, i64)> {
        let d = *self.p.defaults();
        if self.fv.win != start {
            self.fv = FvAnchor {
                win: start,
                ..FvAnchor::default()
            };
        }
        if self.fv.b_open <= 0 {
            self.fv.b_open = self.ref_mid_x2;
        }
        let sigma = self.vol.sigma(d.vol_min_samples as usize)?;
        let q = *self.quotes.get(up)?;
        if self.fv.b_open <= 0 || self.ref_mid_x2 <= 0 || den <= 0 {
            return None;
        }
        let one = i128::from(ONE);
        let moved =
            (i128::from(self.ref_mid_x2) - i128::from(self.fv.b_open)) * one / i128::from(self.fv.b_open);
        let hot = *v.inst(q.inst);
        let unit = i128::from(10i64.saturating_pow(u32::from(hot.px_exp.unsigned_abs())));
        let book = v.book(q.inst);
        if let (Some(x), Some(y)) = (book.best(Side::Buy), book.best(Side::Sell))
            && x.px.0 < y.px.0
            && unit > 0
        {
            let mid = (i128::from(x.px.0) + i128::from(y.px.0)) * one / (2 * unit);
            if mid >= one * 3 / 100 && mid <= one * 97 / 100 {
                // Reference move the Up mid implies, against the actual one: the slow part is the
                // strike / basis offset, the fast part a move the book has not absorbed yet.
                let gap = moved - i128::from(inv_cdf(mid as i64)) * i128::from(den) / one;
                if self.fv.off_ok {
                    let dt = i128::from(now_ens.saturating_sub(self.fv.off_ens).max(0));
                    let hl = i128::from(d.fv_anchor_ms) * 1_000_000;
                    let off = i128::from(self.fv.off);
                    self.fv.off = (off + (gap - off) * dt / (hl + dt)) as i64;
                } else {
                    self.fv.off = gap as i64;
                    self.fv.off_ok = true;
                }
                self.fv.off_ens = now_ens;
            }
        }
        if !self.fv.off_ok {
            return None;
        }
        let drift = i128::from(d.pred_k.0) * i128::from(self.forecast(sigma)) / one * i128::from(sigma) / one
            * SQRT30_MICRO
            / 1_000_000;
        let z = ((moved - i128::from(self.fv.off) + drift) * one / i128::from(den)).clamp(-6 * one, 6 * one);
        let fu = i128::from(cdf(z as i64));
        let unit_qty = i128::from(10i64.saturating_pow(u32::from(hot.qty_exp.unsigned_abs())));
        let cap = i128::from(d.inv_max_shares) * unit_qty;
        let skew = if cap > 0 {
            i128::from(d.inv_skew.0) * i128::from(net) / cap
        } else {
            0
        };
        let edge = i128::from(d.fv_edge.0);
        Some(((fu - edge - skew) as i64, (one - fu - edge + skew) as i64))
    }

    /// README "规则": decides every slot leg's quote.
    fn run<V: Views>(&mut self, v: &V, ctx: &mut Ctx<'_>) {
        let now = v.now();
        let d = *self.p.defaults();
        let pairs = (self.quotes.len() / 2) as i64;
        let w = i64::from(d.window_s);
        let ok = d.enabled && now.clock_ok && pairs > 0 && w > 0;
        let utc_ms = now.utc_ns.div_euclid(1_000_000);
        let start = utc_ms.div_euclid(1_000).div_euclid(w) * w;
        let cur = if pairs > 0 {
            (start / w).rem_euclid(pairs)
        } else {
            -1
        };
        let left_ms = (start + w) * 1_000 - utc_ms;
        // Venue-load gates (README "规则" 11): the previous minute's feed lag closes quoting.
        let (load, slow) = self.lag.gates(now.utc_ns, d.load_gate_ns, d.mdslow_gate_ns);
        let quoting = ok && left_ms > i64::from(d.stop_before_ms) && self.vol_ok(left_ms) && !load && !slow;
        // Shares and average cost of the current window's Up and Down legs.
        let pos = |q: Option<&Quote>| {
            q.and_then(|q| {
                v.group_pos(self.account, q.id)
                    .legs
                    .first()
                    .map(|l| (l.qty.0, l.entry_px.0))
            })
            .unwrap_or((0, 0))
        };
        let up = usize::try_from(cur * 2).unwrap_or(usize::MAX);
        let (qu, cu) = pos(self.quotes.get(up));
        let (qd, cd) = pos(self.quotes.get(up.wrapping_add(1)));
        // Bought cost of the window (1e-6 units) against `max_window_cost`.
        let cost_full = d.max_window_cost.0 > 0
            && self.quotes.get(up).is_some_and(|q| {
                let hot = v.inst(q.inst);
                let e = i32::from(hot.qty_exp) + i32::from(hot.px_exp) + 6;
                let raw = i128::from(qu) * i128::from(cu) + i128::from(qd) * i128::from(cd);
                let cost = if e >= 0 {
                    raw * 10i128.pow(e.unsigned_abs())
                } else {
                    raw / 10i128.pow(e.unsigned_abs())
                };
                cost >= i128::from(d.max_window_cost.0)
            });
        // Fair-value anchor of new quotes: Φ⁻¹(Up token mid) and implied σ × √(left − 40 s).
        let up_mid = self.quotes.get(up).map_or(ONE / 2, |q| {
            let b = v.book(q.inst);
            let unit = 10i64.saturating_pow(u32::from(v.inst(q.inst).px_exp.unsigned_abs()));
            match (b.best(Side::Buy), b.best(Side::Sell)) {
                (Some(x), Some(y)) if unit > 0 => {
                    ((i128::from(x.px.0) + i128::from(y.px.0)) * i128::from(ONE) / (2 * i128::from(unit)))
                        as i64
                }
                _ => ONE / 2,
            }
        });
        let den = self.vol.sigma(d.vol_min_samples as usize).map_or(0, |s| {
            let implied = i128::from(s) * i128::from(d.sigma_scale.0) / i128::from(ONE);
            let v_ms = (left_ms - TWAP_TAIL_MS).max(1);
            let root_milli = isqrt(u128::try_from(v_ms).unwrap_or(1) * 1_000) as i128;
            (implied * root_milli / 1_000) as i64
        });
        let lims = if d.fv_limit && quoting {
            self.fair_limits(v, start, up, den, qu.saturating_sub(qd), now.engine_ns)
        } else {
            None
        };
        self.comp_update(v, quoting, start, up, now.utc_ns);
        let comp = self.comp;
        let requote = i64::from(d.requote_ms) * 1_000_000;
        let cool = i64::from(d.fv_cool_ms) * 1_000_000;
        let nochase = i64::from(d.nochase_ms) * 1_000_000;
        let streak_window = i64::from(d.streak_window_ms) * 1_000_000;
        let streak_pause = i64::from(d.streak_pause_ms) * 1_000_000;
        let stuck = i64::from(d.stuck_reset_ms) * 1_000_000;
        for i in 0..self.quotes.len() {
            let Some(mut q) = self.quotes.get(i).copied() else {
                continue;
            };
            // Stale state (README "规则" 8): a quote of an earlier window on the current slot pair,
            // or a cancel without a terminal update for `stuck_reset_ms`: cancel once more (never
            // waits) and forget it; later updates of that order no longer match `r`.
            let old_window = i / 2 == up / 2 && q.win != start;
            let stale = match q.state {
                QState::Live => old_window,
                QState::Cancelling => old_window || now.engine_ns.saturating_sub(q.last_ens) >= stuck,
                QState::None | QState::Placing => false,
            };
            if stale {
                if let Some(r) = q.r {
                    let _ = ctx.cancel(r);
                }
                q.state = QState::None;
                q.r = None;
                if let Some(m) = self.quotes.get_mut(i) {
                    m.state = QState::None;
                    m.r = None;
                }
            }
            let hot = *v.inst(q.inst);
            let unit_px = 10i64.saturating_pow(u32::from(hot.px_exp.unsigned_abs()));
            let unit_qty = 10i64.saturating_pow(u32::from(hot.qty_exp.unsigned_abs()));
            let is_up = i % 2 == 0;
            let mine = quoting && i / 2 == up / 2;
            let book = v.book(q.inst);
            let live = book.status() == BookStatus::Live;
            let bid = book.best(Side::Buy).map(|l| l.px.0);
            let ask = book.best(Side::Sell).map(|l| l.px.0);
            // Locked or crossed (one taker trade arriving as several level updates): no undercut.
            let locked = matches!((bid, ask), (Some(b), Some(a)) if b >= a);
            // Pair-cost limit when this bid completes pairs: 1 − other side's cost − margin,
            // floored to a whole cent.
            let (own_q, other_q, other_c) = if is_up { (qu, qd, cd) } else { (qd, qu, cu) };
            let lim = (other_q > own_q).then(|| {
                let margin = (i128::from(d.pair_margin.0) * i128::from(unit_px) / i128::from(ONE)) as i64;
                let cent = (unit_px / 100).max(1);
                (unit_px - other_c - margin).div_euclid(cent) * cent
            });
            let lo = (i128::from(d.p_min.0) * i128::from(unit_px) / i128::from(ONE)) as i64;
            let hi = (i128::from(d.p_max.0) * i128::from(unit_px) / i128::from(ONE)) as i64;
            // Fair-value limit of this leg (README "规则" 9), raw price; the bid cap is floored to a cent.
            let fv_px = lims.map(|(lu, ld)| {
                (i128::from(if is_up { lu } else { ld }) * i128::from(unit_px) / i128::from(ONE)) as i64
            });
            let cent = (unit_px / 100).max(1);
            let fv_cap = fv_px.map(|f| f.div_euclid(cent) * cent);
            // Composite score (README "规则" 10): skip the token it is against; the one it favours
            // may hold up to dir_cap_shares net.
            let z = (comp.ok && comp.win == start && i / 2 == up / 2)
                .then(|| comp.z.get(i % 2).copied())
                .flatten();
            let skip = d.comp_skip.0 > 0 && z.is_some_and(|z| z < -d.comp_skip.0);
            let cap_shares = if d.comp_dir.0 > 0 && z.is_some_and(|z| z > d.comp_dir.0) {
                d.dir_cap_shares
            } else {
                d.inv_max_shares
            };
            let cap = i64::from(cap_shares).saturating_mul(unit_qty);
            // The net after this clip fills completely stays within the cap (a short fill such as
            // 9.99 of 10 shares no longer admits a whole further clip).
            let clip = i64::from(d.clip_shares).saturating_mul(unit_qty);
            let target = match (mine && live, bid, ask) {
                (true, Some(b), Some(a))
                    if b < a
                        && !skip
                        && b >= lo
                        && b <= hi
                        && own_q.saturating_sub(other_q).saturating_add(clip) <= cap
                        && (!d.fv_limit || fv_cap.is_some()) =>
                {
                    let t = lim.map_or(b, |l| b.min(l));
                    let t = fv_cap.map_or(t, |f| t.min(f));
                    (t >= lo).then_some(t)
                }
                _ => None,
            };
            let since = now.engine_ns.saturating_sub(q.last_ens);
            match q.state {
                QState::Live => {
                    let mut cancel = !(mine && live) || skip;
                    let mut cooled = false;
                    if !cancel && d.fv_cancel.0 > 0 {
                        let x = self.dfv(&q);
                        if (is_up && x <= -d.fv_cancel.0) || (!is_up && x >= d.fv_cancel.0) {
                            cancel = true;
                            cooled = true;
                        }
                    }
                    // Undercut (a better bid than ours where we would quote higher, the book not
                    // locked or crossed) or above the pair-cost limit; a cleared level (best bid
                    // below ours) keeps the quote.
                    let undercut =
                        !locked && bid.is_some_and(|b| b > q.px) && target.is_none_or(|t| t > q.px);
                    cancel |= undercut;
                    cancel |= lim.is_some_and(|l| q.px > l);
                    let hyst = (i128::from(d.fv_hyst.0) * i128::from(unit_px) / i128::from(ONE)) as i64;
                    cancel |= fv_px.is_some_and(|f| q.px > f.saturating_add(hyst));
                    if cancel
                        && let Some(r) = q.r
                        && ctx.cancel(r).is_ok()
                        && let Some(m) = self.quotes.get_mut(i)
                    {
                        m.state = QState::Cancelling;
                        m.last_ens = now.engine_ns;
                        if cooled {
                            m.cool_until = now.engine_ns.saturating_add(cool);
                        }
                        if undercut && nochase > 0 {
                            m.cool_until = m.cool_until.max(now.engine_ns.saturating_add(nochase));
                        }
                    }
                }
                QState::None => {
                    let Some(px) = target else { continue };
                    if since < requote
                        || now.engine_ns < q.cool_until
                        || cost_full
                        || q.streak_paused(now.engine_ns, d.streak_k, streak_window, streak_pause)
                    {
                        continue;
                    }
                    let it = OrderIntent {
                        venue: q.venue,
                        account: self.account,
                        side: Side::Buy,
                        tif: Tif::PostOnly,
                        class: IntentClass::Open,
                        flags: 0,
                        _pad: [0; 2],
                        inst: q.inst,
                        group: q.id,
                        px: Px(px),
                        qty: Qty(i64::from(d.clip_shares).saturating_mul(unit_qty)),
                        link: LegLinkOpt::NONE,
                        tag: tag_of(i as u16),
                        max_rest_ms: d.max_rest_ms,
                    };
                    if let Ok(h) = ctx.place(it)
                        && let Some(m) = self.quotes.get_mut(i)
                    {
                        m.state = QState::Placing;
                        m.handle = h.0;
                        m.px = px;
                        m.last_ens = now.engine_ns;
                        m.win = start;
                        m.b0 = self.ref_mid_x2;
                        m.d0 = inv_cdf(up_mid);
                        m.den = den;
                    }
                }
                _ => {}
            }
        }
    }

    /// Implied volatility (reference sigma × `sigma_scale`) at least `sigma_min`, and the
    /// remaining standard deviation (implied sigma × √seconds left) at least `min_rem_sd`.
    fn vol_ok(&self, left_ms: i64) -> bool {
        let d = self.p.defaults();
        let Some(s) = self.vol.sigma(d.vol_min_samples as usize) else {
            return false;
        };
        let implied = i128::from(s) * i128::from(d.sigma_scale.0) / i128::from(ONE);
        // √(left_ms × 1000) = √(seconds left) × 1000.
        let root_milli = isqrt(u128::try_from(left_ms.max(0)).unwrap_or(0) * 1_000) as i128;
        implied >= i128::from(d.sigma_min.0) && implied * root_milli / 1_000 >= i128::from(d.min_rem_sd.0)
    }

    /// `n: u16`, then per slot group `id: u32, state: u8, r?: (venue u8, account u8, slot u32,
    /// gen u32), px: i64, b0: i64, d0: i64, den: i64, cool_until: i64, n_fill: u32,
    /// fill_ens: 8 × i64, last_ens: i64, win: i64`. Placing quotes are saved as none.
    pub fn snapshot(&self, w: &mut SnapshotWriter) {
        w.put_u16(self.quotes.len() as u16);
        for q in &self.quotes {
            w.put_u32(q.id.0);
            let (state, r) = match (q.state, q.r) {
                (QState::Live | QState::Cancelling, Some(r)) => (q.state as u8, r),
                _ => (
                    QState::None as u8,
                    OrderRef {
                        venue: VenueId(0),
                        account: AccountId(0),
                        _pad: [0; 2],
                        slot: 0,
                        r#gen: 0,
                    },
                ),
            };
            w.put_u8(state);
            w.put_u8(r.venue.0);
            w.put_u8(r.account.0);
            w.put_u32(r.slot);
            w.put_u32(r.r#gen);
            w.put_i64(q.px);
            w.put_i64(q.b0);
            w.put_i64(q.d0);
            w.put_i64(q.den);
            w.put_i64(q.cool_until);
            w.put_u32(q.n_fill);
            for t in q.fill_ens {
                w.put_i64(t);
            }
            w.put_i64(q.last_ens);
            w.put_i64(q.win);
        }
    }

    /// Restores by `GroupId`; saved groups no longer configured are dropped.
    pub fn restore(&mut self, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        let n = r.get_u16()?;
        for _ in 0..n {
            let id = GroupId(r.get_u32()?);
            let state = r.get_u8()?;
            let venue = VenueId(r.get_u8()?);
            let account = AccountId(r.get_u8()?);
            let slot = r.get_u32()?;
            let r#gen = r.get_u32()?;
            let px = r.get_i64()?;
            let b0 = r.get_i64()?;
            let d0 = r.get_i64()?;
            let den = r.get_i64()?;
            let cool_until = r.get_i64()?;
            let n_fill = r.get_u32()?;
            let mut fill_ens = [0i64; 8];
            for t in fill_ens.iter_mut() {
                *t = r.get_i64()?;
            }
            let last_ens = r.get_i64()?;
            let win = r.get_i64()?;
            if let Some(q) = self.quotes.iter_mut().find(|q| q.id == id) {
                let (st, oref) = match state {
                    2 => (QState::Live, true),
                    3 => (QState::Cancelling, true),
                    _ => (QState::None, false),
                };
                q.state = st;
                q.r = oref.then_some(OrderRef {
                    venue,
                    account,
                    _pad: [0; 2],
                    slot,
                    r#gen,
                });
                q.px = px;
                q.b0 = b0;
                q.d0 = d0;
                q.den = den;
                q.cool_until = cool_until;
                q.n_fill = n_fill;
                q.fill_ens = fill_ens;
                q.last_ens = last_ens;
                q.win = win;
            }
        }
        Ok(())
    }
}
