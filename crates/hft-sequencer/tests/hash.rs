//! Output intervals at snapshot boundaries (TESTS.md SEQ-314).

use hft_event::{ControlCmd, EventKind, RecordFlags, Role};
use hft_sequencer::driver::live::RingId;
use hft_sequencer::driver::replay::{Log, ReplayDriver, ReplayMode};
use hft_sequencer::{BootSource, MemJournal, MemRecord, RecoveryFlags, Sequencer, SnapshotSel, StepOutcome};
use hft_types::{AccountId, InstId, Tsc};
use xxhash_rust::xxh3::Xxh3;
use zerocopy::{FromZeros, IntoBytes};

use crate::common::*;

// Independent of OutHash and its reset/stamping helpers. Only the event schema supplies the
// measurement fields and transport flag bits excluded from the byte stream.
fn accumulate(hash: &mut Xxh3, r: &MemRecord) {
    let mut body = r.body.clone();
    for (kind, off, len) in hft_event::MEASURE_MASK {
        if *kind == r.kind {
            body[usize::from(*off)..usize::from(*off + *len)].fill(0);
        }
    }
    let transport = RecordFlags::EVENT_END.0 | RecordFlags::CRC.0 | RecordFlags::PART.0;
    hash.update(&r.kind.0.to_le_bytes());
    hash.update(&(r.flags.0 & !transport).to_le_bytes());
    hash.update(&body);
}

fn assert_intervals(j: &MemJournal) {
    let mut hash = Xxh3::new();
    for r in j.records() {
        if r.kind == EventKind::STATE_SNAPSHOT {
            assert_eq!(
                snap(r).0.out_hash,
                hash.digest128().to_le_bytes(),
                "seq={}",
                r.seq
            );
            hash.reset();
        } else if r.role() == Role::Output {
            accumulate(&mut hash, r);
        }
    }
}

#[test]
fn seq_314_out_hash_and_verify() {
    reset_script();
    let mut b = b0();
    let book_inst = InstId(15);
    b.rd.insts
        .iter_mut()
        .find(|i| i.id == book_inst)
        .unwrap()
        .l2_depth = 5;
    let mut h = b.boot();
    h.steady();
    let first = h.keyframe().seq;
    // Each expired command produces one Output and does not schedule a snapshot.
    for _ in 0..1_000 {
        let expired = Tsc(h.now() - 1);
        let id = h.cmd_with("cli:test", ADMIN, AC1, &ControlCmd::SnapshotNow, |c| {
            c.expires_tsc = expired;
        });
        assert_eq!(h.run(), StepOutcome::Idle);
        assert_eq!(h.res(id).0, hft_sequencer::codes::status::EXPIRED);
    }
    let second = h.keyframe().seq;
    assert_eq!(
        h.j()
            .records()
            .filter(|r| r.seq > first && r.seq < second && r.role() == Role::Output)
            .count(),
        1_000
    );
    h.cmd(AC1, &ControlCmd::SnapshotNow);
    h.run();
    h.run();
    let on_demand = h.snapshots().last().unwrap().seq;
    assert_eq!(snap(h.snapshots().last().unwrap()).0.purpose, 3);
    assert_intervals(h.j());
    for from in [None, Some(first), Some(second), Some(on_demand)] {
        assert!(h.verify_from(from).is_empty(), "from={from:?}");
    }

    // Corrupt an Output and recompute the following logged digest from that corrupt stream.
    // Re-execution must still disagree both at the first bad record and at the interval end.
    let mut bad = h.j().clone();
    let mut changed = 0;
    let mut hash = Xxh3::new();
    for r in bad.segments.iter_mut().flat_map(|s| &mut s.records) {
        if r.seq <= first || r.seq > second {
            continue;
        }
        if r.seq == second {
            r.body[40..56].copy_from_slice(&hash.digest128().to_le_bytes());
        } else if r.role() == Role::Output {
            if changed == 0 {
                assert_eq!(r.kind, EventKind::CMD_RESULT);
                r.body[0] ^= 1;
                changed = r.seq;
            }
            accumulate(&mut hash, r);
        }
    }
    assert_ne!(changed, 0);
    let mismatches = verify_journal::<TestSet>(&bad, &h.setup, Some(first));
    assert_eq!(mismatches.first().unwrap().seq, changed);
    assert!(mismatches.iter().any(|m| m.seq == second));
    let recovered = H::<TestSet>::restart_over(
        bad.clone(),
        h.setup.clone(),
        plan(SnapshotSel::AtSeq(first), true, 1, 0),
        Rerun::default(),
    )
    .unwrap();
    assert!(
        recovered
            .s
            .engine()
            .recovery_mismatches()
            .iter()
            .any(|m| m.seq == second)
    );
    assert!(
        recovered
            .s
            .engine()
            .recovery_flags()
            .iter()
            .any(|(_, f)| f & RecoveryFlags::RECOVER_MISMATCH != 0)
    );
    let drv = ReplayDriver::new(ReplayMode::Mirror, Log::from_segments(&bad.segments), 0, 7);
    let mut mirror: Sequencer<ReplayDriver, TestSet> = Sequencer::boot(
        BootSource::Journal(plan(SnapshotSel::AtSeq(first), true, 0, 0)),
        &h.setup,
        drv,
        hb(),
    )
    .unwrap();
    assert_eq!(
        (0..100_000)
            .map(|_| mirror.step())
            .find(|s| *s != StepOutcome::Busy),
        Some(StepOutcome::Idle)
    );
    assert!(mirror.driver().mismatches().is_empty());
    assert!(mirror.driver().finished());

    // Segment config Outputs precede the boot snapshot. Drain's command result and mode
    // Outputs precede its snapshot within the very same event.
    let mut l2 = hft_event::L2SnapHdr::new_zeroed();
    l2.h.venue = A;
    l2.h.inst = book_inst;
    l2.h.rx_tsc = h.now();
    l2.snap_id = 1;
    l2.origin = hft_event::L2SnapHdr::TOP_N;
    l2.n_bids = 1;
    l2.n_asks = 1;
    let mut body = l2.as_bytes().to_vec();
    for price in ["10", "10.01"] {
        body.extend_from_slice(
            hft_event::Level {
                px: px(price),
                qty: q("1"),
            }
            .as_bytes(),
        );
    }
    h.push_bytes(RingId::Md(0), EventKind::L2_SNAPSHOT, body);
    h.run();
    h.sys(2);
    let rolled = h.snapshots().last().unwrap().seq;
    assert!(
        h.j()
            .records()
            .any(|r| r.seq > rolled && r.kind == EventKind::L2_SNAPSHOT && r.role() == Role::Output)
    );
    h.keyframe();
    h.cmd_with(
        "cli:ops",
        ADMIN,
        AccountId(0),
        &ControlCmd::Drain {
            cancel_resting: false,
        },
        |c| c.confirmed = 1,
    );
    assert!(matches!(h.run(), StepOutcome::Done { .. }));
    assert_intervals(h.j());
    assert!(h.verify_from(Some(first)).is_empty());
    assert!(h.verify_from(Some(rolled)).is_empty());
}
