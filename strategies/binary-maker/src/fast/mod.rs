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

use hft_feature::{ONE, Own, cdf, inv_cdf, isqrt};
use hft_strategy_sdk::prelude::*;

use crate::features::{self, BidSide, MakerFeatures, PlanCfg, ScoreIn};
use crate::params::BinaryMakerParams;

/// The settlement averages the reference over the last 60 s of the window: the variance left
/// `t` seconds before the end (t ≥ 60 s) is that of `t − 40` seconds of the spot.
pub const TWAP_TAIL_MS: i64 = 40_000;

/// Order tag of the quote of slot group `gix` (`gix + 1`).
pub fn tag_of(gix: u16) -> u32 {
    u32::from(gix) + 1
}

/// Tag bit of a rule-12 (settlement-window) bid.
pub const SETTLE_TAG: u32 = 0x0001_0000;

/// Order tag of a rule-12 bid of slot group `gix` (`SETTLE_TAG | (gix + 1)`).
pub fn settle_tag_of(gix: u16) -> u32 {
    SETTLE_TAG | tag_of(gix)
}

/// Seconds of the settlement average (rule 12): the last `SETTLE_TWAP_S` of the window, against
/// the strike averaged over the `SETTLE_TWAP_S` before the window start.
pub const SETTLE_TWAP_S: i64 = 60;

/// `InjectWarmup` local kind of a Chainlink BTC/USD sample (README "Chainlink").
pub const CL_WARMUP_KIND: u16 = 1;
/// A Chainlink sample older than this many seconds is not the current price.
pub const CL_FRESH_S: i64 = 3;
/// `InjectWarmup` local kind of a chop-gate seed (README "规则" 13 后的"震荡开关预热").
pub const CHOP_WARMUP_KIND: u16 = 2;
/// `InjectWarmup` local kind of Coinbase samples (cb_feed sidecar; v37 Coinbase guard).
pub const CB_WARMUP_KIND: u16 = 3;
/// Coinbase BBO samples kept (cb_feed sends a few per second).
pub const CB_N: usize = 512;
/// A Coinbase sample older than this (ns) is not the current price.
pub const CB_FRESH_NS: i64 = 5_000_000_000;

/// Coinbase BBO mids (v37): engine time of arrival and bid + ask (1e-8 USD) of the latest
/// `CB_N` samples; not in the snapshot.
#[derive(Clone, Debug)]
pub struct CbFeed {
    pub t: Box<[i64; CB_N]>,
    pub mid_x2: Box<[i64; CB_N]>,
    pub pos: usize,
    pub n: usize,
}

impl Default for CbFeed {
    fn default() -> Self {
        CbFeed {
            t: Box::new([i64::MIN; CB_N]),
            mid_x2: Box::new([0; CB_N]),
            pos: 0,
            n: 0,
        }
    }
}

impl CbFeed {
    /// Payload v1 (little endian): version u8 = 1, count u16, then count × 41-byte samples (kind
    /// u8, local receive ns i64, exchange ns i64, a, b, c i64). Kind 1 is a BBO (a bid, b ask,
    /// 1e-8 USD); other kinds are ignored. A sample's time is the arrival `ens` less its receive
    /// age against the command's last sample.
    pub fn on_payload(&mut self, payload: &[u8], ens: i64) {
        if payload.first() != Some(&1) || payload.len() < 3 {
            return;
        }
        let n = usize::from(u16::from_le_bytes([payload[1], payload[2]]));
        let body = payload.get(3..).unwrap_or(&[]);
        let f = |s: &[u8], o: usize| s.get(o..o + 8).and_then(|b| <[u8; 8]>::try_from(b).ok()).map(i64::from_le_bytes);
        let last_rx = body.chunks_exact(41).take(n).filter_map(|s| f(s, 1)).max().unwrap_or(0);
        for s in body.chunks_exact(41).take(n) {
            let (Some(rx), Some(a), Some(b)) = (f(s, 1), f(s, 17), f(s, 25)) else { continue };
            if s.first() != Some(&1) || a <= 0 || b < a {
                continue;
            }
            let t = ens.saturating_sub(last_rx.saturating_sub(rx).max(0));
            if let (Some(x), Some(y)) = (self.t.get_mut(self.pos), self.mid_x2.get_mut(self.pos)) {
                *x = t;
                *y = a.saturating_add(b);
            }
            self.pos = (self.pos + 1) % CB_N;
            self.n = self.n.saturating_add(1);
        }
    }
    /// Mid (× 2) of the latest sample at or before `t`, if at most `CB_FRESH_NS` older than `t`.
    pub fn at(&self, t: i64) -> Option<i64> {
        let mut best: Option<(i64, i64)> = None;
        for (x, y) in self.t.iter().zip(self.mid_x2.iter()) {
            if *x <= t && *x > t.saturating_sub(CB_FRESH_NS) && best.is_none_or(|(bt, _)| *x > bt) {
                best = Some((*x, *y));
            }
        }
        best.map(|(_, m)| m)
    }
}

/// Chainlink BTC/USD samples pushed by `InjectWarmup` (local kind 1): price (1e-8 USD) per Chainlink
/// second and the latest sample; not in the snapshot.
#[derive(Clone, Debug, Default)]
pub struct ClFeed {
    pub sec: Box<features::RefSecs>,
    /// Chainlink second and price of the latest sample (`i64::MIN` / 0 before the first).
    pub last_s: i64,
    pub last_px: i64,
    pub n: u64,
}

impl ClFeed {
    /// Payload v1 (17 bytes, little endian): version u8 = 1, Chainlink timestamp ms i64, price
    /// 1e-8 USD i64. Other versions, short payloads and non-positive prices are ignored; a sample
    /// older than the latest only fills its own second.
    pub fn on_payload(&mut self, payload: &[u8]) {
        let field = |a: usize| payload.get(a..a + 8).and_then(|b| <[u8; 8]>::try_from(b).ok()).map(i64::from_le_bytes);
        if payload.first() != Some(&1) {
            return;
        }
        let (Some(ts), Some(px)) = (field(1), field(9)) else { return };
        if px <= 0 {
            return;
        }
        let s = ts.div_euclid(1_000);
        self.sec.put(s, px);
        if self.n == 0 || s >= self.last_s {
            self.last_s = s;
            self.last_px = px;
        }
        self.n = self.n.saturating_add(1);
    }
    /// The latest price if it is at most `CL_FRESH_S` older than UTC second `sec`.
    pub fn now(&self, sec: i64) -> Option<i64> {
        (self.n > 0 && self.last_s >= sec - CL_FRESH_S).then_some(self.last_px)
    }
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
    // Chainlink samples (settle_src = 1) arrive as InjectWarmup on the fast part.
    out.warmup(Part::Both);
}

/// √30 × 1e6 (drift horizon, README "规则" 9).
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

/// Rule 12 state of the current window (not in the snapshot: after a restart the strike is
/// missing until a window whose previous minute was recorded).
#[derive(Clone, Copy, Debug)]
pub struct SettleState {
    /// Window start (UTC seconds) the state belongs to.
    pub win: i64,
    /// The strike was looked up (once per window) and had enough seconds.
    pub k_done: bool,
    pub k_ok: bool,
    /// Sum and count of the reference 2 × mid seconds of the strike.
    pub k_sum: i128,
    pub k_n: i64,
    /// Shares (raw quantity) the module's bids bought in the window.
    pub bought: i64,
    /// Up probability (1e-9), μ (1e-9) and locked seconds of the last decision (research export;
    /// `NA` when not computed).
    pub out: [i64; 3],
}

impl Default for SettleState {
    fn default() -> SettleState {
        SettleState {
            win: i64::MIN,
            k_done: false,
            k_ok: false,
            k_sum: 0,
            k_n: 0,
            bought: 0,
            out: [hft_strategy_sdk::features::NA; 3],
        }
    }
}

/// Per slot leg level / fill state of the v24 rules (not in the snapshot).
#[derive(Clone, Copy, Debug, Default)]
pub struct Micro {
    /// Best-bid price last seen and the engine time it was first seen.
    pub lvl_px: i64,
    pub lvl_since: i64,
    /// Last fill: price, engine time; the swept check runs until `chk_until`.
    pub fill_px: i64,
    pub fill_ens: i64,
    pub chk_until: i64,
    /// Swept pause end (engine ns).
    pub swept_until: i64,
    /// Engine time our resting bid first fell below the best bid (0 = not below).
    pub below_since: i64,
    /// The complement's best bid / ask last seen, and this leg's pause after it moved up.
    pub oth_bb: i64,
    pub oth_ba: i64,
    pub oth_cool: i64,
    /// v28: public best-bid fall pause end; engine time of the last trade on this leg's inst.
    pub drop_until: i64,
    pub trade_ens: i64,
    /// v29: dip bid active until (engine ns).
    pub dip_until: i64,
}

/// v33 order-flow imbalance bucket (engine time) and number of buckets kept (25.6 s).
pub const OFI_BUCKET_NS: i64 = 100_000_000;
pub const OFI_N: usize = 256;

/// Order-flow imbalance of one slot leg's own book (v33 `ofi_win_ms`): Cont-Kukanov-Stoikov
/// increments of the best bid / ask on every `Book` event, cumulated per 100 ms bucket; restarts
/// when the leg's instrument changes; not in the snapshot.
#[derive(Clone, Debug)]
pub struct OfiRing {
    pub inst: InstId,
    /// Previous best (bid px, bid qty, ask px, ask qty); `None` until both sides are seen.
    pub prev: Option<(i64, i64, i64, i64)>,
    pub cum: i64,
    /// First and last bucket seen (`i64::MIN` = none); `ring` holds the cumulative value at the end of
    /// each bucket.
    pub first: i64,
    pub last: i64,
    pub ring: [i64; OFI_N],
}

