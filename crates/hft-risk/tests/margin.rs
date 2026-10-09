//! Margin tiers by distance to liquidation (RSK-160–RSK-172, RSK-283).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)] // allow-list: test code, not a hot path

mod common;
#[path = "model/ledger_liq.rs"]
mod ledger_liq;

use common::*;
use hft_event::NoteLevel;
use hft_risk::mode::ModeInput;
use hft_risk::snapshot::{MarginRow, SECTION_MARGIN};
use hft_risk::{ForceDeleverage, MarginOut, MarginSample, RiskTimer, mflags};
use hft_strategy_sdk::{MarginLevel, Now, RiskEvent};
use hft_types::{AccountId, CapSource, Rate, VenueId};
use proptest::prelude::*;

thread_local! {
    static BASE: W = r0();
}
fn base() -> W {
    BASE.with(|b| b.clone())
}

fn rate(s: &str) -> Rate {
    if s == "MAX" { Rate::MAX } else { r(s) }
}
/// `ms(rep, est, rr, er)` with `report_ens = mark_ens = now`.
fn ms(rep: &str, est: &str, rr: &str, er: &str, now: i64) -> MarginSample {
    MarginSample {
        rep_liq_dist: rate(rep),
        est_liq_dist: rate(est),
        reported_ratio: rate(rr),
        est_ratio: rate(er),
        report_ens: now,
        mark_ens: now,
        has_report: true,
        truncated: false,
    }
}
fn liq(x: &str, now: i64) -> MarginSample {
    ms(x, x, "0.10", "0.10", now)
}

/// The `MarginRow` of (a, v) in the current snapshot.
fn row(w: &W, a: u8, v: VenueId) -> Option<MarginRow> {
    let bytes = w.snapshot();
    let rd = hft_strategy_sdk::SnapshotReader::new(&bytes);
    let s = rd.find_section(SECTION_MARGIN).unwrap()?;
    let mut x = s.reader();
    (0..s.count)
        .map(|_| x.get_row::<MarginRow>().unwrap())
        .find(|m| m.account == a && m.venue == v.0)
}
fn lvl(w: &W) -> u8 {
    row(w, 1, B).map_or(0, |m| m.level)
}
fn on(w: &mut W, s: &MarginSample, at: i64) {
    w.margin(1, B, s, at);
}
fn timer(w: &mut W, t: RiskTimer, now: &Now) {
    w.risk.on_timer(t, now, &mut w.mo);
}

const STALE: RiskTimer = RiskTimer::MarginStale {
    a: AccountId(1),
    v: B,
};
const DEADLINE: RiskTimer = RiskTimer::MarginDeadline {
    a: AccountId(1),
    v: B,
};
const FORCE: RiskTimer = RiskTimer::MarginForce {
    a: AccountId(1),
    v: B,
};

fn timers(mo: &MarginOut) -> Vec<(RiskTimer, i64)> {
    mo.timers.iter().copied().collect()
}
fn ms_input(mo: &MarginOut) -> Vec<(u8, u64)> {
    mo.mode
        .iter()
        .filter_map(|m| match m {
            ModeInput::Auto(r) if r.source == CapSource::MARGIN_STRESS => Some((r.level, r.params[0])),
            _ => None,
        })
        .collect()
}
fn is_clear(mo: &MarginOut) -> bool {
    mo.mode.iter().any(|m| {
        matches!(m, ModeInput::AutoClear { source, scope } if *source == CapSource::MARGIN_STRESS && *scope == sc_acct(1))
    })
}
fn ev(mo: &MarginOut) -> Vec<(u8, VenueId, MarginLevel)> {
    mo.events
        .iter()
        .filter_map(|(a, e)| match e {
            RiskEvent::MarginStress { venue, level, .. } => Some((a.0, *venue, *level)),
            _ => None,
        })
        .collect()
}
fn force() -> Option<ForceDeleverage> {
    Some(ForceDeleverage {
        account: acc(1),
        venue: B,
        frac: r("0.20"),
    })
}

