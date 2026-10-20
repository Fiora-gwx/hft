//! Fast part of `binary-maker` (sequencer thread, hot). README "规则": one post-only bid at the
//! best bid on the Up and the Down slot of the current window while the reference volatility is
//! high enough, the window is not about to end and the reference has not just jumped. No
//! allocation after `new`/`on_params`; snapshots are keyed by `GroupId`.
#![deny(clippy::float_arithmetic)]

use hft_strategy_sdk::prelude::*;

use crate::params::BinaryMakerParams;
use crate::vol::{ONE, Vol, isqrt};

/// Order tag of the quote of slot group `gix` (`gix + 1`).
pub fn tag_of(gix: u16) -> u32 {
    u32::from(gix) + 1
}

/// Book of every slot leg and the BBO of the reference leg (the last group).
pub fn subscribe(groups: &GroupTable, out: &mut SubsBuilder<'_>) {
    let n = groups.len();
    for (gix, g) in groups.iter() {
        let Some(l) = g.legs.first() else { continue };
        let mask = if gix + 1 == n { EvMask::BBO } else { EvMask::BOOK };
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
    /// Cancel sent.
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
    /// Engine time until which no quote rests (after a reference jump).
    pub pause_until: i64,
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
            pause_until: 0,
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
                let mid_x2 = i128::from(b.bid_px.0) + i128::from(b.ask_px.0);
                if b.bid_px.0 > 0 && b.ask_px.0 > 0 {
                    let now = v.now();
                    let d = self.p.defaults();
                    if let Some(s) = self.vol.sigma(d.vol_min_samples as usize) {
                        let jump = self.vol.ret_from_sample(mid_x2).unsigned_abs();
                        let lim = i128::from(s) * i128::from(d.move_k.0) / i128::from(ONE);
                        if i128::from(jump) >= lim && lim > 0 {
                            self.pause_until =
                                now.engine_ns.saturating_add(i64::from(d.pause_ms) * 1_000_000);
                        }
                    }
                    if now.clock_ok {
                        self.vol.on_mid(mid_x2, now.utc_ns.div_euclid(1_000_000_000));
                    }
                }
                self.run(v, ctx);
            }
            EventRef::Book { .. } => self.run(v, ctx),
            EventRef::Order { u, .. } => {
                if let Some(q) = self.quotes.iter_mut().find(|q| q.r == Some(u.r)) {
                    match u.kind {
                        OrderUpdateKind::Filled { done: true, .. }
                        | OrderUpdateKind::Cancelled
                        | OrderUpdateKind::Rejected { .. }
                        | OrderUpdateKind::Aged => {
                            q.state = QState::None;
                            q.r = None;
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
        let quoting = ok
            && now.engine_ns >= self.pause_until
            && left_ms > i64::from(d.stop_before_ms)
            && self.vol_ok(left_ms);
        // Net shares of the current window: Up minus Down.
        let pos = |q: Option<&Quote>| {
            q.and_then(|q| v.group_pos(self.account, q.id).legs.first().map(|l| l.qty.0))
                .unwrap_or(0)
        };
        let up = usize::try_from(cur * 2).unwrap_or(usize::MAX);
        let net = pos(self.quotes.get(up)).saturating_sub(pos(self.quotes.get(up.wrapping_add(1))));
        let requote = i64::from(d.requote_ms) * 1_000_000;
        for i in 0..self.quotes.len() {
            let Some(q) = self.quotes.get(i).copied() else {
                continue;
            };
            let hot = *v.inst(q.inst);
            let unit_px = 10i64.saturating_pow(u32::from(hot.px_exp.unsigned_abs()));
            let unit_qty = 10i64.saturating_pow(u32::from(hot.qty_exp.unsigned_abs()));
            let mut want = None;
            if quoting && i / 2 == up / 2 {
                let long = if i % 2 == 0 { net } else { -net };
                let book = v.book(q.inst);
                if book.status() == BookStatus::Live
                    && long < i64::from(d.inv_max_shares).saturating_mul(unit_qty)
                    && let Some(bid) = book.best(Side::Buy)
                {
                    let px = bid.px.0;
                    let lo = i128::from(d.p_min.0) * i128::from(unit_px) / i128::from(ONE);
                    let hi = i128::from(d.p_max.0) * i128::from(unit_px) / i128::from(ONE);
                    let below_ask = book.best(Side::Sell).is_none_or(|a| a.px.0 > px);
                    if i128::from(px) >= lo && i128::from(px) <= hi && below_ask {
                        want = Some(px);
                    }
                }
            }
            let since = now.engine_ns.saturating_sub(q.last_ens);
            match (q.state, want) {
                (QState::Live, Some(px)) if px == q.px => {}
                (QState::Live, _) => {
                    if want.is_some() && since < requote {
                        continue;
                    }
                    if let Some(r) = q.r
                        && ctx.cancel(r).is_ok()
                        && let Some(m) = self.quotes.get_mut(i)
                    {
                        m.state = QState::Cancelling;
                        m.last_ens = now.engine_ns;
                    }
                }
                (QState::None, Some(px)) if since >= requote => {
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
    /// gen u32), px: i64`; then `pause_until: i64`. Placing quotes are saved as none.
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
        }
        w.put_i64(self.pause_until);
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
            }
        }
        self.pause_until = r.get_i64()?;
        Ok(())
    }
}
