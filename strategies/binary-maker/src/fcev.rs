//! Rule 14: the fill-conditional EV of a maker bid (README "规则" 14; research Spec
//! "成交条件期望_数学模型", Python reference `sim2.py` `Window.features` / `score_arr`). All
//! quantities are 1e-9 fixed point: basis points, cents, √seconds and probabilities alike.
//!
//! score = fair − p − λ·Δ − (c0 + c1·|p − 50|/50), fair = w·100·Φ(d) + (1 − w)·mid,
//! d = dist / (max(s·σ, 0.03)·sq), Δ = 100·φ(d) / (max(s·σ, 0.03)·sq)·remf.

use hft_feature::{ONE, cdf, inv_cdf, isqrt, ln_ratio};

/// φ(k / 64) × 1e9, k = 0..=512 (z up to 8).
#[rustfmt::skip]
const PDF: [i64; 513] = [
    398942280, 398893584, 398747532, 398504230, 398163857, 397726661, 397192963, 396563151,
    395837687, 395017098, 394101982, 393093005, 391990898, 390796460, 389510555, 388134110,
    386668117, 385113627, 383471754, 381743670, 379930606, 378033849, 376054739, 373994672,
    371855094, 369637502, 367343440, 364974500, 362532317, 360018570, 357434978, 354783298,
    352065327, 349282893, 346437859, 343532119, 340567594, 337546233, 334470008, 331340915,
    328160969, 324932202, 321656664, 318336417, 314973535, 311570103, 308128211, 304649954,
    301137432, 297592745, 294017993, 290415270, 286786668, 283134270, 279460152, 275766378,
    272054998, 268328050, 264587554, 260835512, 257073907, 253304700, 249529829, 245751208,
    241970725, 238190238, 234411581, 230636554, 226866927, 223104437, 219350787, 215607646,
    211876646, 208159383, 204457415, 200772261, 197105402, 193458276, 189832284, 186228781,
    182649085, 179094469, 175566162, 172065353, 168593185, 165150757, 161739128, 158359308,
    155012265, 151698925, 148420167, 145176826, 141969695, 138799523, 135667015, 132572833,
    129517596, 126501881, 123526223, 120591116, 117697011, 114844322, 112033419, 109264637,
    106538268, 103854570, 101213760, 98616021, 96061501, 93550310, 91082525, 88658191,
    86277319, 83939888, 81645848, 79395117, 77187584, 75023113, 72901536, 70822663,
    68786276, 66792133, 64839969, 62929496, 61060405, 59232366, 57445029, 55698024,
    53990967, 52323452, 50695060, 49105356, 47553891, 46040203, 44563815, 43124242,
    41720985, 40353537, 39021381, 37723990, 36460833, 35231369, 34035053, 32871333,
    31739652, 30639451, 29570166, 28531232, 27522081, 26542142, 25590846, 24667622,
    23771901, 22903112, 22060688, 21244063, 20452674, 19685958, 18943360, 18224324,
    17528300, 16854744, 16203114, 15572875, 14963496, 14374452, 13805226, 13255304,
    12724182, 12211359, 11716343, 11238650, 10777802, 10333328, 9904765, 9491659,
    9093563, 8710036, 8340648, 7984977, 7642606, 7313129, 6996148, 6691272,
    6398120, 6116318, 5845500, 5585310, 5335399, 5095425, 4865057, 4643970,
    4431848, 4228383, 4033274, 3846229, 3666962, 3495198, 3330666, 3173104,
    3022258, 2877880, 2739731, 2607576, 2481191, 2360355, 2244855, 2134486,
    2029048, 1928348, 1832197, 1740416, 1652829, 1569267, 1489566, 1413567,
    1341119, 1272073, 1206287, 1143624, 1083952, 1027142, 973073, 921624,
    872683, 826139, 781886, 739823, 699852, 661879, 625814, 591569,
    559062, 528212, 498942, 471179, 444853, 419895, 396240, 373827,
    352596, 332489, 313452, 295433, 278382, 262251, 246994, 232568,
    218932, 206044, 193868, 182367, 171506, 161253, 151575, 142444,
    133830, 125707, 118047, 110827, 104024, 97614, 91577, 85892,
    80540, 75504, 70765, 66307, 62115, 58174, 54470, 50989,
    47719, 44647, 41763, 39056, 36516, 34132, 31896, 29800,
    27834, 25992, 24265, 22648, 21134, 19716, 18388, 17146,
    15984, 14897, 13880, 12930, 12042, 11212, 10436, 9712,
    9036, 8405, 7817, 7267, 6755, 6277, 5831, 5416,
    5030, 4669, 4334, 4021, 3730, 3460, 3208, 2974,
    2756, 2554, 2365, 2191, 2028, 1877, 1737, 1607,
    1487, 1375, 1271, 1175, 1086, 1003, 926, 855,
    790, 729, 672, 620, 572, 527, 486, 448,
    413, 380, 350, 322, 297, 273, 251, 231,
    213, 195, 180, 165, 152, 139, 128, 117,
    108, 99, 91, 83, 76, 70, 64, 59,
    54, 49, 45, 41, 38, 35, 32, 29,
    26, 24, 22, 20, 18, 17, 15, 14,
    13, 12, 11, 10, 9, 8, 7, 7,
    6, 6, 5, 5, 4, 4, 3, 3,
    3, 3, 2, 2, 2, 2, 2, 1,
    1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0,
    0,
];

