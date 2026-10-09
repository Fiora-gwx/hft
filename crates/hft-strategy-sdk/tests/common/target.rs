//! `TargetProbe` (kind 0xFF02, parameters `TP2`, Model `ProbeModel`): target intents only. A
//! fresh, ready `BandAdvice` for a group sets a two-leg `PairOnFill` target (leg 0 long, leg 1
//! short, `max_qty` or 1 000 units); stale advice (`basis_seq < boot_seq`, or age over
//! `ADVICE_MAX_AGE_MS`) or an undecodable payload sets nothing. No strategy commands.
//!
//! Snapshot (version 1): `n: u32`, `_pad: u32`, per group `{id: u32, _pad: u32, fp: u64,
//! last_band: i64, n_targets: u32, n_exec: u32}`.

use hft_strategy_sdk::*;
use hft_types::{GroupId, InstId, Notional, Qty, Rate};

use super::model::{BandAdvice, NOT_READY, ProbeModelT};
use super::params::TP2Params;

/// Largest Advice age the probe accepts.
pub const ADVICE_MAX_AGE_MS: u64 = 1_000;
/// Note code: a target was set.
pub const TARGET_SET: u32 = 1;

pub static TP2: ParamSchemaDesc = TP2Params::SCHEMA;

pub const TARGET_NOTES: [NoteDesc; 1] = [NoteDesc {
    code: TARGET_SET,
    name: "TargetSet",
    level: NoteLevel::Info,
    v: &["band"],
    doc: "按建议设定目标",
}];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TGroup {
    pub id: GroupId,
    pub fp: u64,
    pub legs: [InstId; 2],
    pub last_band: i64,
    pub n_targets: u32,
    pub n_exec: u32,
}

#[derive(Debug)]
pub struct TargetProbeT<const K: u8> {
    pub inst: StrategyInstance,
    pub p: TP2Params,
    pub groups: Vec<TGroup>,
}

pub type TargetProbe = TargetProbeT<0>;

pub const TARGET_DESC: StrategyDesc = StrategyDesc {
    name: "target_probe",
    kind_id: 0xFF02,
    version: 1,
    non_prod_only: false,
    schema: &TP2,
    notes: &TARGET_NOTES,
    report: &[],
    cmds: &[],
};

impl<const K: u8> TargetProbeT<K> {
    pub const DESC: StrategyDesc = TARGET_DESC;

    fn align(&mut self, groups: &GroupTable) {
        let mut next = Vec::with_capacity(usize::from(groups.capacity()));
        for (_, g) in groups.iter() {
            let fp = g.fingerprint();
            let legs = [g.legs[0].inst, g.legs[1].inst];
            let old = self.groups.iter().find(|x| x.id == g.id && x.fp == fp).copied();
            next.push(old.map_or(
                TGroup {
                    id: g.id,
                    fp,
                    legs,
                    last_band: 0,
                    n_targets: 0,
                    n_exec: 0,
                },
                |o| TGroup { legs, ..o },
            ));
        }
        self.groups = next;
    }
}

/// Two-leg target: leg 0 long `q`, leg 1 short `q`, `PairOnFill` with `pair_spec()`.
pub fn pair_target(account: hft_types::AccountId, group: GroupId, q: i64, tag: u32) -> TargetIntent {
    TargetIntent {
        account,
        _pad0: [0; 3],
        group,
        legs: [
            LegTarget::new(0, Qty(q)),
            LegTarget::new(1, Qty(-q)),
            LegTarget::ZERO,
            LegTarget::ZERO,
        ],
        n_legs: 2,
        reason: TargetReason::Rebalance,
        urgency: Urgency::Normal,
        _pad1: [0; 5],
        band: Rate(0),
        exec: pair_spec(),
        tag,
        _pad2: 0,
    }
}

/// The probe's default target exec spec.
pub fn pair_spec() -> ExecSpec {
    ExecSpec::PairOnFill(PairParams {
        slice: TakerSliceParams {
            depth_frac: Rate(500_000_000),
            depth_levels: 5,
            _pad0: [0; 7],
            max_cross: Rate(1_000_000),
            max_chunk_notional: Notional(1_000_000_000),
            min_chunk_notional: Notional(10_000_000),
            cooldown_ms: 100,
            _pad1: 0,
        },
        first_leg: FirstLeg::ByLiquidity,
        first_leg_ix: 0,
        hedge_px_mode: PxMode::Cross,
        _pad0: [0; 5],
        hedge_cross: Rate(2_000_000),
        hedge_max_slip: Rate(5_000_000),
        ratio_num: 1,
        ratio_den: 1,
        fix_tolerance: Rate(10_000_000),
        residual: ResidualPolicy::Defer,
        _pad1: [0; 7],
    })
}

