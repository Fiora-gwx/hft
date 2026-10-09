//! EXE-145–EXE-148: TakerSlice.
mod common;
use common::*;
use hft_exec::*;
use hft_strategy_sdk::ExecSpec;
use hft_types::{Side, Tif};

fn tk(diff: &str, f: impl FnOnce(&mut hft_strategy_sdk::TakerSliceParams)) -> hft_strategy_sdk::TargetIntent {
    let mut tg = t1(&negate(diff));
    if let ExecSpec::TakerSlice(p) = &mut tg.exec {
        p.max_chunk_notional = usd("2000");
        p.depth_frac = rate("0.5");
        p.depth_levels = 5;
        f(p);
    }
    tg
}

#[test]
fn exe_145_chunk_formula() {
    type Edit = fn(&mut hft_strategy_sdk::TakerSliceParams);
    type Row<'a> = (&'a str, &'a str, Edit, bool, &'a str, Option<&'a str>);
    let rows: [Row; 6] = [
        ("EXE-145.1", "10.0", |_| {}, false, "10.0", None),
        (
            "EXE-145.2",
            "100.0",
            |p| p.depth_frac = rate("0.05"),
            false,
            "12.5",
            None,
        ),
        ("EXE-145.3", "100.0", |_| {}, false, "20.1", None),
        ("EXE-145.4", "3.37", |_| {}, false, "3.3", None),
        ("EXE-145.5", "100.0", |p| p.depth_levels = 2, true, "50.0", None),
        (
            "EXE-145.6",
            "100.0",
            |p| p.max_cross = rate("0.001"),
            true,
            "25.0",
            Some("99.71"),
        ),
    ];
    for (row, diff, edit, big, want, wpx) in rows {
        let mut fx = Fx::new();
        fx.risk(A1, |r| {
            r.max_order_notional = usd(if big { "100000" } else { "2000" })
        });
        let tg = tk(diff, |p| {
            edit(p);
            if big {
                p.max_chunk_notional = usd("100000");
            }
        });
        let o = fx.go(&tg).orders[0];
        assert_eq!(o.intent.qty, q(want), "{row}");
        if let Some(x) = wpx {
            assert_eq!(o.intent.px, px(x), "{row}");
        }
    }
}

#[test]
fn exe_146_min_chunk_rules() {
    let min50 = |p: &mut hft_strategy_sdk::TakerSliceParams| p.min_chunk_notional = usd("50");
    // .1
    let mut fx = Fx::new();
    let mut tg = tk("0.4", min50);
    tg.band = rate("0");
    assert_eq!(fx.go(&tg).orders[0].intent.qty, q("0.4"), "EXE-146.1");
    // .2
    let mut fx = Fx::new();
    let mut tg = tk("0.05", min50);
    tg.band = rate("0");
    assert!(fx.go(&tg).orders.is_empty(), "EXE-146.2");
    // .3
    let mut fx = Fx::new();
    let mut tg = tk("10.0", |p| {
        p.min_chunk_notional = usd("50");
        p.depth_frac = rate("0.001");
    });
    tg.band = rate("0");
    assert!(fx.go(&tg).orders.is_empty(), "EXE-146.3");
    fx.at(2000);
    fx.touch(P);
    let r = fx.adv();
    assert!(r.states().contains(&(2, 3, pack(2, 3, 0))), "EXE-146.3");
    // .4 P min_notional 20.
    let mut insts = default_insts();
    insts[2].min_notional = 20_000_000;
    let mut fx = Fx::with_cfg(cfg(c0()), vec![g7(), g8()], insts);
    let mut tg = tk("0.1", min50);
    tg.band = rate("0");
    assert!(fx.go(&tg).orders.is_empty(), "EXE-146.4");
}

#[test]
fn exe_147_ioc_limit() {
    let mut fx = Fx::new();
    let o = fx.go(&t1("-10.0")).orders[0];
    assert_eq!(
        (
            o.intent.tif,
            o.intent.px,
            o.intent.qty,
            o.intent.side,
            o.intent.inst
        ),
        (Tif::Ioc, px("99.31"), q("10.0"), Side::Sell, P)
    );
    assert!(o.intent.link.get().is_none());
}

#[test]
fn exe_148_legs_independent() {
    let mut fx = Fx::new();
    let r = fx.go(&taker(A1, G7, &[(0, "3.37"), (1, "-3.37")]));
    assert_eq!(r.orders.len(), 2);
    let s = r.orders.iter().find(|o| o.intent.inst == S).unwrap();
    let p = r.orders.iter().find(|o| o.intent.inst == P).unwrap();
    assert_eq!((s.intent.side, s.intent.qty), (Side::Buy, q("3.37")));
    assert_eq!((p.intent.side, p.intent.qty), (Side::Sell, q("3.3")));
    assert!(s.intent.link.get().is_none() && p.intent.link.get().is_none());
}
