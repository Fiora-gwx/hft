//! Pre-signed orders (README "预签名"): in the gw's idle time, sign the orders a `Place` is likely
//! to need — per inst, the side and size of its last place at prices around that place's price —
//! so that the `Place` only fills in the request body. With the price grid on, also a buy of the
//! configured size at every multiple of the step inside (0, 1), for every bound inst. Each
//! pre-signed order is used at most once (its hash, once posted or cancelled, is dead);
//! `timestamp` (the V2 nonce) is unique per account.

use hft_refdata::RefData;
use hft_types::{InstId, Side};
use hft_venue_proto::{Credential, WallClock};

use crate::orders::{Templates, amounts, l2_keys, sign_v2, signer};
use crate::reports::GwCtx;

/// Prices kept per inst at most (`below + 1 + above`).
pub const PRESIGN_PRICES: usize = 8;
/// Pre-signed orders kept per account (allocated when pre-signing is enabled).
const SLOTS: usize = 256;
/// Insts with a price anchor per account.
const ANCHORS: usize = 64;
/// Prices of one inst's price grid at most (`1 / step − 1`).
pub const GRID_PRICES: usize = 128;
/// Insts with a price-grid row per account (allocated when the grid is enabled).
const GRID_INSTS: usize = 16;
/// Grid entries one idle step looks at, at most.
const GRID_SCAN: usize = 64;
/// Lifetime bounds of a pre-signed order (ms): its `timestamp` is its signing time; the GTD
/// `expiration` is not signed and is set when it is placed.
const AGE_MIN_MS: u32 = 1_000;
const AGE_MAX_MS: u32 = 240_000;

/// Pre-signing configuration of one account (README "预签名"). Absent = off.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Presign {
    /// Prices below / above the anchor price, in `step`s.
    pub below: u8,
    pub above: u8,
    /// Price step in 1e-6 USDC (0.01 = 10 000); must be a whole number of the inst's price units.
    pub step_e6: u32,
    /// A pre-signed order older than this is not used; it is re-signed from 3/4 of it on.
    pub max_age_ms: u32,
    /// Price grid (README "预签名"): size in 1e-6 shares of the buy kept pre-signed at every
    /// multiple of `step_e6` inside (0, 1) for every bound inst; 0 = off.
    pub grid_qty_e6: u32,
}

impl Presign {
    /// `None` when the window exceeds `PRESIGN_PRICES`, the step is 0 or the age is outside
    /// 1 000–240 000 ms. The price grid is off (see `with_grid`).
    pub fn new(below: u8, above: u8, step_e6: u32, max_age_ms: u32) -> Option<Presign> {
        let ok = usize::from(below) + usize::from(above) < PRESIGN_PRICES
            && step_e6 > 0
            && (AGE_MIN_MS..=AGE_MAX_MS).contains(&max_age_ms);
        ok.then_some(Presign {
            below,
            above,
            step_e6,
            max_age_ms,
            grid_qty_e6: 0,
        })
    }

    /// Turns the price grid on for buys of `qty_e6` (1e-6 shares). `None` when `qty_e6` is 0 or
    /// `step_e6` does not divide 1 USDC into 2 to `GRID_PRICES + 1` steps.
    pub fn with_grid(self, qty_e6: u32) -> Option<Presign> {
        let p = Presign {
            grid_qty_e6: qty_e6,
            ..self
        };
        (p.grid_prices() > 0).then_some(p)
    }

    /// Prices of one inst's grid (`1 / step − 1`); 0 = the grid is off.
    fn grid_prices(&self) -> usize {
        if self.grid_qty_e6 == 0 || self.step_e6 == 0 || 1_000_000 % self.step_e6 != 0 {
            return 0;
        }
        let n = (1_000_000 / self.step_e6) as usize - 1;
        if n <= GRID_PRICES { n } else { 0 }
    }
}

