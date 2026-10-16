//! Core entry, hold, exit and capital gates; all market/account inputs are explicit.
#![allow(
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::disallowed_types
)] // test fixtures
pub(crate) mod common;
use common::*;
use hft_strategy_sdk::params::{RawParams, encode_body};
use hft_strategy_sdk::prelude::*;
use hft_strategy_sdk::testing::{CtxHarness, MockViews, RefFixture};
use portfolio_carry::{CarryParams, PortfolioCarry};

fn group() -> GroupTable {
    let z = LegDef {
        inst: INST_NONE,
        venue: VenueId(0),
        kind: 0,
        _pad: 0,
        route: 0,
        _pad2: 0,
    };
    GroupTable::new(
        &[GroupSpec {
            id: G4,
            asset: BTC_ID,
            n_legs: 2,
            open_allowed: 1,
            legs: [
                LegDef {
                    inst: I2,
                    venue: V2,
                    kind: 1,
                    ..z
                },
                LegDef {
                    inst: I1,
                    venue: V1,
                    kind: 1,
                    ..z
                },
                z,
                z,
            ],
            _rsv: 0,
        }],
        4,
    )
}
fn params(g: &GroupTable) -> CarryParams {
    let mut raw = RawParams {
        defaults: CarryParams::SCHEMA.fields.iter().map(|f| f.default).collect(),
        overrides: vec![],
    };
    raw.defaults[0] = 1;
    let body = encode_body(&CarryParams::SCHEMA, 1, &raw).unwrap();
    CarryParams::decode(&body, 2, g).unwrap()
}
fn funding(rate: i64, interval_s: u32) -> FundingView {
    FundingView {
        rate: Rate(rate),
        next_settle_ns: 120_000_000_000,
        interval_s,
        prev_interval_s: interval_s,
        upd_tsc: Tsc(1_000_000_000),
    }
}
pub(crate) fn setup() -> (PortfolioCarry, MockViews, CtxHarness) {
    let g = group();
    let p = params(&g);
    let rd = fixture_rd();
    let s = PortfolioCarry::new(
        StrategyInstance {
            strategy: S3,
            account: A1,
            kind_id: 1,
        },
        &p,
        &g,
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
    MockViews::leverage(&mut v, A1, I2, Some(200));
    MockViews::funding(&mut v, I2, funding(2_000_000, 28_800));
    MockViews::portfolio(&mut v, A1, |p| {
        p.cash(V1, USDT, 10_000_000_000, 0).margin(
            V2,
            MarginView {
                margin_balance: Notional(10_000_000_000),
                maint_margin: Notional(0),
                avail_margin: Notional(10_000_000_000),
                margin_ratio: Rate(0),
                src_ens: 1_000_000_000,
                stale: false,
            },
        );
    });
    (s, v, CtxHarness::new(&PortfolioCarry::DESC))
}
fn tick(s: &mut PortfolioCarry, v: &MockViews, h: &mut CtxHarness) -> hft_strategy_sdk::testing::Recorded {
    h.run(|ctx| {
        s.on_event(
            &EventRef::Bbo {
                inst: I2,
                gix: 0,
                b: v.bbo_of(I2),
            },
            v,
            ctx,
        )
    })
}

#[test]
fn carry_001_annual_differential_fees_borrow_and_intervals() {
    let p = params(&group());
    let mut f = *p.at(0);
    f.max_cross = Rate(0);
    f.borrow_apr = Rate(10_000_000);
    let a = funding(1_000_000, 28_800);
    let b = funding(400_000, 14_400);
    assert_eq!(
        portfolio_carry::fast::net_edge(&a, Some(&b), Rate(0), &f, -1),
        Some(209_000_000)
    );
    assert!(portfolio_carry::fast::net_edge(&a, Some(&b), Rate(500_000), &f, -1).unwrap() < 0);
    assert_eq!(portfolio_carry::fast::annual(Rate(10), 0), None);
    assert!(portfolio_carry::fast::net_edge(&b, Some(&a), Rate(0), &f, 1).unwrap() > 0);
    let before = portfolio_carry::fast::net_edge(&a, None, Rate(0), &f, -1).unwrap();
    f.expected_basis_cost = Rate(1_000_000);
    assert_eq!(
        portfolio_carry::fast::net_edge(&a, None, Rate(0), &f, -1),
        Some(before - 365_000_000)
    );
}

#[test]
fn carry_001_negative_funding_never_opens_reverse_spot() {
    let (mut s, mut v, mut h) = setup();
    MockViews::funding(&mut v, I2, funding(-20_000_000, 28_800));
    assert!(tick(&mut s, &v, &mut h).targets.is_empty());
}
#[test]
fn carry_002_entry_has_equal_linked_legs_and_cap() {
    let groups = group();
    let mut invalid = *groups.iter().next().unwrap().1;
    invalid.legs[0].kind = LegDef::CHAIN;
    let invalid = GroupTable::new(&[invalid], 4);
    let rd = fixture_rd();
    assert!(params(&invalid).check(&invalid, &RefView::new(&rd, A1)).is_err());
    let (mut s, v, mut h) = setup();
    let r = tick(&mut s, &v, &mut h);
    assert_eq!(r.targets.len(), 1);
    let t = r.targets[0];
    assert!(t.legs[0].qty.0 < 0);
    assert_eq!(t.legs[0].qty.0, -t.legs[1].qty.0);
    assert!(t.legs[1].qty.0 <= 100_000_000);
    assert_simultaneous(t.exec);
    assert_eq!(t.reason, TargetReason::Rebalance);
}
#[test]
fn carry_003_missing_stale_or_late_funding_prevents_entry() {
    for change in 0..4 {
        let (mut s, mut v, mut h) = setup();
        let mut f = funding(2_000_000, 28_800);
        match change {
            0 => f.interval_s = 0,
            1 => f.upd_tsc = Tsc(0),
            2 => f.next_settle_ns = 2_000_000_000,
            _ => f.next_settle_ns = 900_000_000_000,
        };
        MockViews::funding(&mut v, I2, f);
        assert!(tick(&mut s, &v, &mut h).targets.is_empty());
    }
}
#[test]
fn carry_004_margin_and_leverage_fail_closed() {
    for leverage in [None, Some(0), Some(301)] {
        let (mut s, mut v, mut h) = setup();
        MockViews::leverage(&mut v, A1, I2, leverage);
        assert!(tick(&mut s, &v, &mut h).targets.is_empty());
    }
    let (mut s, mut v, mut h) = setup();
    MockViews::portfolio(&mut v, A1, |p| {
        p.cash(V1, USDT, 1, 0);
    });
    assert!(tick(&mut s, &v, &mut h).targets.is_empty());
}
#[test]
fn carry_005_edge_decay_exits_and_hysteresis_holds() {
    let (mut s, mut v, mut h) = setup();
    let mut p = GroupPosView::EMPTY;
    p.n_legs = 2;
    p.legs[0].qty = Qty(-10_000_000);
    p.legs[1].qty = Qty(10_000_000);
    MockViews::group_pos(&mut v, A1, G4, p);
    assert!(tick(&mut s, &v, &mut h).targets.is_empty());
    MockViews::funding(&mut v, I2, funding(-1_000_000, 28_800));
    let r = tick(&mut s, &v, &mut h);
    assert_eq!(r.targets.len(), 1);
    assert_eq!(r.targets[0].reason, TargetReason::Exit);
    assert!(r.targets[0].legs.iter().all(|l| l.qty.0 == 0));
    assert_simultaneous(r.targets[0].exec);
}
#[test]
fn carry_006_healthy_quiet_quote_accepted_disconnected_quote_rejected() {
    let (mut s, mut v, mut h) = setup();
    v.bbo_rx(I1, Tsc(1), 0, RxTsSrc::None);
    assert_eq!(tick(&mut s, &v, &mut h).targets.len(), 1);
    let (mut s, mut v, mut h) = setup();
    v.conn(I1, Tsc(1_000_000_000), false, false);
    assert!(tick(&mut s, &v, &mut h).targets.is_empty());
}

#[test]
fn carry_007_snapshot_restores_cooldown_and_rejects_partial_corruption() {
    let (mut s, v, mut h) = setup();
    assert_eq!(tick(&mut s, &v, &mut h).targets.len(), 1);
    let mut w = SnapshotWriter::new(4096);
    s.snapshot(&mut w);
    let bytes = w.as_bytes().to_vec();
    let (mut restored, _, mut out) = setup();
    restored
        .restore(&mut SnapshotReader::with_desc_version(&bytes, 1))
        .unwrap();
    assert!(tick(&mut restored, &v, &mut out).targets.is_empty());
    let mut truncated = bytes.clone();
    truncated.pop();
    assert!(
        restored
            .restore(&mut SnapshotReader::with_desc_version(&truncated, 1))
            .is_err()
    );
    let mut after = SnapshotWriter::new(4096);
    restored.snapshot(&mut after);
    assert_eq!(after.as_bytes(), bytes);
}

#[test]
fn carry_008_disabled_and_closeonly_exit_held_positions() {
    for disabled in [false, true] {
        let (mut s, mut v, mut h) = setup();
        let mut p = GroupPosView::EMPTY;
        p.n_legs = 2;
        p.legs[0].qty = Qty(-10_000_000);
        p.legs[1].qty = Qty(10_000_000);
        MockViews::group_pos(&mut v, A1, G4, p);
        if disabled {
            let g = group();
            let raw = RawParams {
                defaults: CarryParams::SCHEMA.fields.iter().map(|f| f.default).collect(),
                overrides: vec![],
            };
            let p = CarryParams::decode(&encode_body(&CarryParams::SCHEMA, 1, &raw).unwrap(), 2, &g).unwrap();
            s.on_params(&p, &g, &RefView::new(&fixture_rd(), A1)).unwrap();
        } else {
            MockViews::mode(&mut v, A1, G4, RunMode::CloseOnly);
        }
        let r = tick(&mut s, &v, &mut h);
        assert_eq!(r.targets.len(), 1);
        assert!(r.targets[0].legs.iter().all(|l| l.qty.0 == 0));
        assert_simultaneous(r.targets[0].exec);
    }
}

fn assert_simultaneous(exec: ExecSpec) {
    let ExecSpec::Simultaneous(p) = exec else {
        panic!("entry and exit must dispatch both legs simultaneously");
    };
    assert_eq!(p.on_fail, SimulOnFail::Reverse);
    assert_eq!(p.fix_tolerance, Rate(0));
    assert_eq!(p.residual, ResidualPolicy::TrimExcess);
    assert_eq!(p.slice.depth_frac, Rate(1_000_000_000));
    assert_eq!(p.slice.depth_levels, 1);
    assert_eq!(p.slice.max_cross, params(&group()).at(0).max_cross);
    assert_eq!(p.slice.max_chunk_notional, params(&group()).at(0).max_notional);
}
