//! The check chain (cheap → expensive, short-circuit; README "检查链"), `commit` and the
//! rate-limit backoff.

use hft_event::{Note, NoteLevel, OrderFlags};
use hft_refdata::InstStatus;
use hft_strategy_sdk::{IntentClass, Now};
use hft_types::{AccountId, CapSource, GROUP_NONE, Px, RejectBits, Side as OSide, Tif, VenueId};

use crate::bucket::{Buckets, TOKEN, admits, backoff_ms};
use crate::mode::Mode;
use crate::types::{
    Admission, IntentView, N_RATE_LIMIT_HIT, Origin, RiskView, Scope, VENUE_NONE, div_ceil, note, rate_one,
    with_buffer_ceil,
};
use crate::{InstFacts, Side};

const fn bit(s: CapSource) -> u32 {
    1u32 << s.0
}

/// Bit of check #3 from a blocking cap-source mask (`desired_close`: CloseOnly mode / Delisting).
fn cap_bit(mask: u32, desired_close: bool) -> Option<RejectBits> {
    if mask & bit(CapSource::DRAINING) != 0 {
        Some(RejectBits::DRAINING)
    } else if mask & bit(CapSource::KILL) != 0 {
        Some(RejectBits::KILLED)
    } else if mask & bit(CapSource::DAILY_LOSS) != 0 {
        Some(RejectBits::DAILY_LOSS)
    } else if mask & bit(CapSource::RECON) != 0 {
        Some(RejectBits::RECON_DEVIATION)
    } else if mask != 0 || desired_close {
        Some(RejectBits::CLOSE_ONLY)
    } else {
        None
    }
}

/// `|2px − (bid + ask)| × 1e9 ≤ dev × (bid + ask)`.
fn within_band(px: Px, bid: Px, ask: Px, dev: hft_types::Rate) -> bool {
    let mid2 = i128::from(bid.0) + i128::from(ask.0);
    let d = (2 * i128::from(px.0) - mid2).abs();
    d.saturating_mul(rate_one()) <= i128::from(dev.0).saturating_mul(mid2)
}

/// Adverse cross beyond mid: buy `(px − mid) / mid ≤ cap`, sell `(mid − px) / mid ≤ cap`.
fn within_rr(side: OSide, px: Px, bid: Px, ask: Px, cap: hft_types::Rate) -> bool {
    let mid2 = i128::from(bid.0) + i128::from(ask.0);
    let d = match side {
        OSide::Buy => 2 * i128::from(px.0) - mid2,
        OSide::Sell => mid2 - 2 * i128::from(px.0),
    };
    d.saturating_mul(rate_one()) <= i128::from(cap.0).saturating_mul(mid2)
}

/// IOC cross beyond the touch: buy `(px − ask) / ask`, sell `(bid − px) / bid`.
fn within_ioc(side: OSide, px: Px, bid: Px, ask: Px, cap: hft_types::Rate) -> bool {
    let (d, base) = match side {
        OSide::Buy => (i128::from(px.0) - i128::from(ask.0), i128::from(ask.0)),
        OSide::Sell => (i128::from(bid.0) - i128::from(px.0), i128::from(bid.0)),
    };
    d.saturating_mul(rate_one()) <= i128::from(cap.0).saturating_mul(base)
}

fn open_pos_ok(pos: i64, open: i64, qty: i64, side: OSide, max: i64) -> bool {
    #[cfg(feature = "planted")]
    if crate::planted::on(crate::planted::NO_POSITION_LIMIT) {
        return true;
    }
    let v = match side {
        OSide::Buy => i128::from(pos) + i128::from(open) + i128::from(qty),
        OSide::Sell => i128::from(open) + i128::from(qty) - i128::from(pos),
    };
    v <= i128::from(max)
}

fn reduce_ok(qty: i64, pos: i64, pending: i64) -> bool {
    i128::from(qty) <= i128::from(pos).abs() - i128::from(pending)
}

/// The chain; `Err` carries exactly one bit.
pub(crate) fn evaluate<V: RiskView>(
    s: &Side,
    m: &Mode,
    it: &IntentView,
    v: &V,
    now: &Now,
) -> Result<Admission, RejectBits> {
    evaluate_with_peer(s, m, it, None, v, now)
}