#[test]
fn rsk_160_level_by_distance() {
    let rows: [(&str, MarginSample, u8); 16] = [
        ("RSK-160.1", liq("0.250000001", T0), 0),
        ("RSK-160.2", liq("0.25", T0), 1),
        ("RSK-160.3", liq("0.249999999", T0), 1),
        ("RSK-160.4", liq("0.150000001", T0), 1),
        ("RSK-160.5", liq("0.15", T0), 2),
        ("RSK-160.6", liq("0.100000001", T0), 2),
        ("RSK-160.7", liq("0.10", T0), 3),
        ("RSK-160.8", liq("0.099999999", T0), 3),
        ("RSK-160.9", ms("0.30", "0.12", "0.10", "0.10", T0), 2),
        ("RSK-160.10", ms("0.12", "0.30", "0.10", "0.10", T0), 2),
        ("RSK-160.11", ms("MAX", "0.20", "0.10", "0.10", T0), 1),
        (
            "RSK-160.12",
            MarginSample {
                mark_ens: i64::MAX,
                ..ms("MAX", "MAX", "0", "0", T0)
            },
            0,
        ),
        ("RSK-160.13", ms("0.30", "0.30", "0.80", "0.10", T0), 2),
        ("RSK-160.14", ms("0.30", "0.30", "0.799999999", "0.80", T0), 2),
        (
            "RSK-160.15",
            ms("0.30", "0.30", "0.799999999", "0.799999999", T0),
            0,
        ),
        ("RSK-160.16", ms("0.30", "0.30", "MAX", "0.10", T0), 2),
    ];
    for (id, s, want) in rows {
        let mut w = base();
        on(&mut w, &s, T0);
        assert_eq!(lvl(&w), want, "{id}");
        if id == "RSK-160.13" {
            assert_ne!(
                row(&w, 1, B).unwrap().mflags & mflags::RATIO_L2,
                0,
                "{id} RATIO_L2"
            );
        }
    }
}

#[test]
fn rsk_161_level_entry_outputs() {
    let mut w = base();
    on(&mut w, &liq("0.25", T0), T0);
    assert_eq!(ms_input(&w.mo), vec![(1, 8)], "step 1 mode");
    assert_eq!(ev(&w.mo), vec![(1, B, MarginLevel::L1)]);
    match w.mo.events.get(0).unwrap().1 {
        RiskEvent::MarginStress { liq_dist, ratio, .. } => {
            assert_eq!((liq_dist, ratio), (r("0.25"), r("0.10")))
        }
        _ => panic!(),
    }
    assert!(w.mo.force.is_none() && w.mo.notes.is_empty(), "step 1");
    let t1 = T0 + SEC;
    on(&mut w, &liq("0.15", t1), t1);
    let d = T0 + 31 * SEC;
    assert_eq!(ms_input(&w.mo), vec![(2, 8)], "step 2 mode");
    match w.mo.events.get(0).unwrap() {
        (
            a,
            RiskEvent::MarginStress {
                venue,
                level,
                liq_dist,
                ratio,
                deadline_tsc,
            },
        ) => {
            assert_eq!(
                (a.0, *venue, *level, *liq_dist, *ratio),
                (1, B, MarginLevel::L2, r("0.15"), r("0.10"))
            );
            assert_eq!(*deadline_tsc, now_at(t1).tsc_of(d));
        }
        e => panic!("{e:?}"),
    }
    assert!(timers(&w.mo).contains(&(DEADLINE, d)));
    assert_eq!(w.mo.notes.len(), 1);
    let nt = w.mo.notes.get(0).unwrap();
    assert_eq!((note_n(nt), nt.level), (N_MARGIN_LEVEL, NoteLevel::P1));
    assert_eq!(
        &nt.v[..5],
        &[769, 2, r("0.15").0 as u64, r("0.10").0 as u64, d as u64]
    );
    let m = row(&w, 1, B).unwrap();
    assert_eq!((m.deadline_ens, m.l2_since_ens), (d, t1));
    let t2 = T0 + 2 * SEC;
    on(&mut w, &liq("0.10", t2), t2);
    assert_eq!(ms_input(&w.mo).first().map(|x| x.0), Some(3), "step 3 mode");
    assert_eq!(ev(&w.mo), vec![(1, B, MarginLevel::L3)]);
    assert_eq!(w.mo.force, force());
    assert!(timers(&w.mo).contains(&(FORCE, T0 + 7 * SEC)));
    assert_eq!(w.mo.notes.len(), 1);
    let nt = w.mo.notes.get(0).unwrap();
    assert_eq!(
        (note_n(nt), nt.level, nt.v[1]),
        (N_MARGIN_LEVEL, NoteLevel::P0, 3)
    );
    assert_eq!(row(&w, 1, B).unwrap().next_force_ens, T0 + 7 * SEC);
}

