//! quote probes (TESTS.md LPR-008–LPR-027).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::type_complexity
)] // allow-list: test code

mod common;

use common::*;
use hft_strategy_sdk::{
    CapScopeView, EvMask, IntentClass, IntentOutcome, MarginLevel, OrderStateView, OrderView, Part,
    RiskEvent, RiskStateView, Strategy, SubsBuilder, SubsLimits, VenueSet,
};
use hft_types::{CapSource, GroupId, InstId, Px, Qty, RejectBits, RunMode, Side, StrategyId, Tif, Tsc};
use latency_probe::fast::{G_STARTED, L_INFLIGHT};

const PX_I2_BUY: i64 = 5_820_000;

/// Probe at the current time, Ack at +50 ms and Cancelled at +80 ms after it; returns the ref.
fn probe_cycle(h: &mut H, g: GroupId, i: InstId, leg: u32, t: i64) -> hft_strategy_sdk::OrderRef {
    h.at(t);
    let rec = h.bbo(i);
    assert_eq!(rec.places.len(), 1, "probe at +{t}");
    let r = rec.refs[0];
    h.at(t + 50);
    h.order(g, r, probe_tag(leg), acked());
    h.at(t + 80);
    h.order(g, r, probe_tag(leg), cancelled());
    r
}

/// Starts the group at the current time without probing (a timer tag the quote groups ignore).
fn start(h: &mut H, g: GroupId) {
    h.timer(g, 99);
    assert!(h.group(g).flags & G_STARTED != 0);
}

#[test]
fn lpr_008_subscriptions() {
    let h = H::new(&[], vec![gq(), g(GroupId(0x0500_0010), &[I1, I3], PAIR)]);
    let room = vec![8u16; 16];
    let lim = SubsLimits {
        rd: h.rd,
        feeds: VenueSet::EMPTY.with(V1).with(V2).with(V3),
        room: &room,
    };
    let mut sb = SubsBuilder::new(lim, 16);
    h.s.subscriptions(&h.table, &mut sb);
    let got: Vec<_> = sb
        .entries()
        .iter()
        .map(|e| (e.inst, e.gix, e.ev, e.part))
        .collect();
    assert_eq!(
        got,
        vec![
            (I2, 0, EvMask::BBO, Part::Fast),
            (I1, 1, EvMask::BBO, Part::Fast),
            (I3, 1, EvMask::BBO, Part::Fast)
        ]
    );
    assert_eq!(sb.model_tick_ms(), None);
    assert_eq!(sb.own_mask().0, 0);
    assert_eq!(sb.warmup_part(), Part::Model);
}

#[test]
fn lpr_009_one_probe_per_period() {
    let mut h = H::new(&[], vec![gq()]);
    let mut places = Vec::new();
    // Order lifecycle events: (time, ref, kind).
    let mut pending: Vec<(i64, hft_strategy_sdk::OrderRef, bool)> = Vec::new();
    for k in 0..30i64 {
        let t = k * 100;
        // Deliver due Ack/Cancelled before this Bbo.
        pending.sort_by_key(|x| x.0);
        while pending.first().is_some_and(|x| x.0 < t) {
            let (at, r, ack) = pending.remove(0);
            h.at(at);
            let rec = h.order(GQ, r, probe_tag(0), if ack { acked() } else { cancelled() });
            if ack {
                assert_eq!(rec.cancels.len(), 1);
                assert_eq!(rec.cancels[0].r, r);
            }
        }
        h.at(t);
        let rec = h.bbo(I2);
        if rec.places.is_empty() {
            assert_eq!((rec.intents(), rec.notes.len()), (0, 0), "+{t}");
            continue;
        }
        let p = rec.places[0];
        places.push(t);
        assert_eq!(
            (p.venue, p.side, p.tif, p.class, p.flags, p.inst),
            (V2, Side::Buy, Tif::PostOnly, IntentClass::Open, 0, I2)
        );
        assert_eq!(
            (p.px, p.qty, p.max_rest_ms, p.link.some),
            (Px(PX_I2_BUY), Qty(100_000), 2_000, 0)
        );
        pending.push((t + 50, rec.refs[0], true));
        pending.push((t + 80, rec.refs[0], false));
    }
    assert_eq!(places, vec![0, 1_000, 2_000]);
}

