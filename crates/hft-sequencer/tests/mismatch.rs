//! Decision mismatches during recovery (TESTS.md SEQ-268, SEQ-269, SEQ-362).

use hft_event::EventKind;
use hft_sequencer::{RecoveryFlags, SnapshotSel};
use hft_types::{Side, VenueId};

use crate::common::*;

/// B0 with a keyframe, then an order of account 1 on B whose logged decision is tampered with.
fn tampered(kind: EventKind) -> (H, u64) {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    let kf = h.keyframe().seq;
    h.cmd_confirmed(
        AC1,
        &hft_event::ControlCmd::SetMode {
            scope: scope(S1, AC1, G1),
            mode: hft_types::RunMode::Live,
        },
    );
    h.run();
    h.open_position(AC1, B1, Side::Sell, "0.001");
    let mut j = h.j().clone();
    let mut seq = 0;
    'o: for seg in j.segments.iter_mut() {
        for r in seg.records.iter_mut() {
            if r.kind == kind && r.role() == hft_event::Role::Output && r.seq > kf {
                // Flip the lowest byte of qty (OrderDecision) / of the body tail (others).
                let o = if kind == EventKind::ORDER_DECISION {
                    24
                } else {
                    r.body.len() - 8
                };
                r.body[o] ^= 1;
                seq = r.seq;
                break 'o;
            }
        }
    }
    let setup = h.setup.clone();
    let h2 = H::<TestSet>::restart_over(j, setup, plan(SnapshotSel::Latest, true, 1, 0), Rerun::default())
        .unwrap();
    (h2, seq)
}

#[test]
fn seq_268_mismatch_scope() {
    // .1 an OrderDecision of account 1 on B → only (B, 1).
    let (h, _) = tampered(EventKind::ORDER_DECISION);
    let forced: Vec<(VenueId, hft_types::AccountId)> =
        h.s.engine()
            .recovery_forced()
            .iter()
            .map(|x| (x.0, x.1))
            .collect();
    assert_eq!(forced, vec![(B, AC1)]);
    // .4 a CmdResult (no account) → every (venue, account) of the live accounts.
    let (h, _) = tampered(EventKind::CMD_RESULT);
    let forced: Vec<(VenueId, hft_types::AccountId)> =
        h.s.engine()
            .recovery_forced()
            .iter()
            .map(|x| (x.0, x.1))
            .collect();
    for p in [(A, AC1), (B, AC1), (A, AC2), (B, AC2)] {
        assert!(forced.contains(&p), "{forced:?}");
    }
}

#[test]
fn seq_269_mismatch_handling() {
    let (h, seq) = tampered(EventKind::ORDER_DECISION);
    let e = h.s.engine();
    let f = e
        .recovery_flags()
        .iter()
        .find(|x| x.0 == AC1)
        .map(|x| x.1)
        .unwrap();
    assert!(f & RecoveryFlags::RECOVER_MISMATCH != 0);
    let notes: Vec<hft_event::Note> = h
        .j()
        .records()
        .filter(|r| r.kind == EventKind::NOTE)
        .filter_map(|r| r.read::<hft_event::Note>())
        .filter(|n| n.code == hft_sequencer::codes::seq_note(hft_sequencer::codes::note::RECOVER_MISMATCH))
        .collect();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].level, hft_event::NoteLevel::P0);
    assert_eq!(notes[0].v[0], u64::from(B.0) | (u64::from(AC1.0) << 8));
    assert_eq!(notes[0].v[1], seq);
}

#[test]
fn seq_362_one_note_per_pair() {
    let (h, _) = tampered(EventKind::CMD_RESULT);
    let notes: Vec<hft_event::Note> = h
        .j()
        .records()
        .filter(|r| r.kind == EventKind::NOTE)
        .filter_map(|r| r.read::<hft_event::Note>())
        .filter(|n| n.code == hft_sequencer::codes::seq_note(hft_sequencer::codes::note::RECOVER_MISMATCH))
        .collect();
    let mut keys: Vec<u64> = notes.iter().map(|n| n.v[0]).collect();
    let n = keys.len();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), n);
    assert_eq!(n, h.s.engine().recovery_forced().len());
}
