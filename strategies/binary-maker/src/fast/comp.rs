//! Composite direction score of a slot token (README "规则" 10): eight features of the token X
//! and its complement O, normalized by the `comp_*_mu`/`comp_*_sd` parameters and summed,
//! z = Σ sign·(x − mu)/sd / √8. Everything is integer (`Rate` = 1e-9); the exact arithmetic is
//! the README's, so an offline port can reproduce the score bit for bit.

use hft_strategy_sdk::prelude::*;

use crate::vol::ONE;

/// Seconds kept per history (a power of two; the longest look-back is 62 s).
pub const HIST: usize = 128;
/// Large prints kept for the flow feature (the oldest is overwritten).
pub const TRADES: usize = 256;
/// Feature order (README "规则" 10): imb1, imb5, mom30, mom60, bret15, bret60, flow30, bdrop1.
pub const N_FEAT: usize = 8;
/// Sign of each feature in the score (the flow feature is contrarian).
pub const SIGN: [i128; N_FEAT] = [1, 1, 1, 1, 1, 1, -1, 1];
/// √8 × 1e9.
pub const SQRT8_NANO: i128 = 2_828_427_125;
/// √15 and √60 × 1e6.
pub const SQRT15_MICRO: i128 = 3_872_983;
pub const SQRT60_MICRO: i128 = 7_745_967;
/// ln 2 × 1e9.
pub const LN2_NANO: i128 = 693_147_181;
/// Book levels read from the touch for the imbalance features (5 cents of a 0.001 tick and the
/// levels above the cent the best bid is in).
pub const DEPTH_LEVELS: usize = 64;

const ONE128: i128 = ONE as i128;

/// One value per UTC second, the last write of the second winning (ring of `HIST` seconds).
#[derive(Clone, Debug)]
pub struct SecHist {
    sec: [i64; HIST],
    val: [i64; HIST],
}

impl Default for SecHist {
    fn default() -> SecHist {
        SecHist {
            sec: [i64::MIN; HIST],
            val: [0; HIST],
        }
    }
}

impl SecHist {
    fn slot(sec: i64) -> usize {
        (sec.rem_euclid(HIST as i64)) as usize
    }
    pub fn put(&mut self, sec: i64, v: i64) {
        let k = SecHist::slot(sec);
        if let (Some(s), Some(x)) = (self.sec.get_mut(k), self.val.get_mut(k)) {
            *s = sec;
            *x = v;
        }
    }
    /// The value of second `t`, else of `t − 1`, else of `t − 2`.
    pub fn at(&self, t: i64) -> Option<i64> {
        (0..3).find_map(|back| {
            let s = t.saturating_sub(back);
            let k = SecHist::slot(s);
            (self.sec.get(k) == Some(&s))
                .then(|| self.val.get(k).copied())
                .flatten()
        })
    }
}

/// Per slot leg (same order as `Fast::quotes`): its inst, 2 × mid and best bid per second, and the
/// last market-data stamp read for the lag gates.
#[derive(Clone, Debug)]
pub struct SlotHist {
    pub inst: InstId,
    pub mid_x2: SecHist,
    pub bid: SecHist,
    /// `Views::md_count` already consumed (README "规则" 11).
    pub md_seen: u64,
}

impl SlotHist {
    pub fn new(inst: InstId) -> SlotHist {
        SlotHist {
            inst,
            mid_x2: SecHist::default(),
            bid: SecHist::default(),
            md_seen: 0,
        }
    }
}

/// Large prints (≥ `comp_flow_min_shares`) of the slot insts: UTC time, inst, quantity signed by the
/// taker side (+ buy).
#[derive(Clone, Debug)]
pub struct Trades {
    t: [i64; TRADES],
    inst: [InstId; TRADES],
    q: [i64; TRADES],
    n: usize,
}

impl Default for Trades {
    fn default() -> Trades {
        Trades {
            t: [i64::MIN; TRADES],
            inst: [INST_NONE; TRADES],
            q: [0; TRADES],
            n: 0,
        }
    }
}

