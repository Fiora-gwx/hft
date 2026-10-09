//! pair probes and the phase table (TESTS.md LPR-033–LPR-040).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::type_complexity
)] // allow-list: test code

mod common;

use common::*;
use hft_strategy_sdk::{
    BrokenReason, CapScopeView, ExecUpdateKind, HedgeKind, IntentClass, IntentOutcome, LinkRole, MarginLevel,
    NoticeKind, OnFail, OrderIntent, OrderRef, PxMode, ResidualPolicy, RiskEvent, RiskStateView,
    VenueNoticeView,
};
use hft_types::{
    CapSource, GroupId, InstId, Px, Qty, Rate, RejectBits, RunMode, Side, StrategyId, Tif, VenueId,
};
use latency_probe::Phase;
use latency_probe::fast::G_TICK;

/// A and B insts of a pair group.
fn ab(h: &H, g: GroupId) -> (InstId, InstId) {
    let s = h.table.get(h.gix_of(g));
    (s.legs[0].inst, s.legs[1].inst)
}

/// Starts the group at the current time with a B `Bbo` (Idle, PAIR_TICK registered).
fn idle(h: &mut H, g: GroupId) -> hft_strategy_sdk::ctx::TimerReq {
    let (_, b) = ab(h, g);
    let rec = h.bbo(b);
    assert_eq!(h.phase(g), Phase::Idle);
    rec.timers[0]
}

fn due(h: &mut H, g: GroupId) {
    idle(h, g);
    let rec = h.timer(g, 1);
    assert_eq!(rec.intents(), 0);
    assert_eq!(h.phase(g), Phase::Due);
}

/// Due → A `Bbo`: the first leg (r1).
fn opening(h: &mut H, g: GroupId) -> (OrderIntent, OrderRef) {
    due(h, g);
    let (a, _) = ab(h, g);
    let rec = h.bbo(a);
    assert_eq!(rec.places.len(), 1);
    assert_eq!(h.phase(g), Phase::Opening);
    (rec.places[0], rec.refs[0])
}

fn holding(h: &mut H, g: GroupId) {
    let (_, r1) = opening(h, g);
    h.order(g, r1, OPEN_TAG, filled(100_000, 6_030_001, true));
    assert_eq!(h.phase(g), Phase::Holding);
}

/// Holding with A +100 000 / B −100 000, then FLAT: the closing link (r2) and Flattening(1).
fn flattening1(h: &mut H, g: GroupId) -> OrderRef {
    holding(h, g);
    h.net(g, 0, 100_000).net(g, 1, -100_000);
    let rec = h.timer(g, 2);
    assert_eq!(rec.places.len(), 1);
    assert_eq!((h.phase(g), h.group(g).attempt), (Phase::Flattening, 1));
    rec.refs[0]
}

fn flattening2(h: &mut H, g: GroupId) {
    flattening1(h, g);
    let rec = h.timer(g, 2);
    assert_eq!(rec.places.len(), 1);
    assert_eq!((h.phase(g), h.group(g).attempt), (Phase::Flattening, 2));
}

fn blocked(h: &mut H, g: GroupId) {
    holding(h, g);
    h.ev(&ev::risk(T1, RiskEvent::HedgeBroken { group: g }));
    assert_eq!(h.phase(g), Phase::Blocked);
}

fn pair_h(groups: Vec<G>) -> H {
    H::new(&[], groups)
}