#[test]
fn lpr_010_price_and_qty() {
    let q = |id, i| g(GroupId(id), &[i], QUOTE);
    let rows: Vec<(&str, G, Vec<(u16, i64)>, i64, i64)> = vec![
        ("LPR-010.1", q(0x0500_0021, I2), vec![], 5_820_000, 100_000),
        (
            "LPR-010.2",
            q(0x0500_0021, I2).with(f::QUOTE_SIDE, 1),
            vec![],
            6_180_000,
            100_000,
        ),
        ("LPR-010.3", q(0x0500_0021, I3), vec![], 5_820_100, 100_000),
        (
            "LPR-010.4",
            q(0x0500_0021, I3).with(f::QUOTE_SIDE, 1),
            vec![],
            6_180_110,
            100_000,
        ),
        ("LPR-010.5", q(0x0500_0021, I1), vec![], 5_820_000, 10_000),
        (
            "LPR-010.6",
            q(0x0500_0021, I1),
            vec![(f::SIZE_MARGIN, 0)],
            5_820_000,
            9_000,
        ),
        (
            "LPR-010.7",
            q(0x0500_0021, I1),
            vec![(f::SIZE_MARGIN, 1_000_000_000)],
            5_820_000,
            18_000,
        ),
        (
            "LPR-010.8",
            q(0x0500_0021, I2),
            vec![(f::SIZE_MARGIN, 0)],
            5_820_000,
            100_000,
        ),
        (
            "LPR-010.9",
            q(0x0500_0021, I2).with(f::FAR_BP, 5_000_000),
            vec![],
            5_970_000,
            100_000,
        ),
    ];
    for (row, grp, defaults, px, qty) in rows {
        let i = grp.spec.legs[0].inst;
        let mut h = H::new(&defaults, vec![grp]);
        let rec = h.bbo(i);
        assert_eq!(rec.places.len(), 1, "{row}");
        assert_eq!((rec.places[0].px, rec.places[0].qty), (Px(px), Qty(qty)), "{row}");
    }
}

#[test]
fn lpr_011_period_boundary() {
    let rows = [
        ("LPR-011.1", 999_999_999, 0),
        ("LPR-011.2", 1_000_000_000, 1),
        ("LPR-011.3", 1_000_000_001, 1),
    ];
    for (row, d, n) in rows {
        let mut h = H::new(&[], vec![gq()]);
        probe_cycle(&mut h, GQ, I2, 0, 0);
        h.at_ens(E0 + d);
        assert_eq!(h.bbo(I2).places.len(), n, "{row}");
    }
}

#[test]
fn lpr_012_no_probe_while_inflight() {
    let mut h = H::new(&[], vec![gq()]);
    let r1 = h.bbo(I2).refs[0];
    h.at(1_500);
    let rec = h.bbo(I2);
    assert_eq!(rec.places.len(), 0);
    assert_eq!(h.group(GQ).skip_unhealthy + h.group(GQ).skip_spread, 0);
    h.at(1_600);
    let rec = h.order(GQ, r1, probe_tag(0), acked());
    assert_eq!(rec.cancels.iter().map(|c| c.r).collect::<Vec<_>>(), vec![r1]);
    h.at(1_700);
    h.order(GQ, r1, probe_tag(0), cancelled());
    h.at(1_800);
    assert_eq!(h.bbo(I2).places.len(), 1);
}

/// Makes I2's connection silent for 7 s at the current time.
fn silent(h: &mut H) {
    h.healthy = false;
    let t = h.now().engine_ns - 7_000_000_000;
    let tsc = h.tsc_of(t);
    hft_strategy_sdk::testing::MockViews::conn(&mut h.v.m, I2, tsc, true, false);
}