/// Table step of `PDF`: z = k / 64.
const PDF_STEPS: i64 = 64;

/// Standard normal density φ(z) × 1e9 for z × 1e9 (linear interpolation; 0 beyond |z| = 8).
pub fn pdf(z: i64) -> i64 {
    let pos = i128::from(z).abs() * i128::from(PDF_STEPS);
    let k = pos / i128::from(ONE);
    let frac = pos % i128::from(ONE);
    let (Some(&lo), Some(&hi)) = (
        usize::try_from(k).ok().and_then(|k| PDF.get(k)),
        usize::try_from(k + 1).ok().and_then(|k| PDF.get(k)),
    ) else {
        return 0;
    };
    (i128::from(lo) + (i128::from(hi) - i128::from(lo)) * frac / i128::from(ONE)) as i64
}

/// Floor of s·σ (basis points per √s): 0.03.
pub const SIG_FLOOR: i64 = 30_000_000;
/// |d| clip: 8.
pub const D_CLIP: i64 = 8 * ONE;
/// Implied-σ samples are clipped to [0.03, 5] bp/√s.
pub const IV_MIN: i64 = 30_000_000;
pub const IV_MAX: i64 = 5 * ONE;
/// Fallback σ without implied samples or a realized σ: 0.3 bp/√s.
pub const SIG_DEFAULT: i64 = 300_000_000;
/// Implied-σ samples before their median is used.
pub const IV_MIN_N: usize = 5;
/// Implied-σ ring capacity (`fc_iv_n` ≤ it).
pub const IV_CAP: usize = 600;
/// Seconds of the settlement average.
pub const AVG_S: i64 = 60;

/// Model parameters (fc_s, fc_lam, fc_c0, fc_c1, fc_w; 1e-9).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Model {
    pub s: i64,
    pub lam: i64,
    pub c0: i64,
    pub c1: i64,
    pub w: i64,
}

/// Score (cents, 1e-9) of a bid at `px` (cents) on a token with distance `dist` (bp, the token's
/// orientation), time factors `sq` (√s) and `remf`, σ `sig` (bp/√s) and token mid `mid` (cents).
pub fn score(m: &Model, dist: i64, sq: i64, remf: i64, sig: i64, mid: i64, px: i64) -> i64 {
    let one = i128::from(ONE);
    let s_eff = (i128::from(m.s) * i128::from(sig) / one).max(i128::from(SIG_FLOOR));
    let den = (s_eff * i128::from(sq) / one).max(1);
    let d = (i128::from(dist) * one / den).clamp(-i128::from(D_CLIP), i128::from(D_CLIP)) as i64;
    let delta = 100 * i128::from(pdf(d)) * one / den * i128::from(remf) / one;
    let fair =
        i128::from(m.w) * 100 * i128::from(cdf(d)) / one + (one - i128::from(m.w)) * i128::from(mid) / one;
    let cost = i128::from(m.c0) + i128::from(m.c1) * (i128::from(px) - 50 * one).abs() / (50 * one);
    let s = fair - i128::from(px) - i128::from(m.lam) * delta / one - cost;
    s.clamp(i128::from(i64::MIN / 4), i128::from(i64::MAX / 4)) as i64
}