/// One pre-signed order.
#[derive(Copy, Clone, Debug)]
pub(crate) struct Pre {
    /// 0 = empty.
    pub inst: InstId,
    pub token: [u8; 32],
    pub side: Side,
    pub px: i64,
    pub qty: i64,
    pub salt: u64,
    pub ts_ms: u64,
    pub hash: [u8; 32],
    pub sig_len: u16,
    pub sig: [u8; 512],
}

impl Default for Pre {
    fn default() -> Self {
        Pre {
            inst: InstId(0),
            token: [0; 32],
            side: Side::Buy,
            px: 0,
            qty: 0,
            salt: 0,
            ts_ms: 0,
            hash: [0; 32],
            sig_len: 0,
            sig: [0; 512],
        }
    }
}

impl Pre {
    fn is(&self, inst: InstId, token: &[u8; 32], side: Side, px: i64, qty: i64) -> bool {
        self.inst == inst
            && self.inst.0 != 0
            && self.token == *token
            && self.side == side
            && self.px == px
            && self.qty == qty
    }
}

/// The last place of an inst: the centre of its pre-signed prices.
#[derive(Copy, Clone, Debug)]
struct Anchor {
    inst: InstId,
    side: Side,
    px: i64,
    qty: i64,
}

impl Default for Anchor {
    fn default() -> Self {
        Anchor {
            inst: InstId(0),
            side: Side::Buy,
            px: 0,
            qty: 0,
        }
    }
}

/// One inst of the price grid.
#[derive(Copy, Clone, Debug)]
struct GridRow {
    /// 0 = free.
    inst: InstId,
    /// The grid's price step and order size in the inst's price / quantity units; 0 = not known
    /// yet (the idle step computes them from refdata after every bind).
    step: i64,
    qty: i64,
}

const FREE_ROW: GridRow = GridRow {
    inst: InstId(0),
    step: 0,
    qty: 0,
};

/// Price-grid orders of one account (README "预签名"): `GRID_INSTS` rows of `n` entries; entry
/// `k` of a row is the buy of the grid size at price `(k + 1) × step` of the row's inst.
#[derive(Clone, Debug)]
struct Grid {
    /// Prices per row (at least 1).
    n: usize,
    rows: [GridRow; GRID_INSTS],
    /// Signing time (ms) of every entry, 0 = empty: all the idle scan reads.
    ts: Box<[u64]>,
    orders: Box<[Pre]>,
    /// Entry the idle scan looks at next (round-robin).
    cursor: usize,
    /// Entries left in the scan's current pass over `ts`.
    left: usize,
    /// Earliest refresh time (ms) of the entries seen in the current pass.
    due: u64,
    /// A pass found nothing to sign: no entry needs signing before this time (ms).
    idle_until: u64,
}

impl Grid {
    fn new(n: usize) -> Grid {
        let n = n.max(1);
        Grid {
            n,
            rows: [FREE_ROW; GRID_INSTS],
            ts: vec![0; GRID_INSTS * n].into_boxed_slice(),
            orders: vec![Pre::default(); GRID_INSTS * n].into_boxed_slice(),
            cursor: 0,
            left: GRID_INSTS * n,
            due: u64::MAX,
            idle_until: 0,
        }
    }

    /// An entry was emptied or signed: the idle scan starts a new pass at once.
    fn rescan(&mut self) {
        self.left = self.ts.len();
        self.due = u64::MAX;
        self.idle_until = 0;
    }

    /// The idle scan passes `m` entries.
    fn skip(&mut self, m: usize) {
        self.cursor = (self.cursor + m) % self.ts.len();
        self.left = self.left.saturating_sub(m);
    }

    fn row_of(&self, inst: InstId) -> Option<usize> {
        if inst.0 == 0 {
            return None;
        }
        self.rows.iter().position(|r| r.inst == inst)
    }

    /// Empties every entry of row `r`.
    fn clear(&mut self, r: usize) {
        let (a, b) = (r * self.n, (r + 1) * self.n);
        if let Some(ts) = self.ts.get_mut(a..b) {
            ts.fill(0);
        }
        if let Some(orders) = self.orders.get_mut(a..b) {
            orders.iter_mut().for_each(|p| p.inst = InstId(0));
        }
    }

