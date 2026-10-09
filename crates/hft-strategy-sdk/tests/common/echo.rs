//! `EchoProbe` (kind 0xFF01, `DESC.version = 3`, parameters `TP`): order intents only. On a Bbo
//! of its group it fires one Open IOC when the relative spread (ask − bid) / mid ≤ `open_th`
//! and the group's cooldown (`cooldown_ms`, kept in engine_ns) has passed; a checked-arithmetic
//! overflow drops the group's action and writes `EchoOverflow`. Commands: op 1 `open`, op 2
//! `close` (reduce only), op 3 `set_mode`.
//!
//! Snapshot (version 3): `n: u32`, `mode: u8`, `_pad: [u8; 3]`, then per group in gix order
//! `{id: u32, _pad: u32, fp: u64, cooldown_until_ens: i64, fills: i64, timer_hits: u32,
//! _pad: u32}`. Version 2 lacks `fills`/`timer_hits`/the trailing pad.

use hft_strategy_sdk::*;
use hft_types::{GROUP_NONE, GroupId, InstId, Px, Qty, Rate, Side, Tif, Tsc};

use super::params::{ECHO_CMDS, ECHO_NOTES, ECHO_REPORT, TP, TPParams};

/// Note codes.
pub const ECHO_FIRE: u32 = 1;
pub const ECHO_OVERFLOW: u32 = 2;
pub const ECHO_NOFEED: u32 = 3;
/// Timer tag registered at each fire (fires at the cooldown deadline).
pub const COOLDOWN_TAG: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EchoGroup {
    pub id: GroupId,
    pub fp: u64,
    pub leg0: InstId,
    pub cooldown_until_ens: i64,
    pub fills: i64,
    pub timer_hits: u32,
}

impl EchoGroup {
    fn fresh(id: GroupId, fp: u64, leg0: InstId) -> EchoGroup {
        EchoGroup {
            id,
            fp,
            leg0,
            cooldown_until_ens: i64::MIN,
            fills: 0,
            timer_hits: 0,
        }
    }
}

#[derive(Debug)]
pub struct EchoProbe {
    pub inst: StrategyInstance,
    pub p: TPParams,
    pub groups: Vec<EchoGroup>,
    pub mode: u8,
    /// Subscriptions refused (`NoFeed`), noted at the next event.
    pub nofeed: Vec<InstId>,
}

impl EchoProbe {
    pub const DESC: StrategyDesc = StrategyDesc {
        name: "echo_probe",
        kind_id: 0xFF01,
        version: 3,
        non_prod_only: false,
        schema: &TP,
        notes: &ECHO_NOTES,
        report: &ECHO_REPORT,
        cmds: &ECHO_CMDS,
    };

    fn align(&mut self, groups: &GroupTable) {
        let mut next = Vec::with_capacity(usize::from(groups.capacity()));
        for (_, g) in groups.iter() {
            let fp = g.fingerprint();
            let leg0 = g.legs[0].inst;
            let old = self.groups.iter().find(|x| x.id == g.id && x.fp == fp).copied();
            next.push(old.map_or(EchoGroup::fresh(g.id, fp, leg0), |o| EchoGroup { leg0, ..o }));
        }
        self.groups = next;
    }

    fn fire<V: Views>(&mut self, gix: u16, b: &BboView, v: &V, ctx: &mut Ctx<'_>) {
        let Some(g) = self.groups.get(usize::from(gix)).copied() else {
            return;
        };
        let f = *self.p.at(gix);
        let mid = b.bid_px.0.checked_add(b.ask_px.0).map(|s| s / 2);
        let spread = b.ask_px.0.checked_sub(b.bid_px.0);
        let (Some(mid), Some(spread)) = (mid, spread) else {
            ctx.note(ECHO_OVERFLOW, NoteLevel::P2, g.id, g.leg0, [0; 8]);
            return;
        };
        if !f.enabled || mid <= 0 || spread < 0 {
            return;
        }
        // spread / mid ≤ open_th  ⇔  spread × 10^9 ≤ mid × open_th (i128).
        if i128::from(spread) * 1_000_000_000 > i128::from(mid) * i128::from(f.open_th.0) {
            return;
        }
        let now = v.now();
        if now.engine_ns < g.cooldown_until_ens {
            return;
        }
        let Some(until) = now.engine_ns.checked_add(time::ms(f.cooldown_ms)) else {
            ctx.note(ECHO_OVERFLOW, NoteLevel::P2, g.id, g.leg0, [0; 8]);
            return;
        };
        let qty = if f.max_qty.0 > 0 { f.max_qty } else { Qty(1_000) };
        let it = order(
            self.inst,
            g.id,
            g.leg0,
            Side::Buy,
            IntentClass::Open,
            b.ask_px,
            qty,
            0,
        );
        if ctx.place(it).is_err() {
            return;
        }
        if let Some(s) = self.groups.get_mut(usize::from(gix)) {
            s.cooldown_until_ens = until;
        }
        let _ = ctx.timer_at(now.tsc_of(until), COOLDOWN_TAG, g.id);
        ctx.note(
            ECHO_FIRE,
            NoteLevel::Info,
            g.id,
            g.leg0,
            [mid as u64, f.open_th.0 as u64, 0, 0, 0, 0, 0, 0],
        );
    }
}