#[test]
fn lpr_033_tick_due_first_leg() {
    let mut h = pair_h(vec![gps()]);
    let rec = h.bbo(I2);
    assert!(rec.places.is_empty());
    assert_eq!(rec.timers.len(), 1);
    assert_eq!(
        (rec.timers[0].at, rec.timers[0].tag, rec.timers[0].group),
        (h.tsc_of(E0), 1, GPS)
    );
    h.at(1);
    let rec = h.timer(GPS, 1);
    assert_eq!(rec.intents(), 0);
    assert_eq!(h.phase(GPS), Phase::Due);
    h.at(2);
    let rec = h.bbo(I1);
    assert_eq!(rec.places.len(), 1);
    let p = rec.places[0];
    assert_eq!(
        (p.venue, p.side, p.tif, p.class, p.flags),
        (V1, Side::Buy, Tif::Ioc, IntentClass::Open, 0)
    );
    assert_eq!(
        (p.inst, p.group, p.px, p.qty, p.max_rest_ms),
        (I1, GPS, Px(6_030_001), Qty(100_000), 0)
    );
    assert_eq!(p.link.some, 1);
    let l = p.link.l;
    assert_eq!((l.group, l.role, l.tolerance), (GPS, LinkRole::First, Rate(0)));
    let hp = l.hedge;
    assert_eq!(
        (hp.kind, hp.px_mode, hp.on_fail, hp.tif),
        (HedgeKind::OnFill, PxMode::Cross, OnFail::Retry, Tif::Ioc)
    );
    assert_eq!(
        (hp.venue, hp.account, hp.side, hp.reduce_only, hp.inst),
        (V2, T1, Side::Sell, 0, I2)
    );
    assert_eq!(
        (hp.ratio_num, hp.ratio_den, hp.residual),
        (1, 1, ResidualPolicy::TrimExcess)
    );
    assert_eq!(
        (hp.px_param, hp.max_slip, hp.limit_px),
        (Rate(5_000_000), Rate(5_000_000), Px(0))
    );
    h.at(3);
    let rec = h.order(GPS, rec.refs[0], OPEN_TAG, cancelled());
    assert_eq!(rec.timers.len(), 1);
    assert_eq!(
        (rec.timers[0].at, rec.timers[0].tag),
        (h.tsc_of(E0 + 10_002_000_000), 1)
    );
    assert_eq!(h.group(GPS).nofill, 1);
}

#[test]
fn lpr_034_open_direction() {
    let rows: Vec<(&str, G, i64, usize, Vec<(Side, i64)>, Side)> = vec![
        ("LPR-034.1", gps(), 0, 1, vec![(Side::Buy, 6_030_001)], Side::Sell),
        ("LPR-034.2", gsp(), 0, 1, vec![(Side::Sell, 5_970_000)], Side::Buy),
        (
            "LPR-034.3",
            gpp(),
            0,
            3,
            vec![
                (Side::Buy, 6_030_010),
                (Side::Sell, 5_970_000),
                (Side::Buy, 6_030_010),
            ],
            Side::Sell,
        ),
        (
            "LPR-034.4",
            gpp(),
            1,
            2,
            vec![(Side::Buy, 0), (Side::Buy, 0)],
            Side::Sell,
        ),
        (
            "LPR-034.5",
            gpp(),
            2,
            2,
            vec![(Side::Sell, 0), (Side::Sell, 0)],
            Side::Buy,
        ),
        ("LPR-034.6", gps(), 1, 1, vec![(Side::Buy, 6_030_001)], Side::Sell),
    ];
    for (row, grp, side, n, want, hedge0) in rows {
        let gid = grp.spec.id;
        let mut h = H::new(&[], vec![grp.with(f::PAIR_SIDE, side)]);
        let (a, b) = ab(&h, gid);
        h.bbo(b);
        let mut got = Vec::new();
        for k in 0..n as i64 {
            let t = k * 10_000;
            h.at(t);
            h.timer(gid, 1);
            let rec = h.bbo(a);
            assert_eq!(rec.places.len(), 1, "{row} opening {k}");
            let p = rec.places[0];
            got.push((p.side, p.px.0));
            if k == 0 {
                assert_eq!(p.link.l.hedge.side, hedge0, "{row}");
            }
            h.order(gid, rec.refs[0], OPEN_TAG, filled(100_000, p.px.0, true));
            h.at(t + 5_000);
            h.timer(gid, 2);
            assert_eq!(h.phase(gid), Phase::Idle, "{row}");
        }
        for (k, ((s, px), (ws, wpx))) in got.iter().zip(&want).enumerate() {
            assert_eq!(*s, *ws, "{row} opening {k}");
            if *wpx != 0 {
                assert_eq!(*px, *wpx, "{row} opening {k}");
            }
        }
    }
}

