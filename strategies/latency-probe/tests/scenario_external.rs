//! External fills, notices and the residual cap (TESTS.md LPR-050–LPR-053).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::type_complexity
)] // allow-list: test code

mod common;

use common::*;
use hft_event::ExternalCause;
use hft_strategy_sdk::{
    CapScopeView, ExternalFillView, IntentClass, NoticeKind, OrderIntent, OrderRef, RiskEvent,
    VenueNoticeView,
};
use hft_types::{CapSource, GroupId, INST_ALL, InstId, Px, Qty, Side, StrategyId, Tif, Tsc, VenueId};
use latency_probe::Phase;

fn external(h: &mut H, g: GroupId, i: InstId, cause: ExternalCause) -> Rec {
    let gix = h.gix_of(g);
    let f = ExternalFillView {
        venue: V2,
        inst: i,
        group: g,
        leg_ix: 0,
        side: Side::Buy,
        qty: Qty(100_000),
        px: Px(6_000_000),
        fee: 0,
        fee_asset: USDT,
        cause,
        ex_ts_ns: U,
        deadline_tsc: Tsc(0),
    };
    h.ev(&ev::external(gix, f))
}

fn gps_holding(h: &mut H) {
    h.bbo(I2);
    h.timer(GPS, 1);
    let r = h.bbo(I1).refs[0];
    h.order(GPS, r, OPEN_TAG, filled(100_000, 6_030_001, true));
    assert_eq!(h.phase(GPS), Phase::Holding);
}

#[test]
fn lpr_050_external() {
    for row in ["LPR-050.1", "LPR-050.2"] {
        let mut h = H::new(&[], vec![gq()]);
        h.net(GQ, 0, 100_000);
        let rec = external(&mut h, GQ, I2, ExternalCause::Liquidation);
        assert_eq!(rec.places.len(), 1, "{row}");
        let p = rec.places[0];
        assert_eq!(
            (p.inst, p.side, p.tif, p.class),
            (I2, Side::Sell, Tif::Ioc, IntentClass::Flatten),
            "{row}"
        );
        assert_eq!(
            (p.flags, p.qty),
            (OrderIntent::REDUCE_ONLY, Qty(100_000)),
            "{row}"
        );
        h.at(2_000);
        assert_eq!(h.bbo(I2).places.len(), 0, "{row}");
        if row == "LPR-050.2" {
            h.flat(GQ);
            h.at(3_000);
            assert_eq!(h.bbo(I2).places.len(), 1, "{row}");
        }
    }
    for (row, cause) in [
        ("LPR-050.3", ExternalCause::Adl),
        ("LPR-050.4", ExternalCause::Manual),
    ] {
        let mut h = H::new(&[], vec![gps()]);
        gps_holding(&mut h);
        h.net(GPS, 0, 100_000).net(GPS, 1, 0);
        let rec = external(&mut h, GPS, I2, cause);
        assert_eq!(rec.places.len(), 1, "{row}");
        let p = rec.places[0];
        assert_eq!(
            (p.inst, p.side, p.tif, p.class),
            (I1, Side::Sell, Tif::Ioc, IntentClass::Flatten),
            "{row}"
        );
        assert_eq!((p.flags, p.qty), (0, Qty(100_000)), "{row}");
        assert_eq!(h.phase(GPS), Phase::Blocked, "{row}");
    }
}

fn notice(h: &mut H, g: GroupId, venue: VenueId, inst: InstId, kind: NoticeKind) {
    let gix = h.gix_of(g);
    h.ev(&ev::notice(inst, gix, VenueNoticeView { venue, inst, kind }));
}

#[test]
fn lpr_051_notices() {
    let mut h = H::new(&[], vec![gq()]);
    notice(&mut h, GQ, V2, I2, NoticeKind::Delisting { at_ns: U });
    for k in 0..10 {
        h.at(k * 1_000);
        assert_eq!(h.bbo(I2).places.len(), 0, "LPR-051.1");
    }

    let gid = GroupId(0x0500_0041);
    let mut h = H::new(&[], vec![g(gid, &[I2, I3], QUOTE)]);
    notice(&mut h, gid, V2, INST_ALL, NoticeKind::Delisting { at_ns: U });
    assert_eq!(h.bbo(I2).places.len(), 0, "LPR-051.2");
    assert_eq!(h.bbo(I3).places.len(), 1, "LPR-051.2");

    let maint = NoticeKind::Maintenance {
        from_ns: U + 1_000_000_000,
        to_ns: U + 2_000_000_000,
    };
    let rows = [
        ("LPR-051.3", 999_999_999, 1),
        ("LPR-051.4", 1_000_000_000, 0),
        ("LPR-051.5", 2_000_000_000, 0),
        ("LPR-051.6", 2_000_000_001, 1),
    ];
    for (row, d, places) in rows {
        let mut h = H::new(&[], vec![gq()]);
        notice(&mut h, GQ, V2, I2, maint);
        h.at_ens(E0 + d);
        assert_eq!(h.now().utc_ns, U + d, "{row}");
        assert_eq!(h.bbo(I2).places.len(), places, "{row}");
    }

    let mut h = H::new(&[], vec![gps()]);
    h.bbo(I2);
    h.timer(GPS, 1);
    notice(&mut h, GPS, V2, I2, maint);
    h.at(1_500);
    let rec = h.bbo(I1);
    assert_eq!((rec.intents(), h.phase(GPS)), (0, Phase::Due), "LPR-051.7");
}

