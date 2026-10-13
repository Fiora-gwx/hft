//! Physical numbering is assigned before append returns, and survives retained-tail recovery.
use hft_event::{EventKind, PART_MAX_CHUNK, RecordFlags};
use hft_sequencer::driver::replay::Log;
use hft_sequencer::{JournalSink, MemJournal, SegClock};
use hft_types::Tsc;
use zerocopy::{FromZeros, IntoBytes};

#[test]
fn seq_424_physical_meta_boundaries_distinguish_boot_and_open_events() {
    use hft_sequencer::journal::{BootSink, RollMeta};
    let mut j = MemJournal::with_physical_seq(SegClock::default());
    let meta = hft_event::ClockStatus::new_zeroed();
    let input = hft_event::Timer::new_zeroed();
    let info = RollMeta {
        utc_ns: 0,
        calib: hft_rt::CalibPair::default(),
        tsc_hz_milli: 1_000_000_000_000,
        configs: &[],
    };
    for _ in 0..2 {
        let mut b = j.roll(hft_journal::RollReason::Manual, &info).unwrap();
        b.append_var(EventKind::CLOCK_STATUS, Tsc(0), &[meta.as_bytes()])
            .unwrap();
        b.finish().unwrap();
        let seg = j.segments.last().unwrap();
        assert!(!seg.records[0].flags.contains(RecordFlags::EVENT_END));
        assert!(!seg.records[1].flags.contains(RecordFlags::EVENT_END));
        assert!(seg.records[2].flags.contains(RecordFlags::EVENT_END));
        let standalone = j.append(RecordFlags(0), Tsc(1), &meta).unwrap().0;
        j.publish();
        assert_eq!(j.records().nth(j.committed - 1).unwrap().seq, standalone);
        assert!(j.records().last().unwrap().flags.contains(RecordFlags::EVENT_END));
        j.append(RecordFlags(0), Tsc(2), &input).unwrap();
        j.append(RecordFlags(0), Tsc(2), &meta).unwrap();
        assert!(!j.records().last().unwrap().flags.contains(RecordFlags::EVENT_END));
        j.end_event();
        j.publish();
        let rows = &j.segments.last().unwrap().records;
        assert!(rows[rows.len() - 2].flags.contains(RecordFlags::EVENT_END));
        assert!(!rows.last().unwrap().flags.contains(RecordFlags::EVENT_END));
    }
}

#[test]
fn seq_423_memory_multipart_boundaries_and_truncation() {
    for physical in [false, true] {
        let mut j = if physical {
            MemJournal::with_physical_seq(SegClock::default())
        } else {
            MemJournal::new(SegClock::default())
        };
        let mut next = 1;
        for (len, parts) in [
            (size_of::<hft_event::SnapshotHdr>(), 1),
            (PART_MAX_CHUNK, 1),
            (PART_MAX_CHUNK + 1, 2),
            (2 * PART_MAX_CHUNK, 2),
            (2 * PART_MAX_CHUNK + 1, 3),
        ] {
            let mut body = vec![0; len];
            body[0] = 2; // Keyframe
            let first = j
                .append_var(EventKind::STATE_SNAPSHOT, RecordFlags(0), Tsc(7), &[&body])
                .unwrap()
                .0;
            assert_eq!(first, next);
            next += if physical { parts } else { 1 };
            assert_eq!(j.next_seq, next);
            assert_eq!(j.record_end_seq(first), next - 1);
            j.end_event();
            j.publish();
        }
        assert_eq!(j.len(), 5);
        assert_eq!(j.committed, 5);
        assert_eq!(j.truncate_to_event_end(), 0);
        assert_eq!(j.next_seq, next, "retained tail includes all its parts");
        let retained = Log::from_segments(&j.segments);
        assert_eq!(retained.record_end_seq(4), Some(next - 1));
        let body = vec![2; PART_MAX_CHUNK + 1];
        for externally_cut in [false, true] {
            j.append_var(EventKind::STATE_SNAPSHOT, RecordFlags(0), Tsc(8), &[&body])
                .unwrap();
            assert_eq!(j.len(), 6);
            if externally_cut {
                // SeqSut recovery first removes records beyond the published boundary.
                j.segments[0].records.truncate(5);
                let prefix = Log::from_segments(&j.segments);
                assert_eq!(prefix.recs, retained.recs);
                assert_eq!(prefix.part_ends, retained.part_ends);
            }
            assert_eq!(j.truncate_to_event_end(), usize::from(!externally_cut));
            assert_eq!(j.len(), 5);
            assert_eq!(j.committed, 5);
            assert_eq!(j.next_seq, next);
            assert_eq!(j.segments[0].part_ends, retained.part_ends);
            assert_eq!(j.published().count(), 5);
        }
        assert_eq!(
            j.append_var(EventKind::STATE_SNAPSHOT, RecordFlags(0), Tsc(9), &[&body])
                .unwrap()
                .0,
            next
        );
    }
}