/// A single-leg order intent.
#[allow(clippy::too_many_arguments)]
pub fn order(
    inst: StrategyInstance,
    group: GroupId,
    i: InstId,
    side: Side,
    class: IntentClass,
    px: Px,
    qty: Qty,
    flags: u8,
) -> OrderIntent {
    OrderIntent {
        venue: hft_types::VenueId(0),
        account: inst.account,
        side,
        tif: Tif::Ioc,
        class,
        flags,
        _pad: [0; 2],
        inst: i,
        group,
        px,
        qty,
        link: LegLinkOpt::NONE,
        tag: 0,
        max_rest_ms: 0,
    }
}

impl Strategy for EchoProbe {
    const DESC: StrategyDesc = EchoProbe::DESC;
    type Params = TPParams;
    type Model = NoModel<TPParams>;

    fn new(
        inst: StrategyInstance,
        p: &TPParams,
        groups: &GroupTable,
        _: &RefView<'_>,
    ) -> Result<Self, ParamError> {
        let mut s = EchoProbe {
            inst,
            p: p.clone(),
            groups: Vec::new(),
            mode: 0,
            nofeed: Vec::new(),
        };
        s.align(groups);
        Ok(s)
    }
    fn subscriptions(&self, groups: &GroupTable, out: &mut SubsBuilder<'_>) {
        for (gix, g) in groups.iter() {
            let _ = out.market(g.legs[0].inst, gix, EvMask::BBO, Part::Fast);
        }
    }
    fn on_event<V: Views>(&mut self, ev: &EventRef<'_>, v: &V, ctx: &mut Ctx<'_>) {
        for i in core::mem::take(&mut self.nofeed) {
            ctx.note(
                ECHO_NOFEED,
                NoteLevel::P2,
                GROUP_NONE,
                i,
                [u64::from(i.0), 0, 0, 0, 0, 0, 0, 0],
            );
        }
        match *ev {
            EventRef::Bbo { gix, b, .. } => self.fire(gix, &b, v, ctx),
            EventRef::Order { gix, u } => {
                if let (Some(g), OrderUpdateKind::Filled { d, .. }) =
                    (self.groups.get_mut(usize::from(gix)), u.kind)
                {
                    g.fills = g.fills.saturating_add(d.0);
                }
            }
            EventRef::Timer {
                tag: COOLDOWN_TAG,
                gix,
                ..
            } => {
                if let Some(g) = self.groups.get_mut(usize::from(gix)) {
                    g.timer_hits = g.timer_hits.saturating_add(1);
                }
            }
            EventRef::Mode { gix, new, .. } => {
                if new != hft_types::RunMode::Live
                    && let Some(g) = self.groups.get_mut(usize::from(gix))
                {
                    g.cooldown_until_ens = i64::MIN;
                }
            }
            EventRef::Manual {
                gix,
                payload,
                reduce_only,
            } => {
                let Ok(c) = CmdView::parse(payload) else { return };
                let Some(g) = self.groups.get(usize::from(gix)).copied() else {
                    if c.op() == 3 {
                        self.mode = c.val(0).map_or(0, |m| m as u8);
                    }
                    return;
                };
                let px = v.bbo(g.leg0).ask_px;
                match (c.op(), reduce_only) {
                    (1, false) => {
                        let n = c.val(0).unwrap_or(0);
                        let qty = if px.0 > 0 { Qty(n / px.0) } else { Qty(0) };
                        let _ = ctx.place(order(
                            self.inst,
                            g.id,
                            g.leg0,
                            Side::Buy,
                            IntentClass::Open,
                            px,
                            qty,
                            0,
                        ));
                    }
                    (2, _) => {
                        let it = order(
                            self.inst,
                            g.id,
                            g.leg0,
                            Side::Sell,
                            IntentClass::Reduce,
                            v.bbo(g.leg0).bid_px,
                            Qty(1_000),
                            OrderIntent::REDUCE_ONLY,
                        );
                        let _ = ctx.place(it);
                    }
                    (3, _) => self.mode = c.val(0).map_or(0, |m| m as u8),
                    _ => {}
                }
            }
            _ => {}
        }
    }
    fn on_params(&mut self, p: &TPParams, groups: &GroupTable, _: &RefView<'_>) -> Result<(), ParamError> {
        self.p = p.clone();
        self.align(groups);
        Ok(())
    }
    fn snapshot(&self, w: &mut SnapshotWriter) {
        w.put_u32(self.groups.len() as u32);
        w.put_u8(self.mode);
        w.put_bytes(&[0; 3]);
        for g in &self.groups {
            w.put_u32(g.id.0);
            w.put_u32(0);
            w.put_u64(g.fp);
            w.put_i64(g.cooldown_until_ens);
            w.put_i64(g.fills);
            w.put_u32(g.timer_hits);
            w.put_u32(0);
        }
    }
    fn restore(&mut self, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        let ver = r.desc_version();
        if ver != 3 && ver != 2 {
            return Err(RestoreError::VersionUnsupported);
        }
        let n = r.get_u32()?;
        let mode = r.get_u8()?;
        r.get_bytes(3)?;
        let mut saved = Vec::new();
        for _ in 0..n {
            let id = GroupId(r.get_u32()?);
            r.get_u32()?;
            let fp = r.get_u64()?;
            let cooldown_until_ens = r.get_i64()?;
            let (fills, timer_hits) = if ver == 3 {
                let f = r.get_i64()?;
                let t = r.get_u32()?;
                r.get_u32()?;
                (f, t)
            } else {
                (0, 0)
            };
            saved.push(EchoGroup {
                id,
                fp,
                leg0: InstId(0),
                cooldown_until_ens,
                fills,
                timer_hits,
            });
        }
        if !r.is_empty() {
            return Err(RestoreError::BadValue);
        }
        self.mode = mode;
        for g in &mut self.groups {
            if let Some(s) = saved.iter().find(|s| s.id == g.id && s.fp == g.fp) {
                *g = EchoGroup { leg0: g.leg0, ..*s };
            }
        }
        Ok(())
    }
}

