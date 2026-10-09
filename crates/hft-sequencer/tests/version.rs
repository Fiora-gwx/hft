//! Version changes and rollback (TESTS.md SEQ-273, SEQ-274).

use hft_event::ControlCmd;
use hft_sequencer::{BootError, RecoveryFlags, SnapshotSel, StepOutcome};

use crate::common::*;

/// B0 run that ends with a finished Drain (its snapshot is the journal's tail).
fn drained() -> H {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    h.cmd_confirmed(
        hft_types::AccountId(0),
        &ControlCmd::Drain { cancel_resting: true },
    );
    let mut done = false;
    for _ in 0..200 {
        match h.step() {
            StepOutcome::Done { .. } => {
                done = true;
                break;
            }
            _ => h.adv(10_000_000),
        }
    }
    assert!(done, "the drain finished");
    h
}

#[test]
fn seq_273_version_change_boot() {
    // .1 same version, Latest + tail.
    reset_script();
    let mut h = b0().boot();
    h.steady();
    h.keyframe();
    h.quote(A1, "50000", "50004");
    h.run();
    let j = h.j().clone();
    let setup = h.setup.clone();
    let h1 = H::<TestSet>::restart_over(
        j.clone(),
        setup.clone(),
        plan(SnapshotSel::Latest, true, 1, 0),
        Rerun::default(),
    )
    .unwrap();
    assert!(h1.s.engine().recovery_replayed() > 0);
    // .3 version differs, the tail is not a Drain snapshot: VersionNeedsDrain.
    let rr = Rerun {
        build: [7; 20],
        ..Rerun::default()
    };
    let r = H::<TestSet>::restart_over(
        j.clone(),
        setup.clone(),
        plan(SnapshotSel::Latest, true, 1, 0),
        rr.clone(),
    );
    assert_eq!(r.err(), Some(BootError::VersionNeedsDrain));
    // .4 NewestReadable, no tail, FROM_SNAPSHOT | TAIL_LOST.
    let f = RecoveryFlags::FROM_SNAPSHOT | RecoveryFlags::TAIL_LOST;
    let h4 =
        H::<TestSet>::restart_over(j, setup, plan(SnapshotSel::NewestReadable, false, 1, f), rr).unwrap();
    for (_, fl) in h4.s.engine().recovery_flags() {
        assert_eq!(fl & f, f);
    }
    // .2 / .5 DrainTail: the Drain snapshot is read by the next version (V4) but not by V5.
    let d = drained();
    let j = d.j().clone();
    let setup = d.setup.clone();
    let rr = Rerun {
        build: [7; 20],
        ..Rerun::default()
    };
    let h2 = H::<TestSetV4>::restart_over(
        j.clone(),
        setup.clone(),
        plan(SnapshotSel::DrainTail, false, 1, 0),
        rr.clone(),
    );
    assert!(h2.is_ok(), "{:?}", h2.err());
    let h5 = H::<TestSetV5>::restart_over(j, setup, plan(SnapshotSel::DrainTail, false, 1, 0), rr);
    assert_eq!(h5.err(), Some(BootError::SnapshotUndecodable { section: 5 }));
}

#[test]
fn seq_274_rollback_from_snapshot() {
    let d = drained();
    let j = d.j().clone();
    let setup = d.setup.clone();
    // The new version runs from the Drain snapshot and crashes (no Drain at the tail).
    let rr_new = Rerun {
        build: [9; 20],
        ..Rerun::default()
    };
    let mut v2 = H::<TestSet>::restart_over(
        j,
        setup.clone(),
        plan(SnapshotSel::DrainTail, false, 1, 0),
        rr_new,
    )
    .unwrap();
    v2.step();
    v2.quote(A1, "50000", "50004");
    v2.run();
    let v2_epoch = v2.s.view().engine_epoch();
    let j2 = v2.j().clone();
    // The old version: Latest gives VersionNeedsDrain; NewestReadable without a tail works.
    let rr_old = Rerun {
        build: [1; 20],
        ..Rerun::default()
    };
    let r = H::<TestSet>::restart_over(
        j2.clone(),
        setup.clone(),
        plan(SnapshotSel::Latest, true, v2_epoch, 0),
        rr_old.clone(),
    );
    assert_eq!(r.err(), Some(BootError::VersionNeedsDrain));
    let f = RecoveryFlags::FROM_SNAPSHOT | RecoveryFlags::TAIL_LOST;
    let h = H::<TestSet>::restart_over(
        j2,
        setup,
        plan(SnapshotSel::NewestReadable, false, v2_epoch, f),
        rr_old,
    )
    .unwrap();
    assert!(h.s.view().engine_epoch() > v2_epoch);
    for a in [AC1, AC2, AC9] {
        assert!(
            h.s.engine()
                .risk
                .mode
                .caps()
                .iter()
                .any(|c| c.source == hft_types::CapSource::RECOVERY && c.scope.account == a)
        );
    }
}
