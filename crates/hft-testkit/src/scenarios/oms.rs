//! OMS DST cases on the real engine (crates/hft-oms/TESTS.md OMS-400–OMS-405) in the pair-cycle
//! world (`super::pair`): OnFill links open and close every two seconds under each case's faults;
//! every invariant runs at every step, the case asserts its own outcome and that its faults
//! actually met the links.

use hft_event::{NoteSource, OrderEventKind};
use hft_types::InstId;
use zerocopy::FromBytes;

use super::pair::{
    P1_LOT, ROLE_FIRST, ROLE_SECOND, ROLE_SECOND_RETRY, SECOND, accepted, events, lots_10_40, notes,
    pair_cycles, require, require_pairs, require_truth,
};
use crate::dst::faults::{ConnKind, Fault, FaultCategory};
use crate::dst::world::{ACCT_A, P1, S1, VP, VS};
use crate::dst::{DstConfig, InvariantSet, RunRecord, Scenario, SeedSet};

/// OMS Note codes (hft-oms `notes`).
fn oms_code(c: hft_oms::notes::OmsNoteCode) -> u32 {
    hft_oms::notes::code_of(c) & 0xFFFF
}

// ------------------------------------------------------------------------------- OMS-400

fn oms_400_cfg(c: &mut DstConfig) {
    pair_cycles(c, 14, lots_10_40);
    // The suite's report, reject, connection and two-leg faults at their core rates …
    for (cat, pm) in [
        (FaultCategory::Conn, 30),
        (FaultCategory::Report, 100),
        (FaultCategory::Reject, 60),
    ] {
        c.faults.set(cat, true, pm);
    }
    // … plus the case's explicit ones: late fees, late fills after recycle, second-leg rejects,
    // partial fills, a duplicate and a reorder window, a rate limit and a short disconnect.
    let mut at = SECOND / 2;
    for k in 0..12u64 {
        let t = at + k * 2 * SECOND;
        c.scripted.push((
            t,
            Fault::LateFeeOnly {
                venue: VS,
                delay_ms: 500 + 400 * (k % 5) as u32,
            },
        ));
        if k % 3 == 0 {
            c.scripted.push((t, Fault::SecondLegReject { venue: VP }));
        }
        if k % 4 == 1 {
            c.scripted
                .push((t, Fault::LateFillAfterRecycle { venue: VS, count: 1 }));
        }
    }
    at = 3 * SECOND;
    c.scripted.extend([
        (
            at,
            Fault::PartialFills {
                venue: VS,
                dur_ms: 4_000,
            },
        ),
        (
            at + 4 * SECOND,
            Fault::ReportDup {
                venue: VP,
                dur_ms: 3_000,
            },
        ),
        (
            at + 8 * SECOND,
            Fault::ReportReorder {
                venue: VS,
                window: 3,
                dur_ms: 3_000,
            },
        ),
        (
            at + 12 * SECOND,
            Fault::RateLimit {
                venue: VP,
                dur_ms: 500,
            },
        ),
        (
            at + 16 * SECOND,
            Fault::ConnDown {
                venue: VP,
                conn: ConnKind::UserStream,
                down_ms: 1_500,
                resync: false,
            },
        ),
    ]);
    c.scripted.sort_by_key(|x| x.0);
}

