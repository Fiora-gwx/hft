#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
use crate::common;

use common::*;
use hft_oms::*;
use hft_types::{AccountId, Rate, Side, Tif};

#[test]
fn oms_001_defaults() {
    let mut c = OmsConfig::new(EPOCH, HZ);
    // Refdata expiry windows of the fixture: VS 5 000 ms, VX 3 000 ms, VG none.
    c.expire_window_ms.set(VS, Some(5000));
    c.expire_window_ms.set(VX, Some(3000));
    let rd = rd();
    let rows: [(&str, u64, u64); 25] = [
        ("OMS-001.1", u64::from(c.slots.get(VS, A1)), 4096),
        ("OMS-001.2", u64::from(c.links_per_account), 1024),
        ("OMS-001.3", u64::from(c.ack_timeout_ms.get(VS)), 2000),
        ("OMS-001.4", u64::from(c.cancel_timeout_ms.get(VS)), 2000),
        ("OMS-001.5", u64::from(c.ioc_timeout_ms.get(VS)), 1000),
        ("OMS-001.6", u64::from(c.unknown_query_interval_ms), 1000),
        ("OMS-001.7", u64::from(c.unknown_max_queries), 5),
        ("OMS-001.8", u64::from(c.unknown_alarm_ms), 30_000),
        ("OMS-001.9", u64::from(c.out_quota_of(VS, A1)), 64),
        ("OMS-001.10", u64::from(c.not_found_grace_of(VS, rd)), 5500),
        ("OMS-001.11", u64::from(c.not_found_grace_of(VX, rd)), 3500),
        ("OMS-001.12", u64::from(c.not_found_grace_of(VG, rd)), 2000),
        ("OMS-001.13", c.smoke_far_bp.0 as u64, 30_000_000),
        ("OMS-001.14", u64::from(c.reuse_quarantine_ms), 5000),
        ("OMS-001.15", u64::from(c.tombstone_gens), 2),
        ("OMS-001.16", u64::from(c.tombstone_keep_ms), 3_600_000),
        ("OMS-001.17", u64::from(c.sweep_batch), 256),
        ("OMS-001.18", u64::from(c.max_out_per_event), 16),
        ("OMS-001.19", u64::from(c.hedge_retry_max), 3),
        ("OMS-001.20", u64::from(c.hedge_retry_min_ms), 100),
        ("OMS-001.21", u64::from(c.hedge_deadline_ms), 3000),
        ("OMS-001.22", u64::from(c.resume_hold_max_ms), 30_000),
        ("OMS-001.23", u64::from(c.foreign_fill_track.get(VS, A1)), 64),
        ("OMS-001.24", u64::from(c.ext_held_track.get(VS, A1)), 64),
        ("OMS-001.25", u64::from(c.decode_fail_down_n), 3),
    ];
    for (row, got, want) in rows {
        assert_eq!(got, want, "{row}");
    }
    assert_eq!(c.smoke_far_bp, Rate(30_000_000));
}

#[test]
fn oms_002_slots_range() {
    for (row, n, ok) in [
        ("OMS-002.1", 0u32, false),
        ("OMS-002.2", 1, true),
        ("OMS-002.3", 65_534, true),
        ("OMS-002.4", 65_535, true),
        ("OMS-002.5", 65_536, false),
    ] {
        let mut c = p0();
        c.slots.set(VS, A1, n);
        let r = Oms::new(&c, rd());
        if !ok {
            assert_eq!(
                r.err(),
                Some(OmsConfigError::SlotsOutOfRange {
                    venue: VS,
                    account: A1
                }),
                "{row}"
            );
            continue;
        }
        let mut h = H::from_cfg(&c, rd());
        if n == 1 {
            let s = h.place_ok(&buy_x(), 0);
            assert_eq!(s.slot, 1, "{row}");
            assert_eq!(h.place(&buy_x(), 0), Err(PlaceError::NoFreeSlot), "{row}");
        }
        if n == 65_535 {
            let it = intent(
                VS,
                A1,
                X,
                Side::Buy,
                Tif::Gtc,
                p("60000.00"),
                q("0.01"),
                hft_types::GROUP_NONE,
            );
            let mut last = 0;
            for _ in 0..65_535u32 {
                last = h.place_ok(&it, 0).slot;
            }
            assert_eq!(last, 65_535, "{row}");
        }
    }
}

#[test]
fn oms_003_table_count() {
    // (venues, accounts, ok). Venue ids are a fixed table, so the grid varies the account count.
    for (row, nv, na, ok) in [
        ("OMS-003.1", 3u8, 85u8, true),
        ("OMS-003.2", 2, 128, true),
        ("OMS-003.3", 2, 129, false),
    ] {
        let rd = build_rd(&RdOpts {
            n_venues: nv,
            n_accounts: na,
            ..RdOpts::default()
        });
        let mut c = p0();
        c.slots = PerTable::new(1);
        let r = Oms::new(&c, rd);
        if ok {
            let o = r.unwrap();
            if na == 128 {
                let tv = o.table_view(VX, AccountId(128)).unwrap();
                assert_eq!(tv.table_ix, 255, "{row}");
            }
        } else {
            assert_eq!(r.err(), Some(OmsConfigError::TooManyTables), "{row}");
        }
    }
}

#[test]
fn oms_004_tombstone_gens_range() {
    for (row, t, ok) in [
        ("OMS-004.1", 0u8, false),
        ("OMS-004.2", 1, true),
        ("OMS-004.3", 4, true),
        ("OMS-004.4", 5, false),
    ] {
        let mut c = p0();
        c.tombstone_gens = t;
        let r = Oms::new(&c, rd());
        if ok {
            assert!(r.is_ok(), "{row}");
        } else {
            assert_eq!(r.err(), Some(OmsConfigError::TombGens), "{row}");
        }
    }
}
