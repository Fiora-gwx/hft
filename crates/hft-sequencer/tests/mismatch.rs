//! Decision mismatches during recovery (TESTS.md SEQ-268, SEQ-269, SEQ-362, SEQ-396).

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

#[test]
fn seq_396_recovery_notes_belong_to_next_output_interval() {
    for kind in [EventKind::ORDER_DECISION, EventKind::CMD_RESULT] {
        let (mut h, _) = tampered(kind);
        let start = h.snapshots().last().unwrap().seq;
        assert_eq!(snap(h.snapshots().last().unwrap()).0.purpose, 4);
        let expected_notes = h.s.engine().recovery_forced().len();
        assert!(expected_notes > 0);
        // Rerun's TSC is already past the restored keyframe deadline. Consume the
        // driver's automatic keyframe before requesting two more via the hk ring.
        h.run();
        let automatic = h.snapshots().pop().unwrap();
        assert!(automatic.seq > start);
        assert_eq!(snap(&automatic).0.purpose, 2);
        h.keyframe();
        h.keyframe();
        let mut full = xxhash_rust::xxh3::Xxh3::new();
        let mut without_notes = xxhash_rust::xxh3::Xxh3::new();
        let mut note_count = 0;
        let mut snapshots = 0;
        for r in h.j().records().filter(|r| r.seq > start) {
            if r.kind == EventKind::STATE_SNAPSHOT {
                let actual = snap(r).0.out_hash;
                assert_eq!(actual, full.digest128().to_le_bytes(), "{kind:?}, seq {}", r.seq);
                if snapshots == 0 {
                    assert_ne!(
                        actual,
                        without_notes.digest128().to_le_bytes(),
                        "recovery Notes were omitted"
                    );
                }
                full.reset();
                without_notes.reset();
                snapshots += 1;
            } else if r.role() == hft_event::Role::Output {
                crate::hash::accumulate(&mut full, r);
                let recovery_note = r.kind == EventKind::NOTE
                    && r.read::<hft_event::Note>().is_some_and(|n| {
                        n.code == hft_sequencer::codes::seq_note(hft_sequencer::codes::note::RECOVER_MISMATCH)
                    });
                if recovery_note {
                    note_count += 1;
                } else {
                    crate::hash::accumulate(&mut without_notes, r);
                }
            }
        }
        assert_eq!(note_count, expected_notes);
        assert_eq!(snapshots, 3, "one automatic and two requested keyframes");
        assert!(
            h.verify_from(Some(start)).is_empty(),
            "recovery run must verify strictly"
        );
    }
}
