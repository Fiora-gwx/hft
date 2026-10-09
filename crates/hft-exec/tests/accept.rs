//! EXE-042, EXE-400–EXE-405, EXE-507: target validation, identical targets, replacement,
//! `clear_account`, reduce-only commands.
use crate::common;
use common::*;
use hft_exec::*;
use hft_strategy_sdk::{
    BrokenReason, ExecSpec, ExecUpdateKind, FirstLeg, MakerParams, StuckReason, TargetReason, TargetReject,
    TargetState, Urgency,
};
use hft_types::{Rate, Tsc};

fn with_pair(f: impl FnOnce(&mut hft_strategy_sdk::PairParams)) -> hft_strategy_sdk::TargetIntent {
    let mut p = pair0();
    f(&mut p);
    target(A1, G7, &[(0, q("10")), (1, q("-10"))], ExecSpec::PairOnFill(p))
}

#[test]
fn exe_042_spec_param_ranges() {
    type Edit = fn(&mut hft_strategy_sdk::PairParams);
    let rows: [(&str, Edit, Result<(), TargetReject>); 11] = [
        (
            "EXE-042.1",
            |p| p.slice.depth_levels = 0,
            Err(TargetReject::SpecInvalid),
        ),
        ("EXE-042.2", |p| p.slice.depth_levels = 1, Ok(())),
        ("EXE-042.3", |p| p.slice.depth_levels = 10, Ok(())),
        (
            "EXE-042.4",
            |p| p.slice.depth_levels = 11,
            Err(TargetReject::SpecInvalid),
        ),
        (
            "EXE-042.5",
            |p| p.slice.depth_frac = Rate(0),
            Err(TargetReject::SpecInvalid),
        ),
        ("EXE-042.6", |p| p.slice.depth_frac = Rate(1_000_000_000), Ok(())),
        (
            "EXE-042.7",
            |p| p.slice.depth_frac = Rate(1_000_000_001),
            Err(TargetReject::SpecInvalid),
        ),
        ("EXE-042.8", |p| p.ratio_num = 0, Err(TargetReject::SpecInvalid)),
        ("EXE-042.9", |p| p.ratio_den = 0, Err(TargetReject::SpecInvalid)),
        (
            "EXE-042.10",
            |p| {
                p.first_leg = FirstLeg::Fixed;
                p.first_leg_ix = 1
            },
            Ok(()),
        ),
        (
            "EXE-042.11",
            |p| {
                p.first_leg = FirstLeg::Fixed;
                p.first_leg_ix = 2
            },
            Err(TargetReject::SpecInvalid),
        ),
    ];
    for (row, f, want) in rows {
        let mut fx = Fx::new();
        let (res, r) = fx.set(I1, &with_pair(f));
        assert_eq!(res, want, "{row}");
        if let Err(e) = want {
            assert!(r.has_dec(4, pack(1, e.code(), 0)), "{row}");
        }
    }
    // .10: first leg 1.
    let mut fx = Fx::new();
    let r = fx.go(&with_pair(|p| {
        p.first_leg = FirstLeg::Fixed;
        p.first_leg_ix = 1
    }));
    assert_eq!(r.dec(5)[0].first_leg_ix, 1, "EXE-042.10");
}

