//! SIM-001–SIM-004: tuning defaults, constants, configuration errors, independent sub-streams.

#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use crate::common;

use common::*;
use hft_refdata::MarginMode;
use hft_sim::{
    CMDS_PER_POLL, Dist, LatencyModel, MAX_LATENCY_MODELS, MAX_MARGIN_MODELS, MAX_SIM_ACCOUNTS, MakerFill,
    REPLY_RESERVE, SIM_CODE_BASE, SimCfgErr, SimExchange, SimTuning,
};
use hft_types::{AccountId, AssetId, Tif, VenueId};

#[test]
fn sim_001_tuning_defaults() {
    let t = SimTuning::with_seed(7);
    let rows: [(&str, bool); 8] = [
        ("SIM-001.1", t.consume_decay_ms == 1_000),
        ("SIM-001.2", t.take_avail_bp == 10_000),
        ("SIM-001.3", t.quiet_ms == 50),
        ("SIM-001.4", t.report_interval_s == 60),
        ("SIM-001.5", t.transfer_delay_ms == 300_000),
        ("SIM-001.6", t.rate_limit),
        ("SIM-001.7", !t.late_fee),
        ("SIM-001.8", t.maker_fill == MakerFill::TradeThrough),
    ];
    for (row, ok) in rows {
        assert!(ok, "{row}");
    }
    // SIM-001.9: no default for `seed` — the only constructor takes it and there is no `Default`.
    static_assertions::assert_not_impl_any!(SimTuning: Default);
    assert_eq!(t.seed, 7, "SIM-001.9");
}

#[test]
fn sim_002_constants() {
    assert_eq!(MAX_SIM_ACCOUNTS, 64);
    assert_eq!(MAX_LATENCY_MODELS, 16);
    assert_eq!(MAX_MARGIN_MODELS, 16);
    assert_eq!(SIM_CODE_BASE, 0x1000_0000);
    assert_eq!(CMDS_PER_POLL, 256);
    assert_eq!(REPLY_RESERVE, 4_096);
}

fn emp(points: &[(u16, u32)]) -> LatencyModel {
    let d = Dist::from_points(points);
    LatencyModel::Empirical { entry: d, resp: d }
}

