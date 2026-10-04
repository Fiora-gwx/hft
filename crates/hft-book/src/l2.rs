//! `L2Book`: layout, `apply_snapshot`, `apply_batch`, `apply_delta`, state transitions.
//!
//! Memory (one `[u64]` region, caller-provided or heap): `qty[2][L]` (i64 bit patterns),
//! `occ[2][L/64]`, overflow prices `ovp[2][cap]` and quantities `ovq[2][cap]` (sorted by price
//! ascending), and a `cap`-entry stash used by re-centering. Hot fields live in `Hot`.

use hft_event::{ConnStateKind, L2BatchHdr, L2Delta, L2SnapHdr, Level};
use hft_types::{Px, Qty, Side};
use zerocopy::FromBytes;

use crate::bitmap;
use crate::snapshot::BookMetaRow;
use crate::{BookErr, BookParams, BookState, Outcome, StaleReason};

pub(crate) const BID: usize = 0;
pub(crate) const ASK: usize = 1;

const IN_MSG: u8 = 1;
const SKIP_MSG: u8 = 2;

pub(crate) const ORIGIN_EXCHANGE: u8 = L2SnapHdr::EXCHANGE;
pub(crate) const ORIGIN_KEYFRAME: u8 = L2SnapHdr::KEYFRAME;
pub(crate) const ORIGIN_TOP_N: u8 = L2SnapHdr::TOP_N;

#[inline]
pub(crate) fn side_ix(s: Side) -> usize {
    match s {
        Side::Buy => BID,
        Side::Sell => ASK,
    }
}

/// Word offsets of the regions inside the book memory.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Geo {
    pub tick: i64,
    /// W.
    pub w: i64,
    /// L = 2W rounded up to a multiple of 64.
    pub l: usize,
    /// Bitmap words per side (L/64).
    pub nw: usize,
    pub cap: usize,
    pub tol: u16,
}

impl Geo {
    /// Clamps illegal parameters into range (only `new` can see them).
    pub(crate) fn new(p: &BookParams) -> Geo {
        let w = p.window.clamp(64, 65_536) as usize;
        let l = (2 * w).next_multiple_of(64);
        Geo {
            tick: p.tick.max(1),
            w: w as i64,
            l,
            nw: l / 64,
            cap: usize::from(p.ovf_cap.max(1)),
            tol: p.crossed_tolerance,
        }
    }
    #[inline]
    pub(crate) fn qty(&self, s: usize) -> usize {
        s * self.l
    }
    #[inline]
    pub(crate) fn occ(&self, s: usize) -> usize {
        2 * self.l + s * self.nw
    }
    #[inline]
    pub(crate) fn ovp(&self, s: usize) -> usize {
        2 * self.l + 2 * self.nw + s * self.cap
    }
    #[inline]
    pub(crate) fn ovq(&self, s: usize) -> usize {
        2 * self.l + 2 * self.nw + (2 + s) * self.cap
    }
    #[inline]
    pub(crate) fn stash_p(&self) -> usize {
        2 * self.l + 2 * self.nw + 4 * self.cap
    }
    #[inline]
    pub(crate) fn stash_q(&self) -> usize {
        2 * self.l + 2 * self.nw + 5 * self.cap
    }
    pub(crate) fn words(&self) -> usize {
        2 * self.l + 2 * self.nw + 6 * self.cap
    }
    pub(crate) fn bytes(&self) -> usize {
        (self.words() * 8).next_multiple_of(64)
    }
    /// Absolute tick index of the window centre (`base + W`), the reference for "farthest".
    #[inline]
    pub(crate) fn center(&self, base: i64) -> i64 {
        base.wrapping_add(self.w)
    }
}

/// Hot fields (one cache line).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Hot {
    /// Absolute tick index of window slot 0.
    pub base: i64,
    /// Window index of the best level per side, −1 = none in the window.
    pub best: [i32; 2],
    pub ovf_len: [u16; 2],
    pub dirty: [bool; 2],
    pub state: BookState,
    pub flags: u8,
    pub last_id: u64,
    pub msg_last_id: u64,
    pub next_idx: u16,
    pub crossed_run: u16,
    pub stale_reason: u8,
    pub origin: u8,
    pub truncated: u32,
    /// Quantity merged out of sight when the overflow area was full.
    pub far_qty: [i64; 2],
}

