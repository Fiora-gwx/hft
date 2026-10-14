//! `probe-taker` (README "5. 解耦门槛夹具"): when leg A's bid exceeds leg B's ask by more than
//! `spread_th`, sells A with a `LegLink` whose OnFill second leg buys B; on `Exec`, `External` or
//! `Risk` events selected by `close_on` it sets a 0/0 exit target. In the same event as an opening
//! it arms a hold timer and writes `Note{Opened}`. With `close_on` bit 3 the hold timer of a held
//! group sets the same exit target and re-arms itself as a cool-down, after which the group may
//! open again (so a run trades in cycles). Uses only hft-strategy-sdk (and hft-strat-lib).
//! `ProbeNp` is the same code with `non_prod_only = true` and its own kind_id.

use hft_strategy_sdk::prelude::*;
use hft_strategy_sdk::{
    Ctx, EvMask, EventRef, ExecSpec, GroupTable, HedgeKind, HedgePlan, IntentClass, LegLink, LegLinkOpt,
    LegTarget, LinkRole, NoModel, NoteDesc, OnFail, OrderIntent, ParamError, Part, PxMode, RefView,
    ResidualPolicy, RestoreError, SnapshotReader, SnapshotWriter, Strategy, StrategyDesc, StrategyInstance,
    SubsBuilder, TakerSliceParams, TargetIntent, TargetReason, Urgency, Views,
};

macro_rules! taker_params {
    ($name:ident, $fields:ident, $sid:expr) => {
        hft_strategy_sdk::define_params! {
            pub struct $name(schema_id = $sid, version = 1, fields = $fields) {
                /// opening threshold: (A bid − B ask) / B ask must exceed it
                spread_th: Rate [min = 0, max = 1_000_000_000, default = 1_000_000, per_group],
                /// first-leg quantity in lots of leg A
                qty_lots: Count [min = 1, max = 1_000, default = 1, per_group],
                /// close on events: bit 0 Exec, bit 1 External, bit 2 Risk, bit 3 hold timer (then cool down)
                close_on: Count [min = 0, max = 15, default = 7],
                /// hold timer after an opening (ms)
                hold_ms: DurationMs [min = 0, max = 3_600_000, default = 5_000],
            }
        }
        impl $name {
            /// `hold_ms` in (0, 10) is rejected (field 3).
            pub fn check(&self, _groups: &GroupTable, _rd: &RefView<'_>) -> Result<(), ParamError> {
                let h = self.defaults().hold_ms;
                if h > 0 && h < 10 {
                    return Err(ParamError::Invalid {
                        field_ix: 3,
                        code: 1,
                    });
                }
                Ok(())
            }
        }
        impl TakerCfg for $name {
            fn cfg(&self, gix: u16) -> TakerCfgV {
                let f = self.at(gix);
                TakerCfgV {
                    spread_th: f.spread_th.0,
                    qty_lots: f.qty_lots,
                    close_on: f.close_on,
                    hold_ms: f.hold_ms,
                }
            }
        }
    };
}

/// The parameter values the strategy reads (the same fields in both schemas).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TakerCfgV {
    pub spread_th: i64,
    pub qty_lots: u32,
    pub close_on: u32,
    pub hold_ms: u32,
}

/// Access to the taker fields of `TakerParams` / `NpParams`.
pub trait TakerCfg: hft_strategy_sdk::ParamSet + Clone + core::fmt::Debug {
    fn cfg(&self, gix: u16) -> TakerCfgV;
}

taker_params!(TakerParams, TakerFields, 0xFF01);
taker_params!(NpParams, NpFields, 0xFF03);

/// `close_on` bits.
pub const CLOSE_EXEC: u32 = 1;
pub const CLOSE_EXTERNAL: u32 = 2;
pub const CLOSE_RISK: u32 = 4;
/// Exit when the hold timer fires, then cool down for another `hold_ms` before reopening.
pub const CLOSE_HOLD: u32 = 8;
/// Note code of an opening.
pub const NOTE_OPENED: u32 = 1;
/// Timer tag of the hold timer.
pub const TAG_HOLD: u32 = 1;

pub static NOTES: [NoteDesc; 1] = [NoteDesc {
    code: NOTE_OPENED,
    name: "Opened",
    level: NoteLevel::Info,
    v: &["leg A bid", "leg B ask", "spread"],
    doc: "a two-leg opening was sent",
}];

/// Groups per instance (preallocated).
pub const MAX_GROUPS: usize = 16;

/// Fast-part state of one group.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TakerGroup {
    pub id: u32,
    pub a: (u8, u32),
    pub b: (u8, u32),
    pub held: bool,
    /// Closed by its hold timer; no new opening until the cool-down timer fires.
    pub cooling: bool,
    pub opens: u32,
    pub closes: u32,
}

/// The strategy, generic over its descriptor variant (`NP = true` → `probe-np`).
#[derive(Debug)]
pub struct ProbeTakerT<P: TakerCfg> {
    pub inst: StrategyInstance,
    pub groups: [TakerGroup; MAX_GROUPS],
    pub n: u16,
    p: P,
}

