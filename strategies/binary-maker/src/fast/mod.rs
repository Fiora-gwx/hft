//! Fast part of `binary-maker` (sequencer thread, hot). README "规则": one post-only bid at the
//! best bid on the Up and the Down slot of the current window while the reference volatility is
//! high enough and the window is not about to end; a quote stays when its level clears, is
//! cancelled when undercut or when the reference moves its fair value against it, and a bid that
//! completes pairs rests no higher than the pair-cost limit. No allocation after
//! `new`/`on_params`; snapshots are keyed by `GroupId`.
#![deny(clippy::float_arithmetic)]

use hft_strategy_sdk::prelude::*;

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
    }

    pub fn on_event<V: Views>(&mut self, ev: &EventRef<'_>, v: &V, ctx: &mut Ctx<'_>) {
        self.outcomes(ctx);
        match *ev {
            EventRef::Bbo { inst, b, .. } if inst == self.reference => {
                if b.bid_px.0 > 0 && b.ask_px.0 > 0 {
                    let mid_x2 = b.bid_px.0.saturating_add(b.ask_px.0);
                    self.ref_mid_x2 = mid_x2;
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
        let quoting = ok && left_ms > i64::from(d.stop_before_ms) && self.vol_ok(left_ms);
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
            let cap = i64::from(d.inv_max_shares).saturating_mul(unit_qty);
            let target = match (mine && live, bid, ask) {
                (true, Some(b), Some(a))
                    if b < a && b >= lo && b <= hi && own_q.saturating_sub(other_q) < cap =>
                {
                    let t = lim.map_or(b, |l| b.min(l));
                    (t >= lo).then_some(t)
                }
                _ => None,
            };
            let since = now.engine_ns.saturating_sub(q.last_ens);
            match q.state {
                QState::Live => {
                    let mut cancel = !(mine && live);
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
