//! Cold component/field diagnosis must agree with the independent HashTrail reader, and
//! must not invent row names for unknown or damaged layouts.

use hft_event::{CapRecord, SectionHdr, SnapshotHdr};
use hft_sequencer::driver::snapshot_diff::{InvalidSnapshot, Side, snapshot_difference};
use hft_strategy_sdk::{SnapshotReader, SnapshotWriter};
use hft_testkit::hash::component_hashes;
use zerocopy::{FromZeros, IntoBytes};

const SECTIONS: [u16; 25] = [
    1, 13, 22, 2, 3, 12, 4, 14, 17, 18, 23, 5, 8, 9, 10, 15, 16, 19, 20, 24, 25, 26, 6, 7, 21,
];

fn snapshot(sections: &[u16]) -> Vec<u8> {
    let mut w = SnapshotWriter::new(16 * 1024);
    for &section in sections {
        w.begin_section(section, 1);
        if section == 4 {
            for seq in [100, 200, 300] {
                let mut row = CapRecord::new_zeroed();
                row.set_seq = seq;
                w.put_row(&row);
            }
        } else {
            w.put_u64(u64::from(section));
            w.add_rows(1);
        }
    }
    w.end_section();
    w.status().unwrap();
    let mut h = SnapshotHdr::new_zeroed();
    h.purpose = hft_sequencer::snapshot::PURPOSE_KEYFRAME;
    h.n_sections = sections.len() as u32;
    let mut out = h.as_bytes().to_vec();
    out.extend_from_slice(w.as_bytes());
    out
}

fn section(body: &[u8], id: u16) -> usize {
    let mut r = SnapshotReader::new(&body[96..]);
    loop {
        let at = 96 + r.pos();
        if r.next_section().unwrap().unwrap().section == id {
            return at;
        }
    }
}

fn hash_values(bytes: &[u8]) -> [u64; 6] {
    let h = component_hashes(bytes).unwrap();
    [h.ledger, h.oms, h.risk, h.strategies, h.book, h.exec]
}

#[test]
fn seq_404_snapshot_components_match_independent_hashtrail() {
    let original = snapshot(&SECTIONS);
    assert!(snapshot_difference(&original, &original).unwrap().first.is_none());
    let rows = [
        (1, "hft-ledger"),
        (13, "hft-ledger"),
        (22, "hft-ledger"),
        (2, "hft-oms"),
        (3, "hft-oms"),
        (12, "hft-oms"),
        (4, "hft-risk"),
        (14, "hft-risk"),
        (17, "hft-risk"),
        (18, "hft-risk"),
        (23, "hft-risk"),
        (5, "strategies"),
        (6, "hft-book"),
        (7, "hft-book"),
        (21, "hft-exec"),
        (8, "hft-sequencer"),
        (25, "hft-sequencer"),
    ];
    for (id, owner) in rows {
        let at = section(&original, id) + 16;
        let mut changed = original.clone();
        changed[at] ^= 1;
        let d = snapshot_difference(&changed, &original).unwrap();
        let a = hash_values(&changed);
        let b = hash_values(&original);
        for (i, hash) in d.components.iter().enumerate() {
            assert_eq!((hash.logged, hash.regenerated), (a[i], b[i]), "section {id}");
        }
        assert_ne!(d.logged_state_hash, d.regenerated_state_hash);
        let first = d.first.unwrap();
        assert_eq!(first.component, Some(owner));
        assert_eq!(first.section, Some(id));
        assert_eq!(first.logged_offset, Some(at));
        assert_eq!(first.regenerated_offset, Some(at));
        assert_ne!(first.logged, first.regenerated);
    }
    // Priority follows component order, not section number or the first mutated byte.
    let mut two = original.clone();
    two[section(&original, 23) + 16] ^= 1;
    two[section(&original, 22) + 16] ^= 1;
    assert_eq!(
        snapshot_difference(&two, &original)
            .unwrap()
            .first
            .unwrap()
            .component,
        Some("hft-ledger")
    );
    let mut missing = SECTIONS.to_vec();
    missing.retain(|s| *s != 21);
    let short = snapshot(&missing);
    for (a, b) in [(&original, &short), (&short, &original)] {
        let d = snapshot_difference(a, b).unwrap().first.unwrap();
        assert_eq!(d.component, Some("hft-exec"));
        assert_eq!(d.field, "sections[21].presence");
        assert_eq!(d.layout, "missing_section");
        assert_eq!(d.logged.is_empty(), core::ptr::eq(a, &short));
    }
}