/// GQ probe (r1) Acked; then the unexpected fill with view `net`.
fn gq_fill(h: &mut H, net: i64) -> (OrderRef, Rec) {
    let r1 = h.bbo(I2).refs[0];
    h.at(50);
    h.order(GQ, r1, probe_tag(0), acked());
    h.at(60);
    h.net(GQ, 0, net);
    let rec = h.order(GQ, r1, probe_tag(0), filled(100_000, 5_820_000, true));
    (r1, rec)
}

#[test]
fn lpr_052_residual_cap() {
    // .1 / .2: quote group, mid 6 000 005, residual 60 000 050.
    for (row, cap, halt) in [("LPR-052.1", 60_000_050, false), ("LPR-052.2", 60_000_049, true)] {
        let mut h = H::new(&[(f::MAX_INV_NOTIONAL, cap)], vec![gq()]);
        let (_, rec) = gq_fill(&mut h, 100_000);
        assert_eq!(rec.notes_of(1).len(), 1, "{row}");
        assert_eq!(rec.places.len(), 1, "{row}");
        assert_eq!(
            (rec.places[0].side, rec.places[0].qty),
            (Side::Sell, Qty(100_000)),
            "{row}"
        );
        if halt {
            assert_eq!(rec.halts, vec![(GQ, 2)], "{row}");
            let n = rec.notes_of(7);
            assert_eq!((n.len(), n[0].v[0], n[0].v[1]), (1, 2, 60_000_050), "{row}");
        } else {
            assert!(rec.halts.is_empty(), "{row}");
        }
    }
    // .3: pair legs offset, residual 0.
    let mut h = H::new(&[(f::MAX_INV_NOTIONAL, 1_000_000)], vec![gps()]);
    gps_holding(&mut h);
    h.net(GPS, 0, 100_000).net(GPS, 1, -100_000);
    let rec = h.timer(GPS, 2);
    assert_eq!((rec.places.len(), rec.halts.len()), (1, 0), "LPR-052.3");
    assert_eq!(rec.places[0].class, IntentClass::Reduce, "LPR-052.3");
    // .4: residual 10 000 × 6 000 000 = 6 USDT.
    let mut h = H::new(&[(f::MAX_INV_NOTIONAL, 5_999_999)], vec![gps()]);
    gps_holding(&mut h);
    h.net(GPS, 0, 100_000).net(GPS, 1, -90_000);
    let rec = h.timer(GPS, 2);
    assert_eq!(rec.halts, vec![(GPS, 2)], "LPR-052.4");
    let n = rec.notes_of(7);
    assert_eq!((n[0].v[0], n[0].v[1]), (2, 6_000_000), "LPR-052.4");
    assert_eq!(rec.places.len(), 1, "LPR-052.4");
    let p = rec.places[0];
    assert_eq!(
        (p.inst, p.side, p.class, p.qty),
        (I1, Side::Sell, IntentClass::Flatten, Qty(100_000)),
        "LPR-052.4"
    );
    assert_eq!(h.phase(GPS), Phase::Blocked, "LPR-052.4");
    // .5: the second leg in flight offsets the first leg's fill.
    let mut h = H::new(&[(f::MAX_INV_NOTIONAL, 50_000_000)], vec![gps()]);
    h.bbo(I2);
    h.timer(GPS, 1);
    let r = h.bbo(I1).refs[0];
    h.net(GPS, 0, 100_000).net(GPS, 1, 0).inflight(GPS, 1, -100_000);
    let rec = h.order(GPS, r, OPEN_TAG, filled(100_000, 6_030_001, true));
    assert!(rec.halts.is_empty(), "LPR-052.5");
    assert_eq!(h.phase(GPS), Phase::Holding, "LPR-052.5");
}

#[test]
fn lpr_053_halt_not_repeated() {
    let mut h = H::new(&[(f::MAX_INV_NOTIONAL, 60_000_049)], vec![gq()]);
    let (r1, rec) = gq_fill(&mut h, 100_000);
    assert_eq!(rec.halts.len(), 1);
    h.at(100);
    h.net(GQ, 0, 200_000);
    let rec = h.order(GQ, r1, probe_tag(0), filled(100_000, 5_820_000, true));
    assert_eq!((rec.places.len(), rec.places[0].qty), (1, Qty(200_000)));
    assert!(rec.halts.is_empty() && rec.notes_of(7).is_empty());
    h.at(200);
    let scope = CapScopeView {
        strategy: StrategyId(1),
        venue: VenueId(0),
        account: T1,
        group: GQ,
        inst: InstId(0),
    };
    h.ev(&ev::risk(
        T1,
        RiskEvent::CapCleared {
            source: CapSource::HALTED,
            scope,
        },
    ));
    h.at(300);
    h.net(GQ, 0, 300_000);
    let rec = h.order(GQ, r1, probe_tag(0), filled(100_000, 5_820_000, true));
    assert_eq!((rec.places.len(), rec.places[0].qty), (1, Qty(300_000)));
    assert_eq!(rec.halts, vec![(GQ, 2)]);
    let n = rec.notes_of(7);
    assert_eq!((n[0].v[0], n[0].v[1]), (2, 180_000_150));
}