    /// The entry of the order (`side`, `px`, `qty`) of `inst`, when the grid covers it: a buy of
    /// the grid size at a grid price of an inst with a row whose units are known.
    fn index(&self, inst: InstId, side: Side, px: i64, qty: i64) -> Option<usize> {
        if side != Side::Buy {
            return None;
        }
        let r = self.row_of(inst)?;
        let row = self.rows.get(r)?;
        if row.step <= 0 || row.qty <= 0 || qty != row.qty || px <= 0 || px % row.step != 0 {
            return None;
        }
        let k = usize::try_from(px / row.step).ok()?;
        (k <= self.n).then(|| r * self.n + k - 1)
    }
}

/// Pre-signing state of one account (part of `GwCtx`).
#[derive(Clone, Debug)]
pub(crate) struct Presigned {
    cfg: Option<Presign>,
    anchors: [Anchor; ANCHORS],
    slots: Box<[Pre]>,
    /// The price grid; `None` = off.
    grid: Option<Grid>,
    /// Last `timestamp` signed (ms): every order gets a later one.
    last_ts: u64,
    /// Anchor the idle step looks at first (round-robin).
    next: usize,
}

impl Default for Presigned {
    fn default() -> Self {
        Presigned {
            cfg: None,
            anchors: [Anchor::default(); ANCHORS],
            slots: Box::default(),
            grid: None,
            last_ts: 0,
            next: 0,
        }
    }
}

impl Presigned {
    /// Startup only: turns pre-signing on (allocates the order store, and the price grid when it
    /// is on).
    pub(crate) fn enable(&mut self, p: Presign) {
        self.cfg = Some(p);
        self.slots = vec![Pre::default(); SLOTS].into_boxed_slice();
        let n = p.grid_prices();
        self.grid = (n > 0).then(|| Grid::new(n));
    }

    /// `inst` was bound to a token (`bound`) or unbound: a bound inst gets an empty grid row —
    /// its own, else a free one, else none (its orders are signed at the place) — whose units the
    /// idle step computes; an unbound inst frees its row.
    pub(crate) fn grid_bind(&mut self, inst: InstId, bound: bool) {
        let Some(g) = self.grid.as_mut() else {
            return;
        };
        let mut r = g.row_of(inst);
        if r.is_none() && bound && inst.0 != 0 {
            r = g.rows.iter().position(|x| x.inst.0 == 0);
        }
        let Some(r) = r else {
            return;
        };
        g.clear(r);
        if let Some(row) = g.rows.get_mut(r) {
            *row = GridRow {
                inst: if bound { inst } else { InstId(0) },
                ..FREE_ROW
            };
        }
        if bound {
            // The idle step signs the new row first.
            g.cursor = r * g.n;
        }
        g.rescan();
    }

    /// A unique order `timestamp` (ms): `now_ms`, or one past the last one signed.
    pub(crate) fn next_ts(&mut self, now_ms: u64) -> u64 {
        let ts = now_ms.max(self.last_ts.saturating_add(1));
        self.last_ts = ts;
        ts
    }

    /// The place of `inst` at (`side`, `px`, `qty`) centres its pre-signed prices; not when the
    /// price grid covers the order (the grid signs it, at every price).
    pub(crate) fn anchor(&mut self, inst: InstId, side: Side, px: i64, qty: i64) {
        if self.cfg.is_none() {
            return;
        }
        if let Some(g) = &self.grid
            && g.index(inst, side, px, qty).is_some()
        {
            return;
        }
        let a = Anchor { inst, side, px, qty };
        if let Some(x) = self.anchors.iter_mut().find(|x| x.inst == inst) {
            *x = a;
        } else if let Some(x) = self.anchors.iter_mut().find(|x| x.inst.0 == 0) {
            *x = a;
        }
    }

