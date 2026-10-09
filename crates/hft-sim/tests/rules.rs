//! SIM-090–SIM-091: reject rules.

#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use crate::common;

use common::*;
use hft_event::{OrderEventKind, OrderFlags};
use hft_refdata::{InstStatus, LimitBucket, LimitScope, RateLimit};
use hft_sim::{SimInput, SimSetup};
use hft_types::{InstId, Side, Tif};

struct R {
    row: &'static str,
    rd: Box<dyn Fn(&mut RdSpec)>,
    setup: Box<dyn Fn(&mut SimSetup)>,
    seed_short: bool,
    notice: bool,
    n_before: usize,
    inst: InstId,
    side: Side,
    qty: &'static str,
    px: &'static str,
    tif: Tif,
    flags: u16,
    /// `None` = passes (Ack), `Some(n)` = Reject with `SIM_CODE_BASE + n`.
    want: Option<u32>,
}

fn r(
    row: &'static str,
    inst: InstId,
    side: Side,
    qty: &'static str,
    px: &'static str,
    want: Option<u32>,
) -> R {
    R {
        row,
        rd: Box::new(|_| {}),
        setup: Box::new(|_| {}),
        seed_short: inst == BTC_P2,
        notice: false,
        n_before: 0,
        inst,
        side,
        qty,
        px,
        tif: Tif::Ioc,
        flags: 0,
        want,
    }
}

fn usdt_s1(amount: &'static str) -> Box<dyn Fn(&mut SimSetup)> {
    Box::new(move |s| {
        s.accounts[0].1.initial_balances =
            vec![bal(S1, USDT, usdt(amount)), bal(P2, USDT, usdt("18000"))].into()
    })
}

fn run(x: &R) -> Vec<hft_event::OrderEvent> {
    let rd = rd_with(|s| (x.rd)(s));
    let mut setup = setup_sim();
    (x.setup)(&mut setup);
    let mut s = Sim::with(&setup, &rd);
    if x.seed_short {
        let mut st = seed9_state("18000", &[(BTC_P2, "-1.0", "60000")]);
        st.balances.push((S1, USDT, usdt("100000")));
        s.seed(A9, &st);
    }
    s.topn(ms(50), BTC_S1, &[("60000", "1")], &[("59990", "1")]);
    s.bbo(ms(50), BTC_P2, ("59999.9", "10"), ("60000.0", "10"));
    s.mark(ms(50), BTC_P2, "60000");
    if x.notice {
        s.ex.on_record(
            &rhdr(ms(60)),
            SimInput::VenueNotice {
                venue: P2,
                kind: 1,
                inst: BTC_P2,
            },
        );
    }
    for _ in 0..x.n_before {
        s.place(A9, BTC_S1, Buy, "0.0001", "60000", Tif::Ioc, ms(100));
    }
    let o = s.place_flags(A9, x.inst, x.side, q(x.qty), px(x.px), x.tif, x.flags, ms(100));
    s.run_to(ms(110));
    s.events_of(A9, o)
}

