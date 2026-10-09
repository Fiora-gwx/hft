//! EXE-180–EXE-186: two-leg lot alignment.
use crate::common;
use common::*;
use hft_event::Level;
use hft_exec::*;
use hft_strategy_sdk::{ExecSpec, FirstLeg, TargetReason, TargetReject};
use hft_types::{Notional, Qty};

/// Fixture whose SL (first) and PL (hedge) have the given lots; `min_n` = min notional (micro).
fn fx_lots(lf: &str, lh: &str, min_n: i64) -> Fx {
    let mut insts = default_insts();
    for p in insts.iter_mut() {
        if p.id == SL {
            p.lot = dec(lf, 8);
            p.min_qty = dec(lf, 8);
            p.min_notional = min_n;
        }
        if p.id == PL {
            p.lot = dec(lh, 8);
            p.min_qty = dec(lh, 8);
            p.min_notional = min_n;
        }
    }
    let mut fx = Fx::with_cfg(
        cfg(c0()),
        vec![g7(), g8(), gspec(G14, &[(SL, A), (PL, B)])],
        insts,
    );
    fx.risk(A1, |r| {
        r.max_order_notional = usd("1000000");
        r.max_hedge_notional = usd("1000000");
        r.max_reduce_notional = usd("1000000");
    });
    fx
}

/// Top-of-book depth of `i` so that `depth × 0.5 = c` (one level), quotes at `p`.
fn depth_for(fx: &mut Fx, i: hft_types::InstId, c: &str, bid: &str, ask: &str) {
    let qty = Qty(q(c).0 * 2);
    hft_strategy_sdk::testing::MockViews::bbo(&mut fx.v.m, i, (px(bid), qty), (px(ask), qty));
    let b = [Level { px: px(bid), qty }];
    let a = [Level { px: px(ask), qty }];
    hft_strategy_sdk::testing::MockViews::book(&mut fx.v.m, i, &b, &a);
}

fn pair14(s: Qty, p: Qty, num: u32, den: u32, chunk: &str) -> hft_strategy_sdk::TargetIntent {
    let mut pp = pair0();
    pp.ratio_num = num;
    pp.ratio_den = den;
    pp.first_leg = FirstLeg::Fixed;
    pp.first_leg_ix = 0;
    pp.slice.max_chunk_notional = usd(chunk);
    pp.slice.min_chunk_notional = usd("0");
    pp.slice.depth_levels = 1;
    target(A1, G14, &[(0, s), (1, p)], ExecSpec::PairOnFill(pp))
}

#[test]
fn exe_180_first_leg_pair_lot() {
    for (row, lf, lh, num, den, c, pl, want) in [
        ("EXE-180.1", "0.001", "0.01", 1u32, 1u32, "0.0377", "0.01", "0.03"),
        ("EXE-180.2", "0.01", "0.001", 1, 1, "0.0377", "0.01", "0.03"),
        ("EXE-180.3", "0.003", "0.002", 1, 1, "0.0377", "0.006", "0.036"),
        ("EXE-180.4", "0.01", "0.1", 2, 1, "0.377", "0.05", "0.35"),
        ("EXE-180.5", "0.01", "0.1", 1, 3, "0.977", "0.3", "0.9"),
        ("EXE-180.6", "0.001", "0.001", 1, 1, "0.0377", "0.001", "0.037"),
    ] {
        let mut fx = fx_lots(lf, lh, 1);
        depth_for(&mut fx, SL, c, "99.99", "100.00");
        depth_for(&mut fx, PL, "1000", "100.00", "100.01");
        let tg = pair14(
            q("100"),
            Qty(-q("100").0 * i64::from(num) / i64::from(den)),
            num,
            den,
            "1000000",
        );
        let r = fx.go(&tg);
        assert_eq!(fx.cell(A1, G14).pair_lot, q(pl), "{row}");
        assert_eq!(r.orders[0].intent.qty, q(want), "{row}");
    }
}

