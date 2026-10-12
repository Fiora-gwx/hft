//! SIM-223–226: cash facts come from matching, are cumulative per order and survive seeding.
use crate::common::*;
use hft_event::{GwOp, OrderEventKind, RepMsg};
use hft_sim::{SimCfgErr, SimSeedOrder};
use hft_types::{Qty, SlotId, Tif};

#[test]
fn sim_223_order_cash_is_independent_of_fees_and_queries() {
    for (opening, levels, limit, cash) in [
        (Buy, [("62000", "0.1"), ("61000", "0.1")], "61000", ["200", "300"]),
        (
            Buy,
            [("59000", "0.1"), ("58000", "0.1")],
            "58000",
            ["-100", "-300"],
        ),
        (
            Sell,
            [("58000", "0.1"), ("59000", "0.1")],
            "59000",
            ["200", "300"],
        ),
        (
            Sell,
            [("61000", "0.1"), ("62000", "0.1")],
            "62000",
            ["-100", "-300"],
        ),
    ] {
        for late in [false, true] {
            let mut setup = setup_sim();
            setup.tuning.late_fee = late;
            let mut s = Sim::with(&setup, &rd_sim());
            if opening == Buy {
                s.topn(ms(100), BTC_P2, &[("60000", "1")], &[]);
            } else {
                s.topn(ms(100), BTC_P2, &[], &[("60000", "1")]);
            }
            let open = s.place(A9, BTC_P2, opening, "0.2", "60000", Tif::Ioc, ms(100));
            s.run_to(ms(190));
            let f = s
                .events_of(A9, open)
                .into_iter()
                .find(|e| e.what == OrderEventKind::Fill)
                .unwrap();
            assert_eq!((f.cash_qty, f.cash_sum), (q("0.2"), Qty(0)), "known zero");
            let closing = if opening == Buy { Sell } else { Buy };
            if closing == Sell {
                s.topn(ms(200), BTC_P2, &[], &levels);
            } else {
                s.topn(ms(200), BTC_P2, &levels, &[]);
            }
            let close = s.place(A9, BTC_P2, closing, "0.2", limit, Tif::Ioc, ms(200));
            s.run_to(ms(290));
            let fills: Vec<_> = s
                .events_of(A9, close)
                .into_iter()
                .filter(|e| matches!(e.what, OrderEventKind::PartialFill | OrderEventKind::Fill))
                .collect();
            assert_eq!(fills.len(), 2);
            for (i, e) in fills.iter().enumerate() {
                assert_eq!(e.cash_qty, e.cum_filled);
                assert_eq!(e.cash_sum, usdt(cash[i]));
            }
            let fees: i64 = s.order_events(A9).iter().map(|(_, e)| e.fee.0).sum();
            assert!(fees > 0);
            assert_eq!(s.total(A9, P2, USDT), usdt("18000").0 + usdt(cash[1]).0 - fees);
            for t in [300, 320] {
                s.op(A9, GwOp::Query, BTC_P2, close, Qty(0), px("0"), ms(t));
                s.run_to(ms(t + 10));
            }
            let queries: Vec<_> = s
                .events_of(A9, close)
                .into_iter()
                .filter(|e| e.what == OrderEventKind::QueryResult)
                .collect();
            assert_eq!(queries.len(), 2);
            assert!(
                queries
                    .iter()
                    .all(|e| e.cash_qty == q("0.2") && e.cash_sum == usdt(cash[1]))
            );
        }
    }
    let mut s = Sim::std();
    s.topn(ms(100), BTC_S1, &[("60000", "1")], &[]);
    let slot = s.place(A9, BTC_S1, Buy, "0.1", "60000", Tif::Ioc, ms(100));
    s.run_to(ms(200));
    assert!(
        s.events_of(A9, slot)
            .iter()
            .all(|e| e.cash_qty == Qty(0) && e.cash_sum == Qty(0))
    );
}

fn recovered_order(known: bool) -> SimSeedOrder {
    SimSeedOrder {
        venue: P2,
        inst: BTC_P2,
        slot: SlotId(7),
        r#gen: 1,
        side: Sell,
        tif: Tif::Gtc,
        prev_state: 2,
        qty: q("0.2"),
        filled: q("0.1"),
        px: px("61000"),
        avg_px: px("61000"),
        exch_order_id: 700,
        cash_qty: if known { q("0.1") } else { Qty(0) },
        cash_sum: if known { usdt("100") } else { Qty(0) },
    }
}

