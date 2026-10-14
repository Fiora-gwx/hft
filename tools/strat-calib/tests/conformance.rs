//! Same JSON is asserted by Python. Rust calls production signal/sizing, not a second formula.
#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    clippy::disallowed_types
)]

#[path = "../../../strategies/portfolio-carry/tests/common/mod.rs"]
mod common;

use cross_taker::{XtFields, fast::signal::*};
use hft_strategy_sdk::params::{RawParams, encode_body};
use hft_strategy_sdk::prelude::*;
use hft_strategy_sdk::testing::{CtxHarness, MockViews, RefFixture};
use portfolio_carry::{CarryFields, CarryParams, PortfolioCarry};
use serde_json::{Value, json};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/signals.json")).unwrap()
}
fn n(v: &Value, key: &str, default: i64) -> i64 {
    v[key].as_i64().unwrap_or(default)
}
fn leg(v: &Value, meta: &Value) -> LegInput {
    LegInput {
        book: BboView {
            bid_px: Px(v[0].as_i64().unwrap()),
            ask_px: Px(v[1].as_i64().unwrap()),
            bid_qty: Qty(v[2].as_i64().unwrap()),
            ask_qty: Qty(v[3].as_i64().unwrap()),
            ..BboView::EMPTY
        },
        hot: InstHot {
            px_exp: n(meta, "px_exp", -2) as i8,
            qty_exp: n(meta, "qty_exp", -2) as i8,
            tick: n(meta, "tick", 1),
            lot: n(meta, "lot", 2),
            min_qty: Qty(2),
            max_qty: Qty(1_000_000),
            min_notional: Notional(1_000_000),
            ..InstHot::NONE
        },
        fee: Rate(1_000_000),
        position: Qty(0),
        spot: false,
        spot_free: Qty(0),
    }
}

#[test]
fn calibration_shared_taker_signal_and_sizing() {
    for case in fixture()["taker"].as_array().unwrap() {
        let mut buy = leg(&case["buy"], &case["buy_meta"]);
        let mut sell = leg(&case["sell"], &case["sell_meta"]);
        buy.position = Qty(case["positions"][0].as_i64().unwrap_or(0));
        sell.position = Qty(case["positions"][1].as_i64().unwrap_or(0));
        sell.spot = case["spot"].as_bool().unwrap_or(false);
        sell.spot_free = Qty(n(case, "spot_free", 0));
        let p = XtFields {
            enabled: true,
            min_edge: Rate(1_000_000),
            slip: Rate(n(case, "slip", 0)),
            depth_frac: Rate(500_000_000),
            max_chunk_notional: Notional(100_000_000),
            max_position_notional: Notional(500_000_000),
            max_quote_age_ms: 5,
            cooldown_ms: 100,
            max_hedge_failures: 3,
        };
        let got = signal(buy, sell, &p).map_or(Value::Null, |s| {
            json!({"buy_px":s.buy_px.0,"sell_px":s.sell_px.0,"qty":s.qty.0,
                   "edge":s.edge.0,"reducing":s.reducing})
        });
        assert_eq!(got, case["expected"], "{}", case["name"]);
    }
}

fn funding(rate: i64, interval: u32) -> FundingView {
    FundingView {
        rate: Rate(rate),
        interval_s: interval,
        prev_interval_s: interval,
        next_settle_ns: 120_000_000_000,
        upd_tsc: Tsc(1_000_000_000),
    }
}
fn fields() -> CarryFields {
    CarryFields {
        enabled: true,
        enter_apr: Rate(100_000_000),
        exit_apr: Rate(20_000_000),
        borrow_apr: Rate(0),
        fee_horizon_ms: 86_400_000,
        entry_window_ms: 300_000,
        min_lead_ms: 10_000,
        funding_max_age_ms: 30_000,
        max_notional: Notional(100_000_000),
        capital_fraction: Rate(400_000_000),
        max_leverage_x100: 300,
        max_cross: Rate(100_000),
        retry_ms: 1_000,
    }
}