#[test]
fn lpr_035_pair_qty() {
    for (row, mn, q) in [
        ("LPR-035.1", 5_000_000, 100_000),
        ("LPR-035.2", 54_545_454, 100_000),
        ("LPR-035.3", 60_000_000, 200_000),
    ] {
        let mut h = H::with_rd(rl1_with(mn, false), &[], vec![gps()]);
        due(&mut h, GPS);
        let rec = h.bbo(I1);
        assert_eq!(rec.places[0].qty, Qty(q), "{row}");
    }
}

fn halted_cap(h: &mut H) {
    let scope = CapScopeView {
        strategy: StrategyId(1),
        venue: VenueId(0),
        account: T1,
        group: GPS,
        inst: InstId(0),
    };
    h.ev(&ev::risk(
        T1,
        RiskEvent::CapSet {
            source: CapSource::HALTED,
            scope,
        },
    ));
}

#[test]
fn lpr_036_due_conditions() {
    type Setup = fn(&mut H);
    let rows: Vec<(&str, Setup, usize, Option<Phase>)> = vec![
        ("LPR-036.1", |_| {}, 1, Some(Phase::Opening)),
        (
            "LPR-036.2",
            |h| {
                h.healthy = false;
                let t = h.tsc_of(h.now().engine_ns - 7_000_000_000);
                hft_strategy_sdk::testing::MockViews::conn(&mut h.v.m, I2, t, true, false);
            },
            0,
            Some(Phase::Due),
        ),
        (
            "LPR-036.3",
            |h| {
                h.quote(I1, 6_000_000, 6_100_000);
            },
            0,
            Some(Phase::Due),
        ),
        (
            "LPR-036.4",
            |h| {
                h.net(GPS, 0, 1_000);
            },
            0,
            Some(Phase::Due),
        ),
        (
            "LPR-036.5",
            |h| {
                h.net(GPS, 0, 999).net(GPS, 1, -99_999);
            },
            1,
            Some(Phase::Opening),
        ),
        ("LPR-036.6", |h| halted_cap(h), 0, Some(Phase::Due)),
        (
            "LPR-036.7",
            |h| {
                let r = RiskStateView {
                    margin_level: MarginLevel::L1,
                    conn_liveness_ms: 2_000,
                    ..RiskStateView::EMPTY
                };
                hft_strategy_sdk::testing::MockViews::risk(&mut h.v.m, T1, r);
            },
            0,
            Some(Phase::Due),
        ),
        (
            "LPR-036.8",
            |h| {
                let n = VenueNoticeView {
                    venue: V2,
                    inst: I2,
                    kind: NoticeKind::Delisting { at_ns: U },
                };
                let gix = h.gix_of(GPS);
                h.ev(&ev::notice(I2, gix, n));
            },
            0,
            Some(Phase::Due),
        ),
        (
            "LPR-036.9",
            |h| {
                let groups = h.groups.clone();
                h.params(&[(f::ENABLED, 0)], groups);
            },
            0,
            Some(Phase::Due),
        ),
        (
            "LPR-036.10",
            |h| {
                h.mode(GPS, RunMode::CloseOnly);
            },
            0,
            None,
        ),
    ];
    for (row, setup, places, phase) in rows {
        let mut h = pair_h(vec![gps()]);
        due(&mut h, GPS);
        setup(&mut h);
        let rec = h.bbo(I1);
        assert_eq!(rec.places.len(), places, "{row}");
        assert!(rec.targets.is_empty(), "{row}");
        if let Some(p) = phase {
            assert_eq!(h.phase(GPS), p, "{row}");
        }
        let g = h.group(GPS);
        match row {
            "LPR-036.2" => assert_eq!(g.skip_unhealthy, 1, "{row}"),
            "LPR-036.3" => assert_eq!(g.skip_spread, 1, "{row}"),
            _ => {}
        }
    }
}

