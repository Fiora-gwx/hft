//! Export an already physical-numbered DST log. Writer seqs and reader bytes must match the
//! execution source; no header, payload, embedded basis, or snapshot seq is renumbered.
use hft_event::{BlobHdr, EventKind, RecordFlags, Role, SegmentRoll};
use hft_journal::{InstanceName, JournalConfig, JournalKind, JournalWriter, PageSize, RollReason};
use hft_rt::CalibPair;
use hft_sequencer::driver::replay::Log;
use hft_sequencer::journal::{JournalSink, RollMeta};
use hft_types::{Tsc, VenueId};
use std::path::{Path, PathBuf};

pub fn export(log: &Log, root: &Path) -> PathBuf {
    assert!(
        !root.exists(),
        "fixture destination already exists: {}",
        root.display()
    );
    assert!(!log.recs.is_empty());
    assert_eq!(log.seg_start.first(), Some(&0));
    assert_eq!(log.segs.len(), log.seg_start.len());
    assert!(log.recovery_gaps.is_empty(), "DST retains every committed event");
    let cfg = JournalConfig {
        name: "seq".into(),
        kind: JournalKind::Seq,
        venue: VenueId(0),
        instance: InstanceName::Prod,
        hot_dir: root.join("unused-hot"),
        cold_root: root.join("unused-cold"),
        // Fixed test capacity. Oversized source segments fail instead of silently adding rolls.
        segment_bytes: 64 << 20,
        page: PageSize::Normal,
        size_idle_pct: 75,
        size_valve_pct: 90,
        hot_keep: 1,
    };
    let mut writer = JournalWriter::open_offline(&cfg, root).unwrap();
    for (si, &start) in log.seg_start.iter().enumerate() {
        let end = log.seg_start.get(si + 1).copied().unwrap_or(log.recs.len());
        let rows = &log.recs[start..end];
        let first = rows[0].read::<SegmentRoll>().unwrap();
        assert_eq!(rows[0].kind, EventKind::SEGMENT_ROLL);
        assert_eq!(first.mark, hft_journal::RollMark::Begin as u8);
        assert_eq!(first.segment_no, si as u64 + 1);
        let done = rows
            .iter()
            .position(|r| {
                r.kind == EventKind::SEGMENT_ROLL
                    && r.read::<SegmentRoll>().unwrap().mark == hft_journal::RollMark::BootDone as u8
            })
            .unwrap();
        let refs: Vec<_> = rows[1..done]
            .iter()
            .filter(|r| r.kind == EventKind::CONFIG_BLOB)
            .map(|r| r.read::<BlobHdr>().unwrap().blob)
            .collect();
        let clock = log.segs[si];
        let info = RollMeta {
            utc_ns: first.utc_ns,
            calib: CalibPair {
                tsc: Tsc(clock.calib_tsc),
                realtime_ns: clock.calib_realtime_ns,
                phc_ns: 0,
                err_ns: 0,
            },
            tsc_hz_milli: clock.tsc_hz_milli,
            configs: &refs,
        };
        let mut boot =
            JournalSink::roll(&mut writer, RollReason::from_u8(first.reason).unwrap(), &info).unwrap();
        for r in &rows[1..done] {
            let seq = boot.append_var(r.kind, Tsc(r.tsc), &[&r.body]).unwrap();
            assert_eq!(seq.0, r.seq, "boot seq assigned before execution");
        }
        assert_eq!(boot.finish().unwrap().0, rows[done].seq);
        let mut event_done = false;
        for r in &rows[done + 1..] {
            // end_event changes the last non-Meta record; delay it through that event's
            // trailing measurement Meta so the writer does not mark those as standalone.
            let standalone = r.role() == Role::Meta && r.flags.contains(RecordFlags::EVENT_END);
            if event_done && (r.role() != Role::Meta || standalone) {
                writer.end_event();
                event_done = false;
            }
            if r.kind == EventKind::SEGMENT_ROLL {
                assert_eq!(
                    r.read::<SegmentRoll>().unwrap().mark,
                    hft_journal::RollMark::End as u8
                );
                assert_eq!(r.seq, rows.last().unwrap().seq);
                assert!(si + 1 < log.segs.len(), "source is open, exporter adds final End");
                continue; // next roll writes and validates this exact End in the read-back
            }
            let seq = writer
                .append_var(r.kind, r.flags, Tsc(r.tsc), &[&r.body])
                .unwrap();
            assert_eq!(seq.0, r.seq, "body contains the originally assigned seq");
            event_done |= r.flags.contains(RecordFlags::EVENT_END);
        }
        if event_done {
            writer.end_event();
        }
        writer.publish();
    }
    writer.seal_and_close().unwrap();
    let dir = root.join("prod/seq");
    for si in 1..=log.segs.len() {
        super::seal(&dir.join(format!("{si:010}.jrn")));
    }
    // Use the same real reader/decoder as existing physical fixtures. Keep seed evidence.
    let actual = super::read_root(root);
    assert_eq!(actual.recs.len(), log.recs.len() + 1, "only final End is added");
    assert_eq!(actual.seg_start, log.seg_start);
    assert_eq!(actual.segs, log.segs);
    assert_eq!(actual.part_ends, log.part_ends);
    let wire = RecordFlags::CRC.0 | RecordFlags::PART.0;
    for (ix, (a, b)) in actual.recs.iter().zip(&log.recs).enumerate() {
        assert_eq!((a.seq, a.kind, a.tsc), (b.seq, b.kind, b.tsc));
        assert_eq!(a.body, b.body, "source body at {}", b.seq);
        assert_eq!(a.flags.0 & !wire, b.flags.0 & !wire, "source flags at {}", b.seq);
        assert_eq!(actual.record_end_seq(ix), log.record_end_seq(ix));
    }
    dir
}