impl Hot {
    const fn new() -> Hot {
        Hot {
            base: 0,
            best: [-1, -1],
            ovf_len: [0, 0],
            dirty: [false, false],
            state: BookState::Uninit,
            flags: 0,
            last_id: 0,
            msg_last_id: 0,
            next_idx: 0,
            crossed_run: 0,
            stale_reason: 0,
            origin: 0,
            truncated: 0,
            far_qty: [0, 0],
        }
    }
}

enum Mem {
    Heap(Box<[u64]>),
    Ext(&'static mut [u64]),
}

#[inline]
pub(crate) fn rd(w: &[u64], i: usize) -> i64 {
    w.get(i).copied().unwrap_or(0) as i64
}

#[inline]
pub(crate) fn wr(w: &mut [u64], i: usize, v: i64) {
    if let Some(x) = w.get_mut(i) {
        *x = v as u64;
    }
}

#[inline]
pub(crate) fn sl(w: &[u64], off: usize, len: usize) -> &[u64] {
    w.get(off..off + len).unwrap_or(&[])
}

#[inline]
pub(crate) fn sl_mut(w: &mut [u64], off: usize, len: usize) -> &mut [u64] {
    w.get_mut(off..off + len).unwrap_or(&mut [])
}

/// Visible best (absolute tick index, qty) of one side: window best or overflow, whichever is
/// better. Requires settled `best` (no pending `dirty`).
#[inline]
pub(crate) fn best_abs(h: &Hot, g: &Geo, w: &[u64], s: usize) -> Option<(i64, i64)> {
    let win = if h.best[s] >= 0 {
        let r = h.best[s] as usize;
        Some((h.base.wrapping_add(r as i64), rd(w, g.qty(s) + r)))
    } else {
        None
    };
    let n = usize::from(h.ovf_len[s]);
    let ov = if n == 0 {
        None
    } else {
        let k = if s == BID { n - 1 } else { 0 };
        Some((rd(w, g.ovp(s) + k), rd(w, g.ovq(s) + k)))
    };
    match (win, ov) {
        (Some(a), Some(b)) => {
            let b_better = if s == BID { b.0 > a.0 } else { b.0 < a.0 };
            Some(if b_better { b } else { a })
        }
        (a, b) => a.or(b),
    }
}

/// ⌊(a + b) / 2⌋ without overflow.
#[inline]
pub(crate) fn mid_floor(a: i64, b: i64) -> i64 {
    a.div_euclid(2) + b.div_euclid(2) + (a.rem_euclid(2) + b.rem_euclid(2)) / 2
}

#[inline]
fn level_ok(px: i64, qty: i64, tick: i64) -> bool {
    px > 0 && qty >= 0 && px % tick == 0
}

/// Mutable access to one book: hot fields, geometry and memory.
pub(crate) struct Core<'a> {
    pub h: &'a mut Hot,
    pub g: Geo,
    pub w: &'a mut [u64],
}

