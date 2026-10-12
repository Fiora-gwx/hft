//! Independent quantity and notional reference for decision-time raw facts.

use hft_event::{OrderAction, OrderDecision};
use hft_sequencer::Engine;
use hft_strategy_sdk::{IntentClass, LinkRole};
use hft_types::Side;

use crate::dst::obs::{
    DecClass, DecOrigin, DecisionInfo, HedgeKind, HedgePlaceFailure, Obs, PlaceFailureKind, StepInfo,
};
use crate::dst::sut::JRec;

pub fn fill(
    e: &Engine,
    history: &super::audit::History,
    before: &Obs,
    records: &[JRec],
    info: &mut StepInfo,
) -> Result<(), String> {
    for p in e.exec.audit_broken_polls() {
        if p.phase == hft_exec::AutoFixPhase::Primed && !p.engine_target && p.wait == 0 {
            info.autofix_ready.push((p.account, p.group));
        }
    }
    if e.exec.audit_overflow() {
        return Err("force-target diagnostic buffer overflow".into());
    }
    for f in e.exec.audit_force() {
        info.required_force.push((f.account, f.group, f.inst));
        info.force_targets.push(crate::dst::obs::ForceTarget {
            account: f.account,
            group: f.group,
            inst: f.inst,
            base: i128::from(
                if f.previous_engine {
                    f.previous_target.unwrap_or(f.position)
                } else {
                    f.position
                }
                .0,
            ),
            target: i128::from(f.target.0),
            strategy_target: f.previous_target.map(|q| i128::from(q.0)),
        });
        info.force_fractions.push(f.fraction.0);
    }
    for r in records {
        if let Some(d) = r.try_read::<hft_event::ExecDecision>() {
            let (table, code, _) = hft_exec::unpack(d.detail);
            if table == hft_exec::decision::table::AUTO_FIX && code == hft_exec::decision::autofix::START {
                let account = e
                    .risk
                    .mode
                    .cells()
                    .find(|(c, _, _)| c.group == d.group)
                    .map(|(c, _, _)| c.account)
                    .ok_or("AutoFix/Start missing cell owner")?;
                info.autofix_starts.push((account, d.group));
            }
        }
    }
    if e.decision_observation_failed() {
        return Err("decision diagnostic buffer overflow or unmatched decision".into());
    }
    info.required_place_failures = e.hedge_place_failures().len() as u32;
    for (index, failure) in e.hedge_place_failures().iter().enumerate() {
        let f = failure.facts;
        let raw = f.link.ok_or("hedge place failure missing raw link identity")?;
        if raw.link.link_id != f.link_id || raw.link.account != f.account || raw.link.group != f.group {
            return Err("hedge place failure owner differs from raw link".into());
        }
        info.place_failures.push(HedgePlaceFailure {
            input_seq: f.input_seq,
            index: index as u32,
            key: history.key_at_place(raw, before.links.iter().filter_map(|l| l.key)),
            group: f.group,
            venue: f.venue,
            inst: f.inst,
            free_slots: f.free_slots,
            error: match failure.error {
                hft_oms::PlaceError::NoTable => PlaceFailureKind::NoTable,
                hft_oms::PlaceError::NoFreeSlot => PlaceFailureKind::NoFreeSlot,
                hft_oms::PlaceError::LinkTableFull => PlaceFailureKind::LinkTableFull,
            },
            retries_before: u32::from(raw.link.retries),
        });
    }
    for rec in records {
        let Some(d) = rec.try_read::<OrderDecision>() else {
            continue;
        };
        if d.action != OrderAction::New {
            continue;
        }
        let rows: Vec<_> = e
            .decision_observations()
            .iter()
            .filter(|r| r.seq == rec.seq)
            .collect();
        if rows.len() != 1 {
            return Err(format!(
                "decision {} has {} raw observations",
                rec.seq,
                rows.len()
            ));
        }
        let f = rows[0].facts;
        let class = match f.class {
            IntentClass::Open => DecClass::Open,
            IntentClass::Reduce => DecClass::Close,
            IntentClass::Hedge => DecClass::Hedge,
            IntentClass::Flatten => DecClass::Flatten,
        };
        let origin = match f.origin {
            1 => DecOrigin::Strategy,
            2 => DecOrigin::Exec,
            3 => DecOrigin::Oms,
            4 => DecOrigin::Command,
            _ => return Err(format!("decision {} has unknown origin", rec.seq)),
        };
        let net: i128 = f
            .legs
            .leg
            .iter()
            .take(usize::from(f.legs.n_legs))
            .map(|l| i128::from(l.net.0))
            .sum();
        let mut basis = net.abs();
        let mut naked_dir = if d.side == Side::Sell {
            net.max(0)
        } else {
            (-net).max(0)
        };
        let mut hedge = match f.exec_kind {
            Some(hft_exec::ExecOrderKind::HedgeFix) => Some(HedgeKind::HedgeFix),
            Some(hft_exec::ExecOrderKind::ResidualTrim) => Some(HedgeKind::ResidualTrim),
            Some(hft_exec::ExecOrderKind::ExternalDefault) => Some(HedgeKind::ExternalDefault),
            Some(hft_exec::ExecOrderKind::CmdHedgeFix) => Some(HedgeKind::CmdHedgeFix),
            Some(hft_exec::ExecOrderKind::AutoFix) => Some(HedgeKind::AutoFix),
            _ => None,
        };
        if let Some((role, raw)) = f.hedge {
            let raw = raw.ok_or_else(|| format!("decision {} has no raw hedge facts", rec.seq))?;
            let p = raw.row.plan;
            let l = raw.row.link;
            let num = i128::from(p.ratio_num);
            let den = i128::from(p.ratio_den);
            if num <= 0 || den <= 0 {
                return Err("invalid hedge ratio in observation".into());
            }
            // Simultaneous second legs are Open, not a hedge against already filled inventory.
            if p.kind != hft_strategy_sdk::HedgeKind::Simultaneous || role == LinkRole::Reverse {
                hedge = match role {
                    LinkRole::Second => Some(HedgeKind::Second),
                    LinkRole::SecondRetry => Some(HedgeKind::SecondRetry),
                    LinkRole::Reverse => Some(HedgeKind::Reverse),
                    _ => None,
                };
            }
            basis = match role {
                LinkRole::Second => {
                    raw.first_after * num / den - raw.first_before * num / den + raw.residual_before
                }
                LinkRole::SecondRetry => {
                    (i128::from(l.hedge_req.0) - i128::from(l.hedge_filled.0) - i128::from(l.hedge_open.0))
                        .max(0)
                }
                LinkRole::Reverse if d.inst == raw.first_inst => {
                    i128::from(l.first_net.0) - i128::from(l.hedge_filled.0) * den / num
                }
                LinkRole::Reverse => i128::from(l.hedge_filled.0) - i128::from(l.first_net.0) * num / den,
                _ => basis,
            };
            let fx = f
                .leg_insts
                .iter()
                .position(|i| *i == raw.first_inst)
                .ok_or_else(|| format!("decision {} missing first leg mapping", rec.seq))?;
            let hx = f
                .leg_insts
                .iter()
                .position(|i| *i == p.inst)
                .ok_or_else(|| format!("decision {} missing hedge leg mapping", rec.seq))?;
            let first = f.legs.leg[fx];
            let second = f.legs.leg[hx];
            let exposure = i128::from(first.net.0) * num / den + i128::from(second.net.0);
            let fi = i128::from(first.inflight_immediate.0) * num / den;
            let hi = i128::from(second.inflight_immediate.0);
            naked_dir = if d.side == Side::Sell {
                (exposure + fi.min(0) + hi.min(0)).max(0)
            } else {
                (-exposure - fi.max(0) - hi.max(0)).max(0)
            };
        }
        // qty * px * quote_rate at quote exponent -6, with one final round toward +infinity.
        let product = i128::from(d.qty.0)
            .abs()
            .saturating_mul(i128::from(d.px.0).abs())
            .saturating_mul(i128::from(f.quote_rate.0).abs());
        let scale = i32::from(f.exp.qty_exp) + i32::from(f.exp.px_exp) - 3;
        let notional = if scale >= 0 {
            product.saturating_mul(10i128.pow(scale as u32))
        } else {
            let divisor = 10i128
                .checked_pow((-scale) as u32)
                .ok_or("notional exponent overflow")?;
            product / divisor + i128::from(product % divisor != 0)
        }
        .min(i128::from(i64::MAX));
        let inflight = match (class, d.side) {
            (DecClass::Open, Side::Buy) => f.open_buy,
            (DecClass::Open, Side::Sell) => f.open_sell,
            (_, Side::Buy) => f.imm_buy,
            (_, Side::Sell) => f.imm_sell,
        };
        info.decisions.push(DecisionInfo {
            seq: rec.seq,
            class,
            origin,
            hedge,
            margin_stress: f.margin_stress,
            basis,
            naked_dir,
            pos: i128::from(f.pos.0),
            same_dir_inflight: i128::from(inflight.0),
            max_pos_qty: i128::from(f.max_pos_qty.0),
            notional,
            link: f.link,
            link_unknown: f.legs.has_unknown,
        });
        info.decision_inflight
            .push((rec.seq, f.legs.leg.map(|l| i128::from(l.inflight_immediate.0))));
        info.decision_limits.push((
            rec.seq,
            i128::from(f.max_hedge_notional),
            i128::from(f.max_reduce_notional),
        ));
        let perp_pending = f.exec_cell.is_some_and(|c| {
            c.legs
                .iter()
                .enumerate()
                .take(usize::from(c.n_legs))
                .any(|(ix, l)| {
                    l.kind == hft_exec::LEG_PERP
                        && c.target_of(ix).is_some_and(|t| {
                            (i128::from(f.group_positions[ix].0) - i128::from(t.0)).abs()
                                >= i128::from(l.lot.0.max(1))
                        })
                })
        });
        info.decision_perp_pending.push((rec.seq, perp_pending));
    }
    Ok(())
}