#[test]
fn lpr_013_skipped_counts() {
    type Setup = fn(&mut H);
    let rows: Vec<(&str, Setup, usize, Option<(u64, u64)>)> = vec![
        ("LPR-013.1", |h| silent(h), 0, Some((0, 1))),
        (
            "LPR-013.2",
            |h| {
                h.quote(I2, 6_000_000, 6_100_000);
            },
            0,
            Some((1, 0)),
        ),
        (
            "LPR-013.3",
            |h| {
                h.quote(I2, 0, 6_000_010);
            },
            0,
            Some((1, 0)),
        ),
        // Probed at +500 ms and cancelled: the +1 000 ms Bbo is not due.
        ("LPR-013.4", |h| silent(h), 0, None),
        ("LPR-013.5", |h| silent(h), 0, None),
        ("LPR-013.6", |_| {}, 1, None),
    ];
    for (row, setup, places, note) in rows {
        let mut h = H::new(&[], vec![gq()]);
        start(&mut h, GQ);
        match row {
            "LPR-013.4" => {
                probe_cycle(&mut h, GQ, I2, 0, 500);
            }
            "LPR-013.5" => {
                h.at(500);
                assert_eq!(h.bbo(I2).places.len(), 1, "{row}");
            }
            _ => {}
        }
        h.at(1_000);
        setup(&mut h);
        assert_eq!(h.bbo(I2).places.len(), places, "{row}");
        h.healthy = true;
        h.quote(I2, 6_000_000, 6_000_010);
        h.at(600_000);
        let rec = h.bbo(I2);
        let sk = rec.notes_of(5);
        match note {
            Some((v0, v1)) => {
                assert_eq!(sk.len(), 1, "{row}");
                assert_eq!((sk[0].v[0], sk[0].v[1], sk[0].group), (v0, v1, GQ), "{row}");
            }
            None => assert!(sk.is_empty(), "{row}"),
        }
    }
}

#[test]
fn lpr_014_spread_boundary() {
    let rows = [
        ("LPR-014.1", 9_950_000, 10_050_000, 1, 0),
        ("LPR-014.2", 9_950_000, 10_050_010, 0, 1),
        ("LPR-014.3", 9_950_000, 10_049_990, 1, 0),
    ];
    for (row, bid, ask, places, skip) in rows {
        let mut h = H::new(&[], vec![gq()]);
        h.quote(I2, bid, ask);
        assert_eq!(h.bbo(I2).places.len(), places, "{row}");
        assert_eq!(h.group(GQ).skip_spread, skip, "{row}");
    }
}

fn halted_scope(g: GroupId) -> CapScopeView {
    CapScopeView {
        strategy: StrategyId(1),
        venue: hft_types::VenueId(0),
        account: T1,
        group: g,
        inst: InstId(0),
    }
}

#[test]
fn lpr_015_gates() {
    type Setup = fn(&mut H);
    let cap = |h: &mut H, set: bool, g: GroupId| {
        let e = if set {
            RiskEvent::CapSet {
                source: CapSource::HALTED,
                scope: halted_scope(g),
            }
        } else {
            RiskEvent::CapCleared {
                source: CapSource::HALTED,
                scope: halted_scope(g),
            }
        };
        h.ev(&ev::risk(T1, e));
    };
    let rows: Vec<(&str, Vec<(u16, i64)>, Setup, usize)> = vec![
        ("LPR-015.1", vec![], |_| {}, 1),
        ("LPR-015.2", vec![(f::ENABLED, 0)], |_| {}, 0),
        (
            "LPR-015.3",
            vec![],
            |h| {
                h.mode(GQ, RunMode::CloseOnly);
            },
            0,
        ),
        (
            "LPR-015.4",
            vec![],
            |h| {
                h.mode(GQ, RunMode::Shadow);
            },
            0,
        ),
        (
            "LPR-015.5",
            vec![],
            |h| {
                let r = RiskStateView {
                    margin_level: MarginLevel::L1,
                    conn_liveness_ms: 2_000,
                    ..RiskStateView::EMPTY
                };
                hft_strategy_sdk::testing::MockViews::risk(&mut h.v.m, T1, r);
            },
            0,
        ),
        ("LPR-015.6", vec![], |_| {}, 0),
        ("LPR-015.7", vec![], |_| {}, 1),
        ("LPR-015.8", vec![], |_| {}, 1),
    ];
    for (row, defaults, setup, places) in rows {
        let mut h = H::new(&defaults, vec![gq()]);
        setup(&mut h);
        match row {
            "LPR-015.6" => cap(&mut h, true, GQ),
            "LPR-015.7" => {
                cap(&mut h, true, GQ);
                cap(&mut h, false, GQ);
            }
            "LPR-015.8" => cap(&mut h, true, GroupId(0x0599_0001)),
            _ => {}
        }
        let rec = h.bbo(I2);
        assert_eq!(rec.places.len(), places, "{row}");
        if places == 0 {
            assert_eq!(rec.intents(), 0, "{row}");
        }
    }
}