#[test]
fn exe_181_pair_min() {
    for (row, c, want) in [
        ("EXE-181.1", "0.053", None),
        ("EXE-181.2", "0.054", Some("0.054")),
        ("EXE-181.3", "0.0599", Some("0.054")),
        ("EXE-181.4", "0.060", Some("0.060")),
    ] {
        let mut insts = default_insts();
        for p in insts.iter_mut() {
            if p.id == SL {
                p.lot = dec("0.003", 8);
                p.min_qty = dec("0.003", 8);
            }
            if p.id == PL {
                p.lot = dec("0.002", 8);
                p.min_qty = dec("0.01", 8);
            }
        }
        let mut fx = Fx::with_cfg(
            cfg(c0()),
            vec![g7(), g8(), gspec(G14, &[(SL, A), (PL, B)])],
            insts,
        );
        depth_for(&mut fx, SL, c, "99.99", "100.00");
        depth_for(&mut fx, PL, "1000", "100.00", "100.01");
        let r = fx.go(&pair14(q("10"), q("-10"), 1, 1, "1000"));
        match want {
            Some(x) => assert_eq!(r.orders[0].intent.qty, q(x), "{row}"),
            None => {
                assert!(r.orders.is_empty(), "{row}");
                fx.at(2000);
                fx.touch(SL);
                assert!(fx.adv().states().contains(&(2, 3, pack(2, 3, 0))), "{row}");
            }
        }
    }
}

#[test]
fn exe_182_one_pair_lot_floor() {
    for (row, cap, order) in [("EXE-182.1", "2000", true), ("EXE-182.2", "1100", false)] {
        let mut fx = fx_lots("0.003", "0.002", 5_000_000);
        fx.risk(A1, |r| r.max_order_notional = usd(cap));
        depth_for(&mut fx, SL, "10", "199999.99", "200000.00");
        depth_for(&mut fx, PL, "10", "200000.00", "200000.01");
        fx.mark(SL, "200000");
        fx.mark(PL, "200000");
        inject(&mut fx, &pair14(q("1"), q("-1"), 1, 1, "1000"));
        fx.touch(SL);
        let r = fx.adv();
        assert_eq!(!r.orders.is_empty(), order, "{row}");
        if order {
            assert_eq!(r.orders[0].intent.qty, q("0.006"), "{row}");
        }
    }
}

#[test]
fn exe_183_lot_mismatch_table() {
    type Edit = fn(&mut Fx, &mut hft_strategy_sdk::TargetIntent);
    let rows: [(&str, Edit, Result<(), TargetReject>); 10] = [
        ("EXE-183.1", |fx, _| fx.mark(SL, "100000.00"), Ok(())),
        (
            "EXE-183.2",
            |fx, _| fx.mark(SL, "100000.01"),
            Err(TargetReject::LotMismatch),
        ),
        (
            "EXE-183.3",
            |fx, t| {
                fx.mark(SL, "100000.01");
                t.reason = TargetReason::Manual
            },
            Err(TargetReject::LotMismatch),
        ),
        (
            "EXE-183.4",
            |fx, t| {
                fx.mark(SL, "100000.01");
                t.reason = TargetReason::Exit
            },
            Ok(()),
        ),
        (
            "EXE-183.5",
            |fx, t| {
                fx.mark(SL, "100000.01");
                fx.legs(A1, G14, &["2", "-2"]);
                t.legs[0].qty = q("1");
                t.legs[1].qty = q("-1");
            },
            Ok(()),
        ),
        (
            "EXE-183.6",
            |fx, t| {
                fx.mark(SL, "100000.01");
                t.reason = TargetReason::MarginStress
            },
            Ok(()),
        ),
        (
            "EXE-183.7",
            |fx, t| {
                fx.mark(SL, "100000.01");
                t.reason = TargetReason::Hard
            },
            Ok(()),
        ),
        (
            "EXE-183.8",
            |fx, _| {
                fx.mark(SL, "0");
                hft_strategy_sdk::testing::MockViews::bbo(
                    &mut fx.v.m,
                    SL,
                    (px("100000.00"), q("1")),
                    (px("100000.02"), q("1")),
                );
            },
            Err(TargetReject::LotMismatch),
        ),
        (
            "EXE-183.9",
            |fx, _| {
                fx.mark(SL, "0");
                hft_strategy_sdk::testing::MockViews::bbo(
                    &mut fx.v.m,
                    SL,
                    (hft_types::Px(0), Qty(0)),
                    (hft_types::Px(0), Qty(0)),
                );
            },
            Ok(()),
        ),
        (
            "EXE-183.10",
            |fx, t| {
                fx.mark(SL, "333333.34");
                fx.risk(A1, |r| r.max_order_notional = usd("2000"));
                if let ExecSpec::PairOnFill(p) = &mut t.exec {
                    p.slice.max_chunk_notional = usd("2500");
                }
            },
            Err(TargetReject::LotMismatch),
        ),
    ];
    for (row, edit, want) in rows {
        let mut fx = fx_lots("0.003", "0.002", 5_000_000);
        fx.risk(A1, |r| r.max_order_notional = usd("2000"));
        let mut tg = pair14(q("1"), q("-1"), 1, 1, "600");
        edit(&mut fx, &mut tg);
        let (res, r) = fx.set(I1, &tg);
        assert_eq!(res, want, "{row}");
        if want.is_err() {
            assert!(r.has_dec(4, pack(1, 10, 0)), "{row}");
        }
    }
    let mut fx = fx_lots("0.001", "0.01", 5_000_000);
    fx.mark(SL, "10000000");
    assert_eq!(
        fx.set(I1, &pair14(q("1"), q("-1"), 1, 1, "600")).0,
        Ok(()),
        "EXE-183.11"
    );
}

