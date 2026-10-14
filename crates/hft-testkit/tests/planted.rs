//! TK-053: every invariant has a planted defect the DST finds within 2 000 seeds.
//!
//! The defects are planted in the framework's self-test engine (`ToyEngine`, `toy::PLANTED`); the
//! rows of the real engine crates (`planted-<name>` features) join with the sequencer adapter.
//! Rows TK-053.21 (QF-1) and TK-053.22 (MS-2) need those scenarios and run with them.

use hft_testkit::dst::faults::Fault;
use hft_testkit::dst::faults::FaultCategory;
use hft_testkit::dst::invariants::{InvId, InvariantSet};
use hft_testkit::dst::world::{ACCT_A, VP, VS};
use hft_testkit::dst::{DstConfig, FailKind, RerunPolicy};
use zerocopy::TryFromBytes;

use crate::support::{cfg, first_failure};

fn base_fee(c: &mut DstConfig) {
    for v in &mut c.world.venues {
        if v.id == VS {
            v.fee_in_base = true;
        }
    }
}
/// INV-9 row: the in-process rerun finds it; INV-6 (which replays in the same process) would
/// report the same nondeterminism earlier, so it is off for this row.
fn rerun(c: &mut DstConfig) {
    c.rerun = RerunPolicy::Always;
    c.invariants = InvariantSet::all().without(InvId::I6);
}
/// One margin report pushing account a into L3.
fn l3(c: &mut DstConfig) {
    c.scripted.push((
        5_000_000_000,
        Fault::MarginPath {
            account: ACCT_A,
            venue: VP,
        },
    ));
}
fn fast_broken(c: &mut DstConfig) {
    c.world.risk.broken_auto_fix_ms = 2_000;
}
/// Missing client id alone is resolvable after Ack. Make the unknown report actually arrive
/// before Ack with no hold window, so the planted drop and the healthy path see the same input.
fn unknown_before_ack(c: &mut DstConfig) {
    for venue in &mut c.venues {
        venue.latency_ns = (1_000_000, 1_000_000);
        venue.unmatched_hold_ms = 0;
        c.scripted.push((0, Fault::FillBeforeAck { venue: venue.venue }));
        c.scripted.push((
            0,
            Fault::NoClientIdFill {
                venue: venue.venue,
                within_hold: false,
            },
        ));
    }
}
fn none(_: &mut DstConfig) {}

const MAX_SEEDS: u64 = 2_000;

type Row = (
    &'static str,
    &'static str,
    &'static [FaultCategory],
    u32,
    fn(&mut DstConfig),
    &'static [InvId],
);

#[test]
fn tk_053_planted_defects_found() {
    use FaultCategory::*;
    let rows: &[Row] = &[
        ("TK-053.1", "oms-429-fails", &[Reject], 1_000, none, &[InvId::I1]),
        (
            "TK-053.2",
            "oms-drop-unknown-report",
            &[TwoLeg],
            1_000,
            unknown_before_ack,
            &[InvId::I2],
        ),
        ("TK-053.3", "risk-no-exposure-limit", &[], 0, none, &[InvId::I3]),
        ("TK-053.4", "unknown-opens", &[Reject], 1_000, none, &[InvId::I4]),
        (
            "TK-053.5",
            "kill-no-risk-reducing",
            &[Control],
            1_000,
            none,
            &[InvId::I5],
        ),
        ("TK-053.6", "snapshot-field", &[], 0, none, &[InvId::I6]),
        ("TK-053.7", "slot-leak", &[], 0, none, &[InvId::I7]),
        ("TK-053.8", "hedge-gross-qty", &[], 0, base_fee, &[InvId::I8]),
        ("TK-053.9", "hashmap-order", &[], 0, rerun, &[InvId::I9]),
        (
            "TK-053.10",
            "mode-incremental",
            &[Control],
            1_000,
            none,
            &[InvId::I11],
        ),
        (
            "TK-053.11",
            "draining-blocks-rr",
            &[Control],
            1_000,
            none,
            &[InvId::I12],
        ),
        (
            "TK-053.12",
            "setmode-opens",
            &[Control],
            1_000,
            none,
            &[InvId::I13],
        ),
        ("TK-053.13", "gate-counts", &[Conn], 1_000, none, &[InvId::I14]),
        (
            "TK-053.14",
            "hedge-plus-2-lots",
            &[Reject],
            1_000,
            none,
            &[InvId::I15],
        ),
        ("TK-053.15", "ignore-fee", &[], 0, none, &[InvId::I16]),
        ("TK-053.16", "l3-overshoot", &[], 0, l3, &[InvId::I17]),
        (
            "TK-053.17",
            "drain-early-done",
            &[Control],
            1_000,
            none,
            &[InvId::I18],
        ),
        (
            "TK-053.18",
            "double-hedge-external",
            &[Account],
            1_000,
            none,
            &[InvId::I19],
        ),
        (
            "TK-053.19",
            "broken-never-autofix",
            &[TwoLeg, Reject],
            1_000,
            fast_broken,
            &[InvId::I20],
        ),
        (
            "TK-053.20",
            "broken-repeat-autofix",
            &[TwoLeg, Reject],
            1_000,
            fast_broken,
            &[InvId::I20],
        ),
    ];
    let mut misses = Vec::new();
    for (row, planted, cats, pm, tune, want) in rows {
        let make = |s| {
            let mut c = cfg(s, 4_000, cats, *pm, &[*planted]);
            tune(&mut c);
            c
        };
        match first_failure(MAX_SEEDS, make) {
            Some((mut found, f)) => {
                if *row == "TK-053.2" {
                    found.planted.clear();
                    let healthy = hft_testkit::dst::run_record::<hft_testkit::dst::ToyEngine>(&found);
                    assert!(
                        healthy.outcome.is_pass(),
                        "healthy unknown-report control: {:?}",
                        healthy.outcome.failure().map(|f| (f.step, &f.message))
                    );
                    assert!(
                        healthy.deliveries.iter().any(|(_, input)| {
                            input.kind == hft_event::EventKind::ORDER_EVENT
                                && hft_event::OrderEvent::try_read_from_bytes(&input.body)
                                    .ok()
                                    .is_some_and(|e| {
                                        e.slot == hft_types::SlotId::UNRESOLVED && e.cum_filled.0 > 0
                                    })
                        }),
                        "unknown-report defect must receive a genuinely unresolved fill"
                    );
                }
                let ok = matches!(f.invariant, FailKind::Inv(i) if want.contains(&i));
                if !ok {
                    misses.push(format!(
                        "{row} {planted}: reported {} ({})",
                        f.invariant.label(),
                        f.message
                    ));
                }
            }
            None => misses.push(format!("{row} {planted}: not found within MAX_SEEDS seeds")),
        }
    }
    // TK-053.21–.23 run on the real engine with its planted defects (feature `planted`).
    #[cfg(feature = "planted")]
    misses.extend(crate::planted_seq::tk_053_real_rows());
    assert!(misses.is_empty(), "{misses:#?}");
}