#[test]
fn sim_224_recovered_cash_coverage_is_not_inferred_from_wallet() {
    for known in [true, false] {
        let mut s = Sim::std();
        let mut seed = seed9_state("18100", &[(BTC_P2, "0.1", "60000")]);
        seed.orders.push(recovered_order(known));
        s.seed(A9, &seed);
        s.op(A9, GwOp::Query, BTC_P2, SlotId(7), Qty(0), px("0"), ms(100));
        s.run_to(ms(110));
        let query = s
            .events_of(A9, SlotId(7))
            .into_iter()
            .find(|e| e.what == OrderEventKind::QueryResult)
            .unwrap();
        assert_eq!(
            (query.cash_qty, query.cash_sum),
            (seed.orders[0].cash_qty, seed.orders[0].cash_sum)
        );
        s.trade(ms(200), BTC_P2, "61001", "0.1");
        s.run_to(ms(210));
        let fill = s
            .events_of(A9, SlotId(7))
            .into_iter()
            .find(|e| e.what == OrderEventKind::Fill)
            .unwrap();
        assert_eq!(fill.cum_filled, q("0.2"));
        assert_eq!(fill.cash_qty, if known { q("0.2") } else { q("0.1") });
        assert_eq!(fill.cash_sum, if known { usdt("200") } else { usdt("100") });
    }
    for row in 0..4 {
        let mut s = Sim::std();
        let before = s.ex.state_hash();
        let mut seed = seed9_state("18100", &[]);
        let mut order = recovered_order(true);
        match row {
            0 => order.cash_qty = Qty(-1),
            1 => order.cash_qty = q("0.2"),
            2 => order.cash_qty = Qty(0),
            _ => {
                order.inst = BTC_S1;
                order.venue = S1;
            }
        }
        seed.orders.push(order);
        assert_eq!(s.ex.seed_account_from(A9, &seed), Err(SimCfgErr::SettlementFacts));
        assert_eq!(s.ex.state_hash(), before, "invalid seed row {row}");
    }
}

#[test]
fn sim_225_liquidation_reports_exact_cash_separately_from_fee() {
    let mut s = Sim::std();
    s.seed(A9, &seed9_state("18000", &[(BTC_P2, "-1", "60000")]));
    s.mark(ms(1), BTC_P2, "77611.9403");
    s.run_to(ms(10));
    let fills: Vec<_> = s
        .out
        .iter()
        .filter_map(|r| {
            if let RepMsg::ExternalFill(f) = r.msg {
                Some(f)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(fills.len(), 1);
    let f = fills[0];
    assert_eq!((f.cash_qty, f.cash_amount), (q("1"), usdt("-18000.0001")));
    assert_eq!(f.fee, usdt("390.000001"));
    assert_eq!(s.total(A9, P2, USDT), usdt("18000").0 + f.cash_amount.0 - f.fee.0);
}

#[test]
fn sim_226_external_cash_is_returned_at_settlement() {
    // Independent constants from the five original executions, in exchange matching order.
    // The first close costs 11700 quote units; the final close costs 100, without fees.
    let facts = [
        (Sell, 100_000, 600_189_000, 0),
        (Sell, 100_000, 600_244_000, 0),
        (Buy, 200_000, 600_275_000, -11_700),
        (Sell, 100_000, 600_274_000, 0),
        (Buy, 100_000, 600_275_000, -100),
    ];
    for mirror in [false, true] {
        let mut s = Sim::std();
        let mut wallet = usdt("18000").0;
        let mut position = 0;
        for (side, qty, price, expected) in facts {
            let side = if mirror { side.opposite() } else { side };
            let expected = if mirror { -expected } else { expected };
            assert_eq!(
                s.ex.external_fill(A9, BTC_P2, side, Qty(qty), hft_types::Px(price)),
                Some(Qty(expected))
            );
            wallet += expected;
            position += if side == Buy { qty } else { -qty };
            assert_eq!(s.total(A9, P2, USDT), wallet);
            assert_eq!(s.view(A9).position(BTC_P2).qty, Qty(position));
            assert_eq!(s.ex.pending_replies(), 0, "caller owns report delivery");
        }
        assert_eq!(position, 0);
    }
    // Invalid facts must not settle, including a tracked instrument on an unbound venue.
    for row in 0..7 {
        let mut s = if row == 6 {
            let rd = rd_with(|r| {
                r.accounts.iter_mut().find(|a| a.id == A9).unwrap().venues = vbits(&[S1]);
            });
            let mut setup = setup_with_a8();
            setup.accounts[0].1.initial_balances = cfg_a9()
                .initial_balances
                .iter()
                .filter(|b| b.venue != P2)
                .cloned()
                .collect::<Vec<_>>()
                .into();
            Sim::with(&setup, &rd)
        } else {
            Sim::std()
        };
        let mut account = A9;
        let mut inst = BTC_P2;
        let mut qty = q("0.001");
        let mut price = px("60000");
        match row {
            0 => account = A8,
            1 => inst = hft_types::InstId(999),
            2 => qty = Qty(0),
            3 => qty = Qty(-1),
            4 => price = hft_types::Px(0),
            5 => price = hft_types::Px(-1),
            _ => {}
        }
        let before = s.ex.state_hash();
        assert_eq!(
            s.ex.external_fill(account, inst, Buy, qty, price),
            None,
            "row {row}"
        );
        assert_eq!(s.ex.state_hash(), before, "invalid fact row {row}");
    }
}