#[test]
fn lpr_016_disabled_still_cleans_up() {
    let mut h = H::new(&[], vec![gq()]);
    let r1 = h.bbo(I2).refs[0];
    let groups = h.groups.clone();
    h.params(&[(f::ENABLED, 0)], groups);
    h.at(50);
    let rec = h.order(GQ, r1, probe_tag(0), acked());
    assert_eq!(rec.cancels.iter().map(|c| c.r).collect::<Vec<_>>(), vec![r1]);
    h.at(60);
    h.net(GQ, 0, 100_000);
    let rec = h.order(GQ, r1, probe_tag(0), filled(100_000, 5_820_000, true));
    let n = rec.notes_of(1);
    assert_eq!(
        (n.len(), n[0].v[0], n[0].v[1], n[0].v[2]),
        (1, 0, 100_000, 5_820_000)
    );
    assert_eq!(rec.places.len(), 1);
    let p = rec.places[0];
    assert_eq!(
        (p.inst, p.side, p.tif, p.class),
        (I2, Side::Sell, Tif::Ioc, IntentClass::Flatten)
    );
    assert_eq!(
        (p.flags, p.qty, p.px),
        (
            hft_strategy_sdk::OrderIntent::REDUCE_ONLY,
            Qty(100_000),
            Px(5_970_000)
        )
    );
    h.at(2_000);
    assert_eq!(h.bbo(I2).places.len(), 0);
}

#[test]
fn lpr_017_order_outcomes() {
    let rows: Vec<(
        &str,
        hft_strategy_sdk::OrderUpdateKind,
        usize,
        Option<(u32, [u64; 3])>,
    )> = vec![
        ("LPR-017.1", cancelled(), 1, None),
        ("LPR-017.2", hft_strategy_sdk::OrderUpdateKind::Aged, 1, None),
        ("LPR-017.3", rejected(0, 1013), 1, Some((2, [1, 0, 1013]))),
        (
            "LPR-017.4",
            rejected(RejectBits::LOCAL_SEND_FAIL.0, 3),
            1,
            Some((3, [1, u64::from(RejectBits::LOCAL_SEND_FAIL.0), 0])),
        ),
        ("LPR-017.5", hft_strategy_sdk::OrderUpdateKind::Unknown, 0, None),
    ];
    for (row, kind, places, note) in rows {
        let mut h = H::new(&[], vec![gq()]);
        let r1 = h.bbo(I2).refs[0];
        h.at(50);
        h.order(GQ, r1, probe_tag(0), kind);
        h.at(1_000);
        assert_eq!(h.bbo(I2).places.len(), places, "{row}");
        h.at(600_000);
        let rec = h.bbo(I2);
        let counts: Vec<_> = rec
            .notes
            .iter()
            .filter(|n| (2..=5).contains(&n.code))
            .copied()
            .collect();
        match note {
            None => assert!(counts.is_empty(), "{row}: {counts:?}"),
            Some((code, v)) => {
                assert_eq!(counts.len(), 1, "{row}: {counts:?}");
                assert_eq!(counts[0].code, code, "{row}");
                assert_eq!(&counts[0].v[..3], &v[..], "{row}");
            }
        }
    }
}