impl Core<'_> {
    #[inline]
    pub(crate) fn put_win(&mut self, s: usize, rel: usize, q: i64, defer: bool) {
        let g = self.g;
        wr(self.w, g.qty(s) + rel, q);
        bitmap::set(sl_mut(self.w, g.occ(s), g.nw), rel, q != 0);
        let r = rel as i32;
        let b = self.h.best[s];
        if q != 0 {
            let better = if s == BID { r > b } else { b < 0 || r < b };
            if better {
                self.h.best[s] = r;
            }
        } else if r == b {
            if defer {
                self.h.dirty[s] = true;
            } else {
                self.rescan(s);
            }
        }
    }

    /// Re-finds the window best of side `s` from its current (possibly deleted) position.
    #[inline]
    pub(crate) fn rescan(&mut self, s: usize) {
        self.h.dirty[s] = false;
        let b = self.h.best[s];
        if b < 0 {
            return;
        }
        let occ = sl(self.w, self.g.occ(s), self.g.nw);
        let r = if s == BID {
            bitmap::highest_at_or_below(occ, b as usize)
        } else {
            bitmap::lowest_at_or_above(occ, b as usize)
        };
        self.h.best[s] = r.map_or(-1, |x| x as i32);
    }

    #[inline]
    pub(crate) fn settle(&mut self) {
        if self.h.dirty[BID] {
            self.rescan(BID);
        }
        if self.h.dirty[ASK] {
            self.rescan(ASK);
        }
    }

    /// Writes one level at absolute tick index `abs`; returns `true` when the overflow area
    /// truncated a level.
    #[inline]
    pub(crate) fn put(&mut self, s: usize, abs: i64, q: i64, defer: bool) -> bool {
        let rel = abs.wrapping_sub(self.h.base);
        if (rel as u64) < self.g.l as u64 {
            self.put_win(s, rel as usize, q, defer);
            false
        } else {
            let c = self.g.center(self.h.base);
            self.put_ovf(s, abs, q, c)
        }
    }

    #[inline]
    pub(crate) fn best(&self, s: usize) -> Option<(i64, i64)> {
        best_abs(self.h, &self.g, self.w, s)
    }

    /// Empties both sides (window cleared through the bitmap, not a full memset).
    pub(crate) fn clear_levels(&mut self) {
        let g = self.g;
        for s in [BID, ASK] {
            for k in 0..g.nw {
                let mut word = rd(self.w, g.occ(s) + k) as u64;
                while word != 0 {
                    let b = word.trailing_zeros() as usize;
                    word &= word - 1;
                    wr(self.w, g.qty(s) + k * 64 + b, 0);
                }
                wr(self.w, g.occ(s) + k, 0);
            }
            self.h.ovf_len[s] = 0;
            self.h.best[s] = -1;
            self.h.dirty[s] = false;
            self.h.far_qty[s] = 0;
        }
    }

    fn go_stale(&mut self, r: StaleReason) -> Outcome {
        let was_live = self.h.state == BookState::Live;
        self.h.state = BookState::Stale;
        self.h.stale_reason = r as u8;
        self.h.flags = 0;
        if was_live {
            Outcome::WENT_STALE
        } else {
            Outcome::NONE
        }
    }

    /// End of a message: crossed check, then re-centering.
    fn finish_msg(&mut self) -> Outcome {
        let mut out = Outcome::NONE;
        match (self.best(BID), self.best(ASK)) {
            (Some(b), Some(a)) if b.0 >= a.0 => {
                self.h.crossed_run = self.h.crossed_run.saturating_add(1);
                out |= Outcome::CROSSED;
                if self.h.crossed_run > self.g.tol {
                    out |= self.go_stale(StaleReason::Crossed);
                }
            }
            _ => self.h.crossed_run = 0,
        }
        out | self.recenter_check()
    }

    #[inline]
    fn tops(&self) -> [Option<(i64, i64)>; 2] {
        [self.best(BID), self.best(ASK)]
    }

    #[inline]
    fn top_bits(&self, before: [Option<(i64, i64)>; 2]) -> Outcome {
        let after = self.tops();
        let mut o = Outcome::NONE;
        if before[BID] != after[BID] {
            o |= Outcome::BID_TOP;
        }
        if before[ASK] != after[ASK] {
            o |= Outcome::ASK_TOP;
        }
        o
    }

    pub(crate) fn apply_delta(&mut self, d: &L2Delta) -> Outcome {
        if self.h.origin == ORIGIN_TOP_N {
            return Outcome::INVALID;
        }
        if self.h.state != BookState::Live {
            return Outcome::NOT_LIVE;
        }
        let last = d.dflags & L2Delta::LAST != 0;
        if d.idx == 0 {
            if self.h.flags & IN_MSG != 0 {
                return Outcome::INVALID | self.go_stale(StaleReason::Split);
            }
            self.h.flags &= !SKIP_MSG;
            if d.last_id <= self.h.last_id {
                if !last {
                    self.h.flags |= SKIP_MSG;
                }
                return Outcome::DUPLICATE;
            }
            if d.prev_id != self.h.last_id {
                return Outcome::GAP | self.go_stale(StaleReason::Gap);
            }
            self.h.msg_last_id = d.last_id;
            self.h.next_idx = 0;
            self.h.flags |= IN_MSG;
        } else {
            if self.h.flags & SKIP_MSG != 0 {
                if last {
                    self.h.flags &= !SKIP_MSG;
                }
                return Outcome::DUPLICATE;
            }
            if self.h.flags & IN_MSG == 0 || d.idx != self.h.next_idx || d.last_id != self.h.msg_last_id {
                return Outcome::INVALID | self.go_stale(StaleReason::Split);
            }
        }
        if !level_ok(d.px.0, d.qty.0, self.g.tick) {
            return Outcome::INVALID | self.go_stale(StaleReason::BadLevel);
        }
        self.h.next_idx = d.idx.wrapping_add(1);
        let before = self.tops();
        let s = side_ix(d.side);
        let mut out = Outcome::APPLIED;
        if self.put(s, d.px.0 / self.g.tick, d.qty.0, false) {
            out |= Outcome::TRUNCATED;
        }
        if last {
            self.h.flags &= !IN_MSG;
            self.h.last_id = self.h.msg_last_id;
            out |= self.finish_msg();
        }
        out | self.top_bits(before)
    }

    pub(crate) fn apply_batch(&mut self, hd: &L2BatchHdr, levels: &[Level]) -> Outcome {
        if self.h.origin == ORIGIN_TOP_N {
            return Outcome::INVALID;
        }
        if self.h.state != BookState::Live {
            return Outcome::NOT_LIVE;
        }
        if self.h.flags & IN_MSG != 0 {
            return Outcome::INVALID | self.go_stale(StaleReason::Split);
        }
        self.h.flags = 0;
        if hd.last_id <= self.h.last_id {
            return Outcome::DUPLICATE;
        }
        if hd.prev_id != self.h.last_id {
            return Outcome::GAP | self.go_stale(StaleReason::Gap);
        }
        let nb = usize::from(hd.n_bids);
        if levels.len() != nb + usize::from(hd.n_asks)
            || !levels.iter().all(|l| level_ok(l.px.0, l.qty.0, self.g.tick))
        {
            return Outcome::INVALID | self.go_stale(StaleReason::BadLevel);
        }
        let before = self.tops();
        let tick = self.g.tick;
        let mut out = Outcome::APPLIED;
        let mut trunc = false;
        for (k, l) in levels.iter().enumerate() {
            let s = if k < nb { BID } else { ASK };
            trunc |= self.put(s, l.px.0 / tick, l.qty.0, true);
        }
        self.settle();
        if trunc {
            out |= Outcome::TRUNCATED;
        }
        self.h.last_id = hd.last_id;
        out |= self.finish_msg();
        out | self.top_bits(before)
    }

    pub(crate) fn apply_snapshot(&mut self, hd: &L2SnapHdr, levels: &[Level]) -> Outcome {
        let origin = hd.origin;
        if !matches!(origin, ORIGIN_EXCHANGE | ORIGIN_KEYFRAME | ORIGIN_TOP_N) {
            return Outcome::INVALID;
        }
        if origin == ORIGIN_TOP_N && hd.snap_id != 0 && hd.snap_id <= self.h.last_id {
            return Outcome::DUPLICATE;
        }
        let tick = self.g.tick;
        let nb = usize::from(hd.n_bids);
        let mut ok = levels.len() == nb + usize::from(hd.n_asks);
        let (mut bb, mut ba): (Option<i64>, Option<i64>) = (None, None);
        for (k, l) in levels.iter().enumerate() {
            if l.qty.0 == 0 {
                continue;
            }
            if !level_ok(l.px.0, l.qty.0, tick) {
                ok = false;
                break;
            }
            let i = l.px.0 / tick;
            if k < nb {
                bb = Some(bb.map_or(i, |b| b.max(i)));
            } else {
                ba = Some(ba.map_or(i, |a| a.min(i)));
            }
        }
        let keyframe_restore = origin == ORIGIN_KEYFRAME && self.h.state != BookState::Uninit;
        let reject = if !ok {
            Some(StaleReason::BadLevel)
        } else if origin != ORIGIN_KEYFRAME && matches!((bb, ba), (Some(b), Some(a)) if b >= a) {
            Some(StaleReason::Crossed)
        } else {
            None
        };
        if let Some(r) = reject {
            return Outcome::INVALID | self.go_stale(r);
        }
        self.clear_levels();
        if !keyframe_restore {
            let m = match (bb, ba) {
                (Some(b), Some(a)) => Some(mid_floor(b, a)),
                (b, a) => b.or(a),
            };
            if let Some(m) = m {
                let w = self.g.w;
                let rel = m.wrapping_sub(self.h.base);
                let keep = origin == ORIGIN_TOP_N && rel >= w / 2 && rel < 3 * w / 2;
                if !keep {
                    self.h.base = m.wrapping_sub(w);
                }
            }
        }
        let mut trunc = false;
        for (k, l) in levels.iter().enumerate() {
            if l.qty.0 == 0 {
                continue;
            }
            let s = if k < nb { BID } else { ASK };
            trunc |= self.put(s, l.px.0 / tick, l.qty.0, true);
        }
        self.settle();
        self.h.last_id = hd.snap_id;
        self.h.flags = 0;
        let mut out = Outcome::APPLIED;
        if trunc {
            out |= Outcome::TRUNCATED;
        }
        if keyframe_restore {
            return out;
        }
        self.h.origin = origin;
        self.h.crossed_run = 0;
        self.h.stale_reason = 0;
        if self.h.state != BookState::Live {
            self.h.state = BookState::Live;
            out |= Outcome::WENT_LIVE;
        }
        out
    }

    pub(crate) fn on_conn(&mut self, k: ConnStateKind) -> Outcome {
        match (k, self.h.state) {
            (ConnStateKind::Down, BookState::Live | BookState::Resyncing) => {
                self.go_stale(StaleReason::ConnDown)
            }
            (ConnStateKind::Resync, BookState::Live | BookState::Stale) => {
                let was_live = self.h.state == BookState::Live;
                self.h.state = BookState::Resyncing;
                self.h.flags = 0;
                if was_live {
                    Outcome::WENT_STALE
                } else {
                    Outcome::NONE
                }
            }
            _ => Outcome::NONE,
        }
    }
}