    /// Takes (uses up) the pre-signed order for exactly this order, if one is younger than
    /// `max_age_ms`: the price grid's entry when the grid covers the order, else an anchored one.
    pub(crate) fn take(
        &mut self,
        inst: InstId,
        token: &[u8; 32],
        side: Side,
        px: i64,
        qty: i64,
        now_ms: u64,
    ) -> Option<Pre> {
        let max = u64::from(self.cfg?.max_age_ms);
        if let Some(g) = self.grid.as_mut()
            && let Some(i) = g.index(inst, side, px, qty)
            && let (Some(ts), Some(s)) = (g.ts.get_mut(i), g.orders.get_mut(i))
            && s.is(inst, token, side, px, qty)
            && now_ms.saturating_sub(s.ts_ms) <= max
        {
            let p = *s;
            s.inst = InstId(0);
            *ts = 0;
            // The idle step signs this price again, first.
            g.cursor = i;
            g.rescan();
            return Some(p);
        }
        let s = self
            .slots
            .iter_mut()
            .find(|p| p.is(inst, token, side, px, qty) && now_ms.saturating_sub(p.ts_ms) <= max)?;
        let p = *s;
        s.inst = InstId(0);
        Some(p)
    }

    /// `inst` was bound to another token (or unbound): its anchor and orders are void.
    pub(crate) fn unbind(&mut self, inst: InstId) {
        for a in self.anchors.iter_mut().filter(|a| a.inst == inst) {
            a.inst = InstId(0);
        }
        for p in self.slots.iter_mut().filter(|p| p.inst == inst) {
            p.inst = InstId(0);
        }
        if let Some(g) = self.grid.as_mut()
            && let Some(r) = g.row_of(inst)
        {
            g.clear(r);
            g.rescan();
        }
    }

    /// Pre-signed orders held now (tests, telemetry).
    pub(crate) fn held(&self) -> usize {
        let grid = self
            .grid
            .as_ref()
            .map_or(0, |g| g.ts.iter().filter(|t| **t != 0).count());
        self.slots.iter().filter(|p| p.inst.0 != 0).count() + grid
    }

    /// Where a new order goes: its own earlier copy, an empty slot, else the oldest.
    fn slot_for(
        &mut self,
        inst: InstId,
        token: &[u8; 32],
        side: Side,
        px: i64,
        qty: i64,
    ) -> Option<&mut Pre> {
        let i = self
            .slots
            .iter()
            .position(|p| p.is(inst, token, side, px, qty))
            .or_else(|| self.slots.iter().position(|p| p.inst.0 == 0))
            .or_else(|| {
                self.slots
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, p)| p.ts_ms)
                    .map(|(i, _)| i)
            })?;
        self.slots.get_mut(i)
    }
}

/// `v_e6` (1e-6 USDC, or 1e-6 shares) in units of exponent `exp` (an inst's price, or quantity,
/// units); `None` when not a whole number.
fn step_px(v_e6: u32, exp: i8) -> Option<i64> {
    let e = -6 - i32::from(exp);
    let v = i64::from(v_e6);
    if e >= 0 {
        v.checked_mul(10i64.checked_pow(e.unsigned_abs())?)
    } else {
        let p = 10i64.checked_pow(e.unsigned_abs())?;
        (v % p == 0).then_some(v / p)
    }
}

/// Signs the order (`side`, `px`, `qty`) of `inst` now, with the account's next `timestamp`;
/// `amt` = its side code and amounts (`amounts`). `None` when the credential does not sign for
/// the account.
#[allow(clippy::too_many_arguments)]
fn sign_pre(
    t: &Templates,
    pre: &mut Presigned,
    cred: &Credential,
    now: u64,
    inst: InstId,
    token: &[u8; 32],
    side: Side,
    px: i64,
    qty: i64,
    amt: (u8, u64, u64),
) -> Option<Pre> {
    let w = l2_keys(cred)
        .ok()
        .filter(|k| k.deposit == t.wallet)
        .and_then(|k| signer(&k, t))?;
    let ts = pre.next_ts(now);
    // Unique through `timestamp`; no slot is known yet (README "预签名").
    let salt = (ts & ((1 << 33) - 1)) << 20;
    let mut sig = [0u8; 512];
    let (hash, n) = sign_v2(&w, t, token, amt.0, amt.1, amt.2, salt, ts, &mut sig);
    Some(Pre {
        inst,
        token: *token,
        side,
        px,
        qty,
        salt,
        ts_ms: ts,
        hash,
        sig_len: n as u16,
        sig,
    })
}

