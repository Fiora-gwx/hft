//! Raw component observations. Expected modes, caps and quantities are computed by checkers.

use hft_event::{CapRecord, OmsTimer, TimerCode};
use hft_risk::mode::Effective;
use hft_sequencer::{Engine, StepOutcome};
use hft_strategy_sdk::{MarginLevel, TargetOrigin, TargetReason, TargetState};
use hft_types::{CapSource, GROUP_ALL, GroupId, KillBits, RunMode, Side};

use crate::dst::obs::*;
use crate::dst::world::WorldSpec;

pub fn fill(e: &Engine, w: &WorldSpec, history: &super::audit::History, o: &mut Obs) {
    o.engine_ns = Some(e.now.engine_ns);
    o.required = Some(ObservationRequirements {
        accounts: w.accounts.iter().map(|a| a.id).collect(),
        cells: w
            .groups
            .iter()
            .map(|g| (g.account, g.instance, g.group))
            .collect(),
        instruments: w
            .accounts
            .iter()
            .flat_map(|a| {
                w.insts
                    .iter()
                    .filter(|i| a.venues.contains(&i.venue))
                    .map(|i| (a.id, i.id))
            })
            .collect(),
        venues: e.setup.gws.venues.clone(),
    });
    for r in e.risk.mode.caps() {
        let s = r.scope;
        let kill = r.source == CapSource::KILL;
        let hard = kill && r.cflags & CapRecord::HARD != 0;
        let venue_level = if kill {
            s.venue.0 != 0
        } else {
            matches!(
                r.source,
                CapSource::RECON | CapSource::UNKNOWN_TIMEOUT | CapSource::ACCOUNT_CFG | CapSource::DELISTING
            )
        };
        let scope = if s.venue.0 != 0 {
            CapScope::Venue(s.venue)
        } else if s.group != GROUP_ALL || s.strategy.0 != 0 {
            CapScope::Cell(s.account, s.strategy, s.group)
        } else if s.account.0 != 0 {
            CapScope::Account(s.account)
        } else {
            CapScope::Global
        };
        o.caps.push(CapRow {
            source: r.source.0,
            scope,
            level: if hard { CapLevel::Hard } else { CapLevel::CloseOnly },
            venue_level,
            kill,
            hard,
            set_seq: r.set_seq,
        });
    }
    for (c, desired, eff) in e.risk.mode.cells() {
        let gp = e.ledger.group_pos(c.account, c.group);
        let lv = e.oms.legs(c.account, c.group).view;
        let flat = gp.legs.iter().all(|l| l.qty.0 == 0)
            && lv.leg.iter().all(|l| {
                l.net.0 == 0 && l.residual.0 == 0 && l.inflight_immediate.0 == 0 && l.inflight_resting.0 == 0
            })
            && !lv.has_pending;
        let (effective, deferred_shadow) = match eff {
            Effective::Shadow => (RunMode::Shadow, false),
            Effective::CloseOnly { deferred_shadow } => (RunMode::CloseOnly, deferred_shadow),
            Effective::Live => (RunMode::Live, false),
        };
        o.cells.push(CellModeRow {
            account: c.account,
            strategy: c.strategy,
            group: c.group,
            desired,
            effective,
            deferred_shadow,
            flat,
        });
    }
    for (ix, venue) in e.setup.gws.venues.iter().enumerate() {
        let Some(bits) = e.risk.mode.kill_bits().get(ix) else {
            o.observation_error = Some(format!("missing kill word for venue {}", venue.0));
            continue;
        };
        let mut k = KillRow::clear(*venue);
        k.global = bits.0 & KillBits::GLOBAL.0 != 0;
        k.venue_bit = bits.0 & KillBits::VENUE.0 != 0;
        k.hard = bits.0 & KillBits::HARD.0 != 0;
        for a in &w.accounts {
            if let Some(ai) = e.rd.venue_account_ix(*venue, a.id) {
                if a.id.0 >= 64 {
                    o.observation_error = Some("DST KillRow account bitmap exceeds 64 accounts".into());
                    continue;
                }
                if bits.0 & KillBits::account(ai).0 != 0 {
                    k.accounts |= 1 << a.id.0;
                }
                if bits.0 & KillBits::account_hard(ai).0 != 0 {
                    k.accounts_hard |= 1 << a.id.0;
                }
            }
        }
        o.kill.push(k);
    }
    for a in &w.accounts {
        let r = e.risk.view(a.id, &e.now);
        o.margin.push(MarginRow {
            account: a.id,
            level: match r.margin_level {
                MarginLevel::L1 => 1,
                MarginLevel::L2 => 2,
                MarginLevel::L3 => 3,
                _ => 0,
            },
            engine_flatten: r.flatten_lock,
            hard: r.hard,
        });
        for i in w.insts.iter().filter(|i| a.venues.contains(&i.venue)) {
            let f = e.oms.inflight().pos(a.id, i.id);
            for (side, imm, open) in [
                (Side::Buy, f.imm_buy, f.open_buy),
                (Side::Sell, f.imm_sell, f.open_sell),
            ] {
                o.inflight.push(InflightRow {
                    account: a.id,
                    inst: i.id,
                    side,
                    immediate: i128::from(imm.0),
                    resting: i128::from(open.0) - i128::from(imm.0),
                });
            }
        }
        // Include the ungrouped execution cell as well as configured strategy cells.
        for group in
            core::iter::once(GroupId(0)).chain(w.groups.iter().filter(|g| g.account == a.id).map(|g| g.group))
        {
            let Some(c) = e.exec.cell(a.id, group) else {
                continue;
            };
            let phase = match c.auto_fix.phase() {
                hft_exec::AutoFixPhase::None => AutoFixPhase::Idle,
                hft_exec::AutoFixPhase::Primed => AutoFixPhase::Primed,
                hft_exec::AutoFixPhase::WaitHard => AutoFixPhase::WaitHard,
                hft_exec::AutoFixPhase::WaitHold => AutoFixPhase::WaitHold,
                hft_exec::AutoFixPhase::Running => AutoFixPhase::Running,
                hft_exec::AutoFixPhase::Cancelled => AutoFixPhase::Cancelled,
                hft_exec::AutoFixPhase::Done => AutoFixPhase::Done,
                hft_exec::AutoFixPhase::Failed => AutoFixPhase::Failed,
            };
            o.exec.push(ExecCellRow {
                account: a.id,
                group,
                broken: matches!(c.view.state, TargetState::Broken(_)),
                serial_fallback: c.serial & hft_exec::SERIAL_FALLBACK != 0,
                manual_hold: c.hold & hft_exec::MANUAL_HOLD != 0,
                phase,
                due_ens: if c.auto_fix.due_tsc.0 == 0 {
                    0
                } else {
                    e.now.ens_of(c.auto_fix.due_tsc)
                },
                due_ens_recomputed: history.autofix_due.get(&(a.id.0, group.0)).copied().unwrap_or(0),
                budget: i128::from(c.auto_fix.budget.0),
                autofix_filled: i128::from(c.auto_fix.done_qty.0),
                fail_streak: u32::from(c.fail_streak),
                fix_count: u32::from(c.fix.count),
                engine_target_pending: c.has_target
                    && matches!(c.view.origin, TargetOrigin::Engine(_))
                    && matches!(
                        c.view.intent.reason,
                        TargetReason::MarginStress | TargetReason::Hard
                    )
                    && c.view.state != TargetState::Idle,
            });
        }
    }
    o.drain = DrainRow {
        done: matches!(e.finished, Some(StepOutcome::Done { .. })),
        timed_out: e.drain.timed_out,
        exec_drain_ready: e.exec.drain_ready(),
        hedge_retry_timers: e
            .heap
            .iter()
            .filter(|t| {
                matches!(
                    TimerCode::decode(t.owner, t.timer_id, GroupId(t.group)),
                    Ok(TimerCode::Oms(OmsTimer::HedgeRetry { .. }))
                )
            })
            .count() as u32,
        snapshot_open_links: if matches!(e.finished, Some(StepOutcome::Done { .. })) {
            o.links
                .iter()
                .filter(|l| matches!(l.state, LinkState::Open | LinkState::Hedging))
                .count() as u32
        } else {
            0
        },
    };
}