impl Trades {
    pub fn push(&mut self, utc_ns: i64, inst: InstId, signed_qty: i64) {
        let k = self.n % TRADES;
        if let (Some(t), Some(i), Some(q)) = (self.t.get_mut(k), self.inst.get_mut(k), self.q.get_mut(k)) {
            *t = utc_ns;
            *i = inst;
            *q = signed_qty;
        }
        self.n = self.n.wrapping_add(1);
    }
    /// Net taker flow toward `x` over prints at or after `from_ns`: buys of `x` and sells of `o`
    /// count +, sells of `x` and buys of `o` count − (raw quantity).
    pub fn net(&self, x: InstId, o: InstId, from_ns: i64) -> i128 {
        let mut v = 0i128;
        for ((t, i), q) in self.t.iter().zip(self.inst.iter()).zip(self.q.iter()) {
            if *t < from_ns {
                continue;
            }
            if *i == x {
                v += i128::from(*q);
            } else if *i == o {
                v -= i128::from(*q);
            }
        }
        v
    }
}

/// 2·atanh(u / 1e9) × 1e9 = ln((1 + u)/(1 − u)) for |u| < 1e9: 2·Σ_{j=0}^{11} u^(2j+1) / (2j+1),
/// with u² = ⌊u·u / 1e9⌋, every power ⌊power·u² / 1e9⌋ and every term ⌊power / (2j+1)⌋
/// (truncation toward zero).
pub fn atanh2(u: i128) -> i128 {
    let u2 = u * u / ONE128;
    let mut p = u;
    let mut sum = 0i128;
    for j in 0..12i128 {
        sum += p / (2 * j + 1);
        p = p * u2 / ONE128;
    }
    2 * sum
}

/// ln(a / b) × 1e9 for a, b > 0: `atanh2(⌊(a − b)·1e9 / (a + b)⌋)`; 0 otherwise.
pub fn ln_ratio(a: i128, b: i128) -> i128 {
    if a <= 0 || b <= 0 {
        return 0;
    }
    atanh2((a - b) * ONE128 / (a + b))
}

/// ln(x / 1e9) × 1e9 for x ≥ 1e9: halve x (⌊x / 2⌋) k times until x < 2e9, then
/// k·`LN2_NANO` + `ln_ratio(x, 1e9)`; 0 for x < 1e9.
pub fn ln_rate(x: i128) -> i128 {
    if x < ONE128 {
        return 0;
    }
    let mut m = x;
    let mut k = 0i128;
    while m >= 2 * ONE128 {
        m /= 2;
        k += 1;
    }
    k * LN2_NANO + ln_ratio(m, ONE128)
}

/// Our resting bid on a token: price and remaining raw quantity.
pub type Own = Option<(i64, i64)>;

/// Bid side of one token as the score sees it: the book's bid levels from the touch less our own
/// resting bid.
#[derive(Debug)]
pub struct BidSide {
    lv: [Level; DEPTH_LEVELS],
    n: usize,
    own: Own,
}

impl BidSide {
    pub fn read<B: BookRead>(b: &B, own: Own) -> BidSide {
        let mut lv = [Level {
            px: Px(0),
            qty: Qty(0),
        }; DEPTH_LEVELS];
        let n = b.levels(Side::Buy, &mut lv);
        BidSide { lv, n, own }
    }
    /// Bid size at `px` less our own bid there, at least 0 (raw quantity).
    pub fn size_at(&self, px: i64) -> i64 {
        let q: i64 = self
            .lv
            .iter()
            .take(self.n)
            .filter(|l| l.px.0 == px)
            .map(|l| l.qty.0)
            .sum();
        let mine = self.own.filter(|(p, _)| *p == px).map_or(0, |(_, r)| r);
        q.saturating_sub(mine).max(0)
    }
    /// The highest bid with size left after our own bid is taken out.
    pub fn best(&self) -> Option<i64> {
        self.lv
            .iter()
            .take(self.n)
            .map(|l| l.px.0)
            .find(|px| self.size_at(*px) > 0)
    }
    /// Size left at the best bid (0 without one).
    pub fn top_size(&self) -> i64 {
        self.best().map_or(0, |px| self.size_at(px))
    }
    /// Size left at the 5 one-cent prices from the cent the best bid is in downwards.
    pub fn depth5(&self, cent: i64) -> i64 {
        let Some(bb) = self.best() else { return 0 };
        let top = bb.div_euclid(cent) * cent;
        (0..5).map(|k| self.size_at(top - k * cent)).sum()
    }
}

