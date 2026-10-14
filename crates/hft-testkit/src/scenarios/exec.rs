//! Executor DST cases on the real engine (crates/hft-exec/TESTS.md EXE-500, EXE-505) in the
//! pair-cycle world (`super::pair`).

use hft_event::OrderEventKind;

use super::pair::{
    P1_LOT, ROLE_FIRST, ROLE_SECOND, SECOND, accepted, decisions, events, exec_decisions, lots_10_40,
    pair_cycles, require, require_pairs, require_truth,
};
use crate::dst::faults::Fault;
use crate::dst::world::{ACCT_A, G1, P1, S1, VP, VS};
use crate::dst::{DstConfig, InputRec, InvariantSet, MarketSource, RunRecord, Scenario, SeedSet, Source};

/// `ExecDecision.action` / `state_to` codes (hft-exec `decision`).
const ACT_TARGET_SET: u8 = 1;
const ACT_TARGET_REPLACED: u8 = 2;
const ACT_STATE: u8 = 6;
const ACT_AUTO_FIX: u8 = 14;
const STATE_IDLE: u8 = 1;
const STATE_BROKEN: u8 = 4;

/// Recorded W1 quotes every 50 ms with a seed-derived random walk of ±1 tick and the top-level
/// depth of both insts changing to 0.0005–0.05 BTC every 50–500 ms.
fn walking_depth_market(c: &mut DstConfig, seconds: u64) {
    let s1 = c.world.inst(S1).cloned().expect("W1 S1");
    let p1 = c.world.inst(P1).cloned().expect("W1 P1");
    let mut inputs = Vec::new();
    let mut mid = [0i64; 2];
    let mut depth = [s1.depth, p1.depth];
    let mut next_depth = [0u64; 2];
    for tick in 1..=(seconds * 20) {
        let ns = tick * 50_000_000;
        for (ix, spec) in [&s1, &p1].into_iter().enumerate() {
            let d = crate::seed::derive(c.seed, &format!("exe-500/{ix}/{tick}"));
            mid[ix] += (d % 3) as i64 - 1;
            if ns >= next_depth[ix] {
                depth[ix] = 50_000 + (d >> 8) as i64 % 4_950_001;
                next_depth[ix] = ns + 50_000_000 + (d >> 40) % 450_000_000;
            }
            let shift = mid[ix] * spec.tick;
            let mut h = <hft_event::MarketHdr as zerocopy::FromZeros>::new_zeroed();
            h.inst = spec.id;
            h.venue = spec.venue;
            h.ex_ts_ns = ns as i64;
            let b = hft_event::Bbo {
                h,
                bid_px: hft_types::Px(spec.bid + shift),
                bid_qty: hft_types::Qty(depth[ix]),
                ask_px: hft_types::Px(spec.ask + shift),
                ask_qty: hft_types::Qty(depth[ix]),
                update_id: tick,
            };
            inputs.push((ns, InputRec::of(&b, Source::Market)));
        }
    }
    c.market = MarketSource::Recorded(inputs);
}

/// Recorded flat W1 quotes every 50 ms for `seconds`; `keep(inst, ns)` = false drops that frame.
pub(super) fn flat_market(c: &mut DstConfig, seconds: u64, keep: fn(hft_types::InstId, u64) -> bool) {
    let specs: Vec<_> = c.world.insts.clone();
    let mut inputs = Vec::new();
    for tick in 1..=(seconds * 20) {
        let ns = tick * 50_000_000;
        for spec in &specs {
            if !keep(spec.id, ns) {
                continue;
            }
            let mut h = <hft_event::MarketHdr as zerocopy::FromZeros>::new_zeroed();
            h.inst = spec.id;
            h.venue = spec.venue;
            h.ex_ts_ns = ns as i64;
            let b = hft_event::Bbo {
                h,
                bid_px: hft_types::Px(spec.bid),
                bid_qty: hft_types::Qty(spec.depth),
                ask_px: hft_types::Px(spec.ask),
                ask_qty: hft_types::Qty(spec.depth),
                update_id: tick,
            };
            inputs.push((ns, InputRec::of(&b, Source::Market)));
        }
    }
    c.market = MarketSource::Recorded(inputs);
}

// ------------------------------------------------------------------------------- EXE-500

fn exe_500_cfg(c: &mut DstConfig) {
    pair_cycles(c, 12, lots_10_40);
    walking_depth_market(c, 32);
    for k in 0..12u64 {
        let open = (1 + 2 * k) * SECOND;
        c.scripted.push((
            open - 100_000_000,
            Fault::PartialFills {
                venue: VS,
                dur_ms: 900,
            },
        ));
        c.scripted.push((
            open,
            Fault::ReportReorder {
                venue: VP,
                window: 3,
                dur_ms: 800,
            },
        ));
        if k % 2 == 0 {
            c.scripted.push((
                open,
                Fault::ReportDup {
                    venue: VP,
                    dur_ms: 800,
                },
            ));
        }
    }
    c.scripted.sort_by_key(|x| x.0);
}