#[test]
fn exe_400_reject_codes() {
    let g14 = gspec(G14, &[(SL, A), (PL, B)]);
    let ok = t0("10", "-10");
    let mk = |f: fn(&mut Fx) -> (hft_types::StrategyId, hft_strategy_sdk::TargetIntent)| f;
    type Row = (
        &'static str,
        fn(&mut Fx) -> (hft_types::StrategyId, hft_strategy_sdk::TargetIntent),
        TargetReject,
    );
    let rows: [Row; 12] = [
        ("EXE-400.1", |_| (I2, t0("10", "-10")), TargetReject::NotOwner),
        (
            "EXE-400.2",
            |_| (I1, target(A1, G9, &[(0, q("1"))], ExecSpec::TakerSlice(slice0()))),
            TargetReject::NotOwner,
        ),
        (
            "EXE-400.3",
            |_| {
                (
                    I1,
                    target(
                        A1,
                        G7,
                        &[(2, q("1")), (1, q("-1"))],
                        ExecSpec::PairOnFill(pair0()),
                    ),
                )
            },
            TargetReject::BadLegs,
        ),
        (
            "EXE-400.4",
            |_| {
                (
                    I1,
                    target(A1, G7, &[(0, q("1")), (0, q("2"))], ExecSpec::PairOnFill(pair0())),
                )
            },
            TargetReject::BadLegs,
        ),
        (
            "EXE-400.5",
            |_| {
                (
                    I1,
                    with_pair(|p| {
                        p.first_leg = FirstLeg::Fixed;
                        p.first_leg_ix = 5
                    }),
                )
            },
            TargetReject::SpecInvalid,
        ),
        (
            "EXE-400.6",
            |_| {
                let m = MakerParams {
                    offset: Rate(0),
                    requote: Rate(0),
                    max_rest_ms: 0,
                    _pad0: 0,
                    chunk_notional: usd("0"),
                    hedge: pair0(),
                };
                (
                    I1,
                    target(A1, G7, &[(0, q("1")), (1, q("-1"))], ExecSpec::MakerFirst(m)),
                )
            },
            TargetReject::AlgoUnsupported,
        ),
        ("EXE-400.7", |_| (I1, t0("-1.00", "0")), TargetReject::SpotShort),
        (
            "EXE-400.8",
            |fx| {
                fx.cm(A1, G7).view.state = TargetState::Broken(BrokenReason::LinkBroken);
                (I1, t0("10", "-10"))
            },
            TargetReject::CellBroken,
        ),
        (
            "EXE-400.9",
            |fx| {
                fx.acct(A1).force_active = true;
                (I1, t0("10", "-10"))
            },
            TargetReject::ForcedReduceActive,
        ),
        (
            "EXE-400.10",
            |_| {
                (
                    I1,
                    target(
                        A1,
                        G12,
                        &[(0, q("1")), (1, q("-1")), (2, q("1"))],
                        ExecSpec::PairOnFill(pair0()),
                    ),
                )
            },
            TargetReject::TooManyLegs,
        ),
        (
            "EXE-400.11",
            |fx| {
                fx.acct(A1).flatten_lock = true;
                (I1, t0("10", "-10"))
            },
            TargetReject::FlattenLocked,
        ),
        (
            "EXE-400.12",
            |fx| {
                MockMark::set(fx, SL, "100000.01");
                let mut p = pair0();
                p.slice.max_chunk_notional = usd("600");
                (
                    I1,
                    target(A1, G14, &[(0, q("1")), (1, q("-1"))], ExecSpec::PairOnFill(p)),
                )
            },
            TargetReject::LotMismatch,
        ),
    ];
    let _ = mk;
    for (row, f, want) in rows {
        let mut fx = Fx::with(c0(), vec![g7(), g8(), g14, gspec(G12, &[(S, A), (P, B), (Q, C)])]);
        // .13: an existing target stays unchanged.
        inject(&mut fx, &t1("-3"));
        let before = *fx.ex.view(A1, G8).unwrap();
        let (owner, tg) = f(&mut fx);
        let (res, r) = fx.set(owner, &tg);
        assert_eq!(res, Err(want), "{row}");
        assert!(r.has_dec(4, pack(1, want.code(), 0)), "{row}");
        assert_eq!(*fx.ex.view(A1, G8).unwrap(), before, "EXE-400.13");
    }
    let _ = ok;
}

struct MockMark;
impl MockMark {
    fn set(fx: &mut Fx, i: hft_types::InstId, p: &str) {
        hft_strategy_sdk::testing::MockViews::mark(&mut fx.v.m, i, px(p));
    }
}

#[test]
fn exe_401_identical_target() {
    let mut fx = Fx::new();
    inject(&mut fx, &t1("-10"));
    {
        let c = fx.cm(A1, G8);
        c.view.filled[0] = q("-3.0");
        c.fail_streak = 2;
        c.nofill_streak = 1;
    }
    for _ in 0..1000 {
        let (res, r) = fx.set(I1, &t1("-10"));
        assert_eq!(res, Ok(()));
        assert!(r.decisions.is_empty() && r.timers.is_empty());
    }
    let c = fx.cell(A1, G8);
    assert_eq!(
        (c.view.filled[0], c.fail_streak, c.nofill_streak),
        (q("-3.0"), 2, 1)
    );
}

