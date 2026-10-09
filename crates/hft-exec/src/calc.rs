//! Fixed-point helpers of the execution layer (README "限价与单笔名义", "不交易带与紧急度",
//! "两腿 lot 对齐", TakerSlice "可见深度"). Every product is i128; nothing panics.

use hft_event::Level;
use hft_refdata::InstHot;
use hft_strategy_sdk::{BboView, BookRead, BookStatus};
use hft_types::{HedgePxLimits, Notional, Px, Qty, Rate, Side, cap_cross_px};

const RATE_ONE: i128 = 1_000_000_000;

/// `10^k` (k in 0..=38).
fn pow10(k: i32) -> Option<i128> {
    if !(0..=38).contains(&k) {
        return None;
    }
    let mut v: i128 = 1;
    for _ in 0..k {
        v = v.checked_mul(10)?;
    }
    Some(v)
}

/// `px_exp + qty_exp + 6` (scale of `px × qty` into USDT micro units).
#[inline]
fn scale(h: &InstHot) -> i32 {
    i32::from(h.px_exp) + i32::from(h.qty_exp) + 6
}

fn div_floor(n: i128, d: i128) -> Option<i128> {
    if d == 0 {
        return None;
    }
    n.checked_div_euclid(d)
}

fn div_ceil(n: i128, d: i128) -> Option<i128> {
    if d <= 0 {
        return None;
    }
    let q = n.checked_div_euclid(d)?;
    if n.checked_rem_euclid(d)? != 0 {
        q.checked_add(1)
    } else {
        Some(q)
    }
}

/// `|q| × px` in quote-asset micro units, as the exact fraction `(num, den)`.
fn frac(q: Qty, px: Px, h: &InstHot) -> Option<(i128, i128)> {
    let p = i128::from(q.0).checked_abs()?.checked_mul(i128::from(px.0))?;
    let s = scale(h);
    if s >= 0 {
        Some((p.checked_mul(pow10(s)?)?, 1))
    } else {
        Some((p, pow10(-s)?))
    }
}

/// `|q| × px ≤ n` exactly (quote asset; dust and band-floor checks).
pub fn notional_le(q: Qty, px: Px, h: &InstHot, n: Notional) -> bool {
    match frac(q, px, h) {
        Some((num, den)) => match i128::from(n.0).checked_mul(den) {
            Some(r) => num <= r,
            None => true,
        },
        None => false,
    }
}

/// `⌈|q| × px⌉` in quote micro units (saturating).
pub fn notional_ceil(q: Qty, px: Px, h: &InstHot) -> i128 {
    frac(q, px, h)
        .and_then(|(n, d)| div_ceil(n, d))
        .unwrap_or(i128::MAX)
}

/// `⌈|q| × px × rate⌉` in USDT micro units (risk check #6 basis).
pub fn usdt_ceil(q: Qty, px: Px, rate: Rate, h: &InstHot) -> i128 {
    frac(q, px, h)
        .and_then(|(n, d)| {
            let n = n.checked_mul(i128::from(rate.0))?;
            let d = d.checked_mul(RATE_ONE)?;
            div_ceil(n, d)
        })
        .unwrap_or(i128::MAX)
}

/// Largest raw quantity with `|q| × px × rate ≤ cap` (before lot rounding); ≥ 0.
pub fn qty_for_cap(cap: Notional, px: Px, rate: Rate, h: &InstHot) -> i64 {
    if cap.0 <= 0 || px.0 <= 0 || rate.0 <= 0 {
        return 0;
    }
    let s = scale(h);
    // q ≤ cap × 1e9 × 10^-s / (px × rate)
    let r = (|| {
        let num = i128::from(cap.0).checked_mul(RATE_ONE)?;
        let den = i128::from(px.0).checked_mul(i128::from(rate.0))?;
        let (num, den) = if s >= 0 {
            (num, den.checked_mul(pow10(s)?)?)
        } else {
            (num.checked_mul(pow10(-s)?)?, den)
        };
        div_floor(num, den)
    })();
    match r {
        Some(v) if v > i128::from(hft_types::MAX_RAW) => hft_types::MAX_RAW,
        Some(v) if v < 0 => 0,
        Some(v) => v as i64,
        None => hft_types::MAX_RAW,
    }
}

/// Quantity (multiple of `lot`) satisfying `⌈q × px × rate⌉ ≤ cap` (rule 4 of "限价与单笔名义").
pub fn qty_under_cap(cap: Notional, px: Px, rate: Rate, h: &InstHot) -> Qty {
    let lot = h.lot.max(1);
    let mut q = floor_lot(Qty(qty_for_cap(cap, px, rate, h)), lot);
    while q.0 > 0 && usdt_ceil(q, px, rate, h) > i128::from(cap.0) {
        q = Qty(q.0.saturating_sub(lot));
    }
    q
}