#[test]
fn rsk_162_force_steps() {
    let t = T0;
    let mut w = base();
    on(&mut w, &liq("0.10", t), t);
    timer(&mut w, FORCE, &now_at(t + 5 * SEC));
    assert_eq!(w.mo.force, force(), "step 1");
    assert!(timers(&w.mo).contains(&(FORCE, t + 10 * SEC)));
    on(&mut w, &liq("0.13", t + 8 * SEC), t + 8 * SEC);
    assert_eq!(lvl(&w), 2, "step 2");
    assert_eq!(row(&w, 1, B).unwrap().deadline_ens, t + 38 * SEC);
    assert!(timers(&w.mo).contains(&(DEADLINE, t + 38 * SEC)));
    assert!(w.mo.force.is_none());
    timer(&mut w, FORCE, &now_at(t + 10 * SEC));
    assert!(w.mo.force.is_none() && w.mo.timers.is_empty(), "step 3");
    assert_eq!(lvl(&w), 2);
}

#[test]
fn rsk_163_hysteresis() {
    let t1 = T0 + SEC;
    let run = |id: &str, start: MarginSample, next: MarginSample, want: u8| {
        let mut w = base();
        on(&mut w, &start, T0);
        on(&mut w, &next, t1);
        assert_eq!(lvl(&w), want, "{id}");
        w
    };
    run("RSK-163.1 a", liq("0.25", T0), liq("0.27", t1), 1);
    run("RSK-163.1 b", liq("0.25", T0), liq("0.270000001", t1), 0);
    run("RSK-163.2 a", liq("0.15", T0), liq("0.17", t1), 2);
    run("RSK-163.2 b", liq("0.15", T0), liq("0.170000001", t1), 1);
    run("RSK-163.3 a", liq("0.10", T0), liq("0.12", t1), 3);
    run("RSK-163.3 b", liq("0.10", T0), liq("0.120000001", t1), 2);
    run("RSK-163.4", liq("0.10", T0), liq("0.18", t1), 1);
    run("RSK-163.5", liq("0.10", T0), liq("0.30", t1), 0);
    let ratio = ms("0.30", "0.30", "0.80", "0.10", T0);
    run("RSK-163.6 a", ratio, ms("0.30", "0.30", "0.75", "0.10", t1), 2);
    run(
        "RSK-163.6 b",
        ratio,
        ms("0.30", "0.30", "0.749999999", "0.10", t1),
        0,
    );
    run("RSK-163.7", ratio, ms("0.26", "0.26", "0.70", "0.10", t1), 1);
    let stale = MarginSample {
        report_ens: t1 - 15_001 * MS,
        ..liq("0.50", t1)
    };
    run("RSK-163.8", liq("0.25", T0), stale, 1);
    let w = run("RSK-163.9", liq("0.25", T0), liq("0.10", t1), 3);
    assert_eq!(row(&w, 1, B).unwrap().deadline_ens, 0, "RSK-163.9 no deadline");
}

#[test]
fn rsk_164_deadline_l3() {
    let t = T0;
    let mut w = base();
    on(&mut w, &liq("0.14", t), t);
    timer(&mut w, DEADLINE, &now_at(t + 29_999_999_999));
    assert_eq!(lvl(&w), 2, "step 1");
    assert_eq!(timers(&w.mo), vec![(DEADLINE, t + 30 * SEC)]);
    assert!(w.mo.force.is_none());
    timer(&mut w, DEADLINE, &now_at(t + 30 * SEC));
    let m = row(&w, 1, B).unwrap();
    assert_eq!(m.level, 3, "step 2");
    assert_ne!(m.mflags & mflags::DEADLINE_L3, 0);
    assert_eq!(w.mo.force, force());
    assert!(timers(&w.mo).contains(&(FORCE, t + 35 * SEC)));
    assert!(
        w.mo.notes
            .iter()
            .any(|x| note_n(x) == N_MARGIN_LEVEL && x.level == NoteLevel::P0)
    );
    on(&mut w, &liq("0.16", t + 31 * SEC), t + 31 * SEC);
    assert_eq!(lvl(&w), 3, "step 3");
    on(&mut w, &liq("0.170000001", t + 32 * SEC), t + 32 * SEC);
    assert_eq!(lvl(&w), 1, "step 4");
}

