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