pub(crate) fn evaluate_with_peer<V: RiskView>(
    s: &Side,
    m: &Mode,
    it: &IntentView,
    peer: Option<&IntentView>,
    v: &V,
    now: &Now,
) -> Result<Admission, RejectBits> {
    let rr = it.flags & OrderFlags::RISK_REDUCING.0 != 0;
    let smoke = it.origin == Origin::Command && it.flags & OrderFlags::SMOKE.0 != 0;
    let gated = !rr;
    let linked = it.link_venue != VENUE_NONE;
    let a = it.account;
    let lim = s.lim();

    // #0 structure, tradability, binding.
    let Some(f) = s.inst(it.inst).copied() else {
        return Err(RejectBits::INVALID);
    };
    if it.qty.0 <= 0 || it.px.0 <= 0 || f.venue != it.venue || a.0 == 0 || usize::from(a.0) > s.n_accounts {
        return Err(RejectBits::INVALID);
    }
    if (f.lot > 0 && it.qty.0 % f.lot != 0) || (f.tick > 0 && it.px.0 % f.tick != 0) {
        return Err(RejectBits::INVALID);
    }
    if smoke && !s.acct_live.get(usize::from(a.0)).copied().unwrap_or(false) {
        return Err(RejectBits::INVALID);
    }
    match f.status {
        InstStatus::Trading => {}
        InstStatus::ReduceOnly if rr => {}
        _ => return Err(RejectBits::NOT_TRADABLE),
    }
    let bound = s
        .acct_bound
        .get(usize::from(a.0))
        .copied()
        .unwrap_or(hft_types::StrategyId(0));
    let issuer = match it.origin {
        Origin::Strategy | Origin::Command => it.strategy,
        Origin::Exec | Origin::Oms => {
            if it.group == GROUP_NONE {
                it.strategy
            } else {
                it.group.strategy()
            }
        }
    };
    if bound.0 == 0 || issuer != bound {
        return Err(RejectBits::NOT_BOUND);
    }

    // #1 HARD.
    if m.hard(a) || (linked && m.hard(it.link_account)) {
        return Err(RejectBits::HARD);
    }

    // #2 clock.
    if gated && m.global_mask & bit(CapSource::CLOCK_BAD) != 0 {
        return Err(RejectBits::CLOCK);
    }

    // #3 caps and desired mode.
    if gated {
        let r = if smoke {
            let k = bit(CapSource::KILL);
            let mask = (m.global_mask & (bit(CapSource::DRAINING) | k))
                | (m.acct_mask_of(a) & k)
                | (m.av_mask_of(a, it.venue) & k);
            cap_bit(mask, m.inst_blocked(it.venue, it.inst))
        } else {
            let cell = crate::mode::CellKey {
                strategy: it.strategy,
                account: a,
                group: it.group,
            };
            let (cm, close) = m.cell_mask(cell);
            let mut mask = cm | m.av_mask_of(a, it.venue);
            let mut blocked = close || m.inst_blocked(it.venue, it.inst);
            if linked {
                mask |= m.global_mask
                    | m.acct_mask_of(it.link_account)
                    | m.av_mask_of(it.link_account, it.link_venue);
                blocked |= m.inst_blocked(it.link_venue, it.link_inst);
            }
            cap_bit(mask, blocked)
        };
        if let Some(b) = r {
            return Err(b);
        }
    }

    // #4 condition gates.
    if gated {
        if !v.venue_ready(it.venue, a) || (linked && !v.venue_ready(it.link_venue, it.link_account)) {
            return Err(RejectBits::VENUE_NOT_READY);
        }
        if lim.acct(a).unknown_blocks_open && v.group_has_unknown(a, it.group) {
            return Err(RejectBits::UNKNOWN_PENDING);
        }
    }

    // #5 quote health and price band.
    let max_silence = u64::from(s.params.conn_liveness_ms).saturating_mul(now.tsc_per_ms);
    let b = v.bbo(it.inst);
    if b.bid_px.0 <= 0 || b.ask_px.0 <= 0 {
        return Err(RejectBits::NO_MID);
    }
    if !b.quote_live(now.tsc, max_silence) {
        return Err(RejectBits::STALE_MID);
    }
    if !smoke {
        let band = lim.band(it.inst);
        let ok = if gated {
            within_band(it.px, b.bid_px, b.ask_px, band.max_px_dev)
                && (it.tif != Tif::Ioc || within_ioc(it.side, it.px, b.bid_px, b.ask_px, band.ioc_max_cross))
        } else {
            let mut cap = band.rr_max_cross;
            if it.origin == Origin::Exec && it.class == IntentClass::Flatten && s.params.l3_max_cross.0 > 0 {
                cap = cap.max(s.params.l3_max_cross);
            }
            within_rr(it.side, it.px, b.bid_px, b.ask_px, cap)
        };
        if !ok {
            return Err(RejectBits::PX_BAND);
        }
    }
    if linked && !v.bbo(it.link_inst).quote_live(now.tsc, max_silence) {
        return Err(RejectBits::LINK_STALE);
    }

    // #6 notional and hedge quantity.
    let ac = lim.acct(a);
    match it.class {
        IntentClass::Open => {
            if it.notional > ac.max_order_notional {
                return Err(RejectBits::NOTIONAL_ORDER);
            }
            let gl = lim.group(a, it.group);
            let n = i128::from(it.notional.0);
            let sum = |sc: Scope| {
                let pending = peer
                    .filter(|p| {
                        p.account == a
                            && match sc {
                                Scope::Account => true,
                                Scope::Group(g) => p.group == g,
                                Scope::Venue(venue) => p.venue == venue,
                            }
                    })
                    .map_or(0, |p| i128::from(p.notional.0));
                i128::from(v.held_notional(a, sc).0) + i128::from(v.open_notional(a, sc).0) + n + pending
            };
            if sum(Scope::Group(it.group)) > i128::from(gl.max_group_notional.0) {
                return Err(RejectBits::NOTIONAL_GROUP);
            }
            let eq = v.equity(a);
            let mut acct_max = ac.max_account_notional;
            if eq.valid {
                acct_max = acct_max.min(
                    eq.total
                        .mul_rate(ac.account_notional_from_equity, hft_types::RoundMode::Floor),
                );
            }
            if sum(Scope::Account) > i128::from(acct_max.0) {
                return Err(RejectBits::NOTIONAL_ACCOUNT);
            }
            if sum(Scope::Venue(it.venue)) > i128::from(lim.venue(a, it.venue).max_venue_notional.0) {
                return Err(RejectBits::NOTIONAL_VENUE);
            }
        }
        IntentClass::Hedge => {
            if it.qty > it.hedge_cap_qty || it.notional > ac.max_hedge_notional {
                return Err(RejectBits::HEDGE_LIMIT);
            }
        }
        IntentClass::Reduce | IntentClass::Flatten => {
            if it.notional > ac.max_reduce_notional {
                return Err(RejectBits::NOTIONAL_ORDER);
            }
        }
    }

    // #7 position.
    let pos = v.pos(a, it.inst).0;
    match it.class {
        IntentClass::Open => {
            let max = lim.group(a, it.group).max_pos_qty.0;
            if !open_pos_ok(pos, v.open_qty(a, it.inst, it.side).0, it.qty.0, it.side, max) {
                return Err(RejectBits::POSITION);
            }
            if linked {
                let lmax = lim.group(it.link_account, it.group).max_pos_qty.0;
                let lpos = v.pos(it.link_account, it.link_inst).0;
                let lopen = v.open_qty(it.link_account, it.link_inst, it.link_side).0;
                if !open_pos_ok(lpos, lopen, it.link_qty.0, it.link_side, lmax) {
                    return Err(RejectBits::POSITION);
                }
            }
        }
        IntentClass::Hedge => {
            if it.flags & OrderFlags::REDUCE_ONLY.0 != 0 {
                if !reduce_ok(it.qty.0, pos, v.pending_reduce_immediate(a, it.inst, it.side).0) {
                    return Err(RejectBits::REDUCE_INVALID);
                }
            } else {
                let max = lim.group(a, it.group).max_pos_qty.0;
                if !open_pos_ok(
                    pos,
                    v.open_qty_immediate(a, it.inst, it.side).0,
                    it.qty.0,
                    it.side,
                    max,
                ) {
                    return Err(RejectBits::POSITION);
                }
            }
        }
        IntentClass::Flatten => {
            if !reduce_ok(it.qty.0, pos, v.pending_reduce_immediate(a, it.inst, it.side).0) {
                return Err(RejectBits::REDUCE_INVALID);
            }
        }
        IntentClass::Reduce => {
            if !reduce_ok(it.qty.0, pos, v.pending_reduce(a, it.inst, it.side).0) {
                return Err(RejectBits::REDUCE_INVALID);
            }
        }
    }

    // #7b link-leg notional and affordability.
    if linked && it.class == IntentClass::Open {
        let la = it.link_account;
        let lac = lim.acct(la);
        if it.link_notional > lac.max_hedge_notional {
            return Err(RejectBits::HEDGE_LIMIT);
        }
        let Some(lf) = s.inst(it.link_inst).copied() else {
            return Err(RejectBits::LINK_FUNDS);
        };
        if !affordable(s, &lf, it, v, lac.funds_buffer) {
            return Err(RejectBits::LINK_FUNDS);
        }
    }

    // #8 rate.
    let headroom = s.params.rate_headroom;
    let vb = s
        .buckets
        .venue
        .get(Buckets::vix(a, it.venue))
        .copied()
        .unwrap_or_default();
    let t = now.engine_ns;
    if t < vb.backoff_until_ens {
        return Err(RejectBits::RATE_BACKOFF);
    }
    let (w10, w60) = s.buckets.windows(it.venue, headroom);
    let res = lim.venue(a, it.venue).rr_reserve;
    if !admits(w10.at(vb.tokens_10s, vb.last_ens, t), w10.reserve(res), rr)
        || !admits(w60.at(vb.tokens_min, vb.last_ens, t), w60.reserve(res), rr)
    {
        return Err(RejectBits::RATE);
    }
    let iw = Buckets::inst_window(lim.venue(a, it.venue).inst_bucket_per_s);
    let ib = s
        .buckets
        .inst
        .get(s.buckets.iix(a, it.inst))
        .copied()
        .unwrap_or_default();
    // The instrument bucket is a strategy pacing budget. Repairs use the reserved
    // venue budget, so an opening order cannot lock its own hedge out for a second.
    if !rr && iw.at(ib.tokens, ib.last_ens, t) < TOKEN {
        return Err(RejectBits::RATE);
    }
    if linked {
        let lb = s
            .buckets
            .venue
            .get(Buckets::vix(it.link_account, it.link_venue))
            .copied()
            .unwrap_or_default();
        if t < lb.backoff_until_ens {
            return Err(RejectBits::RATE_BACKOFF);
        }
        let (l10, l60) = s.buckets.windows(it.link_venue, headroom);
        let lres = lim.venue(it.link_account, it.link_venue).rr_reserve;
        if !admits(l10.at(lb.tokens_10s, lb.last_ens, t), l10.reserve(lres), false)
            || !admits(l60.at(lb.tokens_min, lb.last_ens, t), l60.reserve(lres), false)
        {
            return Err(RejectBits::RATE);
        }
    }

    // #9 exposure gate.
    if gated
        && !smoke
        && let Some(sum) = s.legs.get(a, it.group)
    {
        let gl = lim.group(a, it.group);
        if sum.naked_ratio > gl.naked_gate && sum.naked_notional >= gl.naked_gate_min {
            return Err(RejectBits::EXPOSURE_GATE);
        }
    }

    Ok(Admission {
        venue: it.venue,
        account: a,
        inst: it.inst,
        risk_reducing: rr,
    })
}