fn oms_400_assert(r: &RunRecord) -> Result<(), String> {
    require_pairs(r, 1)?;
    // The second legs the venue refused were retried (the reject reached its slot).
    let seconds: Vec<_> = accepted(r, ACCT_A, P1, ROLE_SECOND)
        .into_iter()
        .chain(accepted(r, ACCT_A, P1, ROLE_SECOND_RETRY))
        .collect();
    let rejected = events(r).into_iter().any(|(_, _, e)| {
        e.what == OrderEventKind::Reject
            && seconds
                .iter()
                .any(|(_, _, d)| d.venue == e.venue && d.slot == e.slot && d.r#gen == e.r#gen)
    });
    require(rejected, "no second leg was refused by the venue")?;
    // Late fees arrived as FeeOnly reports.
    require(
        events(r)
            .iter()
            .any(|(_, _, e)| e.what == OrderEventKind::FeeOnly),
        "no late FeeOnly",
    )?;
    // Each first-leg fill hedged at most once: requested hedge ≤ net × ratio + one lot per link.
    for h in &r.obs.hedge_history {
        if h.account != ACCT_A {
            continue;
        }
        let cover = h.first_net.abs() * i128::from(h.ratio_num) / i128::from(h.ratio_den.max(1));
        let bound = cover + i128::from(h.links.max(1)) * i128::from(P1_LOT);
        require(
            h.hedge_req - h.capped <= bound,
            &format!("hedged more than once: {h:?}"),
        )?;
    }
    // Occupied slots = orders still in flight at the exchange (IOC only: none once all reports
    // are delivered); the tables account for every slot.
    for t in &r.obs.tables {
        require(t.free_list_consistent, &format!("slot table inconsistent: {t:?}"))?;
    }
    if r.truth.iter().all(|t| !t.undelivered) {
        require(
            r.obs.slots.iter().all(|s| s.state.is_terminal()),
            "an order is still in flight after every report was delivered",
        )?;
    }
    require_truth(r)
}

/// OMS-400: under drops, duplicates, reorders, disconnects, rate limits, partial fills, late fees,
/// late fills after recycle and second-leg rejects the books equal the exchange, no slot leaks and
/// no first-leg fill is hedged twice.
pub fn oms_400_faults_reconcile_zero() -> Scenario {
    Scenario::new(
        "OMS-400",
        "faults: books equal the exchange, no leak, no double hedge",
    )
    .suite("core")
    .seeds(SeedSet::Standard)
    .steps(4_000)
    .invariants(InvariantSet::all())
    .configure(oms_400_cfg)
    .assert(oms_400_assert)
}

// ------------------------------------------------------------------------------- OMS-401

/// The input record at `seq` and whether it is a first-leg fill report.
fn is_first_fill(r: &RunRecord, seq: u64, firsts: &[(u64, u64, hft_event::OrderDecision)]) -> bool {
    r.journal
        .iter()
        .find(|j| j.seq == seq)
        .and_then(|j| j.try_read::<hft_event::OrderEvent>())
        .is_some_and(|e| {
            (matches!(e.what, OrderEventKind::PartialFill | OrderEventKind::Fill)
                || (e.what == OrderEventKind::QueryResult && e.cum_filled.0 > 0))
                && firsts
                    .iter()
                    .any(|(_, _, d)| d.venue == e.venue && d.slot == e.slot && d.r#gen == e.r#gen)
        })
}

fn oms_401_cfg(c: &mut DstConfig) {
    pair_cycles(c, 12, lots_10_40);
    // The hedge venue's connection answers a ping every 500 ms (case 前置).
    for v in c.venues.iter_mut().filter(|v| v.venue == VP) {
        v.bbo_ping_ms = 500;
    }
    if c.seed.is_multiple_of(2) {
        // Branch 1: P1's book never changes, its connection stays healthy.
        c.scripted.push((
            0,
            Fault::QuietBook {
                inst: P1,
                dur_ms: 3_600_000,
            },
        ));
    } else {
        // Branch 2: flat recorded quotes, but P1's last frame before the silence is at 50 ms and
        // its connection is silent (no pongs) until 2.05 s: the quote is stale from 1.05 s. The
        // spot venue's reports are slowed around the 1 s opening, so the first leg fills after
        // the quote went stale and the second leg is gated.
        super::exec::flat_market(c, 40, |inst, ns| {
            inst != P1 || !(50_000_001..2_050_000_000).contains(&ns)
        });
        c.scripted.push((
            60_000_000,
            Fault::SecondLegSilent {
                venue: VP,
                dur_ms: 1_990,
                resync: false,
            },
        ));
        c.scripted.push((
            950_000_000,
            Fault::GrayConn {
                venue: VS,
                dur_ms: 1_000,
                factor: 100,
            },
        ));
    }
}

fn oms_401_assert(r: &RunRecord) -> Result<(), String> {
    let firsts = accepted(r, ACCT_A, S1, ROLE_FIRST);
    let seconds: Vec<_> = super::pair::decisions(r)
        .into_iter()
        .filter(|(_, _, d)| d.account == ACCT_A && d.inst == P1 && d.link_role == ROLE_SECOND)
        .collect();
    let stale = notes(r, NoteSource::Oms, oms_code(hft_oms::notes::HEDGE_PX_STALE));
    let broken = notes(r, NoteSource::Oms, oms_code(hft_oms::notes::HEDGE_BROKEN));
    if r.seed.is_multiple_of(2) {
        require_pairs(r, 2)?;
        require(
            !r.deliveries.iter().any(|(t, i)| {
                *t > SECOND / 2
                    && i.kind == hft_event::EventKind::BBO
                    && hft_event::Bbo::read_from_prefix(&i.body).is_ok_and(|(b, _)| b.h.inst == P1)
            }),
            "P1's book was not quiet",
        )?;
        // Every second leg in the Input of its first leg's fill; never gated on the quiet book.
        for (_, _, d) in &seconds {
            require(
                is_first_fill(r, d.trigger_seq, &firsts),
                &format!("second leg not in its fill's Input: {d:?}"),
            )?;
        }
        require(stale.is_empty(), "OmsHedgePxStale on a quiet but healthy book")?;
        require(broken.is_empty(), "a link broke on a quiet book")?;
    } else {
        // Gated (StaleQuote), not counted: never RetryExhausted; once the quote is back the
        // retry goes out, or the link breaks only by its deadline.
        require(
            !stale.is_empty(),
            "the late fill was not gated on the silent hedge venue",
        )?;
        require(
            broken
                .iter()
                .all(|(_, n)| n.v[1] == hft_oms::LinkBroken::Deadline as u64),
            "a gated link broke for another reason than its deadline",
        )?;
        let retry_after = accepted(r, ACCT_A, P1, ROLE_SECOND_RETRY)
            .into_iter()
            .any(|(_, t, _)| t >= 2_050_000_000);
        require(
            retry_after || !broken.is_empty(),
            "neither a retry after the quote came back nor a deadline Broken",
        )?;
    }
    require_truth(r)
}

/// OMS-401: a second-leg book unchanged for the whole run with a healthy connection still hedges
/// in the fill's Input; a silent connection only gates (HedgeRetry), never counts a retry.
pub fn oms_401_quiet_book_hedges() -> Scenario {
    Scenario::new(
        "OMS-401",
        "quiet second-leg book still hedges; silence only gates",
    )
    .suite("core")
    .seeds(SeedSet::Standard)
    .steps(3_000)
    .invariants(InvariantSet::all())
    .configure(oms_401_cfg)
    .assert(oms_401_assert)
}

// ------------------------------------------------------------------------------- OMS-404

fn oms_404_cfg(c: &mut DstConfig) {
    pair_cycles(c, 14, lots_10_40);
    // Spot fees in the base asset (the fee changes the net that is hedged).
    for v in c.world.venues.iter_mut().filter(|v| v.id == VS) {
        v.fee_in_base = true;
    }
    // Half the spot fills report their fee late (0.1–3 s), a few only after their slot was
    // recycled (quarantine 5 s + sweep).
    for k in 0..40u64 {
        let d = crate::seed::derive(c.seed, &format!("oms-404/{k}"));
        let delay = if k % 20 == 7 {
            6_500
        } else {
            100 + (d % 2_901) as u32
        };
        c.scripted.push((
            k * 700_000_000,
            Fault::LateFeeOnly {
                venue: VS,
                delay_ms: delay,
            },
        ));
    }
    c.scripted.sort_by_key(|x| x.0);
}

fn truth_sum(
    r: &RunRecord,
    venue: hft_types::VenueId,
    inv: Option<hft_types::AssetId>,
    inst: InstId,
) -> i128 {
    r.truth
        .iter()
        .filter(|t| t.account == ACCT_A && t.venue == venue)
        .map(|t| match inv {
            Some(a) => t
                .inventory
                .iter()
                .filter(|(x, _)| *x == a)
                .map(|(_, q)| *q)
                .sum::<i128>(),
            None => t
                .positions
                .iter()
                .filter(|(x, _)| *x == inst)
                .map(|(_, q)| *q)
                .sum::<i128>(),
        })
        .sum()
}

fn oms_404_assert(r: &RunRecord) -> Result<(), String> {
    require_pairs(r, 2)?;
    let fee_only = events(r)
        .into_iter()
        .filter(|(_, _, e)| e.what == OrderEventKind::FeeOnly && e.venue == VS)
        .count();
    require(fee_only > 0, "no late FeeOnly reached the engine")?;
    require(
        notes(r, NoteSource::Oms, oms_code(hft_oms::notes::FEE_UNATTRIBUTED)).is_empty(),
        "a late fee was not attributed (slot or tombstone)",
    )?;
    if r.truth.iter().all(|t| !t.undelivered) {
        // Each leg's net equals the exchange's inventory / position change.
        let net = |leg: u8| {
            r.obs
                .leg_net
                .iter()
                .filter(|l| l.account == ACCT_A && l.leg == leg)
                .map(|l| l.net)
                .sum::<i128>()
        };
        let inv = truth_sum(r, VS, Some(crate::dst::world::BTC), S1);
        let perp = truth_sum(r, VP, None, P1);
        require(net(0) == inv, &format!("S1 net {} != exchange {inv}", net(0)))?;
        require(net(1) == perp, &format!("P1 net {} != exchange {perp}", net(1)))?;
    }
    // The group remainder stays within one lot plus the unabsorbed fee.
    for b in r.obs.hedge_balances.iter().filter(|b| b.account == ACCT_A) {
        require(
            b.residual.abs() < i128::from(P1_LOT) + b.fee_adjust.abs() + 1,
            &format!("remainder not absorbed: {b:?}"),
        )?;
    }
    require_truth(r)
}

/// OMS-404: late base-asset fees correct the net (ledger and LegTracker) and are absorbed by the
/// group remainder; fees after recycling are attributed through the tombstone.
pub fn oms_404_late_fees() -> Scenario {
    Scenario::new("OMS-404", "late fees: net corrected, remainder absorbed")
        .suite("core")
        .seeds(SeedSet::Standard)
        .steps(4_000)
        .invariants(InvariantSet::all())
        .configure(oms_404_cfg)
        .assert(oms_404_assert)
}

// ------------------------------------------------------------------------------- OMS-405

/// Recorded flat W1 quotes every 50 ms, except that `P1` drops by a seed-derived 0.05 %–1 % in
/// `[open + 20 ms, open + 1.3 s)` of every cycle (the sell second leg hits the lower bid), while
/// the first leg's reports are slowed.
fn oms_405_cfg(c: &mut DstConfig) {
    pair_cycles(c, 12, lots_10_40);
    let p1 = c.world.inst(P1).cloned().expect("W1 P1");
    let s1 = c.world.inst(S1).cloned().expect("W1 S1");
    let mut inputs = Vec::new();
    for tick in 1..=(32 * 20u64) {
        let ns = tick * 50_000_000;
        // Offset from the cycle's opening command (at 1 + 2k s).
        let since_open = ns.saturating_sub(SECOND) % (2 * SECOND);
        let k = ns.saturating_sub(SECOND) / (2 * SECOND);
        // Even cycles beyond `max_slip` (0.55–1 %), odd ones within it (0.05–0.45 %).
        let d = crate::seed::derive(c.seed, &format!("oms-405/{k}"));
        let jump_bp = if k.is_multiple_of(2) {
            55 + (d % 46) as i64
        } else {
            5 + (d % 41) as i64
        };
        let in_jump = ns >= SECOND && (20_000_000..1_300_000_000).contains(&since_open);
        for (spec, is_p1) in [(&s1, false), (&p1, true)] {
            let (mut bid, mut ask) = (spec.bid, spec.ask);
            if is_p1 && in_jump {
                let d = bid * jump_bp / 10_000 / spec.tick * spec.tick;
                bid -= d;
                ask -= d;
            }
            let mut h = <hft_event::MarketHdr as zerocopy::FromZeros>::new_zeroed();
            h.inst = spec.id;
            h.venue = spec.venue;
            h.ex_ts_ns = ns as i64;
            let b = hft_event::Bbo {
                h,
                bid_px: hft_types::Px(bid),
                bid_qty: hft_types::Qty(spec.depth),
                ask_px: hft_types::Px(ask),
                ask_qty: hft_types::Qty(spec.depth),
                update_id: tick,
            };
            inputs.push((ns, crate::dst::InputRec::of(&b, crate::dst::Source::Market)));
        }
    }
    c.market = crate::dst::MarketSource::Recorded(inputs);
    // The spot venue's reports are slowed ×100 around each opening, so the first leg fills after
    // the second leg's price moved.
    for k in 0..12u64 {
        let open = (1 + 2 * k) * SECOND;
        c.scripted.push((
            open - 50_000_000,
            Fault::GrayConn {
                venue: VS,
                dur_ms: 600,
                factor: 100,
            },
        ));
    }
}

fn oms_405_assert(r: &RunRecord) -> Result<(), String> {
    require_pairs(r, 2)?;
    let capped = notes(r, NoteSource::Oms, oms_code(hft_oms::notes::HEDGE_SLIP_CAPPED));
    let gated = notes(r, NoteSource::Oms, oms_code(hft_oms::notes::HEDGE_PX_GATED));
    require(
        !capped.is_empty() || !gated.is_empty(),
        "no jump beyond max_slip met a second leg",
    )?;
    // Never refused by the price band (#5), never broken for a price.
    let hedges: Vec<_> = super::pair::decisions(r)
        .into_iter()
        .filter(|(_, _, d)| d.inst == P1 && (d.link_role == ROLE_SECOND || d.link_role == ROLE_SECOND_RETRY))
        .collect();
    require(!hedges.is_empty(), "no second leg")?;
    for (_, _, d) in &hedges {
        require(
            d.reject & hft_types::RejectBits::PX_BAND.0 == 0,
            &format!("second leg refused by the price band: {d:?}"),
        )?;
    }
    require(
        notes(r, NoteSource::Oms, oms_code(hft_oms::notes::HEDGE_BROKEN)).is_empty(),
        "a price jump broke a link",
    )?;
    require_truth(r)
}

/// OMS-405: a second-leg price jump within `rr_max_cross` is priced at the cross cap in the same
/// Input (no retry counted); beyond it the leg is only gated and goes out when the price is back.
pub fn oms_405_slip_jump() -> Scenario {
    Scenario::new("OMS-405", "slip jump priced at the cap, beyond it gated")
        .suite("core")
        .seeds(SeedSet::Standard)
        .steps(3_000)
        .invariants(InvariantSet::all())
        .configure(oms_405_cfg)
        .assert(oms_405_assert)
}

// ------------------------------------------------------------------------------- OMS-402

fn oms_402_cfg(c: &mut DstConfig) {
    pair_cycles(c, 12, lots_10_40);
    // VS orders and VP second legs reach the matcher 3–8 s late; until then queries answer
    // NotFound (no expiry guard can prove they will not execute).
    for (k, (venue, at)) in [(VS, 950_000_000u64), (VP, 4_950_000_000), (VS, 8_950_000_000)]
        .into_iter()
        .enumerate()
    {
        let d = crate::seed::derive(c.seed, &format!("oms-402/{k}"));
        c.scripted.push((
            at,
            Fault::NotFoundThenFill {
                venue,
                delay_ms: 3_000 + (d % 5_001) as u32,
            },
        ));
    }
}

fn oms_402_assert(r: &RunRecord) -> Result<(), String> {
    let evs = events(r);
    let decs = super::pair::decisions(r);
    // Slots answered NotFound that later filled.
    let mut hidden = Vec::new();
    for (seq, _, e) in &evs {
        if e.what != OrderEventKind::QueryResult || e.detail != 7 {
            continue;
        }
        let key = (e.venue, e.slot, e.r#gen);
        if hidden.iter().any(|(k, _, _)| *k == key) {
            continue;
        }
        let fill = evs.iter().find(|(s2, _, x)| {
            *s2 > *seq
                && (x.venue, x.slot, x.r#gen) == key
                && x.cum_filled.0 > 0
                && matches!(
                    x.what,
                    OrderEventKind::PartialFill | OrderEventKind::Fill | OrderEventKind::QueryResult
                )
        });
        if let Some((fs, _, _)) = fill {
            let placed = decs
                .iter()
                .find(|(_, _, d)| {
                    d.action == hft_event::OrderAction::New && (d.venue, d.slot, d.r#gen) == key
                })
                .map_or(0, |(s, _, _)| *s);
            hidden.push((key, placed, *fs));
        }
    }
    require(
        !hidden.is_empty(),
        "no order was answered NotFound and filled later",
    )?;
    let transition = oms_code(hft_oms::notes::TRANSITION);
    for ((venue, slot, g), placed, filled) in &hidden {
        // Never terminal (Cancelled 6 / Rejected 7) before its fill.
        let early_end = r.journal.iter().any(|j| {
            j.seq > *placed
                && j.seq < *filled
                && j.try_read::<hft_event::Note>().is_some_and(|n| {
                    n.source == NoteSource::Oms
                        && n.code & 0xFFFF == transition
                        && n.v[0] & 0xFF == u64::from(venue.0)
                        && n.v[1] == u64::from(slot.0)
                        && n.v[2] == u64::from(*g)
                        && matches!((n.v[0] >> 24) & 0xFF, 6 | 7)
                })
        });
        require(
            !early_end,
            &format!("slot {slot:?} on {venue:?} ended before its fill"),
        )?;
        // A hidden second leg is not retried meanwhile.
        if *venue == VP {
            let retried = decs.iter().any(|(s, _, d)| {
                *s > *placed && *s < *filled && d.inst == P1 && d.link_role == ROLE_SECOND_RETRY
            });
            require(!retried, "SecondRetry while the second leg was unconfirmed")?;
        }
    }
    require(
        notes(r, NoteSource::Oms, oms_code(hft_oms::notes::HEDGE_BROKEN))
            .iter()
            .all(|(_, n)| n.v[1] == hft_oms::LinkBroken::Deadline as u64),
        "a link broke for another reason than its deadline",
    )?;
    require_truth(r)
}

/// OMS-402: an accepted order the venue answers NotFound for a while, then fills: never
/// Rejected/Cancelled before the fill, never re-sent or retried; late hedge confirmation only
/// breaks a link by its deadline.
pub fn oms_402_notfound_then_fill() -> Scenario {
    Scenario::new("OMS-402", "NotFound then fill: never terminal early, no re-send")
        .suite("core")
        .seeds(SeedSet::Standard)
        .steps(4_000)
        .invariants(InvariantSet::all())
        .configure(oms_402_cfg)
        .assert(oms_402_assert)
}