impl Default for OfiRing {
    fn default() -> Self {
        OfiRing {
            inst: INST_NONE,
            prev: None,
            cum: 0,
            first: i64::MIN,
            last: i64::MIN,
            ring: [0; OFI_N],
        }
    }
}

impl OfiRing {
    fn slot(k: i64) -> usize {
        k.rem_euclid(OFI_N as i64) as usize
    }

    /// One book update of `inst` (best levels after it) at engine time `ens`.
    pub fn on_book(&mut self, inst: InstId, bid: Option<(i64, i64)>, ask: Option<(i64, i64)>, ens: i64) {
        if inst != self.inst {
            *self = OfiRing {
                inst,
                ..OfiRing::default()
            };
        }
        let k = ens.div_euclid(OFI_BUCKET_NS);
        if self.last == i64::MIN {
            self.first = k;
        } else if k > self.last {
            // Buckets without events keep the cumulative value.
            let from = self.last.saturating_add(1).max(k.saturating_sub(OFI_N as i64 - 1));
            for b in from..k {
                self.ring[Self::slot(b)] = self.cum;
            }
        }
        if let (Some((bp, bq)), Some((ap, aq))) = (bid, ask) {
            if let Some((pbp, pbq, pap, paq)) = self.prev {
                let mut e = 0i64;
                if bp >= pbp {
                    e = e.saturating_add(bq);
                }
                if bp <= pbp {
                    e = e.saturating_sub(pbq);
                }
                if ap <= pap {
                    e = e.saturating_sub(aq);
                }
                if ap >= pap {
                    e = e.saturating_add(paq);
                }
                self.cum = self.cum.saturating_add(e);
            }
            self.prev = Some((bp, bq, ap, aq));
        }
        if self.last == i64::MIN || k >= self.last {
            self.ring[Self::slot(k)] = self.cum;
            self.last = k;
        }
    }

    /// Order-flow imbalance over the last `ms` up to `ens`; `None` while the history does not reach
    /// back that far.
    pub fn over(&self, ens: i64, ms: i64) -> Option<i64> {
        if self.last == i64::MIN {
            return None;
        }
        let back = ens.div_euclid(OFI_BUCKET_NS).saturating_sub(ms / (OFI_BUCKET_NS / 1_000_000));
        if back >= self.last {
            return Some(0);
        }
        if back < self.first || back <= self.last.saturating_sub(OFI_N as i64) {
            return None;
        }
        Some(self.cum.saturating_sub(self.ring[Self::slot(back)]))
    }
}

/// Trade-print count of one slot leg (v35 `trd_win_ms`): cumulative count per 100 ms engine-time
/// bucket (256 buckets); restarts when the leg's instrument changes; not in the snapshot.
#[derive(Clone, Debug)]
pub struct TrdRing {
    pub inst: InstId,
    pub cum: i64,
    pub first: i64,
    pub last: i64,
    pub ring: [i64; OFI_N],
}

impl Default for TrdRing {
    fn default() -> Self {
        TrdRing {
            inst: INST_NONE,
            cum: 0,
            first: i64::MIN,
            last: i64::MIN,
            ring: [0; OFI_N],
        }
    }
}

impl TrdRing {
    fn slot(k: i64) -> usize {
        k.rem_euclid(OFI_N as i64) as usize
    }

    /// Advances to engine time `ens` (empty buckets keep the count), restarting on a new instrument.
    fn advance(&mut self, inst: InstId, ens: i64) {
        if inst != self.inst {
            *self = TrdRing {
                inst,
                ..TrdRing::default()
            };
        }
        let k = ens.div_euclid(OFI_BUCKET_NS);
        if self.last == i64::MIN {
            self.first = k;
            self.last = k;
        } else if k > self.last {
            let from = self.last.saturating_add(1).max(k.saturating_sub(OFI_N as i64 - 1));
            for b in from..k {
                self.ring[Self::slot(b)] = self.cum;
            }
            self.last = k;
        }
    }

    /// One trade print of `inst` at `ens`.
    pub fn on_trade(&mut self, inst: InstId, ens: i64) {
        self.advance(inst, ens);
        self.cum = self.cum.saturating_add(1);
        if ens.div_euclid(OFI_BUCKET_NS) >= self.last {
            self.ring[Self::slot(self.last)] = self.cum;
        }
    }

    /// Prints over the last `ms` up to `ens`; `None` while the history does not reach back that far.
    pub fn over(&self, ens: i64, ms: i64) -> Option<i64> {
        if self.last == i64::MIN {
            return None;
        }
        let back = ens.div_euclid(OFI_BUCKET_NS).saturating_sub(ms / (OFI_BUCKET_NS / 1_000_000));
        if back >= self.last {
            return Some(0);
        }
        if back < self.first || back <= self.last.saturating_sub(OFI_N as i64) {
            return None;
        }
        Some(self.cum.saturating_sub(self.ring[Self::slot(back)]))
    }
}

/// Efficiency-ratio history of the Up mid for the chop gate (v31; not in the snapshot).
#[derive(Clone, Copy, Debug, Default)]
pub struct Chop {
    pub win: i64,
    pub last_sec: i64,
    pub first: i64,
    pub last: i64,
    pub path: i64,
    pub n: u32,
    pub ring: [i64; 24],
    pub len: u8,
    pub pos: u8,
}

impl Chop {
    /// Seed after a restart (`InjectWarmup` local kind 2). Payload v1: version u8 = 1, count u8, then
    /// `count` × (window start s i64, efficiency ratio 1e-9 i64), little endian. Applied only while the
    /// ring is empty; entries out of [0, 1e9], at or after the window being accumulated (`win` > 0) or
    /// not in increasing window order are skipped; the last 24 are kept, the newest last.
    pub fn seed(&mut self, payload: &[u8]) {
        if self.len != 0 || payload.first() != Some(&1) {
            return;
        }
        let Some(&count) = payload.get(1) else { return };
        let field = |a: usize| {
            payload
                .get(a..a + 8)
                .and_then(|b| <[u8; 8]>::try_from(b).ok())
                .map(i64::from_le_bytes)
        };
        let mut kept = [0i64; 24];
        let mut n = 0usize;
        let mut prev = i64::MIN;
        for j in 0..usize::from(count) {
            let (Some(w), Some(er)) = (field(2 + 16 * j), field(10 + 16 * j)) else { break };
            if w <= prev || !(0..=ONE).contains(&er) || (self.win > 0 && w >= self.win) {
                continue;
            }
            prev = w;
            if n == 24 {
                kept.copy_within(1.., 0);
                n = 23;
            }
            if let Some(x) = kept.get_mut(n) {
                *x = er;
            }
            n += 1;
        }
        self.ring = [0; 24];
        self.ring[..n].copy_from_slice(&kept[..n]);
        self.len = n as u8;
        self.pos = (n % 24) as u8;
    }