pub type ProbeTaker = ProbeTakerT<TakerParams>;
pub type ProbeNp = ProbeTakerT<NpParams>;

const fn desc(np: bool) -> StrategyDesc {
    StrategyDesc {
        name: if np { "probe-np" } else { "probe-taker" },
        kind_id: if np { 0xFF03 } else { 0xFF01 },
        version: 1,
        non_prod_only: np,
        schema: if np {
            &NpParams::SCHEMA
        } else {
            &TakerParams::SCHEMA
        },
        notes: &NOTES,
        report: &[],
        cmds: &[],
    }
}

/// `(a_bid − b_ask) × 10^9 / b_ask` (Rate raw, truncated), 0 when B's ask is not positive.
pub fn spread(a_bid: Px, b_ask: Px) -> i64 {
    if b_ask.0 <= 0 {
        return 0;
    }
    let d = (i128::from(a_bid.0) - i128::from(b_ask.0)) * 1_000_000_000 / i128::from(b_ask.0);
    d.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}

fn exit_spec() -> ExecSpec {
    ExecSpec::TakerSlice(TakerSliceParams {
        depth_frac: Rate(1_000_000_000),
        depth_levels: 1,
        _pad0: [0; 7],
        max_cross: Rate(5_000_000),
        max_chunk_notional: Notional(i64::MAX / 4),
        min_chunk_notional: Notional(0),
        cooldown_ms: 0,
        _pad1: 0,
    })
}

impl<P: TakerCfg> ProbeTakerT<P> {
    fn load(&mut self, groups: &GroupTable) {
        let mut n = 0u16;
        for (gix, g) in groups.iter() {
            let (Some(a), Some(b)) = (g.legs.first(), g.legs.get(1)) else {
                continue;
            };
            if g.n_legs < 2 {
                continue;
            }
            if let Some(slot) = self.groups.get_mut(usize::from(gix)) {
                let keep = slot.id == g.id.0;
                *slot = TakerGroup {
                    id: g.id.0,
                    a: (a.venue.0, a.inst.0),
                    b: (b.venue.0, b.inst.0),
                    held: keep && slot.held,
                    cooling: keep && slot.cooling,
                    opens: if keep { slot.opens } else { 0 },
                    closes: if keep { slot.closes } else { 0 },
                };
                n = n.max(gix + 1);
            }
        }
        self.n = n;
    }

    fn open<V: Views>(&mut self, gix: u16, v: &V, ctx: &mut Ctx<'_>) {
        let Some(g) = self.groups.get(usize::from(gix)).copied() else {
            return;
        };
        let f = self.p.cfg(gix);
        let (a, b) = (InstId(g.a.1), InstId(g.b.1));
        let ba = v.bbo(a);
        let bb = v.bbo(b);
        let s = spread(ba.bid_px, bb.ask_px);
        if s <= f.spread_th {
            return;
        }
        let gid = GroupId(g.id);
        let lot = v.inst(a).lot.max(1);
        let link = LegLinkOpt::some(LegLink {
            group: gid,
            role: LinkRole::First,
            _pad: [0; 3],
            tolerance: Rate(0),
            hedge: HedgePlan {
                kind: HedgeKind::OnFill,
                px_mode: PxMode::Cross,
                on_fail: OnFail::Retry,
                tif: Tif::Ioc,
                venue: VenueId(g.b.0),
                account: self.inst.account,
                side: Side::Buy,
                reduce_only: 0,
                inst: b,
                ratio_num: 1,
                ratio_den: 1,
                residual: ResidualPolicy::TrimExcess,
                _pad: [0; 3],
                px_param: Rate(5_000_000),
                max_slip: Rate(5_000_000),
                limit_px: Px(0),
            },
        });
        let it = OrderIntent {
            venue: VenueId(g.a.0),
            account: self.inst.account,
            side: Side::Sell,
            tif: Tif::Ioc,
            class: IntentClass::Open,
            flags: 0,
            _pad: [0; 2],
            inst: a,
            group: gid,
            px: ba.bid_px,
            qty: Qty(lot.saturating_mul(i64::from(f.qty_lots))),
            link,
            tag: 0,
            max_rest_ms: 0,
        };
        if ctx.place(it).is_err() {
            return;
        }
        self.arm_hold(gix, v, ctx);
        ctx.note(
            NOTE_OPENED,
            NoteLevel::Info,
            gid,
            a,
            [ba.bid_px.0 as u64, bb.ask_px.0 as u64, s as u64, 0, 0, 0, 0, 0],
        );
        if let Some(g) = self.groups.get_mut(usize::from(gix)) {
            g.held = true;
            g.opens += 1;
        }
    }

    /// Arms the hold timer of `gix` `hold_ms` from now.
    fn arm_hold<V: Views>(&self, gix: u16, v: &V, ctx: &mut Ctx<'_>) {
        let Some(g) = self.groups.get(usize::from(gix)) else {
            return;
        };
        let now = v.now();
        let hold = i64::from(self.p.cfg(gix).hold_ms);
        let at = now.tsc_of(now.engine_ns.saturating_add(hold.saturating_mul(1_000_000)));
        let _ = ctx.timer_at(at, TAG_HOLD, GroupId(g.id));
    }