#[test]
fn lpr_018_intent_outcomes() {
    let px_band = RejectBits::PX_BAND;
    let rows = [
        ("LPR-018.1", IntentOutcome::Placed(r_of(1, V2)), 0usize),
        ("LPR-018.2", IntentOutcome::Rejected(px_band), 1),
        ("LPR-018.3", IntentOutcome::Shadowed, 1),
    ];
    for (row, o, places) in rows {
        let mut h = H::new(&[], vec![gq()]);
        h.next_outcomes = Some(vec![o]);
        assert_eq!(h.bbo(I2).places.len(), 1, "{row}");
        h.at(10);
        h.bbo(I2);
        h.at(1_000);
        assert_eq!(h.bbo(I2).places.len(), places, "{row}");
        assert_eq!(h.group(GQ).thr.consec_fail(), 0, "{row}");
        if row == "LPR-018.1" {
            h.at(1_100);
            let rec = h.order(GQ, r_of(1, V2), probe_tag(0), acked());
            assert_eq!(rec.cancels[0].r, r_of(1, V2), "{row}");
        }
        h.at(600_000);
        let rec = h.bbo(I2);
        let gated = rec.notes_of(3);
        match row {
            "LPR-018.2" => {
                assert_eq!(gated.len(), 1, "{row}");
                assert_eq!((gated[0].v[0], gated[0].v[1]), (1, u64::from(px_band.0)), "{row}");
            }
            _ => assert!(gated.is_empty(), "{row}"),
        }
        assert!(rec.notes_of(2).is_empty(), "{row}");
    }
}

#[test]
fn lpr_019_consec_fail_halt() {
    let rows: &[(&str, i64, &str, Option<usize>)] = &[
        ("LPR-019.1", 5, "RRRR", None),
        ("LPR-019.2", 5, "RRRRR", Some(5)),
        ("LPR-019.3", 5, "RRGRRR", Some(6)),
        ("LPR-019.4", 5, "RRCRRR", None),
        ("LPR-019.5", 1, "R", Some(1)),
    ];
    for &(row, max, seq, halt_at) in rows {
        let mut h = H::new(&[(f::MAX_CONSEC_FAIL, max)], vec![gq()]);
        let mut halted_at = None;
        for (k, c) in seq.chars().enumerate() {
            let t = k as i64 * 1_000;
            h.at(t);
            let rec = h.bbo(I2);
            assert_eq!(rec.places.len(), 1, "{row} probe {}", k + 1);
            h.at(t + 50);
            let kind = match c {
                'R' => rejected(0, 1),
                'G' => rejected(RejectBits::RATE.0, 0),
                _ => cancelled(),
            };
            let rec = h.order(GQ, rec.refs[0], probe_tag(0), kind);
            if !rec.halts.is_empty() {
                assert_eq!(rec.halts, vec![(GQ, 1)], "{row}");
                let n = rec.notes_of(7);
                assert_eq!((n[0].v[0], n[0].v[1]), (1, max as u64), "{row}");
                halted_at.get_or_insert(k + 1);
            }
        }
        assert_eq!(halted_at, halt_at, "{row}");
        if halt_at.is_some() {
            h.at(seq.len() as i64 * 1_000);
            assert_eq!(h.bbo(I2).places.len(), 0, "{row}");
        }
    }
}

/// +0 probe r1 Acked at +50 ms (cancel already sent).
fn acked_probe(h: &mut H) -> hft_strategy_sdk::OrderRef {
    let r1 = h.bbo(I2).refs[0];
    h.at(50);
    h.order(GQ, r1, probe_tag(0), acked());
    r1
}