/// Dense-array L2 book of one inst (README "API").
pub struct L2Book {
    pub(crate) h: Hot,
    pub(crate) g: Geo,
    mem: Mem,
}

impl core::fmt::Debug for L2Book {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("L2Book")
            .field("state", &self.h.state)
            .field("last_id", &self.h.last_id)
            .field("base", &self.h.base)
            .field("window", &self.g.w)
            .field("best_bid", &self.best_bid())
            .field("best_ask", &self.best_ask())
            .finish_non_exhaustive()
    }
}

impl L2Book {
    /// Bytes of caller memory one book needs (a multiple of 64).
    pub fn bytes_needed(p: &BookParams) -> usize {
        Geo::new(p).bytes()
    }

    /// Book in caller memory (huge-page arena); `mem` must start on a 64 B boundary. The used
    /// prefix of `mem` is zeroed here.
    pub fn new_in(p: &BookParams, mem: &'static mut [u8]) -> Result<L2Book, BookErr> {
        if !p.is_valid() {
            return Err(BookErr::BadParams);
        }
        let g = Geo::new(p);
        let words = carve(mem, g.bytes())?;
        Ok(L2Book::from_words(g, words))
    }

    /// Heap-allocated book (tests, replay, PyO3). Illegal parameters are clamped into range
    /// (`tick ≥ 1`, `window ∈ [64, 65536]`, `ovf_cap ≥ 1`).
    pub fn new(p: &BookParams) -> L2Book {
        let g = Geo::new(p);
        L2Book {
            h: Hot::new(),
            g,
            mem: Mem::Heap(vec![0u64; g.bytes() / 8].into_boxed_slice()),
        }
    }