    /// Hold timer: a held group is closed and cools down for another `hold_ms`; a cooling group
    /// may open again on its next quote.
    fn on_hold<V: Views>(&mut self, gix: u16, v: &V, ctx: &mut Ctx<'_>) {
        let Some(g) = self.groups.get(usize::from(gix)).copied() else {
            return;
        };
        if g.cooling {
            if let Some(g) = self.groups.get_mut(usize::from(gix)) {
                g.cooling = false;
            }
        } else if g.held && self.close(gix, ctx) {
            if let Some(g) = self.groups.get_mut(usize::from(gix)) {
                g.cooling = true;
            }
            self.arm_hold(gix, v, ctx);
        }
    }

    /// Sets the 0/0 exit target of a held group; `true` when it was accepted.
    fn close(&mut self, gix: u16, ctx: &mut Ctx<'_>) -> bool {
        let Some(g) = self.groups.get_mut(usize::from(gix)) else {
            return false;
        };
        if !g.held {
            return false;
        }
        let t = TargetIntent {
            account: self.inst.account,
            _pad0: [0; 3],
            group: GroupId(g.id),
            legs: [
                LegTarget::new(0, Qty(0)),
                LegTarget::new(1, Qty(0)),
                LegTarget::ZERO,
                LegTarget::ZERO,
            ],
            n_legs: 2,
            reason: TargetReason::Exit,
            urgency: Urgency::Normal,
            _pad1: [0; 5],
            band: Rate(0),
            exec: exit_spec(),
            tag: 0,
            _pad2: 0,
        };
        if ctx.target(t).is_ok() {
            g.held = false;
            g.closes += 1;
            return true;
        }
        false
    }
}

/// Descriptor selection per parameter type.
pub trait TakerKind: TakerCfg {
    const NP: bool;
}
impl TakerKind for TakerParams {
    const NP: bool = false;
}
impl TakerKind for NpParams {
    const NP: bool = true;
}

impl<P: TakerKind> Strategy for ProbeTakerT<P> {
    const DESC: StrategyDesc = desc(P::NP);
    type Params = P;
    type Model = NoModel<P>;

    fn new(
        inst: StrategyInstance,
        p: &P,
        groups: &GroupTable,
        _rd: &RefView<'_>,
    ) -> Result<Self, ParamError> {
        let mut s = ProbeTakerT {
            inst,
            groups: [TakerGroup::default(); MAX_GROUPS],
            n: 0,
            p: p.clone(),
        };
        s.load(groups);
        Ok(s)
    }

    fn subscriptions(&self, groups: &GroupTable, out: &mut SubsBuilder<'_>) {
        for (gix, g) in groups.iter() {
            for l in g.legs.iter().take(usize::from(g.n_legs).min(2)) {
                let _ = out.market(l.inst, gix, EvMask::BBO, Part::Fast);
            }
        }
    }

    fn on_event<V: Views>(&mut self, ev: &EventRef<'_>, v: &V, ctx: &mut Ctx<'_>) {
        let close_on = self.p.cfg(0).close_on;
        match *ev {
            EventRef::Bbo { gix, .. } => {
                if self
                    .groups
                    .get(usize::from(gix))
                    .is_some_and(|g| g.id != 0 && !g.held && !g.cooling)
                {
                    self.open(gix, v, ctx);
                }
            }
            EventRef::Exec { gix, .. } if close_on & CLOSE_EXEC != 0 => {
                self.close(gix, ctx);
            }
            EventRef::External { gix, .. } if close_on & CLOSE_EXTERNAL != 0 => {
                self.close(gix, ctx);
            }
            EventRef::Risk { .. } if close_on & CLOSE_RISK != 0 => {
                for gix in 0..self.n {
                    self.close(gix, ctx);
                }
            }
            EventRef::Timer {
                tag: TAG_HOLD, gix, ..
            } if close_on & CLOSE_HOLD != 0 => self.on_hold(gix, v, ctx),
            _ => {}
        }
    }

    fn on_params(&mut self, p: &P, groups: &GroupTable, _rd: &RefView<'_>) -> Result<(), ParamError> {
        self.p = p.clone();
        self.load(groups);
        Ok(())
    }

    fn snapshot(&self, w: &mut SnapshotWriter) {
        w.put_u16(self.n);
        for g in self.groups.iter().take(usize::from(self.n)) {
            w.put_u32(g.id);
            // bit 0 held, bit 1 cooling (one byte, the layout of version 1).
            w.put_u8(u8::from(g.held) | u8::from(g.cooling) << 1);
            w.put_u32(g.opens);
            w.put_u32(g.closes);
        }
    }

    fn restore(&mut self, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        let n = r.get_u16()?;
        for g in self.groups.iter_mut().take(usize::from(n)) {
            let id = r.get_u32()?;
            let state = r.get_u8()?;
            let opens = r.get_u32()?;
            let closes = r.get_u32()?;
            if g.id == id {
                g.held = state & 1 != 0;
                g.cooling = state & 2 != 0;
                g.opens = opens;
                g.closes = closes;
            }
        }
        Ok(())
    }
}