#[test]
fn rsk_165_deadline_on_margin() {
    let t = T0;
    let mut w = base();
    on(&mut w, &liq("0.14", t), t);
    let t1 = t + 30 * SEC - 1;
    on(&mut w, &liq("0.14", t1), t1);
    assert_eq!(lvl(&w), 2, "step 1");
    assert!(w.mo.force.is_none());
    let t2 = t + 30 * SEC;
    on(&mut w, &liq("0.14", t2), t2);
    let m = row(&w, 1, B).unwrap();
    assert_eq!(m.level, 3, "step 2");
    assert_ne!(m.mflags & mflags::DEADLINE_L3, 0);
    assert!(w.mo.force.is_some());
}

#[test]
fn rsk_166_stale_data_l1() {
    let b = || ms("0.50", "0.50", "0.10", "0.10", T0);
    let run = |id: &str, run_start: Option<i64>, s: MarginSample, want: u8| {
        let mut w = base();
        if let Some(rs) = run_start {
            w.risk.set_run_start(rs);
        }
        on(&mut w, &s, T0);
        assert_eq!(lvl(&w), want, "{id}");
    };
    run(
        "RSK-166.1",
        None,
        MarginSample {
            has_report: false,
            ..b()
        },
        1,
    );
    run(
        "RSK-166.2",
        Some(T0 - 10 * SEC),
        MarginSample {
            report_ens: T0 - 10 * SEC - 1,
            ..b()
        },
        1,
    );
    run(
        "RSK-166.3",
        None,
        MarginSample {
            report_ens: T0 - 15_000 * MS,
            ..b()
        },
        0,
    );
    run(
        "RSK-166.4",
        None,
        MarginSample {
            report_ens: T0 - 15_000 * MS - 1,
            ..b()
        },
        1,
    );
    run(
        "RSK-166.5",
        None,
        MarginSample {
            truncated: true,
            ..b()
        },
        1,
    );
    run(
        "RSK-166.6 a",
        None,
        MarginSample {
            mark_ens: T0 - 10_000 * MS,
            ..b()
        },
        0,
    );
    run(
        "RSK-166.6 b",
        None,
        MarginSample {
            mark_ens: T0 - 10_000 * MS - 1,
            ..b()
        },
        1,
    );
    run(
        "RSK-166.7",
        Some(T0 - SEC),
        MarginSample {
            mark_ens: T0 - SEC - 1,
            ..b()
        },
        1,
    );
    run(
        "RSK-166.8",
        None,
        MarginSample {
            rep_liq_dist: Rate::MAX,
            est_liq_dist: Rate::MAX,
            mark_ens: i64::MAX,
            ..b()
        },
        0,
    );
    run(
        "RSK-166.9",
        None,
        MarginSample {
            truncated: true,
            ..liq("0.10", T0)
        },
        3,
    );
}

#[test]
fn rsk_167_stale_timer() {
    let t = T0;
    let mut w = base();
    let s1 = MarginSample {
        mark_ens: t - 5 * SEC,
        ..ms("0.50", "0.50", "0.10", "0.10", t)
    };
    on(&mut w, &s1, t);
    assert_eq!(timers(&w.mo), vec![(STALE, t + 5 * SEC + 1)], "step 1");
    let s2 = MarginSample {
        report_ens: t + SEC,
        ..s1
    };
    on(&mut w, &s2, t + SEC);
    assert!(w.mo.timers.is_empty(), "step 2");
    timer(&mut w, STALE, &now_at(t + 5 * SEC + 1));
    assert_eq!(lvl(&w), 1, "step 3");
    assert_eq!(ms_input(&w.mo).iter().map(|x| x.0).collect::<Vec<_>>(), vec![1]);
    assert_eq!(ev(&w.mo), vec![(1, B, MarginLevel::L1)]);
    let s4 = liq("0.50", t + 6 * SEC);
    on(&mut w, &s4, t + 6 * SEC);
    assert_eq!(lvl(&w), 0, "step 4");
    assert!(is_clear(&w.mo));
    assert_eq!(ev(&w.mo), vec![(1, B, MarginLevel::None)]);
    assert!(timers(&w.mo).contains(&(STALE, t + 16 * SEC + 1)));
}