    pub(crate) fn from_words(g: Geo, words: &'static mut [u64]) -> L2Book {
        L2Book {
            h: Hot::new(),
            g,
            mem: Mem::Ext(words),
        }
    }

    pub(crate) fn geo_of(p: &BookParams) -> Geo {
        Geo::new(p)
    }

    #[inline]
    pub(crate) fn words(&self) -> &[u64] {
        match &self.mem {
            Mem::Heap(b) => b,
            Mem::Ext(s) => s,
        }
    }

    #[inline]
    pub(crate) fn core(&mut self) -> Core<'_> {
        let w: &mut [u64] = match &mut self.mem {
            Mem::Heap(b) => b,
            Mem::Ext(s) => s,
        };
        Core {
            h: &mut self.h,
            g: self.g,
            w,
        }
    }

    /// Whole-message snapshot application (Exchange / TopN replace, Keyframe bootstrap).
    pub fn apply_snapshot(&mut self, h: &L2SnapHdr, levels: &[Level]) -> Outcome {
        self.core().apply_snapshot(h, levels)
    }

    /// One exchange message applied atomically.
    pub fn apply_batch(&mut self, h: &L2BatchHdr, levels: &[Level]) -> Outcome {
        self.core().apply_batch(h, levels)
    }

    /// One level of a split message; same checks as `apply_batch`.
    pub fn apply_delta(&mut self, d: &L2Delta) -> Outcome {
        self.core().apply_delta(d)
    }

    /// Down: Live/Resyncing → Stale(ConnDown); Resync: Live/Stale → Resyncing.
    pub fn on_conn(&mut self, k: ConnStateKind) -> Outcome {
        self.core().on_conn(k)
    }

    /// Any state but `Uninit` → `Stale(r)` (overwrites the reason).
    pub fn mark_stale(&mut self, r: StaleReason) {
        if self.h.state != BookState::Uninit {
            self.h.state = BookState::Stale;
            self.h.stale_reason = r as u8;
            self.h.flags = 0;
        }
    }

    #[inline]
    pub fn state(&self) -> BookState {
        self.h.state
    }

    #[inline]
    pub fn last_id(&self) -> u64 {
        self.h.last_id
    }

    #[inline]
    pub fn params(&self) -> BookParams {
        BookParams {
            tick: self.g.tick,
            window: self.g.w as u32,
            ovf_cap: self.g.cap as u16,
            crossed_tolerance: self.g.tol,
        }
    }

    #[inline]
    fn best_level(&self, s: usize) -> Option<Level> {
        best_abs(&self.h, &self.g, self.words(), s).map(|(i, q)| Level {
            px: Px(i.wrapping_mul(self.g.tick)),
            qty: Qty(q),
        })
    }

    /// Best visible bid (window and overflow).
    #[inline]
    pub fn best_bid(&self) -> Option<Level> {
        self.best_level(BID)
    }

    /// Best visible ask (window and overflow).
    #[inline]
    pub fn best_ask(&self) -> Option<Level> {
        self.best_level(ASK)
    }

    /// Section 6 row of this book (`inst` = 0; `BookSet` fills it).
    pub fn meta(&self) -> BookMetaRow {
        BookMetaRow {
            inst: hft_types::InstId(0),
            state: self.h.state as u8,
            stale_reason: self.h.stale_reason,
            origin: self.h.origin,
            _pad0: 0,
            last_id: self.h.last_id,
            base: self.h.base,
            truncated: self.h.truncated,
            crossed_run: self.h.crossed_run,
            _pad1: [0; 2],
        }
    }

    /// Restores the metadata of a section 6 row (levels are cleared; a Keyframe refills them).
    pub(crate) fn restore_meta(&mut self, r: &BookMetaRow, state: BookState) {
        let mut c = self.core();
        c.clear_levels();
        c.h.state = state;
        c.h.stale_reason = r.stale_reason;
        c.h.origin = r.origin;
        c.h.last_id = r.last_id;
        c.h.base = r.base;
        c.h.truncated = r.truncated;
        c.h.crossed_run = r.crossed_run;
        c.h.flags = 0;
        c.h.msg_last_id = 0;
        c.h.next_idx = 0;
    }
}

/// Checks alignment and size, zeroes the used prefix and views it as words.
pub(crate) fn carve(mem: &'static mut [u8], n: usize) -> Result<&'static mut [u64], BookErr> {
    if n == 0 {
        return Ok(&mut []);
    }
    if !(mem.as_ptr() as usize).is_multiple_of(64) {
        return Err(BookErr::MemAlign);
    }
    if mem.len() < n {
        return Err(BookErr::MemTooSmall);
    }
    let (head, _) = mem.split_at_mut(n);
    head.fill(0);
    <[u64]>::mut_from_bytes(head).map_err(|_| BookErr::MemAlign)
}