/// (a − b)·1e9 / (a + b), 0 when a + b ≤ 0.
fn imb(a: i64, b: i64) -> i64 {
    let (a, b) = (i128::from(a), i128::from(b));
    if a + b <= 0 {
        return 0;
    }
    ((a - b) * ONE128 / (a + b)) as i64
}

/// Everything one score needs (README "规则" 10).
#[derive(Debug)]
pub struct ScoreIn<'a> {
    /// Bid sides of X and O.
    pub x: &'a BidSide,
    pub o: &'a BidSide,
    pub hx: &'a SlotHist,
    pub ref_x2: &'a SecHist,
    pub trades: &'a Trades,
    pub x_inst: InstId,
    pub o_inst: InstId,
    /// X is the Up token.
    pub is_up: bool,
    pub utc_ns: i64,
    /// Implied sigma per √s (reference sigma × `sigma_scale`, `Rate`); `None` without sigma.
    pub implied: Option<i64>,
    pub unit_px: i64,
    pub unit_qty: i64,
}

/// The eight features (`Rate`; `None` = missing, contributes 0).
pub fn features(s: &ScoreIn<'_>) -> [Option<i64>; N_FEAT] {
    let sec = s.utc_ns.div_euclid(1_000_000_000);
    let cent = (s.unit_px / 100).max(1);
    let imb1 = imb(s.x.top_size(), s.o.top_size());
    let imb5 = imb(s.x.depth5(cent), s.o.depth5(cent));
    let m0 = s.hx.mid_x2.at(sec);
    let mom = |h: i64| {
        let m1 = s.hx.mid_x2.at(sec - h)?;
        let d = i128::from(m0?) - i128::from(m1);
        Some((d * ONE128 / (2 * i128::from(s.unit_px.max(1)))) as i64)
    };
    let r0 = s.ref_x2.at(sec);
    let sgn: i128 = if s.is_up { 1 } else { -1 };
    let bret = |h: i64, root_micro: i128| {
        let (r0, r1) = (i128::from(r0?), i128::from(s.ref_x2.at(sec - h)?));
        let implied = i128::from(s.implied.filter(|x| *x > 0)?);
        if r0 <= 0 || r1 <= 0 {
            return None;
        }
        Some((sgn * ln_ratio(r0, r1) * 1_000_000 * ONE128 / (implied * root_micro)) as i64)
    };
    let v = s.trades.net(s.x_inst, s.o_inst, s.utc_ns - 30_000_000_000);
    let mag = ln_rate(ONE128 + v.abs() * ONE128 / i128::from(s.unit_qty.max(1)));
    let flow = (if v < 0 { -mag } else { mag }) as i64;
    let bdrop = match (s.hx.bid.at(sec), s.hx.bid.at(sec - 1)) {
        (Some(b0), Some(b1)) => ((i128::from(b0) - i128::from(b1)) * ONE128 / i128::from(cent)) as i64,
        _ => 0,
    };
    [
        Some(imb1),
        Some(imb5),
        mom(30),
        mom(60),
        bret(15, SQRT15_MICRO),
        bret(60, SQRT60_MICRO),
        Some(flow),
        Some(bdrop),
    ]
}

/// z = ⌊Σ_i ⌊sign_i·(x_i − mu_i)·1e9 / sd_i⌋ · 1e9 / `SQRT8_NANO`⌋ (`Rate`); a missing feature
/// adds 0.
pub fn score(f: &[Option<i64>; N_FEAT], mu: &[i64; N_FEAT], sd: &[i64; N_FEAT]) -> i64 {
    let mut sum = 0i128;
    for k in 0..N_FEAT {
        let (Some(Some(x)), Some(m), Some(d), Some(sg)) = (f.get(k), mu.get(k), sd.get(k), SIGN.get(k))
        else {
            continue;
        };
        if *d <= 0 {
            continue;
        }
        sum += sg * (i128::from(*x) - i128::from(*m)) * ONE128 / i128::from(*d);
    }
    (sum * ONE128 / SQRT8_NANO).clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}