/// Δ (cents per bp, 1e-9) of the model probability at distance `dist` (bp, the token's
/// orientation) without the tail factor: 100 · φ(d) / (max(σ, 0.03) · sq) · remf,
/// d = clip(dist / (max(σ, 0.03) · sq), ±8) (rule 15's model price).
pub fn delta(dist: i64, sq: i64, remf: i64, sig: i64) -> i64 {
    let one = i128::from(ONE);
    let den = (i128::from(sig).max(i128::from(SIG_FLOOR)) * i128::from(sq) / one).max(1);
    let d = (i128::from(dist) * one / den).clamp(-i128::from(D_CLIP), i128::from(D_CLIP)) as i64;
    let x = 100 * i128::from(pdf(d)) * one / den * i128::from(remf) / one;
    x.clamp(0, i128::from(i64::MAX / 4)) as i64
}

/// Model probability Φ(d) (1e-9) of a token at distance `dist` (bp, the token's orientation):
/// d = clip(dist / (max(σ, 0.03) · sq), ±8) (rule 17's fair value).
pub fn fair(dist: i64, sq: i64, sig: i64) -> i64 {
    let one = i128::from(ONE);
    let den = (i128::from(sig).max(i128::from(SIG_FLOOR)) * i128::from(sq) / one).max(1);
    let d = (i128::from(dist) * one / den).clamp(-i128::from(D_CLIP), i128::from(D_CLIP)) as i64;
    cdf(d)
}

/// Time factors `(sq, remf)` of the final average at `left_ms` before the window end, with
/// `n_real` realized Chainlink samples of the last 60 s. τ ≥ 60 s: sq = √(τ − 40), remf = 1.
/// τ < 60 s: rem = 60 − n_real, sq = max(rem/60 · √(max(rem, 1)/3), 0.001), remf = rem/60.
pub fn time_factors(left_ms: i64, n_real: i64) -> (i64, i64) {
    if left_ms >= AVG_S * 1_000 {
        let x = u128::try_from(left_ms - 40_000).unwrap_or(0) * 1_000_000_000_000_000;
        return (isqrt(x) as i64, ONE);
    }
    let rem = (AVG_S - n_real).clamp(0, AVG_S);
    let root = isqrt(u128::try_from(rem.max(1)).unwrap_or(1) * 1_000_000_000_000_000_000 / 3) as i128;
    let sq = (i128::from(rem) * root / i128::from(AVG_S)).max(1_000_000) as i64;
    (sq, rem * ONE / AVG_S)
}

/// Distance of the final average from the strike `k` in bp (1e-9), Up orientation:
/// ln(μ / K) × 1e4, μ = `s_now` (τ ≥ 60 s) or (Σ realized + rem · `s_now`) / 60 inside the last
/// 60 s (`real_sum`, `n_real` = the realized samples; all prices 1e-8 USD).
pub fn dist_up(k: i64, s_now: i64, left_ms: i64, real_sum: i128, n_real: i64) -> Option<i64> {
    if k <= 0 || s_now <= 0 {
        return None;
    }
    let mu = if left_ms >= AVG_S * 1_000 || n_real <= 0 {
        i128::from(s_now)
    } else {
        let rem = (AVG_S - n_real).max(0);
        (real_sum + i128::from(rem) * i128::from(s_now)) / i128::from(AVG_S)
    };
    let d = ln_ratio(mu, i128::from(k)) * 10_000;
    Some(d.clamp(i128::from(i64::MIN / 4), i128::from(i64::MAX / 4)) as i64)
}

/// The implied σ (bp/√s, 1e-9) of an Up mid `mid` (probability 1e-9) at distance `dist` and time
/// factor `sq`: dist / (Φ⁻¹(mid) · sq) clipped to [0.03, 5], when |Φ⁻¹| > 0.25, |dist| > 0.5 bp
/// and both have the same sign (mid clipped to [0.02, 0.98] first).
pub fn implied_sigma(mid: i64, dist: i64, sq: i64) -> Option<i64> {
    let z = inv_cdf(mid.clamp(ONE / 50, ONE - ONE / 50));
    if z.abs() <= ONE / 4 || dist.abs() <= ONE / 2 || (z > 0) != (dist > 0) || sq <= 0 {
        return None;
    }
    let one = i128::from(ONE);
    let iv = i128::from(dist) * one * one / (i128::from(z) * i128::from(sq));
    Some(iv.clamp(i128::from(IV_MIN), i128::from(IV_MAX)) as i64)
}

