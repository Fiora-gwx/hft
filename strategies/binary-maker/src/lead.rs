//! Rule 15: the lead quote (README "规则" 15; research: Python reference `sim2.py`
//! `perp_triggers`, `Strat.on_lead`, `Strat.on_perp_against`). A move of the perpetual reference's
//! mid over a short look-back places a post-only bid one cent inside the spread on the token it
//! favours; an adverse move cancels that token's lead bid. 1e-9 fixed point throughout.

use hft_strategy_sdk::prelude::OrderRef;

/// Perpetual mid updates kept (≥ 3 s at 2 000 updates/s).
pub const RING: usize = 8_192;

/// The perpetual's 2 × mid per update in engine time order.
#[derive(Clone, Debug)]
pub struct PerpRing {
    t: Box<[i64; RING]>,
    v: Box<[i64; RING]>,
    head: usize,
    n: usize,
}

impl Default for PerpRing {
    fn default() -> PerpRing {
        PerpRing {
            t: Box::new([0; RING]),
            v: Box::new([0; RING]),
            head: 0,
            n: 0,
        }
    }
}

impl PerpRing {
    /// One update at engine time `t` (non-decreasing) with 2 × mid `mid_x2` (> 0).
    pub fn push(&mut self, t: i64, mid_x2: i64) {
        if let (Some(a), Some(b)) = (self.t.get_mut(self.head), self.v.get_mut(self.head)) {
            *a = t;
            *b = mid_x2;
        }
        self.head = (self.head + 1) % RING;
        self.n = (self.n + 1).min(RING);
    }
    /// Entry `k` from the oldest (k < n).
    fn get(&self, k: usize) -> Option<(i64, i64)> {
        let i = (self.head + RING - self.n + k) % RING;
        Some((*self.t.get(i)?, *self.v.get(i)?))
    }
    /// The 2 × mid of the latest update at or before `t`; `None` when every kept update is later.
    pub fn at_or_before(&self, t: i64) -> Option<i64> {
        // Number of entries with time ≤ t (times are non-decreasing).
        let (mut lo, mut hi) = (0usize, self.n);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.get(mid).is_some_and(|(x, _)| x <= t) {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo.checked_sub(1).and_then(|k| self.get(k)).map(|(_, v)| v)
    }
}

/// Relative change (1e-9) of `now` against `base`; 0 when `base` ≤ 0.
pub fn ret(now: i64, base: i64) -> i64 {
    if base <= 0 {
        return 0;
    }
    ((i128::from(now) - i128::from(base)) * 1_000_000_000 / i128::from(base)) as i64
}

/// A move of at least `x` (1e-9) over the look-back with its own cool-down per direction.
#[derive(Clone, Copy, Debug)]
pub struct Trigger {
    /// Engine time of the last trigger, up (index 0) and down (index 1).
    pub last: [i64; 2],
}

impl Default for Trigger {
    fn default() -> Trigger {
        Trigger {
            last: [i64::MIN / 2; 2],
        }
    }
}

impl Trigger {
    /// The direction (+1 / −1) when `|r| ≥ x` and the same direction last fired at least
    /// `cool_ns` ago (the trigger is recorded); `None` otherwise.
    pub fn fire(&mut self, r: i64, x: i64, t: i64, cool_ns: i64) -> Option<i64> {
        if x <= 0 || r.abs() < x {
            return None;
        }
        let k = usize::from(r < 0);
        let last = self.last.get_mut(k)?;
        if t.saturating_sub(*last) < cool_ns {
            return None;
        }
        *last = t;
        Some(if r > 0 { 1 } else { -1 })
    }
}

/// The lead order of one slot leg: the place handle until its outcome, then its order ref.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LeadLeg {
    pub handle: Option<u16>,
    pub r: Option<OrderRef>,
    /// Engine time of the place.
    pub placed: i64,
    /// Trigger source that placed it (`lead_src` bit).
    pub src: u32,
    /// A Binance reference confirmed the move (`lead_cf_ms`).
    pub confirmed: bool,
}

/// Rule 18: taker-order times kept for the hourly limit (`take_max_hour` <= it).
pub const TAKE_RING: usize = 256;