impl<const K: u8> Strategy for TargetProbeT<K> {
    const DESC: StrategyDesc = TARGET_DESC;
    type Params = TP2Params;
    type Model = ProbeModelT<K>;

    fn new(
        inst: StrategyInstance,
        p: &TP2Params,
        groups: &GroupTable,
        _: &RefView<'_>,
    ) -> Result<Self, ParamError> {
        let mut s = TargetProbeT {
            inst,
            p: p.clone(),
            groups: Vec::new(),
        };
        s.align(groups);
        Ok(s)
    }
    fn subscriptions(&self, groups: &GroupTable, out: &mut SubsBuilder<'_>) {
        for (gix, g) in groups.iter() {
            let _ = out.market(g.legs[0].inst, gix, EvMask::BBO, Part::Both);
        }
        out.model_tick(1_000);
    }
    fn on_event<V: Views>(&mut self, ev: &EventRef<'_>, v: &V, ctx: &mut Ctx<'_>) {
        match *ev {
            EventRef::Advice {
                gix,
                local_kind: 1,
                payload,
                basis_seq,
                basis_tsc,
            } => {
                let now = v.now();
                let Ok(a) = BandAdvice::decode(payload) else {
                    return;
                };
                if basis_seq < now.boot_seq || a.flags & NOT_READY != 0 {
                    return;
                }
                let age = now.tsc.0.saturating_sub(basis_tsc.0);
                if age > ADVICE_MAX_AGE_MS.saturating_mul(now.tsc_per_ms) {
                    return;
                }
                let Some(g) = self.groups.get(usize::from(gix)).copied() else {
                    return;
                };
                let q = self.p.at(gix).max_qty.0.max(1_000);
                let t = pair_target(self.inst.account, g.id, q, a.band as u32);
                if ctx.target(t).is_ok()
                    && let Some(s) = self.groups.get_mut(usize::from(gix))
                {
                    s.last_band = a.band;
                    s.n_targets = s.n_targets.saturating_add(1);
                    ctx.note(
                        TARGET_SET,
                        NoteLevel::Info,
                        g.id,
                        g.legs[0],
                        [a.band as u64, 0, 0, 0, 0, 0, 0, 0],
                    );
                }
            }
            EventRef::Exec { gix, .. } => {
                if let Some(s) = self.groups.get_mut(usize::from(gix)) {
                    s.n_exec = s.n_exec.saturating_add(1);
                }
            }
            EventRef::Mode { gix, new, .. } => {
                if new != hft_types::RunMode::Live
                    && let Some(s) = self.groups.get(usize::from(gix)).copied()
                {
                    let _ = ctx.clear_target(self.inst.account, s.id);
                }
            }
            _ => {}
        }
    }
    fn on_params(&mut self, p: &TP2Params, groups: &GroupTable, _: &RefView<'_>) -> Result<(), ParamError> {
        self.p = p.clone();
        self.align(groups);
        Ok(())
    }
    fn snapshot(&self, w: &mut SnapshotWriter) {
        w.put_u32(self.groups.len() as u32);
        w.put_u32(0);
        for g in &self.groups {
            w.put_u32(g.id.0);
            w.put_u32(0);
            w.put_u64(g.fp);
            w.put_i64(g.last_band);
            w.put_u32(g.n_targets);
            w.put_u32(g.n_exec);
        }
    }
    fn restore(&mut self, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        if r.desc_version() != 1 {
            return Err(RestoreError::VersionUnsupported);
        }
        let n = r.get_u32()?;
        r.get_u32()?;
        let mut saved = Vec::new();
        for _ in 0..n {
            let id = GroupId(r.get_u32()?);
            r.get_u32()?;
            let fp = r.get_u64()?;
            let last_band = r.get_i64()?;
            let n_targets = r.get_u32()?;
            let n_exec = r.get_u32()?;
            saved.push(TGroup {
                id,
                fp,
                legs: [InstId(0); 2],
                last_band,
                n_targets,
                n_exec,
            });
        }
        if !r.is_empty() {
            return Err(RestoreError::BadValue);
        }
        for g in &mut self.groups {
            if let Some(s) = saved.iter().find(|s| s.id == g.id && s.fp == g.fp) {
                *g = TGroup { legs: g.legs, ..*s };
            }
        }
        Ok(())
    }
}
