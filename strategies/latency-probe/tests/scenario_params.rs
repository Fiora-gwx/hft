//! Parameter changes and restore of pair/target groups (TESTS.md LPR-054, LPR-055).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::type_complexity
)] // allow-list: test code

mod common;

use common::*;
use hft_strategy_sdk::{ExecUpdateKind, IntentClass, OrderIntent};
use hft_types::{GroupId, Qty, Side};
use latency_probe::Phase;

#[test]
fn lpr_054_on_params_reset() {
    for (row, new_id, place) in [("LPR-054.1", false, 0), ("LPR-054.2", true, 1)] {
        let mut h = H::new(&[], vec![gq()]);
        let r = h.bbo(I2).refs[0];
        h.at(50);
        h.order(GQ, r, probe_tag(0), acked());
        h.at(80);
        h.order(GQ, r, probe_tag(0), cancelled());
        let grp = if new_id {
            g(GroupId(0x0500_0051), &[I2], QUOTE)
        } else {
            gq()
        };
        h.params(&[], vec![grp]);
        h.at(500);
        assert_eq!(h.bbo(I2).places.len(), place, "{row}");
    }
    // .3 GPS Holding; mode → target: flatten both legs, then Idle.
    let mut h = H::new(&[], vec![gps()]);
    h.bbo(I2);
    h.timer(GPS, 1);
    let r = h.bbo(I1).refs[0];
    h.order(GPS, r, OPEN_TAG, filled(100_000, 6_030_001, true));
    h.net(GPS, 0, 100_000).net(GPS, 1, -100_000);
    let mut grp = gps();
    grp.mode = TARGET;
    h.params(&[], vec![grp]);
    h.at(600);
    let rec = h.bbo(I2);
    let got: Vec<_> = rec
        .places
        .iter()
        .map(|p| (p.inst, p.side, p.qty, p.class, p.flags))
        .collect();
    assert_eq!(
        got,
        vec![
            (I1, Side::Sell, Qty(100_000), IntentClass::Flatten, 0),
            (
                I2,
                Side::Buy,
                Qty(100_000),
                IntentClass::Flatten,
                OrderIntent::REDUCE_ONLY
            )
        ],
        "LPR-054.3"
    );
    assert_eq!(h.phase(GPS), Phase::Idle, "LPR-054.3");
    assert_eq!(rec.timers.iter().filter(|t| t.tag == 1).count(), 1, "LPR-054.3");
}

#[test]
fn lpr_055_restore_pair_target() {
    // (row, group, phase to reach, nets after restore, expected phase, clears, FLAT registered)
    let rows: Vec<(&str, G, &str, (i64, i64), Phase, usize, bool)> = vec![
        ("LPR-055.1", gps(), "due", (0, 0), Phase::Idle, 0, false),
        ("LPR-055.2", gps(), "opening", (0, 0), Phase::Idle, 0, false),
        (
            "LPR-055.3",
            gps(),
            "holding",
            (100_000, -100_000),
            Phase::Holding,
            0,
            true,
        ),
        (
            "LPR-055.4",
            gps(),
            "blocked",
            (100_000, 0),
            Phase::Blocked,
            0,
            false,
        ),
        (
            "LPR-055.5",
            gts(),
            "flattening",
            (100_000, -100_000),
            Phase::Holding,
            1,
            true,
        ),
        ("LPR-055.6", gts(), "opening", (0, 0), Phase::Idle, 1, false),
    ];
    for (row, grp, reach, (na, nb), want, clears, flat) in rows {
        let gid = grp.spec.id;
        let target = grp.mode == TARGET;
        let mut h = H::new(&[], vec![grp]);
        h.bbo(I2);
        h.timer(gid, 1);
        if reach != "due" {
            let rec = h.bbo(I1);
            assert_eq!(rec.places.len() + rec.targets.len(), 1, "{row}");
            if reach != "opening" {
                if target {
                    h.at(10);
                    h.net(gid, 0, 100_000).net(gid, 1, -100_000);
                    h.exec(
                        gid,
                        ExecUpdateKind::Done {
                            cleared_by_engine: false,
                        },
                    );
                } else {
                    h.order(gid, rec.refs[0], OPEN_TAG, filled(100_000, 6_030_001, true));
                }
                assert_eq!(h.phase(gid), Phase::Holding, "{row}");
            }
            if reach == "blocked" {
                h.ev(&ev::risk(
                    T1,
                    hft_strategy_sdk::RiskEvent::HedgeBroken { group: gid },
                ));
            }
            if reach == "flattening" {
                h.net(gid, 0, 100_000).net(gid, 1, -100_000);
                h.at(6_000);
                h.timer(gid, 2);
                assert_eq!(h.phase(gid), Phase::Flattening, "{row}");
            }
        }
        h.restart(7, 2);
        h.flat(gid);
        h.net(gid, 0, na).net(gid, 1, nb);
        h.at(8_000);
        let rec = h.bbo(I2);
        assert_eq!(h.phase(gid), want, "{row}");
        assert_eq!(rec.clears.len(), clears, "{row}");
        let flats: Vec<_> = rec.timers.iter().filter(|t| t.tag == 2).collect();
        assert_eq!(!flats.is_empty(), flat, "{row}");
        if flat {
            assert_eq!(flats[0].at, h.tsc_of(ms(8_000) + 5_000_000_000), "{row}");
        }
        if row == "LPR-055.1" {
            assert_eq!(rec.timers.iter().filter(|t| t.tag == 1).count(), 1, "{row}");
        }
        if row == "LPR-055.4" {
            assert_eq!(rec.intents(), 0, "{row}");
        }
    }
}
