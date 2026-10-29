//! Feature set of `binary-maker` (README "特征"): the hft-feature nodes the rules read, composed
//! with a fixed per-event-kind plan, and the strategy's own composites (the eight score features of
//! rule 10, the direction forecast of rule 9). Everything is integer (`Rate` = 1e-9); the exact
//! arithmetic is the README's, so an offline port can reproduce it bit for bit.
//!
//! Plan (handlers in this order, every event while `Now.clock_ok`; README "特征" 执行计划):
//! reference `Bbo` → `ref_x2` → slot histories → lag statistics → `top` → `vol`;
//! `Trade` → `trades` → slot histories → lag statistics; any other event → slot histories → lag
//! statistics. `ref_x2`, `trades` and the slot histories run only with the score on, the lag
//! statistics only with a lag gate on; `top` runs on every two-sided reference quote and `vol` on
//! those with `clock_ok`.

use hft_feature::{
    BboTop, ColumnDesc, FeatureRow, LagQuantiles, ONE, OwnExcludedSide, RealizedVol, SignedFlow, Unit,
    imbalance, ln_rate, ln_ratio, zsum,
};
use hft_strategy_sdk::prelude::*;

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
/// √5 × 1e6 (forecast return scale, README "规则" 9).
pub const SQRT5_MICRO: i128 = 2_236_068;
/// Book levels read from the touch for the imbalance features (5 cents of a 0.001 tick and the
/// levels above the cent the best bid is in).
pub const DEPTH_LEVELS: usize = 64;

const ONE128: i128 = ONE as i128;

/// One value per UTC second over `HIST` seconds.
pub type SecHist = hft_feature::SecHist<HIST>;
/// Large prints of the slot insts.
pub type Trades = SignedFlow<TRADES>;
/// Bid side of one token as the score sees it: the book's bid levels less our own resting bid.
pub type BidSide = OwnExcludedSide<DEPTH_LEVELS>;

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
    let imb1 = imbalance(s.x.top_size(), s.o.top_size());
    let imb5 = imbalance(s.x.depth(cent, 5), s.o.depth(cent, 5));
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
    zsum(f, mu, sd, &SIGN, SQRT8_NANO)
}

/// What the feature handlers need of the parameters (README "特征" 执行计划).
#[derive(Copy, Clone, Debug)]
pub struct PlanCfg {
    /// The composite score is on (`comp_skip` or `comp_dir` set).
    pub comp: bool,
    /// A lag gate is on (`load_gate_ns` or `mdslow_gate_ns` set).
    pub lag: bool,
    /// `comp_flow_min_shares`.
    pub flow_min_shares: u32,
}

/// Exported columns of [`MakerFeatures::export`].
pub const COLUMNS: &[ColumnDesc] = &[
    ColumnDesc {
        name: "ref_mid_x2",
        unit: Unit::Px,
        exp: 0,
    },
    ColumnDesc {
        name: "ref_imb",
        unit: Unit::Ratio,
        exp: -9,
    },
    ColumnDesc {
        name: "ref_sigma",
        unit: Unit::Ratio,
        exp: -9,
    },
    ColumnDesc {
        name: "ref_samples",
        unit: Unit::Count,
        exp: 0,
    },
    ColumnDesc {
        name: "lag_minute",
        unit: Unit::Minute,
        exp: 0,
    },
    ColumnDesc {
        name: "lag_p90",
        unit: Unit::Ns,
        exp: -2,
    },
    ColumnDesc {
        name: "lag_p50",
        unit: Unit::Ns,
        exp: -2,
    },
];

/// The feature state of one instance (not in the snapshot: rebuilt from events after a restart).
#[derive(Debug)]
pub struct MakerFeatures {
    /// Realized volatility of the reference mid.
    pub vol: RealizedVol,
    /// Reference 2 × mid and level-1 imbalance of the last two-sided quote.
    pub top: BboTop,
    /// Per slot leg (quote order).
    pub hist: Vec<SlotHist>,
    /// Reference 2 × mid per second (score).
    pub ref_x2: Box<SecHist>,
    /// Large prints of the slot legs (score).
    pub trades: Box<Trades>,
    /// Feed-lag statistics (venue-load gates).
    pub lag: LagQuantiles,
}