#[test]
fn calibration_shared_funding_net_apr() {
    for c in fixture()["carry"].as_array().unwrap() {
        let f = CarryFields {
            borrow_apr: Rate(n(c, "borrow_apr", 0)),
            max_cross: Rate(n(c, "max_cross", 0)),
            fee_horizon_ms: n(c, "horizon", 86_400_000) as u32,
            ..fields()
        };
        let a = funding(c["a"][0].as_i64().unwrap(), c["a"][1].as_u64().unwrap() as u32);
        let b = c["b"]
            .as_array()
            .map(|b| funding(b[0].as_i64().unwrap(), b[1].as_u64().unwrap() as u32));
        let got =
            portfolio_carry::fast::net_edge(&a, b.as_ref(), Rate(n(c, "fees", 0)), &f, n(c, "direction", -1));
        assert_eq!(got, c["expected"].as_i64());
    }
}

#[test]
fn calibration_shared_carry_target_uses_real_strategy() {
    use common::*;
    for c in fixture()["carry_targets"].as_array().unwrap() {
        let zero = LegDef {
            inst: INST_NONE,
            venue: VenueId(0),
            kind: 0,
            _pad: 0,
            route: 0,
            _pad2: 0,
        };
        let groups = GroupTable::new(
            &[GroupSpec {
                id: G4,
                asset: BTC_ID,
                n_legs: 2,
                open_allowed: 1,
                _rsv: 0,
                legs: [
                    LegDef {
                        inst: I2,
                        venue: V2,
                        kind: 1,
                        ..zero
                    },
                    LegDef {
                        inst: I1,
                        venue: V1,
                        kind: 1,
                        ..zero
                    },
                    zero,
                    zero,
                ],
            }],
            4,
        );
        let mut defaults: Vec<i64> = CarryParams::SCHEMA.fields.iter().map(|f| f.default).collect();
        defaults[0] = 1;
        let body = encode_body(
            &CarryParams::SCHEMA,
            1,
            &RawParams {
                defaults,
                overrides: vec![],
            },
        )
        .unwrap();
        let params = CarryParams::decode(&body, 1, &groups).unwrap();
        let rd = fixture_rd();
        let mut strategy = PortfolioCarry::new(
            StrategyInstance {
                strategy: S3,
                account: A1,
                kind_id: 1,
            },
            &params,
            &groups,
            &RefView::new(&rd, A1),
        )
        .unwrap();
        let mut v = MockViews::new(RefFixture::new(rd, A1));
        v.at(Now::new(
            Tsc(1_000_000_000),
            2,
            1,
            true,
            EngineClockAnchor {
                base_engine_ns: 0,
                base_tsc: Tsc(0),
                tsc_hz_milli: 1_000_000_000_000,
                utc_tsc: Tsc(0),
                utc_ns: 0,
            },
        ));
        for (i, venue) in [(I1, V1), (I2, V2)] {
            MockViews::bbo(
                &mut v,
                i,
                (Px(10_000), Qty(1_000_000_000)),
                (Px(10_001), Qty(1_000_000_000)),
            );
            v.conn(i, Tsc(1_000_000_000), true, false);
            MockViews::venue_ready(&mut v, A1, venue, true);
        }
        MockViews::mode(&mut v, A1, G4, RunMode::Live);
        MockViews::leverage(&mut v, A1, I2, Some(n(c, "leverage", 200) as u32));
        MockViews::funding(&mut v, I2, funding(2_000_000, 28_800));
        MockViews::portfolio(&mut v, A1, |p| {
            p.cash(V1, USDT, n(c, "capital", 0), 0).margin(
                V2,
                MarginView {
                    margin_balance: Notional(n(c, "capital", 0)),
                    avail_margin: Notional(n(c, "capital", 0)),
                    maint_margin: Notional(0),
                    margin_ratio: Rate(0),
                    src_ens: 1_000_000_000,
                    stale: false,
                },
            );
        });
        let mut h = CtxHarness::new(&PortfolioCarry::DESC);
        let r = h.run(|ctx| {
            strategy.on_event(
                &EventRef::Bbo {
                    inst: I2,
                    gix: 0,
                    b: v.bbo_of(I2),
                },
                &v,
                ctx,
            )
        });
        let got = r.targets.first().map_or(0, |t| -t.legs[0].qty.0);
        assert_eq!(got, n(c, "expected", 0));
        if let Some(t) = r.targets.first() {
            assert_eq!(t.legs[0].qty.0, -t.legs[1].qty.0);
            assert!(matches!(t.exec, ExecSpec::Simultaneous(_)));
        }
    }
}