    /// Mean of the last `k` window efficiency ratios (1e-9), `None` with fewer than k / 2 of them.
    pub fn mean(&self, k: usize) -> Option<i64> {
        let n = usize::from(self.len).min(k);
        if n == 0 || n * 2 < k {
            return None;
        }
        let mut s = 0i128;
        for j in 0..n {
            let at = (usize::from(self.pos) + 24 - 1 - j) % 24;
            s += i128::from(self.ring[at]);
        }
        Some((s / n as i128) as i64)
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
    /// Features the rules read (`crate::features`; not in the snapshot).
    pub feat: MakerFeatures,
    /// Fair-value anchor of the current window (not in the snapshot: rebuilt on the next event).
    pub fv: FvAnchor,
    pub comp: CompCache,
    /// Directional lock of the current window (dir_lock): (window start, 0 none / 1 Up / 2 Down);
    /// not in the snapshot.
    pub lock: (i64, u8),
    /// Phase lock (dir_phase_ms): (window start, phase, 0 none / 1 Up / 2 Down); not in the snapshot.
    pub plock: (i64, u8, u8),
    /// The eight raw features of Up, Down of the cached score (`Rate`; `NA` = missing), for the
    /// research export only.
    pub comp_f: [[i64; features::N_FEAT]; 2],
    /// Best bid / ask of Up and Down when the score was cached (raw px; `NA` = side missing), for the
    /// research export only.
    pub comp_book: [i64; 4],
    /// Signal freshness (dir_fresh_ms): (window start, UTC ns each token's score last rose above
    /// comp_dir, `i64::MIN` while it is not above); not in the snapshot.
    pub fresh: (i64, [i64; 2]),
    /// v24 level / fill state per slot leg (quote order); not in the snapshot.
    pub mic: Vec<Micro>,
    /// Rule 12 state of the current window; not in the snapshot.
    pub settle: SettleState,
    /// Per slot leg (quote order): its current order was placed by rule 12; not in the snapshot.
    pub skind: Vec<bool>,
    /// v31 chop gate state; not in the snapshot.
    pub chop: Chop,
    /// Window start in which a stop-loss pairing bid (rescue_ms) was placed; not in the snapshot.
    pub rescued: i64,
    /// Chainlink samples (settle_src); not in the snapshot.
    pub cl: ClFeed,
    /// v33 order-flow imbalance per slot leg (quote order); not in the snapshot.
    pub ofi: Vec<OfiRing>,
    /// v35 trade-print count per slot leg (quote order); not in the snapshot.
    pub trd: Vec<TrdRing>,
    /// v37 Coinbase samples and, per slot leg (quote order), the engine time its guard holds until;
    /// not in the snapshot.
    pub cb: CbFeed,
    pub cb_until: Vec<i64>,
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
            feat: MakerFeatures::new(p.defaults().vol_window_s as usize, cap),
            fv: FvAnchor {
                win: i64::MIN,
                ..FvAnchor::default()
            },
            comp: CompCache::default(),
            lock: (i64::MIN, 0),
            plock: (i64::MIN, 0, 0),
            comp_f: [[hft_strategy_sdk::features::NA; features::N_FEAT]; 2],
            comp_book: [hft_strategy_sdk::features::NA; 4],
            fresh: (i64::MIN, [i64::MIN; 2]),
            mic: Vec::new(),
            settle: SettleState::default(),
            skind: Vec::new(),
            chop: Chop::default(),
            rescued: i64::MIN,
            cl: ClFeed {
                last_s: i64::MIN,
                ..ClFeed::default()
            },
            ofi: Vec::with_capacity(cap),
            trd: Vec::with_capacity(cap),
            cb: CbFeed::default(),
            cb_until: Vec::with_capacity(cap),
        };
        f.on_params(p, groups);
        f
    }

    /// New parameters: slot groups keep their quote state by `GroupId` and leg; the volatility
    /// ring restarts when its length changes.
    pub fn on_params(&mut self, p: &BinaryMakerParams, groups: &GroupTable) {
        self.p = p.clone();
        self.scratch.clear();
        self.scratch.extend_from_slice(&self.quotes);
        self.quotes.clear();
        let old_kind = std::mem::take(&mut self.skind);
        let mut kind = Vec::with_capacity(usize::from(groups.len()));
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
            let at = self.scratch.iter().position(|q| q.id == g.id && q.inst == l.inst);
            let kept = at.and_then(|k| self.scratch.get(k)).copied().unwrap_or(fresh);
            self.quotes.push(kept);
            kind.push(at.and_then(|k| old_kind.get(k)).copied().unwrap_or(false));
        }
        self.skind = kind;
        // The volatility ring follows `vol_window_s`; score histories follow the slot insts.
        self.feat.on_params(
            p.defaults().vol_window_s as usize,
            self.quotes.iter().map(|q| q.inst),
        );
        self.comp.ok = false;
        self.mic = vec![Micro::default(); self.quotes.len()];
        self.ofi.clear();
        self.ofi.resize(self.quotes.len(), OfiRing::default());
        self.trd.clear();
        self.trd.resize(self.quotes.len(), TrdRing::default());
        self.cb_until.clear();
        self.cb_until.resize(self.quotes.len(), i64::MIN);
    }

    /// Composite score on (`comp_skip` or `comp_dir` set).
    pub fn comp_on(&self) -> bool {
        let d = self.p.defaults();
        d.comp_skip.0 != 0 || d.comp_dir.0 > 0 || d.open_skip_z.0 > 0 || d.mm_min_path.0 > 0
    }

    /// Venue-load gates on.
    pub fn lag_on(&self) -> bool {
        let d = self.p.defaults();
        d.load_gate_ns > 0 || d.mdslow_gate_ns > 0
    }

    /// What the feature plan needs of the parameters.
    fn plan_cfg(&self) -> PlanCfg {
        PlanCfg {
            comp: self.comp_on(),
            lag: self.lag_on(),
            flow_min_shares: self.p.defaults().comp_flow_min_shares,
            settle: self.p.defaults().settle_ms > 0,
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
    pub fn comp_score<V: Views>(
        &self,
        v: &V,
        x: usize,
        o: usize,
        start: i64,
        utc_ns: i64,
    ) -> Option<(i64, [i64; features::N_FEAT])> {
        let d = self.p.defaults();
        let (qx, qo) = (self.quotes.get(x)?, self.quotes.get(o)?);
        let hx = self.feat.hist.get(x)?;
        let hot = v.inst(qx.inst);
        let unit_px = 10i64.saturating_pow(u32::from(hot.px_exp.unsigned_abs()));
        let unit_qty = 10i64.saturating_pow(u32::from(hot.qty_exp.unsigned_abs()));
        let bx = BidSide::read(&v.book(qx.inst), Side::Buy, self.own_bid(v, x, start));
        let bo = BidSide::read(&v.book(qo.inst), Side::Buy, self.own_bid(v, o, start));
        let implied = self
            .feat
            .vol
            .sigma(d.vol_min_samples as usize)
            .map(|s| (i128::from(s) * i128::from(d.sigma_scale.0) / i128::from(ONE)) as i64);
        let f = features::features(&ScoreIn {
            x: &bx,
            o: &bo,
            hx,
            ref_x2: &self.feat.ref_x2,
            trades: &self.feat.trades,
            x_inst: qx.inst,
            o_inst: qo.inst,
            is_up: x.is_multiple_of(2),
            utc_ns,
            implied,
            unit_px,
            unit_qty,
        });
        Some((
            features::score(&f, &self.comp_mu(), &self.comp_sd()),
            f.map(|x| x.unwrap_or(hft_strategy_sdk::features::NA)),
        ))
    }

    /// `comp_*_mu` in feature order.
    pub fn comp_mu(&self) -> [i64; features::N_FEAT] {
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
    pub fn comp_sd(&self) -> [i64; features::N_FEAT] {
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
            (Some((zu, fu)), Some((zd, fd))) => {
                self.comp = CompCache {
                    ok: true,
                    win: start,
                    utc_ns,
                    z: [zu, zd],
                };
                self.comp_f = [fu, fd];
                let side = |i: usize, s: Side| {
                    self.quotes
                        .get(i)
                        .and_then(|q| v.book(q.inst).best(s).map(|l| l.px.0))
                        .unwrap_or(hft_strategy_sdk::features::NA)
                };
                self.comp_book = [side(up, Side::Buy), side(up, Side::Sell), side(dn, Side::Buy), side(dn, Side::Sell)];
            }
            _ => self.comp.ok = false,
        }
    }

    pub fn on_event<V: Views>(&mut self, ev: &EventRef<'_>, v: &V, ctx: &mut Ctx<'_>) {
        self.outcomes(ctx);
        let cfg = self.plan_cfg();
        self.feat.on_event(ev, v, self.reference, cfg);
        match *ev {
            EventRef::Bbo { inst, .. } if inst == self.reference => self.run(v, ctx),
            EventRef::Warmup { local_kind, payload, .. } if local_kind == CL_WARMUP_KIND => self.cl.on_payload(payload),
            EventRef::Warmup { local_kind, payload, .. } if local_kind == CHOP_WARMUP_KIND => self.chop.seed(payload),
            EventRef::Warmup { local_kind, payload, .. } if local_kind == CB_WARMUP_KIND => {
                self.cb.on_payload(payload, v.now().engine_ns);
                if self.p.defaults().cb_win_ms > 0 {
                    self.run(v, ctx);
                }
            }
            EventRef::Book { inst, .. } => {
                if self.p.defaults().ofi_win_ms > 0 {
                    let ens = v.now().engine_ns;
                    let b = v.book(inst);
                    let lv = |s: Side| b.best(s).map(|l| (l.px.0, l.qty.0));
                    for (q, o) in self.quotes.iter().zip(self.ofi.iter_mut()) {
                        if q.inst == inst {
                            o.on_book(inst, lv(Side::Buy), lv(Side::Sell), ens);
                        }
                    }
                }
                self.run(v, ctx);
            }
            EventRef::Trade { inst, .. } => {
                let now = v.now().engine_ns;
                let trd_on = self.p.defaults().trd_win_ms > 0;
                for (k, q) in self.quotes.iter().enumerate() {
                    if q.inst == inst
                        && let Some(m) = self.mic.get_mut(k)
                    {
                        m.trade_ens = now;
                    }
                    if trd_on
                        && q.inst == inst
                        && let Some(t) = self.trd.get_mut(k)
                    {
                        t.on_trade(inst, now);
                    }
                }
            }
            EventRef::Order { u, .. } => {
                let now = v.now().engine_ns;
                let k = self.quotes.iter().position(|q| q.r == Some(u.r));
                if let Some(q) = k.and_then(|k| self.quotes.get_mut(k)) {
                    if let OrderUpdateKind::Filled { d, .. } = u.kind
                        && d.0 > 0
                    {
                        q.fill_ens[(q.n_fill % 8) as usize] = now;
                        q.n_fill = q.n_fill.wrapping_add(1);
                        if k.and_then(|k| self.skind.get(k)) == Some(&true) && q.win == self.settle.win {
                            self.settle.bought = self.settle.bought.saturating_add(d.0);
                        }
                        if let Some(m) = k.and_then(|k| self.mic.get_mut(k)) {
                            m.fill_px = q.px;
                            m.fill_ens = now;
                            m.chk_until = now.saturating_add(1_000_000_000);
                        }
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
        let ref_mid_x2 = self.feat.top.mid_x2;
        if q.den <= 0 || q.b0 <= 0 || ref_mid_x2 <= 0 {
            return 0;
        }
        let one = i128::from(ONE);
        let moved = (i128::from(ref_mid_x2) - i128::from(q.b0)) * one / i128::from(q.b0);
        let shift = moved * one / i128::from(q.den);
        let z = (i128::from(q.d0) + shift).clamp(-6 * one, 6 * one) as i64;
        cdf(z) - cdf(q.d0)
    }

    /// Direction forecast (README "规则" 9) with this instance's `pred_w_*` weights.
    pub fn forecast(&self, sigma: i64) -> i64 {
        let d = self.p.defaults();
        self.feat
            .forecast(sigma, d.pred_w_imb.0, d.pred_w_ret.0, d.pred_w_ser.0)
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
        let ref_mid_x2 = self.feat.top.mid_x2;
        if self.fv.b_open <= 0 {
            self.fv.b_open = ref_mid_x2;
        }
        let sigma = self.feat.vol.sigma(d.vol_min_samples as usize)?;
        let q = *self.quotes.get(up)?;
        if self.fv.b_open <= 0 || ref_mid_x2 <= 0 || den <= 0 {
            return None;
        }
        let one = i128::from(ONE);
        let moved = (i128::from(ref_mid_x2) - i128::from(self.fv.b_open)) * one / i128::from(self.fv.b_open);
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
        // v36 A/B sizing: odd windows scale the share limits by ab_scale.
        let ab = if d.ab_scale > 1 && w > 0 && (start / w).rem_euclid(2) == 1 {
            d.ab_scale
        } else {
            1
        };
        let d = if ab > 1 {
            let mut x = d;
            x.clip_shares = x.clip_shares.saturating_mul(ab);
            x.inv_max_shares = x.inv_max_shares.saturating_mul(ab);
            x.dip_max_shares = x.dip_max_shares.saturating_mul(ab);
            x.early_cap_shares = x.early_cap_shares.saturating_mul(ab);
            x
        } else {
            d
        };
        // Venue-load gates (README "规则" 11): the previous minute's feed lag closes quoting.
        let (load, slow) = self.feat.lag.gates(now.utc_ns, d.load_gate_ns, d.mdslow_gate_ns);
        // v34 low-volatility band: implied sigma in [sigma_lo, sigma_min) quotes with opening conditions.
        let vol = self.vol_state(left_ms);
        let vol_low = vol == 1;
        let quoting = ok && left_ms > i64::from(d.stop_before_ms) && vol > 0 && !load && !slow;
        // v38 pair_ignore_lag: the venue-load gates close openings only; pairing bids keep quoting.
        let lag_pair = d.pair_ignore_lag && ok && left_ms > i64::from(d.stop_before_ms) && vol > 0 && (load || slow);
        // Last-minute stop-loss (late_hedge_ms): only pairing bids by the leg the score favours.
        // Pairing priority (pair_stop_ms): a short leg pairs until pair_stop_ms before the end.
        let pair_window = ok && d.pair_stop_ms > 0 && left_ms > i64::from(d.pair_stop_ms);
        let hedging = ok
            && d.late_hedge_ms > 0
            && left_ms <= i64::from(d.stop_before_ms)
            && left_ms > i64::from(d.late_hedge_ms);
        // Settlement-window liquidity (rule 12): the last settle_ms while the main rules (quoting,
        // the stop-loss minute, pairing priority) have no target; the module owns its legs' quotes.
        let settling = ok
            && d.settle_ms > 0
            && left_ms <= i64::from(d.settle_ms)
            && !quoting
            && !hedging
            && !pair_window;
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
        // Directional lock (dir_lock): the first token whose net exceeds inv_max_shares.
        if self.lock.0 != start {
            self.lock = (start, 0);
        }
        if d.dir_lock
            && self.lock.1 == 0
            && let Some(q) = self.quotes.get(up)
        {
            let uq = 10i64.saturating_pow(u32::from(v.inst(q.inst).qty_exp.unsigned_abs()));
            let cap = i64::from(d.inv_max_shares).saturating_mul(uq);
            if qu.saturating_sub(qd) > cap {
                self.lock.1 = 1;
            } else if qd.saturating_sub(qu) > cap {
                self.lock.1 = 2;
            }
        }
        let lock = if d.dir_lock { self.lock.1 } else { 0 };
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
        // Chop gate state (v31): the Up mid's efficiency ratio of each window, 30..240 s.
        let age_c = i64::from(d.window_s) * 1_000 - left_ms;
        if self.chop.win != start {
            let c = self.chop;
            if c.n >= 100 && c.path > 0 {
                let er = (i128::from((c.last - c.first).abs()) * i128::from(ONE) / i128::from(c.path)) as i64;
                let p = usize::from(c.pos);
                self.chop.ring[p] = er;
                self.chop.pos = ((p + 1) % 24) as u8;
                self.chop.len = c.len.saturating_add(1).min(24);
            }
            self.chop.win = start;
            self.chop.n = 0;
            self.chop.path = 0;
            self.chop.last_sec = 0;
        }
        let up_valid = self.quotes.get(up).is_some_and(|q| {
            let b = v.book(q.inst);
            matches!((b.best(Side::Buy), b.best(Side::Sell)), (Some(x), Some(y)) if x.px.0 < y.px.0)
        });
        let sec_c = now.utc_ns.div_euclid(1_000_000_000);
        if up_valid && (30_000..=240_000).contains(&age_c) && sec_c != self.chop.last_sec {
            if self.chop.n == 0 {
                self.chop.first = up_mid;
            } else {
                self.chop.path = self.chop.path.saturating_add((up_mid - self.chop.last).abs());
            }
            self.chop.last = up_mid;
            self.chop.n += 1;
            self.chop.last_sec = sec_c;
        }
        let chop_mean = if d.chop_k > 0 { self.chop.mean(d.chop_k as usize) } else { None };
        let chop_ok = d.chop_k == 0 || chop_mean.is_some_and(|e| e <= d.chop_er_max.0);
        // v38 calm favourite: this window's running efficiency ratio of the Up mid (30 s .. now) and
        // the direction it moved (+1 Up, -1 Down, 0 flat or too few seconds).
        let (win_er, win_dir) = if self.chop.win == start && self.chop.n >= 20 && self.chop.path > 0 {
            let net = self.chop.last - self.chop.first;
            ((i128::from(net.abs()) * i128::from(ONE) / i128::from(self.chop.path)) as i64, net.signum())
        } else {
            (0, 0)
        };
        let den = self.feat.vol.sigma(d.vol_min_samples as usize).map_or(0, |s| {
            let implied = i128::from(s) * i128::from(d.sigma_scale.0) / i128::from(ONE);
            let v_ms = (left_ms - TWAP_TAIL_MS).max(1);
            let root_milli = isqrt(u128::try_from(v_ms).unwrap_or(1) * 1_000) as i128;
            (implied * root_milli / 1_000) as i64
        });
        // v37 Coinbase guard: the Up fair-value move the Coinbase mid implies over cb_win_ms,
        // Φ(Φ⁻¹(Up mid) + r / den) − Up mid; at least cb_shock against a leg holds it cb_hold_ms.
        if d.cb_win_ms > 0 && d.cb_shock.0 > 0 && den > 0 {
            let ens = now.engine_ns;
            if let (Some(a), Some(b)) = (self.cb.at(ens), self.cb.at(ens.saturating_sub(i64::from(d.cb_win_ms) * 1_000_000)))
                && b > 0
            {
                let one = i128::from(ONE);
                let r = (i128::from(a) - i128::from(b)) * one / i128::from(b);
                let z = (i128::from(inv_cdf(up_mid)) + r * one / i128::from(den)).clamp(-6 * one, 6 * one) as i64;
                let shock = cdf(z) - up_mid;
                let until = ens.saturating_add(i64::from(d.cb_hold_ms) * 1_000_000);
                let leg = if shock <= -d.cb_shock.0 {
                    Some(up)
                } else if shock >= d.cb_shock.0 {
                    Some(up.wrapping_add(1))
                } else {
                    None
                };
                if let Some(u) = leg.and_then(|k| self.cb_until.get_mut(k)) {
                    *u = (*u).max(until);
                }
            }
        }
        let lims = if d.fv_limit && (quoting || lag_pair) {
            self.fair_limits(v, start, up, den, qu.saturating_sub(qd) / i64::from(ab), now.engine_ns)
        } else {
            None
        };
        // Stop-loss pairing window (rescue_ms, v32): from rescue_ms until the stop-loss minute.
        let rescue_on = ok
            && d.rescue_ms > 0
            && age_c >= i64::from(d.rescue_ms)
            && left_ms > i64::from(d.stop_before_ms);
        self.comp_update(v, quoting || hedging || pair_window || d.comp_always || rescue_on, start, up, now.utc_ns);
        let comp = self.comp;
        // Up-mid path over the last 60 s (fraction of one), for mm_min_path: sum of |Δ 2 × mid| / 2 of
        // consecutive recorded seconds.
        let quiet = d.mm_min_path.0 > 0
            && self.feat.hist.get(up).is_some_and(|h| {
                let sec = now.utc_ns.div_euclid(1_000_000_000);
                let unit = self.quotes.get(up).map_or(1, |q| {
                    10i64.saturating_pow(u32::from(v.inst(q.inst).px_exp.unsigned_abs()))
                });
                let mut path = 0i128;
                for k in 0..60 {
                    if let (Some(a), Some(b)) = (h.mid_x2.at(sec - k), h.mid_x2.at(sec - k - 1)) {
                        path += i128::from((a - b).abs());
                    }
                }
                path * i128::from(ONE) / (2 * i128::from(unit.max(1))) < i128::from(d.mm_min_path.0)
            });
        let age_ms = i64::from(d.window_s) * 1_000 - left_ms;
        // One direction per phase (dir_phase_ms): the first token favoured in the phase.
        let phase = u8::from(d.dir_phase_ms > 0 && age_ms >= i64::from(d.dir_phase_ms));
        if (self.plock.0, self.plock.1) != (start, phase) {
            self.plock = (start, phase, 0);
        }
        if d.dir_phase_ms > 0 && self.plock.2 == 0 && d.comp_dir.0 > 0 && comp.ok && comp.win == start {
            let (zu, zd) = (comp.z[0], comp.z[1]);
            if zu > d.comp_dir.0 && zu >= zd {
                self.plock.2 = 1;
            } else if zd > d.comp_dir.0 {
                self.plock.2 = 2;
            }
        }
        let plock = if d.dir_phase_ms > 0 { self.plock.2 } else { 0 };
        // Signal freshness (dir_fresh_ms): when each token's score last rose above comp_dir.
        if self.fresh.0 != start {
            self.fresh = (start, [i64::MIN; 2]);
        }
        if d.dir_fresh_ms > 0 && comp.ok && comp.win == start {
            for k in 0..2 {
                let above = comp.z[k] > d.comp_dir.0;
                if !above {
                    self.fresh.1[k] = i64::MIN;
                } else if self.fresh.1[k] == i64::MIN {
                    self.fresh.1[k] = now.utc_ns;
                }
            }
        }
        let fresh = self.fresh.1;
        // Early fast phase (fast_until_ms): shorter quote change interval, optionally no no-chase
        // cooldown and no streak pause; with fast_pair_only only for a short leg (below).
        let fast_phase = d.fast_until_ms > 0 && age_ms < i64::from(d.fast_until_ms);
        let cool = i64::from(d.fv_cool_ms) * 1_000_000;
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
            // Rule 12 decides its own bids while it runs.
            if settling && q.state != QState::None && self.skind.get(i) == Some(&true) {
                continue;
            }
            let hot = *v.inst(q.inst);
            let unit_px = 10i64.saturating_pow(u32::from(hot.px_exp.unsigned_abs()));
            let unit_qty = 10i64.saturating_pow(u32::from(hot.qty_exp.unsigned_abs()));
            let is_up = i % 2 == 0;
            let book = v.book(q.inst);
            let live = book.status() == BookStatus::Live;
            let bid = book.best(Side::Buy).map(|l| l.px.0);
            let ask = book.best(Side::Sell).map(|l| l.px.0);
            // Locked or crossed (one taker trade arriving as several level updates): no undercut.
            let locked = matches!((bid, ask), (Some(b), Some(a)) if b >= a);
            // Pair-cost limit when this bid completes pairs: 1 − other side's cost − margin,
            // floored to a whole cent.
            let (own_q, other_q, other_c) = if is_up { (qu, qd, cd) } else { (qd, qu, cu) };
            // v24 level / fill state: best-bid level age and size less our own, swept fills, the
            // complement moving up.
            let ens = now.engine_ns;
            let own_at_best = (matches!(q.state, QState::Live | QState::Cancelling) && bid == Some(q.px))
                .then(|| q.r.and_then(|r| v.order(r)))
                .flatten()
                .map_or(0, |o| o.qty.0.saturating_sub(o.filled.0).max(0));
            let bq_ex = book
                .best(Side::Buy)
                .map_or(0, |l| l.qty.0.saturating_sub(own_at_best).max(0));
            let mut m = self.mic.get(i).copied().unwrap_or_default();
            let b_now = bid.unwrap_or(0);
            // Public sweep gate (mkt_drop_min_c): the best bid fell by at least mkt_drop_min_c cents.
            if d.mkt_drop_min_c > 0 && m.lvl_px > 0 && b_now > 0 {
                let c1 = (unit_px / 100).max(1);
                let fell = b_now <= m.lvl_px.saturating_sub(i64::from(d.mkt_drop_min_c).saturating_mul(c1));
                let traded = !d.mkt_drop_need_trade || {
                    let t_oth = self.mic.get(i ^ 1).map_or(0, |o| o.trade_ens);
                    m.trade_ens.max(t_oth) >= ens.saturating_sub(100_000_000)
                };
                if fell && traded {
                    m.drop_until = ens.saturating_add(i64::from(d.mkt_drop_pause_ms) * 1_000_000);
                }
            }
            if b_now != m.lvl_px {
                m.lvl_px = b_now;
                m.lvl_since = ens;
            }
            let lvl_age_ms = ens.saturating_sub(m.lvl_since) / 1_000_000;
            if m.chk_until != 0 {
                if ens <= m.chk_until && bid.is_none_or(|b| b < m.fill_px) {
                    m.swept_until = m.fill_ens.saturating_add(i64::from(d.swept_pause_ms) * 1_000_000);
                    m.chk_until = 0;
                } else if ens > m.chk_until {
                    m.chk_until = 0;
                }
            }
            let swept = d.swept_pause_ms > 0 && ens < m.swept_until;
            if q.state == QState::Live && bid.is_some_and(|b| b > q.px) {
                if m.below_since == 0 {
                    m.below_since = ens;
                }
            } else {
                m.below_since = 0;
            }
            let (obb, oba) = self.quotes.get(i ^ 1).map_or((0, 0), |o| {
                let b = v.book(o.inst);
                (b.best(Side::Buy).map_or(0, |l| l.px.0), b.best(Side::Sell).map_or(0, |l| l.px.0))
            });
            let oth_up = d.oth_up_cancel && ((m.oth_bb > 0 && obb > m.oth_bb) || (m.oth_ba > 0 && oba > m.oth_ba));
            m.oth_bb = obb;
            m.oth_ba = oba;
            if oth_up && own_q >= other_q {
                m.oth_cool = ens.saturating_add(i64::from(d.oth_up_cool_ms) * 1_000_000);
            }
            let oth_cool = d.oth_up_cancel && own_q >= other_q && ens < m.oth_cool;
            let dropped = d.mkt_drop_min_c > 0 && own_q >= other_q && ens < m.drop_until;
            // Dip bids (dip_mode): the token rose and the score turned -> bid below the best bid.
            let (dip_active, dip_depth_c) = if d.dip_mode {
                let zk = (comp.ok && comp.win == start && i / 2 == up / 2).then(|| comp.z.get(i % 2).copied()).flatten();
                let mom = (comp.ok && comp.win == start && i / 2 == up / 2)
                    .then(|| self.comp_f.get(i % 2).and_then(|f| f.get(2)).copied())
                    .flatten()
                    .filter(|x| *x != hft_strategy_sdk::features::NA);
                let trig = if d.dip_falling {
                    mom.is_some_and(|x| x <= -d.dip_mom_min.0) && zk.is_some_and(|z| z >= d.dip_z_min.0)
                } else {
                    mom.is_some_and(|x| x >= d.dip_mom_min.0) && zk.is_some_and(|z| z <= d.dip_z_max.0)
                };
                let capped = d.dip_max_shares > 0
                    && own_q.saturating_add(i64::from(d.clip_shares).saturating_mul(unit_qty))
                        > i64::from(d.dip_max_shares).saturating_mul(unit_qty);
                if ens >= m.dip_until && trig && chop_ok && !capped {
                    m.dip_until = ens.saturating_add(i64::from(d.dip_ttl_ms) * 1_000_000);
                }
                let sec = now.utc_ns.div_euclid(1_000_000_000);
                let c1 = (unit_px / 100).max(1);
                let (mut lo_m, mut hi_m) = (i64::MAX, i64::MIN);
                if let Some(h) = self.feat.hist.get(i) {
                    for k in 0..i64::from(d.dip_range_s) {
                        if let Some(x) = h.mid_x2.at(sec - k) {
                            lo_m = lo_m.min(x);
                            hi_m = hi_m.max(x);
                        }
                    }
                }
                let range_c = if hi_m >= lo_m { (hi_m - lo_m) / 2 / c1 } else { 0 };
                let depth = (i128::from(range_c) * i128::from(d.dip_depth_k.0) / i128::from(ONE)) as i64;
                let depth = depth.clamp(i64::from(d.dip_depth_min_c), i64::from(d.dip_depth_max_c.max(d.dip_depth_min_c)));
                (ens < m.dip_until, depth)
            } else {
                (false, 0)
            };
            if let Some(x) = self.mic.get_mut(i) {
                x.dip_until = m.dip_until;
            }
            // Short pairing window (pair_window_ms): pairing only soon after the long leg's last fill,
            // or while the unpaired risk exceeds pair_risk_usd.
            let no_pair = d.pair_window_ms > 0 && other_q > own_q && {
                let long_fill = self.mic.get(i ^ 1).map_or(0, |o| o.fill_ens);
                let in_window = long_fill > 0 && ens.saturating_sub(long_fill) <= i64::from(d.pair_window_ms) * 1_000_000;
                let risky = d.pair_risk_usd.0 > 0 && {
                    let e = i32::from(hot.qty_exp) + i32::from(hot.px_exp) + 6;
                    let raw = i128::from(other_q.saturating_sub(own_q)) * i128::from(other_c);
                    let usd = if e >= 0 {
                        raw * 10i128.pow(e.unsigned_abs())
                    } else {
                        raw / 10i128.pow(e.unsigned_abs())
                    };
                    usd > i128::from(d.pair_risk_usd.0)
                };
                !in_window && !risky
            };
            if let Some(x) = self.mic.get_mut(i) {
                *x = m;
            }
            // Composite score (README "规则" 10): skip the token it is against; the one it favours
            // may hold up to dir_cap_shares net and, with dir_pair_free, pairs without the limit.
            let z = (comp.ok && comp.win == start && i / 2 == up / 2)
                .then(|| comp.z.get(i % 2).copied())
                .flatten();
            let favoured = d.comp_dir.0 > 0
                && z.is_some_and(|z| z > d.comp_dir.0)
                && (lock == 0 || usize::from(lock) == i % 2 + 1)
                && (plock == 0 || usize::from(plock) == i % 2 + 1)
                && (d.dir_from_ms == 0 || age_ms >= i64::from(d.dir_from_ms))
                && (d.comp_dir_max.0 == 0 || z.is_some_and(|z| z <= d.comp_dir_max.0))
                && (d.dir_fresh_ms == 0
                    || fresh.get(i % 2).is_some_and(|&f| {
                        f != i64::MIN && now.utc_ns.saturating_sub(f) <= i64::from(d.dir_fresh_ms) * 1_000_000
                    }));
            // Fast settings: the early phase, and from fast_dir_from_ms on the favoured leg.
            let fast = (fast_phase && (!d.fast_pair_only || other_q > own_q))
                || (d.fast_dir_from_ms > 0 && age_ms >= i64::from(d.fast_dir_from_ms) && favoured);
            let requote = i64::from(if fast { d.fast_requote_ms } else { d.requote_ms }) * 1_000_000;
            let nochase = if fast && d.fast_free {
                0
            } else {
                i64::from(d.nochase_ms) * 1_000_000
            };
            // Stop-loss pairing (rescue_ms): a short leg whose score is above rescue_z while the held
            // leg's best bid is at most rescue_px.
            let rescue = rescue_on
                && other_q > own_q
                && z.is_some_and(|z| z > d.rescue_z.0)
                && obb > 0
                && i128::from(obb) * i128::from(ONE) <= i128::from(d.rescue_px.0) * i128::from(unit_px);
            // In the stop-loss minute a leg quotes only to pair, when the score is above late_hedge_z;
            // a stop-loss pairing bid quotes the same way (best bid, no pair-cost limit).
            let hedge = (hedging && other_q > own_q && z.is_some_and(|z| z > d.late_hedge_z.0)) || rescue;
            let pairing = pair_window
                && other_q > own_q
                && (d.pair_prio_age_ms == 0 || age_ms < i64::from(d.pair_prio_age_ms));
            let mine = (quoting || hedge || pairing || (lag_pair && other_q > own_q)) && i / 2 == up / 2;
            let late_pair = d.pair_late_ms > 0 && age_ms >= i64::from(d.pair_late_ms);
            let free_fav = d.dir_pair_free && favoured && !(d.pair_late_all && late_pair);
            let lim = (other_q > own_q && !hedge && (!free_fav || d.fav_pair_cap.0 > 0 || swept)).then(|| {
                let pm = if swept {
                    -d.swept_pair_cap.0
                } else if free_fav {
                    -d.fav_pair_cap.0
                } else if late_pair {
                    d.pair_late_margin.0
                } else {
                    d.pair_margin.0
                };
                let margin = (i128::from(pm) * i128::from(unit_px) / i128::from(ONE)) as i64;
                let cent = (unit_px / 100).max(1);
                (unit_px - other_c - margin).div_euclid(cent) * cent
            });
            // rescue_cap: a stop-loss pair locks at a cost of at most 1 + rescue_cap.
            let lim = if rescue && d.rescue_cap.0 > 0 {
                let c1 = (unit_px / 100).max(1);
                let extra = (i128::from(d.rescue_cap.0) * i128::from(unit_px) / i128::from(ONE)) as i64;
                Some((unit_px - other_c + extra).div_euclid(c1) * c1)
            } else {
                lim
            };
            let lo = (i128::from(d.p_min.0) * i128::from(unit_px) / i128::from(ONE)) as i64;
            let hi = (i128::from(d.p_max.0) * i128::from(unit_px) / i128::from(ONE)) as i64;
            // Fair-value limit of this leg (README "规则" 9), raw price; the bid cap is floored to a cent.
            let fv_px = lims.map(|(lu, ld)| {
                (i128::from(if is_up { lu } else { ld }) * i128::from(unit_px) / i128::from(ONE)) as i64
            });
            let cent = (unit_px / 100).max(1);
            let fv_cap = fv_px.map(|f| f.div_euclid(cent) * cent);
            // A negative comp_skip skips every token not favoured by more than −comp_skip.
            // Absorption (absorb_shares): a skipped leg still pairs the net beyond the directional part.
            let excess = d.absorb_shares > 0
                && other_q.saturating_sub(own_q) > i64::from(d.absorb_shares).saturating_mul(unit_qty);
            let skip = d.comp_skip.0 != 0 && z.is_some_and(|z| z < -d.comp_skip.0) && !excess;
            let cap_shares = if favoured {
                d.dir_cap_shares
            } else {
                d.inv_max_shares
            };
            let cap = i64::from(cap_shares).saturating_mul(unit_qty);
            // The net after this clip fills completely stays within the cap (a short fill such as
            // 9.99 of 10 shares no longer admits a whole further clip).
            // Tilt (tilt_z, tilt_clip_shares): the leg the score leans to quotes the larger size.
            let clip_shares = if d.tilt_z.0 > 0 && d.tilt_clip_shares > 0 && z.is_some_and(|z| z > d.tilt_z.0)
            {
                d.tilt_clip_shares
            } else {
                d.clip_shares
            };
            let clip = i64::from(clip_shares).saturating_mul(unit_qty);
            // Opening rules (schema v9): a leg that is not short may not raise the net when one holds.
            let opens = own_q >= other_q && (!favoured || d.fav_obey_open);
            // dip_mode: a leg that is not short and not favoured opens only while its dip bid is on.
            // v38 calm favourite (low band): the opening conditions; lo_plain quotes the plain best bid.
            let lo_fav_on = vol_low
                && (d.lo_fav_min_px.0 > 0 || d.lo_fav_max_px.0 > 0 || d.lo_win_er_min.0 > 0 || d.lo_trend || d.lo_plain);
            let lo_plain = lo_fav_on && d.lo_plain;
            let dip_leg = d.dip_mode && own_q >= other_q && !favoured && !lo_plain;
            let thick = d.mm_max_bq > 0
                && book
                    .best(Side::Buy)
                    .is_some_and(|l| l.qty.0 > i64::from(d.mm_max_bq).saturating_mul(unit_qty));
            let early = d.early_ms > 0
                && age_ms < i64::from(d.early_ms)
                && own_q.saturating_sub(other_q).saturating_add(clip)
                    > i64::from(d.early_cap_shares).saturating_mul(unit_qty);
            let dear = d.open_max_px.0 > 0
                && bid.is_some_and(|b| {
                    i128::from(b) * i128::from(ONE) >= i128::from(d.open_max_px.0) * i128::from(unit_px)
                });
            let cheap = d.open_min_px.0 > 0
                && bid.is_some_and(|b| {
                    i128::from(b) * i128::from(ONE) < i128::from(d.open_min_px.0) * i128::from(unit_px)
                });
            let against = d.open_skip_z.0 > 0 && z.is_some_and(|z| z < -d.open_skip_z.0);
            let no_open = opens && ((quiet && !lo_plain) || thick || early || cheap || dear || against);
            // Book imbalance (open_min_imb): a leg that is not short skips a thin own bid side.
            let thin = d.open_min_imb.0 > 0 && own_q >= other_q && {
                let own_bq = book.best(Side::Buy).map_or(0, |l| l.qty.0);
                let oth_bq = self
                    .quotes
                    .get(i ^ 1)
                    .and_then(|o| v.book(o.inst).best(Side::Buy))
                    .map_or(0, |l| l.qty.0);
                let sum = i128::from(own_bq) + i128::from(oth_bq);
                sum > 0 && i128::from(own_bq) * i128::from(ONE) < i128::from(d.open_min_imb.0) * sum
            };
            let no_open = no_open || thin;
            // Favoured extension price cap (fav_max_px).
            let fav_dear = favoured
                && own_q >= other_q
                && d.fav_max_px.0 > 0
                && bid.is_some_and(|b| {
                    i128::from(b) * i128::from(ONE) >= i128::from(d.fav_max_px.0) * i128::from(unit_px)
                });
            let no_open = no_open || fav_dear;
            // Swept pause (swept_pause_ms) and the complement-up pause (oth_up_cancel): no opening
            // or adding.
            let no_open = no_open || (swept && own_q >= other_q) || oth_cool || dropped;
            // open_from_ms: no opening or adding before this window age.
            let no_open = no_open || (d.open_from_ms > 0 && own_q >= other_q && age_ms < i64::from(d.open_from_ms));
            let no_open = no_open || (dip_leg && !dip_active);
            // After a stop-loss pairing bid the window opens nothing more.
            let no_open = no_open || (self.rescued == start && own_q >= other_q);
            // v33 pressure block: own L1 imbalance below ofi_obi_max while the order flow over
            // ofi_win_ms has not turned towards the leg.
            let pressed = d.ofi_win_ms > 0 && own_q >= other_q && {
                let bq = book.best(Side::Buy).map_or(0, |l| l.qty.0);
                let aq = book.best(Side::Sell).map_or(0, |l| l.qty.0);
                let sum = i128::from(bq) + i128::from(aq);
                sum > 0
                    && (i128::from(bq) - i128::from(aq)) * i128::from(ONE) < i128::from(d.ofi_obi_max.0) * sum
                    && self
                        .ofi
                        .get(i)
                        .and_then(|o| o.over(ens, i64::from(d.ofi_win_ms)))
                        .is_some_and(|f| f <= 0)
            };
            let no_open = no_open || pressed;
            // v34 chop scope / trend gate and the low-volatility band's opening conditions.
            let opening = own_q >= other_q;
            let chop_block = opening
                && d.chop_k > 0
                && ((d.chop_all && !chop_ok)
                    || (d.chop_er_min.0 > 0 && !chop_mean.is_some_and(|e| e >= d.chop_er_min.0)));
            let lo_block = opening
                && vol_low
                && ((d.lo_er_max.0 > 0 && !chop_mean.is_some_and(|e| e <= d.lo_er_max.0))
                    || (d.lo_obi_on && {
                        let bq = book.best(Side::Buy).map_or(0, |l| l.qty.0);
                        let aq = book.best(Side::Sell).map_or(0, |l| l.qty.0);
                        let sum = i128::from(bq) + i128::from(aq);
                        sum <= 0
                            || (i128::from(bq) - i128::from(aq)) * i128::from(ONE)
                                < i128::from(d.lo_obi_min.0) * sum
                    }));
            let lo_fav_block = opening && lo_fav_on && {
                let px_ok = |lim: i64, above: bool| {
                    lim <= 0
                        || bid.is_some_and(|b| {
                            let l = i128::from(b) * i128::from(ONE);
                            let r = i128::from(lim) * i128::from(unit_px);
                            if above { l >= r } else { l < r }
                        })
                };
                let leg_dir = if is_up { 1 } else { -1 };
                !px_ok(d.lo_fav_min_px.0, true)
                    || !px_ok(d.lo_fav_max_px.0, false)
                    || (d.lo_from_ms > 0 && age_ms < i64::from(d.lo_from_ms))
                    || (d.lo_to_ms > 0 && age_ms >= i64::from(d.lo_to_ms))
                    || (d.lo_win_er_min.0 > 0 && win_er < d.lo_win_er_min.0)
                    || (d.lo_trend && win_dir != leg_dir)
            };
            let no_open = no_open || chop_block || lo_block || lo_fav_block;
            // v35 trade-activity gate: both legs' prints over trd_win_ms (history too short: no block).
            let quiet = opening && d.trd_win_ms > 0 && {
                let ms = i64::from(d.trd_win_ms);
                let leg = |k: usize| {
                    let inst = self.quotes.get(k).map_or(INST_NONE, |q| q.inst);
                    self.trd.get(k).and_then(|t| if t.inst == inst { t.over(ens, ms) } else { Some(0) })
                };
                match (leg(i), leg(i ^ 1)) {
                    (Some(a), Some(b)) => a.saturating_add(b) < i64::from(d.trd_min),
                    _ => false,
                }
            };
            let no_open = no_open || quiet;
            // v37 Coinbase guard hold (opening bids; with cb_pair every bid of the leg).
            let cb_hold = d.cb_win_ms > 0
                && (own_q >= other_q || d.cb_pair)
                && self.cb_until.get(i).is_some_and(|u| ens < *u);
            let no_open = no_open || cb_hold;
            let target = match (mine && live && !no_open && !no_pair, bid, ask) {
                (true, Some(b), Some(a))
                    if b < a
                        && (!skip || hedge)
                        && b >= lo
                        && b <= hi
                        && own_q.saturating_sub(other_q).saturating_add(clip) <= cap
                        && (!d.fv_limit || fv_cap.is_some() || hedge || pairing) =>
                {
                    // A leg that is not short may rest bid_offset_c cents below the best bid.
                    let b = if other_q > own_q {
                        b
                    } else {
                        b.saturating_sub(i64::from(d.bid_offset_c).saturating_mul(cent))
                    };
                    // Front of the queue (bid_improve_c): a new level above the best bid, below the ask.
                    let up_c = i64::from(d.bid_improve_c).saturating_mul(cent);
                    let b = if up_c > 0 && (!d.bid_improve_short || other_q > own_q) && b.saturating_add(up_c) < a {
                        b.saturating_add(up_c)
                    } else {
                        b
                    };
                    let b = if dip_leg {
                        b.saturating_sub(dip_depth_c.saturating_mul(cent))
                    } else {
                        b
                    };
                    let t = lim.map_or(b, |l| b.min(l));
                    // Lazy pairing (pair_lazy_c): a short leg the score does not favour waits lower.
                    let t = if other_q > own_q && d.pair_lazy_c > 0 && !z.is_some_and(|z| z >= d.pair_eager_z.0) {
                        t.min(b.saturating_sub(i64::from(d.pair_lazy_c).saturating_mul(cent)))
                    } else {
                        t
                    };
                    let t = if hedge || pairing {
                        t
                    } else {
                        fv_cap.map_or(t, |f| t.min(f))
                    };
                    (t >= lo).then_some(t)
                }
                _ => None,
            };
            let since = now.engine_ns.saturating_sub(q.last_ens);
            match q.state {
                QState::Live => {
                    let mut cancel = !(mine && live) || (skip && !hedge) || no_open || no_pair;
                    let mut cooled = false;
                    // v38 sticky pairing: a short leg's bid at the pair-cost limit ignores fair-value moves.
                    let sticky = d.pair_sticky && other_q > own_q && lim.is_some_and(|l| q.px == l);
                    if !cancel && d.fv_cancel.0 > 0 && !pairing && !sticky {
                        let x = self.dfv(&q);
                        if (is_up && x <= -d.fv_cancel.0) || (!is_up && x >= d.fv_cancel.0) {
                            cancel = true;
                            cooled = true;
                        }
                    }
                    // Undercut (a better bid than ours where we would quote higher, the book not
                    // locked or crossed) or above the pair-cost limit; a cleared level (best bid
                    // below ours) keeps the quote.
                    let mut undercut =
                        !locked && bid.is_some_and(|b| b > q.px) && target.is_none_or(|t| t > q.px);
                    // Patient queueing (hold_thin_undercut): only a mature new best bid moves us.
                    if undercut && d.hold_thin_undercut {
                        undercut = (d.undercut_min_bq > 0
                            && bq_ex >= i64::from(d.undercut_min_bq).saturating_mul(unit_qty))
                            || (d.undercut_min_level_ms > 0 && lvl_age_ms >= i64::from(d.undercut_min_level_ms))
                            || (d.hold_below_max_ms > 0
                                && m.below_since > 0
                                && ens.saturating_sub(m.below_since) >= i64::from(d.hold_below_max_ms) * 1_000_000);
                    }
                    // Leave a thinning level (leave_bq).
                    cancel |= d.leave_bq > 0
                        && own_q >= other_q
                        && bid == Some(q.px)
                        && bq_ex < i64::from(d.leave_bq).saturating_mul(unit_qty);
                    cancel |= undercut;
                    cancel |= lim.is_some_and(|l| q.px > l);
                    let hyst = (i128::from(d.fv_hyst.0) * i128::from(unit_px) / i128::from(ONE)) as i64;
                    cancel |= !hedge && !pairing && !sticky && fv_px.is_some_and(|f| q.px > f.saturating_add(hyst));
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
                    // Mature-level admission (open_min_* / pair_min_*).
                    let (mbq, mage) = if other_q > own_q {
                        (d.pair_min_bq, d.pair_min_level_ms)
                    } else {
                        (d.open_min_bq, d.open_min_level_ms)
                    };
                    if (mbq > 0 && bq_ex < i64::from(mbq).saturating_mul(unit_qty))
                        || (mage > 0 && lvl_age_ms < i64::from(mage))
                    {
                        continue;
                    }
                    if since < requote
                        || (!pairing && now.engine_ns < q.cool_until)
                        || cost_full
                        || (!pairing
                            && !(fast && d.fast_free)
                            && q.streak_paused(now.engine_ns, d.streak_k, streak_window, streak_pause))
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
                        qty: Qty(clip),
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
                        m.b0 = self.feat.top.mid_x2;
                        m.d0 = inv_cdf(up_mid);
                        m.den = den;
                        if let Some(x) = self.skind.get_mut(i) {
                            *x = false;
                        }
                        if rescue {
                            self.rescued = start;
                        }
                    }
                }
                _ => {}
            }
        }
        self.settle_run(v, ctx, settling, start, up, w);
    }

    /// Locked settlement-average probability of Up (rule 12) in window `start` of `w` seconds at
    /// UTC `utc_ns`: (p_up, μ, locked seconds), `Rate` but the seconds; `None` without a strike
    /// (at least 80 % of the 60 s before the start recorded), with more than 20 % of the locked
    /// seconds missing, outside the last 60 s, without a reference quote or without σ.
    pub fn settle_prob(&mut self, utc_ns: i64, start: i64, w: i64) -> Option<(i64, i64, i64)> {
        let d = *self.p.defaults();
        if self.settle.win != start {
            self.settle = SettleState {
                win: start,
                ..SettleState::default()
            };
        }
        // settle_src = 1: Chainlink seconds and price instead of the reference.
        let cl = d.settle_src == 1;
        if !self.settle.k_done {
            let (sum, n) = if cl {
                self.cl.sec.sum(start - SETTLE_TWAP_S, start)
            } else {
                self.feat.ref_sec.sum(start - SETTLE_TWAP_S, start)
            };
            self.settle.k_done = true;
            self.settle.k_ok = sum > 0 && n * 5 >= SETTLE_TWAP_S * 4;
            self.settle.k_sum = sum;
            self.settle.k_n = n;
        }
        if !self.settle.k_ok {
            return None;
        }
        let sec = utc_ns.div_euclid(1_000_000_000);
        let lock_from = start + w - SETTLE_TWAP_S;
        let n_lock = sec - lock_from;
        let n_fut = start + w - sec;
        let r_now = if cl { self.cl.now(sec).unwrap_or(0) } else { self.feat.top.mid_x2 };
        if n_lock < 0 || !(1..=SETTLE_TWAP_S).contains(&n_fut) || r_now <= 0 {
            return None;
        }
        let (ls, ln) = if cl {
            self.cl.sec.sum(lock_from, sec)
        } else {
            self.feat.ref_sec.sum(lock_from, sec)
        };
        if ln * 5 < n_lock * 4 {
            return None;
        }
        let sigma = self.feat.vol.sigma(d.vol_min_samples as usize).filter(|s| *s > 0)?;
        let one = i128::from(ONE);
        // A × 60 × 1e9 = (n_lock × mean(locked) + n_fut × R_now) × 1e9; μ = A / K − 1.
        let locked = if ln > 0 {
            i128::from(n_lock) * ls * one / i128::from(ln)
        } else {
            0
        };
        let a60 = locked + i128::from(n_fut) * i128::from(r_now) * one;
        let k60 = i128::from(SETTLE_TWAP_S) * self.settle.k_sum;
        let mu = (a60 * i128::from(self.settle.k_n) / k60 - one) as i64;
        // V(n) = n(n + 1)(2n + 1) / (6 × 60²): variance of the mean of the last n of n unit steps,
        // times (n / 60)²; sd = σ × settle_vol_mult × √V.
        let nf = u128::try_from(n_fut).unwrap_or(0);
        let tw = u128::try_from(SETTLE_TWAP_S).unwrap_or(1);
        let root = isqrt(nf * (nf + 1) * (2 * nf + 1) * 1_000_000_000_000_000_000 / (6 * tw * tw)) as i128;
        let sd = i128::from(sigma) * i128::from(d.settle_vol_mult.0) / one * root / one;
        let p = if sd > 0 {
            cdf((i128::from(mu) * one / sd).clamp(-6 * one, 6 * one) as i64)
        } else if mu >= 0 {
            ONE
        } else {
            0
        };
        Some((p, mu, n_lock))
    }

    /// Rule 12 (settlement-window liquidity) on the current pair (`up` = Up quote index) while
    /// `settling`: one post-only bid of `settle_clip_shares` at the best bid (one cent above with
    /// `settle_improve` when the spread is at least two cents) of the favoured token while its
    /// probability is at least `settle_p_min` and the price is inside the band; cancelled on a
    /// probability below `settle_cancel_p`, a flip of the favourite or an undercut; held to
    /// settlement.
    fn settle_run<V: Views>(&mut self, v: &V, ctx: &mut Ctx<'_>, settling: bool, start: i64, up: usize, w: i64) {
        use hft_strategy_sdk::features::NA;
        if !settling {
            self.settle.out = [NA; 3];
            return;
        }
        let d = *self.p.defaults();
        let now = v.now();
        let prob = self.settle_prob(now.utc_ns, start, w);
        self.settle.out = prob.map_or([NA; 3], |(p, mu, n)| [p, mu, n]);
        let requote = i64::from(d.settle_requote_ms) * 1_000_000;
        // settle_stop_ms: the module stops this long before the window end.
        let left_ms = (start + w) * 1_000 - now.utc_ns.div_euclid(1_000_000);
        let stopped = d.settle_stop_ms > 0 && left_ms < i64::from(d.settle_stop_ms);
        // Up lead in US cents: μ × strike; the strike is 1e-8 USD (Chainlink) or the reference
        // 2 × mid in its price units.
        let ref_unit = if self.reference == INST_NONE {
            1
        } else {
            i128::from(10i64.saturating_pow(u32::from(v.inst(self.reference).px_exp.unsigned_abs())))
        };
        let k_mean = (self.settle.k_ok && self.settle.k_n > 0).then(|| self.settle.k_sum / i128::from(self.settle.k_n));
        let lead_c = prob.zip(k_mean).map(|((_, mu, _), k)| {
            let raw = i128::from(mu) * k / i128::from(ONE);
            (if d.settle_src == 1 {
                raw / 1_000_000
            } else {
                raw * 100 / (2 * ref_unit.max(1))
            }) as i64
        });
        for k in 0..2usize {
            let i = up.wrapping_add(k);
            let Some(q) = self.quotes.get(i).copied() else {
                continue;
            };
            let owned = self.skind.get(i) == Some(&true);
            let hot = *v.inst(q.inst);
            let unit_px = 10i64.saturating_pow(u32::from(hot.px_exp.unsigned_abs()));
            let unit_qty = 10i64.saturating_pow(u32::from(hot.qty_exp.unsigned_abs()));
            let cent = (unit_px / 100).max(1);
            let px_of = |r: i64| (i128::from(r) * i128::from(unit_px) / i128::from(ONE)) as i64;
            let book = v.book(q.inst);
            let live = book.status() == BookStatus::Live;
            let bid = book.best(Side::Buy).map(|l| l.px.0);
            let ask = book.best(Side::Sell).map(|l| l.px.0);
            let locked = matches!((bid, ask), (Some(b), Some(a)) if b >= a);
            // This leg's probability, and whether it is the favourite (Up when p_up ≥ 0.5).
            let p_leg = prob.map(|(p, _, _)| if k == 0 { p } else { ONE - p });
            let fav = prob.is_some_and(|(p, _, _)| (p >= ONE / 2) == (k == 0));
            let hi = px_of(d.settle_bid_hi.0);
            let target = bid.filter(|_| live && !locked).map(|b| {
                if d.settle_improve && ask.is_some_and(|a| a.saturating_sub(b) >= 2 * cent) {
                    b.saturating_add(cent)
                } else {
                    b
                }
            });
            let clip = i64::from(d.settle_clip_shares).saturating_mul(unit_qty);
            // This leg's lead (US cents): projected settlement average less the strike, Down negated.
            let lead = lead_c.map(|l| if k == 0 { l } else { -l });
            let lead_min = i64::from(d.settle_lead_usd).saturating_mul(100);
            let want = fav
                && (d.settle_lead_usd == 0 || lead.is_some_and(|l| l >= lead_min))
                && (d.settle_edge.0 == 0
                    || p_leg.zip(target).is_some_and(|(p, t)| t <= px_of(p.saturating_sub(d.settle_edge.0))))
                && !stopped
                && p_leg.is_some_and(|p| p >= d.settle_p_min.0)
                && target.is_some_and(|t| t >= px_of(d.settle_bid_lo.0) && t <= hi)
                && self.settle.bought.saturating_add(clip) <= i64::from(d.settle_max_shares).saturating_mul(unit_qty)
                && prob.is_some_and(|(_, mu, n)| {
                    n >= i64::from(d.settle_min_lock_s) && mu.unsigned_abs() >= d.settle_mu_min.0.unsigned_abs()
                });
            match q.state {
                QState::Live if owned => {
                    // A cleared level (best bid below ours) keeps the bid.
                    let undercut = !locked && bid.is_some_and(|b| b > q.px);
                    let cancel = !live
                        || !fav
                        || stopped
                        || p_leg.is_none_or(|p| p < d.settle_cancel_p.0)
                        || undercut
                        || q.px > hi
                        || (d.settle_lead_usd > 0 && lead.is_none_or(|l| l.saturating_mul(2) < lead_min))
                        || (d.settle_edge.0 > 0 && p_leg.is_some_and(|p| q.px > px_of(p)));
                    if cancel
                        && let Some(r) = q.r
                        && ctx.cancel(r).is_ok()
                        && let Some(m) = self.quotes.get_mut(i)
                    {
                        m.state = QState::Cancelling;
                        m.last_ens = now.engine_ns;
                    }
                }
                QState::None => {
                    let Some(px) = target.filter(|_| want) else {
                        continue;
                    };
                    if now.engine_ns.saturating_sub(q.last_ens) < requote {
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
                        qty: Qty(clip),
                        link: LegLinkOpt::NONE,
                        tag: settle_tag_of(i as u16),
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
                        m.b0 = self.feat.top.mid_x2;
                        m.d0 = 0;
                        m.den = 0;
                        if let Some(x) = self.skind.get_mut(i) {
                            *x = true;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Implied volatility (reference sigma × `sigma_scale`) at least `sigma_min`, and the
    /// remaining standard deviation (implied sigma × √seconds left) at least `min_rem_sd`.
    /// 2 = at least `sigma_min`, 1 = in the v34 low band [`sigma_lo`, `sigma_min`), 0 = neither.
    fn vol_state(&self, left_ms: i64) -> u8 {
        let d = self.p.defaults();
        let Some(s) = self.feat.vol.sigma(d.vol_min_samples as usize) else {
            return 0;
        };
        let implied = i128::from(s) * i128::from(d.sigma_scale.0) / i128::from(ONE);
        // √(left_ms × 1000) = √(seconds left) × 1000.
        let root_milli = isqrt(u128::try_from(left_ms.max(0)).unwrap_or(0) * 1_000) as i128;
        if implied * root_milli / 1_000 < i128::from(d.min_rem_sd.0) {
            0
        } else if implied >= i128::from(d.sigma_min.0) {
            2
        } else if d.sigma_lo.0 > 0 && implied >= i128::from(d.sigma_lo.0) {
            1
        } else {
            0
        }
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