#[test]
fn lpr_037_phase_table() {
    // .1 Idle (PAIR_TICK registered) + Timer{1} → Due.
    let mut h = pair_h(vec![gps()]);
    idle(&mut h, GPS);
    assert!(h.group(GPS).flags & G_TICK != 0, "LPR-037.1");
    let rec = h.timer(GPS, 1);
    assert_eq!((h.phase(GPS), rec.intents()), (Phase::Due, 0), "LPR-037.1");
    // .2 Due + A Bbo.
    let mut h = pair_h(vec![gps()]);
    let (p, _) = opening(&mut h, GPS);
    assert_eq!(
        (p.inst, p.side, p.px, p.qty),
        (I1, Side::Buy, Px(6_030_001), Qty(100_000)),
        "LPR-037.2"
    );
    // .3 Opening + Cancelled with net 0.
    let mut h = pair_h(vec![gps()]);
    let (_, r1) = opening(&mut h, GPS);
    h.at(10);
    let rec = h.order(GPS, r1, OPEN_TAG, cancelled());
    assert_eq!((h.phase(GPS), h.group(GPS).nofill), (Phase::Idle, 1), "LPR-037.3");
    assert_eq!(
        rec.timers.iter().map(|t| t.tag).collect::<Vec<_>>(),
        vec![1],
        "LPR-037.3"
    );
    assert_eq!(
        rec.timers[0].at,
        h.tsc_of(h.group(GPS).next_pair_ens),
        "LPR-037.3"
    );
    // .4 Opening + Filled done.
    let mut h = pair_h(vec![gps()]);
    let (_, r1) = opening(&mut h, GPS);
    h.at(20);
    let rec = h.order(GPS, r1, OPEN_TAG, filled(100_000, 6_030_001, true));
    assert_eq!(h.phase(GPS), Phase::Holding, "LPR-037.4");
    assert_eq!(
        (rec.timers[0].at, rec.timers[0].tag),
        (h.tsc_of(ms(20) + 5_000_000_000), 2),
        "LPR-037.4"
    );
    // .5 Holding with both legs → closing link, Flattening(1).
    let mut h = pair_h(vec![gps()]);
    flattening1(&mut h, GPS);
    // .6 Holding flat + FLAT → Idle.
    let mut h = pair_h(vec![gps()]);
    holding(&mut h, GPS);
    let rec = h.timer(GPS, 2);
    assert_eq!((h.phase(GPS), rec.places.len()), (Phase::Idle, 0), "LPR-037.6");
    // .7 Holding with only A → wait.
    let mut h = pair_h(vec![gps()]);
    holding(&mut h, GPS);
    h.net(GPS, 0, 100_000);
    h.at(100);
    let rec = h.timer(GPS, 2);
    assert_eq!((h.phase(GPS), rec.places.len()), (Phase::Holding, 0), "LPR-037.7");
    assert_eq!(
        (rec.timers[0].at, rec.timers[0].tag),
        (h.tsc_of(ms(100) + 5_000_000_000), 2),
        "LPR-037.7"
    );
    // .8 Flattening(1) + closing first leg done → FLAT.
    let mut h = pair_h(vec![gps()]);
    let r2 = flattening1(&mut h, GPS);
    h.at(200);
    let rec = h.order(GPS, r2, CLOSE_TAG, filled(100_000, 5_970_000, true));
    assert_eq!(
        (rec.timers[0].at, rec.timers[0].tag),
        (h.tsc_of(ms(200) + 5_000_000_000), 2),
        "LPR-037.8"
    );
    assert_eq!(
        (h.phase(GPS), h.group(GPS).attempt),
        (Phase::Flattening, 1),
        "LPR-037.8"
    );
    // .9 Flattening(1) flat + FLAT → Idle.
    let mut h = pair_h(vec![gps()]);
    flattening1(&mut h, GPS);
    h.flat(GPS);
    h.timer(GPS, 2);
    assert_eq!(h.phase(GPS), Phase::Idle, "LPR-037.9");
    // .10 Flattening(1) not flat → second closing link.
    let mut h = pair_h(vec![gps()]);
    flattening1(&mut h, GPS);
    let rec = h.timer(GPS, 2);
    assert_eq!(
        (rec.places.len(), rec.places[0].qty),
        (1, Qty(100_000)),
        "LPR-037.10"
    );
    assert_eq!(
        (h.phase(GPS), h.group(GPS).attempt),
        (Phase::Flattening, 2),
        "LPR-037.10"
    );
    // .11 Flattening(2) not flat → FlattenIncomplete, Blocked.
    let mut h = pair_h(vec![gps()]);
    flattening2(&mut h, GPS);
    let rec = h.timer(GPS, 2);
    let n = rec.notes_of(6);
    assert_eq!(
        (n.len(), n[0].v[0], n[0].v[1], rec.places.len()),
        (1, 120_000_050, 1, 0),
        "LPR-037.11"
    );
    assert_eq!(h.phase(GPS), Phase::Blocked, "LPR-037.11");
    // .12 Blocked flat + any event → Idle, PAIR_TICK.
    let mut h = pair_h(vec![gps()]);
    blocked(&mut h, GPS);
    h.flat(GPS);
    let rec = h.bbo(I2);
    assert_eq!(h.phase(GPS), Phase::Idle, "LPR-037.12");
    assert_eq!(rec.timers.iter().filter(|t| t.tag == 1).count(), 1, "LPR-037.12");
    // .13 Holding + HedgeBroken → Blocked.
    let mut h = pair_h(vec![gps()]);
    holding(&mut h, GPS);
    let rec = h.ev(&ev::risk(T1, RiskEvent::HedgeBroken { group: GPS }));
    assert_eq!((h.phase(GPS), rec.intents()), (Phase::Blocked, 0), "LPR-037.13");
    // .14 Opening + Exec Broken → Blocked.
    let mut h = pair_h(vec![gps()]);
    opening(&mut h, GPS);
    let rec = h.exec(GPS, ExecUpdateKind::Broken(BrokenReason::LinkBroken));
    assert_eq!((h.phase(GPS), rec.intents()), (Phase::Blocked, 0), "LPR-037.14");
    // .15 Blocked with A + Done{cleared_by_engine} → closing link.
    let mut h = pair_h(vec![gps()]);
    blocked(&mut h, GPS);
    h.net(GPS, 0, 100_000);
    let rec = h.exec(
        GPS,
        ExecUpdateKind::Done {
            cleared_by_engine: true,
        },
    );
    assert_eq!(
        (rec.places.len(), rec.places[0].qty),
        (1, Qty(100_000)),
        "LPR-037.15"
    );
    assert_eq!(
        (h.phase(GPS), h.group(GPS).attempt),
        (Phase::Flattening, 1),
        "LPR-037.15"
    );
    // .16 Blocked with A + Timer{1} → nothing.
    let mut h = pair_h(vec![gps()]);
    blocked(&mut h, GPS);
    h.net(GPS, 0, 100_000);
    let rec = h.timer(GPS, 1);
    assert_eq!((h.phase(GPS), rec.intents()), (Phase::Blocked, 0), "LPR-037.16");
}