/// Every second leg (`Second`) goes out in the Input of a first-leg fill; first-leg orders are
/// whole pair lots; each target reaches Done at most once.
fn exe_500_assert(r: &RunRecord) -> Result<(), String> {
    require_pairs(r, 2)?;
    let firsts = accepted(r, ACCT_A, S1, ROLE_FIRST);
    for (_, _, d) in &firsts {
        require(
            d.qty.0 % P1_LOT == 0,
            &format!("first-leg order not a whole pair lot: {d:?}"),
        )?;
    }
    let fill_inputs: Vec<u64> = events(r)
        .into_iter()
        .filter(|(_, _, e)| {
            (matches!(e.what, OrderEventKind::PartialFill | OrderEventKind::Fill)
                || (e.what == OrderEventKind::QueryResult && e.cum_filled.0 > 0))
                && firsts
                    .iter()
                    .any(|(_, _, d)| d.venue == e.venue && d.slot == e.slot && d.r#gen == e.r#gen)
        })
        .map(|(seq, _, _)| seq)
        .collect();
    let seconds: Vec<_> = decisions(r)
        .into_iter()
        .filter(|(_, _, d)| d.account == ACCT_A && d.inst == P1 && d.link_role == ROLE_SECOND)
        .collect();
    require(!seconds.is_empty(), "no second leg")?;
    for (_, _, d) in &seconds {
        require(
            fill_inputs.contains(&d.trigger_seq),
            &format!("second leg outside its first-leg fill's Input: {d:?}"),
        )?;
    }
    done_once(r)?;
    common_checker(r)
}

/// A target reaches Done (State → Idle) at most once: never twice without a new target between.
fn done_once(r: &RunRecord) -> Result<(), String> {
    let mut open = false;
    let mut done = 0;
    for (_, d) in exec_decisions(r)
        .into_iter()
        .filter(|(_, d)| d.account == ACCT_A && d.group == G1)
    {
        match d.action {
            ACT_TARGET_SET | ACT_TARGET_REPLACED => open = true,
            ACT_STATE if d.state_to == STATE_IDLE && d.state_from != STATE_BROKEN => {
                require(open, "a target reached Done twice")?;
                open = false;
                done += 1;
            }
            _ => {}
        }
    }
    require(done > 0, "no target reached Done")
}

/// EXE-505's checker, applied to every executor DST case: books equal the exchange, no Broken cell
/// is left without its bounded auto-fix ending (or still within its window at the run's end).
fn common_checker(r: &RunRecord) -> Result<(), String> {
    // broken_auto_fix_ms (W1: at most 60 s) + fix_deadline_ms (10 s default) + 1 s of slack.
    let window = 71 * SECOND;
    let xs = exec_decisions(r);
    let tsc_of = |seq: u64| r.journal.iter().find(|j| j.seq == seq).map_or(0, |j| j.tsc);
    for (seq, d) in xs
        .iter()
        .filter(|(_, d)| d.action == ACT_STATE && d.state_to == STATE_BROKEN)
    {
        let t = tsc_of(*seq);
        if r.end_ns < t + window {
            continue;
        }
        let ended = xs.iter().any(|(s2, x)| {
            *s2 > *seq
                && x.group == d.group
                && ((x.action == ACT_AUTO_FIX
                    && matches!(
                        hft_exec::unpack(x.detail).1,
                        hft_exec::decision::autofix::DONE | hft_exec::decision::autofix::FAILED
                    ))
                    || (x.action == ACT_STATE && x.state_from == STATE_BROKEN))
        });
        require(
            ended,
            &format!("Broken at {t} not handled within the auto-fix window"),
        )?;
    }
    require_truth(r)
}

/// EXE-500: pair opening under random partial fills and depth changes, second-leg reports
/// reordered and duplicated: second legs in their fill's Input, whole pair lots, Done once.
pub fn exe_500_pair_open_partial_fills() -> Scenario {
    Scenario::new(
        "EXE-500",
        "pair open: partial fills, depth changes, reordered second legs",
    )
    .suite("core")
    .seeds(SeedSet::Standard)
    .steps(4_000)
    .invariants(InvariantSet::all())
    .configure(exe_500_cfg)
    .assert(exe_500_assert)
}

// ------------------------------------------------------------------------------- EXE-505

fn exe_505_cfg(c: &mut DstConfig) {
    pair_cycles(c, 14, lots_10_40);
    // Short Broken windows so the bounded auto fix runs within the run.
    c.world.risk.broken_auto_fix_ms = 2_000;
    let d = crate::seed::derive(c.seed, "exe-505");
    c.scripted.extend([
        (
            900_000_000,
            Fault::PartialFills {
                venue: VS,
                dur_ms: 1_500,
            },
        ),
        (
            2_900_000_000,
            Fault::LateFeeOnly {
                venue: VS,
                delay_ms: 800,
            },
        ),
        (
            2_900_000_000,
            Fault::ReportDup {
                venue: VP,
                dur_ms: 1_000,
            },
        ),
        (
            4_950_000_000,
            Fault::PersistentSecondReject {
                venue: VP,
                dur_ms: 3_000,
                autofix_rejected: d.is_multiple_of(2),
                hedge_venue_down: false,
            },
        ),
        (
            8_900_000_000,
            Fault::RateLimit {
                venue: VP,
                dur_ms: 400,
            },
        ),
    ]);
}

fn exe_505_assert(r: &RunRecord) -> Result<(), String> {
    require_pairs(r, 2)?;
    let broken = exec_decisions(r)
        .into_iter()
        .filter(|(_, d)| d.action == ACT_STATE && d.state_to == STATE_BROKEN)
        .count();
    require(broken > 0, "the persistent second-leg rejects broke no cell")?;
    common_checker(r)
}

/// EXE-505: the executor cases' common checker (books equal the exchange, Broken handled within
/// `broken_auto_fix_ms` + `fix_deadline_ms`) in a run with a Broken cell.
pub fn exe_505_common_checker() -> Scenario {
    Scenario::new(
        "EXE-505",
        "executor common checker: naked bounded, Broken handled",
    )
    .suite("core")
    .seeds(SeedSet::Standard)
    .steps(4_000)
    .invariants(InvariantSet::all())
    .configure(exe_505_cfg)
    .assert(exe_505_assert)
}
