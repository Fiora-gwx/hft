//! Counting-Note cadence and Note registration (TESTS.md LPR-056, LPR-057).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::type_complexity
)] // allow-list: test code

mod common;

use std::collections::BTreeSet;

use common::*;
use hft_strategy_sdk::{ExecUpdateKind, IntentOutcome, NoteLevel, StuckReason, TargetReject};
use hft_types::RejectBits;

fn silent(h: &mut H) {
    h.healthy = false;
    let t = h.tsc_of(h.now().engine_ns - 7_000_000_000);
    hft_strategy_sdk::testing::MockViews::conn(&mut h.v.m, I2, t, true, false);
}

/// LPR-056 steps 1–2 on a GQ instance started at +0.
fn counts(h: &mut H) {
    h.timer(GQ, 99);
    for s in 1..=3 {
        h.at(s * 1_000);
        silent(h);
        assert!(h.bbo(I2).places.is_empty());
    }
    h.healthy = true;
    for s in 4..=5 {
        h.at(s * 1_000);
        h.next_outcomes = Some(vec![IntentOutcome::Rejected(RejectBits::RATE)]);
        assert_eq!(h.bbo(I2).places.len(), 1);
    }
    h.at(6_000);
    let r = h.bbo(I2).refs[0];
    h.at(6_050);
    h.order(GQ, r, probe_tag(0), rejected(0, 2010));
}

#[test]
fn lpr_056_count_note_cadence() {
    let mut h = H::new(&[], vec![gq()]);
    counts(&mut h);
    h.at(599_999);
    let rec = h.bbo(I2);
    assert!(
        rec.notes.iter().all(|n| !(2..=5).contains(&n.code)),
        "step 2: {:?}",
        rec.notes
    );
    h.at(600_000);
    let rec = h.bbo(I2);
    let mut got: Vec<(u32, [u64; 3])> = rec
        .notes
        .iter()
        .map(|n| (n.code, [n.v[0], n.v[1], n.v[2]]))
        .collect();
    got.sort();
    let rate = u64::from(RejectBits::RATE.0);
    assert_eq!(got, vec![(2, [1, 0, 2010]), (3, [2, rate, 0]), (5, [0, 3, 0])]);
    assert!(
        rec.notes
            .iter()
            .all(|n| n.group == GQ && n.level == NoteLevel::Info)
    );
    h.at(1_200_000);
    let rec = h.bbo(I2);
    assert!(
        rec.notes.iter().all(|n| !(2..=5).contains(&n.code)),
        "step 4: {:?}",
        rec.notes
    );
}

fn codes(h: &H) -> BTreeSet<(u32, String)> {
    h.all
        .iter()
        .flat_map(|r| r.notes.iter().map(|n| (n.code, format!("{:?}", n.level))))
        .collect()
}

#[test]
fn lpr_057_all_codes_registered() {
    let mut all = BTreeSet::new();
    // 1: unexpected probe fill (LPR-016 step 3).
    let mut h = H::new(&[], vec![gq()]);
    let r = h.bbo(I2).refs[0];
    h.net(GQ, 0, 100_000);
    h.order(GQ, r, probe_tag(0), filled(100_000, 5_820_000, true));
    all.extend(codes(&h));
    // 2, 3, 5: counting Notes (LPR-056).
    let mut h = H::new(&[], vec![gq()]);
    counts(&mut h);
    h.at(600_000);
    h.bbo(I2);
    all.extend(codes(&h));
    // 4: PairNoFill (LPR-033 then +600 000 ms).
    let mut h = H::new(&[], vec![gps()]);
    h.bbo(I2);
    h.timer(GPS, 1);
    let r = h.bbo(I1).refs[0];
    h.order(GPS, r, OPEN_TAG, cancelled());
    h.at(600_000);
    h.bbo(I2);
    all.extend(codes(&h));
    // 6: FlattenIncomplete (LPR-037.11).
    let mut h = H::new(&[], vec![gps()]);
    h.bbo(I2);
    h.timer(GPS, 1);
    let r = h.bbo(I1).refs[0];
    h.order(GPS, r, OPEN_TAG, filled(100_000, 6_030_001, true));
    h.net(GPS, 0, 100_000).net(GPS, 1, -100_000);
    for _ in 0..3 {
        h.timer(GPS, 2);
    }
    all.extend(codes(&h));
    // 7: ProbeHalted (LPR-019.2).
    let mut h = H::new(&[], vec![gq()]);
    for k in 0..5 {
        h.at(k * 1_000);
        let r = h.bbo(I2).refs[0];
        h.order(GQ, r, probe_tag(0), rejected(0, 1));
    }
    all.extend(codes(&h));
    // 8: TargetStuck (LPR-044); 9: TargetRejected (LPR-042.3).
    for stuck in [true, false] {
        let mut h = H::new(&[], vec![gts()]);
        h.bbo(I2);
        h.timer(GTS, 1);
        if !stuck {
            h.next_outcomes = Some(vec![IntentOutcome::TargetRejected(TargetReject::BadLegs)]);
        }
        h.bbo(I1);
        h.at(10);
        h.bbo(I2);
        if stuck {
            h.exec(GTS, ExecUpdateKind::Stuck(StuckReason::NoLiquidity));
        }
        all.extend(codes(&h));
    }
    use NoteLevel::{Info, P1, P2};
    let want: BTreeSet<(u32, String)> = [
        (1, P2),
        (2, Info),
        (3, Info),
        (4, Info),
        (5, Info),
        (6, P1),
        (7, P1),
        (8, P2),
        (9, P2),
    ]
    .into_iter()
    .map(|(c, l)| (c, format!("{l:?}")))
    .collect();
    assert_eq!(all, want);
}