#[test]
fn rsk_168_account_level_and_bitmap() {
    let mut w = base();
    let apply_mo = |w: &mut W| {
        let inputs: Vec<ModeInput<'static>> = w.mo.mode.iter().copied().collect();
        for i in inputs {
            w.ok(i);
        }
    };
    w.margin(1, B, &liq("0.15", T0), T0);
    apply_mo(&mut w);
    w.margin(1, C, &liq("0.25", T0), T0);
    apply_mo(&mut w);
    let c = w.cap(CapSource::MARGIN_STRESS, sc_acct(1)).unwrap();
    assert_eq!(
        (c.level, c.params[0]),
        (2, (1u64 << B.0) | (1u64 << C.0)),
        "step 1 cap"
    );
    let now = now_at(T0);
    let v = w.risk.view(acc(1), &now);
    assert_eq!((v.margin_level, v.liq_dist), (MarginLevel::L2, r("0.15")));
    assert_eq!(v.deadline_tsc, now.tsc_of(row(&w, 1, B).unwrap().deadline_ens));
    assert!(v.open_blocked);
    let t1 = T0 + SEC;
    w.margin(1, B, &liq("0.50", t1), t1);
    assert_eq!(ms_input(&w.mo), vec![(1, 1u64 << C.0)], "step 2");
    assert_eq!(ev(&w.mo), vec![(1, B, MarginLevel::None)]);
    apply_mo(&mut w);
    w.margin(1, C, &liq("0.50", t1), t1);
    assert!(is_clear(&w.mo), "step 3");
    assert_eq!(ev(&w.mo), vec![(1, C, MarginLevel::None)]);
    apply_mo(&mut w);
    assert!(w.out.risk_events.iter().any(|(a, e)| *a == acc(1)
        && matches!(e, RiskEvent::CapCleared { source, .. } if *source == CapSource::MARGIN_STRESS)));
    w.margin(1, A, &liq("0.10", t1), t1);
    assert!(
        w.mo.mode.is_empty()
            && w.mo.events.is_empty()
            && w.mo.force.is_none()
            && w.mo.timers.is_empty()
            && w.mo.notes.is_empty(),
        "step 4"
    );
    assert!(row(&w, 1, A).is_none());
}

/// Snapshot + restore into a new run starting at engine_ns `run_start`.
fn restart(w: &mut W, run_start: i64) {
    let snap = w.snapshot();
    w.restore_from(&snap);
    w.risk.set_run_start(run_start);
}

#[test]
fn rsk_169_restart_across_deadline() {
    let t = T0;
    let mut w = base();
    on(&mut w, &liq("0.14", t), t);
    let rs = t + 10 * SEC;
    restart(&mut w, rs);
    let m = row(&w, 1, B).unwrap();
    assert_eq!((m.level, m.deadline_ens), (2, t + 30 * SEC), "step 1");
    timer(&mut w, DEADLINE, &now_run(rs, t + 29_999_999_999));
    assert_eq!(timers(&w.mo), vec![(DEADLINE, t + 30 * SEC)], "step 2");
    assert_eq!(lvl(&w), 2);
    timer(&mut w, DEADLINE, &now_run(rs, t + 30 * SEC));
    let m = row(&w, 1, B).unwrap();
    assert_eq!(m.level, 3, "step 3");
    assert_ne!(m.mflags & mflags::DEADLINE_L3, 0);
    assert!(w.mo.force.is_none() && !timers(&w.mo).iter().any(|(k, _)| *k == FORCE));
    let t31 = t + 31 * SEC;
    w.risk
        .on_margin(acc(1), B, &liq("0.14", t31), &now_run(rs, t31), &mut w.mo);
    assert_eq!(lvl(&w), 3, "step 4");
    assert_eq!(w.mo.force, force());
    assert!(timers(&w.mo).contains(&(FORCE, t + 36 * SEC)));
}