#[test]
fn seq_405_snapshot_fields_versions_and_metadata_are_distinguished() {
    let original = snapshot(&SECTIONS);
    let start = section(&original, 4);
    // Literal offsets/names are the fixture oracle; do not derive expectations from the
    // production field lookup that is being tested. Exercise a non-first row and nested scope.
    for (offset, width, field) in [
        (0, 1, "source"),
        (1, 1, "level"),
        (2, 1, "cflags"),
        (3, 5, "_pad"),
        (10, 1, "scope.account"),
        (12, 4, "scope.group"),
        (16, 4, "scope.inst"),
        (24, 8, "set_seq"),
        (32, 16, "params"),
    ] {
        let at = start + 16 + 48 + offset;
        let mut changed = original.clone();
        changed[at] ^= 1;
        let d = snapshot_difference(&changed, &original).unwrap().first.unwrap();
        assert_eq!(d.component, Some("hft-risk"));
        assert_eq!(d.row, Some(1));
        assert_eq!(d.field, format!("sections[4].rows[1].{field}"));
        assert_eq!(d.layout, "field");
        assert_eq!(d.logged_offset, Some(at));
        assert_eq!(d.logged, &changed[at..at + width]);
        assert_eq!(d.regenerated, &original[at..at + width]);
    }
    for (header_offset, value, layout) in [
        (2, 99u16.to_le_bytes().to_vec(), "unknown_row_version"),
        (4, u32::MAX.to_le_bytes().to_vec(), "invalid_row_layout"),
    ] {
        let mut base = original.clone();
        base[start + header_offset..start + header_offset + value.len()].copy_from_slice(&value);
        let mut changed = base.clone();
        changed[start + 16 + 24] ^= 1;
        let d = snapshot_difference(&changed, &base).unwrap().first.unwrap();
        assert_eq!(d.layout, layout);
        assert_eq!(d.field, "sections[4].body[24]");
        assert_eq!(d.row, None);
        assert_eq!(d.logged.len(), 1);
    }
    let mut version = original.clone();
    version[start + 2] ^= 1;
    assert_eq!(
        snapshot_difference(&version, &original)
            .unwrap()
            .first
            .unwrap()
            .field,
        "sections[4].version"
    );
    let mut clock = original.clone();
    clock[section(&original, 16) + 16] ^= 1;
    let d = snapshot_difference(&clock, &original).unwrap();
    assert_eq!(d.logged_state_hash, d.regenerated_state_hash);
    assert_eq!(d.first.unwrap().component, None);
    clock[section(&original, 25) + 16] ^= 1;
    let d = snapshot_difference(&clock, &original).unwrap().first.unwrap();
    assert_eq!((d.component, d.section), (Some("hft-sequencer"), Some(25)));
    let mut header = original.clone();
    header[16] ^= 1;
    let d = snapshot_difference(&header, &original).unwrap().first.unwrap();
    assert_eq!(
        (d.component, d.section, d.field.as_str()),
        (None, None, "state_hash")
    );
    let mut order = SECTIONS;
    order.rotate_left(3);
    let reordered = snapshot(&order);
    let d = snapshot_difference(&reordered, &original).unwrap().first.unwrap();
    assert_eq!(d.component, Some("hft-sequencer"));
    assert!(d.field.ends_with(".section"));
    let unknown = snapshot(&[u16::MAX]);
    let mut changed = unknown.clone();
    changed[112] ^= 1;
    assert_eq!(
        snapshot_difference(&changed, &unknown)
            .unwrap()
            .first
            .unwrap()
            .component,
        None
    );
}

#[test]
fn seq_406_snapshot_diagnostics_reject_malformed_framing() {
    let original = snapshot(&SECTIONS);
    let start = section(&original, 4);
    let mut rows = vec![(vec![0; 95], 0, InvalidSnapshot::Header)];
    let mut count = original.clone();
    count[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    rows.push((count, 4, InvalidSnapshot::SectionCount));
    for (offset, value) in [(8, 1u32), (8, u32::MAX), (12, 1)] {
        let mut damaged = original.clone();
        damaged[start + offset..start + offset + 4].copy_from_slice(&value.to_le_bytes());
        rows.push((damaged, start, InvalidSnapshot::SectionFraming));
    }
    let mut duplicate = original.clone();
    duplicate[start..start + 2].copy_from_slice(&1u16.to_le_bytes());
    rows.push((duplicate, start, InvalidSnapshot::DuplicateSection));
    let mut short = original.clone();
    short.pop();
    rows.push((short, section(&original, 21), InvalidSnapshot::SectionFraming));
    for (bytes, offset, reason) in rows {
        for (a, b, side) in [
            (&bytes, &original, Side::Logged),
            (&original, &bytes, Side::Regenerated),
        ] {
            let err = snapshot_difference(a, b).unwrap_err();
            assert_eq!((err.side, err.offset, err.reason), (side, offset, reason));
        }
    }
    assert_eq!(size_of::<SectionHdr>(), 16);
}
