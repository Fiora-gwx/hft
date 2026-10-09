//! `DirtyFilter` (TESTS.md SLB-033–SLB-035, SLB-038).

use hft_strat_lib::{DirtyFilter, DirtyParams, DirtyParamsTsc, DirtyReason, Freshness, LegFresh};
use hft_strategy_sdk::BboView;
use hft_types::{Rate, Tsc};

const NOW: u64 = 100_000_000_000;

fn params() -> DirtyParams {
    DirtyParams {
        stale_fast_ms: 2_000,
        stale_slow_ms: 1_000,
        max_abs_spread: Rate(10_000_000),
        junk_after: 3,
    }
}

fn healthy() -> LegFresh {
    LegFresh {
        live_tsc: Tsc(NOW),
        conn_ok: true,
        resync: false,
    }
}

/// Base input: two healthy legs, spread 0, streak 0.
#[derive(Clone, Copy)]
struct In {
    spread: i64,
    fr: Freshness,
}

fn base() -> In {
    In {
        spread: 0,
        fr: Freshness {
            fast: healthy(),
            slow: healthy(),
            no_quote_streak: 0,
        },
    }
}

fn check(i: &In, p: &DirtyParamsTsc) -> Result<(), DirtyReason> {
    DirtyFilter::check(Rate(i.spread), &i.fr, Tsc(NOW), p)
}

#[test]
fn slb_033_check_reasons() {
    use DirtyReason::*;
    let p = params().tsc(3_000_000);
    assert_eq!((p.stale_fast, p.stale_slow), (6_000_000_000, 3_000_000_000));
    type Edit = fn(&mut In);
    let rows: &[(&str, Edit, Result<(), DirtyReason>)] = &[
        ("SLB-033.1", |_| {}, Ok(())),
        ("SLB-033.2", |i| i.fr.fast.conn_ok = false, Err(Stale)),
        (
            "SLB-033.3",
            |i| i.fr.fast.live_tsc = Tsc(NOW - 6_000_000_000),
            Ok(()),
        ),
        (
            "SLB-033.4",
            |i| i.fr.fast.live_tsc = Tsc(NOW - 6_000_000_001),
            Err(Stale),
        ),
        (
            "SLB-033.5",
            |i| i.fr.fast.live_tsc = Tsc(NOW - 5_999_999_999),
            Ok(()),
        ),
        (
            "SLB-033.6",
            |i| i.fr.slow.live_tsc = Tsc(NOW - 3_000_000_001),
            Err(Stale),
        ),
        (
            "SLB-033.7",
            |i| i.fr.slow.live_tsc = Tsc(NOW - 3_000_000_000),
            Ok(()),
        ),
        ("SLB-033.8", |i| i.fr.slow.conn_ok = false, Err(Stale)),
        ("SLB-033.9", |i| i.fr.fast.resync = true, Err(Resync)),
        ("SLB-033.10", |i| i.fr.slow.resync = true, Err(Resync)),
        ("SLB-033.11", |i| i.spread = 10_000_000, Ok(())),
        ("SLB-033.12", |i| i.spread = 10_000_001, Err(AbsSpread)),
        ("SLB-033.13", |i| i.spread = -10_000_001, Err(AbsSpread)),
        ("SLB-033.14", |i| i.spread = 9_999_999, Ok(())),
        ("SLB-033.15", |i| i.fr.no_quote_streak = 2, Ok(())),
        ("SLB-033.16", |i| i.fr.no_quote_streak = 3, Err(Junk)),
        ("SLB-033.17", |i| i.fr.no_quote_streak = 4, Err(Junk)),
        (
            "SLB-033.18",
            |i| {
                i.fr.fast.conn_ok = false;
                i.fr.slow.resync = true;
                i.spread = 10_000_001;
                i.fr.no_quote_streak = 3;
            },
            Err(Stale),
        ),
        (
            "SLB-033.19",
            |i| {
                i.fr.fast.resync = true;
                i.spread = 10_000_001;
                i.fr.no_quote_streak = 3;
            },
            Err(Resync),
        ),
        (
            "SLB-033.20",
            |i| {
                i.spread = 10_000_001;
                i.fr.no_quote_streak = 3;
            },
            Err(AbsSpread),
        ),
        ("SLB-033.21", |i| i.fr.fast.live_tsc = Tsc(NOW + 1_000), Ok(())),
    ];
    for &(row, edit, want) in rows {
        let mut i = base();
        edit(&mut i);
        assert_eq!(check(&i, &p), want, "{row}");
    }
}

#[test]
fn slb_034_zero_disables() {
    type Edit = fn(&mut DirtyParams, &mut In);
    let rows: &[(&str, Edit, Result<(), DirtyReason>)] = &[
        (
            "SLB-034.1",
            |p, i| {
                p.stale_fast_ms = 0;
                i.fr.fast.live_tsc = Tsc(0);
            },
            Ok(()),
        ),
        (
            "SLB-034.2",
            |p, i| {
                p.stale_fast_ms = 0;
                i.fr.fast.conn_ok = false;
            },
            Err(DirtyReason::Stale),
        ),
        (
            "SLB-034.3",
            |p, i| {
                p.stale_slow_ms = 0;
                i.fr.slow.live_tsc = Tsc(0);
            },
            Ok(()),
        ),
        (
            "SLB-034.4",
            |p, i| {
                p.max_abs_spread = Rate(0);
                i.spread = i64::MAX;
            },
            Ok(()),
        ),
        (
            "SLB-034.5",
            |p, i| {
                p.junk_after = 0;
                i.fr.no_quote_streak = 65_535;
            },
            Ok(()),
        ),
        (
            "SLB-034.6",
            |p, i| {
                p.junk_after = 1;
                i.fr.no_quote_streak = 1;
            },
            Err(DirtyReason::Junk),
        ),
    ];
    for &(row, edit, want) in rows {
        let mut p = params();
        let mut i = base();
        edit(&mut p, &mut i);
        assert_eq!(check(&i, &p.tsc(3_000_000)), want, "{row}");
    }
}

#[test]
fn slb_035_legfresh_ctors() {
    let b1 = BboView {
        conn_live_tsc: Tsc(5_000),
        conn_up: true,
        resync: false,
        rx_tsc: Tsc(1),
        ..BboView::EMPTY
    };
    let b2 = BboView {
        rx_tsc: Tsc(4_999),
        ..b1
    };
    let want = LegFresh {
        live_tsc: Tsc(5_000),
        conn_ok: true,
        resync: false,
    };
    assert_eq!(LegFresh::from_bbo(&b1), want);
    assert_eq!(LegFresh::from_bbo(&b2), want);
    assert_eq!(
        LegFresh::from_quote(Tsc(7_000), false, true),
        LegFresh {
            live_tsc: Tsc(7_000),
            conn_ok: false,
            resync: true
        }
    );
}

#[test]
fn slb_038_params_tsc() {
    let a = params().tsc(3_000_000);
    assert_eq!(
        a,
        DirtyParamsTsc {
            stale_fast: 6_000_000_000,
            stale_slow: 3_000_000_000,
            max_abs_spread: Rate(10_000_000),
            junk_after: 3
        },
        "SLB-038.1"
    );
    let b = DirtyParams {
        stale_fast_ms: u32::MAX,
        ..params()
    }
    .tsc(4_000_000);
    assert_eq!(b.stale_fast, 17_179_869_180_000_000, "SLB-038.2");
    let c = DirtyParams {
        stale_fast_ms: 0,
        ..params()
    }
    .tsc(3_000_000);
    assert_eq!(c.stale_fast, 0, "SLB-038.3");
}
