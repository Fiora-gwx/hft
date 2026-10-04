//! Snapshot writer and reader (SDK-025, SDK-191).

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
