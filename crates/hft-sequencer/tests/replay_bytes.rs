use hft_event::{EventKind, OrderDecision, RecordFlags, SnapshotHdr};
use hft_sequencer::driver::replay::{Log, ReplayMode, ReplaySink};
use hft_sequencer::{JournalSink, MemRecord};
use hft_types::Tsc;
use zerocopy::{FromZeros, IntoBytes};

fn compared(kind: EventKind, logged: &[u8], actual: &[u8], flags: RecordFlags) -> bool {
    let mut log = Log::default();
    log.recs.push(MemRecord {
        seq: 73,
        kind,
        flags,
        tsc: 40,
        body: logged.to_vec(),
    });
    let mut sink = ReplaySink::new(ReplayMode::Verify, log);
    assert_eq!(
        sink.append_var(kind, RecordFlags(0), Tsc(999), &[actual])
            .unwrap()
            .0,
        73
    );
    assert_eq!(sink.compared, 1);
    sink.mismatches.is_empty()
}

#[test]
fn seq_392_extra_output_preserves_input_and_missing_tail_fails() {
    use hft_sequencer::driver::replay::ReplayDriver;
    let input = MemRecord {
        seq: 80,
        kind: EventKind::TIMER,
        flags: RecordFlags::EVENT_END,
        tsc: 10,
        body: vec![0; 32],
    };
    let output = MemRecord {
        seq: 81,
        kind: EventKind::ORDER_DECISION,
        flags: RecordFlags::EVENT_END,
        tsc: 10,
        body: vec![0; size_of::<OrderDecision>()],
    };
    let log = Log {
        recs: vec![input.clone(), output.clone()],
        ..Default::default()
    };
    let mut sink = ReplaySink::new(ReplayMode::Verify, log.clone());
    sink.append_var(output.kind, output.flags, Tsc(0), &[&output.body])
        .unwrap();
    assert_eq!(sink.pos, 0, "generated Output consumed the next Input");
    assert_eq!(sink.mismatches[0].seq, input.seq);
    assert_eq!(
        sink.append_var(input.kind, input.flags, Tsc(0), &[&input.body])
            .unwrap()
            .0,
        input.seq
    );
    assert_eq!(sink.pos, 1);
    sink.append_var(output.kind, output.flags, Tsc(0), &[&output.body])
        .unwrap();
    assert_eq!(sink.pos, 2);
    assert_eq!(sink.mismatches.len(), 1);
    let mut driver = ReplayDriver::new(ReplayMode::Verify, log.clone(), 1, 1);
    driver.finish_verification();
    assert_eq!(driver.mismatches()[0].seq, output.seq);
    assert_eq!(driver.sink.detail[0].2, Vec::<u8>::new());
    driver.finish_verification();
    assert_eq!(
        driver.mismatches().len(),
        1,
        "finalization double-counted a missing Output"
    );
    // The next run's Output may be byte-identical. It must not satisfy an extra Output
    // emitted by the old run, and a boundary at zero has no preceding record.
    for end in [0, 1] {
        let mut sink = ReplaySink::new(ReplayMode::Verify, log.clone());
        sink.pos = end;
        sink.record_end = end;
        sink.append_var(output.kind, output.flags, Tsc(0), &[&output.body])
            .unwrap();
        assert_eq!(sink.pos, end);
        assert_eq!(sink.compared, 0);
        assert_eq!(sink.mismatches.len(), 1);
        assert_eq!(sink.mismatches[0].seq, if end == 0 { 1 } else { 81 });
        assert!(sink.detail[0].1.is_empty());
    }
    let mut sink = ReplaySink::new(ReplayMode::Verify, log);
    sink.stop_on_first = true;
    sink.append_var(output.kind, output.flags, Tsc(0), &[&output.body])
        .unwrap();
    assert!(sink.halted);
    assert!(
        sink.append_var(input.kind, input.flags, Tsc(0), &[&input.body])
            .is_err()
    );
    assert_eq!(sink.pos, 0);
}

#[test]
fn seq_384_replay_body_length_is_significant() {
    let mut bytes = vec![0u8; size_of::<OrderDecision>()];
    bytes[..8].copy_from_slice(&72u64.to_le_bytes());
    // Valid action/side/tif; the comparator uses the schema's byte mask, not decoded orders.
    bytes[59..62].fill(1);
    let original = bytes.as_slice();
    assert_eq!(original.last(), Some(&0));
    assert!(compared(
        EventKind::ORDER_DECISION,
        original,
        original,
        RecordFlags::EVENT_END
    ));
    for extra in [1, 8] {
        let mut changed = original.to_vec();
        changed.resize(changed.len() + extra, 0);
        assert!(!compared(
            EventKind::ORDER_DECISION,
            &changed,
            original,
            RecordFlags(0)
        ));
        assert!(!compared(
            EventKind::ORDER_DECISION,
            original,
            &changed,
            RecordFlags(0)
        ));
    }
    assert!(!compared(
        EventKind::ORDER_DECISION,
        &original[..original.len() - 1],
        original,
        RecordFlags(0)
    ));
    assert!(!compared(
        EventKind::ORDER_DECISION,
        original,
        &original[..original.len() - 1],
        RecordFlags(0)
    ));
    assert!(!compared(
        EventKind::ORDER_DECISION,
        original,
        original,
        RecordFlags::SHADOW
    ));
    let mut measurement = original.to_vec();
    measurement[8..16].fill(0xff);
    assert!(compared(
        EventKind::ORDER_DECISION,
        &measurement,
        original,
        RecordFlags::CRC
    ));
    let snapshot = SnapshotHdr::new_zeroed();
    let mut changed = snapshot.as_bytes().to_vec();
    changed.push(0);
    assert!(!compared(
        EventKind::STATE_SNAPSHOT,
        &changed,
        snapshot.as_bytes(),
        RecordFlags(0)
    ));
}