#[test]
fn lpr_038_ignored_combinations() {
    type Setup = fn(&mut H);
    let rows: Vec<(&str, Setup, &str, Phase)> = vec![
        (
            "LPR-038.1",
            |h| {
                idle(h, GPS);
            },
            "a",
            Phase::Idle,
        ),
        ("LPR-038.2", |h| holding(h, GPS), "t1", Phase::Holding),
        (
            "LPR-038.3",
            |h| {
                idle(h, GPS);
            },
            "t2",
            Phase::Idle,
        ),
        ("LPR-038.4", |h| due(h, GPS), "t2", Phase::Due),
        (
            "LPR-038.5",
            |h| {
                opening(h, GPS);
            },
            "t1",
            Phase::Opening,
        ),
        ("LPR-038.6", |h| holding(h, GPS), "a", Phase::Holding),
        (
            "LPR-038.7",
            |h| {
                flattening1(h, GPS);
            },
            "t1",
            Phase::Flattening,
        ),
    ];
    for (row, setup, e, phase) in rows {
        let mut h = pair_h(vec![gps()]);
        setup(&mut h);
        let attempt = h.group(GPS).attempt;
        let rec = match e {
            "a" => h.bbo(I1),
            "t1" => h.timer(GPS, 1),
            _ => h.timer(GPS, 2),
        };
        assert_eq!(rec.intents(), 0, "{row}");
        assert_eq!((h.phase(GPS), h.group(GPS).attempt), (phase, attempt), "{row}");
    }
}

