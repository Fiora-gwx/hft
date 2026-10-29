//! Pre-signed orders (README "预签名"): in the gw's idle time, sign the orders a `Place` is likely
//! to need — per inst, the side and size of its last place at prices around that place's price —
//! so that the `Place` only fills in the request body. Each pre-signed order is used at most once
//! (its hash, once posted or cancelled, is dead); `timestamp` (the V2 nonce) is unique per account.

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
}

impl Presign {
    /// `None` when the window exceeds `PRESIGN_PRICES`, the step is 0 or the age is outside
    /// 1 000–240 000 ms.
    pub fn new(below: u8, above: u8, step_e6: u32, max_age_ms: u32) -> Option<Presign> {
        let ok = usize::from(below) + usize::from(above) < PRESIGN_PRICES
            && step_e6 > 0
            && (AGE_MIN_MS..=AGE_MAX_MS).contains(&max_age_ms);
        ok.then_some(Presign {
            below,
            above,
            step_e6,
            max_age_ms,
        })
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

/// Pre-signing state of one account (part of `GwCtx`).
#[derive(Clone, Debug)]
pub(crate) struct Presigned {
    cfg: Option<Presign>,
    anchors: [Anchor; ANCHORS],
    slots: Box<[Pre]>,
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
            last_ts: 0,
            next: 0,
        }
    }
}

impl Presigned {
    /// Startup only: turns pre-signing on (allocates the order store).
    pub(crate) fn enable(&mut self, p: Presign) {
        self.cfg = Some(p);
        self.slots = vec![Pre::default(); SLOTS].into_boxed_slice();
    }

    /// A unique order `timestamp` (ms): `now_ms`, or one past the last one signed.
    pub(crate) fn next_ts(&mut self, now_ms: u64) -> u64 {
        let ts = now_ms.max(self.last_ts.saturating_add(1));
        self.last_ts = ts;
        ts
    }

    /// The place of `inst` at (`side`, `px`, `qty`) centres its pre-signed prices.
    pub(crate) fn anchor(&mut self, inst: InstId, side: Side, px: i64, qty: i64) {
        if self.cfg.is_none() {
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
    /// `max_age_ms`.
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
    }

    /// Pre-signed orders held now (tests, telemetry).
    pub(crate) fn held(&self) -> usize {
        self.slots.iter().filter(|p| p.inst.0 != 0).count()
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

/// `step_e6` (1e-6 USDC) in price units of exponent `px_exp`; `None` when not a whole number.
fn step_px(step_e6: u32, px_exp: i8) -> Option<i64> {
    let e = -6 - i32::from(px_exp);
    let v = i64::from(step_e6);
    if e >= 0 {
        v.checked_mul(10i64.checked_pow(e.unsigned_abs())?)
    } else {
        let p = 10i64.checked_pow(e.unsigned_abs())?;
        (v % p == 0).then_some(v / p)
    }
}

/// One unit of idle work (`VenueProtocol::gw_idle`): signs the first missing or ageing order of
/// the anchors' price windows; `false` when every one is fresh (or pre-signing is off).
pub(crate) fn step(rd: &RefData, t: &Templates, ctx: &mut GwCtx, cred: &Credential, clk: &WallClock) -> bool {
    let Some(cfg) = ctx.pre.cfg else {
        return false;
    };
    let Ok(now) = u64::try_from(clk.unix_ms) else {
        return false;
    };
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
        let Some(token) = ctx
            .token_of(a.inst)
            .and_then(|s| core::str::from_utf8(s).ok())
            .and_then(crate::sign::parse_u256)
        else {
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
            let Some((side, maker_amount, taker_amount)) = amounts(rd, a.inst, a.side, px, a.qty) else {
                continue;
            };
            let Some(w) = l2_keys(cred)
                .ok()
                .filter(|k| k.deposit == t.wallet)
                .and_then(|k| signer(&k, t))
            else {
                return false;
            };
            let ts = ctx.pre.next_ts(now);
            // Unique through `timestamp`; no slot is known yet (README "预签名").
            let salt = (ts & ((1 << 33) - 1)) << 20;
            let mut sig = [0u8; 512];
            let (hash, n) = sign_v2(
                &w,
                t,
                &token,
                side,
                maker_amount,
                taker_amount,
                salt,
                ts,
                &mut sig,
            );
            ctx.pre.next = ai;
            if let Some(s) = ctx.pre.slot_for(a.inst, &token, a.side, px, a.qty) {
                *s = Pre {
                    inst: a.inst,
                    token,
                    side: a.side,
                    px,
                    qty: a.qty,
                    salt,
                    ts_ms: ts,
                    hash,
                    sig_len: n as u16,
                    sig,
                };
            }
            return true;
        }
    }
    false
}