impl MakerFeatures {
    /// Preallocates for `slots` slot legs and a `vol_cap`-second volatility ring.
    pub fn new(vol_cap: usize, slots: usize) -> MakerFeatures {
        MakerFeatures {
            vol: RealizedVol::new(vol_cap),
            top: BboTop::default(),
            hist: Vec::with_capacity(slots),
            ref_x2: Box::default(),
            trades: Box::default(),
            lag: LagQuantiles::new(),
        }
    }

    /// New parameters (command path): the volatility ring restarts when its length changes; the
    /// slot histories follow the slot insts (rebuilt when they change).
    pub fn on_params(&mut self, vol_cap: usize, slots: impl Iterator<Item = InstId> + Clone) {
        if vol_cap != self.vol.capacity() {
            self.vol = RealizedVol::new(vol_cap);
        }
        if !self.hist.iter().map(|h| h.inst).eq(slots.clone()) {
            self.hist.clear();
            self.hist.extend(slots.map(SlotHist::new));
        }
    }

    /// The plan of one event (module docs), before the rules run.
    pub fn on_event<V: Views>(&mut self, ev: &EventRef<'_>, v: &V, reference: InstId, cfg: PlanCfg) {
        let now = v.now();
        if now.clock_ok {
            self.observe(ev, v, reference, cfg, now.utc_ns);
        }
        if let EventRef::Bbo { inst, b, .. } = *ev
            && inst == reference
            && self
                .top
                .on_quote(b.bid_px.0, b.bid_qty.0, b.ask_px.0, b.ask_qty.0)
            && now.clock_ok
        {
            self.vol
                .on_mid(i128::from(self.top.mid_x2), now.utc_ns.div_euclid(1_000_000_000));
        }
    }

    /// Score and lag handlers (README "规则" 10, 11): the reference 2 × mid of the second, large
    /// prints, every slot book's 2 × mid and best bid of the second; the slot insts' new
    /// market-data stamps into the lag statistics.
    fn observe<V: Views>(&mut self, ev: &EventRef<'_>, v: &V, reference: InstId, cfg: PlanCfg, utc_ns: i64) {
        let sec = utc_ns.div_euclid(1_000_000_000);
        if cfg.comp {
            match *ev {
                EventRef::Bbo { inst, b, .. } if inst == reference => {
                    if b.bid_px.0 > 0 && b.ask_px.0 > 0 {
                        self.ref_x2.put(sec, b.bid_px.0.saturating_add(b.ask_px.0));
                    }
                }
                EventRef::Trade { inst, t, .. } => {
                    let unit_qty = 10i64.saturating_pow(u32::from(v.inst(inst).qty_exp.unsigned_abs()));
                    let min = i64::from(cfg.flow_min_shares).saturating_mul(unit_qty);
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
        if cfg.lag {
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

    /// Direction forecast in standard deviations of the next 30 s (`Rate`, README "规则" 9):
    /// `w_imb` × level-1 imbalance + `w_ret` × (sum of the last 5 one-second returns) / (σ √5) +
    /// `w_ser` × (sum of the last 15) / (sum of their absolute values); clamped to ±6.
    pub fn forecast(&self, sigma: i64, w_imb: i64, w_ret: i64, w_ser: i64) -> i64 {
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
        let z = (i128::from(w_imb) * i128::from(self.top.imb)
            + i128::from(w_ret) * ret
            + i128::from(w_ser) * ser)
            / one;
        z.clamp(-6 * one, 6 * one) as i64
    }

    /// Writes [`COLUMNS`] (reference quote, volatility with at least one sample, last complete
    /// lag minute).
    pub fn export(&self, row: &mut FeatureRow<'_>) {
        use hft_feature::Feature;
        self.top.export(row);
        self.vol.export(row);
        self.lag.export(row);
    }
}
