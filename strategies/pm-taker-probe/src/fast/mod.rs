//! Fast part of `pm-taker-probe` (sequencer thread, hot). README "规则": nothing until `enabled`
//! changes from 0 to 1; then exactly one round on the group's leg — a spend buy (Ioc,
//! `OrderIntent::SPEND`) at the best ask for `spend_usdc`, and after it ends, with `sell_back`, an
//! Ioc sell of the leg's position at the best bid — then idle until `enabled` goes 0 and 1 again.
//! At most one order is open at any time. No allocation; the snapshot is the round state.
#![deny(clippy::float_arithmetic)]

use hft_strategy_sdk::prelude::*;
use hft_strategy_sdk::{
    Ctx, EvMask, EventRef, GroupTable, Part, RestoreError, SnapshotReader, SnapshotWriter, SubsBuilder, Views,
};

use crate::desc::{self, end, skip};
use crate::params::PmTakerProbeParams;

/// Order tag of the buy.
pub const TAG_BUY: u32 = 1;
/// Order tag of the sell-back.
pub const TAG_SELL: u32 = 2;

/// The book of the leg (the BBO view is read from it; the engine also subscribes the BBO stream
/// where the venue has one).
pub fn subscribe(groups: &GroupTable, out: &mut SubsBuilder<'_>) {
    for (gix, g) in groups.iter() {
        if let Some(l) = g.legs.first().filter(|_| g.n_legs > 0) {
            let _ = out.market(l.inst, gix, EvMask::BOOK, Part::Fast);
        }
    }
}

/// Where the round is (README "规则"); the discriminant is the snapshot code.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Phase {
    /// Not armed: waits for `enabled` 0 → 1.
    #[default]
    Idle = 0,
    /// Armed: the buy goes out at the next quote of the leg.
    Armed = 1,
    /// Buy placed in the previous event; its outcome is read on the next one.
    Buying = 2,
    /// Buy has an OMS slot; waits for its end.
    BuyLive = 3,
    /// Sell placed in the previous event.
    Selling = 4,
    /// Sell has an OMS slot; waits for its end.
    SellLive = 5,
    /// Round over while `enabled` is still 1: waits for `enabled` 0.
    Done = 6,
}

impl Phase {
    fn of(c: u8) -> Option<Phase> {
        Some(match c {
            0 => Phase::Idle,
            1 => Phase::Armed,
            2 => Phase::Buying,
            3 => Phase::BuyLive,
            4 => Phase::Selling,
            5 => Phase::SellLive,
            6 => Phase::Done,
            _ => return None,
        })
    }
}

#[derive(Debug)]
pub struct Fast {
    pub account: AccountId,
    pub group: GroupId,
    pub gix: u16,
    pub inst: InstId,
    pub venue: VenueId,
    /// `enabled` as last applied (arming is its 0 → 1 change).
    pub enabled: bool,
    pub spend: Notional,
    pub sell_back: bool,
    pub phase: Phase,
    /// `IntentHandle` of the order placed in the previous event.
    pub handle: u16,
    /// The open order (`BuyLive`, `SellLive`).
    pub r: Option<OrderRef>,
    /// Cumulative fill of the open order.
    pub cum: Qty,
    /// Rounds started since the instance was created.
    pub rounds: u32,
}

impl Fast {
    /// A new instance never starts armed, whatever `enabled` is (README "规则" 1).
    pub fn new(account: AccountId, p: &PmTakerProbeParams, groups: &GroupTable) -> Fast {
        let mut f = Fast {
            account,
            group: GROUP_NONE,
            gix: 0,
            inst: INST_NONE,
            venue: VenueId(0),
            enabled: p.defaults().enabled,
            spend: Notional(0),
            sell_back: true,
            phase: Phase::Idle,
            handle: 0,
            r: None,
            cum: Qty(0),
            rounds: 0,
        };
        f.on_params(p, groups);
        f
    }