#[test]
fn exe_402_any_field_replaces() {
    type Edit = fn(&mut hft_strategy_sdk::TargetIntent);
    let rows: [(&str, Edit); 6] = [
        ("EXE-402.1", |t| t.legs[0].qty = q("-10.1")),
        ("EXE-402.2", |t| t.reason = TargetReason::Exit),
        ("EXE-402.3", |t| t.urgency = Urgency::Urgent),
        ("EXE-402.4", |t| t.band = rate("0.02")),
        ("EXE-402.5", |t| {
            if let ExecSpec::TakerSlice(p) = &mut t.exec {
                p.cooldown_ms = 301
            }
        }),
        ("EXE-402.6", |t| t.tag = 0x11),
    ];
    for (row, f) in rows {
        let mut fx = Fx::new();
        inject(&mut fx, &t1("-10"));
        let mut tg = t1("-10");
        f(&mut tg);
        let (res, r) = fx.set(I1, &tg);
        assert_eq!(res, Ok(()), "{row}");
        assert_eq!(r.dec(2).len(), 1, "{row}");
    }
}

#[test]
fn exe_403_replace_effects() {
    let mut fx = Fx::new();
    let r = fx.go(&t1("-10"));
    fx.placed(&r.orders[0]);
    {
        let c = fx.cm(A1, G8);
        c.view.filled[0] = q("-3.0");
        c.fail_streak = 1;
        c.nofill_streak = 2;
    }
    let (res, r) = fx.set(I1, &t1("-12"));
    assert_eq!(res, Ok(()));
    assert_eq!(r.dec(2).len(), 1);
    let c = fx.cell(A1, G8);
    assert_eq!((c.view.filled[0], c.fail_streak, c.nofill_streak), (q("0"), 0, 0));
    assert_eq!(c.view.state, TargetState::Working);
    assert!(fx.ex.has_pending());
    assert!(r.cancels.is_empty());
}

#[test]
fn exe_404_exit_validated() {
    let mut fx = Fx::new();
    let mut tg = t0("0", "0");
    tg.reason = TargetReason::Exit;
    assert_eq!(fx.set(I1, &tg).0, Ok(()), "EXE-404.1");
    let mut tg = t0("-1.00", "0");
    tg.reason = TargetReason::Exit;
    assert_eq!(fx.set(I1, &tg).0, Err(TargetReject::SpotShort), "EXE-404.2");
    let mut tg = t0("0", "0");
    tg.reason = TargetReason::Exit;
    tg.tag = 9;
    assert_eq!(fx.set(I2, &tg).0, Err(TargetReject::NotOwner), "EXE-404.3");
}

#[test]
fn exe_405_clear_account() {
    let groups = vec![
        g7(),
        g8(),
        gspec(G20, &[(S, A), (P, B)]),
        gspec(G21, &[(S, A), (P, B)]),
        gspec(G22, &[(S, A), (P, B)]),
    ];
    let mut fx = Fx::with(c0(), groups);
    for g in [G20, G21, G22] {
        inject(
            &mut fx,
            &target(S1, g, &[(0, q("1")), (1, q("-1"))], ExecSpec::PairOnFill(pair0())),
        );
    }
    {
        let c = fx.cm(S1, G20);
        c.fix.count = 1;
        c.fix.first_tsc = Tsc(5);
        c.ext_deadline = t(6000);
        c.hold = MANUAL_HOLD;
    }
    {
        let c = fx.cm(S1, G21);
        c.view.state = TargetState::Broken(BrokenReason::LinkBroken);
        c.auto_fix.phase = AutoFixPhase::Primed as u8;
        c.auto_fix.due_tsc = t(120_000);
    }
    fx.cm(S1, G22).serial = 1;
    fx.acct(S1).flatten_lock = true;
    let now = fx.now();
    fx.ex.clear_account(S1, now, &mut fx.out);
    assert!(fx.out.is_empty());
    let r = fx.adv();
    let g20: Vec<_> = r.decisions.iter().filter(|d| d.group == G20).copied().collect();
    assert!(
        g20.iter()
            .any(|d| d.action == 6 && d.state_from == 2 && d.state_to == 1)
    );
    assert!(g20.iter().any(|d| d.action == 3 && d.eflags == 2));
    assert!(g20.iter().any(|d| d.action == 9 && d.detail == pack(6, 0, 5)));
    let dones: Vec<_> = r
        .notify
        .iter()
        .filter(|n| {
            n.u.group == G20
                && n.u.kind
                    == ExecUpdateKind::Done {
                        cleared_by_engine: true,
                    }
        })
        .collect();
    assert_eq!(dones.len(), 1);
    let c = fx.cell(S1, G20);
    assert_eq!((c.fix, c.ext_deadline, c.hold), (FixState::ZERO, Tsc(0), 0));
    let hn = r.notes_n(hft_exec::decision::note::EXEC_MANUAL_HOLD);
    assert!(
        hn.iter()
            .any(|n| n.level == hft_event::NoteLevel::Info && n.v[2] == 4)
    );
    let c21 = fx.cell(S1, G21);
    assert!(matches!(c21.view.state, TargetState::Broken(_)));
    assert_eq!(c21.auto_fix.phase, 0);
    assert!(!c21.has_target);
    assert_eq!(fx.cell(S1, G22).serial, 0);
    assert!(r.orders.is_empty());
    assert!(fx.ex.acct_state(S1).unwrap().flatten_lock);
}