#[test]
fn rsk_170_restart_in_l3() {
    let t = T0;
    let rs = t + SEC;
    let start = || {
        let mut w = base();
        on(&mut w, &liq("0.10", t), t);
        restart(&mut w, rs);
        timer(&mut w, FORCE, &now_run(rs, t + 5 * SEC));
        w
    };
    let w = start();
    assert!(w.mo.force.is_none() && w.mo.timers.is_empty(), "RSK-170.1");
    assert_eq!(lvl(&w), 3);
    let t6 = t + 6 * SEC;
    let feed = |s: MarginSample| {
        let mut w = start();
        w.risk.on_margin(acc(1), B, &s, &now_run(rs, t6), &mut w.mo);
        w
    };
    let w = feed(liq("0.10", t6));
    assert_eq!(w.mo.force, force(), "RSK-170.2");
    assert!(timers(&w.mo).contains(&(FORCE, t + 11 * SEC)));
    let w = feed(liq("0.13", t6));
    assert_eq!(lvl(&w), 2, "RSK-170.3");
    assert!(w.mo.force.is_none());
    assert_eq!(row(&w, 1, B).unwrap().deadline_ens, t + 36 * SEC);
    let w = feed(MarginSample {
        report_ens: t - SEC,
        ..liq("0.10", t6)
    });
    assert!(w.mo.force.is_some(), "RSK-170.4");
    let w = feed(liq("0.30", t6));
    assert_eq!(lvl(&w), 0, "RSK-170.5");
    assert!(w.mo.force.is_none());
}

#[test]
fn rsk_171_distance_example() {
    for (id, y, approx, want) in [
        ("RSK-171.1", "0.034", "0.2510", 0u8),
        ("RSK-171.2", "0.035", "0.2498", 1),
        ("RSK-171.3", "0.124", "0.1508", 1),
        ("RSK-171.4", "0.125", "0.1498", 2),
        ("RSK-171.5", "0.175", "0.1009", 2),
        ("RSK-171.6", "0.176", "0.0999", 3),
    ] {
        let est = ledger_liq::est_liq_dist(r(y).0);
        assert!((est - r(approx).0).abs() < 100_000, "{id}: est {est}");
        let s = MarginSample {
            rep_liq_dist: Rate::MAX,
            est_liq_dist: Rate(est),
            ..liq("0.5", T0)
        };
        let mut w = base();
        on(&mut w, &s, T0);
        assert_eq!(lvl(&w), want, "{id}");
    }
}

// ------------------------------------------------------------------------------------ RSK-172

#[derive(Clone, Debug)]
struct Op {
    dt_ms: i64,
    on_c: bool,
    timer_only: bool,
    rep: Option<i64>,
    est: Option<i64>,
    rr: i64,
    er: i64,
    rep_age_ms: i64,
    mark_age_ms: i64,
    truncated: bool,
    has_report: bool,
}

fn op() -> impl Strategy<Value = Op> {
    let dist = prop_oneof![3 => (50_000_000i64..=400_000_000).prop_map(Some), 1 => Just(None)];
    (
        (
            0i64..=10_000,
            any::<bool>(),
            prop::bool::weighted(0.2),
            dist.clone(),
            dist,
        ),
        (
            0i64..=1_000_000_000,
            0i64..=1_000_000_000,
            0i64..=20_000,
            0i64..=20_000,
        ),
        (prop::bool::weighted(0.05), prop::bool::weighted(0.95)),
    )
        .prop_map(
            |((dt_ms, on_c, timer_only, rep, est), (rr, er, ra, ma), (truncated, has_report))| Op {
                dt_ms,
                on_c,
                timer_only,
                rep,
                est,
                rr,
                er,
                rep_age_ms: ra,
                mark_age_ms: ma,
                truncated,
                has_report,
            },
        )
}