    /// New parameters: `enabled` 0 → 1 arms an idle instance; `enabled` 0 disarms an armed one
    /// and lets a finished round start over. A round in flight finishes either way.
    pub fn on_params(&mut self, p: &PmTakerProbeParams, groups: &GroupTable) {
        let d = p.defaults();
        self.spend = d.spend_usdc;
        self.sell_back = d.sell_back;
        if let Some((gix, g)) = groups.iter().next()
            && let Some(l) = g.legs.first().filter(|_| g.n_legs > 0)
        {
            self.group = g.id;
            self.gix = gix;
            self.inst = l.inst;
            self.venue = l.venue;
        }
        let en = d.enabled;
        if en && !self.enabled && self.phase == Phase::Idle {
            self.phase = Phase::Armed;
        }
        if !en && matches!(self.phase, Phase::Armed | Phase::Done) {
            self.phase = Phase::Idle;
        }
        self.enabled = en;
    }

    pub fn on_event<V: Views>(&mut self, ev: &EventRef<'_>, v: &V, ctx: &mut Ctx<'_>) {
        self.outcome(v, ctx);
        match *ev {
            EventRef::Book { inst, .. } | EventRef::Bbo { inst, .. }
                if inst == self.inst && self.phase == Phase::Armed =>
            {
                self.buy(v, ctx);
            }
            EventRef::Order { u, .. } if self.r == Some(u.r) => self.order(u, v, ctx),
            _ => {}
        }
    }

    /// The previous event's place: an OMS slot, or the end of that step.
    fn outcome<V: Views>(&mut self, v: &V, ctx: &mut Ctx<'_>) {
        let selling = match self.phase {
            Phase::Buying => false,
            Phase::Selling => true,
            _ => return,
        };
        let (e, code) = match ctx.outcome(IntentHandle(self.handle)) {
            Some(IntentOutcome::Placed(r)) => {
                self.r = Some(r);
                self.cum = Qty(0);
                self.phase = if selling { Phase::SellLive } else { Phase::BuyLive };
                return;
            }
            Some(IntentOutcome::Shadowed) => (end::SHADOWED, 0),
            Some(IntentOutcome::Rejected(bits)) => (end::GATED, u64::from(bits.0)),
            _ => (end::GATED, 0),
        };
        self.ended(selling, e, code, v, ctx);
    }

    /// Updates of the open order; its end moves the round on.
    fn order<V: Views>(&mut self, u: &OrderUpdate, v: &V, ctx: &mut Ctx<'_>) {
        let selling = match self.phase {
            Phase::BuyLive => false,
            Phase::SellLive => true,
            _ => return,
        };
        let (e, code) = match u.kind {
            OrderUpdateKind::Filled { cum, done, .. } => {
                self.cum = cum;
                if !done {
                    return;
                }
                (end::FILLED, 0)
            }
            OrderUpdateKind::Cancelled => (end::CANCELLED, 0),
            OrderUpdateKind::Rejected { bits, venue_code } => (
                end::REJECTED,
                if venue_code != 0 {
                    u64::from(venue_code)
                } else {
                    u64::from(bits.0)
                },
            ),
            _ => return,
        };
        self.r = None;
        self.ended(selling, e, code, v, ctx);
    }

    /// The buy or the sell ended: note it; after the buy, the sell-back (README "规则" 3).
    fn ended<V: Views>(&mut self, selling: bool, e: u64, code: u64, v: &V, ctx: &mut Ctx<'_>) {
        let c = if selling { desc::SELL_END } else { desc::BUY_END };
        self.note(ctx, c, [e, self.cum.0 as u64, code]);
        self.cum = Qty(0);
        if selling || !self.sell_back {
            self.finish();
            return;
        }
        self.sell(v, ctx);
    }

    /// The spend buy at the best ask (README "规则" 2); waits while the quote is missing or
    /// unhealthy.
    fn buy<V: Views>(&mut self, v: &V, ctx: &mut Ctx<'_>) {
        let b = v.bbo(self.inst);
        if b.ask_px.0 <= 0 || !v.quote_ok(self.account, self.inst) {
            return;
        }
        let qty = OrderIntent::spend_qty(self.spend, b.ask_px, v.inst(self.inst));
        if qty.0 <= 0 {
            self.note(ctx, desc::SKIPPED, [skip::BUY_QTY_ZERO, b.ask_px.0 as u64, 0]);
            self.finish();
            return;
        }
        let it = OrderIntent::spend_buy(
            self.venue,
            self.account,
            self.group,
            self.inst,
            Tif::Ioc,
            b.ask_px,
            qty,
            TAG_BUY,
        );
        if let Ok(h) = ctx.place(it) {
            self.handle = h.0;
            self.phase = Phase::Buying;
            self.rounds = self.rounds.wrapping_add(1);
            self.note(
                ctx,
                desc::BUY_SENT,
                [b.ask_px.0 as u64, qty.0 as u64, self.spend.0 as u64],
            );
        }
    }

