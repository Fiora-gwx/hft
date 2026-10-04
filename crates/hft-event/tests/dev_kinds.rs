//! Test kinds with feature `dev-kinds` (EVT-008). Built only with that feature (Cargo.toml).

use hft_event::*;

#[test]
fn evt_008_dev_kinds_registered() {
    let all = JournalMask::SEQ | JournalMask::RAW | JournalMask::CHAIN;
    let rows: [(&str, u16, &str, Layout, Role, CrcPolicy); 6] = [
        (
            "EVT-008.1",
            0x7F01,
            "TIn",
            Layout::Fixed { size: 80 },
            Role::Input,
            CrcPolicy::Never,
        ),
        (
            "EVT-008.2",
            0x7F02,
            "TOut",
            Layout::Fixed { size: 56 },
            Role::Output,
            CrcPolicy::Never,
        ),
        (
            "EVT-008.3",
            0x7F03,
            "TMeta",
            Layout::Fixed { size: 32 },
            Role::Meta,
            CrcPolicy::Never,
        ),
        (
            "EVT-008.4",
            0x7F04,
            "TCrc",
            Layout::Fixed { size: 40 },
            Role::Input,
            CrcPolicy::Always,
        ),
        (
            "EVT-008.5",
            0x7F05,
            "TMulti",
            Layout::Multi { prefix: 0 },
            Role::Input,
            CrcPolicy::Never,
        ),
        (
            "EVT-008.6",
            0x7F06,
            "TVar",
            Layout::Var {
                prefix: 16,
                max: 65_000,
            },
            Role::Input,
            CrcPolicy::Never,
        ),
    ];
    for (row, k, name, layout, role, crc) in rows {
        let e = kind_info(EventKind(k)).unwrap_or_else(|| panic!("{row}"));
        assert_eq!(
            (e.name, e.layout, e.role, e.crc),
            (name, layout, role, crc),
            "{row}"
        );
        assert_eq!(e.domain, Domain::Test, "{row}");
        assert_eq!(e.journals, all, "{row}");
        assert_eq!(e.schema_version, 1, "{row}");
        assert!(!e.optional, "{row}");
    }
    assert_eq!(record_len(40, true), 72, "EVT-008.4");
    assert!(kind_info(EventKind(0x7F07)).is_none(), "EVT-008.7");
    assert!(kind_info(EventKind(0x7F00)).is_none(), "EVT-008.7");
    // EVT-008.8: the gen-schema export never contains test kinds.
    let cat = schema::catalog();
    assert_eq!(cat.kinds.len(), 39);
    assert!(cat.kinds.iter().all(|k| k.kind.0 >> 8 != 0x7F), "EVT-008.8");
    // The dev bodies round-trip like any other kind.
    let b = dev::TCrc { bytes: [7; 40] };
    let mut out = [0u8; 72];
    let n = encode_fixed(&mut out, 1, 2, RecordFlags::CRC, &b);
    assert_eq!(n, 72);
    let r = decode_record(&out).unwrap();
    assert_eq!(r.read::<dev::TCrc>().unwrap(), b);
}