/// The uint256 token id bound to `inst`.
fn token_id(ctx: &GwCtx, inst: InstId) -> Option<[u8; 32]> {
    ctx.token_of(inst)
        .and_then(|s| core::str::from_utf8(s).ok())
        .and_then(crate::sign::parse_u256)
}

/// One unit of idle work (`VenueProtocol::gw_idle`): signs the first missing or ageing order of
/// the anchors' price windows, else of the price grid; `false` when it signed nothing (every
/// order is fresh, or pre-signing is off).
pub(crate) fn step(rd: &RefData, t: &Templates, ctx: &mut GwCtx, cred: &Credential, clk: &WallClock) -> bool {
    let Some(cfg) = ctx.pre.cfg else {
        return false;
    };
    let Ok(now) = u64::try_from(clk.unix_ms) else {
        return false;
    };
    anchor_step(rd, t, ctx, cred, &cfg, now) || grid_step(rd, t, ctx, cred, &cfg, now)
}

/// Signs the first missing or ageing order of the anchors' price windows.
fn anchor_step(
    rd: &RefData,
    t: &Templates,
    ctx: &mut GwCtx,
    cred: &Credential,
    cfg: &Presign,
    now: u64,
) -> bool {
    let refresh = u64::from(cfg.max_age_ms) * 3 / 4;
    for i in 0..ANCHORS {
        let ai = (ctx.pre.next + i) % ANCHORS;
        let a = ctx.pre.anchors[ai];
        if a.inst.0 == 0 {
            continue;
        }
        let row = rd.inst(a.inst);
        let Some(step) = step_px(cfg.step_e6, row.px_exp) else {
            continue;
        };
        // Price 1 (the payout) in price units: valid prices are strictly between 0 and it.
        let Some(one) = 10i64.checked_pow(u32::from(row.px_exp.unsigned_abs())) else {
            continue;
        };
        let Some(token) = token_id(ctx, a.inst) else {
            continue;
        };
        for k in -i64::from(cfg.below)..=i64::from(cfg.above) {
            let px = a.px + k * step;
            if px <= 0 || px >= one {
                continue;
            }
            let fresh =
                ctx.pre.slots.iter().any(|p| {
                    p.is(a.inst, &token, a.side, px, a.qty) && now.saturating_sub(p.ts_ms) < refresh
                });
            if fresh {
                continue;
            }
            let Some(amt) = amounts(rd, a.inst, a.side, px, a.qty) else {
                continue;
            };
            let Some(p) = sign_pre(t, &mut ctx.pre, cred, now, a.inst, &token, a.side, px, a.qty, amt) else {
                return false;
            };
            ctx.pre.next = ai;
            if let Some(s) = ctx.pre.slot_for(a.inst, &token, a.side, px, a.qty) {
                *s = p;
            }
            return true;
        }
    }
    false
}

/// What the grid scan does with the entry under its cursor.
enum Scan {
    /// Fresh, or nothing to sign there: pass this many entries.
    Skip(usize),
    /// Sign entry `i` of row `r`: the buy of `row.qty` at `px` on `row.inst`.
    Sign {
        i: usize,
        r: usize,
        row: GridRow,
        px: i64,
        token: [u8; 32],
        amt: (u8, u64, u64),
    },
    /// The pass is over (or the grid is off).
    Stop,
}

