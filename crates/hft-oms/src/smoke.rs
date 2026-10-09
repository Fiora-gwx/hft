//! Smoke orders (README "冒烟单"): intent generation and TTL cancellation.

use hft_refdata::InstHot;
use hft_strategy_sdk::{BboView, IntentClass, LegLinkOpt, OrderIntent};
use hft_types::{AccountId, GROUP_NONE, Px, Qty, Rate, RoundMode, Side, Tif, Tsc, VenueId};

use crate::Oms;
use crate::types::{OmsOut, OrderState, SlotFlags, SmokeErr, SmokeReq, TransitionCause};

/// `ceil(n / d)` for `d > 0`.
fn ceil_div(n: i128, d: i128) -> i128 {
    if d <= 0 {
        return 0;
    }
    n.div_euclid(d) + i128::from(n.rem_euclid(d) != 0)
}

impl Oms {
    /// Builds the far-from-market PostOnly of a `SmokeOrder` command (read-only).
    pub fn smoke_intent(
        &self,
        c: &SmokeReq,
        bbo: &BboView,
        hot: &InstHot,
        conn_liveness_ms: u32,
        now: Tsc,
    ) -> Result<OrderIntent, SmokeErr> {
        let t = self.table(c.venue, c.account).ok_or(SmokeErr::BadInst)?;
        if t.smoke_slot != 0 {
            return Err(SmokeErr::Busy);
        }
        if hot.venue != c.venue || self.inst(c.inst).venue != c.venue {
            return Err(SmokeErr::BadInst);
        }
        let silence = self.ms(conn_liveness_ms);
        if !bbo.quote_live(now, silence) || bbo.bid_px.0 <= 0 || bbo.ask_px.0 <= 0 {
            return Err(SmokeErr::NoQuote);
        }
        let far = self.p.smoke_far_bp;
        let px = match c.side {
            Side::Buy => bbo
                .ask_px
                .mul_rate(Rate(far.0.saturating_neg()), RoundMode::Floor)
                .round_to_tick(hot.tick, RoundMode::Floor),
            Side::Sell => bbo
                .bid_px
                .mul_rate(far, RoundMode::Ceil)
                .round_to_tick(hot.tick, RoundMode::Ceil),
        };
        if px.0 <= 0 {
            return Err(SmokeErr::NoQuote);
        }
        // qty = ceil(min_notional × 1.1 / px) at the asset's qty exponent.
        let k = -6 - i32::from(hot.px_exp) - i32::from(hot.qty_exp);
        let (num, den) = if k >= 0 {
            (
                i128::from(hot.min_notional.0) * 11 * 10i128.pow(k as u32),
                10 * i128::from(px.0),
            )
        } else {
            (
                i128::from(hot.min_notional.0) * 11,
                10 * i128::from(px.0) * 10i128.pow((-k) as u32),
            )
        };
        let q = ceil_div(num, den).min(i128::from(i64::MAX)) as i64;
        let q = Qty(q.max(hot.min_qty.0)).round_to_lot(hot.lot, RoundMode::Ceil);
        Ok(OrderIntent {
            venue: c.venue,
            account: c.account,
            side: c.side,
            tif: Tif::PostOnly,
            class: IntentClass::Open,
            flags: 0,
            _pad: [0; 2],
            inst: c.inst,
            group: GROUP_NONE,
            px: Px(px.0),
            qty: q,
            link: LegLinkOpt::NONE,
            tag: 0,
            max_rest_ms: 0,
        })
    }

    /// `Timer{Oms, SmokeTtl}` of (a, v). `keep` = Drain without cancelling resting orders.
    pub fn on_smoke_ttl(
        &mut self,
        a: AccountId,
        v: VenueId,
        keep: bool,
        now: Tsc,
        cause_seq: u64,
        out: &mut OmsOut,
    ) {
        let Some(ti) = self.tix(v, a) else { return };
        let Some(t) = self.t(ti) else { return };
        let s = t.smoke_slot;
        if s == 0 || (keep && t.has_dms) {
            return;
        }
        let Some(sl) = t.slot(s).copied() else { return };
        match sl.state {
            OrderState::Live | OrderState::PendingNew if !sl.sflags.has(SlotFlags::CANCEL_SENT) => {
                self.cancel_slot(ti, s, TransitionCause::SmokeTtl, None, now, cause_seq, out);
            }
            OrderState::Unknown => {
                if let Some(x) = self.tm(ti).and_then(|t| t.slots.get_mut(s as usize)) {
                    x.sflags.0 |= SlotFlags::SMOKE_DUE;
                }
            }
            _ => {}
        }
        self.flush(out);
    }
}
