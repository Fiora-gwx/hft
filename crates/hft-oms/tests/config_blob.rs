use crate::common::*;
use hft_oms::{OmsConfig, OmsConfigRow};
use hft_refdata::{EcfBuilder, EngineConfig, EngineSection};
use hft_types::{HostClass, Rate, VenueId};
use zerocopy::IntoBytes;

#[test]
fn oms_465_config_section_roundtrip() {
    let mut c = OmsConfig::new(EPOCH, HZ);
    c.slots.default = 61;
    c.slots.set(VS, A1, 53);
    c.links_per_account = 39;
    c.max_groups = 81;
    c.ack_timeout_ms.default = 4123;
    c.ack_timeout_ms.set(VS, 789);
    c.cancel_timeout_ms.default = 4567;
    c.cancel_timeout_ms.set(VX, 876);
    c.ioc_timeout_ms.default = 987;
    c.ioc_timeout_ms.set(VS, 123);
    c.unknown_query_interval_ms = 1234;
    c.unknown_max_queries = 7;
    c.unknown_alarm_ms = 45678;
    c.out_quota.default = Some(49);
    c.out_quota.set(VS, A1, None);
    c.not_found_grace_ms.default = Some(8000);
    c.not_found_grace_ms.set(VX, None);
    c.expire_window_ms.default = None;
    c.expire_window_ms.set(VS, Some(3999));
    c.smoke_far_bp = Rate(31_000_001);
    c.reuse_quarantine_ms = 4321;
    c.tombstone_gens = 3;
    c.tombstone_keep_ms = 3_222_000;
    c.sweep_batch = 255;
    c.max_out_per_event = 13;
    c.hedge_retry_max = 6;
    c.hedge_retry_min_ms = 412;
    c.hedge_deadline_ms = 8123;
    c.resume_hold_max_ms = 121_000;
    c.foreign_fill_track.default = 71;
    c.foreign_fill_track.set(VS, A1, 75);
    c.ext_held_track.default = 89;
    c.ext_held_track.set(VS, A1, 94);
    c.decode_fail_down_n = 5;
    let rows = c.to_rows();
    assert_eq!(OmsConfig::from_rows(&rows, EPOCH, HZ), Some(c.clone()));
    let mut b = EcfBuilder::new(1, HostClass::Dev2);
    b.section(
        EngineSection::Oms,
        1,
        16,
        rows.len() as u32,
        rows.as_slice().as_bytes().to_vec(),
    );
    let ec = EngineConfig::load(&b.finish(), None).unwrap();
    assert_eq!(OmsConfig::from_engine(&ec, rd(), EPOCH, HZ).unwrap(), c);
    // Override order does not affect encoding, but repeated keys keep the effective last value.
    c.slots.set(VS, A1, 73);
    assert_eq!(
        OmsConfig::from_rows(&c.to_rows(), EPOCH, HZ)
            .unwrap()
            .slots
            .get(VS, A1),
        73
    );
}

#[test]
fn oms_466_config_section_rejects_incomplete() {
    let rows = OmsConfig::new(EPOCH, HZ).to_rows();
    for i in 0..rows.len() {
        let mut bad = rows.clone();
        bad.remove(i);
        assert!(
            OmsConfig::from_rows(&bad, EPOCH, HZ).is_none(),
            "missing field {i}"
        );
    }
    let cases: Vec<OmsConfigRow> = [
        (1, 0, 0, 1, 0),
        (1, 0, 1, 0, 2),
        (1, 64, 1, 0, 2),
        (2, 1, 0, 0, 2),
        (4, 1, 1, 0, 2),
        (8, 0, 0, 0, 256),
        (10, 0, 0, 0, -2),
        (17, 0, 0, 0, 65_536),
        (26, 0, 0, 0, 0),
    ]
    .map(|(field, venue, account, reserved, value)| OmsConfigRow {
        field,
        venue,
        account,
        reserved,
        value,
    })
    .into();
    for row in cases {
        let mut bad = rows.clone();
        bad.retain(|r| r.field != row.field);
        bad.push(row);
        bad.sort_by_key(|r| (r.field, r.venue, r.account));
        assert!(OmsConfig::from_rows(&bad, EPOCH, HZ).is_none(), "{row:?}");
    }
    let mut duplicate = rows.clone();
    duplicate.insert(0, rows[0]);
    assert!(OmsConfig::from_rows(&duplicate, EPOCH, HZ).is_none());
    let mut c = OmsConfig::new(EPOCH, HZ);
    c.ack_timeout_ms.set(VenueId(63), 123);
    let rows = c.to_rows();
    let mut b = EcfBuilder::new(1, HostClass::Dev2);
    b.section(
        EngineSection::Oms,
        1,
        16,
        rows.len() as u32,
        rows.as_slice().as_bytes().to_vec(),
    );
    assert!(
        OmsConfig::from_engine(&EngineConfig::load(&b.finish(), None).unwrap(), rd(), EPOCH, HZ).is_err()
    );
}

#[test]
fn oms_484_config_section_stride_must_be_whole_rows() {
    let rows = OmsConfig::new(EPOCH, HZ).to_rows();
    let bytes = rows.as_slice().as_bytes().to_vec();
    let n = rows.len() as u32;
    // .1 a whole number of 16-byte rows loads; .2/.3 other strides over the same bytes do not.
    for (row, size, count, ok) in [
        ("OMS-484.1", 16u32, n, true),
        ("OMS-484.2", 8, n * 2, false),
        ("OMS-484.3", 32, n / 2, false),
    ] {
        let mut b = EcfBuilder::new(1, HostClass::Dev2);
        b.section(
            EngineSection::Oms,
            1,
            size,
            count,
            bytes[..(size * count) as usize].to_vec(),
        );
        // The container may already refuse a short stride; either layer must refuse it.
        let loaded = EngineConfig::load(&b.finish(), None)
            .ok()
            .is_some_and(|ec| OmsConfig::from_engine(&ec, rd(), EPOCH, HZ).is_ok());
        assert_eq!(loaded, ok, "{row}");
    }
}
