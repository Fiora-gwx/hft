//! Directed transport-race fixtures shared by the real-engine tests and file replayer.
use hft_event::{CmdHdr, CmdScope, ControlCmd, OrderAction, OrderDecision, OrderEvent};
use hft_types::{InstId, StrategyId, VenueId};
use zerocopy::IntoBytes;

use crate::dst::actors::encode_cmd;
use crate::dst::world::{ACCT_A, UTC0, VP, VS};
use crate::dst::{DstConfig, Fault, RerunPolicy, RunRecord};

/// Open and close real paired orders, cancel each still-unreported completed leg, then drain
/// all original fees. Recovery takes place after the initial race; the exchange keeps truth.
pub fn config(seed: u64, restart: bool) -> DstConfig {
    let mut c = super::foreign_ledger::led_080_foreign_order_cold_hot_equal()
        .config(&super::suites(), seed)
        .expect("core suite");
    c.rerun = RerunPolicy::Never;
    c.horizon_ns = DstConfig::default().horizon_ns;
    c.steps = 10_000;
    c.scripted.clear();
    for venue in [VS, VP] {
        c.scripted.push((
            0,
            Fault::CancelRejectFilled {
                venue,
                count: 1,
                hold_ms: 5_000,
            },
        ));
        c.scripted.push((
            0,
            Fault::LateFeeOnly {
                venue,
                delay_ms: 6_000,
            },
        ));
    }
    for (id, at) in [(51, 1_200_000_000), (52, 1_400_000_000)] {
        c.confirmed_commands.push((
            at,
            encode_cmd(
                &ControlCmd::CancelAll {
                    scope: CmdScope {
                        strategy: StrategyId(0),
                        venue: VenueId(0),
                        account: ACCT_A,
                        _pad: 0,
                        group: hft_types::GROUP_ALL,
                        inst: InstId(0),
                        _pad2: 0,
                    },
                },
                "cancel-filled",
                CmdHdr::ROLE_ADMIN,
                ACCT_A,
                UTC0 as u64 + at,
                id,
            ),
        ));
    }
    c.confirmed_commands.sort_by_key(|(at, _)| *at);
    if restart {
        c.scripted.push((
            0,
            Fault::Crash(crate::dst::CrashFault {
                at_step: 500,
                repeat: 2,
                point: crate::dst::CrashPoint::SeqInput,
            }),
        ));
    }
    c
}

/// Validate actual command and journal evidence. Counts neither requests nor held receipts as
/// completed races, and checks original cash/fee fields independently of wallet differences.
pub fn validate(r: &RunRecord) -> Result<usize, String> {
    let mut count = 0;
    for v in &r.cancel_filled {
        if v.requested != v.cases.len() as u64 || v.awaiting_fill != 0 || v.awaiting_release != 0 {
            return Err(format!(
                "cancel request conservation at venue {}: requested {}, captured {}, awaiting fill {}, awaiting release {}",
                v.venue.0,
                v.requested,
                v.cases.len(),
                v.awaiting_fill,
                v.awaiting_release
            ));
        }
        for x in &v.cases {
            let (Some(cmd), Some(rej), Some(fill)) = (x.command, x.rejection, x.report) else {
                return Err("incomplete cancel race evidence".into());
            };
            let full_id = |e: OrderEvent| {
                (
                    e.venue,
                    e.account,
                    e.inst,
                    e.epoch,
                    e.slot,
                    e.r#gen,
                    e.exch_order_id,
                )
            };
            let mut expected = x.original;
            if fill.fee.0 == 0 {
                expected.fee = fill.fee;
                expected.fee_asset = fill.fee_asset;
            }
            if expected.as_bytes() != fill.as_bytes()
                || x.original.qty.0 <= 0
                || fill.venue != v.venue
                || fill.what != hft_event::OrderEventKind::Fill
                || full_id(rej) != full_id(fill)
                || rej.detail != 1
                || rej.what != hft_event::OrderEventKind::CancelReject
                || rej.qty.0 != 0
                || rej.fee.0 != 0
                || rej.cum_filled != fill.cum_filled
                || rej.avg_px != fill.avg_px
                || rej.cash_qty != fill.cash_qty
                || rej.cash_sum != fill.cash_sum
                || cmd.op != hft_event::GwOp::Cancel
                || (cmd.account, cmd.inst, cmd.target_slot, cmd.r#gen)
                    != (fill.account, fill.inst, fill.slot, fill.r#gen)
            {
                return Err("cancel original identity or settlement mismatch".into());
            }
            if fill.fee != x.original.fee {
                let fees = r
                    .journal
                    .iter()
                    .filter_map(|j| j.try_read::<OrderEvent>())
                    .filter(|e| {
                        e.what == hft_event::OrderEventKind::FeeOnly
                            && full_id(*e) == full_id(fill)
                            && e.fee == x.original.fee
                            && e.fee_asset == x.original.fee_asset
                            && e.qty.0 == 0
                    })
                    .count();
                if fees != 1 {
                    return Err("missing or duplicated original fee".into());
                }
            }
            let decision = r
                .journal
                .iter()
                .find(|j| j.seq == cmd.decision_seq)
                .and_then(|j| j.try_read::<OrderDecision>())
                .ok_or("missing actual cancel decision")?;
            if decision.action != OrderAction::Cancel
                || decision.reject != 0
                || !r.frames.iter().any(|(_, f)| f.cmd.as_bytes() == cmd.as_bytes())
            {
                return Err("missing real cancel frame".into());
            }
            let find = |e: OrderEvent| {
                r.journal
                    .iter()
                    .find(|j| j.kind == hft_event::EventKind::ORDER_EVENT && j.body == e.as_bytes())
            };
            let rejection = find(rej).ok_or("missing actual cancel rejection")?;
            let original = find(fill).ok_or("missing actual fill delivery")?;
            if !(cmd.decision_seq < rejection.seq && rejection.seq < original.seq)
                || !r.ledger_checks.contains(&rejection.seq)
                || !r.ledger_checks.contains(&original.seq)
            {
                return Err("cancel delivery order or ledger observation missing".into());
            }
            count += 1;
        }
    }
    if count == 0 {
        return Err("no actual cancel-filled races".into());
    }
    Ok(count)
}