/// Per-window model state: the implied-σ ring of the current window (at most one sample per UTC
/// second) and the reference mid at the end of each recent second (the spot price at a Chainlink
/// sample's time). Not in the snapshot.
#[derive(Clone, Debug)]
pub struct FcState {
    pub win: i64,
    iv: Box<[i64; IV_CAP]>,
    iv_n: usize,
    iv_head: usize,
    pub iv_sec: i64,
    /// Median of the ring, recomputed when a sample is added (decisions read it every event).
    iv_med: Option<i64>,
    /// Reference 2 × mid of the last quote of UTC second `spot_sec[k]`, slot k = second mod 512.
    spot_sec: Box<[i64; 512]>,
    spot_val: Box<[i64; 512]>,
}

impl Default for FcState {
    fn default() -> FcState {
        FcState {
            win: i64::MIN,
            iv: Box::new([0; IV_CAP]),
            iv_n: 0,
            iv_head: 0,
            iv_sec: i64::MIN,
            iv_med: None,
            spot_sec: Box::new([i64::MIN; 512]),
            spot_val: Box::new([0; 512]),
        }
    }
}

impl FcState {
    /// A reference quote (2 × mid, raw) at UTC second `sec`: the last of each second is kept.
    pub fn on_spot(&mut self, sec: i64, mid_x2: i64) {
        let k = sec.rem_euclid(512) as usize;
        if let (Some(s), Some(v)) = (self.spot_sec.get_mut(k), self.spot_val.get_mut(k)) {
            *s = sec;
            *v = mid_x2;
        }
    }
    /// The reference 2 × mid at the start of UTC second `sec`: the last quote of the latest
    /// earlier second, looking back at most 5 s.
    pub fn spot_before(&self, sec: i64) -> Option<i64> {
        (1..=5).find_map(|b| {
            let s = sec - b;
            let k = s.rem_euclid(512) as usize;
            (self.spot_sec.get(k) == Some(&s))
                .then(|| self.spot_val.get(k).copied())
                .flatten()
        })
    }
    /// Window `start`: a new window restarts the implied-σ ring.
    pub fn window(&mut self, start: i64) {
        if self.win != start {
            self.win = start;
            self.iv_n = 0;
            self.iv_head = 0;
            self.iv_sec = i64::MIN;
            self.iv_med = None;
        }
    }
    /// Whether second `sec` still takes an implied-σ sample (one per second).
    pub fn iv_due(&self, sec: i64) -> bool {
        sec != self.iv_sec
    }
    /// The implied-σ sample of second `sec` (`None`: the mid gave none); `cap` = the ring length
    /// (`fc_iv_n`, at most `IV_CAP`).
    pub fn push_iv(&mut self, sec: i64, iv: Option<i64>, cap: usize) {
        if sec == self.iv_sec {
            return;
        }
        self.iv_sec = sec;
        let Some(iv) = iv else { return };
        let cap = cap.clamp(1, IV_CAP);
        if let Some(x) = self.iv.get_mut(self.iv_head % cap) {
            *x = iv;
        }
        self.iv_head = (self.iv_head + 1) % cap;
        self.iv_n = (self.iv_n + 1).min(cap);
        self.iv_med = self.median();
    }
    /// Median of the implied-σ ring (mean of the two middle values for an even count); `None`
    /// below `IV_MIN_N` samples.
    pub fn iv_median(&self) -> Option<i64> {
        self.iv_med
    }

    fn median(&self) -> Option<i64> {
        if self.iv_n < IV_MIN_N {
            return None;
        }
        let mut buf = [0i64; IV_CAP];
        let v = buf.get_mut(..self.iv_n)?;
        v.copy_from_slice(self.iv.get(..self.iv_n)?);
        v.sort_unstable();
        let h = self.iv_n / 2;
        if self.iv_n % 2 == 1 {
            v.get(h).copied()
        } else {
            Some(((i128::from(*v.get(h - 1)?) + i128::from(*v.get(h)?)) / 2) as i64)
        }
    }
    pub fn iv_len(&self) -> usize {
        self.iv_n
    }
}