#[test]
fn lpr_039_first_leg_gated() {
    let rows = [
        ("LPR-039.1", IntentOutcome::Rejected(RejectBits::LINK_STALE), 1),
        ("LPR-039.2", IntentOutcome::Shadowed, 0),
    ];
    for (row, o, gated) in rows {
        let mut h = pair_h(vec![gps()]);
        due(&mut h, GPS);
        h.next_outcomes = Some(vec![o]);
        assert_eq!(h.bbo(I1).places.len(), 1, "{row}");
        h.at(5);
        h.bbo(I2);
        assert_eq!(h.phase(GPS), Phase::Idle, "{row}");
        assert_eq!((h.group(GPS).gated, h.group(GPS).nofill), (gated, 0), "{row}");
    }
}

#[test]
fn lpr_040_close_link() {
    let ro = OrderIntent::REDUCE_ONLY;
    let rows: Vec<(
        &str,
        G,
        i64,
        i64,
        (InstId, Side, u8, i64, i64),
        (Side, u8, InstId),
    )> = vec![
        (
            "LPR-040.1",
            gps(),
            100_000,
            -100_000,
            (I1, Side::Sell, 0, 5_970_000, 100_000),
            (Side::Buy, 1, I2),
        ),
        (
            "LPR-040.2",
            gpp(),
            100_000,
            -100_000,
            (I2, Side::Sell, ro, 5_970_000, 100_000),
            (Side::Buy, 1, I3),
        ),
        (
            "LPR-040.3",
            gsp(),
            -100_000,
            100_000,
            (I2, Side::Buy, ro, 6_030_010, 100_000),
            (Side::Sell, 0, I1),
        ),
        (
            "LPR-040.4",
            gps(),
            99_900,
            -100_000,
            (I1, Side::Sell, 0, 5_970_000, 99_000),
            (Side::Buy, 1, I2),
        ),
    ];
    for (row, grp, na, nb, (inst, side, flags, px, qty), (hs, hro, hi)) in rows {
        let gid = grp.spec.id;
        let mut h = pair_h(vec![grp]);
        holding(&mut h, gid);
        h.net(gid, 0, na).net(gid, 1, nb);
        let rec = h.timer(gid, 2);
        assert_eq!(rec.places.len(), 1, "{row}");
        let p = rec.places[0];
        assert_eq!(
            (p.inst, p.side, p.tif, p.class),
            (inst, side, Tif::Ioc, IntentClass::Reduce),
            "{row}"
        );
        assert_eq!((p.flags, p.px, p.qty), (flags, Px(px), Qty(qty)), "{row}");
        let hp = p.link.l.hedge;
        assert_eq!(
            (hp.side, hp.reduce_only, hp.inst, hp.kind),
            (hs, hro, hi, HedgeKind::OnFill),
            "{row}"
        );
        assert_eq!(
            (hp.px_param, hp.max_slip),
            (Rate(5_000_000), Rate(5_000_000)),
            "{row}"
        );
    }
}