#[test]
fn lpr_020_unexpected_fill_below_lot() {
    let mut h = H::new(&[], vec![gq()]);
    let r1 = acked_probe(&mut h);
    h.at(60);
    h.net(GQ, 0, 40_000);
    let rec = h.order(GQ, r1, probe_tag(0), filled(40_000, 5_820_000, false));
    let n = rec.notes_of(1);
    assert_eq!(
        (n.len(), n[0].v[0], n[0].v[1], n[0].v[2]),
        (1, 0, 40_000, 5_820_000)
    );
    assert_eq!(rec.places.len(), 0);
    h.at(70);
    h.order(GQ, r1, probe_tag(0), cancelled());
    h.at(1_000);
    assert_eq!(h.bbo(I2).places.len(), 1);
}

#[test]
fn lpr_021_unexpected_fill_flatten() {
    let mut h = H::new(&[], vec![gq()]);
    let r1 = acked_probe(&mut h);
    h.at(60);
    h.net(GQ, 0, 100_000);
    let rec = h.order(GQ, r1, probe_tag(0), filled(100_000, 5_820_000, true));
    assert_eq!(rec.notes_of(1).len(), 1);
    assert_eq!(rec.places.len(), 1);
    let p = rec.places[0];
    assert_eq!(
        (p.inst, p.side, p.tif, p.class),
        (I2, Side::Sell, Tif::Ioc, IntentClass::Flatten)
    );
    assert_eq!(
        (p.flags, p.qty, p.px),
        (
            hft_strategy_sdk::OrderIntent::REDUCE_ONLY,
            Qty(100_000),
            Px(5_970_000)
        )
    );
    assert_eq!(h.group(GQ).legs[0].flags & L_INFLIGHT, 0);
}

#[test]
fn lpr_022_unknown_pauses_leg() {
    let mut h = H::new(&[], vec![gq()]);
    let r1 = h.bbo(I2).refs[0];
    h.at(50);
    h.order(GQ, r1, probe_tag(0), hft_strategy_sdk::OrderUpdateKind::Unknown);
    for t in [1_000, 2_000] {
        h.at(t);
        assert_eq!(h.bbo(I2).places.len(), 0);
    }
    assert_eq!(h.group(GQ).skip_unhealthy + h.group(GQ).skip_spread, 0);
    h.at(2_100);
    h.order(GQ, r1, probe_tag(0), cancelled());
    h.at(2_200);
    assert_eq!(h.bbo(I2).places.len(), 1);
    h.at(600_000);
    assert!(h.bbo(I2).notes_of(5).is_empty());
}

#[test]
fn lpr_023_alternate() {
    let rows = [
        ("LPR-023.1", I2, [Side::Buy, Side::Sell, Side::Buy, Side::Sell]),
        ("LPR-023.2", I1, [Side::Buy; 4]),
    ];
    for (row, i, want) in rows {
        let gid = GroupId(0x0500_0031);
        let mut h = H::new(&[(f::QUOTE_SIDE, 2)], vec![g(gid, &[i], QUOTE)]);
        let mut got = Vec::new();
        for k in 0..4i64 {
            h.at(k * 1_000);
            let rec = h.bbo(i);
            got.push(rec.places[0].side);
            h.at(k * 1_000 + 50);
            h.order(gid, rec.refs[0], probe_tag(0), acked());
            h.at(k * 1_000 + 80);
            h.order(gid, rec.refs[0], probe_tag(0), cancelled());
        }
        assert_eq!(got, want, "{row}");
    }
}