#[test]
fn exe_507_reduce_only_cmd_target() {
    let base = || {
        let mut fx = Fx::new();
        inject(&mut fx, &t0("2.00", "-2.0"));
        fx.legs(A1, G7, &["2.00", "-2.0"]);
        fx.cm(A1, G7).view.state = TargetState::Idle;
        fx
    };
    type Row<'a> = (&'a str, &'a str, &'a str, bool, Result<(), TargetReject>);
    let rows: [Row; 6] = [
        ("EXE-507.1", "1.00", "-1.0", true, Ok(())),
        ("EXE-507.2", "0", "0", true, Ok(())),
        (
            "EXE-507.4",
            "2.01",
            "-2.0",
            true,
            Err(TargetReject::ReduceOnlyCmd),
        ),
        ("EXE-507.5", "1.00", "1.0", true, Err(TargetReject::ReduceOnlyCmd)),
        (
            "EXE-507.6",
            "1.00",
            "-2.1",
            true,
            Err(TargetReject::ReduceOnlyCmd),
        ),
        ("EXE-507.8", "2.01", "-2.0", false, Ok(())),
    ];
    for (row, s, p, ro, want) in rows {
        let mut fx = base();
        let tg = t0(s, p);
        let (res, r) = fx.set_ro(I1, &tg, ro);
        assert_eq!(res, want, "{row}");
        match want {
            Ok(()) => assert_eq!(fx.ex.view(A1, G7).unwrap().intent, tg, "{row}"),
            Err(_) => {
                assert!(r.has_dec(4, 1 | 11 << 8), "{row}");
                assert_eq!(fx.ex.view(A1, G7).unwrap().intent, t0("2.00", "-2.0"), "{row}");
                assert!(r.orders.is_empty());
            }
        }
    }
    // .3 identical → no Wake.
    let mut fx = base();
    let (res, r) = fx.set_ro(I1, &t0("2.00", "-2.0"), true);
    assert_eq!(res, Ok(()), "EXE-507.3");
    assert!(r.timers.is_empty() && r.decisions.is_empty(), "EXE-507.3");
    // .7 g8 flat.
    let mut fx = base();
    assert_eq!(
        fx.set_ro(I1, &t1("-0.1"), true).0,
        Err(TargetReject::ReduceOnlyCmd),
        "EXE-507.7"
    );
    // .9 Broken first.
    let mut fx = base();
    fx.cm(A1, G7).view.state = TargetState::Broken(BrokenReason::LinkBroken);
    assert_eq!(
        fx.set_ro(I1, &t0("1.00", "-1.0"), true).0,
        Err(TargetReject::CellBroken),
        "EXE-507.9"
    );
    // .10 clear_target not restricted.
    let mut fx = base();
    let r = fx.clear(A1, G7);
    assert_eq!(r.dec(3).len(), 1, "EXE-507.10");
    let _ = StuckReason::Shadow;
}