/// #7b ②: the second leg can be funded.
fn affordable<V: RiskView>(s: &Side, lf: &InstFacts, it: &IntentView, v: &V, buf: hft_types::Rate) -> bool {
    let la = it.link_account;
    if lf.perp {
        let Some(lev) = s.lev.get(la, it.link_inst) else {
            return false;
        };
        let Some(avail) = v.avail_margin(la, it.link_venue) else {
            return false;
        };
        let num = i128::from(it.link_notional.0).saturating_mul(rate_one().saturating_add(i128::from(buf.0)));
        let need = div_ceil(num, i128::from(lev.0));
        i128::from(avail.0) >= need
    } else {
        match it.link_side {
            OSide::Buy => {
                let Some(avail) = v.avail_quote(la, it.link_venue, lf.quote) else {
                    return false;
                };
                i128::from(avail.0) >= with_buffer_ceil(it.link_notional, buf)
            }
            OSide::Sell => {
                let have = i128::from(v.pos(la, it.link_inst).0)
                    - i128::from(v.pending_reduce(la, it.link_inst, OSide::Sell).0);
                have >= i128::from(it.link_qty.0)
            }
        }
    }
}

/// Earliest admission by all applicable rate buckets (other risk checks still apply).
pub(crate) fn rate_ready_at(s: &Side, it: &IntentView, now: &Now) -> i64 {
    let rr = it.flags & OrderFlags::RISK_REDUCING.0 != 0;
    let venue_ready = |a, v, reducing| {
        let b = s
            .buckets
            .venue
            .get(Buckets::vix(a, v))
            .copied()
            .unwrap_or_default();
        let (w10, w60) = s.buckets.windows(v, s.params.rate_headroom);
        let reserve = s.lim().venue(a, v).rr_reserve;
        let need =
            |w: crate::bucket::Window| TOKEN.saturating_add(if reducing { 0 } else { w.reserve(reserve) });
        now.engine_ns
            .max(b.backoff_until_ens)
            .max(w10.ready_at(b.tokens_10s, b.last_ens, now.engine_ns, need(w10)))
            .max(w60.ready_at(b.tokens_min, b.last_ens, now.engine_ns, need(w60)))
    };
    let mut at = venue_ready(it.account, it.venue, rr);
    if !rr {
        let w = Buckets::inst_window(s.lim().venue(it.account, it.venue).inst_bucket_per_s);
        let b = s
            .buckets
            .inst
            .get(s.buckets.iix(it.account, it.inst))
            .copied()
            .unwrap_or_default();
        at = at.max(w.ready_at(b.tokens, b.last_ens, now.engine_ns, TOKEN));
    }
    if it.link_venue != VENUE_NONE {
        at = at.max(venue_ready(it.link_account, it.link_venue, false));
    }
    at
}