    /// The sell-back of the leg's position, rounded down to the lot, at the best bid.
    fn sell<V: Views>(&mut self, v: &V, ctx: &mut Ctx<'_>) {
        let pos = v
            .group_pos(self.account, self.group)
            .legs
            .first()
            .map_or(0, |l| l.qty.0);
        let lot = v.inst(self.inst).lot.max(1);
        let qty = pos.max(0) / lot * lot;
        if qty <= 0 {
            self.note(ctx, desc::SKIPPED, [skip::NO_POSITION, pos as u64, 0]);
            self.finish();
            return;
        }
        let b = v.bbo(self.inst);
        if b.bid_px.0 <= 0 {
            self.note(ctx, desc::SKIPPED, [skip::NO_BID, qty as u64, 0]);
            self.finish();
            return;
        }
        let it = OrderIntent {
            venue: self.venue,
            account: self.account,
            side: Side::Sell,
            tif: Tif::Ioc,
            class: IntentClass::Reduce,
            flags: 0,
            _pad: [0; 2],
            inst: self.inst,
            group: self.group,
            px: b.bid_px,
            qty: Qty(qty),
            link: LegLinkOpt::NONE,
            tag: TAG_SELL,
            max_rest_ms: 0,
        };
        match ctx.place(it) {
            Ok(h) => {
                self.handle = h.0;
                self.phase = Phase::Selling;
                self.note(ctx, desc::SELL_SENT, [b.bid_px.0 as u64, qty as u64, pos as u64]);
            }
            // No room in this event: the position stays; the round ends (the Note says so).
            Err(_) => {
                self.note(ctx, desc::SKIPPED, [skip::NO_BID, qty as u64, 1]);
                self.finish();
            }
        }
    }

    /// Round over: `Done` while `enabled` is 1, else `Idle`.
    fn finish(&mut self) {
        self.r = None;
        self.phase = if self.enabled { Phase::Done } else { Phase::Idle };
    }

    fn note(&self, ctx: &mut Ctx<'_>, code: u32, v3: [u64; 3]) {
        ctx.note(
            code,
            NoteLevel::Info,
            self.group,
            self.inst,
            [v3[0], v3[1], v3[2], u64::from(self.rounds), 0, 0, 0, 0],
        );
    }

    /// `phase: u8`, `rounds: u32`, `cum: i64`, `has_r: u8` and, when set, the open order's
    /// `venue: u8, account: u8, slot: u32, gen: u32`.
    pub fn snapshot(&self, w: &mut SnapshotWriter) {
        w.put_u8(self.phase as u8);
        w.put_u32(self.rounds);
        w.put_i64(self.cum.0);
        match self.r {
            Some(r) => {
                w.put_u8(1);
                w.put_u8(r.venue.0);
                w.put_u8(r.account.0);
                w.put_u32(r.slot);
                w.put_u32(r.r#gen);
            }
            None => w.put_u8(0),
        }
    }

    /// Restores the round. A restart disarms an armed instance (re-arm by `enabled` 0 → 1) and
    /// ends a step whose place outcome was not read yet (its order, if any, is the engine's);
    /// an open order keeps being followed.
    pub fn restore(&mut self, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        let phase = Phase::of(r.get_u8()?).ok_or(RestoreError::BadValue)?;
        self.rounds = r.get_u32()?;
        self.cum = Qty(r.get_i64()?);
        self.r = if r.get_u8()? != 0 {
            let venue = VenueId(r.get_u8()?);
            let account = AccountId(r.get_u8()?);
            let slot = r.get_u32()?;
            let g = r.get_u32()?;
            Some(OrderRef::new(venue, account, slot, g))
        } else {
            None
        };
        // `enabled` is that of the parameters the instance was built with.
        match phase {
            Phase::BuyLive | Phase::SellLive if self.r.is_some() => self.phase = phase,
            Phase::Idle | Phase::Armed => {
                self.r = None;
                self.phase = Phase::Idle;
            }
            _ => self.finish(),
        }
        Ok(())
    }
}