/// Signs the first empty or ageing order of the price grid (README "预签名"). One call looks at
/// `GRID_SCAN` entries at most; after a whole pass with nothing to sign, later calls return at
/// once until the first refresh is due.
fn grid_step(
    rd: &RefData,
    t: &Templates,
    ctx: &mut GwCtx,
    cred: &Credential,
    cfg: &Presign,
    now: u64,
) -> bool {
    // A grid order's `timestamp` is the clock's, never ahead of it: at most one per millisecond.
    match &ctx.pre.grid {
        Some(g) if now >= g.idle_until && ctx.pre.last_ts < now => {}
        _ => return false,
    }
    let refresh = u64::from(cfg.max_age_ms) * 3 / 4;
    for _ in 0..GRID_SCAN {
        match grid_scan(rd, ctx, cfg, now, refresh) {
            Scan::Stop => return false,
            Scan::Skip(m) => {
                if let Some(g) = ctx.pre.grid.as_mut() {
                    g.skip(m);
                }
            }
            Scan::Sign {
                i,
                r,
                row,
                px,
                token,
                amt,
            } => {
                let Some(p) = sign_pre(
                    t,
                    &mut ctx.pre,
                    cred,
                    now,
                    row.inst,
                    &token,
                    Side::Buy,
                    px,
                    row.qty,
                    amt,
                ) else {
                    return false;
                };
                let Some(g) = ctx.pre.grid.as_mut() else {
                    return false;
                };
                if let (Some(ts), Some(o), Some(x)) =
                    (g.ts.get_mut(i), g.orders.get_mut(i), g.rows.get_mut(r))
                {
                    *ts = p.ts_ms;
                    *o = p;
                    // The row's units, once computed, stay until its next bind.
                    *x = row;
                }
                g.skip(1);
                g.rescan();
                return true;
            }
        }
    }
    false
}

/// Looks at the entry under the grid's cursor; the cursor stays.
fn grid_scan(rd: &RefData, ctx: &mut GwCtx, cfg: &Presign, now: u64, refresh: u64) -> Scan {
    let Some(g) = ctx.pre.grid.as_mut() else {
        return Scan::Stop;
    };
    if g.left == 0 {
        // A whole pass signed nothing: nothing to do before the first refresh.
        g.idle_until = g.due;
        g.left = g.ts.len();
        g.due = u64::MAX;
        return Scan::Stop;
    }
    let i = g.cursor;
    let (r, k) = (i / g.n, i % g.n);
    // What is left of the row: a row without an inst, a token or units has nothing to sign.
    let rest = g.n - k;
    let (Some(mut row), Some(ts)) = (g.rows.get(r).copied(), g.ts.get(i).copied()) else {
        return Scan::Stop;
    };
    if row.inst.0 == 0 {
        return Scan::Skip(rest);
    }
    if ts != 0 && now.saturating_sub(ts) < refresh {
        g.due = g.due.min(ts.saturating_add(refresh));
        return Scan::Skip(1);
    }
    let ir = rd.inst(row.inst);
    if ir.venue != crate::VENUE {
        return Scan::Skip(rest);
    }
    let Some(token) = token_id(ctx, row.inst) else {
        return Scan::Skip(rest);
    };
    if row.step <= 0 || row.qty <= 0 {
        match (
            step_px(cfg.step_e6, ir.px_exp),
            step_px(cfg.grid_qty_e6, ir.qty_exp),
        ) {
            (Some(step), Some(qty)) if step > 0 && qty > 0 => (row.step, row.qty) = (step, qty),
            // Not whole numbers of the inst's units: no grid for this inst.
            _ => return Scan::Skip(rest),
        }
    }
    // Price 1 (the payout) in price units: valid prices are strictly below it.
    let px = i64::try_from(k + 1).ok().and_then(|k| k.checked_mul(row.step));
    let one = 10i64.checked_pow(u32::from(ir.px_exp.unsigned_abs()));
    let (Some(px), Some(one)) = (px, one) else {
        return Scan::Skip(1);
    };
    match amounts(rd, row.inst, Side::Buy, px, row.qty) {
        Some(amt) if px < one => Scan::Sign {
            i,
            r,
            row,
            px,
            token,
            amt,
        },
        // Not a price inside (0, 1), or amounts that are not whole: never signed.
        _ => Scan::Skip(1),
    }
}