#[test]
fn sim_003_config_errors() {
    let rd = rd_sim();
    let new = |s: &hft_sim::SimSetup, rd: &hft_refdata::RefData| SimExchange::new(s, rd).map(|_| ());
    let with_a9 = |f: &dyn Fn(&mut hft_refdata::SimAccountCfg)| {
        let mut s = setup_sim();
        f(&mut s.accounts[0].1);
        s
    };
    let push_bal = |c: &mut hft_refdata::SimAccountCfg, b: hft_refdata::SimBalance| {
        let mut v = c.initial_balances.to_vec();
        v.push(b);
        c.initial_balances = v.into();
    };
    let s1 = with_a9(&|c| push_bal(c, bal(VenueId(9), USDT, usdt("1"))));
    assert_eq!(new(&s1, &rd), Err(SimCfgErr::UnknownVenue), "SIM-003.1");
    let s2 = with_a9(&|c| push_bal(c, bal(S1, AssetId(9), usdt("1"))));
    assert_eq!(new(&s2, &rd), Err(SimCfgErr::UnknownAsset), "SIM-003.2");
    let s3 = with_a9(&|c| c.fee_tier = 9);
    assert_eq!(new(&s3, &rd), Err(SimCfgErr::FeeTier), "SIM-003.3");
    let s4 = with_a9(&|c| c.latency_model = 1);
    assert_eq!(new(&s4, &rd), Err(SimCfgErr::LatencyModelIndex), "SIM-003.4");
    let s5 = with_a9(&|c| c.latency_model = 0);
    assert_eq!(new(&s5, &rd), Ok(()), "SIM-003.5");
    let s6 = with_a9(&|c| c.margin_model = 1);
    assert_eq!(new(&s6, &rd), Err(SimCfgErr::MarginModelIndex), "SIM-003.6");
    let mut s7 = setup_sim();
    s7.latency_models.clear();
    assert_eq!(new(&s7, &rd), Err(SimCfgErr::LatencyModelIndex), "SIM-003.7");
    let mut s8 = setup_sim();
    s8.margin_models.clear();
    assert_eq!(new(&s8, &rd), Err(SimCfgErr::MarginModelIndex), "SIM-003.8");
    let dist_rows: [(&str, Vec<(u16, u32)>, Result<(), SimCfgErr>); 4] = [
        (
            "SIM-003.9",
            vec![(0, 100), (5_000, 200), (5_000, 300), (10_000, 400)],
            Err(SimCfgErr::BadDist),
        ),
        (
            "SIM-003.10",
            vec![(100, 100), (5_000, 200), (10_000, 400)],
            Err(SimCfgErr::BadDist),
        ),
        (
            "SIM-003.11",
            vec![(0, 100), (5_000, 200), (9_999, 400)],
            Err(SimCfgErr::BadDist),
        ),
        (
            "SIM-003.12",
            vec![(0, 100), (3_000, 200), (6_000, 150), (10_000, 300)],
            Err(SimCfgErr::BadDist),
        ),
    ];
    for (row, pts, want) in dist_rows {
        let mut s = setup_sim();
        s.latency_models = vec![emp(&pts)];
        assert_eq!(new(&s, &rd), want, "{row}");
    }
    let pts64: Vec<(u16, u32)> = (0..64u32).map(|k| ((k * 10_000 / 63) as u16, 100 + k)).collect();
    let mut s13 = setup_sim();
    s13.latency_models = vec![emp(&pts64)];
    assert_eq!(new(&s13, &rd), Ok(()), "SIM-003.13");
    let pts65: Vec<(u16, u32)> = (0..65u32).map(|k| ((k * 10_000 / 64) as u16, 100 + k)).collect();
    let mut s14 = setup_sim();
    s14.latency_models = vec![emp(&pts65)];
    assert_eq!(new(&s14, &rd), Err(SimCfgErr::BadDist), "SIM-003.14");
    let iso = rd_with(|s| s.caps_mut(P2).required_margin_mode = MarginMode::Isolated);
    assert_eq!(new(&setup_sim(), &iso), Err(SimCfgErr::MarginMode), "SIM-003.15");
    let s16 = with_a9(&|c| push_bal(c, bal(S4, USDT, usdt("1"))));
    assert_eq!(new(&s16, &rd), Err(SimCfgErr::SameExchangeMarkets), "SIM-003.16");
    let many = |n: u8| {
        let mut s = setup_sim();
        s.accounts = (1..=n).map(|a| (AccountId(a), cfg_a9())).collect();
        s
    };
    assert_eq!(new(&many(64), &rd), Ok(()), "SIM-003.17");
    assert_eq!(new(&many(65), &rd), Err(SimCfgErr::TooManyAccounts), "SIM-003.18");
}

/// `due_tsc` of every `A9` order event of one run.
fn run_004(with_a8: bool, a8_id: AccountId) -> Vec<u64> {
    let mut setup = setup_sim();
    setup.latency_models = vec![emp(&[(0, 1_000), (10_000, 5_000)])];
    if with_a8 {
        setup.accounts.push((a8_id, cfg_a8()));
    }
    let mut s = Sim::with(&setup, &rd_sim());
    s.topn(ms(100), BTC_S1, &[("60000", "10"), ("60010", "10")], &[]);
    for k in 0..20u64 {
        let t = ms(100 + 10 * k);
        if with_a8 {
            for j in 0..3u64 {
                if k * 3 + j < 50 {
                    s.place(a8_id, BTC_S1, Buy, "0.001", "60010", Tif::Ioc, t + j);
                }
            }
        }
        s.place(A9, BTC_S1, Buy, "0.01", "60010", Tif::Ioc, t);
        s.clock(t + ms(5));
    }
    s.run_to(ms(1_000));
    s.order_events(A9).into_iter().map(|(d, _)| d).collect()
}

#[test]
fn sim_004_substreams_independent() {
    let r1 = run_004(false, A8);
    let r2 = run_004(true, A8);
    assert!(!r1.is_empty());
    assert_eq!(r1, r2);
}