#[test]
fn exe_184_pair_lot_overflow() {
    let mut fx = fx_lots("0.003", "0.002", 5_000_000);
    {
        let c = fx.cm(A1, G14);
        c.legs[0].lot = Qty(4_611_686_018_427_387_903);
        c.legs[1].lot = Qty(4_611_686_018_427_387_902);
    }
    assert_eq!(
        fx.set(I1, &pair14(q("1"), q("-1"), 1, 1, "600")).0,
        Err(TargetReject::LotMismatch)
    );
    let mut tg = pair14(q("0"), q("0"), 1, 1, "600");
    tg.reason = TargetReason::Exit;
    assert_eq!(fx.set(I1, &tg).0, Ok(()));
}

#[test]
fn exe_185_coarse_lot_for_reduce() {
    for (row, reason, chunk, want) in [
        ("EXE-185.1", TargetReason::Exit, "500", "0.015"),
        ("EXE-185.2", TargetReason::Hard, "500", "0.015"),
        ("EXE-185.3", TargetReason::Exit, "700", "0.012"),
    ] {
        let mut fx = fx_lots("0.003", "0.002", 1);
        fx.mark(SL, "100000");
        fx.mark(PL, "100000");
        fx.legs(A1, G14, &["1", "-1"]);
        fx.spot(A1, A, G14, "1", "1");
        depth_for(&mut fx, SL, "0.0167", "99.99", "100.00");
        depth_for(&mut fx, PL, "0.0167", "100.00", "100.01");
        let mut tg = pair14(q("0"), q("0"), 1, 1, chunk);
        tg.reason = reason;
        let r = fx.go(&tg);
        assert_eq!(r.orders[0].intent.qty, q(want), "{row}");
    }
}

#[test]
fn exe_186_pair_lot_after_restore() {
    let mut fx = fx_lots("0.003", "0.002", 1);
    depth_for(&mut fx, SL, "0.0377", "99.99", "100.00");
    depth_for(&mut fx, PL, "1000", "100.00", "100.01");
    fx.mark(SL, "100");
    fx.set(I1, &pair14(q("10"), q("-10"), 1, 1, "1000000")).0.unwrap();
    assert_eq!(fx.cell(A1, G14).pair_lot, q("0.006"));
    let bytes = fx.snap();
    let gt = hft_strategy_sdk::GroupTable::new(&fx.groups, 256);
    let mut rdr = hft_strategy_sdk::SnapshotReader::new(&bytes);
    let mut ex2 = Exec::restore(
        &mut rdr,
        &cfg(c0()),
        &caps(256),
        &gt,
        &hft_strategy_sdk::RefView::new(&fx.rd, A1),
    )
    .unwrap();
    assert_eq!(ex2.cell(A1, G14).unwrap().pair_lot, q("0.006"));
    let now = fx.now();
    let mut o2 = ExecOut::with_caps(OutCaps::all(64));
    ex2.advance(&fx.v, now, &mut o2);
    let r1 = fx.adv();
    let q1 = r1.orders[0].intent.qty;
    let q2 = o2.orders.as_slice()[0].intent.qty;
    assert_eq!(q1, q2);
    assert_eq!(q1.0 % q("0.006").0, 0);
    let _ = Notional(0);
}