/// Deducts venue tokens for every order; only ordinary orders consume strategy pacing tokens.
pub(crate) fn commit(s: &mut Side, adm: &Admission, now: &Now) {
    let t = now.engine_ns;
    let headroom = s.params.rate_headroom;
    let (w10, w60) = s.buckets.windows(adm.venue, headroom);
    let per_s = s.lim().venue(adm.account, adm.venue).inst_bucket_per_s;
    if let Some(b) = s.buckets.venue.get_mut(Buckets::vix(adm.account, adm.venue)) {
        if w10.cap.is_some() {
            b.tokens_10s = w10.at(b.tokens_10s, b.last_ens, t).saturating_sub(TOKEN);
        }
        if w60.cap.is_some() {
            b.tokens_min = w60.at(b.tokens_min, b.last_ens, t).saturating_sub(TOKEN);
        }
        b.last_ens = t;
    }
    let iw = Buckets::inst_window(per_s);
    let ix = s.buckets.iix(adm.account, adm.inst);
    if !adm.risk_reducing
        && let Some(b) = s.buckets.inst.get_mut(ix)
    {
        b.tokens = iw.at(b.tokens, b.last_ens, t).saturating_sub(TOKEN);
        b.last_ens = t;
    }
}

/// Backoff after an exchange rate-limit hit; returns `Note{RateLimitHit}`.
pub(crate) fn rate_limit_hit(s: &mut Side, v: VenueId, a: AccountId, now: &Now) -> Note {
    let t = now.engine_ns;
    let vl = *s.lim().venue(a, v);
    let max_ns = i64::from(vl.backoff_max_ms).saturating_mul(1_000_000);
    let mut k = 0u32;
    let mut until = t;
    if let Some(b) = s.buckets.venue.get_mut(Buckets::vix(a, v)) {
        k = if b.backoff_until_ens != 0
            && i128::from(t) - i128::from(b.backoff_until_ens) < i128::from(max_ns)
        {
            b.backoff_k.saturating_add(1)
        } else {
            0
        };
        let d = i64::from(backoff_ms(vl.backoff_base_ms, vl.backoff_max_ms, k)).saturating_mul(1_000_000);
        until = t.saturating_add(d);
        b.backoff_k = k;
        b.backoff_until_ens = until;
        b.tokens_10s = 0;
        b.tokens_min = 0;
        b.last_ens = t;
    }
    note(
        N_RATE_LIMIT_HIT,
        NoteLevel::P2,
        [
            u64::from(v.0) | u64::from(a.0) << 8,
            u64::from(k),
            until as u64,
            0,
            0,
            0,
            0,
            0,
        ],
    )
}