/// Toward-zero rounding to a multiple of `lot`.
#[inline]
pub fn floor_lot(q: Qty, lot: i64) -> Qty {
    if lot <= 0 {
        return q;
    }
    Qty(q.0.checked_div(lot).unwrap_or(0).saturating_mul(lot))
}

/// `q × r` (Rate 1e-9) toward zero.
#[inline]
pub fn mul_rate(q: Qty, r: Rate) -> Qty {
    let v = i128::from(q.0).saturating_mul(i128::from(r.0)) / RATE_ONE;
    Qty(v.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64)
}

/// Quantity worth `n` at `px` (quote asset, toward zero; band floor).
pub fn qty_of_notional(n: Notional, px: Px, h: &InstHot) -> Qty {
    if px.0 <= 0 || n.0 <= 0 {
        return Qty(0);
    }
    let s = scale(h);
    let r = (|| {
        if s >= 0 {
            div_floor(i128::from(n.0), i128::from(px.0).checked_mul(pow10(s)?)?)
        } else {
            div_floor(i128::from(n.0).checked_mul(pow10(-s)?)?, i128::from(px.0))
        }
    })();
    Qty(r.map_or(hft_types::MAX_RAW, |v| {
        v.clamp(0, i128::from(hft_types::MAX_RAW)) as i64
    }))
}

/// `(bid + ask) / 2` of a quote (floor).
#[inline]
pub fn mid(b: &BboView) -> Px {
    Px::mid_floor(b.bid_px, b.ask_px)
}

/// Touch price for `side` (buy: ask, sell: bid).
#[inline]
pub fn touch(b: &BboView, side: Side) -> Px {
    match side {
        Side::Buy => b.ask_px,
        Side::Sell => b.bid_px,
    }
}

/// Limit price (`cap_cross_px`).
pub fn limit_px(side: Side, b: &BboView, max_cross: Rate, rr: Rate, liveness_ms: u32, h: &InstHot) -> Px {
    let lim = HedgePxLimits {
        conn_liveness_ms: liveness_ms,
        rr_max_cross: rr,
    };
    cap_cross_px(side, touch(b, side), mid(b), max_cross, &lim, h.tick)
}

/// Whether the touch is beyond the mid band `mid × (1 ± rr)` (IOC at the capped limit cannot fill).
pub fn beyond_cross(side: Side, b: &BboView, rr: Rate) -> bool {
    let m = i128::from(mid(b).0);
    let t = i128::from(touch(b, side).0).saturating_mul(RATE_ONE);
    match side {
        Side::Buy => t > m.saturating_mul(RATE_ONE + i128::from(rr.0)),
        Side::Sell => t < m.saturating_mul(RATE_ONE - i128::from(rr.0)),
    }
}

/// Visible depth on the side `side` trades against: up to `levels` levels within
/// `touch × (1 ± max_cross)`; `(qty, Σ px × qty in quote micro units)`. Without a live L2 book the
/// BBO top level only.
pub fn depth<B: BookRead>(
    book: &B,
    b: &BboView,
    side: Side,
    levels: u8,
    max_cross: Rate,
    h: &InstHot,
) -> (Qty, i128) {
    let t = touch(b, side);
    let bound = match side {
        Side::Buy => i128::from(t.0).saturating_mul(RATE_ONE + i128::from(max_cross.0)),
        Side::Sell => i128::from(t.0).saturating_mul(RATE_ONE - i128::from(max_cross.0)),
    };
    let within = |px: Px| {
        let p = i128::from(px.0).saturating_mul(RATE_ONE);
        match side {
            Side::Buy => p <= bound,
            Side::Sell => p >= bound,
        }
    };
    let top_qty = match side {
        Side::Buy => b.ask_qty,
        Side::Sell => b.bid_qty,
    };
    if book.status() != BookStatus::Live {
        return (top_qty, notional_ceil(top_qty, t, h));
    }
    let mut buf = [Level {
        px: Px(0),
        qty: Qty(0),
    }; 10];
    let k = usize::from(levels.clamp(1, 10));
    let n = book.levels(side.opposite(), buf.get_mut(..k).unwrap_or(&mut []));
    let (mut q, mut v) = (0i64, 0i128);
    for l in buf.iter().take(n) {
        if !within(l.px) {
            break;
        }
        q = q.saturating_add(l.qty.0);
        v = v.saturating_add(notional_ceil(l.qty, l.px, h));
    }
    (Qty(q), v)
}

/// Sign of a quantity (−1, 0, +1).
#[inline]
pub const fn sgn(q: Qty) -> i64 {
    q.0.signum()
}