/// Drives `w` through `ops` starting at index `from`, with the heap `heap` (engine_ns
/// deadlines); returns the Debug string of every call's `MarginOut`.
fn drive(
    w: &mut W,
    ops: &[Op],
    from: usize,
    now0: i64,
    heap: &mut Vec<(RiskTimer, i64)>,
) -> (Vec<String>, i64) {
    let mut out = Vec::new();
    let mut now = now0;
    for o in &ops[from..] {
        now += o.dt_ms * MS;
        // Fire due timers in deadline order.
        loop {
            heap.sort_by_key(|(t, d)| (*d, t.timer_id()));
            let Some(pos) = heap.iter().position(|(_, d)| *d <= now) else {
                break;
            };
            let (t, d) = heap.remove(pos);
            let n = now_at(d.max(0));
            w.risk.on_timer(t, &n, &mut w.mo);
            check_caps(&w.mo);
            heap.extend(w.mo.timers.iter().copied());
            out.push(format!("{:?}", w.mo));
        }
        if !o.timer_only {
            let s = MarginSample {
                rep_liq_dist: o.rep.map_or(Rate::MAX, Rate),
                est_liq_dist: o.est.map_or(Rate::MAX, Rate),
                reported_ratio: Rate(o.rr),
                est_ratio: Rate(o.er),
                report_ens: now - o.rep_age_ms * MS,
                mark_ens: now - o.mark_age_ms * MS,
                has_report: o.has_report,
                truncated: o.truncated,
            };
            let v = if o.on_c { C } else { B };
            w.risk.on_margin(acc(1), v, &s, &now_at(now), &mut w.mo);
            check_caps(&w.mo);
            heap.extend(w.mo.timers.iter().copied());
            out.push(format!("{:?}", w.mo));
        }
    }
    (out, now)
}

fn check_caps(mo: &MarginOut) {
    assert!(mo.mode.len() <= 4 && mo.events.len() <= 4 && mo.timers.len() <= 4 && mo.notes.len() <= 4);
}

fn rows_of(w: &W) -> Vec<MarginRow> {
    let bytes = w.snapshot();
    let rd = hft_strategy_sdk::SnapshotReader::new(&bytes);
    let s = rd.find_section(SECTION_MARGIN).unwrap().unwrap();
    let mut x = s.reader();
    (0..s.count).map(|_| x.get_row::<MarginRow>().unwrap()).collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1_000))]
    #[test]
    fn rsk_172_margin_replay_deterministic(ops in prop::collection::vec(op(), 200), cut in 1usize..200) {
        let mut a = base();
        let mut b = base();
        let (oa, _) = drive(&mut a, &ops, 0, T0, &mut Vec::new());
        let (ob, _) = drive(&mut b, &ops, 0, T0, &mut Vec::new());
        prop_assert_eq!(&oa, &ob);
        prop_assert_eq!(rows_of(&a), rows_of(&b));
        // Interrupted at `cut`: snapshot, restore, continue.
        let mut c = base();
        let mut heap = Vec::new();
        let (oc1, now) = drive(&mut c, &ops[..cut], 0, T0, &mut heap);
        let snap = c.snapshot();
        c.restore_from(&snap);
        c.risk.set_run_start(0);
        let (oc2, _) = drive(&mut c, &ops, cut, now, &mut heap);
        let mut oc = oc1;
        oc.extend(oc2);
        prop_assert_eq!(&oa, &oc);
        prop_assert_eq!(rows_of(&a), rows_of(&c));
    }
}

#[test]
fn rsk_283_stale_boundary() {
    let t = T0;
    let start = || {
        let mut w = base();
        w.margin(2, B, &liq("0.50", t), t);
        assert!(
            w.mo.timers
                .iter()
                .any(|x| *x == (RiskTimer::MarginStale { a: acc(2), v: B }, t + 10 * SEC + 1))
        );
        w
    };
    let st2 = RiskTimer::MarginStale { a: acc(2), v: B };
    let lvl2 = |w: &W| row(w, 2, B).map_or(0, |m| m.level);
    for (id, at) in [("RSK-283.1", t + 10 * SEC - 1), ("RSK-283.2", t + 10 * SEC)] {
        let mut w = start();
        w.risk.on_timer(st2, &now_at(at), &mut w.mo);
        assert_eq!(lvl2(&w), 0, "{id}");
        assert_eq!(timers(&w.mo), vec![(st2, t + 10 * SEC + 1)], "{id}");
        assert!(w.mo.mode.is_empty(), "{id}");
    }
    let mut w = start();
    w.risk.on_timer(st2, &now_at(t + 10 * SEC + 1), &mut w.mo);
    assert_eq!(lvl2(&w), 1, "RSK-283.3");
    let lv: Vec<u8> =
        w.mo.mode
            .iter()
            .filter_map(|m| match m {
                ModeInput::Auto(r) if r.source == CapSource::MARGIN_STRESS => Some(r.level),
                _ => None,
            })
            .collect();
    assert_eq!(lv, vec![1], "RSK-283.3 mode");
}