impl EchoProbe {
    /// SDK-103 shape check: two legs, leg 0 spot, leg 1 USDT perp, same asset; `enabled` with an
    /// effective `open_th` of 0 → `Invalid{0, 1}`.
    pub fn check_params(p: &TPParams, groups: &GroupTable, rd: &RefView<'_>) -> Result<(), ParamError> {
        for (gix, g) in groups.iter() {
            if g.n_legs != 2 {
                return Err(ParamError::GroupShape { gix, code: 1 });
            }
            let (Some(l0), Some(l1)) = (rd.try_inst(g.legs[0].inst), rd.try_inst(g.legs[1].inst)) else {
                return Err(ParamError::GroupShape { gix, code: 2 });
            };
            if l0.kind != hft_refdata::InstKind::Spot || l1.kind != hft_refdata::InstKind::Perp {
                return Err(ParamError::GroupShape { gix, code: 2 });
            }
            if l0.asset != g.asset || l1.asset != g.asset {
                return Err(ParamError::GroupShape { gix, code: 3 });
            }
        }
        for (gix, _) in groups.iter() {
            let f = p.at(gix);
            if f.enabled && f.open_th == Rate(0) {
                return Err(ParamError::Invalid { field_ix: 0, code: 1 });
            }
        }
        Ok(())
    }
}

/// `TPParams::check` (hooked into `ParamSet::validate`).
impl TPParams {
    pub fn check(&self, groups: &GroupTable, rd: &RefView<'_>) -> Result<(), ParamError> {
        EchoProbe::check_params(self, groups, rd)
    }
}

/// A 1 000-unit-wide quote around `mid` (spread / mid well under the default `open_th`).
pub fn quote(mid: i64) -> BboView {
    BboView {
        bid_px: Px(mid - 50),
        bid_qty: Qty(100_000_000),
        ask_px: Px(mid + 50),
        ask_qty: Qty(100_000_000),
        rx_tsc: Tsc(1),
        ..BboView::EMPTY
    }
}