/// Rule 15 state; not in the snapshot.
#[derive(Clone, Debug)]
pub struct LeadState {
    pub ring: PerpRing,
    /// The lead trigger and the perp-cancel trigger; both restart with each window.
    pub lead: Trigger,
    pub pc: Trigger,
    /// The fair-value (spot) reference's mids and its own lead trigger (`lead_src` bit 1).
    pub spot: PerpRing,
    pub lead_spot: Trigger,
    /// The second (Coinbase spot) reference's mids and its lead trigger (`lead_src` bit 2).
    pub cb: PerpRing,
    pub lead_cb: Trigger,
    /// The Coinbase perpetual's mids and its lead trigger (`lead_src` bit 3).
    pub cbp: PerpRing,
    pub lead_cbp: Trigger,
    /// Rule 17: engine time of the last fair-gap bid placed for the Up (index 0) and the Down
    /// (index 1) token of the window.
    pub fv_last: [i64; 2],
    /// Rule 18: taker orders sent for the Up (index 0) and the Down (index 1) token of the window,
    /// and the engine times of the last `TAKE_RING` taker orders (hourly limit).
    pub take_n: [u32; 2],
    pub take_t: Box<[i64; TAKE_RING]>,
    pub take_head: usize,
    /// Source of the trigger being placed.
    pub cur_src: u32,
    pub win: i64,
    /// Per slot leg (quote order).
    pub legs: Vec<LeadLeg>,
    /// Per slot leg: rule-14 entries are blocked until this engine time (perp cancel).
    pub block: Vec<i64>,
}

impl Default for LeadState {
    fn default() -> LeadState {
        LeadState {
            ring: PerpRing::default(),
            lead: Trigger::default(),
            pc: Trigger::default(),
            spot: PerpRing::default(),
            lead_spot: Trigger::default(),
            cb: PerpRing::default(),
            lead_cb: Trigger::default(),
            cbp: PerpRing::default(),
            lead_cbp: Trigger::default(),
            fv_last: [i64::MIN / 2; 2],
            take_n: [0; 2],
            take_t: Box::new([i64::MIN / 2; TAKE_RING]),
            take_head: 0,
            cur_src: 0,
            win: 0,
            legs: Vec::new(),
            block: Vec::new(),
        }
    }
}

impl LeadState {
    /// Rule 18: taker orders sent within the hour before `ens`.
    pub fn takes_last_hour(&self, ens: i64) -> u32 {
        self.take_t
            .iter()
            .filter(|t| ens.saturating_sub(**t) < 3_600_000_000_000)
            .count() as u32
    }

    /// Rule 18: a taker order sent at `ens` for direction `k` (0 = Up, 1 = Down).
    pub fn took(&mut self, k: usize, ens: i64) {
        if let Some(n) = self.take_n.get_mut(k) {
            *n += 1;
        }
        if let Some(t) = self.take_t.get_mut(self.take_head) {
            *t = ens;
        }
        self.take_head = (self.take_head + 1) % TAKE_RING;
    }

    /// Window `start`: the cool-downs restart (the research computes them per window).
    pub fn window(&mut self, start: i64) {
        if self.win != start {
            self.win = start;
            self.lead = Trigger::default();
            self.pc = Trigger::default();
            self.lead_spot = Trigger::default();
            self.lead_cb = Trigger::default();
            self.lead_cbp = Trigger::default();
            self.fv_last = [i64::MIN / 2; 2];
            self.take_n = [0; 2];
        }
    }
}

/// Rule 15 model price (raw price units; `cent` = one cent): the token mid plus the expected
/// repricing E = `k` × `delta` (cents per bp, 1e-9) × |move| (`move_abs`, 1e-9 of one) minus `m`
/// (cents, 1e-9), floored to a whole cent, then held within [bid + 1 cent, min(bid + `maxtick`
/// cents, ask − 1 cent)]: max(bid + 1, min(bid + maxtick, ⌊mid + E − m⌋, ask − 1)).
#[allow(clippy::too_many_arguments)]
pub fn model_px(
    bid: i64,
    ask: i64,
    cent: i64,
    delta: i64,
    move_abs: i64,
    k: i64,
    m: i64,
    maxtick: i64,
) -> i64 {
    let one = 1_000_000_000i128;
    let cent = i128::from(cent.max(1));
    // E in raw price units: k · delta · (move in bp) · cent; move bp = move_abs / 1e5.
    let e = i128::from(k) * i128::from(delta) / one * i128::from(move_abs) / 100_000 * cent / one;
    let m_raw = i128::from(m) * cent / one;
    // ⌊mid + E − m⌋ to a whole cent, mid = (bid + ask) / 2 (doubled to stay exact).
    let cap2 = i128::from(bid) + i128::from(ask) + 2 * e - 2 * m_raw;
    let cap = cap2.div_euclid(2 * cent) * cent;
    let lo = i128::from(bid) + cent;
    let hi = (i128::from(bid) + i128::from(maxtick) * cent)
        .min(cap)
        .min(i128::from(ask) - cent);
    lo.max(hi)
        .clamp(i128::from(i64::MIN / 4), i128::from(i64::MAX / 4)) as i64
}