#[test]
fn sim_090_reject_rules() {
    let ro = OrderFlags::REDUCE_ONLY.0;
    let limit = |s: &mut RdSpec| {
        s.limits.push((
            S1,
            RateLimit {
                scope: LimitScope::Account,
                bucket: LimitBucket::Orders,
                limit: 10,
                window_ms: 1_000,
                default_cost: 1,
            },
        ))
    };
    let rows = vec![
        R {
            rd: Box::new(|s| s.inst_mut(BTC_S1).status = InstStatus::Halted),
            ..r("SIM-090.1", BTC_S1, Buy, "0.1", "60000", Some(1))
        },
        r("SIM-090.2", BTC_S1, Buy, "0.1", "60000", None),
        R {
            notice: true,
            ..r("SIM-090.3", BTC_P2, Sell, "0.1", "60000", Some(1))
        },
        R {
            notice: true,
            ..r("SIM-090.4", BTC_P2, Buy, "0.1", "60000", None)
        },
        r("SIM-090.5", BTC_S1, Buy, "0.000015", "60000", Some(2)),
        R {
            rd: Box::new(|s| {
                s.inst_mut(BTC_S1).min_qty = q("0.0001");
                s.inst_mut(BTC_S1).min_qty_str = "0.0001".into();
            }),
            ..r("SIM-090.6", BTC_S1, Buy, "0.00005", "60000", Some(2))
        },
        R {
            rd: Box::new(|s| {
                s.inst_mut(BTC_S1).min_qty = q("0.0001");
                s.inst_mut(BTC_S1).min_qty_str = "0.0001".into();
            }),
            ..r("SIM-090.7", BTC_S1, Buy, "0.0001", "60000", None)
        },
        r("SIM-090.8", BTC_S1, Buy, "0.1", "60000.005", Some(3)),
        r("SIM-090.9", BTC_S1, Buy, "0.00008", "60000", Some(4)),
        r("SIM-090.10", BTC_S1, Buy, "0.0001", "50000", None),
        r("SIM-090.11", BTC_S1, Buy, "1.0", "10.00", None),
        R {
            flags: ro,
            ..r("SIM-090.12", BTC_S1, Sell, "0.1", "60000", Some(6))
        },
        R {
            flags: ro,
            seed_short: false,
            ..r("SIM-090.13", BTC_P2, Buy, "0.1", "60000", Some(7))
        },
        R {
            flags: ro,
            ..r("SIM-090.14", BTC_P2, Buy, "0.5", "60000", None)
        },
        R {
            flags: ro,
            ..r("SIM-090.15", BTC_P2, Buy, "1.5", "60000", Some(7))
        },
        R {
            tif: Tif::PostOnly,
            ..r("SIM-090.16", BTC_S1, Buy, "0.1", "60000", Some(8))
        },
        R {
            tif: Tif::PostOnly,
            ..r("SIM-090.17", BTC_S1, Buy, "0.1", "59995", None)
        },
        R {
            setup: usdt_s1("6002.39"),
            ..r("SIM-090.18", BTC_S1, Buy, "0.1", "60000", Some(9))
        },
        R {
            setup: usdt_s1("6002.40"),
            ..r("SIM-090.19", BTC_S1, Buy, "0.1", "60000", None)
        },
        R {
            setup: Box::new(|s| {
                s.accounts[0].1.initial_balances =
                    vec![bal(S1, BTC, q("0.09999")), bal(P2, USDT, usdt("18000"))].into()
            }),
            ..r("SIM-090.20", BTC_S1, Sell, "0.1", "59990", Some(9))
        },
        r("SIM-090.21", BTC_P2, Sell, "0.5", "60000", None),
        r("SIM-090.22", BTC_P2, Sell, "0.501", "60000", Some(10)),
        R {
            rd: Box::new(limit),
            setup: Box::new(|s| s.tuning.rate_limit = true),
            n_before: 9,
            ..r("SIM-090.23", BTC_S1, Buy, "0.0001", "60000", None)
        },
        R {
            rd: Box::new(limit),
            setup: Box::new(|s| s.tuning.rate_limit = true),
            n_before: 10,
            ..r("SIM-090.24", BTC_S1, Buy, "0.0001", "60000", Some(11))
        },
        R {
            rd: Box::new(limit),
            setup: Box::new(|s| s.tuning.rate_limit = false),
            n_before: 10,
            ..r("SIM-090.25", BTC_S1, Buy, "0.0001", "60000", None)
        },
        r("SIM-090.26", BTC_S1, Buy, "0.000015", "60000.005", Some(2)),
    ];
    for x in &rows {
        let ev = run(x);
        match x.want {
            None => assert_eq!(
                ev.first().map(|e| e.what),
                Some(OrderEventKind::Ack),
                "{}: {ev:?}",
                x.row
            ),
            Some(n) => {
                assert_eq!(ev.len(), 1, "{}: {ev:?}", x.row);
                assert_eq!(
                    (ev[0].what, ev[0].reject_code),
                    (OrderEventKind::Reject, 0x1000_0000 + n),
                    "{}",
                    x.row
                );
            }
        }
    }
}

#[test]
fn sim_091_reject_no_side_effects() {
    let mut s = Sim::std();
    s.topn(ms(50), BTC_S1, &[("60000", "1")], &[("59990", "1")]);
    let before: (Vec<_>, Vec<_>) = {
        let v = s.view(A9);
        (v.balances().collect(), v.positions().collect())
    };
    let o = s.place(A9, BTC_S1, Buy, "0.00008", "60000", Tif::Ioc, ms(100));
    s.run_to(ms(110));
    let ev = s.order_events(A9);
    assert_eq!(ev.len(), 1);
    let e = ev[0].1;
    assert_eq!(
        (e.what, e.slot, e.r#gen, e.reject_code),
        (OrderEventKind::Reject, o, 1, 0x1000_0004)
    );
    let v = s.view(A9);
    let after: (Vec<_>, Vec<_>) = (v.balances().collect(), v.positions().collect());
    assert_eq!(before, after);
    assert_eq!(v.order(o).map(|x| (x.status, x.resting)), Some((5, false)));
}
