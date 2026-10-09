//! Snapshot writer and reader (SDK-025, SDK-191).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use hft_strategy_sdk::{RestoreError, SnapshotFull, SnapshotReader, SnapshotWriter};

#[test]
fn sdk_025_overflow_reports_snapshot_full() {
    let mut w = SnapshotWriter::new(262_144);
    w.put_bytes(&[0u8; 262_144]);
    assert_eq!(w.status(), Ok(262_144));
    w.put_u8(1);
    assert_eq!(w.status(), Err(SnapshotFull));
    assert_eq!(w.len(), 262_144);
}

#[test]
fn sdk_191_writer_reader_le() {
    let mut w = SnapshotWriter::new(1_024);
    w.section(7);
    w.put_u8(1);
    w.put_u16(0x0203);
    w.put_i64(-2);
    w.put_i128(1 << 100);
    w.put_bytes(&[9, 9]);
    let b = w.as_bytes();
    assert_eq!(b[1..3], [0x03, 0x02]);
    assert_eq!(b[3..11], [0xFE, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
    for (mut r, ver) in [
        (SnapshotReader::with_desc_version(b, 5), 5),
        (SnapshotReader::new(b), 0),
    ] {
        assert_eq!(r.desc_version(), ver);
        assert_eq!(r.get_u8(), Ok(1));
        assert_eq!(r.get_u16(), Ok(0x0203));
        assert_eq!(r.get_i64(), Ok(-2));
        assert_eq!(r.get_i128(), Ok(1 << 100));
        assert_eq!(r.get_bytes(2), Ok(&[9u8, 9][..]));
        assert_eq!(r.get_u8(), Err(RestoreError::Truncated));
    }

    // Engine sections: SectionHdr framing, body padded to 8, located by number.
    let mut w = SnapshotWriter::new(1_024);
    w.begin_section(6, 1);
    w.put_row(&[1u32, 2]);
    w.put_row(&[3u32, 4]);
    w.begin_section(7, 2);
    w.put_u8(5);
    w.add_rows(1);
    w.end_section();
    assert_eq!(w.status(), Ok(16 + 16 + 16 + 8));
    let r = SnapshotReader::new(w.as_bytes());
    let s7 = r.find_section(7).unwrap().unwrap();
    assert_eq!((s7.section, s7.version, s7.count, s7.body.len()), (7, 2, 1, 8));
    assert_eq!(s7.reader().get_u8(), Ok(5));
    let s6 = r.find_section(6).unwrap().unwrap();
    assert_eq!((s6.version, s6.count, s6.body.len()), (1, 2, 16));
    assert!(r.find_section(9).unwrap().is_none());
}

use crate::common;

#[test]
fn sdk_024_max_groups_fit_snapshot_max() {
    use common::*;
    use hft_strategy_sdk::{GroupTable, Strategy};
    let specs: Vec<_> = (1..=8u32)
        .map(|n| gspec(hft_types::GroupId(0x0300_0000 + n)))
        .collect();
    let t = GroupTable::new(&specs, 8);
    assert_eq!((t.len(), t.capacity()), (8, 8));
    let v = clocked_views();
    let mut e = EchoProbe::new(s3(0xFF01), &tp(8, &[]), &t, &v.refview()).unwrap();
    for g in &mut e.groups {
        // Cooldown running, counters non-zero (every field written with a non-default value).
        g.cooldown_until_ens = i64::MAX - 1;
        g.fills = i64::MAX;
        g.timer_hits = u32::MAX;
    }
    let mut tp_ = TargetProbe::new(s3(0xFF02), &tp2(8, &[]), &t, &v.refview()).unwrap();
    for g in &mut tp_.groups {
        g.last_band = i64::MAX;
        g.n_targets = u32::MAX;
        g.n_exec = u32::MAX;
    }
    let mut w = SnapshotWriter::new(262_144);
    e.snapshot(&mut w);
    let n = w.status().expect("EchoProbe snapshot fits");
    assert!(n <= 262_144, "{n}");
    let mut w = SnapshotWriter::new(262_144);
    tp_.snapshot(&mut w);
    let n = w.status().expect("TargetProbe snapshot fits");
    assert!(n <= 262_144, "{n}");
}