#[test]
fn lpr_024_burst_trigger() {
    let rows: &[(&str, i64, i64, Option<(u64, u64)>, usize)] = &[
        ("LPR-024.1", 200, ms(300), Some((9_000_000, 9_000_000)), 1),
        ("LPR-024.2", 200, ms(300), Some((9_000_000, 9_000_001)), 0),
        (
            "LPR-024.3",
            200,
            E0 + 199_999_999,
            Some((9_000_000, 9_000_000)),
            0,
        ),
        (
            "LPR-024.4",
            200,
            E0 + 200_000_000,
            Some((9_000_000, 9_000_000)),
            1,
        ),
        ("LPR-024.5", 0, ms(300), Some((9_000_000, 9_000_000)), 0),
        ("LPR-024.6", 200, ms(300), Some((0, 0)), 0),
    ];
    for &(row, burst, at, rx, places) in rows {
        let mut h = H::new(&[], vec![gq().with(f::BURST_PERIOD_MS, burst)]);
        let rec = h.bbo_rx(I2, 1_000);
        let r = rec.refs[0];
        h.at(50);
        h.order(GQ, r, probe_tag(0), acked());
        h.at(80);
        h.order(GQ, r, probe_tag(0), cancelled());
        h.at_ens(at);
        let (a, b) = rx.unwrap();
        assert_eq!(h.bbo_rx(I2, a).places.len(), 0, "{row} first");
        assert_eq!(h.bbo_rx(I2, b).places.len(), places, "{row} second");
    }
}

#[test]
fn lpr_025_burst_state_not_persisted() {
    let mut h = H::new(&[], vec![gq().with(f::BURST_PERIOD_MS, 200)]);
    let r = h.bbo_rx(I2, 1_000).refs[0];
    h.at(50);
    h.order(GQ, r, probe_tag(0), acked());
    h.at(80);
    h.order(GQ, r, probe_tag(0), cancelled());
    h.at(250);
    assert_eq!(h.bbo_rx(I2, 9_000_000).places.len(), 0);
    h.restart(7, 2);
    h.at(300);
    assert_eq!(h.bbo_rx(I2, 9_000_000).places.len(), 0);
}

#[test]
fn lpr_026_restart_continues() {
    let run = |tsc0: u64, tpm: u64| {
        let mut h = H::new(&[], vec![gq()]);
        hft_strategy_sdk::testing::MockViews::at(
            &mut h.v.m,
            hft_strategy_sdk::Now::new(Tsc(tsc0), 1, 1, true, anchor(tsc0, tpm, E0, U)),
        );
        h.at(0);
        probe_cycle(&mut h, GQ, I2, 0, 0);
        h.at(500);
        (h.snapshot(), h)
    };
    let (s1, mut h) = run(BASE_TSC, TSC_PER_MS);
    let (s2, _) = run(1_000_000_000_000_000, 1_000_000);
    assert_eq!(s1, s2);
    h.restart(7, 2);
    h.at_ens(E0 + 999_999_999);
    assert_eq!(h.bbo(I2).places.len(), 0);
    h.at_ens(E0 + 1_000_000_000);
    assert_eq!(h.bbo(I2).places.len(), 1);
}

#[test]
fn lpr_027_restore_pending_rest() {
    let rows = [
        ("LPR-027.1", Some(OrderStateView::Live), 1, 0),
        ("LPR-027.2", Some(OrderStateView::PendingNew), 1, 0),
        ("LPR-027.3", None, 0, 1),
        ("LPR-027.4", Some(OrderStateView::Cancelled), 0, 1),
    ];
    for (row, st, cancels, places) in rows {
        let mut h = H::new(&[], vec![gq()]);
        let r1 = h.bbo(I2).refs[0];
        h.at(10);
        h.bbo(I2);
        h.restart(7, 2);
        if let Some(state) = st {
            h.v.orders.push(OrderView {
                r: r1,
                group: GQ,
                inst: I2,
                side: Side::Buy,
                tif: Tif::PostOnly,
                class: IntentClass::Open,
                px: Px(PX_I2_BUY),
                qty: Qty(100_000),
                filled: Qty(0),
                state,
                tag: probe_tag(0),
            });
        }
        h.at(1_500);
        let rec = h.bbo(I2);
        assert_eq!(rec.cancels.len(), cancels, "{row}");
        if cancels == 1 {
            assert_eq!(rec.cancels[0].r, r1, "{row}");
        }
        assert_eq!(rec.places.len(), places, "{row}");
    }
}
